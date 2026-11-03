<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/logo/brodo-logo-dark.svg">
  <img alt="brodo: sparse volumes in pure Rust" src="docs/logo/brodo-logo-light.svg" width="410">
</picture>

# brodo

Sparse 3D volumes in pure Rust. Store fields over the voxels that matter rather than a
whole box, edit where they lie, and save them to files you can read back in place, map
from disk, compress, or open in OpenVDB tools. Turn triangle meshes into signed distance
fields, and find out whether the voxelisation changed the shape's topology.

- **[brodo](crates/brodo/README.md)**: the sparse tree, in the style of VDB, with many
  fields sharing one topology; connected components, the Euler characteristic, dilation
  and erosion; its own file formats and OpenVDB's `.vdb`; Hermite data; and a slice as PNG
  or a field for ParaView, to check a result by eye.
- **[colino](crates/colino/README.md)**: reads STL, OBJ and 3MF meshes and converts them
  to narrow-band signed distance fields and Hermite data on brodo's tree, with a report of
  the holes, cavities, thin walls and narrow gaps the conversion may have closed or opened.
- **[mestolo](crates/mestolo/README.md)**: a command that shows what a cache file, an
  archive or a `.vdb` holds, draws a slice of a grid as PNG, and converts grids to `.vdb`
  or `.vtu`, for someone who writes no Rust.

Each crate's README has an example that runs as a test.

## Using it

The crates are not on crates.io. Depend on them from Git, pinning a release tag rather
than a branch or a revision:

```toml
[dependencies]
brodo = { git = "https://github.com/akahmys/brodo", tag = "v0.8.2" }
colino = { git = "https://github.com/akahmys/brodo", tag = "v0.8.2" }
```

- They need Rust 1.98 or later.
- Pin the same tag in every crate of one program that uses brodo: two different revisions
  build two copies of brodo whose types do not mix. If you need a change before the next
  release, ask for a small release rather than pinning a revision (ADR-0040).
- Until 1.0, a minor version (0.8 to 0.9) may break the API, and a patch version (0.8.1
  to 0.8.2) is meant only to fix. [CHANGELOG.md](CHANGELOG.md) lists what each release
  changes and anything it breaks.

## Documentation

- The API reference, built from the code with every feature and opened in a browser
  (docs.rs has none, the crates not being on crates.io):

  ```bash
  cargo doc --workspace --no-deps --all-features --open
  ```

- What the crates do and promise: [ARCHITECTURE.md](ARCHITECTURE.md), its scope in §1,
  brodo in §4 and colino in §5.
- The bytes of each file brodo writes, for a reader in another language:
  [docs/formats.md](docs/formats.md).
- Where the work is, and what is planned, a viewer among it:
  [ROADMAP.md](ROADMAP.md).

## How it is written

brodo is written by a human, its owner, together with AI coding agents: chiefly Claude
Code, and Antigravity in support. The owner sets the direction, decides every rule and
design question, and approves each release; the agents write code, tests and documents
under the rules in [AGENTS.md](AGENTS.md). A change from any of them lands only through
the same gate of checks and tests (TESTING.md §4), and each commit Claude Code took part
in names it as a co-author.

## Issues and pull requests

[Issues](https://github.com/akahmys/brodo/issues) are welcome: a bug with the steps that
show it and the result you expected, or a request for something the crates do not do.

Pull requests are not accepted, and only collaborators can open one. brodo is MIT or
Apache-2.0, and must hold no code derived from a project under a copyleft licence, not
even by translation or from memory (AGENTS.md S1). Of code written elsewhere, that cannot
be shown, however good the code, so none is taken: a fix is written here from an issue's
steps and expected result, never from code the issue carries (AGENTS.md S5).

## Licence

Licensed under either of [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your
option.

## Working on brodo

Contributors, human or agent, start with [AGENTS.md](AGENTS.md). To build and test:

```bash
cargo build --workspace
cargo test --workspace --all-features
```

To set up a clone that can land a change:

1. Install the commit hooks:

   ```bash
   git config core.hooksPath scripts/hooks
   ```

2. Install what the gate needs: `rustup` (it selects the toolchain pinned in
   `rust-toolchain.toml`), `cargo-deny`, `cargo-mutants` at the version TESTING.md §3
   pins, `betterleaks`, `python3` and `git`; for the fuzzer and Miri, a nightly toolchain,
   `cargo-fuzz` and the `miri` component (TESTING.md §3).
3. Write the personal rules, which match the owner's name and stay outside the repository
   (AUDITING.md §2), at `~/.config/betterleaks/personal.toml` or `$PERSONAL_LEAKS`; and
   the local list of forbidden sources (AGENTS.md S1), one project a line, at
   `~/.config/brodo/forbidden-sources` or `$BRODO_FORBIDDEN_SOURCES`.
4. Run the gate:

   ```bash
   ./scripts/audit/verify_compliance.sh
   ```
