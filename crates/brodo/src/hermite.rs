//! Hermite data on a grid's voxel corners, a format any producer writes (ARCHITECTURE.md
//! §4.9, ADR-0032): a sign at each lattice point and, on each edge whose ends differ, where
//! the surface crosses it, its normal there and its group (Ju et al. 2002).
//!
//! Point v lies at the corner of voxel v, the image of v under the grid's transform. It owns
//! its +x, +y and +z edges. A set is ten fields on one topology: the signs, 1 inside and 0
//! outside, and per axis a fraction, a normal and a group for the edges points own. An edge
//! has a crossing exactly when the stored signs of its ends differ, those of tiles and the
//! background included; its owner is then a leaf point. Its fraction is the crossing's place
//! from the edge's lower end, in [0, 1]; its normal a unit vector whose component along the
//! edge, where not zero, points from the inside end to the outside end; its group an id the
//! set's table names. An edge with no crossing, every tile and the background hold
//! `NO_CROSSING`, compared by bits; the signs' background is 0.
//!
//! In a cache file, `GridMeta::extra` tells a set's grids apart and holds the group table,
//! as `docs/formats.md` §4 states, which this code follows.

use std::collections::{BTreeMap, BTreeSet};

use crate::cache::{CacheEntry, CacheFile};
use crate::error::{FormatError, HermiteError, Rule};
use crate::field::{Field, LEAF_VOXELS, Value};
use crate::meta::GridMeta;
use crate::topology::{Location, Topology};
use crate::tree::{Coord, TileLevel};
use crate::view::{GridView, ViewLocation};

/// The format's version, in each grid's `hermite.version`.
pub const VERSION: &str = "1";
/// The ten fields of a set, in the order `Hermite::entries` writes them.
pub const FIELDS: [&str; 10] = [
    "sign",
    "x.fraction",
    "x.normal",
    "x.group",
    "y.fraction",
    "y.normal",
    "y.group",
    "z.fraction",
    "z.normal",
    "z.group",
];
/// How far a normal's length may be from 1 (ADR-0032): a unit `f64` vector rounded to
/// `f32` errs by about 1e-7.
pub const NORMAL_TOLERANCE: f64 = 1e-6;
/// The fraction, normal and group of an edge with no crossing, of a tile and of the
/// background.
pub const NO_CROSSING: (f32, [f32; 3], u32) = (-1.0, [0.0; 3], 0);

const KEY_VERSION: &str = "hermite.version";
const KEY_SET: &str = "hermite.set";
const KEY_FIELD: &str = "hermite.field";
const KEY_GROUP: &str = "hermite.group.";
const PREFIX: &str = "hermite.";

/// The crossings on one axis's edges.
#[derive(Debug, Clone)]
pub struct Crossings {
    /// Each crossing's place from the edge's lower end, in [0, 1].
    pub fraction: Field<f32>,
    /// The surface's unit normal at each crossing.
    pub normal: Field<[f32; 3]>,
    /// Each crossing's group.
    pub group: Field<u32>,
}

/// A set of Hermite data on one topology, checked whole when made.
#[derive(Debug, Clone)]
pub struct Hermite {
    topology: Topology,
    signs: Field<u8>,
    axes: [Crossings; 3],
    group_names: Vec<String>,
}

/// One axis's crossings, read in place from a cache file.
#[derive(Debug, Clone, Copy)]
pub struct CrossingsView<'a> {
    /// Each crossing's place from the edge's lower end.
    pub fraction: GridView<'a, f32>,
    /// The surface's unit normal at each crossing.
    pub normal: GridView<'a, [f32; 3]>,
    /// Each crossing's group.
    pub group: GridView<'a, u32>,
}

/// A set read in place from a cache file, checked whole when found.
#[derive(Debug, Clone)]
pub struct HermiteView<'a> {
    signs: GridView<'a, u8>,
    axes: [CrossingsView<'a>; 3],
    group_names: Vec<String>,
}

/// The unit step along an axis.
fn step(axis: usize) -> Coord {
    let mut e = [0; 3];
    if let Some(c) = e.get_mut(axis) {
        *c = 1;
    }
    e
}

