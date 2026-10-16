# Changelog

Each version, newest first, with the milestones it closed and every change that breaks a
user (TESTING.md V2, V3). Format versions of the `GridView` layout and the file profiles
are recorded here when they change (V6).

## Unreleased

- `brodo::tree`: the sparse tree, with activation, tiles and their split; `brodo::error`.
- `brodo::topology` and `brodo::field`: topologies numbered in Morton order, their editor and
  `Remap`, and fields that refuse a stale topology. `Tree::leaves` and `Tree::tiles`.
- `brodo::manifest`: each leaf's 26 neighbours, built with its topology at each commit.
- `brodo::parallel` and `brodo::gather`: the leaf-parallel update, the fixed-order reduction,
  and the gather of a leaf with one ghost layer.
- `brodo::grid`: `Grid<T>`, one topology and one field, edited and remapped in one step.

## 0.1.0 (2026-10-16)

Closes P0: the repository with its rules, the `brodo` and `colino` crates (empty), and the
design. No API and no format yet.
