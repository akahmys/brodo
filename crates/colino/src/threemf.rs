//! 3MF (ARCHITECTURE.md §5.1, ADR-0026), after the 3MF Core Specification 1.4.0. This
//! module parses untrusted bytes: it is listed in `UNTRUSTED_INPUT`, indexes nothing
//! (CODING.md Rule 3), and is fuzzed (TESTING.md T6).
//!
//! A 3MF document is a ZIP package. Its relationships part, `_rels/.rels`, names the 3D
//! model part by the StartPart relationship. Parts are read stored or deflated, each
//! expanded to no more than `Limits::bytes` and checked against its size and CRC.
//!
//! The model part is XML, read as a stream. Refused: a document type declaration, an
//! entity other than XML's predefined ones and character references, an undeclared prefix,
//! an element in no namespace, a second root, an element left open, and text outside the
//! root. Elements of the core namespace must stand where the format puts them; elements
//! of other namespaces, extensions not required, are skipped with all they hold, as are
//! attributes with a prefix.
//!
//! What is read: the unit, millimetres when none is given; each object's mesh or
//! components; the build's items. A model that requires an extension is refused. The mesh
//! returned holds every item of the build, each object placed by its item's transform and
//! its components' transforms, the vertex a row vector on the left of each 3 × 4 matrix,
//! components' first. Under a transform that turns space inside out, triangles are turned
//! over, so that their orientation keeps to the outside. Support objects are skipped;
//! a surface or an object of type `other` in the build is refused, as it bounds no solid.
//! Objects no item reaches are not returned. Overlapping items are kept as they are.

use std::collections::BTreeMap;

use quick_xml::XmlVersion;
use quick_xml::events::{BytesStart, Event};
use quick_xml::name::ResolveResult;
use quick_xml::reader::NsReader;
use rawzip::{CompressionMethod, ZipArchive, ZipArchiveEntryWayfinder, ZipSliceArchive};

use crate::error::ReadError;
use crate::mesh::{Limits, Mesh};

const CORE: &str = "http://schemas.microsoft.com/3dmanufacturing/core/2015/02";
const RELATIONSHIPS: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const START_PART: &str = "http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel";
const UNITS: [&str; 6] = ["micron", "millimeter", "centimeter", "inch", "foot", "meter"];

/// Reads a 3MF document's bytes.
pub fn read(bytes: &[u8], limits: Limits) -> Result<Mesh, ReadError> {
    let archive =
        ZipArchive::from_slice(bytes).map_err(|_| ReadError::Package("not a ZIP archive"))?;
    let parts = index(&archive)?;
    let rels = part(&archive, &parts, "_rels/.rels", limits)?;
    let target = start_part(&rels)?;
    let model = part(&archive, &parts, &target, limits)?;
    let text = std::str::from_utf8(&model).map_err(|_| ReadError::NotText)?;
    let mut reading = Reading::new(limits);
    walk(text, CORE, |node| reading.visit(node))?;
    reading.build()
}

/// The package's parts by name, without the leading `/` and compared without case, as the
/// Open Packaging Conventions name them, with how each is compressed.
fn index<T: AsRef<[u8]>>(
    archive: &ZipSliceArchive<T>,
) -> Result<BTreeMap<String, (ZipArchiveEntryWayfinder, CompressionMethod)>, ReadError> {
    let mut parts = BTreeMap::new();
    let mut entries = archive.entries();
    while let Some(entry) =
        entries.next_entry().map_err(|_| ReadError::Package("a damaged central directory"))?
    {
        if entry.is_dir() {
            continue;
        }
        let name = std::str::from_utf8(entry.file_path().as_bytes())
            .map_err(|_| ReadError::Package("a part name that is not UTF-8"))?
            .trim_start_matches('/')
            .to_ascii_lowercase();
        let place = (entry.wayfinder(), entry.compression_method());
        if parts.insert(name, place).is_some() {
            return Err(ReadError::Package("two parts of one name"));
        }
    }
    Ok(parts)
}

