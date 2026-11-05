//! A file's kind, told by its first bytes, its magic number (docs/formats.md §2.1, §3.1,
//! §8.1, §9.1; ADR-0056): each format's magic number is here, and its module reads it from
//! here, so that the kinds are told apart whichever features are built.

/// "BRDG", a frozen grid's magic number, read as a little-endian `u32`.
pub(crate) const GRID_MAGIC: u32 = u32::from_le_bytes(*b"BRDG");
/// "BRDC", a cache file's.
pub(crate) const CACHE_MAGIC: u32 = u32::from_le_bytes(*b"BRDC");
/// "BRDA", an archive's.
pub(crate) const ARCHIVE_MAGIC: u32 = u32::from_le_bytes(*b"BRDA");
/// An OpenVDB file's, an `i64`.
pub(crate) const VDB_MAGIC: i64 = 0x5644_4220;

/// The kind of a file brodo reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum FileKind {
    /// A bare frozen grid (`view::GridView::new`).
    Grid,
    /// A cache file (`cache::read`).
    Cache,
    /// An archive (`archive::read`).
    Archive,
    /// An OpenVDB file (`vdb::read`).
    Vdb,
}

impl FileKind {
    /// The bytes [`FileKind::of`] needs to tell every kind.
    pub const HEAD: usize = 8;

    /// The kind whose magic number `head`, a file's first bytes, starts with; none if no
    /// kind's does. That the rest of the file is of the kind is the reader's to check.
    pub fn of(head: &[u8]) -> Option<Self> {
        let word = |n: usize| head.get(..n);
        if word(8) == Some(&VDB_MAGIC.to_le_bytes()[..]) {
            return Some(FileKind::Vdb);
        }
        let magic = u32::from_le_bytes(word(4)?.try_into().ok()?);
        match magic {
            GRID_MAGIC => Some(FileKind::Grid),
            CACHE_MAGIC => Some(FileKind::Cache),
            ARCHIVE_MAGIC => Some(FileKind::Archive),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_kind_is_told_by_its_magic_number_and_no_other_bytes_are() {
        let cases: [(&[u8], Option<FileKind>); 9] = [
            (b"BRDG\x02\0\0\0", Some(FileKind::Grid)),
            (b"BRDC", Some(FileKind::Cache)),
            (b"BRDAxyz", Some(FileKind::Archive)),
            (&[0x20, 0x42, 0x44, 0x56, 0, 0, 0, 0], Some(FileKind::Vdb)),
            (&[0x20, 0x42, 0x44, 0x56, 0, 0, 0, 1], None),
            (&[0x20, 0x42, 0x44, 0x56], None),
            (b"BRD", None),
            (b"BRDX\0\0\0\0", None),
            (b"", None),
        ];
        for (head, kind) in cases {
            assert_eq!(FileKind::of(head), kind, "{head:?}");
        }
    }
}
