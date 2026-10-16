//! The diffusion scenario on each candidate: five fields (temperature and velocity
//! double-buffered, a flag), leaf-parallel updates reading other fields, two commits that
//! grow, split and merge, and a final commit that a merge rule must refuse. Every candidate
//! must end with the same bits.

mod common;

use std::sync::Arc;

use common::*;
use remap_spike::core::{FieldData, MergeRule, uniform};
use remap_spike::{brand, fieldset, generation};

// ---------------------------------------------------------------- A: generation
struct SolverA {
    t: generation::Topology,
    temp: [generation::Field<f32>; 2],
    vel: [generation::Field<[f32; 3]>; 2],
    flag: generation::Field<u8>,
}

fn run_a() -> Outcome {
    use generation::{Field, Topology, par_update};
    let (t, _) = Topology::new().commit(&diffusion_initial()).unwrap();
    let mut s = SolverA {
        temp: [Field::new(&t, 0.0), Field::new(&t, 0.0)],
        vel: [Field::new(&t, [0.0; 3]), Field::new(&t, [0.0; 3])],
        flag: Field::new(&t, 0),
        t,
    };
    let [temp0, _] = &mut s.temp;
    diffusion_init(s.t.shape(), temp0.data_mut(&s.t).unwrap(), s.flag.data_mut(&s.t).unwrap());
    for step in 1..=STEPS {
        let shape = s.t.shape();
        let [t_old, t_new] = &mut s.temp;
        let told = t_old.data(&s.t).unwrap();
        par_update(&s.t, t_new, |leaf, out| temp_kernel(shape, told, leaf, out)).unwrap();
        let [v_old, v_new] = &mut s.vel;
        let vold = v_old.data(&s.t).unwrap();
        par_update(&s.t, v_new, |leaf, out| vel_kernel(vold, told, leaf, out)).unwrap();
        s.temp.swap(0, 1);
        s.vel.swap(0, 1);
        if let Some(ops) = diffusion_edit(step) {
            let (t2, r) = s.t.commit(&ops).unwrap();
            // Every field, the scratch buffers included, must be remapped by hand.
            s.temp = [s.temp[0].remap(&r, &mean).unwrap(), s.temp[1].remap(&r, &mean).unwrap()];
            s.vel = [s.vel[0].remap(&r, &uniform).unwrap(), s.vel[1].remap(&r, &uniform).unwrap()];
            s.flag = s.flag.remap(&r, &uniform).unwrap();
            s.t = t2;
        }
    }
    // The refused commit: the temperature remaps, then the velocity is refused. The new
    // topology and the remapped temperature are dropped; the old ones are intact.
    let (_t2, r) = s.t.commit(&diffusion_refused()).unwrap();
    let refused = s.temp[0].remap(&r, &mean).and_then(|_| s.vel[0].remap(&r, &uniform)).map(|_| ());
    Outcome {
        leaves: s.t.shape().leaves.clone(),
        tiles: s.t.shape().tiles.clone(),
        temp: s.temp[0].data(&s.t).unwrap().clone(),
        vel: s.vel[0].data(&s.t).unwrap().clone(),
        flag: s.flag.data(&s.t).unwrap().clone(),
        refused,
    }
}

// ---------------------------------------------------------------- B: field set
struct SolverB {
    set: fieldset::FieldSet,
    temp: [fieldset::FieldId<f32>; 2],
    vel: [fieldset::FieldId<[f32; 3]>; 2],
    flag: fieldset::FieldId<u8>,
}