fn plus(v: Coord, d: Coord) -> Coord {
    let ([a, b, c], [x, y, z]) = (v, d);
    [a.wrapping_add(x), b.wrapping_add(y), c.wrapping_add(z)]
}

fn minus(v: Coord, d: Coord) -> Coord {
    let ([a, b, c], [x, y, z]) = (v, d);
    [a.wrapping_sub(x), b.wrapping_sub(y), c.wrapping_sub(z)]
}

/// The point a step `d` from `v`, forward or back, or nothing beyond the `i32` range, which
/// is the background: sign 0, in no leaf (docs/formats.md §4.2).
fn neighbour(v: Coord, d: Coord, forward: bool) -> Option<Coord> {
    let step = |c: i32, k: i32| if forward { c.checked_add(k) } else { c.checked_sub(k) };
    Some([step(v[0], d[0])?, step(v[1], d[1])?, step(v[2], d[2])?])
}

fn violation(at: Option<Coord>, axis: Option<usize>, rule: Rule) -> HermiteError {
    HermiteError::Violation { at, axis, rule }
}

/// Refuses a group table whose first name is not `""`, or whose other names are empty or
/// repeated; empty, it stands for `[""]`.
fn check_names(names: &[String]) -> Result<(), HermiteError> {
    let mut seen = BTreeSet::new();
    for (id, name) in names.iter().enumerate() {
        let bad = if id == 0 { !name.is_empty() } else { name.is_empty() || !seen.insert(name) };
        if bad {
            return Err(HermiteError::BadGroupName(u32::try_from(id).unwrap_or(u32::MAX)));
        }
    }
    Ok(())
}

