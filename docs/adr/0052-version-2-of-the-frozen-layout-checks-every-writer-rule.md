# ADR-0052: Version 2 of the frozen layout and the cache profile checks every writer rule

- **Status**: Accepted
- **Date**: 2026-11-03

## Context

Version 1's readers leave some of the writer's rules unchecked (docs/formats.md §2.5, §3.4):
padding and the background's bytes beyond its size, the order of nodes, whether every node
and leaf is reached, the places of root tiles and of each node's first child and tile, that
each section starts right after the one before, and a cache file's bounds against its grid
(ROADMAP.md §3, B-022). Each check a reader adds refuses files version 1 accepts, so B-022
waited for a version raised for another reason (ADR-0038). Hermite data held sparsely
raises the cache profile's version (ADR-0051), and with it the frozen layout's and the
archive profile's (docs/formats.md §6).

Two of these were checked at version 1 already where they cost most: a Hermite set and
thawing refuse a node, leaf or tile value reached twice and a leaf never reached
(ADR-0038's amendments, ADR-0042).

docs/formats.md §2.7, §3.5, §4.3 and §8.7 state version 2 as proposed here.

## Decision

The owner chose, on 2026-11-03, the first option of each point, as proposed.

- **a. What a version 2 reader checks**: (1) every rule its writer keeps: §2.7's rules 7 to
  11, padding zero, sections with no gap, root tiles numbered in table order and each
  upper node named once, nodes in Morton order, and each node's first child and first
  tile the count before it. Rule 11 gives every node, leaf and tile value one place that reaches it, so
  that §2.6's walk is needed no more, and checking costs one pass over the nodes. (2) The
  structural rules alone, 8 to 11, padding left unchecked: a reader still refuses what
  would be read wrong, but a file's padding could carry data no reader sees.
- **b. A cache file's bounds**: (1) checked against the grid's leaves and tiles, by a walk
  of the nodes, which rule 11 makes one pass. (2) Dropped from the metadata at version 2,
  a reader working them out when asked: one fact fewer in the file, but every reader
  asking for them walks the tree.
- **c. An entry of bytes**: as docs/formats.md §3.5 states: a kind word in the directory,
  its byte length in its metadata after its name, so that the archive's directory entry,
  whose words are all used, needs no new one; its payload cut into chunks of 256 KiB in
  an archive.
- **d. Version 1**: read as now, its checks unchanged, and written on request (TESTING.md
  V6, ADR-0038); a release after the next refuses it.

## Consequences

docs/formats.md §2.7, §3.5, §4.3 and §8.7 state version 2; brodo writes it, reads it and
version 1, and writes version 1 on request, in the unit that closes P8 (ROADMAP.md §2).
A reader of version 2 needs no walk of its own to find a node reached twice, so the
checks ADR-0038's amendments added at version 1 stay for version 1 alone. `formats.py`,
the reader written from the document, learns version 2 with the crates, and each version
2 golden file joins §7's.
