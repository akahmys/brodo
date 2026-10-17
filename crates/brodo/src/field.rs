//! A field: one value per voxel of a topology's leaves, one per tile, and a background
//! (ARCHITECTURE.md §4.2, ADR-0014). It records its topology's stamp and refuses any other
//! topology as `FieldError::StaleField`.

use std::iter::repeat_n;

use crate::error::FieldError;
use crate::topology::{
    LeafNumber, LeafSource, Location, Part, Remap, TileNumber, TileSource, Topology,
};
use crate::tree::Coord;

/// Voxels in a leaf.
pub const LEAF_VOXELS: usize = 512;

/// A value a field holds (ARCHITECTURE.md §4.8).
pub trait Value: Copy + Send + Sync + 'static {
    /// Whether two values have the same bits.
    fn same_bits(&self, other: &Self) -> bool;
}

impl Value for f32 {
    fn same_bits(&self, other: &Self) -> bool {
        self.to_bits() == other.to_bits()
    }
}

impl Value for f64 {
    fn same_bits(&self, other: &Self) -> bool {
        self.to_bits() == other.to_bits()
    }
}

impl Value for u8 {
    fn same_bits(&self, other: &Self) -> bool {
        self == other
    }
}

impl Value for u32 {
    fn same_bits(&self, other: &Self) -> bool {
        self == other
    }
}

impl Value for [f32; 3] {
    fn same_bits(&self, other: &Self) -> bool {
        self.iter().zip(other).all(|(a, b)| a.to_bits() == b.to_bits())
    }
}

/// How a merged tile's value comes from the values it covers, each with the number of
/// voxels holding it (ADR-0014); `None` refuses the merge.
pub type MergeRule<T> = dyn Fn(&[(T, u32)]) -> Option<T> + Send + Sync;

/// The merge rule that accepts only values with the same bits.
pub fn uniform<T: Value>(values: &[(T, u32)]) -> Option<T> {
    let (first, _) = *values.first()?;
    values.iter().all(|(v, _)| v.same_bits(&first)).then_some(first)
}

/// Voxels in a leaf, as a count.
const LEAF_COUNT: u32 = 512;

/// A voxel's index within its leaf, x fastest.
fn local_index(v: Coord) -> usize {
    let [x, y, z] = v.map(|c| c.cast_unsigned() & 7);
    // Lossless: below 512.
    (x + 8 * y + 64 * z) as usize
}

/// One value per voxel of a topology's leaves, one per tile, and a background.
#[derive(Debug, Clone)]
pub struct Field<T> {
    /// Leaf count × 512, by leaf number.
    leaf_values: Vec<T>,
    /// One per tile, by tile number.
    tile_values: Vec<T>,
    background: T,
    stamp: u64,
}

impl<T: Value> Field<T> {
    /// A field on `t` with every value the background.
    pub fn new(t: &Topology, background: T) -> Self {
        Self {
            leaf_values: vec![background; t.leaf_count() * LEAF_VOXELS],
            tile_values: vec![background; t.tile_count()],
            background,
            stamp: t.stamp(),
        }
    }

    /// The value outside the tree.
    pub fn background(&self) -> T {
        self.background
    }

    fn check(&self, t: &Topology) -> Result<(), FieldError> {
        if self.stamp == t.stamp() { Ok(()) } else { Err(FieldError::StaleField) }
    }

    /// Every leaf's values, by leaf number, if the field belongs to `t`.
    pub(crate) fn leaves(&self, t: &Topology) -> Result<&[[T; LEAF_VOXELS]], FieldError> {
        self.check(t)?;
        Ok(self.leaf_values.as_chunks().0)
    }

    /// Every leaf's values to write, by leaf number, if the field belongs to `t`.
    pub(crate) fn leaves_mut(
        &mut self,
        t: &Topology,
    ) -> Result<&mut [[T; LEAF_VOXELS]], FieldError> {
        self.check(t)?;
        Ok(self.leaf_values.as_chunks_mut().0)
    }

    fn leaf_slice(&self, n: LeafNumber) -> Result<&[T], FieldError> {
        let s = n.get() as usize * LEAF_VOXELS;
        self.leaf_values.get(s..s + LEAF_VOXELS).ok_or(FieldError::UnknownLeaf(n.get()))
    }

    fn tile_value(&self, n: TileNumber) -> Result<T, FieldError> {
        self.tile_values.get(n.get() as usize).copied().ok_or(FieldError::UnknownTile(n.get()))
    }

    /// A leaf's 512 values, x fastest.
    pub fn leaf(&self, t: &Topology, n: LeafNumber) -> Result<&[T; LEAF_VOXELS], FieldError> {
        self.check(t)?;
        self.leaf_slice(n)?.try_into().map_err(|_| FieldError::UnknownLeaf(n.get()))
    }

    /// A leaf's 512 values, to write.
    pub fn leaf_mut(
        &mut self,
        t: &Topology,
        n: LeafNumber,
    ) -> Result<&mut [T; LEAF_VOXELS], FieldError> {
        self.check(t)?;
        let s = n.get() as usize * LEAF_VOXELS;
        let slice = self.leaf_values.get_mut(s..s + LEAF_VOXELS);
        slice.and_then(|c| c.try_into().ok()).ok_or(FieldError::UnknownLeaf(n.get()))
    }

