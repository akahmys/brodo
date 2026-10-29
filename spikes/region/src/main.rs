//! Spike for reading a region of an archive (ROADMAP.md P6, ADR-0016): how much of a grid
//! does a box of voxels need decompressed, chunks being runs of leaves in Morton order,
//! and how much faster is decompressing only those than the whole grid?
//!
//! Each grid is frozen (docs/formats.md §2) and cut as the archive cuts it (§8.3), here
//! with 64, 128 or 512 leaves a chunk, each chunk compressed exact by `lz4_flex`'s block
//! format. A box whose side is a fraction of the grid's leaf bounds along each axis is
//! placed at the low end, the middle and the high end of each axis, 27 places; a leaf
//! chunk is needed if a leaf in it meets the box. The first chunk, header and nodes, and
//! the last, the tiles, are always needed.
//!
//! Timing (TESTING.md T5), the box at the place needing the most leaf chunks: decompressing the needed chunks in
//! parallel, each into a buffer of its own, against decompressing every chunk into one
//! zeroed buffer and checking it as a `GridView`, as the archive's reader does; `--parts`
//! times that reader's fixed costs instead; `ROUNDS` rounds, each run after 512 MB are rewritten
//! on every thread, outside the time. Run the binary twice and compare.

use std::time::Instant;

use brodo::field::Field;
use brodo::frozen::freeze;
use brodo::topology::Topology;
use brodo::view::GridView;
use colino::convert::{Options, convert};
use colino::mesh::Mesh;
use rayon::prelude::*;
use zerocopy::{FromBytes, FromZeros, Immutable, IntoBytes, KnownLayout};

const BLOCK: usize = 32;
const ROUNDS: usize = 30;
const SIDES: [f64; 3] = [0.1, 0.2, 0.5];
const CHUNKS: [usize; 3] = [64, 128, 512];

#[derive(Clone, Copy, FromBytes, IntoBytes, Immutable, KnownLayout)]
#[repr(C, align(32))]
struct Block([u8; BLOCK]);

struct Grid {
    name: &'static str,
    bytes: Vec<u8>,
    /// Each leaf's origin, by leaf number, the order of the leaf values section.
    origins: Vec<[i32; 3]>,
    leaf_start: usize,
    leaf_bytes: usize,
}

fn word(b: &[u8], at: usize) -> usize {
    u32::from_le_bytes(b[at..at + 4].try_into().unwrap()) as usize
}

impl Grid {
    fn new(name: &'static str, t: &Topology, f: &Field<f32>) -> Self {
        let bytes = freeze(t, f).unwrap().as_bytes().to_vec();
        let origins: Vec<_> = t.leaf_numbers().map(|n| t.leaf_origin(n).unwrap()).collect();
        let view = GridView::<f32>::new(&bytes).unwrap();
        // Leaf numbers are the buffer's order.
        for n in t.leaf_numbers().step_by(97) {
            assert!(view.leaf(n.get()).unwrap().iter().zip(f.leaf(t, n).unwrap()).all(|(a, b)| a.to_bits() == b.to_bits()));
        }
        let leaf_start = word(&bytes, 48) * BLOCK;
        assert_eq!(word(&bytes, 28), origins.len());
        Grid { name, bytes, origins, leaf_start, leaf_bytes: 512 * 4 }
    }

    /// Byte ranges of the chunks: before the leaves, every `chunk` leaves, after them.
    fn chunks(&self, chunk: usize) -> Vec<std::ops::Range<usize>> {
        let end = self.leaf_start + self.origins.len() * self.leaf_bytes;
        let mut marks = vec![0];
        marks.extend((self.leaf_start..end).step_by(chunk * self.leaf_bytes));
        marks.extend([end, self.bytes.len()]);
        marks.windows(2).filter(|w| w[1] > w[0]).map(|w| w[0]..w[1]).collect()
    }

    /// The leaf bounds, least and greatest voxel.
    fn bounds(&self) -> ([i32; 3], [i32; 3]) {
        let mut lo = [i32::MAX; 3];
        let mut hi = [i32::MIN; 3];
        for o in &self.origins {
            for k in 0..3 {
                lo[k] = lo[k].min(o[k]);
                hi[k] = hi[k].max(o[k] + 7);
            }
        }
        (lo, hi)
    }

    /// Which leaf chunks of `chunk` leaves a box `lo..=hi` needs.
    fn needed(&self, chunk: usize, lo: [i32; 3], hi: [i32; 3]) -> Vec<bool> {
        let mut need = vec![false; self.origins.len().div_ceil(chunk)];
        for (n, o) in self.origins.iter().enumerate() {
            if (0..3).all(|k| o[k] + 7 >= lo[k] && o[k] <= hi[k]) {
                need[n / chunk] = true;
            }
        }
        need
    }
}

