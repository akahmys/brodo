//! `convert`: a file's grids written as `.vdb`, by brodo's writer, which takes `f32` and
//! `f64` grids; or one grid as an ASCII `.vtu` (ARCHITECTURE.md §4.7, §6.1).

use std::ffi::OsStr;
use std::path::Path;

use brodo::vdb::VdbEntry;
use brodo::vtk::write;

use crate::error::MestoloError;
use brodo::field::AnyField;

use crate::load::{Grid, Loaded};

/// Writes `loaded` at `output` in the format its name ends in: every grid as `.vdb`, or
/// the one `grid` names; one grid as `.vtu`, its values named as the grid is.
pub fn convert(loaded: &Loaded, grid: Option<&str>, output: &Path) -> Result<(), MestoloError> {
    match output.extension().and_then(OsStr::to_str) {
        Some("vdb") => {
            let grids = match grid {
                Some(_) => vec![loaded.grid(grid)?],
                None => loaded.grids.iter().collect(),
            };
            let entries = grids.into_iter().map(entry).collect::<Result<Vec<_>, _>>()?;
            Ok(brodo::vdb::write(output, &entries)?)
        }
        Some("vtu") => vtu(loaded.grid(grid)?, output),
        _ => Err(MestoloError::OutputKind(output.to_path_buf())),
    }
}

fn entry(g: &Grid) -> Result<VdbEntry, MestoloError> {
    let (m, t) = (&g.meta, &g.topology);
    match &g.field {
        AnyField::F32(f) => Ok(VdbEntry::new(m, t, f)?),
        AnyField::F64(f) => Ok(VdbEntry::new(m, t, f)?),
        AnyField::U8(_) | AnyField::U32(_) | AnyField::F32x3(_) | AnyField::F64x3(_) => {
            Err(MestoloError::NotWritten { grid: m.name.clone(), value_type: g.field.value_type() })
        }
        _ => Err(MestoloError::ValueType(g.field.value_type())),
    }
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
