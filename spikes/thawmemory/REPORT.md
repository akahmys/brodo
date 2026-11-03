# Spike: the memory mestolo's thawing makes

Measured on 2026-11-03 with rustc 1.98.1 on an Apple M5, against brodo and mestolo at the
commit that adds this spike. Peak resident memory is not a timing, so the load was not
held at rest. Reproduce from the repository's root:

```bash
spikes/thawmemory/run.sh out/thawmemory-spike
```

## Question

ADR-0050, after the owner's review of 2026-11-03: `mestolo info` thaws each grid of a
cache file or an archive, beyond what `--limit` bounds in brodo's readers. What does
thawing make for each leaf, tile and node, and which of them can be counted before it?

## Method

`src/main.rs` writes, with brodo's cache writer, one `f32` grid of each shape: `tiles`,
n upper nodes each of 32 767 upper-level tiles; `nodes`, n upper nodes each holding one
lower node of one lower-level tile; `leaves`, n leaves. `run.sh` reads each with
`mestolo info` under no limit, and `/usr/bin/time -l` gives its peak resident memory.

## Results

| Case | n | File (bytes) | Peak resident (bytes) | Leaves, tiles |
| :--- | ---: | ---: | ---: | :--- |
| tiles | 1 | 139 680 | 5 505 024 | 0, 32 767 |
| tiles | 16 | 2 229 536 | 75 743 232 | 0, 524 272 |
| nodes | 4 096 | 38 158 688 | 119 881 728 | 0, 4 096 |
| nodes | 32 768 | 305 267 040 | 923 959 296 | 0, 32 768 |
| leaves | 32 768 | 67 252 640 | 147 570 688 | 32 768, 0 |
| leaves | 262 144 | 537 960 864 | 1 164 394 496 | 262 144, 0 |

Between the two of each case, beside the file's own bytes, which reading holds:

- a tile made about 139 bytes, against the 256 it is counted;
- a leaf of `f32` about 2 381, against 3 072 counted, its values' 2 048 and 1 024;
- a node, an upper node with its lower node, about 18 470 bytes beyond its tile, about
  1.98 times the 9 316 bytes it takes in the file; `GridView` gives no count of nodes, so
  they cannot be counted before thawing.

## Conclusion

Leaves and tiles are counted before thawing within their measured cost; nodes make about
twice the bytes they take in the file, and the file is within the limit. The owner's
decision is in ADR-0050's amendment of 2026-11-03.