/// One axis's fractions, normals and groups over a leaf.
type LeafCrossings<'a> =
    (&'a [f32; LEAF_VOXELS], &'a [[f32; 3]; LEAF_VOXELS], &'a [u32; LEAF_VOXELS]);

/// What the check reads, from a set made in memory or read from a file.
trait Source {
    fn leaves(&self) -> Result<Vec<Coord>, HermiteError>;
    /// Each tile's level, origin and the number its values are read by.
    fn tiles(&self) -> Result<Vec<(TileLevel, Coord, u32)>, HermiteError>;
    fn is_leaf(&self, v: Coord) -> Result<bool, HermiteError>;
    /// Whether a node holds the voxel's region of `level`.
    fn holds_node(&self, level: TileLevel, v: Coord) -> Result<bool, HermiteError>;
    fn sign_at(&self, v: Coord) -> Result<u8, HermiteError>;
    fn sign_leaf(&self, n: u32) -> Result<&[u8; LEAF_VOXELS], HermiteError>;
    fn crossing_leaf(&self, axis: usize, n: u32) -> Result<LeafCrossings<'_>, HermiteError>;
    fn sign_tile(&self, t: u32) -> Result<u8, HermiteError>;
    fn crossing_tile(&self, axis: usize, t: u32) -> Result<(f32, [f32; 3], u32), HermiteError>;
    fn backgrounds(&self) -> (u8, [(f32, [f32; 3], u32); 3]);
}

/// Whether two crossings' values are the same, by bits for the floats.
fn same(a: (f32, [f32; 3], u32), b: (f32, [f32; 3], u32)) -> bool {
    a.0.same_bits(&b.0) && a.1.same_bits(&b.1) && a.2 == b.2
}

/// Checks one crossed edge's values.
fn check_crossing(
    c: (f32, [f32; 3], u32),
    axis: usize,
    lower_inside: bool,
    groups: usize,
) -> Result<(), Rule> {
    let (fraction, normal, group) = c;
    if !(0.0..=1.0).contains(&fraction) {
        return Err(Rule::Fraction);
    }
    let length = normal.iter().map(|x| f64::from(*x) * f64::from(*x)).sum::<f64>().sqrt();
    let near = (length - 1.0).abs().partial_cmp(&NORMAL_TOLERANCE);
    if !matches!(near, Some(std::cmp::Ordering::Less | std::cmp::Ordering::Equal)) {
        return Err(Rule::NormalLength);
    }
    let along = normal.get(axis).copied().unwrap_or(0.0);
    if (lower_inside && along < 0.0) || (!lower_inside && along > 0.0) {
        return Err(Rule::NormalInward);
    }
    if usize::try_from(group).map_or(true, |g| g >= groups) {
        return Err(Rule::UnnamedGroup);
    }
    Ok(())
}

/// The voxel at index `i` of a leaf at `origin`, x fastest.
fn voxel(origin: Coord, i: usize) -> Coord {
    let i = i32::try_from(i).unwrap_or(0);
    plus(origin, [i & 7, i >> 3 & 7, i >> 6])
}

/// Checks a set against the format (ADR-0032).
fn check(s: &impl Source, names: &[String]) -> Result<(), HermiteError> {
    check_names(names)?;
    let groups = names.len().max(1);
    let (sign_bg, crossing_bg) = s.backgrounds();
    if sign_bg != 0 {
        return Err(violation(None, None, Rule::Sign));
    }
    for (axis, c) in crossing_bg.iter().enumerate() {
        if !same(*c, NO_CROSSING) {
            return Err(violation(None, Some(axis), Rule::NoCrossing));
        }
    }
    let tiles = s.tiles()?;
    for (_, origin, t) in &tiles {
        if s.sign_tile(*t)? > 1 {
            return Err(violation(Some(*origin), None, Rule::Sign));
        }
        for axis in 0..3 {
            if !same(s.crossing_tile(axis, *t)?, NO_CROSSING) {
                return Err(violation(Some(*origin), Some(axis), Rule::NoCrossing));
            }
        }
    }
    let leaves = s.leaves()?;
    for (n, origin) in (0_u32..).zip(&leaves) {
        let bad = s.sign_leaf(n)?.iter().position(|sign| *sign > 1);
        if let Some(i) = bad {
            return Err(violation(Some(voxel(*origin, i)), None, Rule::Sign));
        }
    }
    // Tiles' faces before leaves, so that a change only a tile's face meets is the one
    // reported.
    for (level, origin, t) in &tiles {
        check_tile_faces(s, *level, *origin, s.sign_tile(*t)?)?;
    }
    for (n, origin) in (0_u32..).zip(leaves) {
        check_leaf(s, n, origin, groups)?;
    }
    Ok(())
}

/// Checks a leaf's points: signs, the crossings they own, and that an edge into the leaf
/// from a point outside every leaf crosses nothing.
fn check_leaf(s: &impl Source, n: u32, origin: Coord, groups: usize) -> Result<(), HermiteError> {
    let signs = s.sign_leaf(n)?;
    let sign_of = |v: Coord| -> Result<u8, HermiteError> {
        let local = minus(v, origin);
        if local.iter().all(|c| (0..8).contains(c)) {
            let [x, y, z] = local.map(|c| usize::try_from(c).unwrap_or(0));
            return signs.get(x + 8 * y + 64 * z).copied().ok_or(HermiteError::Violation {
                at: Some(v),
                axis: None,
                rule: Rule::Sign,
            });
        }
        s.sign_at(v)
    };
    for axis in 0..3 {
        let (fractions, normals, groups_of) = s.crossing_leaf(axis, n)?;
        let e = step(axis);
        for (i, ((sign, fraction), (normal, group))) in
            signs.iter().zip(fractions).zip(normals.iter().zip(groups_of)).enumerate()
        {
            let v = voxel(origin, i);
            let crossing = (*fraction, *normal, *group);
            let beyond = match neighbour(v, e, true) {
                Some(w) => sign_of(w)?,
                None => 0,
            };
            if beyond == *sign {
                if !same(crossing, NO_CROSSING) {
                    return Err(violation(Some(v), Some(axis), Rule::NoCrossing));
                }
            } else {
                check_crossing(crossing, axis, *sign == 1, groups)
                    .map_err(|rule| violation(Some(v), Some(axis), rule))?;
            }
            // The point before v owns the edge into it; beyond the range it is the
            // background, which owns nothing, and the error names v.
            match neighbour(v, e, false) {
                Some(u) => {
                    let in_leaf = minus(u, origin).iter().all(|c| (0..8).contains(c));
                    if !in_leaf && !s.is_leaf(u)? && sign_of(u)? != *sign {
                        return Err(violation(Some(u), Some(axis), Rule::Owner));
                    }
                }
                None if *sign != 0 => return Err(violation(Some(v), Some(axis), Rule::Owner)),
                None => {}
            }
        }
    }
    Ok(())
}

/// Checks that no edge a tile's point owns, or that reaches the tile from outside every
/// leaf, crosses anything. Edges between a tile and a leaf are the leaf's to check. Across
/// each face, the region of the tile's level beyond it is one value unless a node holds it,
/// and then each of the node's regions along the face is checked in turn, so the work
/// follows the nodes the face touches, not its area.
fn check_tile_faces(
    s: &impl Source,
    level: TileLevel,
    origin: Coord,
    sign: u8,
) -> Result<(), HermiteError> {
    let far = level.edge() - 1;
    for axis in 0..3 {
        let e = step(axis);
        // The tile's upper face owns the edges out of it; the point below its lower face
        // owns the edges into it. Beyond the range lies the background, sign 0, in no leaf;
        // the error then names the tile's point.
        let upper = plus(origin, e.map(|c| c * far));
        let face = Face { axis, sign, forward: true };
        match neighbour(upper, e, true) {
            Some(o) => face.check_region(s, level, o)?,
            None if sign != 0 => return Err(violation(Some(upper), Some(axis), Rule::Owner)),
            None => {}
        }
        let face = Face { forward: false, ..face };
        match neighbour(origin, e, false) {
            Some(o) => face.check_region(s, level, o)?,
            None if sign != 0 => return Err(violation(Some(origin), Some(axis), Rule::Owner)),
            None => {}
        }
    }
    Ok(())
}

/// The outside of one face of a tile: the voxels beyond it along `axis`, after it if
/// `forward`, which must have the tile's `sign` wherever no leaf holds them.
#[derive(Clone, Copy)]
struct Face {
    axis: usize,
    sign: u8,
    forward: bool,
}

impl Face {
    /// Checks the voxels beyond the face in the region of `level` whose corner is `at`.
    // Bounded recursion: each call goes one tile level finer, root to upper to lower, so
    // three deep at most.
    fn check_region(
        self,
        s: &impl Source,
        level: TileLevel,
        at: Coord,
    ) -> Result<(), HermiteError> {
        if !s.holds_node(level, at)? {
            if s.sign_at(at)? == self.sign {
                return Ok(());
            }
            let e = step(self.axis);
            let owner = if self.forward { minus(at, e) } else { at };
            return Err(violation(Some(owner), Some(self.axis), Rule::Owner));
        }
        let finer = match level {
            TileLevel::Root => TileLevel::Upper,
            TileLevel::Upper => TileLevel::Lower,
            // A leaf's edges to a tile are the leaf's to check.
            TileLevel::Lower => return Ok(()),
        };
        let [a, b] = [step((self.axis + 1) % 3), step((self.axis + 2) % 3)];
        let edge = usize::try_from(finer.edge()).unwrap_or(usize::MAX);
        for i in (0..level.edge()).step_by(edge) {
            for j in (0..level.edge()).step_by(edge) {
                self.check_region(s, finer, plus(at, plus(a.map(|c| c * i), b.map(|c| c * j))))?;
            }
        }
        Ok(())
    }
}

/// A set in memory.
struct Owned<'a>(&'a Hermite);

