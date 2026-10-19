//! Wavefront OBJ (ARCHITECTURE.md §5.1, ADR-0026), after the Wavefront specification as
//! published at paulbourke.net/dataformats/obj/. This module parses untrusted bytes: it is
//! listed in `UNTRUSTED_INPUT`, indexes nothing (CODING.md Rule 3), and is fuzzed
//! (TESTING.md T6).
//!
//! Read: `v x y z`, with an optional weight `w`, or with three colour numbers, a common
//! extension; both unused. `f`, three or more vertex references, each `v`, `v/vt`,
//! `v/vt/vn` or `v//vn`, of which only `v` is read: numbered from 1 through the file, or
//! negative, counting back from the vertices read so far. A polygon becomes a fan of
//! triangles from its first vertex. `#` begins a comment, and a line ending in `\` goes on
//! in the next.
//!
//! Skipped by name, having no part in a mesh's triangles: texture and normal vertices,
//! grouping, display and rendering attributes, points and lines. Refused: free-form
//! geometry, which defines shape this reader does not triangulate; `call` and `csh`, which
//! would read another file or run a command; and any statement the format does not name.

use crate::error::ReadError;
use crate::mesh::{Limits, Mesh};

/// Statements with no part in the triangles: skipped.
const SKIPPED: &[&str] = &[
    "vt",
    "vn",
    "g",
    "s",
    "mg",
    "o",
    "usemtl",
    "mtllib",
    "usemap",
    "maplib",
    "bevel",
    "c_interp",
    "d_interp",
    "lod",
    "shadow_obj",
    "trace_obj",
    "ctech",
    "stech",
    "p",
    "l",
];

/// Statements refused: free-form geometry, and the two that reach outside the file.
const REFUSED: &[&str] = &[
    "vp", "cstype", "deg", "bmat", "step", "curv", "curv2", "surf", "parm", "trim", "hole", "scrv",
    "sp", "end", "con", "bsp", "bzp", "cdc", "cdp", "res", "call", "csh",
];

/// Each statement with the line it begins on, from 1: comments dropped, continued lines
/// joined.
fn statements(text: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    let mut pending: Option<(usize, String)> = None;
    for (n, raw) in (1..).zip(text.lines()) {
        let line = raw.split('#').next().unwrap_or("");
        let (body, goes_on) = match line.trim_end().strip_suffix('\\') {
            Some(b) => (b, true),
            None => (line, false),
        };
        let (start, mut joined) = pending.take().unwrap_or((n, String::new()));
        joined.push(' ');
        joined.push_str(body);
        if goes_on {
            pending = Some((start, joined));
        } else {
            out.push((start, joined));
        }
    }
    out.extend(pending);
    out
}

fn number(word: &str, line: usize) -> Result<f64, ReadError> {
    word.parse::<f64>()
        .ok()
        .filter(|x| x.is_finite())
        .ok_or_else(|| ReadError::BadNumberOnLine { line, found: word.to_owned() })
}

/// Reads a `v` statement's numbers into a vertex.
fn vertex<'a>(
    mesh: &mut Mesh,
    words: impl Iterator<Item = &'a str>,
    line: usize,
    limits: Limits,
) -> Result<(), ReadError> {
    let numbers = words.map(|w| number(w, line)).collect::<Result<Vec<_>, _>>()?;
    let bad = ReadError::BadVertex { line, numbers: numbers.len() };
    if ![3, 4, 6].contains(&numbers.len()) {
        return Err(bad);
    }
    let (Some(x), Some(y), Some(z)) = (numbers.first(), numbers.get(1), numbers.get(2)) else {
        return Err(bad);
    };
    if u64::try_from(mesh.vertices.len()).unwrap_or(u64::MAX) >= limits.vertices {
        return Err(ReadError::TooLarge { what: "vertices", limit: limits.vertices });
    }
    mesh.vertices.push([*x, *y, *z]);
    Ok(())
}

/// Reads an `f` statement into a fan of triangles. A positive number is checked once the
/// file is read: the largest, with its line, is kept in `furthest`.
fn face<'a>(
    mesh: &mut Mesh,
    words: impl Iterator<Item = &'a str>,
    line: usize,
    limits: Limits,
    furthest: &mut Option<(usize, i64)>,
) -> Result<(), ReadError> {
    let read_so_far = i64::try_from(mesh.vertices.len()).unwrap_or(i64::MAX);
    let mut corners = Vec::new();
    for w in words {
        let first = w.split('/').next().unwrap_or("");
        let i: i64 = first.parse().map_err(|_| ReadError::BadFace { line })?;
        let at = match i {
            0 => return Err(ReadError::NoVertex { line, index: 0 }),
            i if i < 0 => read_so_far + i,
            i => {
                if furthest.is_none_or(|(_, f)| i > f) {
                    *furthest = Some((line, i));
                }
                i - 1
            }
        };
        corners.push(u32::try_from(at).map_err(|_| ReadError::NoVertex { line, index: i })?);
    }
    let Some((first, rest)) = corners.split_first() else {
        return Err(ReadError::BadFace { line });
    };
    if rest.len() < 2 {
        return Err(ReadError::BadFace { line });
    }
    for pair in rest.windows(2) {
        if let [b, c] = pair {
            if u64::try_from(mesh.triangles.len()).unwrap_or(u64::MAX) >= limits.triangles {
                return Err(ReadError::TooLarge { what: "triangles", limit: limits.triangles });
            }
            mesh.triangles.push([*first, *b, *c]);
        }
    }
    Ok(())
}

/// Reads an OBJ file's bytes.
pub fn read(bytes: &[u8], limits: Limits) -> Result<Mesh, ReadError> {
    limits.check_bytes(u64::try_from(bytes.len()).unwrap_or(u64::MAX))?;
    let text = std::str::from_utf8(bytes).map_err(|_| ReadError::NotText)?;
    let mut mesh = Mesh::default();
    let mut furthest: Option<(usize, i64)> = None;
    for (line, statement) in statements(text) {
        let mut words = statement.split_ascii_whitespace();
        match words.next() {
            None => {}
            Some("v") => vertex(&mut mesh, words, line, limits)?,
            Some("f") => face(&mut mesh, words, line, limits, &mut furthest)?,
            Some(k) if SKIPPED.contains(&k) => {}
            Some(k) if REFUSED.contains(&k) => {
                return Err(ReadError::Unsupported { line, word: k.to_owned() });
            }
            Some(k) => return Err(ReadError::UnknownStatement { line, word: k.to_owned() }),
        }
    }
    if let Some((line, i)) = furthest
        && i > i64::try_from(mesh.vertices.len()).unwrap_or(i64::MAX)
    {
        return Err(ReadError::NoVertex { line, index: i });
    }
    Ok(mesh)
}
