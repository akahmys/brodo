//! STL, binary and ASCII (ARCHITECTURE.md §5.1), after the Library of Congress's format
//! descriptions fdd000505 and fdd000506. This module parses untrusted bytes: it is listed
//! in `UNTRUSTED_INPUT`, indexes nothing (CODING.md Rule 3), and is fuzzed (TESTING.md T6).
//!
//! Binary: an 80-byte header, a little-endian `u32` triangle count, and 50 bytes a
//! triangle: twelve little-endian `f32`s, a normal and three vertices, then a 2-byte
//! attribute. ASCII: `solid`, an optional name, then per triangle `facet`, optionally
//! `normal` n n n, `outer`
//! `loop` and three `vertex` x y z, `endloop` `endfacet`; then `endsolid` and an optional
//! name; a file
//! may hold several solids. Words are compared without case and separated by any ASCII
//! whitespace; numbers are read as Rust reads an `f64` and must be finite.
//!
//! What the descriptions leave open is decided so: a file whose length its count gives,
//! 84 + 50 × n bytes, is binary even when it begins with `solid`, as binary files may;
//! otherwise it must be ASCII. Normals and attribute bytes are read and not used: a normal
//! follows from the vertices, and the two colour conventions for the attribute cannot be
//! told apart. Each triangle's corners are vertices of their own; `Mesh::welded` joins
//! equal ones.
//!
//! A name is the rest of its line, after `solid` or `endsolid`, its words joined by one
//! space: it may hold the format's own words. On `solid`'s line, it ends before a `facet`
//! the line goes on with `normal` or `outer`, and before an `endsolid` that ends the line,
//! so that a solid written on one line reads.
//!
//! Groups (ADR-0030): in ASCII, each solid is a group, named by its name after `solid`; a
//! binary file's triangles are all in group 0.

use crate::error::ReadError;
use crate::mesh::{Grouping, Limits, Mesh};

const HEADER: usize = 80;
const RECORD: usize = 50;

/// Reads a binary or ASCII STL file's bytes.
pub fn read(bytes: &[u8], limits: Limits) -> Result<Mesh, ReadError> {
    limits.check_bytes(u64::try_from(bytes.len()).unwrap_or(u64::MAX))?;
    match binary_count(bytes) {
        Some(n) => read_binary(bytes, n, limits),
        None => read_ascii(bytes, limits),
    }
}

/// The triangle count, when the bytes are as long as a binary file of it is.
fn binary_count(bytes: &[u8]) -> Option<u64> {
    let count = bytes.get(HEADER..HEADER + 4)?;
    let n = u64::from(u32::from_le_bytes(count.try_into().ok()?));
    let len = u64::try_from(bytes.len()).ok()?;
    (n.checked_mul(50)?.checked_add(84)? == len).then_some(n)
}

fn check(limits: Limits, triangles: u64) -> Result<(), ReadError> {
    if triangles > limits.triangles {
        return Err(ReadError::TooLarge { what: "triangles", limit: limits.triangles });
    }
    if triangles.saturating_mul(3) > limits.vertices {
        return Err(ReadError::TooLarge { what: "vertices", limit: limits.vertices });
    }
    Ok(())
}

/// Each triangle's three corners as three new vertices.
fn push(mesh: &mut Mesh, corners: [[f64; 3]; 3]) -> Result<(), ReadError> {
    let first = u32::try_from(mesh.vertices.len())
        .map_err(|_| ReadError::TooLarge { what: "vertices", limit: u64::from(u32::MAX) })?;
    let ids = [0, 1, 2].map(|k| first.checked_add(k));
    let [Some(a), Some(b), Some(c)] = ids else {
        return Err(ReadError::TooLarge { what: "vertices", limit: u64::from(u32::MAX) });
    };
    mesh.vertices.extend(corners);
    mesh.triangles.push([a, b, c]);
    Ok(())
}

fn read_binary(bytes: &[u8], n: u64, limits: Limits) -> Result<Mesh, ReadError> {
    // The count is checked against the length first, so it allocates for what is there.
    check(limits, n)?;
    let records = bytes.get(HEADER + 4..).ok_or(ReadError::NotStl("no triangles"))?;
    let mut mesh = Mesh::default();
    for (i, record) in (0..).zip(records.as_chunks::<RECORD>().0) {
        let mut floats =
            record.as_chunks::<4>().0.iter().map(|b| f64::from(f32::from_le_bytes(*b)));
        // The normal is not used.
        floats.by_ref().take(3).for_each(drop);
        let mut corners = [[0.0; 3]; 3];
        for corner in &mut corners {
            for c in corner.iter_mut() {
                *c = floats.next().ok_or(ReadError::NotStl("short triangle"))?;
            }
        }
        if corners.iter().flatten().any(|c| !c.is_finite()) {
            return Err(ReadError::NotFinite(i));
        }
        push(&mut mesh, corners)?;
    }
    Ok(mesh)
}

/// A word: its number from 0, the line it is on, from 0, and its text.
type Word<'a> = (usize, usize, &'a str);

/// The words of a text, numbered from 0, each with its line, read ahead as far as asked.
struct Words<'a, I: Iterator<Item = Word<'a>>> {
    words: I,
    ahead: std::collections::VecDeque<Word<'a>>,
    /// The line of the word taken last.
    line: usize,
}

