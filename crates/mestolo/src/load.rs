//! A file read whole, through brodo's readers alone, its kind told by its first bytes
//! (ADR-0050): a cache file, an archive or a `.vdb`, each grid thawed into a topology and
//! a field. A cache file's or an archive's grids are counted before they are thawed, each
//! leaf and tile at what thawing makes of it at most, and refused if they pass the limit.

use std::io::Read;
use std::path::Path;

use brodo::cache::CacheFile;
use brodo::field::{Field, Stored};
use brodo::meta::GridMeta;
use brodo::topology::Topology;
use brodo::vdb::VdbField;

use crate::args::Input;
use crate::error::MestoloError;

/// The unit a `.vdb`'s grids are given, since OpenVDB states none.
pub const VDB_UNIT: &str = "unstated";

/// An OpenVDB file's first eight bytes: its magic number as a little-endian `i64`
/// (docs/formats.md §9.1).
const VDB_MAGIC: [u8; 8] = 0x5644_4220_i64.to_le_bytes();

/// What thawing a tile is counted to make, and a leaf beside its values (ADR-0050).
const TILE: u64 = 256;
const LEAF: u64 = 1024;

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

/// A grid's values, of each type brodo stores.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum Values {
    /// `f32`.
    F32(Field<f32>),
    /// `f64`.
    F64(Field<f64>),
    /// `u8`.
    U8(Field<u8>),
    /// `u32`.
    U32(Field<u32>),
    /// `[f32; 3]`.
    F32x3(Field<[f32; 3]>),
    /// `[f64; 3]`.
    F64x3(Field<[f64; 3]>),
}

impl Values {
    /// The value type's name, as Rust writes it.
    pub fn type_name(&self) -> &'static str {
        match self {
            Values::F32(_) => "f32",
            Values::F64(_) => "f64",
            Values::U8(_) => "u8",
            Values::U32(_) => "u32",
            Values::F32x3(_) => "[f32; 3]",
            Values::F64x3(_) => "[f64; 3]",
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
    pub values: Values,
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
    std::fs::File::open(path)?.take(8).read_to_end(&mut head)?;
    if head.starts_with(b"BRDC") {
        from_cache(CacheFile::read(path, input.limit)?, Kind::Cache, input.limit)
    } else if head.starts_with(b"BRDA") {
        from_cache(brodo::archive::read(path, input.limit)?, Kind::Archive, input.limit)
    } else if head == VDB_MAGIC {
        from_vdb(path, input.limit)
    } else if head.starts_with(b"BRDG") {
        Err(MestoloError::BareGrid(path.to_path_buf()))
    } else {
        Err(MestoloError::UnknownKind(path.to_path_buf()))
    }
}

/// What thawing may still make of the limit.
struct Budget {
    left: u64,
    limit: u64,
}

fn from_cache(file: CacheFile, kind: Kind, limit: u64) -> Result<Loaded, MestoloError> {
    let mut grids = Vec::new();
    let mut budget = Budget { left: limit, limit };
    for i in 0..file.grid_count() {
        let b = &mut budget;
        let (topology, values) = match file.value_type(i)? {
            f32::TYPE_CODE => thaw(&file, i, Values::F32, b)?,
            f64::TYPE_CODE => thaw(&file, i, Values::F64, b)?,
            u8::TYPE_CODE => thaw(&file, i, Values::U8, b)?,
            u32::TYPE_CODE => thaw(&file, i, Values::U32, b)?,
            <[f32; 3]>::TYPE_CODE => thaw(&file, i, Values::F32x3, b)?,
            <[f64; 3]>::TYPE_CODE => thaw(&file, i, Values::F64x3, b)?,
            code => return Err(MestoloError::ValueType(code)),
        };
        grids.push(Grid { meta: file.meta(i)?.clone(), topology, values });
    }
    Ok(Loaded { kind, grids, skipped: Vec::new() })
}

/// Grid `i` thawed, once its leaves and tiles are counted within `budget`.
fn thaw<T: Stored>(
    file: &CacheFile,
    i: usize,
    wrap: fn(Field<T>) -> Values,
    budget: &mut Budget,
) -> Result<(Topology, Values), MestoloError> {
    let view = file.view::<T>(i)?;
    let values = u64::try_from(512 * size_of::<T>()).unwrap_or(u64::MAX);
    let count = |n: usize, each: u64| u64::try_from(n).unwrap_or(u64::MAX).saturating_mul(each);
    let need = count(view.leaf_count(), values.saturating_add(LEAF))
        .saturating_add(count(view.tile_count(), TILE));
    let counted = (budget.limit - budget.left).saturating_add(need);
    budget.left = budget
        .left
        .checked_sub(need)
        .ok_or(MestoloError::TooMuchToThaw { counted, limit: budget.limit })?;
    let (t, f) = view.thaw()?;
    Ok((t, wrap(f)))
}

fn from_vdb(path: &Path, limit: u64) -> Result<Loaded, MestoloError> {
    let file = brodo::vdb::read(path, VDB_UNIT, limit)?;
    let mut loaded = Loaded { kind: Kind::Vdb, grids: Vec::new(), skipped: file.skipped };
    for g in file.grids {
        // `VdbField` is `#[non_exhaustive]`: a type brodo adds is listed as skipped.
        let values = match g.field {
            VdbField::F32(f) => Values::F32(f),
            VdbField::F64(f) => Values::F64(f),
            _ => {
                loaded.skipped.push(g.meta.name);
                continue;
            }
        };
        loaded.grids.push(Grid { meta: g.meta, topology: g.topology, values });
    }
    Ok(loaded)
}