impl Source for Owned<'_> {
    fn leaves(&self) -> Result<Vec<Coord>, HermiteError> {
        let t = &self.0.topology;
        Ok(t.leaf_numbers().map(|n| t.leaf_origin(n)).collect::<Result<_, _>>()?)
    }

    fn tiles(&self) -> Result<Vec<(TileLevel, Coord, u32)>, HermiteError> {
        let t = &self.0.topology;
        let mut out = Vec::with_capacity(t.tile_count());
        for n in t.tile_numbers() {
            let (level, origin) = t.tile(n)?;
            out.push((level, origin, n.get()));
        }
        Ok(out)
    }

    fn is_leaf(&self, v: Coord) -> Result<bool, HermiteError> {
        Ok(matches!(self.0.topology.locate(v), Location::Leaf(_)))
    }

    fn holds_node(&self, level: TileLevel, v: Coord) -> Result<bool, HermiteError> {
        let t = &self.0.topology;
        Ok(match level {
            TileLevel::Root => t.has_upper_node(v),
            TileLevel::Upper => t.has_lower_node(v),
            TileLevel::Lower => matches!(t.locate(v), Location::Leaf(_)),
        })
    }

    fn sign_at(&self, v: Coord) -> Result<u8, HermiteError> {
        Ok(self.0.signs.value_at(&self.0.topology, v)?)
    }

    fn sign_leaf(&self, n: u32) -> Result<&[u8; LEAF_VOXELS], HermiteError> {
        Ok(self.0.signs.leaf(&self.0.topology, crate::topology::LeafNumber(n))?)
    }

    fn crossing_leaf(&self, axis: usize, n: u32) -> Result<LeafCrossings<'_>, HermiteError> {
        let (t, n) = (&self.0.topology, crate::topology::LeafNumber(n));
        let c = self.0.axes.get(axis).ok_or(HermiteError::Missing("an axis"))?;
        Ok((c.fraction.leaf(t, n)?, c.normal.leaf(t, n)?, c.group.leaf(t, n)?))
    }

    fn sign_tile(&self, t: u32) -> Result<u8, HermiteError> {
        Ok(self.0.signs.tile_value_at(t)?)
    }

    fn crossing_tile(&self, axis: usize, t: u32) -> Result<(f32, [f32; 3], u32), HermiteError> {
        let c = self.0.axes.get(axis).ok_or(HermiteError::Missing("an axis"))?;
        Ok((c.fraction.tile_value_at(t)?, c.normal.tile_value_at(t)?, c.group.tile_value_at(t)?))
    }

    fn backgrounds(&self) -> (u8, [(f32, [f32; 3], u32); 3]) {
        let bg =
            |c: &Crossings| (c.fraction.background(), c.normal.background(), c.group.background());
        (self.0.signs.background(), self.0.axes.each_ref().map(bg))
    }
}

