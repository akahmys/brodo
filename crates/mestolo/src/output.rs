//! A command's file: refused where one is already there but by `--force`, or where `-o`
//! names no file, before the input is read. brodo's writers write it whole, to a hidden file
//! beside it renamed over it, so that a reader sees the old file or the new one
//! (ADR-0050).

use crate::args::Output;
use crate::error::MestoloError;

/// Refuses `output` if its path names no file, or a file is at its path and `--force` was
/// not given.
pub fn check(output: &Output) -> Result<(), MestoloError> {
    if output.path.file_name().is_none() {
        return Err(MestoloError::Missing("a file name in -o"));
    }
    if !output.force && std::fs::symlink_metadata(&output.path).is_ok() {
        return Err(MestoloError::Exists(output.path.clone()));
    }
    Ok(())
}
