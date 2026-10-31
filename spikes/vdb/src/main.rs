//! Spike for P7's writing step (ADR-0045): a `.vdb` writer for brodo's float grids, learnt
//! from OpenVDB at `v14.0.0` (ADR-0044; the files read are in PROVENANCE.md), checked by
//! OpenVDB's own library (`oracle/`).
//!
//! `vdb-spike write OUT CASE MODE` writes brodo's grid CASE (`sphere20`, `sphere80`) to OUT
//! with MODE `none` (compression flags 0) or `mask` (active-mask, flags 2), and beside it
//! OUT.tsv: brodo's own list of its active boxes and values, in the oracle's form.

use std::collections::BTreeMap;
use std::io::{Cursor, Seek, SeekFrom, Write};

use brodo::field::{Field, Value};
use brodo::meta::GridMeta;
use brodo::parallel::par_update;
use brodo::topology::Topology;
use brodo::tree::{Coord, TileLevel};

type Failure = Box<dyn std::error::Error + Send + Sync>;

const BAND: f32 = 3.0;
const MAGIC: u32 = 0x5644_4220;
const FILE_VERSION: u32 = 225;
/// The library version a file claims; OpenVDB's reader stores it and checks nothing.
const LIBRARY: (u32, u32) = (14, 0);

// ---- brodo's grid ----

/// A shape's centre and the scale of each axis: a sphere, or an ellipsoid off the origin
/// whose values change when two axes are swapped, so that a swap is seen.
#[derive(Clone, Copy)]
struct Shape {
    r: f64,
    centre: [f64; 3],
    scale: [f64; 3],
}

