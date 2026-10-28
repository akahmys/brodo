//! Spike for B-005 (ROADMAP.md §3): which compressor is the archive's default, and is a
//! pure-Rust one enough (CODING.md Rule 12)?
//!
//! Each grid is frozen (docs/formats.md §2) and cut into pieces: everything before the leaf
//! values (header, root table, nodes), the leaf values in chunks of `CHUNKS` leaves, and the
//! tile values after them. Each piece is compressed on its own, the leaf chunks optionally
//! byte-shuffled first (byte k of every scalar together). Decompressing runs the pieces in
//! parallel into one aligned buffer, unshuffles, and checks it as a `GridView`, the same
//! check `CacheFile::from_bytes` makes, against which it is timed.
//!
//! Every variant's output is compared with the frozen bytes; a difference ends the run.
//!
//! Timing (TESTING.md T5): the fastest of `ROUNDS` runs, the variants in turn in each
//! round. Run the binary twice and compare.
//!
//! Output: one row a grid and variant, tab-separated, on standard output.

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
const CHUNKS: [usize; 3] = [64, 512, 4096];
const ROUNDS: usize = 5;
const COMPRESS_ROUNDS: usize = 2;

#[derive(Clone, Copy, FromBytes, IntoBytes, Immutable, KnownLayout)]
#[repr(C, align(32))]
struct Block([u8; BLOCK]);

#[derive(Clone, Copy, Debug)]
enum Codec {
    None,
    Lz4,
    Ruzstd(u8),
    Zstd(i32),
    Deflate(u8),
}

impl Codec {
    fn name(self) -> String {
        match self {
            Codec::None => "none".into(),
            Codec::Lz4 => "lz4_flex".into(),
            Codec::Ruzstd(l) => format!("ruzstd-{}", ["fastest", "default", "better", "best"][l as usize]),
            Codec::Zstd(l) => format!("zstd-c-{l}"),
            Codec::Deflate(l) => format!("miniz-{l}"),
        }
    }

    fn compress(self, data: &[u8]) -> Vec<u8> {
        use ruzstd::encoding::{CompressionLevel, compress_to_vec};
        match self {
            Codec::None => data.to_vec(),
            Codec::Lz4 => lz4_flex::block::compress(data),
            Codec::Ruzstd(l) => {
                let level = [
                    CompressionLevel::Fastest,
                    CompressionLevel::Default,
                    CompressionLevel::Better,
                    CompressionLevel::Best,
                ][l as usize];
                compress_to_vec(data, level)
            }
            Codec::Zstd(l) => zstd::bulk::compress(data, l).expect("zstd compresses"),
            Codec::Deflate(l) => miniz_oxide::deflate::compress_to_vec(data, l),
        }
    }

    fn decompress(self, src: &[u8], out: &mut [u8]) {
        let n = match self {
            Codec::None => {
                out.copy_from_slice(src);
                src.len()
            }
            Codec::Lz4 => lz4_flex::block::decompress_into(src, out).expect("lz4 decompresses"),
            Codec::Ruzstd(_) => {
                let mut d = ruzstd::decoding::FrameDecoder::new();
                d.decode_all(src, out).expect("ruzstd decompresses")
            }
            Codec::Zstd(_) => zstd::bulk::decompress_to_buffer(src, out).expect("zstd decompresses"),
            Codec::Deflate(_) => {
                use miniz_oxide::inflate::core::{DecompressorOxide, decompress, inflate_flags};
                let mut d = DecompressorOxide::new();
                let flags = inflate_flags::TINFL_FLAG_USING_NON_WRAPPING_OUTPUT_BUF;
                let (status, _, written) = decompress(&mut d, src, out, 0, flags);
                assert!(
                    matches!(status, miniz_oxide::inflate::TINFLStatus::Done),
                    "deflate decompresses: {status:?}"
                );
                written
            }
        };
        assert_eq!(n, out.len(), "{} filled its piece", self.name());
    }
}

