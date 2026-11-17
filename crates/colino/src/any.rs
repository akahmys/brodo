//! A mesh file of any format colino reads, the format told from its bytes (ARCHITECTURE.md
//! §5.1, ADR-0055). This module parses untrusted bytes: it is listed in `UNTRUSTED_INPUT`,
//! indexes nothing (CODING.md Rule 3), and is fuzzed (TESTING.md T6).
//!
//! The format is told by the first of these that holds:
//!
//! 1. the bytes begin with a ZIP local file header, `PK\x03\x04`: 3MF, a ZIP package;
//! 2. their length is 84 + 50 × n bytes, n the little-endian `u32` after an 80-byte
//!    header: binary STL, as `stl::from_bytes` tells it, even when it begins with `solid`;
//! 3. their first word, after any ASCII whitespace, is `solid`, without regard to case, and
//!    ends the bytes or is followed by ASCII whitespace: ASCII STL;
//! 4. otherwise OBJ, which has no mark of its own: bytes that are none of the three are
//!    refused by the OBJ reader, with its error.

#[cfg(any(feature = "stl", feature = "obj", feature = "threemf"))]
use std::path::Path;

#[cfg(any(feature = "stl", feature = "obj", feature = "threemf"))]
use crate::error::ReadError;
#[cfg(any(feature = "stl", feature = "obj", feature = "threemf"))]
use crate::mesh::{Limits, Mesh};

/// A mesh file's format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Format {
    /// STL, binary or ASCII.
    Stl,
    /// Wavefront OBJ.
    Obj,
    /// 3MF, a ZIP package.
    ThreeMf,
}

/// The ZIP local file header's signature.
const ZIP: &[u8] = b"PK\x03\x04";

/// The format of a mesh file's bytes, told by the rules of this module's documentation.
/// Telling it reads no more than the first bytes and the length, and refuses nothing.
pub fn format_of(bytes: &[u8]) -> Format {
    if bytes.starts_with(ZIP) {
        Format::ThreeMf
    } else if binary_stl(bytes) || ascii_stl(bytes) {
        Format::Stl
    } else {
        Format::Obj
    }
}

/// Whether the bytes are as long as a binary STL file of the count they hold.
fn binary_stl(bytes: &[u8]) -> bool {
    let Some(Ok(count)) = bytes.get(80..84).map(<[u8; 4]>::try_from) else { return false };
    let n = u64::from(u32::from_le_bytes(count));
    let len = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    n.checked_mul(50).and_then(|b| b.checked_add(84)) == Some(len)
}

/// Whether the first word is `solid`, in any case.
fn ascii_stl(bytes: &[u8]) -> bool {
    let rest = bytes.trim_ascii_start();
    let Some((word, after)) = rest.split_at_checked(5) else { return false };
    word.eq_ignore_ascii_case(b"solid") && after.first().is_none_or(u8::is_ascii_whitespace)
}

/// Reads a mesh file at `path`, of no more than `limits.bytes`, as `from_bytes` reads its
/// bytes: a longer file is refused by its length before any of it is read, and no more
/// than the limit and one byte are read. Left out of a build with none of the readers.
#[cfg(any(feature = "stl", feature = "obj", feature = "threemf"))]
pub fn read(path: &Path, limits: Limits) -> Result<Mesh, ReadError> {
    from_bytes(&limits.read_file(path)?, limits)
}

/// Reads a mesh file's bytes by the reader of the format `format_of` tells. A format whose
/// reader this build leaves out, by its feature (`stl`, `obj`, `threemf`), is refused as
/// `ReadError::NotBuilt`. Left out of a build with none of the readers.
#[cfg(any(feature = "stl", feature = "obj", feature = "threemf"))]
pub fn from_bytes(bytes: &[u8], limits: Limits) -> Result<Mesh, ReadError> {
    limits.check_bytes(u64::try_from(bytes.len()).unwrap_or(u64::MAX))?;
    match format_of(bytes) {
        #[cfg(feature = "stl")]
        Format::Stl => crate::stl::from_bytes(bytes, limits),
        #[cfg(feature = "obj")]
        Format::Obj => crate::obj::from_bytes(bytes, limits),
        #[cfg(feature = "threemf")]
        Format::ThreeMf => crate::threemf::from_bytes(bytes, limits),
        #[cfg(not(all(feature = "stl", feature = "obj", feature = "threemf")))]
        other => Err(ReadError::NotBuilt(other)),
    }
}

#[cfg(test)]
mod tests {
    /// In a build without the OBJ reader, as `--no-default-features --features stl` makes,
    /// bytes told to be OBJ are refused as `NotBuilt`, and STL's still read.
    #[cfg(all(feature = "stl", not(feature = "obj")))]
    #[test]
    fn a_format_left_out_of_the_build_is_refused() {
        use super::*;
        let limits = Limits::new(16, 16, 1 << 10);
        let obj = from_bytes(b"v 0 0 0\n", limits);
        assert!(matches!(obj, Err(ReadError::NotBuilt(Format::Obj))), "{obj:?}");
        assert!(from_bytes(b"solid\nendsolid\n", limits).is_ok());
    }
}