impl Hermite {
    /// A set of `signs` and the crossings on each axis, x, y and z, all on `topology`, with
    /// the group table `group_names`, indexed by id; refused unless it keeps to the format.
    pub fn new(
        topology: Topology,
        signs: Field<u8>,
        axes: [Crossings; 3],
        group_names: Vec<String>,
    ) -> Result<Hermite, HermiteError> {
        let mut belongs = vec![signs.belongs_to(&topology)];
        for c in &axes {
            belongs.push(c.fraction.belongs_to(&topology));
            belongs.push(c.normal.belongs_to(&topology));
            belongs.push(c.group.belongs_to(&topology));
        }
        if let Some((name, _)) = FIELDS.iter().zip(&belongs).find(|(_, r)| r.is_err()) {
            return Err(HermiteError::OtherTopology(name));
        }
        // `[""]` names group 0 alone, as an empty table does; it is kept empty, as a file
        // reads it back.
        let group_names = if group_names == [""] { Vec::new() } else { group_names };
        let set = Hermite { topology, signs, axes, group_names };
        check(&Owned(&set), &set.group_names)?;
        Ok(set)
    }

    /// The topology every field is on.
    pub fn topology(&self) -> &Topology {
        &self.topology
    }

    /// The signs, 1 inside, 0 outside.
    pub fn signs(&self) -> &Field<u8> {
        &self.signs
    }

    /// The crossings on the x, y and z edges.
    pub fn axes(&self) -> &[Crossings; 3] {
        &self.axes
    }

    /// Each group's name, by id; empty, every group is 0.
    pub fn group_names(&self) -> &[String] {
        &self.group_names
    }

