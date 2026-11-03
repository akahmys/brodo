//! The `mestolo` program: runs the command its arguments give, and on an error writes it to
//! standard error and exits with status 2 for a command line refused, 1 otherwise.

#![expect(clippy::print_stderr, reason = "a program reports its error on standard error")]

use std::process::ExitCode;

fn main() -> ExitCode {
    let done = mestolo::args::parse(std::env::args_os().skip(1))
        .and_then(|c| mestolo::run(&c, &mut std::io::stdout().lock()));
    match done {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) if e.is_usage() => {
            eprintln!("mestolo: {e}\n\n{}", mestolo::USAGE);
            ExitCode::from(2)
        }
        Err(e) => {
            eprintln!("mestolo: {e}");
            ExitCode::FAILURE
        }
    }
}
