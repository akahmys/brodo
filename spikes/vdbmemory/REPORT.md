# Spike: the memory `vdb::read` makes

Measured on 2026-11-02 with rustc 1.98 on an Apple M5, against brodo at `c262672`. Peak
resident memory is not a timing, so the load was not held at rest. Reproduce from the
repository's root:

```bash
spikes/vdbmemory/run.sh out/vdbmemory-spike
```

## Question

ROADMAP.md, "After P7", from an outside review of 2026-11-01: how much memory does
`vdb::read` make for each byte of the file it reads, on the file that makes the most? Its
`limit` bounds the file's length alone, where `archive::read`'s bounds what it makes too
(ADR-0036).

## Method

`gen.py` writes, as docs/formats.md §9 states, the two shapes that give brodo the most for
the fewest bytes (§9.7 rule 8, §9.8):

- **Tiles**: upper nodes with no child and no active place, each with indicator 2 and one
  inactive value other than the background: 8209 bytes for 32 768 tiles of brodo's upper
  level, a quarter of a byte a tile; as `f32` (`tiles`) and `f64` (`dtiles`). A lower node
  costs the same per tile and needs an upper node above it, so it gives no more.
- **Leaves**: one upper node of lower nodes full of leaves, every leaf inactive and holding
  the background, as `f64`: about 129 bytes a leaf, 64 of its value mask in the topology
  and 65 in its buffer.

`src/main.rs` reads a file by `vdb::read` and asserts it holds the leaves and tiles it was
written to hold; `/usr/bin/time -l` gives its peak resident memory.

## Results

| Case | n | File (bytes) | Peak resident (bytes) |
| :--- | ---: | ---: | ---: |
| tiles | 1 | 8 492 | 5 914 624 |
| tiles | 4 | 33 119 | 21 938 176 |
| tiles | 16 | 131 627 | 82 952 192 |
| tiles | 64 | 525 659 | 326 156 288 |
| dtiles | 16 | 131 696 | 87 965 696 |
| dtiles | 64 | 525 920 | 338 952 192 |
| leaves | 4 096 | 537 902 | 38 043 648 |
| leaves | 32 768 | 4 243 765 | 288 980 992 |
| leaves | 262 144 | 33 890 669 | 2 299 281 408 |

Between the two largest of each case, each further byte of the file made about 617 bytes
(`tiles`), 637 (`dtiles`) and 68 (`leaves`): about 155 bytes a tile, for the reader's map
of tiles, the topology and the field alive at once, and about 7.7 KB a leaf, its values
held twice, in the reader's map and in the field.

## Conclusion

What `vdb::read` makes is a multiple of the file's length, as its module says, but of
about 640: a 16 MB file can make about 10 GB, which `limit` does not stop. The owner's
decision is in ADR-0047's amendment of 2026-11-02.
