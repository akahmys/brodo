//! Mapping a file into memory (ARCHITECTURE.md §4.7): brodo's one module allowed `unsafe`
//! (CODING.md Rule 4, ADR-0008), holding one unsafe operation. Its tests are ignored under
//! Miri, which cannot map a file; its soundness rests on the argument below.

#![expect(
    unsafe_code,
    reason = "mapping a file is unsafe in Rust: another process can change it under the mapping (ADR-0008)"
)]

use std::path::Path;

use memmap2::Mmap;

use crate::cache::{CacheFile, Storage, index};
use crate::error::CacheError;
use crate::frozen::BLOCK;

impl CacheFile {
    /// Maps a cache file into memory and checks it whole, without a copy. Behind the
    /// feature `mmap`.
    ///
    /// # Safety
    ///
    /// While the returned value lives, the file is not changed or truncated, by this
    /// process or any other: the mapped bytes would change under references that Rust
    /// assumes cannot change, which is undefined behaviour (ADR-0008, ADR-0036). A file
    /// that is merely wrong is refused by the check, not trusted.
    pub unsafe fn map(path: &Path) -> Result<CacheFile, CacheError> {
        let file = std::fs::File::open(path)?;
        let len = file.metadata()?.len();
        if len == 0 || !len.is_multiple_of(BLOCK as u64) {
            return Err(CacheError::Inconsistent("length"));
        }
        // SAFETY: `Mmap::map` is unsafe because the mapped bytes change if the file is
        // modified or truncated while mapped. This function's caller promises, by its own
        // `unsafe`, to keep the file unchanged while the mapping lives (`# Safety` above),
        // and the bytes are checked whole as untrusted input before anything reads them.
        let map = unsafe { Mmap::map(&file) }?;
        let grids = index(&map)?;
        Ok(CacheFile { storage: Storage::Mapped(map), grids })
    }
}
