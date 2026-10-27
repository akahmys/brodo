//! A test's work run against the clock: a mutant that makes it endless fails within a
//! minute rather than waiting out the run's timeout (TESTING.md T11).

/// Runs `f` on its own thread, refused unless it ends within a minute.
pub fn in_time<R: Send + 'static>(f: impl FnOnce() -> R + Send + 'static) -> R {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || tx.send(f()).unwrap_or(()));
    rx.recv_timeout(std::time::Duration::from_secs(60)).expect("ends within a minute")
}
