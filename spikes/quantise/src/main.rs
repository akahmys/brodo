//! Spike for value quantisation in the archive (ROADMAP.md P6, ADR-0016): how small, how
//! wrong and how slow to read are leaf values stored as IEEE binary16, or as 8 bits over a
//! range, the range a grid's or a chunk's, with and without byte shuffling?
//!
//! Each grid is frozen (docs/formats.md §2) and cut as the archive cuts it (§8.3): the part
//! before the leaf values, chunks of 512 leaves (or `--chunk=N`), the part after. The parts
//! other than leaf chunks are compressed as they are. A leaf chunk's scalars are quantised, optionally
//! byte-shuffled, and compressed by `lz4_flex`'s block format; an 8-bit chunk also stores
//! its ranges, two `f64` a component, counted in its size. Reading decompresses the chunks
//! in parallel, unshuffles and restores the values into one aligned buffer, then checks it
//! as a `GridView`, timed against copying the exact bytes and the same check.
//!
//! Every variant's error is measured on every leaf value against the bound its encoding
//! states; one beyond it ends the run. `--exhaustive` instead checks the binary16
//! conversion on every `f32`, against its neighbours, and exits.
//!
//! `--flush` rewrites 512 MB before each timed run, outside the time, so that every run
//! starts with the grid out of the caches.
//!
//! Timing (TESTING.md T5): `ROUNDS` rounds, the yardstick and the variants in turn in each,
//! reported as the fastest and the median. Run the binary twice and compare.

use std::time::Instant;

use brodo::field::Field;
use brodo::frozen::freeze;
use brodo::topology::Topology;
use brodo::view::GridView;
use colino::convert::{Options, convert};
use colino::mesh::Mesh;
use rayon::prelude::*;
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout};

const BLOCK: usize = 32;

/// Leaves a chunk: 512, the archive's, unless `--chunk=N` says otherwise.
fn chunk() -> usize {
    static CHUNK: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    *CHUNK.get_or_init(|| {
        std::env::args().find_map(|a| a.strip_prefix("--chunk=").map(|n| n.parse().unwrap())).unwrap_or(512)
    })
}
const ROUNDS: usize = 50;

#[derive(Clone, Copy, FromBytes, IntoBytes, Immutable, KnownLayout)]
#[repr(C, align(32))]
struct Block([u8; BLOCK]);

// ---- binary16 ------------------------------------------------------------------------

/// The largest finite binary16, 65504.
const F16_MAX: f64 = 65504.0;

/// `x`, finite and within ±65504, rounded to the nearest binary16, ties to even.
fn to_f16(x: f64) -> u16 {
    let sign = if x.is_sign_negative() { 0x8000 } else { 0 };
    let a = x.abs();
    assert!(a <= F16_MAX, "{x} is beyond binary16");
    // The binade's exponent, the subnormals sharing the least normal one; its quantum.
    let e = if a < 2f64.powi(-14) { -14 } else { ((a.to_bits() >> 52) & 0x7ff) as i32 - 1023 };
    let r = (a / 2f64.powi(e - 10)).round_ties_even() as u16;
    // (e + 14) << 10 plus r: a subnormal for e = -14 and r < 1024, and r = 2048 carries
    // into the next binade.
    sign | (((e + 14) as u16) << 10).wrapping_add(r)
}

fn from_f16(h: u16) -> f64 {
    let (e, m) = (i32::from(h >> 10 & 31), f64::from(h & 1023));
    let a = match e {
        0 => m * 2f64.powi(-24),
        31 => if m == 0.0 { f64::INFINITY } else { f64::NAN },
        _ => (1024.0 + m) * 2f64.powi(e - 25),
    };
    if h & 0x8000 != 0 { -a } else { a }
}

/// Every `f32` within ±65504 converts to a binary16 no farther than either neighbour, and
/// to the even one at a tie.
fn exhaustive() {
    let bad: u64 = (0..=u32::MAX)
        .into_par_iter()
        .filter(|b| {
            let x = f64::from(f32::from_bits(*b));
            if !x.is_finite() || x.abs() > F16_MAX {
                return false;
            }
            let h = to_f16(x);
            let d = (from_f16(h) - x).abs();
            let mut wrong = false;
            for n in [h.wrapping_add(1), h.wrapping_sub(1)] {
                let y = from_f16(n);
                // Neighbours by magnitude; across zero, -0 and +0 are one value.
                if (n & 0x7fff) == 0x7c00 || n & 0x7fff > 0x7c00 || (n ^ h) & 0x8000 != 0 {
                    continue;
                }
                let dn = (y - x).abs();
                wrong |= dn < d || (dn == d && h & 1 == 1);
            }
            wrong || ((h & 0x8000 != 0) != x.is_sign_negative())
        })
        .count() as u64;
    println!("binary16 conversion: {bad} of the f32 within range wrong");
    assert_eq!(bad, 0);
}

