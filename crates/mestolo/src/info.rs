//! `info`: what a file holds, as text of a fixed form, one line a fact, each grid's facts
//! indented under its name (ADR-0050). Every string read from the file, a name, the unit and
//! each metadata key and value, is written quoted and escaped as Rust's `{:?}` escapes it,
//! so that a control character in it can neither move the terminal nor make a line.

use std::fmt::Display;
use std::io::Write;

use brodo::field::{AnyField, Field, Value};
use brodo::meta::Bounds;
use brodo::topology::Topology;

use crate::error::MestoloError;
use crate::load::{Grid, Loaded};

/// A value's components, each compared and written alone.
pub trait Parts: Value {
    /// A component.
    type Part: Copy + PartialOrd + Display;
    /// The number of components.
    const COUNT: usize;
    /// Calls `f` with each component and its place.
    fn each(self, f: &mut impl FnMut(usize, Self::Part));
}

macro_rules! scalar_parts {
    ($($t:ty),*) => {$(
        impl Parts for $t {
            type Part = $t;
            const COUNT: usize = 1;
            fn each(self, f: &mut impl FnMut(usize, $t)) {
                f(0, self);
            }
        }
    )*};
}
scalar_parts!(f32, f64, u8, u32);

macro_rules! vector_parts {
    ($($t:ty),*) => {$(
        impl Parts for [$t; 3] {
            type Part = $t;
            const COUNT: usize = 3;
            fn each(self, f: &mut impl FnMut(usize, $t)) {
                for (k, c) in self.into_iter().enumerate() {
                    f(k, c);
                }
            }
        }
    )*};
}
vector_parts!(f32, f64);

/// The least and greatest of each component over a field's leaf voxels and tiles, a NaN
/// never among them; `None` where there is none.
pub type Extremes<P> = (Vec<Option<P>>, Vec<Option<P>>);

/// Each component's least and greatest value over `f`'s leaf voxels and tiles.
pub fn extremes<T: Parts>(t: &Topology, f: &Field<T>) -> Result<Extremes<T::Part>, MestoloError> {
    let (mut least, mut most) = (vec![None; T::COUNT], vec![None; T::COUNT]);
    let mut see = |k: usize, c: T::Part| {
        // A NaN is unordered even with itself.
        if c.partial_cmp(&c).is_none() {
            return;
        }
        if let Some(l) = least.get_mut(k).filter(|l| l.is_none_or(|x| c < x)) {
            *l = Some(c);
        }
        if let Some(m) = most.get_mut(k).filter(|m| m.is_none_or(|x| c > x)) {
            *m = Some(c);
        }
    };
    for n in t.leaf_numbers() {
        for v in f.leaf(t, n)? {
            v.each(&mut see);
        }
    }
    for n in t.tile_numbers() {
        f.tile(t, n)?.each(&mut see);
    }
    Ok((least, most))
}

/// Writes what `loaded` holds to `out`.
pub fn write(loaded: &Loaded, out: &mut impl Write) -> Result<(), MestoloError> {
    writeln!(out, "kind: {}", loaded.kind.name())?;
    writeln!(out, "grids: {}", loaded.grids.len())?;
    for name in &loaded.skipped {
        writeln!(out, "skipped: {name:?}")?;
    }
    for (name, len) in &loaded.bytes {
        writeln!(out, "bytes: {name:?}, {len} bytes")?;
    }
    for (i, g) in loaded.grids.iter().enumerate() {
        writeln!(out)?;
        writeln!(out, "grid {i}: {:?}", g.meta.name)?;
        match &g.field {
            AnyField::F32(f) => grid(g, f, out)?,
            AnyField::F64(f) => grid(g, f, out)?,
            AnyField::U8(f) => grid(g, f, out)?,
            AnyField::U32(f) => grid(g, f, out)?,
            AnyField::F32x3(f) => grid(g, f, out)?,
            AnyField::F64x3(f) => grid(g, f, out)?,
            _ => return Err(MestoloError::ValueType(g.field.value_type())),
        }
    }
    Ok(())
}

fn grid<T: Parts>(g: &Grid, f: &Field<T>, out: &mut impl Write) -> Result<(), MestoloError> {
    let (m, t) = (&g.meta, &g.topology);
    writeln!(out, "  type: {}", g.field.value_type())?;
    writeln!(out, "  unit: {:?}", m.unit)?;
    writeln!(out, "  voxel size: {}", joined(m.voxel_size))?;
    let rows: Vec<String> = m.transform.iter().map(|r| joined(*r)).collect();
    writeln!(out, "  transform: {}", rows.join(" | "))?;
    match Bounds::of(t)? {
        Bounds::Empty => writeln!(out, "  bounds: empty")?,
        Bounds::Box { min, max } => writeln!(out, "  bounds: {} to {}", joined(min), joined(max))?,
    }
    writeln!(out, "  leaves: {}", t.leaf_count())?;
    writeln!(out, "  tiles: {}", t.tile_count())?;
    let (in_leaves, in_tiles) = voxels(t)?;
    let all = in_leaves.saturating_add(in_tiles);
    writeln!(out, "  voxels: {all}, {in_leaves} in leaves and {in_tiles} in tiles")?;
    writeln!(out, "  background: {}", parts(f.background()))?;
    let (least, most) = extremes(t, f)?;
    writeln!(out, "  least: {}", maybe(&least))?;
    writeln!(out, "  greatest: {}", maybe(&most))?;
    for (k, v) in &m.extra {
        writeln!(out, "  metadata: {k:?} = {v:?}")?;
    }
    Ok(())
}

/// The voxels a topology's leaves cover, and those its tiles cover; brodo holds every one
/// active (ARCHITECTURE.md §4.1).
fn voxels(t: &Topology) -> Result<(u128, u128), MestoloError> {
    let leaves = u128::try_from(t.leaf_count()).unwrap_or(u128::MAX).saturating_mul(512);
    let mut tiles = 0_u128;
    for k in t.tile_numbers() {
        let edge = u128::from(t.tile(k)?.0.edge().unsigned_abs());
        tiles = tiles.saturating_add(edge.pow(3));
    }
    Ok((leaves, tiles))
}

fn joined<T: Display>(xs: impl IntoIterator<Item = T>) -> String {
    xs.into_iter().map(|x| x.to_string()).collect::<Vec<_>>().join(" ")
}

/// A value as its components, one alone, three in brackets.
fn parts<T: Parts>(v: T) -> String {
    let mut out = Vec::new();
    v.each(&mut |_, c| out.push(c.to_string()));
    bracketed(out)
}

fn maybe<P: Display>(xs: &[Option<P>]) -> String {
    bracketed(xs.iter().map(|x| x.as_ref().map_or("none".to_owned(), P::to_string)).collect())
}

fn bracketed(xs: Vec<String>) -> String {
    match xs.as_slice() {
        [one] => one.clone(),
        many => format!("[{}]", many.join(" ")),
    }
}
