//! Spike for B-012: writes one narrow-band sphere SDF in each documented VTK format, ASCII
//! only (the binary header is not in the published documentation, ADR-0018), with samples
//! for `check.py` to compare, and times each writer.
//!
//! Candidates, both from the published XML file-format documentation: (1) one
//! UnstructuredGrid (`.vtu`) of VTK_VOXEL cells, a leaf voxel each, a tile one cell of its
//! region's size; (2) one ImageData (`.vti`) per leaf (8³ cells) and per tile (one cell),
//! with nothing tying the files together. A voxel at index i spans [i, i + 1] on each axis;
//! its value is cell data. VTK_VOXEL is 11 (VTK's public Python API); its points are written
//! x fastest, then y, then z, which `check.py` tests by probing cell centres.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::time::Instant;

use brodo::field::Field;
use brodo::topology::Topology;
use brodo::tree::{Coord, TileLevel};

const BAND: f32 = 3.0;
const VOXEL: u8 = 11;

fn sphere(r: f32) -> (Topology, Field<f32>) {
    let d = |v: Coord| (v.map(|c| c as f32).iter().map(|c| c * c).sum::<f32>()).sqrt() - r;
    let lim = (r as i32 + 8) / 8 * 8 + 8;
    let empty = Topology::new();
    let mut e = empty.edit();
    for z in (-lim..lim).step_by(8) {
        for y in (-lim..lim).step_by(8) {
            for x in (-lim..lim).step_by(8) {
                let vs: Vec<f32> = (0..512).map(|i| d([x + i % 8, y + i / 8 % 8, z + i / 64])).collect();
                if vs.iter().all(|v| *v <= -BAND) {
                    e.add_tile(TileLevel::Lower, [x, y, z]).unwrap();
                } else if !vs.iter().all(|v| *v >= BAND) {
                    e.activate([x, y, z]).unwrap();
                }
            }
        }
    }
    let (t, _) = e.commit().unwrap();
    let mut f = Field::new(&t, BAND);
    for n in t.leaf_numbers().collect::<Vec<_>>() {
        let o = t.leaf_origin(n).unwrap();
        for (i, v) in f.leaf_mut(&t, n).unwrap().iter_mut().enumerate() {
            let i = i as i32;
            *v = d([o[0] + i % 8, o[1] + i / 8 % 8, o[2] + i / 64]).clamp(-BAND, BAND);
        }
    }
    for n in t.tile_numbers().collect::<Vec<_>>() {
        f.set_tile(&t, n, -BAND).unwrap();
    }
    (t, f)
}

/// Every cell: its smallest corner, its edge, its value.
fn cells(t: &Topology, f: &Field<f32>) -> Vec<(Coord, i32, f32)> {
    let mut out = Vec::new();
    for n in t.leaf_numbers() {
        let o = t.leaf_origin(n).unwrap();
        for (i, v) in f.leaf(t, n).unwrap().iter().enumerate() {
            let i = i as i32;
            out.push(([o[0] + i % 8, o[1] + i / 8 % 8, o[2] + i / 64], 1, *v));
        }
    }
    for n in t.tile_numbers() {
        let (level, o) = t.tile(n).unwrap();
        out.push((o, 1 << level.edge_log2(), f.tile(t, n).unwrap()));
    }
    out
}