    /// A tile's value.
    pub fn tile(&self, t: &Topology, n: TileNumber) -> Result<T, FieldError> {
        self.check(t)?;
        self.tile_value(n)
    }

    /// Sets a tile's value.
    pub fn set_tile(&mut self, t: &Topology, n: TileNumber, value: T) -> Result<(), FieldError> {
        self.check(t)?;
        let slot = self.tile_values.get_mut(n.get() as usize);
        *slot.ok_or(FieldError::UnknownTile(n.get()))? = value;
        Ok(())
    }

    /// The value at a voxel: its leaf's, its tile's, or the background.
    pub fn value_at(&self, t: &Topology, v: Coord) -> Result<T, FieldError> {
        self.check(t)?;
        match t.locate(v) {
            Location::Leaf(n) => {
                let leaf = self.leaf_slice(n)?;
                leaf.get(local_index(v)).copied().ok_or(FieldError::UnknownLeaf(n.get()))
            }
            Location::Tile(n, _) => self.tile_value(n),
            Location::Background => Ok(self.background),
        }
    }

    /// The values a merged tile covers, each with its voxel count.
    fn merged(&self, parts: &[Part]) -> Result<Vec<(T, u32)>, FieldError> {
        let mut out = Vec::new();
        for p in parts {
            match *p {
                Part::Leaf(n) => out.extend(self.leaf_slice(n)?.iter().map(|v| (*v, 1))),
                Part::Tile(t) => out.push((self.tile_value(t)?, LEAF_COUNT)),
                Part::Background(k) => out.push((self.background, LEAF_COUNT.saturating_mul(k))),
            }
        }
        Ok(out)
    }

