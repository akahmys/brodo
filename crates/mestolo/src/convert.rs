//! `convert`: a file's grids written as `.vdb`, by brodo's writer, which takes `f32` and
//! `f64` grids; or one grid as an ASCII `.vtu` (ARCHITECTURE.md §4.7, §6.1).

use std::ffi::OsStr;
use std::path::Path;

use brodo::vdb::VdbEntry;
use brodo::vtk::write;

use crate::error::MestoloError;
use crate::load::{Grid, Loaded, Values};

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
    match &g.values {
        Values::F32(f) => Ok(VdbEntry::new(m, t, f)?),
        Values::F64(f) => Ok(VdbEntry::new(m, t, f)?),
        Values::U8(_) | Values::U32(_) | Values::F32x3(_) | Values::F64x3(_) => {
            Err(MestoloError::NotWritten { grid: m.name.clone(), value_type: g.values.type_name() })
        }
    }
}

fn vtu(g: &Grid, output: &Path) -> Result<(), MestoloError> {
    let (t, name) = (&g.topology, g.meta.name.as_str());
    match &g.values {
        Values::F32(f) => write(output, t, f, name)?,
        Values::F64(f) => write(output, t, f, name)?,
        Values::U8(f) => write(output, t, f, name)?,
        Values::U32(f) => write(output, t, f, name)?,
        Values::F32x3(f) => write(output, t, f, name)?,
        Values::F64x3(f) => write(output, t, f, name)?,
    }
    Ok(())
}