// ---- grids -----------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    F32,
    F64x3,
}

struct Grid {
    name: &'static str,
    kind: Kind,
    bytes: Vec<u8>,
    leaf_start: usize,
    leaf_end: usize,
}

fn word(b: &[u8], at: usize) -> usize {
    u32::from_le_bytes(b[at..at + 4].try_into().unwrap()) as usize
}

impl Grid {
    fn new(name: &'static str, kind: Kind, bytes: Vec<u8>) -> Grid {
        let leaves = word(&bytes, 16 + 3 * 4);
        let leaf_start = word(&bytes, 36 + 3 * 4) * BLOCK;
        let leaf_end = leaf_start + leaves * 512 * word(&bytes, 12);
        Grid { name, kind, bytes, leaf_start, leaf_end }
    }

    /// Bytes a scalar, and scalars a value.
    fn scalar(&self) -> (usize, usize) {
        match self.kind {
            Kind::F32 => (4, 1),
            Kind::F64x3 => (8, 3),
        }
    }

    fn leaf_bytes(&self) -> usize {
        let (s, c) = self.scalar();
        512 * s * c
    }

    /// The parts: before the leaves, leaf chunks (marked), after them.
    fn parts(&self) -> Vec<(std::ops::Range<usize>, bool)> {
        let mut out = vec![(0..self.leaf_start, false)];
        let step = chunk() * self.leaf_bytes();
        let mut at = self.leaf_start;
        while at < self.leaf_end {
            let end = (at + step).min(self.leaf_end);
            out.push((at..end, true));
            at = end;
        }
        out.push((self.leaf_end..self.bytes.len(), false));
        out.retain(|(r, _)| !r.is_empty());
        out
    }

    fn scalars(&self, r: std::ops::Range<usize>) -> Vec<f64> {
        let b = &self.bytes[r];
        match self.kind {
            Kind::F32 => b.chunks_exact(4).map(|c| f64::from(f32::from_le_bytes(c.try_into().unwrap()))).collect(),
            Kind::F64x3 => b.chunks_exact(8).map(|c| f64::from_le_bytes(c.try_into().unwrap())).collect(),
        }
    }

    /// The error a stored scalar adds beyond its encoding's: `f32`'s half ulp at `x`.
    fn storage_error(&self, x: f64) -> f64 {
        match self.kind {
            Kind::F32 => f64::from(f32::EPSILON) / 2.0 * x.abs(),
            Kind::F64x3 => 0.0,
        }
    }

    fn check(&self, bytes: &[u8]) {
        match self.kind {
            Kind::F32 => drop(GridView::<f32>::new(bytes).expect("a grid")),
            Kind::F64x3 => drop(GridView::<[f64; 3]>::new(bytes).expect("a grid")),
        }
    }
}

// ---- meshes, as spikes/archive/ builds them ------------------------------------------

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


// ---- encodings -------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Debug)]
enum Encoding {
    /// The exact bytes, unshuffled: the archive as built.
    Exact,
    F16 { shuffled: bool },
    /// 8 bits over each component's range, the grid's or each chunk's.
    U8 { per_chunk: bool },
}

impl Encoding {
    fn name(self) -> String {
        match self {
            Encoding::Exact => "exact".into(),
            Encoding::F16 { shuffled } => format!("f16{}", if shuffled { "-shuffled" } else { "" }),
            Encoding::U8 { per_chunk } => format!("u8-{}-range", if per_chunk { "chunk" } else { "grid" }),
        }
    }
}

const ENCODINGS: [Encoding; 5] = [
    Encoding::Exact,
    Encoding::F16 { shuffled: false },
    Encoding::F16 { shuffled: true },
    Encoding::U8 { per_chunk: false },
    Encoding::U8 { per_chunk: true },
];

/// Each component's least and greatest scalar in `xs`.
fn ranges(xs: &[f64], components: usize) -> Vec<(f64, f64)> {
    let mut r = vec![(f64::INFINITY, f64::NEG_INFINITY); components];
    for (i, x) in xs.iter().enumerate() {
        let c = &mut r[i % components];
        *c = (c.0.min(*x), c.1.max(*x));
    }
    r
}

fn to_u8(x: f64, (lo, hi): (f64, f64)) -> u8 {
    if hi == lo { 0 } else { ((x - lo) / (hi - lo) * 255.0).round() as u8 }
}

