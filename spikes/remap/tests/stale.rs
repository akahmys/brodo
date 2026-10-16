//! What each candidate does with a field not remapped after a commit. C's case is the
//! `compile_fail` doctest in `src/brand.rs`.

use std::sync::Arc;

use remap_spike::core::{Op, SpikeError, uniform};
use remap_spike::{fieldset, generation};

#[test]
fn a_refuses_a_stale_field_at_run_time() {
    let t = generation::Topology::new();
    let f = generation::Field::new(&t, 0.0_f32);
    let mut g = generation::Field::new(&t, 0_u8);
    let (t2, r) = t.commit(&[Op::Activate([0, 0, 0])]).unwrap();
    let f = f.remap(&r, &uniform).unwrap();
    // `g` was forgotten: every use with the new topology is refused.
    assert_eq!(g.data(&t2).err(), Some(SpikeError::StaleField));
    assert_eq!(
        generation::par_update(&t2, &mut g, |_, _| {}).err(),
        Some(SpikeError::StaleField)
    );
    // A remap applied twice, or to the wrong topology, is refused as well.
    assert_eq!(f.remap(&r, &uniform).err(), Some(SpikeError::StaleField));
    assert!(f.data(&t2).is_ok());
    // A field and a topology that match are not stale, even old ones.
    assert!(g.data(&t).is_ok());
}

#[test]
fn b_has_no_stale_field_but_refuses_a_foreign_handle() {
    let mut s1 = fieldset::FieldSet::new();
    let mut s2 = fieldset::FieldSet::new();
    let a = s1.add(0.0_f32, Arc::new(uniform::<f32>));
    let _ = s2.add(0.0_f32, Arc::new(uniform::<f32>));
    s1.commit(&[Op::Activate([0, 0, 0])]).unwrap();
    assert_eq!(s1.get(a).unwrap().leaf_values.len(), 512, "remapped by the commit");
    assert_eq!(s2.get(a).err(), Some(SpikeError::UnknownField));
}
