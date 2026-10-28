//! A grid: one topology and one field on it, for single-field use (ARCHITECTURE.md §4.2).
//! Editing a grid commits its topology and remaps its field in one step, so its field is
//! never stale.

use std::sync::Arc;

use crate::error::{FieldError, TreeError};
use crate::field::{Field, LEAF_VOXELS, MergeRule, Value, uniform};
use crate::parallel::par_update;
use crate::topology::{LeafNumber, TileNumber, Topology, TopologyEditor};
use crate::tree::Coord;

/// Why an edit of a grid was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum GridError {
    /// The edit of the topology was refused.
    #[error(transparent)]
    Tree(#[from] TreeError),
    /// The field could not be remapped.
    #[error(transparent)]
    Field(#[from] FieldError),
}

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
}