fn vtu(t: &Topology, f: &Field<f32>) -> String {
    let mut points: BTreeMap<Coord, u32> = BTreeMap::new();
    let mut order: Vec<Coord> = Vec::new();
    let mut conn = String::new();
    let all = cells(t, f);
    for (o, e, _) in &all {
        for k in 0..8 {
            let p = [o[0] + (k & 1) * e, o[1] + (k >> 1 & 1) * e, o[2] + (k >> 2) * e];
            let id = *points.entry(p).or_insert_with(|| {
                order.push(p);
                (order.len() - 1) as u32
            });
            write!(conn, "{id} ").unwrap();
        }
    }
    let mut s = String::new();
    writeln!(s, r#"<?xml version="1.0"?>"#).unwrap();
    writeln!(s, r#"<VTKFile type="UnstructuredGrid" version="0.1" byte_order="LittleEndian">"#).unwrap();
    writeln!(s, "<UnstructuredGrid>").unwrap();
    writeln!(s, r#"<Piece NumberOfPoints="{}" NumberOfCells="{}">"#, order.len(), all.len()).unwrap();
    writeln!(s, r#"<CellData Scalars="value">"#).unwrap();
    s.push_str(r#"<DataArray type="Float32" Name="value" format="ascii">"#);
    for (_, _, v) in &all {
        write!(s, "{v} ").unwrap();
    }
    s.push_str("</DataArray>\n</CellData>\n<Points>\n");
    s.push_str(r#"<DataArray type="Float32" NumberOfComponents="3" format="ascii">"#);
    for p in &order {
        write!(s, "{} {} {} ", p[0], p[1], p[2]).unwrap();
    }
    s.push_str("</DataArray>\n</Points>\n<Cells>\n");
    write!(s, r#"<DataArray type="Int32" Name="connectivity" format="ascii">{conn}</DataArray>"#).unwrap();
    s.push_str(r#"<DataArray type="Int32" Name="offsets" format="ascii">"#);
    for i in 1..=all.len() {
        write!(s, "{} ", i * 8).unwrap();
    }
    s.push_str("</DataArray>");
    s.push_str(r#"<DataArray type="UInt8" Name="types" format="ascii">"#);
    for _ in &all {
        write!(s, "{VOXEL} ").unwrap();
    }
    s.push_str("</DataArray>\n</Cells>\n</Piece>\n</UnstructuredGrid>\n</VTKFile>\n");
    s
}

/// One ImageData file per leaf and per tile: (name, text).
fn vtis(t: &Topology, f: &Field<f32>) -> Vec<(String, String)> {
    let one = |o: Coord, n: i32, spacing: i32, values: &mut dyn Iterator<Item = f32>| {
        let mut s = String::new();
        writeln!(s, r#"<?xml version="1.0"?>"#).unwrap();
        writeln!(s, r#"<VTKFile type="ImageData" version="0.1" byte_order="LittleEndian">"#).unwrap();
        writeln!(s, r#"<ImageData WholeExtent="0 {n} 0 {n} 0 {n}" Origin="{} {} {}" Spacing="{spacing} {spacing} {spacing}">"#, o[0], o[1], o[2]).unwrap();
        writeln!(s, r#"<Piece Extent="0 {n} 0 {n} 0 {n}">"#).unwrap();
        writeln!(s, r#"<CellData Scalars="value">"#).unwrap();
        s.push_str(r#"<DataArray type="Float32" Name="value" format="ascii">"#);
        for v in values {
            write!(s, "{v} ").unwrap();
        }
        s.push_str("</DataArray>\n</CellData>\n</Piece>\n</ImageData>\n</VTKFile>\n");
        s
    };
    let mut out = Vec::new();
    for n in t.leaf_numbers() {
        let o = t.leaf_origin(n).unwrap();
        let mut vals = f.leaf(t, n).unwrap().iter().copied();
        out.push((format!("leaf_{}.vti", n.get()), one(o, 8, 1, &mut vals)));
    }
    for n in t.tile_numbers() {
        let (level, o) = t.tile(n).unwrap();
        let mut vals = std::iter::once(f.tile(t, n).unwrap());
        out.push((format!("tile_{}.vti", n.get()), one(o, 1, 1 << level.edge_log2(), &mut vals)));
    }
    out
}

fn main() {
    let dir = std::path::PathBuf::from(std::env::args().nth(1).expect("an output directory"));
    for r in [40.0_f32, 80.0] {
        let (t, f) = sphere(r);
        let tag = format!("r{}", r as i32);
        let base = dir.join(&tag);
        std::fs::create_dir_all(base.join("vti")).unwrap();
        // Samples: every 97th voxel cell's centre and value, and every tile's centre.
        let mut samples = String::new();
        for (k, (o, e, v)) in cells(&t, &f).iter().enumerate() {
            if k % 97 == 0 || *e > 1 {
                let c = o.map(|x| x as f32 + *e as f32 / 2.0);
                writeln!(samples, "{} {} {} {}", c[0], c[1], c[2], v).unwrap();
            }
        }
        std::fs::write(base.join("samples.txt"), samples).unwrap();
        let mut best = [f64::MAX; 2];
        for _ in 0..3 {
            let s = Instant::now();
            let text = vtu(&t, &f);
            std::fs::write(base.join("sphere.vtu"), text).unwrap();
            best[0] = best[0].min(s.elapsed().as_secs_f64());
            let s = Instant::now();
            for (name, text) in vtis(&t, &f) {
                std::fs::write(base.join("vti").join(name), text).unwrap();
            }
            best[1] = best[1].min(s.elapsed().as_secs_f64());
        }
        println!("{tag}: {} leaves, {} tiles; write vtu {:.3} s, vti {:.3} s (fastest of 3)", t.leaf_count(), t.tile_count(), best[0], best[1]);
    }
}
