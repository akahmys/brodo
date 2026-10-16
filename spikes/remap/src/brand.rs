//! Candidate C: a topology and its fields carry a brand, an invariant lifetime no other
//! topology shares. A commit hands the new topology to a closure under a fresh brand, and a
//! field crosses only through `Remap::remap`. A stale field does not compile.
//!
//! The stale use is refused:
//!
//! ```compile_fail
//! use remap_spike::brand::{with_topology, Field};
//! use remap_spike::core::Op;
//! with_topology(|t| {
//!     let f = Field::new(&t, 0.0_f32);
//!     t.commit(&[Op::AddTile([0, 0, 0])], |t2, _remap| {
//!         f.data(&t2).tile_values.len()
//!     })
//! });
//! ```
//!
//! and the same code, remapping first, compiles (so the failure above is the brand):
//!
//! ```
//! use remap_spike::brand::{with_topology, Field};
//! use remap_spike::core::{uniform, Op};
//! let n = with_topology(|t| {
//!     let f = Field::new(&t, 0.0_f32);
//!     t.commit(&[Op::AddTile([0, 0, 0])], |t2, remap| {
//!         let f = remap.remap(&f, &uniform).unwrap();
//!         f.data(&t2).tile_values.len()
//!     })
//! });
//! assert_eq!(n, Ok(1));
//! ```
//!
//! A field cannot leave the closure of its topology either:
//!
//! ```compile_fail
//! use remap_spike::brand::{with_topology, Field};
//! let f = with_topology(|t| Field::new(&t, 0.0_f32));
//! ```

use std::marker::PhantomData;

use crate::core::{self, FieldData, MergeRule, Op, Shape, SpikeError, Value};

type Brand<'id> = PhantomData<fn(&'id ()) -> &'id ()>;

/// A topology under the brand `'id`.
#[derive(Debug)]
pub struct Topology<'id> {
    shape: Shape,
    _brand: Brand<'id>,
}

/// A remap from the topology branded `'old` to the one branded `'new`.
#[derive(Debug)]
pub struct Remap<'old, 'new> {
    inner: core::Remap,
    _old: Brand<'old>,
    _new: Brand<'new>,
}

/// A field on the topology branded `'id`.
#[derive(Debug, Clone)]
pub struct Field<'id, T> {
    data: FieldData<T>,
    _brand: Brand<'id>,
}

/// Runs `f` on an empty topology under a fresh brand.
pub fn with_topology<R>(f: impl for<'id> FnOnce(Topology<'id>) -> R) -> R {
    f(Topology { shape: Shape::default(), _brand: PhantomData })
}

impl<'id> Topology<'id> {
    /// Its leaves and tiles.
    pub fn shape(&self) -> &Shape {
        &self.shape
    }

    /// Applies the edits and runs `f` on the new topology under a fresh brand, with the remap
    /// to it. What `f` returns cannot carry the new brand.
    pub fn commit<R>(
        &self,
        ops: &[Op],
        f: impl for<'new> FnOnce(Topology<'new>, Remap<'id, 'new>) -> R,
    ) -> Result<R, SpikeError> {
        let (shape, inner) = core::apply(&self.shape, ops)?;
        let remap = Remap { inner, _old: PhantomData, _new: PhantomData };
        Ok(f(Topology { shape, _brand: PhantomData }, remap))
    }
}

impl<'old, 'new> Remap<'old, 'new> {
    /// The field on the new topology.
    pub fn remap<T: Value>(
        &self,
        f: &Field<'old, T>,
        merge: &MergeRule<T>,
    ) -> Result<Field<'new, T>, SpikeError> {
        Ok(Field { data: core::remap_data(&f.data, &self.inner, merge)?, _brand: PhantomData })
    }
}

impl<'id, T: Value> Field<'id, T> {
    /// Every value the background.
    pub fn new(t: &Topology<'id>, background: T) -> Self {
        Self { data: FieldData::new(&t.shape, background), _brand: PhantomData }
    }

    /// Its values; the brand guarantees they match `t`.
    pub fn data(&self, _t: &Topology<'id>) -> &FieldData<T> {
        &self.data
    }

    /// Its values, to write.
    pub fn data_mut(&mut self, _t: &Topology<'id>) -> &mut FieldData<T> {
        &mut self.data
    }

    /// Its values with the brand dropped, to leave the closure (an escape hatch: unchecked).
    pub fn into_data(self) -> FieldData<T> {
        self.data
    }
}

/// Leaf-parallel write of `out`; no check is needed.
pub fn par_update<'id, U: Value>(
    t: &Topology<'id>,
    out: &mut Field<'id, U>,
    f: impl Fn(u32, &mut [U]) + Sync,
) {
    core::par_update(out.data_mut(t), f);
}