fn from_u8(q: u8, (lo, hi): (f64, f64)) -> f64 {
    lo + f64::from(q) * ((hi - lo) / 255.0)
}

/// A part as stored: its compressed bytes, the ranges an 8-bit chunk keeps, and whether it
/// holds leaf values.
struct Stored {
    bytes: Vec<u8>,
    ranges: Vec<(f64, f64)>,
    leaf: bool,
    raw: std::ops::Range<usize>,
}

struct Archive {
    encoding: Encoding,
    parts: Vec<Stored>,
}

impl Archive {
    fn size(&self) -> usize {
        self.parts.iter().map(|p| p.bytes.len() + 16 * p.ranges.len()).sum()
    }
}

fn shuffle(data: &[u8], scalar: usize) -> Vec<u8> {
    let n = data.len() / scalar;
    let mut out = vec![0; data.len()];
    for (i, v) in data.chunks_exact(scalar).enumerate() {
        for (k, b) in v.iter().enumerate() {
            out[k * n + i] = *b;
        }
    }
    out
}

fn encode(g: &Grid, e: Encoding) -> Archive {
    let (_, components) = g.scalar();
    let grid_range = ranges(&g.scalars(g.leaf_start..g.leaf_end), components);
    let parts = g
        .parts()
        .into_par_iter()
        .map(|(r, leaf)| {
            if !leaf || e == Encoding::Exact {
                let bytes = lz4_flex::block::compress(&g.bytes[r.clone()]);
                return Stored { bytes, ranges: Vec::new(), leaf, raw: r };
            }
            let xs = g.scalars(r.clone());
            let (plain, ranges) = match e {
                Encoding::F16 { shuffled } => {
                    let b: Vec<u8> = xs.iter().flat_map(|x| to_f16(*x).to_le_bytes()).collect();
                    (if shuffled { shuffle(&b, 2) } else { b }, Vec::new())
                }
                Encoding::U8 { per_chunk } => {
                    let rs = if per_chunk { ranges(&xs, components) } else { grid_range.clone() };
                    let b = xs.iter().enumerate().map(|(i, x)| to_u8(*x, rs[i % components])).collect();
                    (b, if per_chunk { rs } else { Vec::new() })
                }
                Encoding::Exact => unreachable!(),
            };
            Stored { bytes: lz4_flex::block::compress(&plain), ranges, leaf, raw: r }
        })
        .collect();
    let mut a = Archive { encoding: e, parts };
    // The grid's ranges, once, counted with the first part.
    if matches!(e, Encoding::U8 { per_chunk: false }) {
        a.parts[0].ranges = grid_range;
    }
    a
}

/// Every binary16's value as `f32` and as `f64`: built once a process, as a reader would
/// keep them, outside the timing.
struct Tables {
    f32: Vec<f32>,
    f64: Vec<f64>,
}

fn tables() -> &'static Tables {
    static T: std::sync::OnceLock<Tables> = std::sync::OnceLock::new();
    T.get_or_init(|| {
        let f64: Vec<f64> = (0..=u16::MAX).map(from_f16).collect();
        Tables { f32: f64.iter().map(|x| *x as f32).collect(), f64 }
    })
}

/// Writes the value of each stored scalar of `s`, `width` bytes each,
/// planes apart when `shuffled`, to `out`, a component at a time in turn,
/// by table: `f16` for binary16, `lut` a component's 256 levels for 8 bits.
fn restore<T: Copy>(out: &mut [T], s: &[u8], shuffled: bool, f16: Option<&[T]>, lut: &[[T; 256]]) {
    let n = out.len();
    match f16 {
        Some(t) if shuffled => {
            let (lo, hi) = s.split_at(n);
            for ((o, l), h) in out.iter_mut().zip(lo).zip(hi) {
                *o = t[usize::from(*l) | usize::from(*h) << 8];
            }
        }
        Some(t) => {
            for (o, c) in out.iter_mut().zip(s.chunks_exact(2)) {
                *o = t[usize::from(u16::from_le_bytes([c[0], c[1]]))];
            }
        }
        None if lut.len() == 1 => {
            let l = &lut[0];
            for (o, q) in out.iter_mut().zip(s) {
                *o = l[usize::from(*q)];
            }
        }
        None => {
            for (o, q) in out.chunks_exact_mut(3).zip(s.chunks_exact(3)) {
                for k in 0..3 {
                    o[k] = lut[k][usize::from(q[k])];
                }
            }
        }
    }
}

