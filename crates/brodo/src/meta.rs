//! A grid's metadata and bounding box (ARCHITECTURE.md §4.7, ADR-0016): what a file stores
//! beside a grid's topology and values.

use std::collections::BTreeMap;

use crate::error::{MetaError, TreeError};
use crate::topology::Topology;
use crate::tree::Coord;

/// What a file stores of a grid beside its topology and values.
#[derive(Debug, Clone, PartialEq)]
pub struct GridMeta {
    /// The grid's name.
    pub name: String,
    /// A voxel's size along x, y and z, in `unit`.
    pub voxel_size: [f64; 3],
    /// The unit of length, a non-empty string brodo does not interpret.
    pub unit: String,
    /// From voxel to world coordinates: the rows of a 3×4 affine map.
    pub transform: [[f64; 4]; 3],
    /// Free string pairs.
    pub extra: BTreeMap<String, String>,
}

impl GridMeta {
    /// Metadata whose transform scales by the voxel size, with no rotation or translation.
    pub fn new(name: &str, voxel_size: [f64; 3], unit: &str) -> Result<Self, MetaError> {
        let [x, y, z] = voxel_size;
        let meta = Self {
            name: name.to_owned(),
            voxel_size,
            unit: unit.to_owned(),
            transform: [[x, 0.0, 0.0, 0.0], [0.0, y, 0.0, 0.0], [0.0, 0.0, z, 0.0]],
            extra: BTreeMap::new(),
        };
        meta.check()?;
        Ok(meta)
    }

    /// Refuses an empty unit, and a voxel size or transform entry that is not finite or,
    /// for the size, not positive.
    pub fn check(&self) -> Result<(), MetaError> {
        if self.unit.is_empty() {
            return Err(MetaError::EmptyUnit);
        }
        if !self.voxel_size.iter().all(|s| s.is_finite() && *s > 0.0) {
            return Err(MetaError::BadVoxelSize);
        }
        if !self.transform.iter().flatten().all(|t| t.is_finite()) {
            return Err(MetaError::BadTransform);
        }
        Ok(())
    }
}

/// The voxels a grid's leaves and tiles cover.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bounds {
    /// No leaf or tile: every voxel is the background.
    Empty,
    /// The smallest and largest voxel covered, inclusive.
    Box {
        /// The smallest voxel along each axis.
        min: Coord,
        /// The largest voxel along each axis.
        max: Coord,
    },
}

impl Bounds {
    /// What the topology's leaves and tiles cover.
    pub fn of(t: &Topology) -> Result<Bounds, TreeError> {
        let mut cubes = Vec::new();
        for n in t.leaf_numbers() {
            cubes.push((t.leaf_origin(n)?, 7));
        }
        for n in t.tile_numbers() {
            let (level, o) = t.tile(n)?;
            cubes.push((o, (1 << level.edge_log2()) - 1));
        }
        Ok(cubes.into_iter().fold(Bounds::Empty, |b, (o, e)| {
            let far = o.map(|c| c + e);
            match b {
                Bounds::Empty => Bounds::Box { min: o, max: far },
                Bounds::Box { min, max } => Bounds::Box {
                    min: [0, 1, 2].map(|k| min[k].min(o[k])),
                    max: [0, 1, 2].map(|k| max[k].max(far[k])),
                },
            }
        }))
    }
}