    /// The ten cache entries of the set named `set`, in the order of `FIELDS`, each with
    /// `meta` and the keys that tell it apart; `meta` may hold no key beginning `hermite.`.
    pub fn entries(&self, set: &str, meta: &GridMeta) -> Result<Vec<CacheEntry>, HermiteError> {
        if let Some(key) = meta.extra.keys().find(|k| k.starts_with(PREFIX)) {
            return Err(HermiteError::ReservedKey(key.clone()));
        }
        let keyed = |field: &str| {
            let mut m = meta.clone();
            m.extra.insert(KEY_VERSION.to_owned(), VERSION.to_owned());
            m.extra.insert(KEY_SET.to_owned(), set.to_owned());
            m.extra.insert(KEY_FIELD.to_owned(), field.to_owned());
            m
        };
        let mut sign_meta = keyed("sign");
        for (id, name) in self.group_names.iter().enumerate().skip(1) {
            sign_meta.extra.insert(format!("{KEY_GROUP}{id}"), name.clone());
        }
        let t = &self.topology;
        let mut out = vec![CacheEntry::new(&sign_meta, t, &self.signs)?];
        for (axis, c) in ["x", "y", "z"].iter().zip(&self.axes) {
            out.push(CacheEntry::new(&keyed(&format!("{axis}.fraction")), t, &c.fraction)?);
            out.push(CacheEntry::new(&keyed(&format!("{axis}.normal")), t, &c.normal)?);
            out.push(CacheEntry::new(&keyed(&format!("{axis}.group")), t, &c.group)?);
        }
        Ok(out)
    }
}

/// A set read from a file.
struct Viewed<'a, 'b> {
    set: &'b HermiteView<'a>,
    regions: crate::view::Regions,
}

impl Source for Viewed<'_, '_> {
    fn leaves(&self) -> Result<Vec<Coord>, HermiteError> {
        Ok(self.regions.leaves.clone())
    }

    fn tiles(&self) -> Result<Vec<(TileLevel, Coord, u32)>, HermiteError> {
        Ok(self.regions.tiles.clone())
    }

    fn is_leaf(&self, v: Coord) -> Result<bool, HermiteError> {
        Ok(matches!(self.set.signs.locate(v)?, ViewLocation::Leaf(_)))
    }

    fn holds_node(&self, level: TileLevel, v: Coord) -> Result<bool, HermiteError> {
        Ok(self.set.signs.holds_node(level, v)?)
    }

    fn sign_at(&self, v: Coord) -> Result<u8, HermiteError> {
        Ok(self.set.signs.get(v)?)
    }

    fn sign_leaf(&self, n: u32) -> Result<&[u8; LEAF_VOXELS], HermiteError> {
        Ok(self.set.signs.leaf(n)?)
    }

    fn crossing_leaf(&self, axis: usize, n: u32) -> Result<LeafCrossings<'_>, HermiteError> {
        let c = self.set.axes.get(axis).ok_or(HermiteError::Missing("an axis"))?;
        Ok((c.fraction.leaf(n)?, c.normal.leaf(n)?, c.group.leaf(n)?))
    }

    fn sign_tile(&self, t: u32) -> Result<u8, HermiteError> {
        Ok(self.set.signs.tile_value(t)?)
    }

    fn crossing_tile(&self, axis: usize, t: u32) -> Result<(f32, [f32; 3], u32), HermiteError> {
        let c = self.set.axes.get(axis).ok_or(HermiteError::Missing("an axis"))?;
        Ok((c.fraction.tile_value(t)?, c.normal.tile_value(t)?, c.group.tile_value(t)?))
    }

    fn backgrounds(&self) -> (u8, [(f32, [f32; 3], u32); 3]) {
        let bg = |c: &CrossingsView<'_>| {
            (c.fraction.background(), c.normal.background(), c.group.background())
        };
        (self.set.signs.background(), self.set.axes.each_ref().map(bg))
    }
}

/// The group table in a sign grid's `extra`: ids from 1 with no gap.
fn group_table(meta: &GridMeta) -> Result<Vec<String>, HermiteError> {
    let mut by_id = BTreeMap::new();
    for (key, name) in &meta.extra {
        let Some(id) = key.strip_prefix(KEY_GROUP) else { continue };
        let n: u32 = id.parse().map_err(|_| HermiteError::BadGroupKey(key.clone()))?;
        if n == 0 || n.to_string() != id {
            return Err(HermiteError::BadGroupKey(key.clone()));
        }
        by_id.insert(n, name.clone());
    }
    let mut names = vec![String::new()];
    for (expected, (id, name)) in (1_u32..).zip(by_id) {
        if id != expected {
            return Err(HermiteError::BadGroupName(expected));
        }
        names.push(name);
    }
    if names.len() == 1 {
        names.clear();
    }
    Ok(names)
}