/// The box of side `side` of the bounds along each axis, at place `p` (0 low, 1 middle, 2 high).
fn region(g: &Grid, side: f64, p: [usize; 3]) -> ([i32; 3], [i32; 3]) {
    let (lo, hi) = g.bounds();
    let mut a = [0; 3];
    let mut b = [0; 3];
    for k in 0..3 {
        let span = f64::from(hi[k] - lo[k] + 1);
        let len = (span * side).round().max(1.0);
        let start = f64::from(lo[k]) + (span - len) * p[k] as f64 / 2.0;
        a[k] = start.round() as i32;
        b[k] = a[k] + len as i32 - 1;
    }
    (a, b)
}

/// Rewrites 512 MB on every thread, to push the grid out of the caches.
fn flush(buffer: &mut [u64], round: u64) {
    buffer.par_chunks_mut(1 << 16).for_each(|c| c.iter_mut().for_each(|x| *x = x.wrapping_add(round)));
    std::hint::black_box(&*buffer);
}

/// Decompresses the chunks `which` marks into a zeroed buffer and checks it as a view.
fn decompress(g: &Grid, ranges: &[std::ops::Range<usize>], packed: &[Vec<u8>], which: &[bool]) -> u128 {
    let t = Instant::now();
    let mut out = Block::new_vec_zeroed(g.bytes.len() / BLOCK).unwrap();
    let mut rooms = Vec::new();
    let mut rest = out.as_mut_bytes();
    for r in ranges {
        let (room, after) = rest.split_at_mut(r.len());
        rooms.push(room);
        rest = after;
    }
    rooms
        .into_par_iter()
        .zip(packed)
        .zip(which)
        .filter(|(_, w)| **w)
        .for_each(|((room, c), _)| assert_eq!(lz4_flex::block::decompress_into(c, room).unwrap(), room.len()));
    std::hint::black_box(GridView::<f32>::new(out.as_bytes()).expect("a grid"));
    t.elapsed().as_nanos()
}

/// Decompresses only the chunks `which` marks, each into a buffer of its own, and checks
/// a sample of the voxels they hold against the grid: the time, and the bytes held.
fn sparse(g: &Grid, ranges: &[std::ops::Range<usize>], packed: &[Vec<u8>], which: &[bool], check: bool) -> (u128, usize) {
    let t = Instant::now();
    let held: Vec<(usize, Vec<Block>)> = ranges
        .par_iter()
        .zip(packed)
        .zip(which)
        .enumerate()
        .filter(|(_, (_, w))| **w)
        .map(|(k, ((r, c), _))| {
            let mut out = Block::new_vec_zeroed(r.len() / BLOCK).unwrap();
            assert_eq!(lz4_flex::block::decompress_into(c, out.as_mut_bytes()).unwrap(), r.len());
            (k, out)
        })
        .collect();
    let time = t.elapsed().as_nanos();
    if check {
        for (k, b) in &held {
            let r = &ranges[*k];
            for at in (0..r.len()).step_by(4093) {
                assert_eq!(b.as_bytes()[at], g.bytes[r.start + at]);
            }
        }
    }
    (time, held.iter().map(|(_, b)| b.len() * BLOCK).sum())
}

fn summary(xs: &mut [u128]) -> (f64, f64) {
    xs.sort_unstable();
    (xs[0] as f64 / 1e6, xs[xs.len() / 2] as f64 / 1e6)
}

/// The fixed costs of a partial read, fastest of `ROUNDS`, caches flushed: a zeroed buffer
/// of the grid's size made and dropped; the same with every page touched; the first chunk
/// decompressed into it; and the check as a view of a whole buffer.
fn parts(buffer: &mut [u64]) {
    println!("grid\tzeroed_ms\ttouched_ms\tfirst_chunk_ms\tview_check_ms");
    for g in grids() {
        let ranges = g.chunks(512);
        let first = lz4_flex::block::compress(&g.bytes[ranges[0].clone()]);
        let mut best = [u128::MAX; 4];
        for round in 0..ROUNDS as u64 {
            let mut time = |k: usize, f: &mut dyn FnMut()| {
                flush(buffer, round);
                let t = Instant::now();
                f();
                best[k] = best[k].min(t.elapsed().as_nanos());
            };
            time(0, &mut || drop(std::hint::black_box(Block::new_vec_zeroed(g.bytes.len() / BLOCK).unwrap())));
            time(1, &mut || {
                let mut v = vec![Block([0; BLOCK]); g.bytes.len() / BLOCK];
                v.as_mut_bytes().iter_mut().step_by(16384).for_each(|b| *b = 1);
                std::hint::black_box(&v);
            });
            time(2, &mut || {
                let mut v = vec![Block([0; BLOCK]); g.bytes.len() / BLOCK];
                let room = &mut v.as_mut_bytes()[..ranges[0].len()];
                lz4_flex::block::decompress_into(&first, room).unwrap();
                std::hint::black_box(&v);
            });
            time(3, &mut || {
                std::hint::black_box(GridView::<f32>::new(&g.bytes).unwrap());
            });
        }
        let ms = best.map(|x| x as f64 / 1e6);
        println!("{}\t{:.3}\t{:.3}\t{:.3}\t{:.3}", g.name, ms[0], ms[1], ms[2], ms[3]);
    }
}