// ruzstd 0.9.1 implements only `Fastest`; its other levels panic `unimplemented!()`.
const CODECS: [Codec; 10] = [
    Codec::None,
    Codec::Lz4,
    Codec::Ruzstd(0),
    Codec::Zstd(1),
    Codec::Zstd(3),
    Codec::Zstd(7),
    Codec::Zstd(11),
    Codec::Deflate(1),
    Codec::Deflate(6),
    Codec::Deflate(9),
];

/// Which `GridView` checks a grid's bytes.
#[derive(Clone, Copy)]
enum Kind {
    F32,
    F64x3,
}

struct Grid {
    name: &'static str,
    kind: Kind,
    /// The scalar's size, by which leaf values are shuffled.
    scalar: usize,
    bytes: Vec<u8>,
    leaf_start: usize,
    leaf_end: usize,
    leaf_bytes: usize,
}

fn word(b: &[u8], at: usize) -> usize {
    u32::from_le_bytes(b[at..at + 4].try_into().unwrap()) as usize
}

impl Grid {
    fn new(name: &'static str, kind: Kind, scalar: usize, bytes: Vec<u8>) -> Grid {
        let value_size = word(&bytes, 12);
        let leaves = word(&bytes, 16 + 3 * 4);
        let leaf_start = word(&bytes, 36 + 3 * 4) * BLOCK;
        let leaf_bytes = 512 * value_size;
        Grid { name, kind, scalar, leaf_end: leaf_start + leaves * leaf_bytes, leaf_start, leaf_bytes, bytes }
    }

    /// The pieces' ranges: before the leaves, leaf chunks, after them.
    fn pieces(&self, chunk: usize) -> Vec<(std::ops::Range<usize>, bool)> {
        let mut out = vec![(0..self.leaf_start, false)];
        let step = chunk * self.leaf_bytes;
        let mut at = self.leaf_start;
        while at < self.leaf_end {
            let end = (at + step).min(self.leaf_end);
            out.push((at..end, true));
            at = end;
        }
        out.push((self.leaf_end..self.bytes.len(), false));
        out
    }

    fn check(&self, bytes: &[u8]) {
        match self.kind {
            Kind::F32 => drop(GridView::<f32>::new(bytes).expect("a grid")),
            Kind::F64x3 => drop(GridView::<[f64; 3]>::new(bytes).expect("a grid")),
        }
    }
}

fn shuffle(data: &[u8], scalar: usize) -> Vec<u8> {
    let n = data.len() / scalar;
    let mut out = vec![0; data.len()];
    for (i, s) in data.chunks_exact(scalar).enumerate() {
        for (k, b) in s.iter().enumerate() {
            out[k * n + i] = *b;
        }
    }
    out
}

fn unshuffle(data: &[u8], scalar: usize, out: &mut [u8]) {
    let n = data.len() / scalar;
    for (i, s) in out.chunks_exact_mut(scalar).enumerate() {
        for (k, b) in s.iter_mut().enumerate() {
            *b = data[k * n + i];
        }
    }
}

struct Archive {
    codec: Codec,
    shuffled: bool,
    scalar: usize,
    pieces: Vec<(std::ops::Range<usize>, bool, Vec<u8>)>,
    total: usize,
}

fn compress(g: &Grid, codec: Codec, shuffled: bool, chunk: usize) -> Archive {
    let pieces = g
        .pieces(chunk)
        .into_iter()
        .map(|(r, leaf)| {
            let data = &g.bytes[r.clone()];
            let c = if shuffled && leaf { codec.compress(&shuffle(data, g.scalar)) } else { codec.compress(data) };
            (r, leaf, c)
        })
        .collect();
    Archive { codec, shuffled, scalar: g.scalar, pieces, total: g.bytes.len() }
}

