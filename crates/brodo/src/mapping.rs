//! Mapping a file into memory (ARCHITECTURE.md §4.7): brodo's one module allowed `unsafe`
//! (CODING.md Rule 4, ADR-0008), holding one unsafe operation. Its tests are ignored under
//! Miri, which cannot map a file; its soundness rests on the argument below.

#![expect(
    unsafe_code,
    reason = "mapping a file is unsafe in Rust: another process can change it under the mapping (ADR-0008)"
)]

use std::fs::File;

use memmap2::Mmap;

/// Maps `file` read-only. The caller keeps the file unchanged while the mapping lives
/// (`CacheFile::map` states it to its own callers).
pub(crate) fn map(file: &File) -> std::io::Result<Mmap> {
    // SAFETY: `Mmap::map` is unsafe because the mapped bytes change if the file is modified
    // or truncated while mapped, which Rust's references assume cannot happen. Its only
    // caller, `CacheFile::map`, requires its own caller to keep the file unchanged while the
    // mapping lives (ADR-0008), and checks the mapped bytes whole as untrusted input, so a
    // file that is merely wrong is refused rather than trusted.
    unsafe { Mmap::map(file) }
}
