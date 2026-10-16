//! The sphere scenario on each candidate: a narrow-band sphere, the union with a second one
//! (its band splits the first sphere's inside tiles), and the merge of leaves left wholly
//! inside. Checked voxel by voxel against exact distances.

mod common;

use std::sync::Arc;

use common::*;
use remap_spike::core::{FieldData, Shape, uniform};
use remap_spike::{brand, fieldset, generation};

fn run_a() -> (Shape, FieldData<f32>) {
    use generation::{Field, Topology, par_update};
    let (t1, _) = Topology::new().commit(&sphere_ops()).unwrap();
    let mut f = Field::new(&t1, BAND);
    sphere_fill(t1.shape(), f.data_mut(&t1).unwrap());
    let (t2, r) = t1.commit(&union_ops(t1.shape())).unwrap();
    let mut f = f.remap(&r, &uniform).unwrap();
    let shape = t2.shape();
    par_update(&t2, &mut f, |leaf, out| union_kernel(shape, leaf, out)).unwrap();
    union_tiles(f.data_mut(&t2).unwrap());
    let (t3, r) = t2.commit(&merge_ops(t2.shape())).unwrap();
    let f = f.remap(&r, &uniform).unwrap();
    let data = f.data(&t3).unwrap().clone();
    (t3.shape().clone(), data)
}

fn run_b() -> (Shape, FieldData<f32>) {
    let mut set = fieldset::FieldSet::new();
    set.commit(&sphere_ops()).unwrap();
    let id = set.add(BAND, Arc::new(uniform::<f32>));
    let shape = set.shape().clone();
    sphere_fill(&shape, set.get_mut(id).unwrap());
    set.commit(&union_ops(&shape)).unwrap();
    set.par_update(id, |rd, leaf, out| union_kernel(rd.shape(), leaf, out)).unwrap();
    union_tiles(set.get_mut(id).unwrap());
    let shape = set.shape().clone();
    set.commit(&merge_ops(&shape)).unwrap();
    (set.shape().clone(), set.get(id).unwrap().clone())
}

fn run_c() -> (Shape, FieldData<f32>) {
    brand::with_topology(|t0| {
        t0.commit(&sphere_ops(), |t1, _| {
            let mut f = brand::Field::new(&t1, BAND);
            sphere_fill(t1.shape(), f.data_mut(&t1));
            t1.commit(&union_ops(t1.shape()), |t2, r| {
                let mut f = r.remap(&f, &uniform).unwrap();
                let shape = t2.shape();
                brand::par_update(&t2, &mut f, |leaf, out| union_kernel(shape, leaf, out));
                union_tiles(f.data_mut(&t2));
                t2.commit(&merge_ops(t2.shape()), |t3, r| {
                    let f = r.remap(&f, &uniform).unwrap();
                    (t3.shape().clone(), f.into_data())
                })
                .unwrap()
            })
            .unwrap()
        })
        .unwrap()
    })
}

#[test]
fn every_candidate_matches_exact_distances() {
    for (name, (shape, f)) in [("A", run_a()), ("B", run_b()), ("C", run_c())] {
        check_union(&shape, &f);
        let merged = shape.tiles.iter().filter(|r| classify(S1, **r) == Class::Band).count();
        assert!(merged > 0, "{name}: some band leaves of the first sphere merged");
        let split = shape.leaves.iter().filter(|r| classify(S1, **r) == Class::Inside).count();
        assert!(split > 0, "{name}: some inside tiles of the first sphere split");
    }
}
