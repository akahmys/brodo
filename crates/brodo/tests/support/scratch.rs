//! The running test's own directory for the files it writes (TESTING.md T15), included by
//! `#[path]` into every test file that writes one, so that tests run in parallel never
//! share a file. No other test file names `CARGO_TARGET_TMPDIR`
//! (`scripts/audit/tests.py`).

use std::path::PathBuf;

/// `CARGO_TARGET_TMPDIR`, then the test target's name, then the running test's, made if
/// missing. The test harness names each test's thread after the test, its module path
/// joined by `::`, with one thread or many; `::` is written `-`, which every system's
/// paths take.
pub fn dir() -> PathBuf {
    let target = module_path!().split("::").next().unwrap_or("tests");
    let current = std::thread::current();
    let test = current.name().expect("scratch::dir is called on a test's own thread");
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(target).join(test.replace("::", "-"));
    std::fs::create_dir_all(&dir).expect("the test's directory is made");
    dir
}