fn decompress(a: &Archive) -> Vec<Block> {
    let mut blocks = vec![Block([0; BLOCK]); a.total / BLOCK];
    let mut rest: &mut [u8] = blocks.as_mut_bytes();
    let mut outs = Vec::with_capacity(a.pieces.len());
    for (r, _, _) in &a.pieces {
        let (head, tail) = rest.split_at_mut(r.len());
        outs.push(head);
        rest = tail;
    }
    outs.into_par_iter().zip(a.pieces.par_iter()).for_each(|(out, (_, leaf, c))| {
        if a.shuffled && *leaf {
            let mut scratch = vec![0; out.len()];
            a.codec.decompress(c, &mut scratch);
            unshuffle(&scratch, a.scalar, out);
        } else {
            a.codec.decompress(c, out);
        }
    });
    blocks
}

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

fn grids() -> Vec<Grid> {
    let ball = convert(&sphere(7, 200.0), &Options::new(1.0)).unwrap();
    let plate = convert(&slab([320.0, 320.0, 6.0]), &Options::new(1.0)).unwrap();
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
        Grid::new("sdf_sphere_r200", Kind::F32, 4, bytes(freeze(&ball.topology, &ball.field).unwrap())),
        Grid::new("sdf_slab_320", Kind::F32, 4, bytes(freeze(&plate.topology, &plate.field).unwrap())),
        Grid::new("density_sphere_r200", Kind::F32, 4, bytes(freeze(&ball.topology, &density).unwrap())),
        Grid::new("normals_sphere_r100", Kind::F64x3, 8, bytes(freeze(&small.topology, &normals).unwrap())),
    ]
}

/// Copying and checking the bytes as a cache file reads them: the reference.
fn reference(g: &Grid) -> u128 {
    let mut best = u128::MAX;
    for _ in 0..ROUNDS {
        let t = Instant::now();
        let mut blocks = vec![Block([0; BLOCK]); g.bytes.len() / BLOCK];
        blocks.as_mut_bytes().copy_from_slice(&g.bytes);
        g.check(blocks.as_bytes());
        best = best.min(t.elapsed().as_nanos());
    }
    best
}

fn main() {
    let threads = rayon::current_num_threads();
    println!("grid\tcodec\tshuffled\tchunk_leaves\tbytes\tcompressed\tratio\tcompress_ms_1t\tdecompress_ms_{threads}t\tcopy_check_ms\tdecompress_over_copy");
    for g in grids() {
        let leaves = (g.leaf_end - g.leaf_start) / g.leaf_bytes;
        eprintln!("{}: {} bytes, {leaves} leaves", g.name, g.bytes.len());
        let copy = reference(&g);
        let mut variants = Vec::new();
        for codec in CODECS {
            for shuffled in [false, true] {
                for chunk in CHUNKS {
                    if matches!(codec, Codec::None) && (shuffled || chunk != 512) {
                        continue;
                    }
                    let mut best = u128::MAX;
                    let mut archive = None;
                    for _ in 0..COMPRESS_ROUNDS {
                        let t = Instant::now();
                        let a = compress(&g, codec, shuffled, chunk);
                        best = best.min(t.elapsed().as_nanos());
                        archive = Some(a);
                    }
                    let a = archive.unwrap();
                    let back = decompress(&a);
                    assert!(back.as_bytes() == g.bytes.as_slice(), "{} {} {shuffled} {chunk} differs", g.name, codec.name());
                    variants.push((a, chunk, best, u128::MAX));
                }
            }
        }
        for _ in 0..ROUNDS {
            for v in &mut variants {
                let t = Instant::now();
                let back = decompress(&v.0);
                g.check(back.as_bytes());
                v.3 = v.3.min(t.elapsed().as_nanos());
            }
        }
        for (a, chunk, c, d) in variants {
            let size: usize = a.pieces.iter().map(|p| p.2.len()).sum();
            println!(
                "{}\t{}\t{}\t{chunk}\t{}\t{size}\t{:.4}\t{:.2}\t{:.2}\t{:.2}\t{:.2}",
                g.name,
                a.codec.name(),
                a.shuffled,
                g.bytes.len(),
                size as f64 / g.bytes.len() as f64,
                c as f64 / 1e6,
                d as f64 / 1e6,
                copy as f64 / 1e6,
                d as f64 / copy as f64,
            );
        }
    }
}
