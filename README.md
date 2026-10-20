# brodo

Sparse volumes in pure Rust: a VDB-style sparse tree whose fields share one topology, a
zero-copy frozen view and file format, and topology analysis. The repository holds two
crates, and a third after P1:

- `brodo`: the sparse tree, fields, the frozen view `GridView`, file formats, topology
  analysis, and output for checking a field by eye (a slice as PNG, VTK for ParaView).
- `colino`: converts triangle meshes to narrow-band signed distance fields on brodo's tree,
  and reports the topology changes the conversion may cause.
- `piatto`, after P1: a viewer of brodo's grid files, as a library to embed and a small
  windowed program.

Licensed under either of [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your
option.

[ROADMAP.md](ROADMAP.md) says where the work is, and [ARCHITECTURE.md](ARCHITECTURE.md)
what the design is. Contributors, human or agent, start with [AGENTS.md](AGENTS.md).

## Setting up a clone

1. Install the commit hooks:

   ```bash
   git config core.hooksPath scripts/hooks
   ```

2. Install what the gate needs: `rustup` (it selects the toolchain pinned in
   `rust-toolchain.toml`), `cargo-deny`, `betterleaks` and `python3`; for the fuzzer and
   Miri, a nightly toolchain, `cargo-fuzz` and the `miri` component (TESTING.md §3).
3. Write the personal rules, which match the owner's name and stay outside the repository
   (AUDITING.md §2), at `~/.config/betterleaks/personal.toml` or `$PERSONAL_LEAKS`; and
   the local list of forbidden sources (AGENTS.md S1), one project a line, at
   `~/.config/brodo/forbidden-sources` or `$BRODO_FORBIDDEN_SOURCES`.
4. Run the gate:

   ```bash
   ./scripts/audit/verify_compliance.sh
   ```

## Building

```bash
cargo build --workspace
cargo test --workspace --all-features
```