/// A part's bytes, expanded, of no more than `limits.bytes`, checked against its size and
/// CRC.
fn part<T: AsRef<[u8]>>(
    archive: &ZipSliceArchive<T>,
    parts: &BTreeMap<String, (ZipArchiveEntryWayfinder, CompressionMethod)>,
    name: &str,
    limits: Limits,
) -> Result<Vec<u8>, ReadError> {
    let (wayfinder, method) =
        parts.get(name).ok_or(ReadError::Package("a part the package names is missing"))?;
    let entry = archive.get_entry(*wayfinder).map_err(|_| ReadError::Package("a damaged part"))?;
    let claim = entry.claim_verifier();
    limits.check_bytes(claim.uncompressed_size)?;
    let size = usize::try_from(claim.uncompressed_size)
        .map_err(|_| ReadError::Package("a part too large to hold"))?;
    let data = entry.data();
    let out = if *method == CompressionMethod::STORE {
        data.get(..size).ok_or(ReadError::Package("a stored part shorter than its size"))?.to_vec()
    } else if *method == CompressionMethod::DEFLATE {
        miniz_oxide::inflate::decompress_to_vec_with_limit(data, size)
            .map_err(|_| ReadError::Package("deflated data that does not expand to its size"))?
    } else {
        return Err(ReadError::Compression(method.to_string()));
    };
    if out.len() != size {
        return Err(ReadError::Package("a part that does not expand to its size"));
    }
    if rawzip::crc32(&out) != claim.crc {
        return Err(ReadError::Package("a part whose CRC does not match"));
    }
    Ok(out)
}

/// The model part's name, from the StartPart relationship.
fn start_part(rels: &[u8]) -> Result<String, ReadError> {
    let text = std::str::from_utf8(rels).map_err(|_| ReadError::NotText)?;
    let mut target = None;
    let mut path: Vec<String> = Vec::new();
    walk(text, RELATIONSHIPS, |node| {
        match node {
            Node::Open { name, attrs, .. } => {
                if name == "Relationship"
                    && path.last().is_some_and(|p| p == "Relationships")
                    && attr(attrs, "Type") == Some(START_PART)
                {
                    if target.is_some() {
                        return Err(ReadError::Package("two start parts"));
                    }
                    target = attr(attrs, "Target").map(str::to_owned);
                }
                path.push(name.to_owned());
            }
            Node::Close { .. } => {
                path.pop();
            }
        }
        Ok(())
    })?;
    let target = target.ok_or(ReadError::Package("no start part"))?;
    let name = target.trim_start_matches('/');
    let unsafe_path =
        name.is_empty() || name.split('/').any(|s| s.is_empty() || s == "." || s == "..");
    if unsafe_path || name.contains('\\') {
        return Err(ReadError::Package("a start part's name that is not a plain path"));
    }
    Ok(name.to_ascii_lowercase())
}

/// An element of the namespace a walk reads, as it is met.
enum Node<'a> {
    Open { name: &'a str, attrs: &'a [(String, String)], at: u64 },
    Close { at: u64 },
}

fn attr<'a>(attrs: &'a [(String, String)], key: &str) -> Option<&'a str> {
    attrs.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
}

/// Whether an entity reference is one XML predefines, or a character reference.
fn predefined(name: &str) -> bool {
    matches!(name, "lt" | "gt" | "amp" | "apos" | "quot") || name.starts_with('#')
}

/// An element's attributes that have no prefix, values normalised, into `out`.
fn attributes(
    e: &BytesStart<'_>,
    at: u64,
    out: &mut Vec<(String, String)>,
) -> Result<(), ReadError> {
    out.clear();
    for a in e.attributes() {
        let a = a.map_err(|x| ReadError::Xml { at, reason: x.to_string() })?;
        if a.key.as_namespace_binding().is_some() || a.key.prefix().is_some() {
            continue;
        }
        let value = a
            .normalized_value(XmlVersion::Implicit1_0)
            .map_err(|x| ReadError::Xml { at, reason: x.to_string() })?;
        out.push((a.key.local_name().as_ref().to_owned(), value.into_owned()));
    }
    Ok(())
}

/// Where a walk stands: elements open, of them those inside a skipped one, and whether
/// the root was met.
#[derive(Default)]
struct Walk {
    depth: usize,
    foreign: usize,
    rooted: bool,
    attrs: Vec<(String, String)>,
}

