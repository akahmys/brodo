# Spike: vtk (B-012)

Measured on 2026-10-17 with rustc 1.98.1 and VTK 9.6.2's Python package (in a virtual
environment, as the human allowed, under `out/`; ADR-0003's amendment lets the secret
scan skip it), on an Apple M5. Reproduce from the
repository's root:

```bash
cargo run --release --manifest-path spikes/vtk/Cargo.toml -- out/vtk
out/vtk-venv/bin/python -I spikes/vtk/check.py out/vtk
```

`check.py` compared values within 1e-6 when the spike ran; it now compares them exactly at
`Float32` precision, a NaN matching a NaN, since brodo's own files hold values near 100
whose shortest text differs from them by more (2026-10-17). The spike's results hold
under either.

## Question

**B-012**: which VTK format does the checking output write?

## Sources

Only published documentation (ADR-0018): VTK's XML file-format page and its file-format
design document. They give ImageData and UnstructuredGrid with ASCII data, but not the
multiblock (`.vtm`) or hierarchical-box (`.vthb`) structures, nor the byte-count header of
binary data; so the human limited the candidates to ASCII ImageData and UnstructuredGrid.
VTK_VOXEL's number, 11, was read from VTK's public Python API (`vtk.VTK_VOXEL`); its point
order, x fastest, then y, then z, is not stated in the documents read and was checked by
measurement below.

## Candidates

| | File | Cells |
| :--- | :--- | :--- |
| (1) | one UnstructuredGrid, `.vtu` | a VTK_VOXEL per leaf voxel; a tile one VTK_VOXEL of its region's size; points shared |
| (2) | one ImageData, `.vti`, per leaf and per tile | a leaf 8³ cells, a tile one cell spaced by its edge; nothing ties the files together |

## Measurements

Narrow-band sphere SDFs (band ±3), leaves where the band passes, inside tiles. Writing:
fastest of 3. Reading: VTK's XML readers, once.

| Sphere | Leaves, tiles | Cells | Candidate | Size | Files | Write | Read | Samples wrong |
| :--- | :--- | ---: | :--- | ---: | ---: | ---: | ---: | ---: |
| r = 40 | 645, 274 | 330 514 | `.vtu` | 25 MB | 1 | 0.160 s | 0.24 s | 0 of 3679 |
| | | | `.vti` | 4.3 MB | 919 | 0.039 s | 0.07 s | 0 of 3679 |
| r = 80 | 2606, 3025 | 1 337 297 | `.vtu` | 109 MB | 1 | 0.687 s | 1.01 s | 0 of 16 781 |
| | | | `.vti` | 25 MB | 5631 | 0.223 s | 0.39 s | 0 of 16 781 |

Samples are every 97th voxel's centre and every tile's centre; a sample is wrong when the
cell VTK finds there holds another value. Cell counts equal leaves × 512 + tiles in both.

Checked to fail: writing VTK_VOXEL's points y fastest made every `.vtu` sample wrong; swapping
the x and y of the `.vti` origins made 1376 of 3679 wrong.

## Findings

1. Both candidates are read by VTK exactly, cell for cell.
2. Per-leaf ImageData is 4 to 6 times smaller and 3 times faster to write and read, since
   its geometry is implicit; but it is one file per leaf and tile, 5631 for the larger
   sphere, with no documented way to tie them into one dataset.
3. One UnstructuredGrid is one file, holding tiles as larger cells, at the cost of storing
   every point and connectivity in ASCII.
4. Not measured: opening either in ParaView itself, which is not installed; thousands of
   separate files are expected to be unwieldy there, one source each.
