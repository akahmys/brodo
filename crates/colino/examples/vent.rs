//! A vented box: whether its cavity reaches the outside, at two voxel sizes (ROADMAP.md,
//! P2). What the topology report's second layer is for: a gap narrower than a voxel may
//! close when converted, and named points then fall apart.
//!
//! The box is 40 mm on a side, built of cubes of 2 mm, its walls 6 mm thick around a
//! cavity, and one vent 2 mm square through one wall. Its surface is the boundary of the
//! cubes, faces between a cube and no cube, facing out of the cube. At h = 1 mm the vent is
//! two voxels wide, centres at 20.5 and 21.5 mm: the cavity and the outside are one
//! component, and the solid's Euler characteristic is the surface's half. At h = 3 mm voxel
//! centres fall at 19.5 and 22.5 mm, either side of the vent's 20 to 22: it closes, the
//! cavity is apart from the outside, and χ differs from the input's by one. A slice near
//! the vent at each size and the reports are written to `out/examples/colino/`.

use std::error::Error;
use std::path::Path;

use brodo::slice::{Axis, ColorMap, Slice, encode_png, render};
use colino::convert::{Options, convert};
use colino::mesh::Mesh;
use colino::report::{Report, report};

type BoxError = Box<dyn Error + Send + Sync>;

/// The cube's edge, in millimetres; cubes along each side; the wall's thickness in cubes.
const CUBE: f64 = 2.0;
const SIDE: i32 = 20;
const WALL: i32 = 3;

/// The box's cubes: all but the cavity's, and but the vent's through the wall at x = 0.
fn cells() -> Vec<[i32; 3]> {
    let mut out = Vec::new();
    for z in 0..SIDE {
        for y in 0..SIDE {
            for x in 0..SIDE {
                let cavity = [x, y, z].iter().all(|c| (WALL..SIDE - WALL).contains(c));
                let vent = x < WALL && y == SIDE / 2 && z == SIDE / 2;
                if !cavity && !vent {
                    out.push([x, y, z]);
                }
            }
        }
    }
    out
}

/// The boundary of the union of `cells`, each face two triangles facing out of its cube.
fn boundary(cells: &[[i32; 3]]) -> Result<Mesh, BoxError> {
    let set: std::collections::BTreeSet<[i32; 3]> = cells.iter().copied().collect();
    let mut m =
        Mesh { vertices: Vec::new(), triangles: Vec::new(), unit: Some("millimeter".into()) };
    for c in cells {
        for axis in 0..3 {
            for side in [0, 1] {
                let mut n = *c;
                n[axis] += 2 * side - 1;
                if set.contains(&n) {
                    continue;
                }
                let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
                let corner = |a: i32, b: i32| {
                    let mut p = c.map(f64::from);
                    p[axis] += f64::from(side);
                    p[u] += f64::from(a);
                    p[v] += f64::from(b);
                    p.map(|x| x * CUBE)
                };
                let base = u32::try_from(m.vertices.len())?;
                m.vertices.extend([corner(0, 0), corner(1, 0), corner(1, 1), corner(0, 1)]);
                // u × v points along +axis: counter-clockwise seen from outside on the high
                // side, the other way round on the low.
                let q = if side == 1 { [0, 1, 2, 3] } else { [0, 3, 2, 1] }.map(|i| base + i);
                m.triangles.extend([[q[0], q[1], q[2]], [q[0], q[2], q[3]]]);
            }
        }
    }
    Ok(m.welded()?)
}

/// The box converted with voxels of edge `h`, its report naming the cavity and the outside,
/// and a slice near the vent written as `vent_h{tag}.png`.
fn run(mesh: &Mesh, h: f64, tag: &str) -> Result<Report, BoxError> {
    let c = convert(mesh, &Options::new(h))?;
    let middle = f64::from(SIDE) * CUBE / 2.0;
    let points = [("cavity".into(), [middle; 3]), ("outside".into(), [-10.0, middle, middle])];
    let r = report(mesh, &c, &points)?;
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../out/examples/colino");
    std::fs::create_dir_all(&dir)?;
    #[expect(clippy::cast_possible_truncation, reason = "a voxel index within the box")]
    let (vent, end) = (((middle + 1.0) / h) as i32, ((f64::from(SIDE) * CUBE) / h) as i32 + 4);
    let slice = Slice { axis: Axis::Z, position: vent, min: [-4, -4], max: [end, end] };
    let image = render(&c.topology, &c.field, slice, ColorMap::Diverging { limit: 3.0 * h })?;
    std::fs::write(dir.join(format!("vent_h{tag}.png")), encode_png(&image)?)?;
    std::fs::write(dir.join(format!("vent_h{tag}_report.txt")), format!("{r:#?}\n"))?;
    Ok(r)
}

/// The two conversions' reports, fine then coarse.
fn both() -> Result<[Report; 2], BoxError> {
    let mesh = boundary(&cells())?;
    Ok([run(&mesh, 1.0, "1")?, run(&mesh, 3.0, "3")?])
}

/// Fine: the cavity reaches the outside and χ is the input's. Coarse: they are apart, the
/// cavity a cavity of the solid, and χ is one more.
fn check([fine, coarse]: &[Report; 2]) -> Result<(), BoxError> {
    let betti = |r: &Report| [r.solid.components, r.solid.tunnels, r.solid.cavities];
    if fine.together != [(0, 1)] || fine.euler_change != Some(0) || betti(fine) != [1, 0, 0] {
        return Err(format!("at 1 mm: {fine:?}").into());
    }
    if coarse.apart != [(0, 1)] || coarse.euler_change != Some(1) || betti(coarse) != [1, 0, 1] {
        return Err(format!("at 3 mm: {coarse:?}").into());
    }
    Ok(())
}

fn main() -> Result<(), BoxError> {
    check(&both()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_vent_closes_when_narrower_than_a_voxel() {
        check(&both().unwrap()).unwrap();
    }
}
