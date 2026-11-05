//! Integration tier (TESTING.md §1): every public function that takes a topology and a
//! field refuses a field of another topology, whatever the topology holds: on an empty one,
//! where a loop over its leaves never reads the field, as on one with leaves. Each checks
//! the field before anything else, so that no answer is given about values it never had
//! (ARCHITECTURE.md §4.2, ADR-0014).

use brodo::accessor::{ValueAccessor, ValueAccessorMut};
use brodo::analysis::{Connectivity, flood_fill, label};
use brodo::cache::CacheEntry;
use brodo::euler::euler;
use brodo::field::Field;
use brodo::frozen::freeze;
use brodo::meta::GridMeta;
use brodo::morphology::{dilate_values, erode_values};
use brodo::parallel::{par_reduce, par_update};
use brodo::topology::Topology;

/// A topology with one leaf, and one with none.
fn topologies() -> [Topology; 2] {
    let empty = Topology::new();
    let mut e = empty.edit();
    e.activate([0, 0, 0]).unwrap();
    [e.commit().unwrap().0, Topology::new()]
}

/// The calls, each by its name, on topology `t` with field `f` of another.
fn refusals(t: &Topology, f: &Field<f32>) -> Vec<(&'static str, String)> {
    let conn = Connectivity::Six;
    let meets = |x: f32| x < 0.0;
    let mut g = f.clone();
    let meta = GridMeta::new("g", [1.0; 3], "mm").unwrap();
    let mut out = vec![
        ("ValueAccessor::new", format!("{:?}", ValueAccessor::new(t, f).err())),
        ("ValueAccessorMut::new", format!("{:?}", ValueAccessorMut::new(t, &mut g).err())),
        ("label", format!("{:?}", label(t, f, meets, conn).err())),
        ("flood_fill", format!("{:?}", flood_fill(t, f, meets, conn, [0, 0, 0]).err())),
        ("euler", format!("{:?}", euler(t, f, meets, conn).err())),
        ("CacheEntry::new", format!("{:?}", CacheEntry::new(&meta, t, f).err())),
        ("freeze", format!("{:?}", freeze(t, f).err())),
        (
            "par_update",
            format!(
                "{:?}",
                par_update(t, &mut g, |_, _| Ok::<_, brodo::error::FieldError>(())).err()
            ),
        ),
        ("par_reduce", format!("{:?}", par_reduce(t, f, 0, |_, _| 0, |a, b| a + b).err())),
    ];
    for k in [0, 1] {
        out.push(("dilate_values", format!("{:?}", dilate_values(t, f, conn, k).err())));
        out.push(("erode_values", format!("{:?}", erode_values(t, f, conn, k).err())));
    }
    #[cfg(feature = "vdb")]
    out.push(("VdbEntry::new", format!("{:?}", brodo::vdb::VdbEntry::new(&meta, t, f).err())));
    #[cfg(feature = "vtk")]
    {
        out.push(("vtk::to_bytes", format!("{:?}", brodo::vtk::to_bytes(t, f, "g").err())));
        // A directory that is not there: the field is refused before the file is made.
        let path = std::path::Path::new("/nonexistent-directory/g.vtu");
        out.push(("vtk::write", format!("{:?}", brodo::vtk::write(path, t, f, "g").err())));
    }
    #[cfg(feature = "slice-png")]
    {
        use brodo::slice::{Axis, ColorMap, Slice, render};
        let slice = Slice::new(Axis::Z, 0, [0, 0], [7, 7]);
        let map = ColorMap::Diverging { limit: 1.0 };
        out.push(("render", format!("{:?}", render(t, f, slice, map).err())));
    }
    out
}

#[test]
fn every_function_taking_a_field_refuses_one_of_another_topology() {
    let [one, none] = topologies();
    // The same leaves, made again: another topology all the same.
    let [again, _] = topologies();
    for (t, other) in [(&one, &none), (&none, &one), (&one, &again)] {
        let f = Field::new(other, -1.0_f32);
        for (name, refused) in refusals(t, &f) {
            assert!(refused.contains("StaleField"), "{name}: {refused}");
        }
    }
}