/// The places in `file` of the set's ten grids, in the order of `FIELDS`.
fn grids_of(file: &CacheFile, set: &str) -> Result<Vec<usize>, HermiteError> {
    let mut found: [Option<usize>; 10] = [None; 10];
    for i in 0..file.grid_count() {
        let meta = file.meta(i)?;
        if meta.extra.get(KEY_SET).map(String::as_str) != Some(set) {
            continue;
        }
        let version = meta.extra.get(KEY_VERSION).cloned().unwrap_or_default();
        if version != VERSION {
            return Err(HermiteError::Version(version));
        }
        let field = meta.extra.get(KEY_FIELD).cloned().unwrap_or_default();
        let place = FIELDS.iter().zip(found.iter_mut()).find(|(f, _)| **f == field);
        let (name, slot) = place.ok_or(HermiteError::UnknownField(field))?;
        if slot.replace(i).is_some() {
            return Err(HermiteError::Twice(name));
        }
    }
    let mut at = Vec::with_capacity(FIELDS.len());
    for (f, name) in found.iter().zip(FIELDS) {
        at.push(f.ok_or(HermiteError::Missing(name))?);
    }
    Ok(at)
}

impl<'a> HermiteView<'a> {
    /// The set named `set` in `file`, its ten grids found by their keys and checked whole:
    /// the format's version, metadata that agree, one topology, and the values.
    pub fn find(file: &'a CacheFile, set: &str) -> Result<HermiteView<'a>, HermiteError> {
        let at = grids_of(file, set)?;
        let [sign_at, x @ ..] = at.as_slice() else { return Err(HermiteError::Missing("sign")) };
        let sign_meta = file.meta(*sign_at)?;
        for (i, name) in x.iter().zip(FIELDS.iter().skip(1)) {
            let m = file.meta(*i)?;
            let bits =
                |a: &[f64], b: &[f64]| a.iter().zip(b).all(|(p, q)| p.to_bits() == q.to_bits());
            let agree = bits(&m.voxel_size, &sign_meta.voxel_size)
                && m.unit == sign_meta.unit
                && bits(m.transform.as_flattened(), sign_meta.transform.as_flattened());
            if !agree {
                return Err(HermiteError::OtherMeta(name));
            }
        }
        let group_names = group_table(sign_meta)?;
        let [xf, xn, xg, yf, yn, yg, zf, zn, zg] = x else {
            return Err(HermiteError::Missing("an axis"));
        };
        let crossings = |f: usize, n: usize, g: usize| -> Result<CrossingsView<'a>, HermiteError> {
            Ok(CrossingsView {
                fraction: file.view(f)?,
                normal: file.view(n)?,
                group: file.view(g)?,
            })
        };
        let set = HermiteView {
            signs: file.view(*sign_at)?,
            axes: [crossings(*xf, *xn, *xg)?, crossings(*yf, *yn, *yg)?, crossings(*zf, *zn, *zg)?],
            group_names,
        };
        let mut same = Vec::with_capacity(9);
        for c in &set.axes {
            same.push(set.signs.same_topology(&c.fraction));
            same.push(set.signs.same_topology(&c.normal));
            same.push(set.signs.same_topology(&c.group));
        }
        if let Some((name, _)) = FIELDS.iter().skip(1).zip(&same).find(|(_, same)| !**same) {
            return Err(HermiteError::OtherTopology(name));
        }
        let regions = set.signs.regions()?;
        check(&Viewed { set: &set, regions }, &set.group_names)?;
        Ok(set)
    }

    /// The signs, 1 inside, 0 outside.
    pub fn signs(&self) -> &GridView<'a, u8> {
        &self.signs
    }

    /// The crossings on the x, y and z edges.
    pub fn axes(&self) -> &[CrossingsView<'a>; 3] {
        &self.axes
    }

    /// Each group's name, by id; empty, every group is 0.
    pub fn group_names(&self) -> &[String] {
        &self.group_names
    }
}

impl From<FormatError> for HermiteError {
    fn from(e: FormatError) -> Self {
        HermiteError::Format(e)
    }
}
