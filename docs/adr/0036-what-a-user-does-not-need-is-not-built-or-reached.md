# ADR-0036: What a user does not need is not built or reached

- **Status**: Amended
- **Date**: 2026-10-25

## Context

Anthropic's article on attack-surface reduction (2026) argued that making code harder to
reach lowers the cost of bugs not yet found: code reachable by default that no default use
needs, options that together weaken a check, and complex code run before a check. brodo is
a library, and what reaches it from outside is a file another party made: a mesh, a cache
file, a Hermite set. Every module parsing one is in `UNTRUSTED_INPUT`, fuzzed and barred
from indexing (ADR-0009). The code as it stood (2026-10-25):

- colino built all three readers for every user. Only the 3MF reader uses `rawzip`,
  `miniz_oxide` and `quick-xml`, a ZIP, a deflate and an XML parser; STL and OBJ use none.
- brodo's features `slice-png` and `vtk` were on by default (ADR-0018), so that the gate's
  tests covered them; the gate has since tested and linted with all features and with each
  alone (ADR-0021). Only `slice-png` uses a crate, `png`.
- `CacheFile::read` read a whole file into memory, then copied it into an aligned buffer,
  before any check, with no limit on its size.
- `CacheFile::map`'s promise, that the file is not changed while mapped or the behaviour is
  undefined, was a sentence in its documentation.
- No option weakened a check: readers, the cache file and `GridView` check every input
  whole, with no way to skip it, and `Limits` takes no default. The approximate winding
  number changes accuracy, not safety.

## Decision

The owner chose, on 2026-10-25, the recommended option of each point; ADR-0008 is amended
with the change that makes `map` an `unsafe fn`, in the same commit:

- **a. colino's readers.** (1) Features `stl`, `obj` and `threemf`, all on by default, so
  that a user who reads only STL, or builds meshes itself, can leave the rest unbuilt.
  (2) The same with `threemf` off by default. (3) As now. *Recommended: (1)*: reading these
  formats is what colino is for, and the reduction is there for any user who asks.
- **b. brodo's checking output.** (1) `slice-png` and `vtk` off by default, opted into.
  (2) As now. *Recommended: (1)*: neither is needed to hold, edit or store a grid, and the
  gate already covers every feature.
- **c. Reading a cache file.** (1) `CacheFile::read` takes a byte limit and refuses a larger
  file by its length before reading it, as colino's readers do with `Limits`. (2) As now,
  the caller checking the size. *Recommended: (1)*.
- **d. Mapping a cache file.** (1) `CacheFile::map` becomes an `unsafe fn`, its promise
  stated under `# Safety`, as `memmap2::Mmap::map` is; brodo's own tests and benchmarks that
  map then hold one `unsafe` block each, which ADR-0008 is amended to allow. (2) A feature
  `mmap`, on by default, so that a user who never maps can leave `memmap2` and the unsafe
  module unbuilt; the promise stays in the documentation. (3) Both. (4) As now.
  *Recommended: (3)*: the caller's promise is then written where the compiler asks for it,
  and the code is not built where it is not used.

## Consequences

Each change is breaking and listed in CHANGELOG.md for 0.5.0. The review finds no option
that weakens a check and no parsing before one, so none is changed for that.

## Amendment: the tree behind the topology

- **Date**: 2026-10-28

`brodo::tree::Tree` was public, its documentation binding the orders it walks leaves and
nodes in and a leaf numbering of its own, though every user in ARCHITECTURE.md §1 builds and
reads a topology through `Topology` and `TopologyEditor`. A review of the promises
(2026-10-28) found it. The human decided that `Tree` is crate-private; its three methods
only tests called are built for tests alone. How a module's code works, as its
documentation tells it, promises nothing (AGENTS.md), so colino's `bvh`, `distance`,
`intersect` and `winding` stay public for what their functions give.