fn distance(v: Coord, s: Shape) -> f32 {
    let p: Vec<f64> = (0..3).map(|k| (f64::from(v[k]) - s.centre[k]) * s.scale[k]).collect();
    ((p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt() - s.r) as f32
}

/// The bench's sphere: a leaf where the band passes, an inside lower tile below it.
fn sphere(rf: Shape) -> Result<(Topology, Field<f32>), Failure> {
    let lim = ((rf.r * 1.5) as i32 + 24) & !7;
    let (mut leaves, mut tiles) = (Vec::new(), Vec::new());
    for z in (-lim..lim).step_by(8) {
        for y in (-lim..lim).step_by(8) {
            for x in (-lim..lim).step_by(8) {
                let ds = (0..512).map(|i| distance([x + i % 8, y + i / 8 % 8, z + i / 64], rf));
                let (lo, hi) = ds.fold((f32::MAX, f32::MIN), |(lo, hi), d| (lo.min(d), hi.max(d)));
                if hi <= -BAND {
                    tiles.push([x, y, z]);
                } else if lo < BAND {
                    leaves.push([x, y, z]);
                }
            }
        }
    }
    let empty = Topology::new();
    let mut e = empty.edit();
    for v in &leaves {
        e.activate(*v)?;
    }
    for v in &tiles {
        e.add_tile(TileLevel::Lower, *v)?;
    }
    let t = e.commit()?.0;
    let mut f = Field::new(&t, BAND);
    par_update(&t, &mut f, |n, values| -> Result<(), Failure> {
        for (voxel, v) in t.leaf_voxels(n)?.zip(values.iter_mut()) {
            *v = distance(voxel, rf).clamp(-BAND, BAND);
        }
        Ok(())
    })
    .map_err(|e| e.to_string())?;
    for n in t.tile_numbers().collect::<Vec<_>>() {
        f.set_tile(&t, n, -BAND)?;
    }
    Ok((t, f))
}

// ---- brodo's tree as OpenVDB's nodes ----

/// A lower node: its tiles and leaves by place, OpenVDB's bit order (z fastest).
#[derive(Default)]
struct Lower {
    tiles: BTreeMap<usize, f64>,
    leaves: BTreeMap<usize, [f64; 512]>,
}

#[derive(Default)]
struct Upper {
    tiles: BTreeMap<usize, f64>,
    lowers: BTreeMap<usize, Lower>,
}

struct Tree {
    background: f64,
    root_tiles: BTreeMap<Coord, f64>,
    uppers: BTreeMap<Coord, Upper>,
}

/// A place's bit in a node whose children have edge 2^child and which has 2^log a side.
fn place(v: Coord, child: u32, log: u32) -> usize {
    let local = |c: i32| ((c as u32 >> child) & ((1 << log) - 1)) as usize;
    (local(v[0]) << (2 * log)) + (local(v[1]) << log) + local(v[2])
}

fn origin(v: Coord, edge_log: u32) -> Coord {
    v.map(|c| c & !((1 << edge_log) - 1))
}

fn tree<T: Value + Into<f64>>(t: &Topology, f: &Field<T>) -> Result<Tree, Failure> {
    let mut tr = Tree { background: f.background().into(), root_tiles: BTreeMap::new(), uppers: BTreeMap::new() };
    for n in t.leaf_numbers() {
        let o = t.leaf_origin(n)?;
        let values = f.leaf(t, n)?;
        // brodo's leaf is x fastest; OpenVDB's is z fastest.
        let mut vdb = [0.0_f64; 512];
        for (i, v) in values.iter().enumerate() {
            let (x, y, z) = (i % 8, i / 8 % 8, i / 64);
            vdb[(x << 6) + (y << 3) + z] = (*v).into();
        }
        let upper = tr.uppers.entry(origin(o, 12)).or_default();
        let lower = upper.lowers.entry(place(o, 7, 5)).or_default();
        lower.leaves.insert(place(o, 3, 4), vdb);
    }
    for n in t.tile_numbers() {
        let (level, o) = t.tile(n)?;
        let value: f64 = f.tile(t, n)?.into();
        match level {
            TileLevel::Root => {
                tr.root_tiles.insert(o, value);
            }
            TileLevel::Upper => {
                tr.uppers.entry(origin(o, 12)).or_default().tiles.insert(place(o, 7, 5), value);
            }
            TileLevel::Lower => {
                let upper = tr.uppers.entry(origin(o, 12)).or_default();
                upper.lowers.entry(place(o, 7, 5)).or_default().tiles.insert(place(o, 3, 4), value);
            }
        }
    }
    Ok(tr)
}

// ---- The bytes ----

struct Out {
    w: Cursor<Vec<u8>>,
    mask_compression: bool,
    /// Values are written as `f64`, else as `f32`.
    double: bool,
}

impl Out {
    fn bytes(&mut self, b: &[u8]) -> Result<(), Failure> {
        self.w.write_all(b)?;
        Ok(())
    }
    fn u8(&mut self, v: u8) -> Result<(), Failure> {
        self.bytes(&[v])
    }
    fn u32(&mut self, v: u32) -> Result<(), Failure> {
        self.bytes(&v.to_le_bytes())
    }
    fn i32(&mut self, v: i32) -> Result<(), Failure> {
        self.bytes(&v.to_le_bytes())
    }
    fn i64(&mut self, v: i64) -> Result<(), Failure> {
        self.bytes(&v.to_le_bytes())
    }
    /// A value at the grid's width: the tree holds every value exactly as an `f64`.
    fn value(&mut self, v: f64) -> Result<(), Failure> {
        if self.double { self.bytes(&v.to_le_bytes()) } else { self.bytes(&(v as f32).to_le_bytes()) }
    }
    fn f64(&mut self, v: f64) -> Result<(), Failure> {
        self.bytes(&v.to_le_bytes())
    }
    fn string(&mut self, s: &str) -> Result<(), Failure> {
        self.u32(u32::try_from(s.len())?)?;
        self.bytes(s.as_bytes())
    }
    fn coord(&mut self, c: Coord) -> Result<(), Failure> {
        c.iter().try_for_each(|v| self.i32(*v))
    }
    /// A mask of `bits` bits as u64 words, bit n in word n/64 at bit n%64.
    fn mask(&mut self, bits: usize, on: impl Fn(usize) -> bool) -> Result<(), Failure> {
        for w in 0..bits / 64 {
            let word = (0..64).fold(0_u64, |a, b| a | (u64::from(on(w * 64 + b)) << b));
            self.bytes(&word.to_le_bytes())?;
        }
        Ok(())
    }
    /// A node's values block: every slot (indicator 6), or with active-mask compression,
    /// where every inactive slot holds the background, the active ones alone (indicator 0).
    fn values(&mut self, slots: usize, active: impl Fn(usize) -> Option<f64>, child: impl Fn(usize) -> bool, bg: f64) -> Result<(), Failure> {
        if self.mask_compression {
            self.u8(0)?;
            for i in 0..slots {
                if let Some(v) = active(i) {
                    self.value(v)?;
                }
            }
        } else {
            self.u8(6)?;
            for i in 0..slots {
                let v = match active(i) {
                    Some(v) => v,
                    None if child(i) => 0.0,
                    None => bg,
                };
                self.value(v)?;
            }
        }
        Ok(())
    }
}

struct Stats {
    min: Coord,
    max: Coord,
    voxels: i64,
}

fn stats(tr: &Tree) -> Stats {
    let mut s = Stats { min: [i32::MAX; 3], max: [i32::MIN; 3], voxels: 0 };
    let mut add = |o: Coord, edge: i32| {
        for k in 0..3 {
            s.min[k] = s.min[k].min(o[k]);
            s.max[k] = s.max[k].max(o[k] + edge - 1);
        }
        s.voxels += i64::from(edge).pow(3);
    };
    for o in tr.root_tiles.keys() {
        add(*o, 4096);
    }
    for (uo, u) in &tr.uppers {
        let at = |i: usize, child_log: u32, log: u32, base: Coord| -> Coord {
            let m = (1 << log) - 1;
            [(i >> (2 * log)) & m, (i >> log) & m, i & m].iter().zip(base).map(|(l, b)| b + ((*l as i32) << child_log)).collect::<Vec<_>>().try_into().unwrap_or(base)
        };
        for i in u.tiles.keys() {
            add(at(*i, 7, 5, *uo), 128);
        }
        for (li, l) in &u.lowers {
            let lo = at(*li, 7, 5, *uo);
            for i in l.tiles.keys() {
                add(at(*i, 3, 4, lo), 8);
            }
            for i in l.leaves.keys() {
                add(at(*i, 3, 4, lo), 8);
            }
        }
    }
    s
}

fn write(tr: &Tree, meta: &GridMeta, mask: bool, double: bool) -> Result<Vec<u8>, Failure> {
    let mut o = Out { w: Cursor::new(Vec::new()), mask_compression: mask, double };
    // Header.
    o.i64(i64::from(MAGIC))?;
    o.u32(FILE_VERSION)?;
    o.u32(LIBRARY.0)?;
    o.u32(LIBRARY.1)?;
    o.u8(1)?; // grid offsets follow
    o.bytes(b"00000000-0000-0000-0000-000000000000")?;
    o.u32(0)?; // file metadata: none
    o.i32(1)?; // one grid
    // Grid descriptor.
    o.string(&meta.name)?;
    o.string(if double { "Tree_double_5_4_3" } else { "Tree_float_5_4_3" })?;
    o.string("")?;
    let offsets_at = o.w.position();
    o.i64(0)?;
    o.i64(0)?;
    o.i64(0)?;
    let grid_pos = o.w.position();
    let flags: u32 = if mask { 2 } else { 0 };
    o.u32(flags)?;
    // Metadata, sorted by name.
    let s = stats(tr);
    let entries: Vec<(&str, &str, Vec<u8>)> = vec![
        ("class", "string", b"level set".to_vec()),
        ("file_bbox_max", "vec3i", s.max.iter().flat_map(|v| v.to_le_bytes()).collect()),
        ("file_bbox_min", "vec3i", s.min.iter().flat_map(|v| v.to_le_bytes()).collect()),
        ("file_compression", "string", if mask { b"active values".to_vec() } else { b"none".to_vec() }),
        ("file_mem_bytes", "int64", 0_i64.to_le_bytes().to_vec()),
        ("file_voxel_count", "int64", s.voxels.to_le_bytes().to_vec()),
        ("name", "string", meta.name.as_bytes().to_vec()),
    ];
    o.u32(u32::try_from(entries.len())?)?;
    for (name, ty, value) in &entries {
        o.string(name)?;
        o.string(ty)?;
        o.u32(u32::try_from(value.len())?)?;
        o.bytes(value)?;
    }
    // Transform: OpenVDB samples index v where brodo samples v + 1/2 (docs/formats.md §5),
    // and multiplies row vectors, so its matrix is brodo's transposed.
    o.string("AffineMap")?;
    let a = meta.transform;
    let mut m = [[0.0_f64; 4]; 4];
    for r in 0..3 {
        for c in 0..3 {
            m[c][r] = a[r][c];
        }
        m[3][r] = a[r][3] + 0.5 * (a[r][0] + a[r][1] + a[r][2]);
    }
    m[3][3] = 1.0;
    m.iter().flatten().try_for_each(|v| o.f64(*v))?;
    // Topology.
    o.i32(1)?;
    o.value(tr.background)?;
    o.u32(u32::try_from(tr.root_tiles.len())?)?;
    o.u32(u32::try_from(tr.uppers.len())?)?;
    for (c, v) in &tr.root_tiles {
        o.coord(*c)?;
        o.value(*v)?;
        o.u8(1)?;
    }
    let bg = tr.background;
    for (c, u) in &tr.uppers {
        o.coord(*c)?;
        o.mask(32768, |i| u.lowers.contains_key(&i))?;
        o.mask(32768, |i| u.tiles.contains_key(&i))?;
        o.values(32768, |i| u.tiles.get(&i).copied(), |i| u.lowers.contains_key(&i), bg)?;
        for l in u.lowers.values() {
            o.mask(4096, |i| l.leaves.contains_key(&i))?;
            o.mask(4096, |i| l.tiles.contains_key(&i))?;
            o.values(4096, |i| l.tiles.get(&i).copied(), |i| l.leaves.contains_key(&i), bg)?;
            for _ in l.leaves.values() {
                o.mask(512, |_| true)?;
            }
        }
    }
    // Buffers: each leaf in the same order.
    let block_pos = o.w.position();
    for u in tr.uppers.values() {
        for l in u.lowers.values() {
            for values in l.leaves.values() {
                o.mask(512, |_| true)?;
                o.values(512, |i| Some(values[i]), |_| false, bg)?;
            }
        }
    }
    let end_pos = o.w.position();
    o.w.seek(SeekFrom::Start(offsets_at))?;
    for p in [grid_pos, block_pos, end_pos] {
        o.i64(i64::try_from(p)?)?;
    }
    Ok(o.w.into_inner())
}

/// brodo's own list of its active boxes, as the oracle prints them, read from the topology
/// and field themselves rather than from the nodes written, so that a mistake in the
/// conversion shows as a difference.
fn listing<T: Value + Into<f64>>(t: &Topology, f: &Field<T>) -> Result<String, Failure> {
    let mut rows = Vec::new();
    let mut boxed = |o: Coord, edge: i32, v: f64| {
        rows.push(format!("{}\t{}\t{}\t{}\t{}\t{}\t{}", o[0], o[1], o[2], o[0] + edge - 1, o[1] + edge - 1, o[2] + edge - 1, v));
    };
    for n in t.leaf_numbers() {
        for (voxel, v) in t.leaf_voxels(n)?.zip(f.leaf(t, n)?) {
            boxed(voxel, 1, (*v).into());
        }
    }
    for n in t.tile_numbers() {
        let (level, o) = t.tile(n)?;
        let edge = match level {
            TileLevel::Root => 4096,
            TileLevel::Upper => 128,
            TileLevel::Lower => 8,
        };
        boxed(o, edge, f.tile(t, n)?.into());
    }
    Ok(format!("background\t{}\n{}\n", f.background().into(), rows.join("\n")))
}

fn main() -> Result<(), Failure> {
    let args: Vec<String> = std::env::args().collect();
    let (cmd, out, case, mode, ty) = match args.as_slice() {
        [_, c, o, k, m] => (c, o, k, m, "float"),
        [_, c, o, k, m, t] => (c, o, k, m, t.as_str()),
        _ => return Err("usage: vdb-spike write OUT sphere20|sphere80|ellipsoid20 none|mask [float|double]".into()),
    };
    if cmd != "write" {
        return Err("only write".into());
    }
    let round = |r: f64| Shape { r, centre: [0.0; 3], scale: [1.0; 3] };
    let shape = match case.as_str() {
        "sphere20" => round(20.0),
        "sphere80" => round(80.0),
        "ellipsoid20" => Shape { r: 20.0, centre: [3.0, -5.0, 7.0], scale: [1.0, 1.3, 0.8] },
        _ => return Err("case: sphere20, sphere80 or ellipsoid20".into()),
    };
    let (t, f) = sphere(shape)?;
    let double = ty == "double";
    // The double grid holds each value of the float one plus a part below f32's precision,
    // so that a value written through f32 shows as a difference.
    let fd = {
        let mut g = Field::new(&t, f64::from(f.background()));
        par_update(&t, &mut g, |n, values| -> Result<(), Failure> {
            for ((voxel, v), s) in t.leaf_voxels(n)?.zip(values.iter_mut()).zip(f.leaf(&t, n)?) {
                *v = f64::from(*s) + 1e-12 * f64::from(voxel[0] - 2 * voxel[1] + 3 * voxel[2]);
            }
            Ok(())
        })
        .map_err(|e| e.to_string())?;
        for n in t.tile_numbers().collect::<Vec<_>>() {
            g.set_tile(&t, n, f64::from(f.tile(&t, n)?))?;
        }
        g
    };
    let tr = if double { tree(&t, &fd)? } else { tree(&t, &f)? };
    // Refuse to write a case that holds nothing the check would see (PLANNING.md §3).
    if tr.uppers.values().all(|u| u.lowers.values().all(|l| l.leaves.is_empty())) {
        return Err("the case holds no leaf".into());
    }
    let meta = GridMeta::new("sdf", [1.0; 3], "voxel")?;
    let bytes = write(&tr, &meta, mode == "mask", double)?;
    std::fs::write(out, &bytes)?;
    std::fs::write(format!("{out}.tsv"), if double { listing(&t, &fd)? } else { listing(&t, &f)? })?;
    eprintln!("{out}: {} bytes, {} leaves, {} tiles", bytes.len(), t.leaf_count(), t.tile_count());
    Ok(())
}
