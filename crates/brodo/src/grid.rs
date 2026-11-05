//! A grid: one topology and one field on it, for single-field use (ARCHITECTURE.md §4.2).
//! Editing a grid commits its topology and remaps its field in one step, so its field is
//! never stale.

use std::sync::Arc;

use crate::error::{FieldError, GridError, TreeError};
use crate::field::{Field, LEAF_VOXELS, MergeRule, Value, uniform};
use crate::parallel::par_update;
use crate::topology::{LeafNumber, Remap, TileNumber, Topology, TopologyEditor};
use crate::tree::Coord;

/// One topology and one field on it, with the rule its merges use.
#[derive(Clone)]
pub struct Grid<T> {
    topology: Topology,
    field: Field<T>,
    rule: Arc<MergeRule<T>>,
}

impl<T: std::fmt::Debug> std::fmt::Debug for Grid<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Grid")
            .field("topology", &self.topology)
            .field("field", &self.field)
            .finish_non_exhaustive()
    }
}

impl<T: Value> Grid<T> {
    /// An empty grid: every voxel the background. Its merges use the same-bits rule.
    pub fn new(background: T) -> Self {
        Self::with_rule(background, Arc::new(uniform::<T>))
    }

    /// An empty grid whose merges use `rule`.
    pub fn with_rule(background: T, rule: Arc<MergeRule<T>>) -> Self {
        let topology = Topology::new();
        let field = Field::new(&topology, background);
        Self { topology, field, rule }
    }

    /// A grid of `topology` and `field`, whose merges use `rule`, such as a grid thawed from
    /// a file (`GridView::thaw`) or the values `morphology::dilate_values` gives. Refused if
    /// the field belongs to another topology.
    pub fn from_parts(
        topology: Topology,
        field: Field<T>,
        rule: Arc<MergeRule<T>>,
    ) -> Result<Self, FieldError> {
        field.check(&topology)?;
        Ok(Self { topology, field, rule })
    }

    /// Its topology, field and merge rule, the grid given up.
    pub fn into_parts(self) -> (Topology, Field<T>, Arc<MergeRule<T>>) {
        (self.topology, self.field, self.rule)
    }

    /// Takes `topology` in place of its own, its field remapped by `remap`, the remap from
    /// its topology to `topology` that a commit or `morphology::dilate` or `erode` gives. If
    /// the remap is not from its topology to `topology`, or the merge rule refuses a merged
    /// tile, the grid is unchanged.
    pub fn replace(&mut self, topology: Topology, remap: &Remap) -> Result<(), FieldError> {
        let field = self.field.remap(remap, &*self.rule)?;
        field.check(&topology)?;
        self.topology = topology;
        self.field = field;
        Ok(())
    }

    /// Its topology.
    pub fn topology(&self) -> &Topology {
        &self.topology
    }

    /// Its field.
    pub fn field(&self) -> &Field<T> {
        &self.field
    }

    /// Writes every leaf in parallel, as `parallel::par_update` does, `f` given the
    /// topology, the leaf's number and its 512 values. The field is written, never replaced,
    /// so that it stays the topology's; as there, a failure undoes nothing.
    pub fn par_update_leaves<E, F>(&mut self, f: F) -> Result<(), E>
    where
        E: Send + From<FieldError>,
        F: Fn(&Topology, LeafNumber, &mut [T; LEAF_VOXELS]) -> Result<(), E> + Sync,
    {
        let t = &self.topology;
        par_update(t, &mut self.field, |n, values| f(t, n, values))
    }

    /// A leaf's values, to write.
    pub fn leaf_mut(&mut self, n: LeafNumber) -> Result<&mut [T; LEAF_VOXELS], FieldError> {
        self.field.leaf_mut(&self.topology, n)
    }

    /// Sets a tile's value.
    pub fn set_tile(&mut self, n: TileNumber, value: T) -> Result<(), FieldError> {
        self.field.set_tile(&self.topology, n, value)
    }

    /// The value at a voxel.
    pub fn value_at(&self, v: Coord) -> Result<T, FieldError> {
        self.field.value_at(&self.topology, v)
    }

