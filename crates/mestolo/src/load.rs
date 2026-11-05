//! A file read whole, through brodo's readers alone, its kind told by its first bytes
//! (ADR-0050): a cache file, an archive or a `.vdb`, each grid thawed into a topology and
//! a field. A cache file's or an archive's grids are counted before they are thawed, by
//! what brodo counts thawing to make, and refused if they pass the limit.

use std::io::Read;
use std::path::Path;

use brodo::cache::CacheFile;
use brodo::field::AnyField;
use brodo::kind::FileKind;
use brodo::meta::GridMeta;
use brodo::topology::Topology;

use crate::args::Input;
use crate::error::MestoloError;

/// The unit a `.vdb`'s grids are given, since OpenVDB states none.
pub const VDB_UNIT: &str = "unstated";

/// What kind of file was read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Kind {
    /// brodo's cache profile.
    Cache,
    /// brodo's archive profile.
    Archive,
    /// An OpenVDB file.
    Vdb,
}

impl Kind {
    /// Its name, as `info` writes it.
    pub fn name(self) -> &'static str {
        match self {
            Kind::Cache => "cache file",
            Kind::Archive => "archive",
            Kind::Vdb => "OpenVDB file",
        }
    }
}

/// A grid read.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Grid {
    /// Its metadata.
    pub meta: GridMeta,
    /// Its leaves and tiles.
    pub topology: Topology,
    /// Its values on them.
    pub field: AnyField,
}

/// A file read.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Loaded {
    /// What kind of file it is.
    pub kind: Kind,
    /// Its grids, in its order.
    pub grids: Vec<Grid>,
    /// The names of a `.vdb`'s grids brodo does not read (`brodo::vdb::VdbFile::skipped`).
    pub skipped: Vec<String>,
    /// A cache file's or an archive's entries of bytes, each its name and length.
    pub bytes: Vec<(String, usize)>,
}

impl Loaded {
    /// The grid named `name`, the first of that name; or, with no name, the only grid.
    pub fn grid(&self, name: Option<&str>) -> Result<&Grid, MestoloError> {
        match name {
            Some(n) => self
                .grids
                .iter()
                .find(|g| g.meta.name == n)
                .ok_or_else(|| MestoloError::NoGrid(n.to_owned())),
            None => match self.grids.as_slice() {
                [g] => Ok(g),
                all => Err(MestoloError::WhichGrid(all.len())),
            },
        }
    }
}

/// Reads `input` whole, within its limit, by the kind its first bytes name.
pub fn load(input: &Input) -> Result<Loaded, MestoloError> {
    let path = input.path.as_path();
    let mut head = Vec::new();
    std::fs::File::open(path)?.take(FileKind::HEAD as u64).read_to_end(&mut head)?;
    match FileKind::of(&head) {
        Some(FileKind::Cache) => {
            from_cache(brodo::cache::read(path, input.limit)?, Kind::Cache, input.limit)
        }
        Some(FileKind::Archive) => {
            from_cache(brodo::archive::read(path, input.limit)?, Kind::Archive, input.limit)
        }
        Some(FileKind::Vdb) => from_vdb(path, input.limit),
        Some(FileKind::Grid) => Err(MestoloError::BareGrid(path.to_path_buf())),
        _ => Err(MestoloError::UnknownKind(path.to_path_buf())),
    }
}

/// Each grid thawed, refused once what thawing the grids is counted to make
/// (`GridEntry::thaw_size`) passes `limit` (ADR-0050).
fn from_cache(file: CacheFile, kind: Kind, limit: u64) -> Result<Loaded, MestoloError> {
    let mut grids = Vec::new();
    let mut left = limit;
    for g in file.grids() {
        let need = g.thaw_size()?;
        left = left.checked_sub(need).ok_or(MestoloError::TooMuchToThaw {
            counted: (limit - left).saturating_add(need),
            limit,
        })?;
        let (topology, field) = g.thaw(need)?;
        grids.push(Grid { meta: g.meta().clone(), topology, field });
    }
    let bytes = file.bytes_entries().iter().map(|b| (b.name.to_owned(), b.bytes.len())).collect();
    Ok(Loaded { kind, grids, skipped: Vec::new(), bytes })
}

fn from_vdb(path: &Path, limit: u64) -> Result<Loaded, MestoloError> {
    let file = brodo::vdb::read(path, VDB_UNIT, limit)?;
    let grids = file
        .grids
        .into_iter()
        .map(|g| Grid { meta: g.meta, topology: g.topology, field: g.field })
        .collect();
    Ok(Loaded { kind: Kind::Vdb, grids, skipped: file.skipped, bytes: Vec::new() })
}