    /// This field on the remap's new topology: each new leaf from its old leaf, from the
    /// tile it was split from, or the background; each new tile from its old tile, the
    /// background, or `rule` applied to what it merges. Merged tiles are worked out in tile
    /// order, so the error does not depend on the thread count.
    pub fn remap(&self, r: &Remap, rule: &MergeRule<T>) -> Result<Field<T>, FieldError> {
        if self.stamp != r.from {
            return Err(FieldError::StaleField);
        }
        let mut leaf_values = Vec::with_capacity(r.leaf_sources().len() * LEAF_VOXELS);
        for s in r.leaf_sources() {
            match *s {
                LeafSource::Leaf(n) => leaf_values.extend_from_slice(self.leaf_slice(n)?),
                LeafSource::Split(t) => {
                    leaf_values.extend(repeat_n(self.tile_value(t)?, LEAF_VOXELS))
                }
                LeafSource::Background => {
                    leaf_values.extend(repeat_n(self.background, LEAF_VOXELS))
                }
            }
        }
        let tile_values = r
            .tile_sources()
            .iter()
            .zip(0_u32..)
            .map(|(s, i)| match *s {
                TileSource::Tile(t) => self.tile_value(t),
                TileSource::Background => Ok(self.background),
                TileSource::Merged(range) => {
                    rule(&self.merged(r.parts(range))?).ok_or(FieldError::MergeRefused(i))
                }
            })
            .collect::<Result<Vec<T>, FieldError>>()?;
        Ok(Field { leaf_values, tile_values, background: self.background, stamp: r.to })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::TileLevel;

    #[test]
    fn a_root_tile_split_passes_its_value_to_every_part() {
        let t0 = Topology::new();
        let mut e = t0.edit();
        e.add_tile(TileLevel::Root, [0, 0, 0]).unwrap();
        let (t1, _) = e.commit().unwrap();
        let mut f = Field::new(&t1, 0.0_f32);
        f.set_tile(&t1, TileNumber(0), 5.0).unwrap();
        let mut e = t1.edit();
        e.split_tile([300, 4000, 17]).unwrap();
        let (t2, r) = e.commit().unwrap();
        let g = f.remap(&r, &uniform).unwrap();
        assert_eq!(t2.tile_count(), 32767 + 4095);
        for n in t2.tile_numbers() {
            assert_eq!(g.tile(&t2, n).unwrap().to_bits(), 5.0_f32.to_bits());
        }
        let leaf = g.leaf(&t2, LeafNumber(0)).unwrap();
        assert!(leaf.iter().all(|v| v.to_bits() == 5.0_f32.to_bits()));
        assert_eq!(g.value_at(&t2, [-1, 0, 0]).map(f32::to_bits), Ok(0.0_f32.to_bits()));
    }

    /// The voxel-weighted mean, exact here: values and counts are small integers.
    fn mean(values: &[(f64, u32)]) -> Option<f64> {
        let n: f64 = values.iter().map(|(_, c)| f64::from(*c)).sum();
        Some(values.iter().map(|(v, c)| v * f64::from(*c)).sum::<f64>() / n)
    }

    #[test]
    fn a_merge_weighs_each_value_by_its_voxels() {
        let t0 = Topology::new();
        let mut e = t0.edit();
        for i in 0..4096 {
            let r = [i % 16 * 8, i / 16 % 16 * 8, i / 256 * 8];
            if i == 0 { e.activate(r).map(|_| ()) } else { e.add_tile(TileLevel::Lower, r) }
                .unwrap();
        }
        let (t1, _) = e.commit().unwrap();
        let mut f = Field::new(&t1, 0.0_f64);
        for n in t1.tile_numbers().collect::<Vec<_>>() {
            f.set_tile(&t1, n, 2.0).unwrap();
        }
        let leaf = t1.leaf_numbers().next().unwrap();
        f.leaf_mut(&t1, leaf).unwrap().fill(4.0);
        let mut e = t1.edit();
        e.merge(TileLevel::Upper, [0, 0, 0]).unwrap();
        let (t2, r) = e.commit().unwrap();
        let g = f.remap(&r, &mean).unwrap();
        // 4095 tiles of 512 voxels at 2 and one leaf of 512 at 4: 2 + 2 / 4096. Weighing
        // each tile as one value would give 10 238 / 4607 instead.
        let n = t2.tile_numbers().next().unwrap();
        assert_eq!(g.tile(&t2, n).map(f64::to_bits), Ok((2.0 + 2.0 / 4096.0_f64).to_bits()));
        let refused = f.remap(&r, &uniform);
        assert_eq!(refused.map(|_| ()), Err(FieldError::MergeRefused(0)));
    }

    #[test]
    fn regions_activated_in_the_merging_commit_count_as_the_background() {
        let t0 = Topology::new();
        let mut e = t0.edit();
        for i in 0..2048 {
            e.add_tile(TileLevel::Lower, [i % 16 * 8, i / 16 % 16 * 8, i / 256 * 8]).unwrap();
        }
        let (t1, _) = e.commit().unwrap();
        let mut f = Field::new(&t1, 3.0_f32);
        let mut e = t1.edit();
        for i in 2048..4096 {
            e.activate([i % 16 * 8, i / 16 % 16 * 8, i / 256 * 8]).unwrap();
        }
        e.merge(TileLevel::Upper, [0, 0, 0]).unwrap();
        let (t2, r) = e.commit().unwrap();
        let TileSource::Merged(range) = r.tile_sources()[0] else { panic!("merged") };
        assert_eq!(r.parts(range).last(), Some(&Part::Background(2048)));
        let g = f.remap(&r, &uniform).unwrap();
        assert_eq!(g.value_at(&t2, [100, 100, 100]).map(f32::to_bits), Ok(3.0_f32.to_bits()));
        let n = t1.tile_numbers().next().unwrap();
        f.set_tile(&t1, n, 1.0).unwrap();
        assert_eq!(f.remap(&r, &uniform).map(|_| ()), Err(FieldError::MergeRefused(0)));
    }

    #[test]
    fn a_stale_field_is_refused_everywhere() {
        let t0 = Topology::new();
        let mut e = t0.edit();
        e.activate([0, 0, 0]).unwrap();
        e.add_tile(TileLevel::Lower, [8, 0, 0]).unwrap();
        let (t1, _) = e.commit().unwrap();
        let mut f = Field::new(&t1, 1_u8);
        let (t2, r) = t1.edit().commit().unwrap();
        let stale = Err(FieldError::StaleField);
        assert_eq!(f.leaf(&t2, LeafNumber(0)).map(|_| ()), stale);
        assert_eq!(f.tile(&t2, TileNumber(0)).map(|_| ()), stale);
        assert_eq!(f.value_at(&t2, [0, 0, 0]).map(|_| ()), stale);
        assert_eq!(f.set_tile(&t2, TileNumber(0), 2), stale);
        assert_eq!(f.leaf_mut(&t2, LeafNumber(0)).map(|_| ()), stale);
        let g = f.remap(&r, &uniform).unwrap();
        assert_eq!(g.remap(&r, &uniform).map(|_| ()), stale, "a remap applies once");
        assert_eq!(g.value_at(&t2, [0, 0, 0]), Ok(1));
        assert_eq!(f.value_at(&t1, [0, 0, 0]), Ok(1), "the old pair still works");
    }

    #[test]
    fn a_voxel_finds_its_place_in_its_leaf() {
        let t0 = Topology::new();
        let mut e = t0.edit();
        e.activate([-1, 2, 3]).unwrap();
        let (t, _) = e.commit().unwrap();
        let mut f = Field::new(&t, 0_u32);
        for (i, v) in f.leaf_mut(&t, LeafNumber(0)).unwrap().iter_mut().enumerate() {
            *v = u32::try_from(i).unwrap();
        }
        // [-1, 2, 3] is at (7, 2, 3) within the leaf whose origin is [-8, 0, 0].
        assert_eq!(f.value_at(&t, [-1, 2, 3]), Ok(7 + 8 * 2 + 64 * 3));
        assert_eq!(f.leaf(&t, LeafNumber(1)).map(|_| ()), Err(FieldError::UnknownLeaf(1)));
    }
}