fn decode(g: &Grid, a: &Archive) -> Vec<Block> {
    let (scalar, _) = g.scalar();
    let grid_range = &a.parts[0].ranges;
    let t = tables();
    let mut blocks = vec![Block([0; BLOCK]); g.bytes.len() / BLOCK];
    let mut rest = blocks.as_mut_bytes();
    let mut rooms = Vec::new();
    for p in &a.parts {
        let (room, next) = rest.split_at_mut(p.raw.len());
        rooms.push((p, room));
        rest = next;
    }
    rooms.into_par_iter().for_each_init(Vec::new, |scratch: &mut Vec<u8>, (p, room)| {
        if !p.leaf || a.encoding == Encoding::Exact {
            assert_eq!(lz4_flex::block::decompress_into(&p.bytes, room).unwrap(), room.len());
            return;
        }
        let n = room.len() / scalar;
        let (width, shuffled, rs) = match a.encoding {
            Encoding::F16 { shuffled } => (2, shuffled, &p.ranges),
            Encoding::U8 { per_chunk } => (1, false, if per_chunk { &p.ranges } else { grid_range }),
            Encoding::Exact => unreachable!(),
        };
        scratch.resize(n * width, 0);
        assert_eq!(lz4_flex::block::decompress_into(&p.bytes, scratch).unwrap(), n * width);
        let f16 = width == 2;
        match g.kind {
            Kind::F32 => {
                let lut: Vec<[f32; 256]> =
                    rs.iter().map(|r| std::array::from_fn(|q| from_u8(q as u8, *r) as f32)).collect();
                let out = <[f32]>::mut_from_bytes(room).unwrap();
                restore(out, scratch, shuffled, f16.then_some(&t.f32[..]), &lut);
            }
            Kind::F64x3 => {
                let lut: Vec<[f64; 256]> =
                    rs.iter().map(|r| std::array::from_fn(|q| from_u8(q as u8, *r))).collect();
                let out = <[f64]>::mut_from_bytes(room).unwrap();
                restore(out, scratch, shuffled, f16.then_some(&t.f64[..]), &lut);
            }
        }
    });
    blocks
}

/// The largest error over every leaf scalar, absolute and relative to the scalar, and the
/// largest error over its bound; the parts outside the leaves must come back exact.
fn errors(g: &Grid, a: &Archive, back: &[u8]) -> (f64, f64, f64) {
    let (_, components) = g.scalar();
    let grid_range = a.parts[0].ranges.clone();
    let (mut abs, mut rel, mut over) = (0.0_f64, 0.0_f64, 0.0_f64);
    for p in &a.parts {
        if !p.leaf {
            assert!(back[p.raw.clone()] == g.bytes[p.raw.clone()], "{}: a part outside the leaves changed", g.name);
            continue;
        }
        let xs = g.scalars(p.raw.clone());
        let ys: Vec<f64> = match g.kind {
            Kind::F32 => back[p.raw.clone()].chunks_exact(4).map(|c| f64::from(f32::from_le_bytes(c.try_into().unwrap()))).collect(),
            Kind::F64x3 => back[p.raw.clone()].chunks_exact(8).map(|c| f64::from_le_bytes(c.try_into().unwrap())).collect(),
        };
        for (i, (x, y)) in xs.iter().zip(&ys).enumerate() {
            let err = (y - x).abs();
            let bound = match a.encoding {
                Encoding::Exact => 0.0,
                Encoding::F16 { .. } => x.abs() * 2f64.powi(-11) + 2f64.powi(-25),
                Encoding::U8 { per_chunk } => {
                    let (lo, hi) = if per_chunk { p.ranges[i % components] } else { grid_range[i % components] };
                    (hi - lo) / 510.0 * (1.0 + 1e-12)
                }
            } + g.storage_error(*y);
            abs = abs.max(err);
            if *x != 0.0 {
                rel = rel.max(err / x.abs());
            }
            if err > bound {
                panic!("{} {}: error {err} at {x} beyond its bound {bound}", g.name, a.encoding.name());
            }
            if bound > 0.0 {
                over = over.max(err / bound);
            }
        }
    }
    (abs, rel, over)
}

