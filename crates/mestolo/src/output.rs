//! Writing a command's file: refused where one is already there but by `--force`, and
//! written whole, to a hidden file beside it renamed over it, so that a reader sees the old
//! file or the new one (ADR-0050).

use std::io::Write;

use crate::args::Output;
use crate::error::MestoloError;

/// Refuses `output` if a file is at its path and `--force` was not given.
pub fn check(output: &Output) -> Result<(), MestoloError> {
    if !output.force && std::fs::symlink_metadata(&output.path).is_ok() {
        return Err(MestoloError::Exists(output.path.clone()));
    }
    Ok(())
}

/// Writes `bytes` at `output`'s path, as the module states.
pub fn write(output: &Output, bytes: &[u8]) -> Result<(), MestoloError> {
    check(output)?;
    let path = output.path.as_path();
    let name = path.file_name().ok_or(MestoloError::Missing("a file name in -o"))?;
    let mut hidden = std::ffi::OsString::from(".");
    hidden.push(name);
    hidden.push(format!(".{}.tmp", std::process::id()));
    let temp = path.with_file_name(hidden);
    let written = (|| -> Result<(), MestoloError> {
        let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&temp, path)?;
        Ok(())
    })();
    if written.is_err() {
        // The hidden file may hold part of the bytes; a file at the path is untouched.
        let _ignored = std::fs::remove_file(&temp);
    }
    written
}