fn run_b() -> Outcome {
    let mut set = fieldset::FieldSet::new();
    set.commit(&diffusion_initial()).unwrap();
    let m: Arc<MergeRule<f32>> = Arc::new(mean);
    let u3: Arc<MergeRule<[f32; 3]>> = Arc::new(uniform::<[f32; 3]>);
    let temp = [set.add(0.0, Arc::clone(&m)), set.add(0.0, m)];
    let vel = [set.add([0.0; 3], Arc::clone(&u3)), set.add([0.0; 3], u3)];
    let flag = set.add(0_u8, Arc::new(uniform::<u8>));
    let mut s = SolverB { set, temp, vel, flag };
    // Two fields written at once need two lookups the borrow checker accepts only apart.
    let shape = s.set.shape().clone();
    let mut t0 = s.set.get(s.temp[0]).unwrap().clone();
    let mut f0 = s.set.get(s.flag).unwrap().clone();
    diffusion_init(&shape, &mut t0, &mut f0);
    *s.set.get_mut(s.temp[0]).unwrap() = t0;
    *s.set.get_mut(s.flag).unwrap() = f0;
    for step in 1..=STEPS {
        let [to, tn] = s.temp;
        let [vo, vn] = s.vel;
        // The reader's `get` returns a Result inside the parallel closure, where it cannot
        // be propagated.
        s.set
            .par_update(tn, |rd, leaf, out| {
                temp_kernel(rd.shape(), rd.get(to).expect("read field"), leaf, out)
            })
            .unwrap();
        s.set
            .par_update(vn, |rd, leaf, out| {
                vel_kernel(rd.get(vo).expect("read"), rd.get(to).expect("read"), leaf, out)
            })
            .unwrap();
        s.temp.swap(0, 1);
        s.vel.swap(0, 1);
        if let Some(ops) = diffusion_edit(step) {
            s.set.commit(&ops).unwrap();
        }
    }
    let before = s.set.shape().leaves.clone();
    let refused = s.set.commit(&diffusion_refused());
    assert_eq!(s.set.shape().leaves, before, "a refused commit changes nothing");
    let get = |s: &SolverB| -> (FieldData<f32>, FieldData<[f32; 3]>, FieldData<u8>) {
        (
            s.set.get(s.temp[0]).unwrap().clone(),
            s.set.get(s.vel[0]).unwrap().clone(),
            s.set.get(s.flag).unwrap().clone(),
        )
    };
    let (temp, vel, flag) = get(&s);
    Outcome {
        leaves: s.set.shape().leaves.clone(),
        tiles: s.set.shape().tiles.clone(),
        temp,
        vel,
        flag,
        refused,
    }
}

// ---------------------------------------------------------------- C: brand
struct SolverC<'id> {
    temp: [brand::Field<'id, f32>; 2],
    vel: [brand::Field<'id, [f32; 3]>; 2],
    flag: brand::Field<'id, u8>,
}

fn run_c() -> Outcome {
    brand::with_topology(|t0| {
        t0.commit(&diffusion_initial(), |t, _| {
            let mut s = SolverC {
                temp: [brand::Field::new(&t, 0.0), brand::Field::new(&t, 0.0)],
                vel: [brand::Field::new(&t, [0.0; 3]), brand::Field::new(&t, [0.0; 3])],
                flag: brand::Field::new(&t, 0),
            };
            let [temp0, _] = &mut s.temp;
            diffusion_init(t.shape(), temp0.data_mut(&t), s.flag.data_mut(&t));
            steps_c(t, s, 1)
        })
        .unwrap()
    })
}

// Bounded recursion: one level per commit, `diffusion_edit` makes two. A loop cannot carry
// a topology across a commit, since each has its own brand.
fn steps_c<'id>(t: brand::Topology<'id>, mut s: SolverC<'id>, first: u32) -> Outcome {
    for step in first..=STEPS {
        let shape = t.shape();
        let [t_old, t_new] = &mut s.temp;
        let told = t_old.data(&t);
        brand::par_update(&t, t_new, |leaf, out| temp_kernel(shape, told, leaf, out));
        let [v_old, v_new] = &mut s.vel;
        let vold = v_old.data(&t);
        brand::par_update(&t, v_new, |leaf, out| vel_kernel(vold, told, leaf, out));
        s.temp.swap(0, 1);
        s.vel.swap(0, 1);
        if let Some(ops) = diffusion_edit(step) {
            return t
                .commit(&ops, |t2, r| {
                    let s2 = SolverC {
                        temp: [r.remap(&s.temp[0], &mean).unwrap(), r.remap(&s.temp[1], &mean).unwrap()],
                        vel: [
                            r.remap(&s.vel[0], &uniform).unwrap(),
                            r.remap(&s.vel[1], &uniform).unwrap(),
                        ],
                        flag: r.remap(&s.flag, &uniform).unwrap(),
                    };
                    steps_c(t2, s2, step + 1)
                })
                .unwrap();
        }
    }
    let refused = t
        .commit(&diffusion_refused(), |_t2, r| {
            r.remap(&s.temp[0], &mean).and_then(|_| r.remap(&s.vel[0], &uniform)).map(|_| ())
        })
        .unwrap();
    let leaves = t.shape().leaves.clone();
    let tiles = t.shape().tiles.clone();
    let [temp, _] = s.temp;
    let [vel, _] = s.vel;
    Outcome {
        leaves,
        tiles,
        temp: temp.into_data(),
        vel: vel.into_data(),
        flag: s.flag.into_data(),
        refused,
    }
}

#[test]
fn every_candidate_ends_with_the_same_bits() {
    let (a, b, c) = (run_a(), run_b(), run_c());
    check_outcome(&a, &b);
    check_outcome(&b, &c);
    check_outcome(&c, &a);
}