    /// Edits the topology with `f`, commits it and remaps the field. If `f`, the commit or
    /// the remap fails, the grid is unchanged.
    ///
    /// Edits whose work fails with an error of the caller's own are made by the steps this
    /// joins, editing the topology's copy, committing it and `replace`, the grid unchanged
    /// as here if any fails (ADR-0059):
    ///
    /// ```
    /// use brodo::error::{FieldError, TreeError};
    /// use brodo::grid::Grid;
    ///
    /// #[derive(Debug)]
    /// enum Error {
    ///     Tree(TreeError),
    ///     Field(FieldError),
    ///     Behind([i32; 3]),
    /// }
    /// impl From<TreeError> for Error {
    ///     fn from(e: TreeError) -> Self {
    ///         Self::Tree(e)
    ///     }
    /// }
    /// impl From<FieldError> for Error {
    ///     fn from(e: FieldError) -> Self {
    ///         Self::Field(e)
    ///     }
    /// }
    ///
    /// /// Activates each voxel, refusing all of them if one lies behind the origin.
    /// fn grow(g: &mut Grid<f32>, at: &[[i32; 3]]) -> Result<(), Error> {
    ///     let mut e = g.topology().edit();
    ///     for v in at {
    ///         if v[0] < 0 {
    ///             return Err(Error::Behind(*v));
    ///         }
    ///         e.activate(*v)?;
    ///     }
    ///     let (t, remap) = e.commit()?;
    ///     g.replace(t, &remap)?;
    ///     Ok(())
    /// }
    ///
    /// let mut g = Grid::new(0.0_f32);
    /// grow(&mut g, &[[0, 0, 0], [8, 0, 0]])?;
    /// assert_eq!(g.topology().leaf_count(), 2);
    /// assert!(matches!(grow(&mut g, &[[16, 0, 0], [-1, 0, 0]]), Err(Error::Behind(_))));
    /// assert_eq!(g.topology().leaf_count(), 2);
    /// # Ok::<(), Error>(())
    /// ```
    pub fn edit<R>(
        &mut self,
        f: impl FnOnce(&mut TopologyEditor<'_>) -> Result<R, TreeError>,
    ) -> Result<R, GridError> {
        let mut editor = self.topology.edit();
        let out = f(&mut editor)?;
        let (topology, remap) = editor.commit()?;
        let field = self.field.remap(&remap, &*self.rule)?;
        self.topology = topology;
        self.field = field;
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::TileLevel;

    #[test]
    fn a_grid_shows_its_topology_and_field() {
        let s = format!("{:?}", Grid::new(1.5_f32));
        assert!(s.starts_with("Grid {") && s.contains("topology:") && s.contains("1.5"), "{s}");
        assert!(s.ends_with(".. }"), "{s}");
    }

    #[test]
    fn a_grid_writes_its_leaves_in_parallel_and_its_tiles() {
        let mut g = Grid::new(0_u32);
        g.edit(|e| {
            e.activate([0, 0, 0])?;
            e.activate([8, 0, 0])?;
            e.add_tile(TileLevel::Lower, [16, 0, 0])
        })
        .unwrap();
        g.par_update_leaves(|t, n, values| {
            let o = t.leaf_origin(n)?;
            values.fill(o[0].cast_unsigned() + 1);
            Ok::<(), GridError>(())
        })
        .unwrap();
        let n = g.topology().tile_numbers().next().unwrap();
        g.set_tile(n, 7).unwrap();
        let leaf = g.topology().leaf_numbers().nth(1).unwrap();
        g.leaf_mut(leaf).unwrap()[1] = 5;
        let at = |v| g.value_at(v).unwrap();
        assert_eq!([at([0, 0, 0]), at([8, 0, 0]), at([9, 0, 0]), at([20, 0, 0])], [1, 9, 5, 7]);
    }

    #[test]
    fn an_edit_keeps_the_values_and_a_refused_one_changes_nothing() {
        let mut g = Grid::new(1.5_f32);
        g.edit(|e| e.add_tile(TileLevel::Lower, [0, 0, 0])).unwrap();
        let n = g.topology().tile_numbers().next().unwrap();
        g.set_tile(n, -2.0).unwrap();
        g.edit(|e| e.split_tile([3, 3, 3])).unwrap();
        assert_eq!(g.topology().leaf_count(), 1);
        assert_eq!(g.value_at([3, 3, 3]).map(f32::to_bits), Ok((-2.0_f32).to_bits()));
        assert_eq!(g.value_at([8, 0, 0]).map(f32::to_bits), Ok(1.5_f32.to_bits()));
        let before = g.topology().leaf_count();
        let refused = g.edit(|e| {
            e.activate([100, 0, 0])?;
            e.split_tile([500, 0, 0])
        });
        assert_eq!(refused, Err(GridError::Tree(TreeError::NotATile([500, 0, 0]))));
        assert_eq!(g.topology().leaf_count(), before);
        assert_eq!(g.value_at([3, 3, 3]).map(f32::to_bits), Ok((-2.0_f32).to_bits()));
    }

    #[test]
    fn a_removed_root_tile_leaves_the_background_and_a_tile_added_again_takes_none_of_it() {
        let mut g = Grid::new(0_u32);
        g.edit(|e| e.add_tile(TileLevel::Root, [0, 0, 0])).unwrap();
        let n = g.topology().tile_numbers().next().unwrap();
        g.set_tile(n, 9).unwrap();
        assert_eq!(g.edit(|e| e.remove_tile([5, 6, 7])), Ok(TileLevel::Root));
        assert_eq!((g.topology().tile_count(), g.topology().leaf_count()), (0, 0));
        assert_eq!(g.value_at([4095, 0, 0]), Ok(0));
        g.edit(|e| e.add_tile(TileLevel::Root, [0, 0, 0])).unwrap();
        g.set_tile(g.topology().tile_numbers().next().unwrap(), 9).unwrap();
        g.edit(|e| {
            e.remove_tile([0, 0, 0])?;
            e.add_tile(TileLevel::Upper, [0, 0, 0])?;
            e.activate([200, 0, 0]).map(|_| ())
        })
        .unwrap();
        assert_eq!([g.value_at([1, 1, 1]), g.value_at([200, 0, 0])], [Ok(0), Ok(0)]);
        let refused = g.edit(|e| e.remove_tile([300, 0, 0]));
        assert_eq!(refused, Err(GridError::Tree(TreeError::NotATile([300, 0, 0]))));
    }

    #[test]
    fn a_grid_is_made_from_its_parts_and_takes_a_remap() {
        let mut g = Grid::new(1_u8);
        g.edit(|e| e.activate([0, 0, 0]).map(|_| ())).unwrap();
        g.leaf_mut(g.topology().leaf_numbers().next().unwrap()).unwrap()[0] = 5;
        let (t, f, rule) = g.clone().into_parts();
        let again = Grid::from_parts(t.clone(), f.clone(), Arc::clone(&rule)).unwrap();
        assert_eq!(again.value_at([0, 0, 0]), Ok(5));
        let other = Topology::new();
        assert_eq!(
            Grid::from_parts(other, f.clone(), Arc::clone(&rule)).err(),
            Some(FieldError::StaleField)
        );
        // A dilation's remap from the grid's topology: the values follow it.
        let (bigger, remap) =
            crate::morphology::dilate(&t, crate::analysis::Connectivity::Six, 1).unwrap();
        assert!(bigger.leaf_count() > 1);
        let want = f.remap(&remap, &*rule).unwrap();
        g.replace(bigger.clone(), &remap).unwrap();
        for v in [[0, 0, 0], [8, 0, 0], [100, 0, 0]] {
            assert_eq!(g.value_at(v), want.value_at(&bigger, v));
        }
        // A remap from another topology changes nothing.
        let before = g.topology().leaf_count();
        assert_eq!(g.replace(Topology::new(), &remap), Err(FieldError::StaleField));
        assert_eq!(g.topology().leaf_count(), before);
    }
}
