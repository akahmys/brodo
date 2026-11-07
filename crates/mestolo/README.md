# mestolo

A command that looks inside [brodo](https://github.com/akahmys/brodo)'s files, for
someone who receives a cache file, an archive or a `.vdb` and writes no Rust. It needs no
GPU and no window. ARCHITECTURE.md §6.3 says what each command gives.

```bash
cargo install mestolo

mestolo info grids.cache                                   # what the file holds
mestolo slice grids.cache --grid sdf --axis z --at 0 -o z0.png
mestolo convert grids.cache -o grids.vdb                   # for Blender, Houdini
mestolo convert grids.cache --grid sdf -o sdf.vtu          # for ParaView
mestolo convert grids.vdb --as archive -o grids.archive     # brodo's compressed archive
```

- A file's kind is told by its first bytes, not its name, and it is read whole within
  `--limit` bytes, 4 GiB unless given, which bounds the file and what reading and thawing
  its leaves and tiles make; its nodes make about twice the bytes they take in the file.
- Every string the file holds is written quoted and escaped, so that none can move your
  terminal.
- `--grid` names the grid where a file holds more than one.
- A file already at the output's path is refused unless `--force` is given.
- `convert` writes the format `--as` names, `vdb`, `vtu`, `cache` or `archive`, or else
  the one the output's name ends in, `.vdb` or `.vtu`.
- A command line refused, none given among them, exits with status 2, a command failing
  with 1.

## As a library

The program is a thin user of the library, which runs a command line into any writer.
Its promises are `run`, `USAGE`, `args` and `error`, as semantic versioning holds them; the
command's other parts are public for its tests alone, hidden from its documentation, and
promise nothing:

```rust
use mestolo::args::{Command, parse};

let command = parse(["help"].map(std::ffi::OsString::from)).unwrap();
assert_eq!(command, Command::Help);
let mut out = Vec::new();
mestolo::run(&command, &mut out).unwrap();
assert!(out.starts_with(b"usage: mestolo"));
```
