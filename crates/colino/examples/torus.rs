//! A torus converted and checked (ROADMAP.md, P2): its mesh written as OBJ text from the
//! textbook parametrisation, read back through `colino::obj`, converted, and its topology
//! reported; a slice and the report are written to `out/examples/colino/`.
//!
//! Textbook material only (AGENTS.md S1): the torus of ring radius R and tube radius r is
//! ((R + r cos v) cos u, (R + r cos v) sin u, r sin v), and the signed distance to it is
//! √((√(x² + y²) − R)² + z²) − r. The checks: the solid has one component and one tunnel,
//! its Euler characteristic is the mesh's, and every voxel in the band lies within the
//! faceting's error of the exact distance.
//!
//! Given a mesh file (`.stl`, `.obj` or `.3mf`) and a voxel size, it converts that instead
//! and writes its report: `cargo run -p colino --example torus -- part.stl 0.5`.

use std::error::Error;
use std::path::{Path, PathBuf};

use brodo::slice::{Axis, ColorMap, Slice, encode_png, render};
use colino::convert::{Converted, Options, convert};
use colino::mesh::{Limits, Mesh};
use colino::report::{Report, report};

type BoxError = Box<dyn Error + Send + Sync>;

/// Ring and tube radii, in millimetres, and the quads around each.
const RING: f64 = 20.0;
const TUBE: f64 = 7.0;
const STEPS: u32 = 48;
/// The voxel size, in millimetres.
const H: f64 = 1.0;
/// The most a file read here may hold.
const LIMITS: Limits = Limits::new(1 << 24, 1 << 24, 1 << 30);

/// The torus as OBJ text, its quads split into triangles facing out.
fn torus_obj() -> String {
    let angle = |i: u32| 2.0 * std::f64::consts::PI * f64::from(i % STEPS) / f64::from(STEPS);
    let mut text = String::new();
    for i in 0..STEPS {
        for j in 0..STEPS {
            let (u, v) = (angle(i), angle(j));
            let ring = RING + TUBE * v.cos();
            text.push_str(&format!("v {} {} {}\n", ring * u.cos(), ring * u.sin(), TUBE * v.sin()));
        }
    }
    // OBJ numbers vertices from 1.
    let at = |i: u32, j: u32| (i % STEPS) * STEPS + j % STEPS + 1;
    for i in 0..STEPS {
        for j in 0..STEPS {
            let (a, b, c, d) = (at(i, j), at(i + 1, j), at(i + 1, j + 1), at(i, j + 1));
            text.push_str(&format!("f {a} {b} {c}\nf {a} {c} {d}\n"));
        }
    }
    text
}

/// The exact signed distance to the smooth torus.
fn exact(p: [f64; 3]) -> f64 {
    ((p[0].hypot(p[1]) - RING).powi(2) + p[2] * p[2]).sqrt() - TUBE
}

/// The largest difference, in the band, between a voxel's value and the exact distance.
fn worst(c: &Converted) -> Result<f64, BoxError> {
    let width = 3.0 * H;
    let mut worst: f64 = 0.0;
    for n in c.topology.leaf_numbers() {
        for (v, x) in c.topology.leaf_voxels(n)?.zip(c.field.leaf(&c.topology, n)?) {
            let d = exact(v.map(|k| (f64::from(k) + 0.5) * H));
            if d.abs() < width - 0.5 {
                worst = worst.max((f64::from(*x) - d).abs());
            }
        }
    }
    Ok(worst)
}

/// How far the mesh may lie from the smooth torus: its vertices are on it, and between
/// them the ring's chords fall inside the circle of radius R + r by (R + r)(1 − cos π/n) at
/// most, the tube's by r(1 − cos π/n); 0.0728 mm at n = 48. Stored as `f32`, a band value
/// adds at most 3h · 2⁻²⁴ more. Measured on 2026-10-20: 0.0726 mm.
fn faceting() -> f64 {
    let sag = 1.0 - (std::f64::consts::PI / f64::from(STEPS)).cos();
    (RING + 2.0 * TUBE) * sag + 3.0 * H / f64::from(1_u32 << 24)
}

fn out_dir() -> Result<PathBuf, BoxError> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../out/examples/colino");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Writes a slice through z = 0 and the report.
fn write(name: &str, c: &Converted, r: &Report, half: i32) -> Result<(), BoxError> {
    let dir = out_dir()?;
    let slice = Slice::new(Axis::Z, 0, [-half, -half], [half - 1, half - 1]);
    let image = render(&c.topology, &c.field, slice, ColorMap::Diverging { limit: 3.0 })?;
    std::fs::write(dir.join(format!("{name}_z0.png")), encode_png(&image)?)?;
    std::fs::write(dir.join(format!("{name}_report.txt")), format!("{r:#?}\n"))?;
    Ok(())
}

/// Converts the torus, checks it and writes it.
fn run() -> Result<(Report, f64), BoxError> {
    let mesh = colino::obj::read(torus_obj().as_bytes(), LIMITS)?;
    let options = Options::new(H).with_unit(Some("millimeter".into()));
    let c = convert(&mesh, &options)?;
    let r = report(&mesh, &c, &[("hole".into(), [0.0; 3]), ("tube".into(), [RING, 0.0, 0.0])])?;
    write("torus", &c, &r, 32)?;
    Ok((r, worst(&c)?))
}

/// A mesh file, read by its extension.
fn read(path: &Path) -> Result<Mesh, BoxError> {
    let bytes = std::fs::read(path)?;
    let ext = path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase);
    Ok(match ext.as_deref() {
        Some("stl") => colino::stl::read(&bytes, LIMITS)?,
        Some("obj") => colino::obj::read(&bytes, LIMITS)?,
        Some("3mf") => colino::threemf::read(&bytes, LIMITS)?,
        _ => return Err(format!("{}: not .stl, .obj or .3mf", path.display()).into()),
    })
}

/// Converts the file at `path` with voxels of edge `h` and writes its report.
fn convert_file(path: &Path, h: f64) -> Result<(), BoxError> {
    let mesh = read(path)?;
    let options = Options::new(h).with_unit(Some("millimeter".into()));
    let c = convert(&mesh, &options)?;
    let r = report(&mesh, &c, &[])?;
    let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("mesh");
    std::fs::write(out_dir()?.join(format!("{name}_report.txt")), format!("{r:#?}\n"))?;
    Ok(())
}

fn main() -> Result<(), BoxError> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let [path, h] = args.as_slice() {
        return convert_file(Path::new(path), h.parse()?);
    }
    let (r, worst) = run()?;
    check(&r, worst)
}

fn check(r: &Report, worst: f64) -> Result<(), BoxError> {
    let betti = [r.solid.components, r.solid.tunnels, r.solid.cavities];
    if betti != [1, 1, 0] || r.euler_change != Some(0) {
        return Err(format!("β {betti:?}, χ change {:?}", r.euler_change).into());
    }
    if worst > faceting() {
        return Err(format!("a band value {worst} mm from the exact distance").into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use colino::report::Place;

    #[test]
    fn the_torus_keeps_its_tunnel_and_its_distances() {
        let (r, worst) = run().unwrap();
        check(&r, worst).unwrap();
        assert!(matches!(r.regions[0].place, Place::Complement { outside: true, .. }));
        assert_eq!(r.regions[1].place, Place::Solid);
        assert!(worst > 0.0, "the faceting shows");
    }
}