fn main() {
    let mut buffer = vec![1_u64; 1 << 26];
    if std::env::args().any(|a| a == "--parts") {
        parts(&mut buffer);
        return;
    }
    println!("grid\tleaves\tchunk_leaves\tside\tleaf_chunks\tneeded_mean\tneeded_max\tbytes_mean\tbytes_max\theld_timed\tpartial_ms\tfull_ms\tpartial_over_full\tpartial_median_ms\tfull_median_ms");
    for g in grids() {
        eprintln!("{}: {} bytes, {} leaves", g.name, g.bytes.len(), g.origins.len());
        for chunk in CHUNKS {
            let ranges = g.chunks(chunk);
            let packed: Vec<Vec<u8>> = ranges.par_iter().map(|r| lz4_flex::block::compress(&g.bytes[r.clone()])).collect();
            let stored: usize = packed.iter().map(Vec::len).sum();
            let leaf_chunks = g.origins.len().div_ceil(chunk);
            let tiles = ranges.len() - 1 - leaf_chunks;
            for side in SIDES {
                let (mut sum, mut max, mut bsum, mut bmax) = (0.0, 0.0_f64, 0.0, 0.0_f64);
                // The place needing the most leaf chunks, the last of equals, is timed.
                let (mut centre, mut most) = (Vec::new(), 0.0);
                for p in 0..27 {
                    let at = [p % 3, p / 3 % 3, p / 9];
                    let (lo, hi) = region(&g, side, at);
                    let need = g.needed(chunk, lo, hi);
                    let mut which = vec![true];
                    which.extend(&need);
                    which.extend(std::iter::repeat_n(true, tiles));
                    let share = need.iter().filter(|x| **x).count() as f64 / leaf_chunks.max(1) as f64;
                    let bytes: usize = packed.iter().zip(&which).filter(|(_, w)| **w).map(|(c, _)| c.len()).sum();
                    let bshare = bytes as f64 / stored as f64;
                    sum += share;
                    max = max.max(share);
                    bsum += bshare;
                    bmax = bmax.max(bshare);
                    if share >= most {
                        most = share;
                        centre = which;
                    }
                }
                let (mut p, mut f) = (Vec::new(), Vec::new());
                let all = vec![true; ranges.len()];
                let (_, held) = sparse(&g, &ranges, &packed, &centre, true);
                for round in 0..ROUNDS as u64 {
                    flush(&mut buffer, round);
                    p.push(sparse(&g, &ranges, &packed, &centre, false).0);
                    flush(&mut buffer, round);
                    f.push(decompress(&g, &ranges, &packed, &all));
                }
                let held = held as f64 / g.bytes.len() as f64;
                let ((pf, pm), (ff, fm)) = (summary(&mut p), summary(&mut f));
                println!(
                    "{}\t{}\t{chunk}\t{side}\t{leaf_chunks}\t{:.3}\t{:.3}\t{:.3}\t{:.3}\t{held:.3}\t{pf:.3}\t{ff:.3}\t{:.3}\t{pm:.3}\t{fm:.3}",
                    g.name,
                    g.origins.len(),
                    sum / 27.0,
                    max,
                    bsum / 27.0,
                    bmax,
                    if ff > 0.0 { pf / ff } else { 0.0 },
                );
            }
        }
    }
}