impl<'a, I: Iterator<Item = Word<'a>>> Words<'a, I> {
    /// The word `n` places ahead, not taken.
    fn peek(&mut self, n: usize) -> Option<Word<'a>> {
        while self.ahead.len() <= n {
            let w = self.words.next()?;
            self.ahead.push_back(w);
        }
        self.ahead.get(n).copied()
    }

    /// Takes the next word.
    fn advance(&mut self) -> Option<(usize, &'a str)> {
        let (at, line, w) = self.ahead.pop_front().or_else(|| self.words.next())?;
        self.line = line;
        Some((at, w))
    }

    fn next(&mut self, expected: &'static str) -> Result<(usize, &'a str), ReadError> {
        self.advance().ok_or(ReadError::Unexpected {
            expected,
            at: usize::MAX,
            found: String::new(),
        })
    }

    /// Takes the next word, which must be `keyword`.
    fn keyword(&mut self, keyword: &'static str) -> Result<usize, ReadError> {
        let (at, w) = self.next(keyword)?;
        if w.eq_ignore_ascii_case(keyword) {
            Ok(at)
        } else {
            Err(ReadError::Unexpected { expected: keyword, at, found: w.to_owned() })
        }
    }

    /// Whether the next word is `keyword`, taking it if so.
    fn take_if(&mut self, keyword: &str) -> bool {
        let is = self.peek(0).is_some_and(|(_, _, w)| w.eq_ignore_ascii_case(keyword));
        if is {
            self.advance();
        }
        is
    }

    fn number(&mut self) -> Result<f64, ReadError> {
        let (at, w) = self.next("a number")?;
        w.parse::<f64>()
            .ok()
            .filter(|x| x.is_finite())
            .ok_or_else(|| ReadError::BadNumber { at, found: w.to_owned() })
    }

    /// Takes a name: the rest of the line of the word taken last, joined by one space. A
    /// solid's name ends before a `facet` its line goes on with `normal` or `outer`, and
    /// before an `endsolid` that ends its line, so that a solid written on one line reads.
    fn name(&mut self, solid: bool) -> String {
        let line = self.line;
        let mut name = Vec::new();
        while let Some((_, l, w)) = self.peek(0)
            && l == line
        {
            let after = self.peek(1).filter(|(_, l, _)| *l == line).map(|(_, _, w)| w);
            let is = |a: &str, b: &str| a.eq_ignore_ascii_case(b);
            let facet = is(w, "facet") && after.is_some_and(|a| is(a, "normal") || is(a, "outer"));
            if solid && (facet || (is(w, "endsolid") && after.is_none())) {
                break;
            }
            name.push(w);
            self.advance();
        }
        name.join(" ")
    }
}

/// The words of `text`, each with its line.
fn words(text: &str) -> Words<'_, impl Iterator<Item = Word<'_>>> {
    let words = text
        .lines()
        .enumerate()
        .flat_map(|(line, l)| l.split_ascii_whitespace().map(move |w| (line, w)));
    let words = words.enumerate().map(|(at, (line, w))| (at, line, w));
    Words { words, ahead: std::collections::VecDeque::new(), line: 0 }
}

fn read_ascii(bytes: &[u8], limits: Limits) -> Result<Mesh, ReadError> {
    // Looked for in the bytes first, so that a damaged binary file is not taken for text.
    let start = bytes.iter().position(|b| !b.is_ascii_whitespace()).unwrap_or(bytes.len());
    let solid = bytes.get(start..start + 5).is_some_and(|w| w.eq_ignore_ascii_case(b"solid"));
    if !solid {
        return Err(ReadError::NotStl("neither the length of a binary file nor 'solid'"));
    }
    let text = std::str::from_utf8(bytes).map_err(|_| ReadError::NotText)?;
    let mut words = words(text);
    let mut mesh = Mesh::default();
    let mut triangles = 0_u64;
    let mut grouping = Grouping::default();
    while words.take_if("solid") {
        let name = words.name(true);
        let before = mesh.triangles.len();
        while words.take_if("facet") {
            if words.take_if("normal") {
                for _ in 0..3 {
                    words.number()?;
                }
            }
            words.keyword("outer")?;
            let at = words.keyword("loop")?;
            let mut corners = Vec::with_capacity(3);
            while words.take_if("vertex") {
                corners.push([words.number()?, words.number()?, words.number()?]);
            }
            words.keyword("endloop")?;
            words.keyword("endfacet")?;
            let [a, b, c] = corners[..] else {
                return Err(ReadError::NotATriangle { at, vertices: corners.len() });
            };
            triangles += 1;
            check(limits, triangles)?;
            push(&mut mesh, [a, b, c])?;
        }
        words.keyword("endsolid")?;
        words.name(false);
        grouping.take(&name, mesh.triangles.len() - before);
    }
    match words.advance() {
        None => {
            grouping.into(&mut mesh);
            Ok(mesh)
        }
        Some((at, w)) => {
            Err(ReadError::Unexpected { expected: "solid or the end", at, found: w.to_owned() })
        }
    }
}
