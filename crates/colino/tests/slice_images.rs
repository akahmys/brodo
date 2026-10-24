//! Slices of converted meshes as PNG, for a look (TESTING.md T8): a sphere and a torus
//! through their middle, the zero contour drawn, tiles hatched, the background grey.

use brodo::slice::{Axis, ColorMap, Slice, encode_png, render};
use colino::convert::{Options, convert};

mod common;
use common::{sphere, torus};

/// Writes the slices to `out/slices/` for a look. Ignored: it writes files.
#[test]
#[ignore = "needs a person or agent to look at out/slices/colino_*.png"]
fn write_slices_for_a_look() {
    let dir = format!("{}/../../out/slices", env!("CARGO_MANIFEST_DIR"));
    std::fs::create_dir_all(&dir).unwrap();
    let scenes = [
        ("colino_sphere_z0", sphere(4, 20.0, [0.0; 3]), Axis::Z, 30),
        ("colino_torus_z0", torus(20.0, 7.0, 48), Axis::Z, 32),
        ("colino_torus_y0", torus(20.0, 7.0, 48), Axis::Y, 32),
    ];
    for (name, mesh, axis, half) in scenes {
        let c = convert(&mesh, &Options::new(1.0)).unwrap();
        let slice = Slice::new(axis, 0, [-half, -half], [half - 1, half - 1]);
        let image =
            render(&c.topology, &c.field, slice, ColorMap::Diverging { limit: 3.0 }).unwrap();
        std::fs::write(format!("{dir}/{name}.png"), encode_png(&image).unwrap()).unwrap();
    }
}
