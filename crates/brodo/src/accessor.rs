//! Value accessors (ARCHITECTURE.md §4.4, ADR-0015): reads and writes by voxel that start
//! from the regions last visited, so nearby accesses skip most of the tree. One per thread.

use crate::error::FieldError;
use crate::field::{Field, Value};
use crate::topology::{Location, Path, Topology};
use crate::tree::Coord;

/// Reads a field by voxel. The field's stamp is checked when the accessor is made; the
/// borrows keep topology and field unchanged after.
#[derive(Debug)]
pub struct ValueAccessor<'a, T> {
    topology: &'a Topology,
    field: &'a Field<T>,
    path: Path,
}

impl<'a, T: Value> ValueAccessor<'a, T> {
    /// An accessor for `field`, which must belong to `topology`.
    pub fn new(topology: &'a Topology, field: &'a Field<T>) -> Result<Self, FieldError> {
        field.belongs_to(topology)?;
        Ok(Self { topology, field, path: Path::default() })
    }

    /// What holds the voxel.
    pub fn locate(&mut self, v: Coord) -> Location {
        self.topology.locate_on(&mut self.path, v)
    }

    /// The value at the voxel: its leaf's, its tile's, or the background.
    pub fn get(&mut self, v: Coord) -> T {
        let loc = self.locate(v);
        self.field.at(loc, v)
    }
    /// Lookups answered from the leaf, lower, upper level, and from the root.
    #[cfg(test)]
    fn hits(&self) -> [u64; 4] {
        self.path.hits
    }
}

/// Reads and writes a field by voxel; writes go to leaves only (ADR-0015).
#[derive(Debug)]
pub struct ValueAccessorMut<'a, T> {
    topology: &'a Topology,
    field: &'a mut Field<T>,
    path: Path,
}

impl<'a, T: Value> ValueAccessorMut<'a, T> {
    /// An accessor for `field`, which must belong to `topology`.
    pub fn new(topology: &'a Topology, field: &'a mut Field<T>) -> Result<Self, FieldError> {
        field.belongs_to(topology)?;
        Ok(Self { topology, field, path: Path::default() })
    }

    /// What holds the voxel.
    pub fn locate(&mut self, v: Coord) -> Location {
        self.topology.locate_on(&mut self.path, v)
    }

    /// The value at the voxel.
    pub fn get(&mut self, v: Coord) -> T {
        let loc = self.locate(v);
        self.field.at(loc, v)
    }