impl Walk {
    /// A start or empty tag: `ours` is whether its namespace is the one read, none when it
    /// has no namespace.
    fn element(
        &mut self,
        e: &BytesStart<'_>,
        open: bool,
        ours: Option<bool>,
        at: u64,
        visit: &mut impl FnMut(Node<'_>) -> Result<(), ReadError>,
    ) -> Result<(), ReadError> {
        if self.depth == 0 {
            if self.rooted {
                return Err(ReadError::Model { at, reason: "a second root element" });
            }
            self.rooted = true;
        }
        self.depth += usize::from(open);
        if self.foreign > 0 {
            self.foreign += usize::from(open);
            return Ok(());
        }
        match ours {
            None => Err(ReadError::Model { at, reason: "an element in no namespace" }),
            Some(false) => {
                self.foreign = usize::from(open);
                Ok(())
            }
            Some(true) => {
                attributes(e, at, &mut self.attrs)?;
                let name = e.local_name();
                visit(Node::Open { name: name.as_ref(), attrs: &self.attrs, at })?;
                if open { Ok(()) } else { visit(Node::Close { at }) }
            }
        }
    }

    fn end(
        &mut self,
        at: u64,
        visit: &mut impl FnMut(Node<'_>) -> Result<(), ReadError>,
    ) -> Result<(), ReadError> {
        self.depth = self
            .depth
            .checked_sub(1)
            .ok_or(ReadError::Model { at, reason: "an end tag with no start" })?;
        if self.foreign > 0 {
            self.foreign -= 1;
            Ok(())
        } else {
            visit(Node::Close { at })
        }
    }
}

/// Walks a well-formed document, giving `visit` the elements of namespace `home` with
/// their attributes that have no prefix; elements of other namespaces are skipped with all
/// they hold.
fn walk(
    text: &str,
    home: &str,
    mut visit: impl FnMut(Node<'_>) -> Result<(), ReadError>,
) -> Result<(), ReadError> {
    let mut r = NsReader::from_str(text);
    let mut w = Walk::default();
    let model = |at, reason| ReadError::Model { at, reason };
    loop {
        let at = r.buffer_position();
        let (ns, event) =
            r.read_resolved_event().map_err(|e| ReadError::Xml { at, reason: e.to_string() })?;
        let ours = match ns {
            ResolveResult::Bound(n) => Some(n.as_ref() == home),
            ResolveResult::Unbound => None,
            ResolveResult::Unknown(_) => return Err(model(at, "an undeclared namespace prefix")),
        };
        let outside = w.depth == 0;
        match event {
            Event::Start(e) => w.element(&e, true, ours, at, &mut visit)?,
            Event::Empty(e) => w.element(&e, false, ours, at, &mut visit)?,
            Event::End(_) => w.end(at, &mut visit)?,
            Event::Text(t) if outside && !t.bytes().all(|b| b.is_ascii_whitespace()) => {
                return Err(model(at, "text outside the root element"));
            }
            Event::CData(_) if outside => return Err(model(at, "text outside the root element")),
            Event::GeneralRef(e) if outside || !predefined(&e) => {
                return Err(model(at, "an entity other than XML's predefined ones"));
            }
            Event::DocType(_) => return Err(model(at, "a document type declaration")),
            Event::Eof if !w.rooted => return Err(model(at, "no root element")),
            Event::Eof if w.depth > 0 => return Err(model(at, "an element left open")),
            Event::Eof => return Ok(()),
            Event::Text(_)
            | Event::CData(_)
            | Event::GeneralRef(_)
            | Event::Comment(_)
            | Event::Decl(_)
            | Event::PI(_) => {}
        }
    }
}

/// Where each element of the core namespace may stand: its parents, "" for the root.
const PLACES: [(&str, &[&str]); 16] = [
    ("model", &[""]),
    ("metadata", &["model", "metadatagroup"]),
    ("metadatagroup", &["object", "item"]),
    ("resources", &["model"]),
    ("basematerials", &["resources"]),
    ("base", &["basematerials"]),
    ("object", &["resources"]),
    ("mesh", &["object"]),
    ("vertices", &["mesh"]),
    ("vertex", &["vertices"]),
    ("triangles", &["mesh"]),
    ("triangle", &["triangles"]),
    ("components", &["object"]),
    ("component", &["components"]),
    ("build", &["model"]),
    ("item", &["build"]),
];

/// A 3 × 4 affine matrix, m00 m01 m02 m10 m11 m12 m20 m21 m22 m30 m31 m32, applied to a
/// row vector on its left.
type Transform = [f64; 12];

const IDENTITY: Transform = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0];

fn apply(m: &Transform, p: [f64; 3]) -> [f64; 3] {
    let [a, b, c, d, e, f, g, h, i, x, y, z] = *m;
    [
        p[0] * a + p[1] * d + p[2] * g + x,
        p[0] * b + p[1] * e + p[2] * h + y,
        p[0] * c + p[1] * f + p[2] * i + z,
    ]
}

/// `first` then `second`: a point moved by `first`, then by `second`.
fn then(first: &Transform, second: &Transform) -> Transform {
    let [a, b, c, d, e, f, g, h, i, x, y, z] = *first;
    let rows = [[a, b, c], [d, e, f], [g, h, i]].map(|r| apply(second, r));
    let shift = [second[9], second[10], second[11]];
    let moved = |r: [f64; 3]| [r[0] - shift[0], r[1] - shift[1], r[2] - shift[2]];
    let [r0, r1, r2] = rows.map(moved);
    let t = apply(second, [x, y, z]);
    [r0[0], r0[1], r0[2], r1[0], r1[1], r1[2], r2[0], r2[1], r2[2], t[0], t[1], t[2]]
}

fn turns_inside_out(m: &Transform) -> bool {
    let [a, b, c, d, e, f, g, h, i, ..] = *m;
    a * (e * i - f * h) - b * (d * i - f * g) + c * (d * h - e * g) < 0.0
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Model,
    Support,
    Surface,
    Other,
}

#[derive(Debug, Default)]
enum Body {
    #[default]
    None,
    Mesh(Vec<[f64; 3]>, Vec<[u32; 3]>),
    Components(Vec<(usize, Transform)>),
}

#[derive(Debug)]
struct Object {
    kind: Kind,
    body: Body,
}

/// Two counts of vertices, triangles and placements, added without overflow.
fn add(a: [u64; 3], b: [u64; 3]) -> [u64; 3] {
    let ([a0, a1, a2], [b0, b1, b2]) = (a, b);
    [a0.saturating_add(b0), a1.saturating_add(b1), a2.saturating_add(b2)]
}

/// The model part as it is read.
struct Reading {
    limits: Limits,
    path: Vec<&'static str>,
    objects: Vec<Object>,
    ids: BTreeMap<u64, usize>,
    items: Vec<(usize, Transform)>,
    unit: String,
    stored: [u64; 2],
}

fn number(attrs: &[(String, String)], key: &str, at: u64) -> Result<f64, ReadError> {
    attr(attrs, key)
        .and_then(|v| v.trim().parse::<f64>().ok())
        .filter(|x| x.is_finite())
        .ok_or(ReadError::Model { at, reason: "a coordinate missing or not a finite number" })
}

fn transform(attrs: &[(String, String)], at: u64) -> Result<Transform, ReadError> {
    let Some(text) = attr(attrs, "transform") else { return Ok(IDENTITY) };
    let bad = ReadError::Model { at, reason: "a transform that is not 12 finite numbers" };
    let mut m = IDENTITY;
    let mut words = text.split_ascii_whitespace();
    for x in &mut m {
        *x = words
            .next()
            .and_then(|w| w.parse::<f64>().ok())
            .filter(|x| x.is_finite())
            .ok_or(bad.clone())?;
    }
    if words.next().is_some() {
        return Err(bad);
    }
    Ok(m)
}

impl Reading {
    fn new(limits: Limits) -> Self {
        Reading {
            limits,
            path: Vec::new(),
            objects: Vec::new(),
            ids: BTreeMap::new(),
            items: Vec::new(),
            unit: String::from("millimeter"),
            stored: [0, 0],
        }
    }

    fn visit(&mut self, node: Node<'_>) -> Result<(), ReadError> {
        match node {
            Node::Open { name, attrs, at } => self.open(name, attrs, at),
            Node::Close { at } => self.close(at),
        }
    }

    /// The object being read, whose body is now `body` if it has none yet.
    fn body(&mut self, body: Body, at: u64) -> Result<(), ReadError> {
        let object = self.objects.last_mut().ok_or(ReadError::Model { at, reason: "no object" })?;
        if !matches!(object.body, Body::None) {
            return Err(ReadError::Model { at, reason: "an object with a mesh and components" });
        }
        object.body = body;
        Ok(())
    }

    /// The object named by an `objectid`, which must be defined before.
    fn named(&self, attrs: &[(String, String)], at: u64) -> Result<usize, ReadError> {
        let id: u64 = attr(attrs, "objectid")
            .and_then(|v| v.trim().parse().ok())
            .ok_or(ReadError::Model { at, reason: "an object id missing or not a number" })?;
        self.ids.get(&id).copied().ok_or(ReadError::NoObject { at, id })
    }

    fn open(&mut self, name: &str, attrs: &[(String, String)], at: u64) -> Result<(), ReadError> {
        let parent = self.path.last().copied().unwrap_or("");
        let (place, parents) = PLACES
            .iter()
            .find(|(n, _)| *n == name)
            .ok_or_else(|| ReadError::UnknownElement { at, name: name.to_owned() })?;
        if !parents.contains(&parent) {
            return Err(ReadError::Model { at, reason: "an element where the format puts none" });
        }
        self.path.push(place);
        match *place {
            "model" => self.model(attrs, at),
            "object" => self.object(attrs, at),
            "mesh" => self.body(Body::Mesh(Vec::new(), Vec::new()), at),
            "components" => self.body(Body::Components(Vec::new()), at),
            "vertex" => self.vertex(attrs, at),
            "triangle" => self.triangle(attrs, at),
            "component" => {
                let child = (self.named(attrs, at)?, transform(attrs, at)?);
                if let Some(Object { body: Body::Components(c), .. }) = self.objects.last_mut() {
                    c.push(child);
                }
                Ok(())
            }
            "item" => {
                let item = (self.named(attrs, at)?, transform(attrs, at)?);
                self.items.push(item);
                Ok(())
            }
            _ => Ok(()),
        }
    }

    fn close(&mut self, at: u64) -> Result<(), ReadError> {
        let closed = self.path.pop();
        if closed == Some("object")
            && self.objects.last().is_some_and(|o| matches!(o.body, Body::None))
        {
            return Err(ReadError::Model {
                at,
                reason: "an object with neither mesh nor components",
            });
        }
        Ok(())
    }

    fn model(&mut self, attrs: &[(String, String)], at: u64) -> Result<(), ReadError> {
        if let Some(required) = attr(attrs, "requiredextensions").filter(|r| !r.trim().is_empty()) {
            return Err(ReadError::RequiredExtension(required.trim().to_owned()));
        }
        if let Some(unit) = attr(attrs, "unit") {
            if !UNITS.contains(&unit) {
                return Err(ReadError::Model { at, reason: "a unit the format does not name" });
            }
            unit.clone_into(&mut self.unit);
        }
        Ok(())
    }

    fn object(&mut self, attrs: &[(String, String)], at: u64) -> Result<(), ReadError> {
        let id: u64 = attr(attrs, "id")
            .and_then(|v| v.trim().parse().ok())
            .ok_or(ReadError::Model { at, reason: "an object id missing or not a number" })?;
        let kind = match attr(attrs, "type").unwrap_or("model") {
            "model" => Kind::Model,
            "support" | "solidsupport" => Kind::Support,
            "surface" => Kind::Surface,
            "other" => Kind::Other,
            _ => {
                return Err(ReadError::Model {
                    at,
                    reason: "an object type the format does not name",
                });
            }
        };
        if self.ids.insert(id, self.objects.len()).is_some() {
            return Err(ReadError::Model { at, reason: "two objects of one id" });
        }
        self.objects.push(Object { kind, body: Body::None });
        Ok(())
    }

    fn vertex(&mut self, attrs: &[(String, String)], at: u64) -> Result<(), ReadError> {
        let p = [number(attrs, "x", at)?, number(attrs, "y", at)?, number(attrs, "z", at)?];
        self.stored[0] += 1;
        if self.stored[0] > self.limits.vertices {
            return Err(ReadError::TooLarge { what: "vertices", limit: self.limits.vertices });
        }
        if let Some(Object { body: Body::Mesh(v, _), .. }) = self.objects.last_mut() {
            v.push(p);
        }
        Ok(())
    }

    fn triangle(&mut self, attrs: &[(String, String)], at: u64) -> Result<(), ReadError> {
        let index = |key| {
            attr(attrs, key)
                .and_then(|v| v.trim().parse::<u32>().ok())
                .ok_or(ReadError::Model { at, reason: "a vertex index missing or not a number" })
        };
        let t = [index("v1")?, index("v2")?, index("v3")?];
        self.stored[1] += 1;
        if self.stored[1] > self.limits.triangles {
            return Err(ReadError::TooLarge { what: "triangles", limit: self.limits.triangles });
        }
        if let Some(Object { body: Body::Mesh(v, triangles), .. }) = self.objects.last_mut() {
            let n = u32::try_from(v.len()).unwrap_or(u32::MAX);
            if t.iter().any(|i| *i >= n) {
                return Err(ReadError::Model {
                    at,
                    reason: "a triangle naming a vertex the mesh lacks",
                });
            }
            triangles.push(t);
        }
        Ok(())
    }

    /// What each object adds once placed, in vertices, triangles and placements, counted
    /// in order, since a component names an object defined before it.
    fn sizes(&self) -> Vec<[u64; 3]> {
        let mut sizes: Vec<[u64; 3]> = Vec::with_capacity(self.objects.len());
        for o in &self.objects {
            let size = match (&o.kind, &o.body) {
                (Kind::Support, _) | (_, Body::None) => [0, 0, 1],
                (_, Body::Mesh(v, t)) => {
                    [v.len(), t.len(), 1].map(|n| u64::try_from(n).unwrap_or(u64::MAX))
                }
                (_, Body::Components(c)) => c.iter().fold([0_u64, 0, 1], |s, (i, _)| {
                    add(s, sizes.get(*i).copied().unwrap_or([0; 3]))
                }),
            };
            sizes.push(size);
        }
        sizes
    }

    /// The mesh of every item of the build, placed.
    fn build(self) -> Result<Mesh, ReadError> {
        let sizes = self.sizes();
        let total = self
            .items
            .iter()
            .fold([0_u64; 3], |s, (i, _)| add(s, sizes.get(*i).copied().unwrap_or([0; 3])));
        let limits = self.limits;
        if total[0] > limits.vertices {
            return Err(ReadError::TooLarge { what: "vertices", limit: limits.vertices });
        }
        // A placement counts against the triangles too, so that components that add no
        // triangle cannot make the expansion endless in all but name.
        if total[1] > limits.triangles || total[2] > limits.triangles {
            return Err(ReadError::TooLarge { what: "triangles", limit: limits.triangles });
        }
        let mut mesh = Mesh { unit: Some(self.unit.clone()), ..Mesh::default() };
        let mut stack: Vec<(usize, Transform)> = self.items.iter().rev().copied().collect();
        while let Some((i, m)) = stack.pop() {
            let Some(object) = self.objects.get(i) else { continue };
            match (&object.kind, &object.body) {
                (Kind::Support, _) | (_, Body::None) => {}
                (Kind::Surface | Kind::Other, _) => {
                    return Err(ReadError::Model {
                        at: 0,
                        reason: "a surface or other object in the build",
                    });
                }
                (Kind::Model, Body::Mesh(v, t)) => place(&mut mesh, v, t, &m)?,
                (Kind::Model, Body::Components(c)) => {
                    stack.extend(c.iter().rev().map(|(child, cm)| (*child, then(cm, &m))));
                }
            }
        }
        Ok(mesh)
    }
}

/// An object's mesh, moved by `m`, added to `mesh`; turned over where `m` turns space
/// inside out.
fn place(mesh: &mut Mesh, v: &[[f64; 3]], t: &[[u32; 3]], m: &Transform) -> Result<(), ReadError> {
    let base = u32::try_from(mesh.vertices.len())
        .map_err(|_| ReadError::TooLarge { what: "vertices", limit: u64::from(u32::MAX) })?;
    for p in v {
        let q = apply(m, *p);
        if !q.iter().all(|c| c.is_finite()) {
            return Err(ReadError::Model {
                at: 0,
                reason: "a vertex a transform moves beyond f64",
            });
        }
        mesh.vertices.push(q);
    }
    let flip = turns_inside_out(m);
    for [a, b, c] in t {
        let [a, b, c] = [a, b, c].map(|i| base.saturating_add(*i));
        mesh.triangles.push(if flip { [a, c, b] } else { [a, b, c] });
    }
    Ok(())
}
