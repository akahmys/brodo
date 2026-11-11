//! `convert`: a file's grids written as `.vdb`, by brodo's writer, which takes grids of
//! `f32`, `f64`, `[f32; 3]` and `[f64; 3]`; as a cache file or an archive, a cache file's or an archive's entries
//! copied as they were; or one grid as an ASCII `.vtu` (ARCHITECTURE.md §4.7, §6.1, §6.3).

use std::ffi::OsStr;
use std::path::Path;

use brodo::cache::{CacheEntry, CacheFile};
use brodo::field::AnyField;
use brodo::vdb::{VdbEntry, VdbFile};
use brodo::vtk::write;

use std::io::Write;

use crate::args::{Format, Input, Output};
use crate::error::MestoloError;
use crate::load::{Grid, Loaded, Opened, load, open};

/// Writes `input`'s grids at `output` in `format`, or with none in the format the output's
/// name ends in, `.vdb` or `.vtu`: every grid, or the one `grid` names, as `.vdb`, a cache
/// file or an archive; one grid as `.vtu`, its values named as the grid is. Grids `.vdb`
/// cannot hold are refused, or, if `output.skip_unsupported`, left out and each written to
/// `out` as a line naming it and its value type; none left is refused.
pub fn convert(
    input: &Input,
    grid: Option<&str>,
    format: Option<Format>,
    output: &Output,
    out: &mut impl Write,
) -> Result<(), MestoloError> {
    let (skip, output) = (output.skip_unsupported, output.path.as_path());
    let by_name = || match output.extension().and_then(OsStr::to_str) {
        Some("vdb") => Ok(Format::Vdb),
        Some("vtu") => Ok(Format::Vtu),
        _ => Err(MestoloError::OutputKind(output.to_path_buf())),
    };
    match format.map_or_else(by_name, Ok)? {
        Format::Vdb => vdb(&load(input)?, grid, output, skip.then_some(out)),
        Format::Vtu => vtu(load(input)?.grid(grid)?, output),
        Format::Cache => Ok(brodo::cache::write(output, &entries(&open(input)?, grid)?)?),
        Format::Archive => Ok(brodo::archive::write(output, &entries(&open(input)?, grid)?)?),
    }
}

/// Every grid, or the one `grid` names, as `.vdb`, refused naming every grid of a type
/// `.vdb` does not hold, or, given `skipped`, each such grid left out and named there.
fn vdb(
    loaded: &Loaded,
    grid: Option<&str>,
    output: &Path,
    skipped: Option<&mut impl Write>,
) -> Result<(), MestoloError> {
    let grids = match grid {
        Some(_) => vec![loaded.grid(grid)?],
        None => loaded.grids.iter().collect(),
    };
    let (grids, not_written): (Vec<_>, Vec<_>) = grids.into_iter().partition(|g| {
        matches!(
            g.field,
            AnyField::F32(_) | AnyField::F64(_) | AnyField::F32x3(_) | AnyField::F64x3(_)
        )
    });
    let not_written: Vec<_> =
        not_written.iter().map(|g| (g.meta.name.clone(), g.field.value_type())).collect();
    match skipped {
        Some(out) if !grids.is_empty() => {
            for (name, value_type) in &not_written {
                writeln!(out, "skipped {name:?}: .vdb does not hold {value_type}")?;
            }
        }
        _ if not_written.is_empty() => {}
        _ => return Err(MestoloError::NotWritten(not_written)),
    }
    let entries = grids.into_iter().map(entry).collect::<Result<Vec<_>, _>>()?;
    Ok(brodo::vdb::write(output, &entries)?)
}

fn entry(g: &Grid) -> Result<VdbEntry, MestoloError> {
    let (m, t) = (&g.meta, &g.topology);
    match &g.field {
        AnyField::F32(f) => Ok(VdbEntry::new(m, t, f)?),
        AnyField::F64(f) => Ok(VdbEntry::new(m, t, f)?),
        AnyField::F32x3(f) => Ok(VdbEntry::new(m, t, f)?),
        AnyField::F64x3(f) => Ok(VdbEntry::new(m, t, f)?),
        AnyField::U8(_) | AnyField::U32(_) | _ => {
            Err(MestoloError::ValueType(g.field.value_type()))
        }
    }
}

/// A file's entries for a cache file or an archive: a cache file's or an archive's copied
/// as they were, its grids and then its entries of bytes, or the one grid `grid` names; a
/// `.vdb`'s grids as read, or the one named.
fn entries(opened: &Opened, grid: Option<&str>) -> Result<Vec<CacheEntry>, MestoloError> {
    match opened {
        Opened::Cache { file, .. } => copied(file, grid),
        Opened::Vdb(file) => from_vdb(file, grid),
    }
}

fn copied(file: &CacheFile, grid: Option<&str>) -> Result<Vec<CacheEntry>, MestoloError> {
    let grids = file.grids();
    let Some(name) = grid else {
        let bytes = file.bytes_entries().into_iter().map(CacheEntry::copy_bytes);
        return Ok(grids.into_iter().map(CacheEntry::copy).chain(bytes).collect());
    };
    let g = grids.into_iter().find(|g| g.meta().name == name);
    Ok(vec![CacheEntry::copy(g.ok_or_else(|| MestoloError::NoGrid(name.to_owned()))?)])
}

fn from_vdb(file: &VdbFile, grid: Option<&str>) -> Result<Vec<CacheEntry>, MestoloError> {
    let chosen: Vec<_> = match grid {
        None => file.grids.iter().collect(),
        Some(name) => vec![
            file.grids
                .iter()
                .find(|g| g.meta.name == name)
                .ok_or_else(|| MestoloError::NoGrid(name.to_owned()))?,
        ],
    };
    chosen
        .into_iter()
        .map(|g| match &g.field {
            AnyField::F32(f) => Ok(CacheEntry::new(&g.meta, &g.topology, f)?),
            AnyField::F64(f) => Ok(CacheEntry::new(&g.meta, &g.topology, f)?),
            AnyField::F32x3(f) => Ok(CacheEntry::new(&g.meta, &g.topology, f)?),
            AnyField::F64x3(f) => Ok(CacheEntry::new(&g.meta, &g.topology, f)?),
            AnyField::U8(_) | AnyField::U32(_) | _ => {
                Err(MestoloError::ValueType(g.field.value_type()))
            }
        })
        .collect()
}

fn vtu(g: &Grid, output: &Path) -> Result<(), MestoloError> {
    let (t, name) = (&g.topology, g.meta.name.as_str());
    match &g.field {
        AnyField::F32(f) => write(output, t, f, name)?,
        AnyField::F64(f) => write(output, t, f, name)?,
        AnyField::U8(f) => write(output, t, f, name)?,
        AnyField::U32(f) => write(output, t, f, name)?,
        AnyField::F32x3(f) => write(output, t, f, name)?,
        AnyField::F64x3(f) => write(output, t, f, name)?,
        _ => return Err(MestoloError::ValueType(g.field.value_type())),
    }
    Ok(())
}
