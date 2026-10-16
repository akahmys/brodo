//! Candidate B: the topology owns every field on it, as a `FieldSet`. A commit remaps every
//! field before it changes anything, so a stale field cannot exist and a refused merge leaves
//! the set as it was. Fields are reached through typed handles; a handle from another set,
//! or of another type, is refused at run time.

use std::any::Any;
use std::marker::PhantomData;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::core::{self, FieldData, MergeRule, Op, Remap, Shape, SpikeError, Value};

static NEXT_SET: AtomicU64 = AtomicU64::new(1);

/// A typed handle to a field in one set.
#[derive(Debug)]
pub struct FieldId<T> {
    index: usize,
    set: u64,
    _type: PhantomData<fn() -> T>,
}

impl<T> Clone for FieldId<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for FieldId<T> {}

trait Slot: Send + Sync {
    fn remapped(&self, r: &Remap) -> Result<Box<dyn Slot>, SpikeError>;
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

struct Entry<T: Value> {
    data: FieldData<T>,
    merge: Arc<MergeRule<T>>,
}

impl<T: Value> Slot for Entry<T> {
    fn remapped(&self, r: &Remap) -> Result<Box<dyn Slot>, SpikeError> {
        let data = core::remap_data(&self.data, r, &*self.merge)?;
        Ok(Box::new(Entry { data, merge: Arc::clone(&self.merge) }))
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

fn downcast<T: Value>(slot: &dyn Slot) -> Result<&FieldData<T>, SpikeError> {
    let e = slot.as_any().downcast_ref::<Entry<T>>().ok_or(SpikeError::UnknownField)?;
    Ok(&e.data)
}

fn downcast_mut<T: Value>(slot: &mut dyn Slot) -> Result<&mut FieldData<T>, SpikeError> {
    let e = slot.as_any_mut().downcast_mut::<Entry<T>>().ok_or(SpikeError::UnknownField)?;
    Ok(&mut e.data)
}

/// A topology and every field on it.
pub struct FieldSet {
    shape: Shape,
    id: u64,
    fields: Vec<Box<dyn Slot>>,
}

impl Default for FieldSet {
    fn default() -> Self {
        Self::new()
    }
}

impl FieldSet {
    /// An empty topology with no fields.
    pub fn new() -> Self {
        Self { shape: Shape::default(), id: NEXT_SET.fetch_add(1, Ordering::Relaxed), fields: Vec::new() }
    }

    /// Its leaves and tiles.
    pub fn shape(&self) -> &Shape {
        &self.shape
    }

    /// Adds a field, every value the background, with the rule for merging its leaves.
    pub fn add<T: Value>(&mut self, background: T, merge: Arc<MergeRule<T>>) -> FieldId<T> {
        let data = FieldData::new(&self.shape, background);
        self.fields.push(Box::new(Entry { data, merge }));
        FieldId { index: self.fields.len() - 1, set: self.id, _type: PhantomData }
    }

    fn index<T>(&self, id: FieldId<T>) -> Result<usize, SpikeError> {
        if id.set == self.id && id.index < self.fields.len() {
            Ok(id.index)
        } else {
            Err(SpikeError::UnknownField)
        }
    }

    /// A field's values.
    pub fn get<T: Value>(&self, id: FieldId<T>) -> Result<&FieldData<T>, SpikeError> {
        downcast(&*self.fields[self.index(id)?])
    }

    /// A field's values, to write.
    pub fn get_mut<T: Value>(&mut self, id: FieldId<T>) -> Result<&mut FieldData<T>, SpikeError> {
        let i = self.index(id)?;
        downcast_mut(&mut *self.fields[i])
    }

    /// Applies the edits and remaps every field; on any error nothing changes.
    pub fn commit(&mut self, ops: &[Op]) -> Result<(), SpikeError> {
        let (shape, remap) = core::apply(&self.shape, ops)?;
        let fields =
            self.fields.iter().map(|f| f.remapped(&remap)).collect::<Result<Vec<_>, _>>()?;
        self.shape = shape;
        self.fields = fields;
        Ok(())
    }

    /// Leaf-parallel write of one field; the closure reads every other field through the
    /// `Reader`, which refuses the field being written.
    pub fn par_update<U: Value>(
        &mut self,
        write: FieldId<U>,
        f: impl Fn(&Reader<'_>, u32, &mut [U]) + Sync,
    ) -> Result<(), SpikeError> {
        let w = self.index(write)?;
        let (before, rest) = self.fields.split_at_mut(w);
        let (target, after) = rest.split_first_mut().ok_or(SpikeError::UnknownField)?;
        let reader = Reader { shape: &self.shape, before, after, write: w, set: self.id };
        core::par_update(downcast_mut(&mut **target)?, |i, chunk| f(&reader, i, chunk));
        Ok(())
    }
}

/// Read access to every field of a set but the one being written.
pub struct Reader<'a> {
    shape: &'a Shape,
    before: &'a [Box<dyn Slot>],
    after: &'a [Box<dyn Slot>],
    write: usize,
    set: u64,
}

impl Reader<'_> {
    /// The set's leaves and tiles.
    pub fn shape(&self) -> &Shape {
        self.shape
    }

    /// A field's values; the field being written is refused.
    pub fn get<T: Value>(&self, id: FieldId<T>) -> Result<&FieldData<T>, SpikeError> {
        if id.set != self.set {
            return Err(SpikeError::UnknownField);
        }
        let slot = match id.index.cmp(&self.write) {
            std::cmp::Ordering::Less => self.before.get(id.index),
            std::cmp::Ordering::Equal => return Err(SpikeError::SameField),
            std::cmp::Ordering::Greater => self.after.get(id.index - self.write - 1),
        };
        downcast(&**slot.ok_or(SpikeError::UnknownField)?)
    }
}