    /// Sets the value at a voxel of a leaf; anywhere else is refused, naming what holds it.
    pub fn set(&mut self, v: Coord, value: T) -> Result<(), FieldError> {
        match self.locate(v) {
            Location::Leaf(n) => {
                *self.field.at_mut(n, v) = value;
                Ok(())
            }
            location @ (Location::Tile(..) | Location::Background) => {
                Err(FieldError::NotALeaf { voxel: v, location })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;
    use crate::topology::TopologyEditor;
    use crate::tree::TileLevel;

    fn topology(edit: impl FnOnce(&mut TopologyEditor<'_>)) -> Topology {
        let empty = Topology::new();
        let mut e = empty.edit();
        edit(&mut e);
        e.commit().unwrap().0
    }

    /// Distinct values everywhere: each leaf voxel and each tile its own number.
    fn numbered(t: &Topology) -> Field<u32> {
        let mut f = Field::new(t, u32::MAX);
        let mut k = 0;
        for n in t.leaf_numbers() {
            for v in f.leaf_mut(t, n).unwrap() {
                *v = k;
                k += 1;
            }
        }
        for n in t.tile_numbers() {
            f.set_tile(t, n, k).unwrap();
            k += 1;
        }
        f
    }

    /// The index of a voxel in its leaf, worked out here.
    fn index(v: Coord) -> usize {
        let [x, y, z] = v.map(|c| usize::try_from(c.rem_euclid(8)).unwrap());
        x + 8 * y + 64 * z
    }

    /// The value at a voxel without any accessor: located from the root, read by index.
    fn read(t: &Topology, f: &Field<u32>, v: Coord) -> u32 {
        match t.locate(v) {
            Location::Leaf(n) => f.leaf(t, n).unwrap()[index(v)],
            Location::Tile(n, _) => f.tile(t, n).unwrap(),
            Location::Background => f.background(),
        }
    }

    #[test]
    fn each_level_of_the_path_answers_in_turn() {
        let t = topology(|e| {
            for v in [[0, 0, 0], [8, 0, 0], [200, 0, 0], [5000, 0, 0]] {
                e.activate(v).unwrap();
            }
        });
        let f = numbered(&t);
        let mut a = ValueAccessor::new(&t, &f).unwrap();
        // From the root, then the same leaf, the next leaf of the same lower node, another
        // lower node of the same upper node, and another upper node.
        for v in [[1, 1, 1], [2, 3, 4], [9, 0, 0], [201, 0, 0], [5001, 0, 0]] {
            assert_eq!(a.get(v), read(&t, &f, v), "at {v:?}");
        }
        assert_eq!(a.hits(), [1, 1, 1, 2]);
    }

    #[test]
    fn writes_go_to_leaves_only_and_a_stale_field_is_refused() {
        let t = topology(|e| {
            e.activate([0, 0, 0]).unwrap();
            e.add_tile(TileLevel::Lower, [8, 0, 0]).unwrap();
        });
        let mut f = numbered(&t);
        let mut a = ValueAccessorMut::new(&t, &mut f).unwrap();
        a.set([3, 4, 5], 7).unwrap();
        assert_eq!(a.get([3, 4, 5]), 7);
        let tile = t.locate([9, 0, 0]);
        assert_eq!(
            a.set([9, 0, 0], 1),
            Err(FieldError::NotALeaf { voxel: [9, 0, 0], location: tile })
        );
        let bg = Location::Background;
        assert_eq!(
            a.set([-1, 0, 0], 1),
            Err(FieldError::NotALeaf { voxel: [-1, 0, 0], location: bg })
        );
        let (t2, _) = t.edit().commit().unwrap();
        assert_eq!(ValueAccessor::new(&t2, &f).map(|_| ()), Err(FieldError::StaleField));
    }

    #[derive(Debug, Clone, Copy)]
    enum Move {
        /// Up to two voxels along each axis.
        Near([i32; 3]),
        /// One leaf along one axis.
        Leaf(usize, i32),
        /// To a shared coordinate, moved a little.
        Far(usize, [i32; 3]),
        /// To where a tile may be, near a shared coordinate less 300.
        Tile(usize, [i32; 3]),
    }

    fn coord() -> impl Strategy<Value = Coord> {
        let anchors = vec![-4097, -4096, -129, -128, -9, -8, -1, 0, 7, 8, 127, 128, 4095, 4096];
        let axis = (prop::sample::select(anchors), 0..3_i32).prop_map(|(a, d)| a + d);
        [axis.clone(), axis.clone(), axis]
    }

    fn moves() -> impl Strategy<Value = Vec<Move>> {
        let m = prop_oneof![
            6 => [-2..=2_i32, -2..=2, -2..=2].prop_map(Move::Near),
            3 => (0..3_usize, prop_oneof![Just(-8), Just(8)]).prop_map(|(a, s)| Move::Leaf(a, s)),
            1 => (0..4_usize, [0..24_i32, 0..24, 0..24]).prop_map(|(i, d)| Move::Far(i, d)),
            1 => (0..4_usize, [0..8_i32, 0..8, 0..8]).prop_map(|(i, d)| Move::Tile(i, d)),
        ];
        prop::collection::vec(m, 1..200)
    }

    /// Leaves around the shared coordinates, and tiles at each level among them.
    fn build(pool: &[Coord], tiles: &[(TileLevel, usize)]) -> Topology {
        topology(|e| {
            for p in pool {
                for i in 0..27 {
                    let d = [i % 3, i / 3 % 3, i / 9].map(|c| c * 8);
                    e.activate([p[0] + d[0], p[1] + d[1], p[2] + d[2]]).unwrap();
                }
            }
            for (level, i) in tiles {
                let _occupied = e.add_tile(*level, pool[*i].map(|c| c - 300));
            }
        })
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(128))]

        #[test]
        fn accessors_read_and_write_as_the_field_does(
            pool in prop::collection::vec(coord(), 4),
            tiles in prop::collection::vec((prop_oneof![Just(TileLevel::Upper), Just(TileLevel::Lower)], 0..4_usize), 1..5),
            moves in moves(),
        ) {
            let t = build(&pool, &tiles);
            let f = numbered(&t);
            let mut want = f.clone();
            let mut got = f.clone();
            let mut r = ValueAccessor::new(&t, &f).unwrap();
            let mut w = ValueAccessorMut::new(&t, &mut got).unwrap();
            let mut v = pool[0];
            for (k, m) in (0_u32..).zip(&moves) {
                v = match *m {
                    Move::Near(d) => [v[0] + d[0], v[1] + d[1], v[2] + d[2]],
                    Move::Leaf(a, s) => { let mut x = v; x[a] += s; x }
                    Move::Far(i, d) => [pool[i][0] + d[0], pool[i][1] + d[1], pool[i][2] + d[2]],
                    Move::Tile(i, d) => [pool[i][0] - 300 + d[0], pool[i][1] - 300 + d[1], pool[i][2] - 300 + d[2]],
                };
                prop_assert_eq!(r.locate(v), t.locate(v), "at {:?}", v);
                prop_assert_eq!(r.get(v), read(&t, &f, v), "at {:?}", v);
                let value = 1_000_000 + k;
                match (w.set(v, value), t.locate(v)) {
                    (Ok(()), Location::Leaf(n)) => want.leaf_mut(&t, n).unwrap()[index(v)] = value,
                    (Err(FieldError::NotALeaf { location, .. }), at) => prop_assert_eq!(location, at),
                    (got, at) => prop_assert!(false, "{:?} at {:?}", got, at),
                }
                prop_assert_eq!(w.get(v), read(&t, &want, v));
            }
            for n in t.leaf_numbers() {
                prop_assert_eq!(got.leaf(&t, n).unwrap(), want.leaf(&t, n).unwrap());
            }
        }
    }
}