fn grids() -> Vec<Grid> {
    let ball = convert(&sphere(7, 200.0), &Options::new(1.0)).unwrap();
    let plate = convert(&slab([320.0, 320.0, 6.0]), &Options::new(1.0)).unwrap();
    let wide = convert(&slab([640.0, 640.0, 6.0]), &Options::new(1.0)).unwrap();
    let small = convert(&sphere(7, 100.0), &Options::new(1.0)).unwrap();
    let density = field(&ball.topology, 0.0_f32, |c| {
        let r2: f64 = c.iter().map(|x| x * x).sum();
        (-r2 / (2.0 * 100.0 * 100.0)).exp() as f32
    });
    let normals = field(&small.topology, [0.0_f64; 3], |c| {
        let n = c.iter().map(|x| x * x).sum::<f64>().sqrt();
        c.map(|x| x / n)
    });
    let bytes = |g: brodo::frozen::FrozenGrid| g.as_bytes().to_vec();
    vec![
        Grid::new("sdf_sphere_r200", Kind::F32, bytes(freeze(&ball.topology, &ball.field).unwrap())),
        Grid::new("sdf_slab_320", Kind::F32, bytes(freeze(&plate.topology, &plate.field).unwrap())),
        Grid::new("sdf_slab_640", Kind::F32, bytes(freeze(&wide.topology, &wide.field).unwrap())),
        Grid::new("density_sphere_r200", Kind::F32, bytes(freeze(&ball.topology, &density).unwrap())),
        Grid::new("normals_sphere_r100", Kind::F64x3, bytes(freeze(&small.topology, &normals).unwrap())),
    ]
}

/// Copying and checking the exact bytes as a cache file reads them: the yardstick.
fn reference(g: &Grid) -> u128 {
    let t = Instant::now();
    let mut blocks = vec![Block([0; BLOCK]); g.bytes.len() / BLOCK];
    blocks.as_mut_bytes().copy_from_slice(&g.bytes);
    g.check(blocks.as_bytes());
    t.elapsed().as_nanos()
}

/// Rewrites 512 MB on every thread, when `--flush` is given, to push a grid out of the caches.
fn flush(buffer: &mut [u64], round: u64) {
    if !buffer.is_empty() {
        buffer.par_chunks_mut(1 << 16).for_each(|c| c.iter_mut().for_each(|x| *x = x.wrapping_add(round)));
        std::hint::black_box(&*buffer);
    }
}

/// The fastest and the median of `xs`, in milliseconds.
fn summary(xs: &mut [u128]) -> (f64, f64) {
    xs.sort_unstable();
    (xs[0] as f64 / 1e6, xs[xs.len() / 2] as f64 / 1e6)
}

fn main() {
    if std::env::args().any(|a| a == "--exhaustive") {
        exhaustive();
        return;
    }
    let threads = rayon::current_num_threads();
    let mut buffer = if std::env::args().any(|a| a == "--flush") { vec![1_u64; 1 << 26] } else { Vec::new() };
    println!("grid\tencoding\tbytes\tstored\tratio\tmax_abs_error\tmax_rel_error\terror_over_bound\tencode_ms_{threads}t\tdecode_ms_{threads}t\tcopy_check_ms\tdecode_over_copy\tdecode_median_ms\tcopy_median_ms\tmedian_over_median");
    for g in grids() {
        eprintln!("{}: {} bytes, leaves {}..{}, {} leaves a chunk", g.name, g.bytes.len(), g.leaf_start, g.leaf_end, chunk());
        let mut copy = Vec::with_capacity(ROUNDS);
        tables();
        let mut variants = Vec::new();
        for e in ENCODINGS {
            let t = Instant::now();
            let a = encode(&g, e);
            let enc = t.elapsed().as_nanos();
            let back = decode(&g, &a);
            g.check(back.as_bytes());
            let err = errors(&g, &a, back.as_bytes());
            if e == Encoding::Exact {
                assert!(back.as_bytes() == g.bytes.as_slice(), "{}: exact differs", g.name);
            }
            variants.push((a, enc, Vec::with_capacity(ROUNDS), err));
        }
        for round in 0..ROUNDS as u64 {
            flush(&mut buffer, round);
            copy.push(reference(&g));
            for v in &mut variants {
                flush(&mut buffer, round);
                let t = Instant::now();
                let back = decode(&g, &v.0);
                g.check(back.as_bytes());
                v.2.push(t.elapsed().as_nanos());
            }
        }
        let (copy, copy_median) = summary(&mut copy);
        for (a, enc, mut dec, (abs, rel, over)) in variants {
            let (dec, dec_median) = summary(&mut dec);
            println!(
                "{}\t{}\t{}\t{}\t{:.4}\t{abs:.3e}\t{rel:.3e}\t{over:.3}\t{:.2}\t{dec:.3}\t{copy:.3}\t{:.2}\t{dec_median:.3}\t{copy_median:.3}\t{:.2}",
                g.name,
                a.encoding.name(),
                g.bytes.len(),
                a.size(),
                a.size() as f64 / g.bytes.len() as f64,
                enc as f64 / 1e6,
                dec / copy,
                dec_median / copy_median,
            );
        }
    }
}
