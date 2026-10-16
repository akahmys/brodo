//! Candidate A: every topology carries a stamp, unique across the process; a field records
//! the stamp it was made or remapped for and is checked against the topology it is used
//! with. A stale field is refused at run time, as `SpikeError::StaleField`.

use std::sync::atomic::{AtomicU64, Ordering};

use crate::core::{self, FieldData, MergeRule, Op, Shape, SpikeError, Value};

static NEXT_STAMP: AtomicU64 = AtomicU64::new(1);

fn next_stamp() -> u64 {
    NEXT_STAMP.fetch_add(1, Ordering::Relaxed)
}

/// A topology with its stamp.
#[derive(Debug)]
pub struct Topology {
    shape: Shape,
    stamp: u64,
}

impl Default for Topology {
    fn default() -> Self {
        Self::new()
    }
}

impl Topology {
    /// An empty topology.
    pub fn new() -> Self {
        Self { shape: Shape::default(), stamp: next_stamp() }
    }

    /// Its leaves and tiles.
    pub fn shape(&self) -> &Shape {
        &self.shape
    }

    /// A new topology with the edits applied, and the remap to it. The old one stays usable
    /// with its own fields.
    pub fn commit(&self, ops: &[Op]) -> Result<(Topology, Remap), SpikeError> {
        let (shape, inner) = core::apply(&self.shape, ops)?;
        let next = Topology { shape, stamp: next_stamp() };
        let remap = Remap { inner, from: self.stamp, to: next.stamp };
        Ok((next, remap))
    }
}

/// A remap between two stamped topologies.
#[derive(Debug)]
pub struct Remap {
    inner: core::Remap,
    from: u64,
    to: u64,
}

/// A field with the stamp of its topology.
#[derive(Debug, Clone)]
pub struct Field<T> {
    data: FieldData<T>,
    stamp: u64,
}

impl<T: Value> Field<T> {
    /// Every value the background.
    pub fn new(t: &Topology, background: T) -> Self {
        Self { data: FieldData::new(&t.shape, background), stamp: t.stamp }
    }

    fn check(&self, t: &Topology) -> Result<(), SpikeError> {
        if self.stamp == t.stamp { Ok(()) } else { Err(SpikeError::StaleField) }
    }

    /// Its values, if it belongs to `t`.
    pub fn data(&self, t: &Topology) -> Result<&FieldData<T>, SpikeError> {
        self.check(t)?;
        Ok(&self.data)
    }

    /// Its values to write, if it belongs to `t`.
    pub fn data_mut(&mut self, t: &Topology) -> Result<&mut FieldData<T>, SpikeError> {
        self.check(t)?;
        Ok(&mut self.data)
    }

    /// The field on the remap's new topology.
    pub fn remap(&self, r: &Remap, merge: &MergeRule<T>) -> Result<Field<T>, SpikeError> {
        if self.stamp != r.from {
            return Err(SpikeError::StaleField);
        }
        Ok(Field { data: core::remap_data(&self.data, &r.inner, merge)?, stamp: r.to })
    }
}

/// Leaf-parallel write of `out`, checked once against `t`. Reads go through `data(t)`
/// before the call, each checked there.
pub fn par_update<U: Value>(
    t: &Topology,
    out: &mut Field<U>,
    f: impl Fn(u32, &mut [U]) + Sync,
) -> Result<(), SpikeError> {
    core::par_update(out.data_mut(t)?, f);
    Ok(())
}

/// For the cost measurement only: the same update with no check.
pub fn par_update_unchecked<U: Value>(out: &mut Field<U>, f: impl Fn(u32, &mut [U]) + Sync) {
    core::par_update(&mut out.data, f);
}

/// For the cost measurement only: the stamp of `read` checked again in every leaf, as an
/// accessor made per leaf would.
pub fn par_update_checked_per_leaf<T: Value, U: Value>(
    t: &Topology,
    read: &Field<T>,
    out: &mut Field<U>,
    f: impl Fn(&FieldData<T>, u32, &mut [U]) + Sync,
) -> Result<(), SpikeError> {
    out.check(t)?;
    let stale = std::sync::atomic::AtomicBool::new(false);
    core::par_update(&mut out.data, |i, chunk| match read.data(std::hint::black_box(t)) {
        Ok(d) => f(d, i, chunk),
        Err(_) => stale.store(true, Ordering::Relaxed),
    });
    if stale.into_inner() { Err(SpikeError::StaleField) } else { Ok(()) }
}