fn grids() -> Vec<Grid> {
    let ball = convert(&sphere(7, 200.0), &Options::new(1.0)).unwrap();
    let wide = convert(&slab([640.0, 640.0, 6.0]), &Options::new(1.0)).unwrap();
    let block = cube(256);
    let density = field(&block, 0.0_f32, |c| (-(c[0] * c[0] + c[1] * c[1] + c[2] * c[2]) / 20000.0).exp() as f32);
    let mut out = vec![
        Grid::new("sdf_sphere_r200", &ball.topology, &ball.field),
        Grid::new("sdf_slab_640", &wide.topology, &wide.field),
        Grid::new("density_block_256", &block, &density),
    ];
    if std::env::args().any(|a| a == "--big") {
        let big = cube(512);
        let f = field(&big, 0.0_f32, |c| (-(c[0] * c[0] + c[1] * c[1] + c[2] * c[2]) / 80000.0).exp() as f32);
        out.push(Grid::new("density_block_512", &big, &f));
    }
    out
}

/// Every leaf of a cube `edge` voxels on a side, active.
fn cube(edge: i32) -> Topology {
    let empty = Topology::new();
    let mut e = empty.edit();
    for x in (0..edge).step_by(8) {
        for y in (0..edge).step_by(8) {
            for z in (0..edge).step_by(8) {
                e.activate([x, y, z]).unwrap();
            }
        }
    }
    e.commit().unwrap().0
}

// ---- meshes and fields, as spikes/quantise/ builds them --------------------------------

/// The octahedron split `levels` times onto a sphere of radius `r`, off the lattice.
fn sphere(levels: u32, r: f64) -> Mesh {
    let mut vertices = vec![
        [1.0, 0.0, 0.0],
        [-1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, -1.0, 0.0],
        [0.0, 0.0, 1.0],
        [0.0, 0.0, -1.0],
    ];
    let mut triangles: Vec<[u32; 3]> = vec![
        [0, 2, 4],
        [2, 1, 4],
        [1, 3, 4],
        [3, 0, 4],
        [2, 0, 5],
        [1, 2, 5],
        [3, 1, 5],
        [0, 3, 5],
    ];
    for _ in 0..levels {
        let mut mids = std::collections::BTreeMap::new();
        let mut next = Vec::with_capacity(triangles.len() * 4);
        for [a, b, c] in triangles {
            let mut mid = |u: u32, v: u32| {
                *mids.entry((u.min(v), u.max(v))).or_insert_with(|| {
                    let (p, q): ([f64; 3], [f64; 3]) = (vertices[u as usize], vertices[v as usize]);
                    let s = [0, 1, 2].map(|k| (p[k] + q[k]) / 2.0);
                    let n = s.iter().map(|x| x * x).sum::<f64>().sqrt();
                    vertices.push(s.map(|x| x / n));
                    (vertices.len() - 1) as u32
                })
            };
            let (ab, bc, ca) = (mid(a, b), mid(b, c), mid(c, a));
            next.extend([[a, ab, ca], [ab, b, bc], [ca, bc, c], [ab, bc, ca]]);
        }
        triangles = next;
    }
    let vertices = vertices.into_iter().map(|v| v.map(|x| x * r + 0.3)).collect();
    Mesh::new(vertices, triangles).with_unit(Some("millimeter".into()))
}

/// A box `size` voxels on a side, facing out, turned about x and then z, off the lattice.
fn slab(size: [f64; 3]) -> Mesh {
    let (a, b) = (0.5_f64, 0.3_f64);
    let turn = |corner: [f64; 3]| {
        let [x, y, z] = [0, 1, 2].map(|k| (corner[k] - 0.5) * size[k]);
        let (y, z) = (y * a.cos() - z * a.sin(), y * a.sin() + z * a.cos());
        let (x, y) = (x * b.cos() - y * b.sin(), x * b.sin() + y * b.cos());
        [x + 0.3, y + 0.1, z - 0.2]
    };
    let vertices = (0..8).map(|i| turn([0, 1, 2].map(|k| f64::from(i >> k & 1)))).collect();
    let triangles = vec![
        [0, 2, 3],
        [0, 3, 1],
        [4, 5, 7],
        [4, 7, 6],
        [0, 1, 5],
        [0, 5, 4],
        [2, 6, 7],
        [2, 7, 3],
        [0, 4, 6],
        [0, 6, 2],
        [1, 3, 7],
        [1, 7, 5],
    ];
    Mesh::new(vertices, triangles).with_unit(Some("millimeter".into()))
}

/// A field on `t` whose leaf voxels take `value` of their centre.
fn field<T: brodo::field::Value>(t: &Topology, background: T, value: impl Fn([f64; 3]) -> T) -> Field<T> {
    let mut f = Field::new(t, background);
    for n in t.leaf_numbers() {
        let voxels: Vec<_> = t.leaf_voxels(n).unwrap().collect();
        for (v, c) in f.leaf_mut(t, n).unwrap().iter_mut().zip(voxels) {
            *v = value(c.map(|x| f64::from(x) + 0.5));
        }
    }
    f
}


