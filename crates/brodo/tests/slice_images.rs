//! Image tier (TESTING.md §1): slices of a sphere's SDF, a torus's SDF and a field with tiles
//! keep the pixels of committed golden images, compared decoded so that an encoder's change
//! does not fail them; the same image always encodes to the same bytes, and decodes back to
//! its pixels.
//!
//! The shapes are textbook signed distances: |p| − r for a sphere, and
//! |(√(x² + y²) − R, z)| − r for a torus round the z axis, clamped to a band of ±3.

#![cfg(feature = "slice-png")]

use brodo::field::Field;
use brodo::slice::{Axis, ColorMap, Image, Slice, encode_png, render};
use brodo::topology::Topology;
use brodo::tree::{Coord, TileLevel};

const BAND: f32 = 3.0;

/// A narrow-band SDF: a leaf where a region's voxels straddle the band, an inside tile where
/// all lie below it, nothing where all lie above it.
fn sdf(lo: [i32; 3], hi: [i32; 3], d: impl Fn(Coord) -> f32) -> (Topology, Field<f32>) {
    let empty = Topology::new();
    let mut e = empty.edit();
    for z in (lo[2]..hi[2]).step_by(8) {
        for y in (lo[1]..hi[1]).step_by(8) {
            for x in (lo[0]..hi[0]).step_by(8) {
                let vs: Vec<f32> =
                    (0..512).map(|i| d([x + i % 8, y + i / 8 % 8, z + i / 64])).collect();
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
            let i = i32::try_from(i).unwrap();
            *v = d([o[0] + i % 8, o[1] + i / 8 % 8, o[2] + i / 64]).clamp(-BAND, BAND);
        }
    }
    for n in t.tile_numbers().collect::<Vec<_>>() {
        f.set_tile(&t, n, -BAND).unwrap();
    }
    (t, f)
}

fn length(v: [f32; 3]) -> f32 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}

fn sphere() -> Image {
    let (t, f) = sdf([-32, -32, -8], [32, 32, 8], |v| length(v.map(|c| c as f32)) - 20.0);
    let slice = Slice::new(Axis::Z, 0, [-28, -28], [28, 28]);
    render(&t, &f, slice, ColorMap::Diverging { limit: f64::from(BAND) }).unwrap()
}

fn torus() -> Image {
    let d = |v: Coord| {
        let [x, y, z] = v.map(|c| c as f32);
        length([(x * x + y * y).sqrt() - 16.0, z, 0.0]) - 6.0
    };
    let (t, f) = sdf([-32, -8, -16], [32, 8, 16], d);
    let slice = Slice::new(Axis::Y, 0, [-28, -12], [28, 12]);
    render(&t, &f, slice, ColorMap::Diverging { limit: f64::from(BAND) }).unwrap()
}

/// Leaves with a gradient, two lower-level tiles and an upper-level one, on the background.
fn tiles() -> Image {
    let empty = Topology::new();
    let mut e = empty.edit();
    for x in (0..64).step_by(8) {
        e.activate([x, 0, 0]).unwrap();
        e.activate([x, 16, 0]).unwrap();
    }
    e.add_tile(TileLevel::Lower, [72, 0, 0]).unwrap();
    e.add_tile(TileLevel::Lower, [80, 8, 0]).unwrap();
    e.add_tile(TileLevel::Upper, [128, 0, 0]).unwrap();
    let (t, _) = e.commit().unwrap();
    let mut f = Field::new(&t, 0.0_f32);
    for n in t.leaf_numbers().collect::<Vec<_>>() {
        let o = t.leaf_origin(n).unwrap();
        for (i, v) in f.leaf_mut(&t, n).unwrap().iter_mut().enumerate() {
            let i = i32::try_from(i).unwrap();
            *v = (o[0] + i % 8) as f32 / 64.0;
        }
    }
    for (n, value) in t.tile_numbers().collect::<Vec<_>>().into_iter().zip([0.25, 0.5, 1.0]) {
        f.set_tile(&t, n, value).unwrap();
    }
    let slice = Slice::new(Axis::Z, 4, [-8, -8], [160, 40]);
    render(&t, &f, slice, ColorMap::Sequential { min: 0.0, max: 1.0 }).unwrap()
}

fn decode(bytes: &[u8]) -> Image {
    let mut reader = png::Decoder::new(std::io::Cursor::new(bytes)).read_info().unwrap();
    let mut rgba = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut rgba).unwrap();
    rgba.truncate(info.buffer_size());
    Image::new(info.width, info.height, rgba)
}

fn scenes() -> [(&'static str, Image); 3] {
    [("sphere", sphere()), ("torus", torus()), ("tiles", tiles())]
}

#[test]
fn slices_keep_their_golden_pixels() {
    for (name, image) in scenes() {
        let path = format!("{}/tests/data/slice_{name}.png", env!("CARGO_MANIFEST_DIR"));
        let golden = decode(&std::fs::read(&path).unwrap());
        assert_eq!((image.width, image.height), (golden.width, golden.height), "{name}: size");
        assert!(image.rgba == golden.rgba, "{name}: pixels differ from {path}");
    }
}

#[test]
fn the_same_image_encodes_to_the_same_bytes_and_decodes_back() {
    for (name, image) in scenes() {
        let bytes = encode_png(&image).unwrap();
        assert_eq!(encode_png(&image).unwrap(), bytes, "{name}");
        assert_eq!(decode(&bytes), image, "{name}");
    }
}

/// Writes the scenes' PNGs to `out/` for a look (TESTING.md T8). Ignored: it writes files.
#[test]
#[ignore = "needs a person or agent to look at out/slices/*.png"]
fn write_scenes_for_a_look() {
    let dir = format!("{}/../../out/slices", env!("CARGO_MANIFEST_DIR"));
    std::fs::create_dir_all(&dir).unwrap();
    for (name, image) in scenes() {
        std::fs::write(format!("{dir}/{name}.png"), encode_png(&image).unwrap()).unwrap();
    }
}
