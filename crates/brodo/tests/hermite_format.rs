//! Format tier (TESTING.md §1) for Hermite data (ADR-0032): a set written to a cache file
//! reads back with the same bits and the same group table; the committed file keeps its
//! bytes; each rule of the format, broken alone, is refused for that rule, in memory and
//! from a file; a file's set is found by its keys and refused when they do not fit.

mod timed;

use std::collections::BTreeMap;

use brodo::cache::{CacheEntry, CacheFile, to_bytes};
use brodo::error::{HermiteError, Rule};
use brodo::field::Field;
use brodo::hermite::{Crossing, Crossings, Hermite, HermiteView};
use brodo::tree::Axis as EdgeAxis;

/// The fraction, normal and group a dense field of this test holds where an edge holds no
/// crossing.
const NO_CROSSING: (f32, [f32; 3], u32) = (-1.0, [0.0; 3], 0);
use brodo::meta::{GridMeta, SDF_KEY, SDF_VERSION};
use brodo::topology::{Location, Topology};
use brodo::tree::{Coord, TileLevel};

/// One axis's crossings as dense fields, as `Hermite::expand` gives them: the parts a test
/// breaks.
struct Axis {
    fraction: Field<f32>,
    normal: Field<[f32; 3]>,
    group: Field<u32>,
}

/// A set's parts before they are checked.
struct Parts {
    topology: Topology,
    signs: Field<u8>,
    axes: [Axis; 3],
    names: Vec<String>,
}

/// Whether a dense value is `NO_CROSSING`'s, by bits.
fn is_none(c: (f32, [f32; 3], u32)) -> bool {
    let bits = |x: [f32; 3]| x.map(f32::to_bits);
    c.0.to_bits() == NO_CROSSING.0.to_bits()
        && bits(c.1) == bits(NO_CROSSING.1)
        && c.2 == NO_CROSSING.2
}

impl Parts {
    /// Each leaf's crossings on each axis, by place: every leaf value other than
    /// `NO_CROSSING`'s, as a set held sparsely keeps them; tiles' and backgrounds' values
    /// have no place there.
    fn crossings(&self) -> Vec<[Vec<(usize, Crossing)>; 3]> {
        let t = &self.topology;
        t.leaf_numbers()
            .map(|n| {
                self.axes.each_ref().map(|a| {
                    let (f, v, g) = (
                        a.fraction.leaf(t, n).unwrap(),
                        a.normal.leaf(t, n).unwrap(),
                        a.group.leaf(t, n).unwrap(),
                    );
                    (0..512)
                        .filter(|&i| !is_none((f[i], v[i], g[i])))
                        .map(|i| (i, Crossing { fraction: f[i], normal: v[i], group: g[i] }))
                        .collect()
                })
            })
            .collect()
    }

    fn sparse(&self) -> Crossings {
        let mut b = Crossings::builder(&self.topology);
        for (n, [x, y, z]) in self.topology.leaf_numbers().zip(self.crossings()) {
            b.leaf(n, [&x, &y, &z]).unwrap();
        }
        b.build().unwrap()
    }

    fn make(self) -> Result<Hermite, HermiteError> {
        let crossings = self.sparse();
        Hermite::new(self.topology, self.signs, crossings, self.names)
    }
}

/// Crossings on `t` holding no crossing anywhere.
fn none(t: &Topology) -> Axis {
    Axis {
        fraction: Field::new(t, NO_CROSSING.0),
        normal: Field::new(t, NO_CROSSING.1),
        group: Field::new(t, NO_CROSSING.2),
    }
}

/// Leaves covering [0, 8·leaves)³, but for a lower tile at `tile`, if any.
fn topology(leaves: i32, tile: Option<Coord>) -> Topology {
    let empty = Topology::new();
    let mut e = empty.edit();
    for x in 0..leaves {
        for y in 0..leaves {
            for z in 0..leaves {
                let o = [8 * x, 8 * y, 8 * z];
                if Some(o) != tile {
                    e.activate(o).unwrap();
                }
            }
        }
    }
    if let Some(o) = tile {
        e.add_tile(TileLevel::Lower, o).unwrap();
    }
    e.commit().unwrap().0
}

fn step(axis: usize) -> Coord {
    let mut e = [0; 3];
    e[axis] = 1;
    e
}

fn plus(v: Coord, d: Coord) -> Coord {
    [v[0] + d[0], v[1] + d[1], v[2] + d[2]]
}

/// A valid set: inside the box [lo, hi)³, its crossings at the middle of each crossed
/// edge, normals along the edge, x's edges in group 1, y's in 2, z's in 0.
fn parts(leaves: i32, tile: Option<Coord>, lo: i32, hi: i32) -> Parts {
    let topology = topology(leaves, tile);
    let inside = |v: Coord| v.iter().all(|c| (lo..hi).contains(c));
    let mut signs = Field::new(&topology, 0_u8);
    let mut axes = [none(&topology), none(&topology), none(&topology)];
    for n in topology.leaf_numbers() {
        let voxels: Vec<Coord> = topology.leaf_voxels(n).unwrap().collect();
        for (s, v) in signs.leaf_mut(&topology, n).unwrap().iter_mut().zip(&voxels) {
            *s = u8::from(inside(*v));
        }
        for (axis, c) in axes.iter_mut().enumerate() {
            for (i, v) in voxels.iter().enumerate() {
                if inside(*v) == inside(plus(*v, step(axis))) {
                    continue;
                }
                let out = if inside(*v) { 1.0 } else { -1.0 };
                let mut normal = [0.0_f32; 3];
                normal[axis] = out;
                c.fraction.leaf_mut(&topology, n).unwrap()[i] = 0.5;
                c.normal.leaf_mut(&topology, n).unwrap()[i] = normal;
                c.group.leaf_mut(&topology, n).unwrap()[i] = [1, 2, 0][axis];
            }
        }
    }
    for t in topology.tile_numbers() {
        let (_, o) = topology.tile(t).unwrap();
        signs.set_tile(&topology, t, u8::from(inside(o))).unwrap();
    }
    let names = vec![String::new(), "a".into(), "b".into()];
    Parts { topology, signs, axes, names }
}

/// The set the tests break: 26 leaves round a lower tile, inside [4, 20)³.
fn valid() -> Parts {
    parts(3, Some([8, 8, 8]), 4, 20)
}

fn leaf_of(t: &Topology, v: Coord) -> brodo::topology::LeafNumber {
    match t.locate(v) {
        Location::Leaf(n) => n,
        other @ (Location::Tile(..) | Location::Background) => panic!("{v:?} is in {other:?}"),
    }
}

fn index(v: Coord) -> usize {
    let [x, y, z] = v.map(|c| usize::try_from(c & 7).unwrap());
    x + 8 * y + 64 * z
}

/// One value of a set, changed.
#[derive(Clone, Copy)]
enum Edit {
    Sign(Coord, u8),
    Fraction(usize, Coord, f32),
    Normal(usize, Coord, [f32; 3]),
    Group(usize, Coord, u32),
}

fn apply(p: &mut Parts, e: Edit) {
    let t = &p.topology;
    match e {
        Edit::Sign(v, s) => p.signs.leaf_mut(t, leaf_of(t, v)).unwrap()[index(v)] = s,
        Edit::Fraction(a, v, f) => {
            p.axes[a].fraction.leaf_mut(t, leaf_of(t, v)).unwrap()[index(v)] = f
        }
        Edit::Normal(a, v, n) => p.axes[a].normal.leaf_mut(t, leaf_of(t, v)).unwrap()[index(v)] = n,
        Edit::Group(a, v, g) => p.axes[a].group.leaf_mut(t, leaf_of(t, v)).unwrap()[index(v)] = g,
    }
}

fn meta(name: &str) -> GridMeta {
    GridMeta::new(name, [0.5; 3], "millimeter").unwrap()
}

/// The rule a set breaks, or what was made instead of a violation.
fn rule<T>(r: Result<T, HermiteError>) -> Result<(Option<Coord>, Option<usize>, Rule), String> {
    match r {
        Err(HermiteError::Violation { at, axis, rule }) => {
            Ok((at, axis.map(EdgeAxis::index), rule))
        }
        Err(other) => Err(format!("{other:?}")),
        Ok(_) => Err("a set".to_owned()),
    }
}

#[test]
fn a_valid_set_is_taken() {
    let set = valid().make();
    assert!(set.as_ref().is_ok_and(|s| s.group_names() == ["", "a", "b"]), "{set:?}");
    // Crossings on edges from one leaf into the next, at 15 → 16.
    let across = parts(3, None, 4, 16).make();
    assert!(across.is_ok(), "{across:?}");
    // An outside tile at the leaves' edge, next to the background of its own sign.
    let edge = parts(2, Some([8, 8, 8]), 2, 6).make();
    assert!(edge.is_ok(), "{edge:?}");
}

/// An inside upper tile wrapped in a shell of leaves, inside to 135 and from −7, but for one
/// leaf of the shell left out by the tile's +x face: only the tile's face, at the patch
/// (y, z) = (64, 32), meets the background there, and it is reported, not the leaves'
/// own changes beside the hole.
fn shell_with_hole() -> Parts {
    let (tile, hole) = ([0, 0, 0], [128, 64, 32]);
    let empty = Topology::new();
    let mut e = empty.edit();
    for x in -1..17 {
        for y in -1..17 {
            for z in -1..17 {
                let o = [8 * x, 8 * y, 8 * z];
                let in_tile = o.iter().all(|c| (0..128).contains(c));
                if !in_tile && o != hole {
                    e.activate(o).unwrap();
                }
            }
        }
    }
    e.add_tile(TileLevel::Upper, tile).unwrap();
    let topology = e.commit().unwrap().0;
    let inside = |v: Coord| v.iter().all(|c| (-7..136).contains(c));
    let mut signs = Field::new(&topology, 0_u8);
    let mut axes = [none(&topology), none(&topology), none(&topology)];
    for n in topology.leaf_numbers() {
        let voxels: Vec<Coord> = topology.leaf_voxels(n).unwrap().collect();
        for (s, v) in signs.leaf_mut(&topology, n).unwrap().iter_mut().zip(&voxels) {
            *s = u8::from(inside(*v));
        }
        for (axis, c) in axes.iter_mut().enumerate() {
            for (i, v) in voxels.iter().enumerate() {
                if inside(*v) == inside(plus(*v, step(axis))) {
                    continue;
                }
                let mut normal = [0.0_f32; 3];
                normal[axis] = if inside(*v) { 1.0 } else { -1.0 };
                c.fraction.leaf_mut(&topology, n).unwrap()[i] = 0.5;
                c.normal.leaf_mut(&topology, n).unwrap()[i] = normal;
            }
        }
    }
    let t = topology.tile_numbers().next().unwrap();
    signs.set_tile(&topology, t, 1).unwrap();
    Parts { topology, signs, axes, names: Vec::new() }
}

/// A set on leaves at `origins`, inside where `inside` says, its crossings as `parts`
/// makes them; a point beyond the `i32` range is outside, as the format takes it.
fn parts_at(origins: &[Coord], inside: impl Fn(Coord) -> bool) -> Parts {
    let empty = Topology::new();
    let mut e = empty.edit();
    for o in origins {
        e.activate(*o).unwrap();
    }
    let topology = e.commit().unwrap().0;
    let next = |v: Coord, axis: usize| {
        let mut w = v;
        w[axis] = v[axis].checked_add(1)?;
        Some(w)
    };
    let inside_at = |v: Option<Coord>| v.is_some_and(&inside);
    let mut signs = Field::new(&topology, 0_u8);
    let mut axes = [none(&topology), none(&topology), none(&topology)];
    for n in topology.leaf_numbers() {
        let voxels: Vec<Coord> = topology.leaf_voxels(n).unwrap().collect();
        for (s, v) in signs.leaf_mut(&topology, n).unwrap().iter_mut().zip(&voxels) {
            *s = u8::from(inside(*v));
        }
        for (axis, c) in axes.iter_mut().enumerate() {
            for (i, v) in voxels.iter().enumerate() {
                if inside(*v) == inside_at(next(*v, axis)) {
                    continue;
                }
                let mut normal = [0.0_f32; 3];
                normal[axis] = if inside(*v) { 1.0 } else { -1.0 };
                c.fraction.leaf_mut(&topology, n).unwrap()[i] = 0.5;
                c.normal.leaf_mut(&topology, n).unwrap()[i] = normal;
            }
        }
    }
    Parts { topology, signs, axes, names: Vec::new() }
}

#[test]
fn beyond_the_i32_range_lies_the_background() {
    // Leaves at both ends of the x range, one point inside at x = i32::MIN. The point
    // before it along x lies beyond the range, the background, sign 0, in no leaf: the
    // edge from it crosses a surface no leaf point owns, and the set is refused there. Had
    // the range wrapped, that point would be i32::MAX, in the other leaf.
    let (low, high) = ([i32::MIN, 0, 0], [i32::MAX - 7, 0, 0]);
    let point = [i32::MIN, 3, 3];
    let p = parts_at(&[low, high], |v| v == point);
    assert_eq!(rule(p.make()), Ok((Some(point), Some(0), Rule::Owner)));
    // Inside at x = i32::MAX instead: the edge beyond it is its own, and holds its crossing.
    let point = [i32::MAX, 3, 3];
    let p = parts_at(&[low, high], |v| v == point);
    assert!(p.make().is_ok());
}

#[test]
fn a_change_on_a_far_patch_of_a_tiles_face_is_found_there() {
    assert_eq!(rule(shell_with_hole().make()), Ok((Some([127, 64, 32]), Some(0), Rule::Owner)));
}

/// Tiles of the given levels, origins and signs, on no leaf, with no crossing.
fn tiles(tiles: &[(TileLevel, Coord, u8)]) -> Parts {
    let empty = Topology::new();
    let mut e = empty.edit();
    for (level, o, _) in tiles {
        e.add_tile(*level, *o).unwrap();
    }
    let topology = e.commit().unwrap().0;
    let mut signs = Field::new(&topology, 0_u8);
    for t in topology.tile_numbers() {
        let (level, o) = topology.tile(t).unwrap();
        let sign = tiles.iter().find(|(l, p, _)| (*l, *p) == (level, o)).unwrap().2;
        signs.set_tile(&topology, t, sign).unwrap();
    }
    let axes = [none(&topology), none(&topology), none(&topology)];
    Parts { topology, signs, axes, names: Vec::new() }
}

/// The sign of `tiles` at `v`, the background's, 0, in none of them and beyond the range.
fn sign_at(tiles: &[(TileLevel, Coord, u8)], v: [i64; 3]) -> u8 {
    let holds = |(level, o, _): &&(TileLevel, Coord, u8)| {
        (0..3).all(|k| (0..i64::from(level.edge())).contains(&(v[k] - i64::from(o[k]))))
    };
    tiles.iter().find(holds).map_or(0, |t| t.2)
}

/// That `tiles`, on no leaf, are refused by the owner rule (ARCHITECTURE.md §4.9): the
/// error names a point and an axis on one of whose edges along it, to the next point or
/// from the one before, the signs differ. Which of several such edges it names is not
/// promised.
fn refused_at_a_change(tiles: &[(TileLevel, Coord, u8)]) {
    let named = rule(self::tiles(tiles).make());
    let Ok((Some(at), Some(axis), Rule::Owner)) = named else { panic!("{named:?}") };
    let at = at.map(i64::from);
    let step = |d: i64| {
        let mut v = at;
        v[axis] += d;
        sign_at(tiles, v)
    };
    assert!(step(-1) != step(0) || step(0) != step(1), "{named:?}");
}

/// An outside root tile, and beside its +x face, in an upper node, an inside lower tile:
/// the signs change across the lower tile's faces, and the set is refused at one.
#[test]
fn a_root_tiles_face_reaches_the_tiles_of_the_node_beside_it() {
    refused_at_a_change(&[(TileLevel::Root, [0; 3], 0), (TileLevel::Lower, [4096, 136, 8], 1)]);
}

/// Tiles at the ends of the `i32` range (the final mutation run of P5): beyond them lies
/// the background, sign 0, in no leaf (ARCHITECTURE.md §4.9), so an outside tile there is
/// taken and an inside one refused at a change of sign.
#[test]
fn a_tile_at_an_end_of_the_range_meets_the_background_beyond() {
    let (low, high) = ([i32::MIN, 0, 0], [i32::MAX - 7, 0, 0]);
    for o in [low, high] {
        assert!(tiles(&[(TileLevel::Lower, o, 0)]).make().is_ok(), "{o:?}");
        refused_at_a_change(&[(TileLevel::Lower, o, 1)]);
    }
    refused_at_a_change(&[(TileLevel::Lower, low, 1), (TileLevel::Lower, [i32::MIN + 8, 0, 0], 1)]);
}

/// Two inside tiles side by side, the background around them: refused at a change of sign.
#[test]
fn inside_tiles_side_by_side_are_refused_at_a_change() {
    refused_at_a_change(&[(TileLevel::Lower, [8, 0, 0], 1), (TileLevel::Lower, [16, 0, 0], 1)]);
}

#[test]
fn a_tiles_faces_cost_what_the_nodes_beside_them_hold() {
    // 4096 root tiles, each face 4096² voxels beside another tile or the background: each
    // face is checked once, not patch by patch.
    let mut all = Vec::new();
    for x in 0..16 {
        for y in 0..16 {
            for z in 0..16 {
                all.push((TileLevel::Root, [4096 * x, 4096 * y, 4096 * z], 0));
            }
        }
    }
    assert!(timed::in_time(move || tiles(&all).make().is_ok()));
}

#[test]
fn each_broken_value_is_refused_for_its_rule() {
    // A crossed edge: point 19 inside, 20 outside, on x; and a point deep inside, whose
    // edges cross nothing.
    let (c, i) = ([19, 10, 10], [5, 5, 5]);
    let cases = [
        ("a sign of 2", Edit::Sign(i, 2), (Some(i), None, Rule::Sign)),
        (
            "a crossing where none is",
            Edit::Fraction(1, i, 0.5),
            (Some(i), Some(1), Rule::NoCrossing),
        ),
        (
            "an inward normal, the upper end inside",
            Edit::Normal(0, [3, 10, 10], [0.6, 0.8, 0.0]),
            (Some([3, 10, 10]), Some(0), Rule::NormalInward),
        ),
        ("a fraction above 1", Edit::Fraction(0, c, 1.5), (Some(c), Some(0), Rule::Fraction)),
        ("a fraction below 0", Edit::Fraction(0, c, -0.5), (Some(c), Some(0), Rule::Fraction)),
        ("a NaN fraction", Edit::Fraction(0, c, f32::NAN), (Some(c), Some(0), Rule::Fraction)),
        (
            "a short normal",
            Edit::Normal(0, c, [0.999_99, 0.0, 0.0]),
            (Some(c), Some(0), Rule::NormalLength),
        ),
        (
            "an inward normal",
            Edit::Normal(0, c, [-0.6, 0.8, 0.0]),
            (Some(c), Some(0), Rule::NormalInward),
        ),
        ("an unnamed group", Edit::Group(0, c, 3), (Some(c), Some(0), Rule::UnnamedGroup)),
    ];
    for (name, edit, want) in cases {
        let mut p = valid();
        apply(&mut p, edit);
        assert_eq!(rule(p.make()), Ok(want), "{name}");
    }
    // Inside reaching the leaves' lower faces: the points below them, in the background,
    // own edges whose ends differ.
    assert_eq!(
        rule(parts(3, Some([8, 8, 8]), 0, 20).make()),
        Ok((Some([-1, 0, 0]), Some(0), Rule::Owner))
    );
    // The tile turned outside inside the box: the leaf point before it owns a crossing it
    // lacks, an edge whose ends differ holding none.
    let mut p = valid();
    let t = p.topology.tile_numbers().next().unwrap();
    p.signs.set_tile(&p.topology, t, 0).unwrap();
    assert_eq!(rule(p.make()).map(|r| r.2), Ok(Rule::NoCrossing));
}

/// A normal perpendicular to its edge points neither way and is taken, whichever end is
/// inside: 19 → 20 has its lower end inside, 3 → 4 its upper.
#[test]
fn a_normal_perpendicular_to_its_edge_is_taken() {
    for v in [[19, 10, 10], [3, 10, 10]] {
        let mut p = valid();
        apply(&mut p, Edit::Normal(0, v, [0.0, 1.0, 0.0]));
        assert!(p.make().is_ok(), "at {v:?}");
    }
}

/// A tile's sign and the signs' background are 0 or 1, and the background 0 (docs/formats.md
/// §4.2); a set holds no crossing on a tile or the background, which have no place in it.
#[test]
fn a_tiles_or_the_backgrounds_sign_out_of_range_is_refused() {
    let mut p = valid();
    let t = p.topology.tile_numbers().next().unwrap();
    p.signs.set_tile(&p.topology, t, 2).unwrap();
    assert_eq!(rule(p.make()), Ok((Some([8, 8, 8]), None, Rule::Sign)));
    let mut p = valid();
    p.signs = Field::new(&p.topology, 1);
    assert_eq!(rule(p.make()), Ok((None, None, Rule::Sign)));
}

/// One inside tile and no leaf: its faces own edges into the outside background.
#[test]
fn a_tile_face_owning_a_change_is_refused() {
    refused_at_a_change(&[(TileLevel::Lower, [8, 0, 0], 1)]);
}

#[test]
fn group_tables_and_topologies_that_do_not_fit_are_refused() {
    for (names, bad) in [(vec!["x"], 0), (vec!["", "a", ""], 2), (vec!["", "a", "a"], 2)] {
        let mut p = valid();
        p.names = names.into_iter().map(String::from).collect();
        assert!(matches!(p.make(), Err(HermiteError::BadGroupName(b)) if b == bad));
    }
    let p = valid();
    let other = topology(3, Some([8, 8, 8]));
    let crossings = Crossings::builder(&other).build();
    assert!(matches!(crossings, Err(HermiteError::Build("a leaf left out"))));
    let mut b = Crossings::builder(&other);
    for n in other.leaf_numbers() {
        b.leaf(n, [&[], &[], &[]]).unwrap();
    }
    let made = Hermite::new(p.topology, p.signs, b.build().unwrap(), p.names);
    assert!(matches!(made, Err(HermiteError::OtherTopology("crossings"))));
    let mut p = valid();
    p.signs = Field::new(&other, 0);
    assert!(matches!(p.make(), Err(HermiteError::OtherTopology("sign"))));
}

/// The builder takes leaves in number order, each once, and places below 512 in order on
/// each axis; anything else is refused, named.
#[test]
fn the_builder_refuses_crossings_out_of_its_order() {
    let t = topology(2, None);
    let n: Vec<_> = t.leaf_numbers().collect();
    let c = Crossing { fraction: 0.5, normal: [1.0, 0.0, 0.0], group: 0 };
    let refused = |e: Result<(), HermiteError>| match e {
        Err(HermiteError::Build(what)) => what,
        other => panic!("{other:?}"),
    };
    let mut b = Crossings::builder(&t);
    assert_eq!(refused(b.leaf(n[1], [&[], &[], &[]])), "a leaf out of number order");
    assert_eq!(refused(b.leaf(n[0], [&[(512, c)], &[], &[]])), "a place");
    let mut b = Crossings::builder(&t);
    assert_eq!(refused(b.leaf(n[0], [&[], &[(3, c), (3, c)], &[]])), "places out of order");
    let mut b = Crossings::builder(&t);
    b.leaf(n[0], [&[(511, c)], &[], &[(0, c), (7, c)]]).unwrap();
    assert_eq!(refused(b.leaf(n[0], [&[], &[], &[]])), "a leaf out of number order");
    b.leaf(n[1], [&[], &[], &[]]).unwrap();
    assert_eq!(refused(b.leaf(n[1], [&[], &[], &[]])), "a leaf out of number order");
    for m in &n[2..] {
        b.leaf(*m, [&[], &[], &[]]).unwrap();
    }
    let built = b.build().unwrap();
    assert_eq!(built.len(), 3);
    assert!(!built.is_empty());
    assert!(Crossings::builder(&Topology::new()).build().unwrap().is_empty());
    assert_eq!(built.get(n[0], EdgeAxis::X, 511), Some(c));
    assert_eq!(built.get(n[0], EdgeAxis::Z, 7), Some(c));
    assert_eq!(built.get(n[0], EdgeAxis::Z, 6), None);
    assert_eq!(built.get(n[1], EdgeAxis::X, 511), None);
    assert_eq!(built.get(n[0], EdgeAxis::X, 512), None);
    assert!(Crossings::builder(&topology(1, None)).leaf(n[0], [&[], &[], &[]]).is_ok());
}

fn file(entries: &[CacheEntry]) -> CacheFile {
    brodo::cache::from_bytes(&to_bytes(entries).unwrap()).unwrap()
}

#[test]
fn a_table_naming_group_0_alone_is_kept_empty_as_a_file_reads_it() {
    // Every crossing in group 0.
    let mut p = parts_at(&[[0; 3]], |v| v == [3, 3, 3]);
    p.names = vec![String::new()];
    let set = p.make().unwrap();
    assert!(set.group_names().is_empty());
    let f = file(&set.entries("s", &meta("s")).unwrap());
    assert!(HermiteView::find(&f, "s").unwrap().group_names().is_empty());
}

#[test]
fn a_set_round_trips_through_a_file_among_others() {
    let set = valid().make().unwrap();
    let small = parts(1, None, 2, 6).make().unwrap();
    let mut entries = small.entries("small", &meta("s")).unwrap();
    entries.extend(set.entries("box", &meta("b")).unwrap());
    let t = topology(1, None);
    entries.push(CacheEntry::new(&meta("other"), &t, &Field::new(&t, 0.0_f32)).unwrap());
    let f = file(&entries);
    let read = HermiteView::find(&f, "box").unwrap();
    assert_eq!(read.group_names(), ["", "a", "b"]);
    let t = set.topology();
    let bits = |c: Option<Crossing>| {
        c.map(|c| (c.fraction.to_bits(), c.normal.map(f32::to_bits), c.group))
    };
    let mut seen = 0;
    for n in t.leaf_numbers() {
        for (place, v) in t.leaf_voxels(n).unwrap().enumerate() {
            assert_eq!(read.signs().get(v).unwrap(), set.signs().value_at(t, v).unwrap());
            for axis in EdgeAxis::ALL {
                let held = set.crossings().get(n, axis, place);
                seen += usize::from(held.is_some());
                assert_eq!(bits(read.crossing(n.get(), axis, place).unwrap()), bits(held));
            }
        }
    }
    assert_eq!(seen, set.crossings().len());
    assert!(seen > 0);
    assert_eq!(HermiteView::find(&f, "small").unwrap().signs().get([2, 2, 2]).unwrap(), 1);
    assert!(matches!(HermiteView::find(&f, "none"), Err(HermiteError::Missing("sign"))));
}

/// A file's grids and entries of bytes, copied to be written again (`CacheEntry::copy`,
/// `CacheEntry::copy_bytes`), the grids first.
fn copied(f: &CacheFile) -> Vec<CacheEntry> {
    let mut out: Vec<CacheEntry> = f.grids().into_iter().map(CacheEntry::copy).collect();
    out.extend(f.bytes_entries().into_iter().map(CacheEntry::copy_bytes));
    out
}

/// A set and an SDF read from a file are written again, keys beginning `brodo.` kept, to
/// the same bytes, and through an archive back to them (ADR-0060).
#[test]
fn a_set_and_an_sdf_copied_from_a_file_are_written_again_as_they_were() {
    let set = valid().make().unwrap();
    let t = topology(1, None);
    let mut sdf = meta("sdf");
    sdf.extra.insert(SDF_KEY.to_owned(), SDF_VERSION.to_owned());
    let mut entries = vec![CacheEntry::new(&sdf, &t, &Field::new(&t, 1.0_f32)).unwrap()];
    entries.extend(set.entries("box", &meta("b")).unwrap());
    let bytes = to_bytes(&entries).unwrap();
    let f = brodo::cache::from_bytes(&bytes).unwrap();
    assert_eq!(to_bytes(&copied(&f)).unwrap(), bytes);
    #[cfg(feature = "archive")]
    {
        let archive = brodo::archive::to_bytes(&copied(&f)).unwrap();
        let back = brodo::archive::from_bytes(&archive, u64::MAX).unwrap();
        assert_eq!(to_bytes(&copied(&back)).unwrap(), bytes);
        assert_eq!(HermiteView::find(&back, "box").unwrap().group_names(), ["", "a", "b"]);
    }
    let again = brodo::cache::from_bytes(&to_bytes(&copied(&f)).unwrap()).unwrap();
    assert_eq!(HermiteView::find(&again, "box").unwrap().group_names(), ["", "a", "b"]);
    assert_eq!(again.grids()[0].meta().extra.get(SDF_KEY).map(String::as_str), Some(SDF_VERSION));
}

/// The keys of a set's entry: `brodo.hermite.` and `rest` joined to it.
fn keys(set: &str, field: &str, rest: &[(String, String)]) -> BTreeMap<String, String> {
    let mut out: BTreeMap<String, String> = [
        ("brodo.hermite.version", "3"),
        ("brodo.hermite.set", set),
        ("brodo.hermite.field", field),
    ]
    .map(|(k, v)| (k.to_owned(), v.to_owned()))
    .into();
    out.extend(rest.iter().cloned());
    out
}

/// A set's parts written as they are, unchecked, under the keys `edit` leaves of those
/// `entries` would give, the sign grid's at 0 and the crossings' at 1: its sign grid, and its
/// crossings' bytes as docs/formats.md §4 lays them out, written here from the document.
fn unchecked_as(
    p: &Parts,
    set: &str,
    edit: impl Fn(usize, &mut BTreeMap<String, String>),
) -> Vec<CacheEntry> {
    let t = &p.topology;
    let groups: Vec<(String, String)> = p
        .names
        .iter()
        .enumerate()
        .skip(1)
        .map(|(id, name)| (format!("brodo.hermite.group.{id}"), name.clone()))
        .collect();
    let mut m = meta(set);
    m.extra = keys(set, "sign", &groups);
    edit(0, &mut m.extra);
    let (mut masks, mut items) = (Vec::new(), Vec::new());
    for leaf in p.crossings() {
        for axis in &leaf {
            let mut words = [0_u64; 8];
            for (place, c) in axis {
                words[place / 64] |= 1 << (place % 64);
                items.extend(c.fraction.to_le_bytes());
                items.extend(c.normal.iter().flat_map(|x| x.to_le_bytes()));
                items.extend(c.group.to_le_bytes());
            }
            masks.extend(words.iter().flat_map(|w| w.to_le_bytes()));
        }
    }
    masks.extend(items);
    let mut extra = keys(set, "crossings", &[]);
    edit(1, &mut extra);
    vec![
        CacheEntry::with_reserved_keys(&m, t, &p.signs).unwrap(),
        CacheEntry::bytes_with_reserved_keys(set, extra, masks),
    ]
}

/// A set's parts written unchecked under the keys `entries` would give, named `u`.
fn unchecked(p: &Parts) -> Vec<CacheEntry> {
    unchecked_as(p, "u", |_, _| {})
}

/// The small set's entries, named `s`, each with its keys changed by `edit`.
fn edited(edit: impl Fn(usize, &mut BTreeMap<String, String>)) -> Vec<CacheEntry> {
    unchecked_as(&parts(1, None, 2, 6), "s", edit)
}

#[test]
fn a_files_keys_that_do_not_fit_are_refused() {
    let found = |e: Vec<CacheEntry>| HermiteView::find(&file(&e), "s").map(|_| ());
    assert!(found(edited(|_, _| {})).is_ok());
    for (i, field) in [(0, "sign"), (1, "crossings")] {
        let mut missing = edited(|_, _| {});
        missing.remove(i);
        assert!(matches!(found(missing), Err(HermiteError::Missing(f)) if f == field));
        let mut twice = edited(|_, _| {});
        twice.push(edited(|_, _| {}).remove(i));
        assert!(matches!(found(twice), Err(HermiteError::Twice(f)) if f == field));
    }
    let e = edited(|i, m| {
        if i == 1 {
            m.insert("brodo.hermite.version".into(), "2".into());
        }
    });
    assert!(matches!(found(e), Err(HermiteError::UnsupportedVersion(v)) if v == "2"));
    let e = edited(|_, m| {
        m.insert("brodo.hermite.version".into(), "4".into());
    });
    assert!(matches!(found(e), Err(HermiteError::UnsupportedVersion(v)) if v == "4"));
    for (i, other) in [(0, "signs"), (1, "x.fraction")] {
        let e = edited(|j, m| {
            if j == i {
                m.insert("brodo.hermite.field".into(), other.into());
            }
        });
        assert!(matches!(found(e), Err(HermiteError::UnknownField(f)) if f == other));
    }
    let e = edited(|i, m| {
        if i == 0 {
            m.remove("brodo.hermite.group.1");
        }
    });
    assert!(matches!(found(e), Err(HermiteError::BadGroupName(1))));
    let e = edited(|i, m| {
        if i == 0 {
            m.insert("brodo.hermite.group.03".into(), "c".into());
        }
    });
    assert!(matches!(found(e), Err(HermiteError::BadGroupKey(k)) if k == "brodo.hermite.group.03"));
    let e = edited(|i, m| {
        if i == 1 {
            m.insert("brodo.hermite.group.1".into(), "a".into());
        }
    });
    assert!(matches!(found(e), Err(HermiteError::ReservedKey(k)) if k == "brodo.hermite.group.1"));
}

/// A set's writers refuse a key beginning `brodo.` in the metadata given them, kept for
/// brodo's formats (ADR-0053), the SDF convention's named for itself; keys beginning
/// `hermite.`, an earlier version's, are free, and a set is still found among them.
#[test]
fn a_sets_writers_refuse_a_brodo_key() {
    let set = parts(1, None, 2, 6).make().unwrap();
    for key in ["brodo.kind", "brodo.hermite.set"] {
        let mut later = meta("s");
        later.extra.insert(key.into(), "t".into());
        assert!(matches!(set.entries("s", &later), Err(HermiteError::ReservedKey(k)) if k == key));
    }
    let mut sdf = meta("s");
    sdf.extra.insert(brodo::meta::SDF_KEY.into(), brodo::meta::SDF_VERSION.into());
    assert!(matches!(set.entries("s", &sdf), Err(HermiteError::ClaimsSdf)));
    let mut free = meta("s");
    free.extra.insert("hermite.set".into(), "s".into());
    free.extra.insert("hermite.version".into(), "2".into());
    let f = file(&set.entries("s", &free).unwrap());
    assert!(HermiteView::find(&f, "s").is_ok());
}

/// An earlier version's keys mark an entry of an earlier set only where it holds none of
/// this version's (docs/formats.md §4.2, rule 1): a set of this version whose entry carries
/// `hermite.set` naming another, as a free key, leaves that other set to be found, or
/// missing, as any (issue 15).
#[test]
fn an_earlier_versions_key_beside_this_versions_names_no_earlier_set() {
    let set = parts(1, None, 2, 6).make().unwrap();
    let mut free = meta("s");
    free.extra.insert("hermite.set".into(), "b".into());
    free.extra.insert("hermite.version".into(), "2".into());
    let mut entries = set.entries("s", &free).unwrap();
    assert!(matches!(HermiteView::find(&file(&entries), "b"), Err(HermiteError::Missing("sign"))));
    entries.extend(set.entries("b", &meta("b")).unwrap());
    let f = file(&entries);
    assert!(HermiteView::find(&f, "b").is_ok());
    assert!(HermiteView::find(&f, "s").is_ok());
}

#[test]
fn a_file_with_a_broken_rule_or_another_topology_is_refused() {
    // A sign grid of 2s in the place of the signs, written with the same keys.
    let mut p = parts(1, None, 2, 6);
    let t = p.topology.clone();
    let n = t.leaf_numbers().next().unwrap();
    p.signs.leaf_mut(&t, n).unwrap()[index([3, 3, 3])] = 2;
    assert!(matches!(
        HermiteView::find(&file(&unchecked_as(&p, "s", |_, _| {})), "s"),
        Err(HermiteError::Violation { at: Some([3, 3, 3]), axis: None, rule: Rule::Sign })
    ));
    // The crossings of a set on two leaves beside the signs of one.
    let one = unchecked_as(&parts(1, None, 2, 6), "s", |_, _| {});
    let two = unchecked_as(&parts(2, None, 2, 6), "s", |_, _| {});
    let entries = vec![one[0].clone(), two[1].clone()];
    assert!(matches!(
        HermiteView::find(&file(&entries), "s"),
        Err(HermiteError::OtherTopology("crossings"))
    ));
}

/// The committed file's set: the one leaf inside [2, 6)³.
fn golden_set() -> Hermite {
    parts(1, None, 2, 6).make().unwrap()
}

const GOLDEN: &[u8] = include_bytes!("data/hermite_v3.bin");
const EARLIER: [(&[u8], &str); 1] = [(include_bytes!("data/hermite_v2.bin"), "2")];

#[test]
fn format_version_3_keeps_its_bytes() {
    let bytes = to_bytes(&golden_set().entries("golden", &meta("cube")).unwrap()).unwrap();
    if bytes != GOLDEN {
        let path = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("hermite_v3.bin");
        std::fs::write(&path, &bytes).unwrap();
        panic!("the bytes of Hermite format version 3 changed; they are at {}", path.display());
    }
    let f = brodo::cache::from_bytes(GOLDEN).unwrap();
    let c = HermiteView::find(&f, "golden").unwrap().crossing(0, EdgeAxis::X, 5 + 8 * 3 + 64 * 3);
    let c = c.unwrap();
    let c = c.unwrap();
    assert_eq!((c.fraction.to_bits(), c.group), (0.5_f32.to_bits(), 1));
}

/// A set of version 1 or 2, its keys beginning `hermite.`, is refused by its version, not
/// read (ADR-0053); the file holding it is still read as a cache file.
#[test]
fn a_set_of_an_earlier_version_is_refused() {
    for (bytes, version) in EARLIER {
        let f = brodo::cache::from_bytes(bytes).unwrap();
        assert!(
            matches!(HermiteView::find(&f, "golden"), Err(HermiteError::UnsupportedVersion(v)) if v == version)
        );
        assert!(matches!(HermiteView::find(&f, "other"), Err(HermiteError::Missing("sign"))));
    }
}

/// What a check decided: taken, or the violation it named.
fn verdict<T>(r: Result<T, HermiteError>) -> Result<(), String> {
    r.map(|_| ()).map_err(|e| format!("{e:?}"))
}
/// Sets of tiles alone, by name, and whether they are taken.
fn tile_sets() -> [(&'static str, Parts, bool); 3] {
    [
        (
            "a root tile by a node's tile of its sign",
            tiles(&[(TileLevel::Root, [0; 3], 0), (TileLevel::Upper, [4096, 128, 0], 0)]),
            true,
        ),
        (
            // The upper tile's +x neighbour lies in the second root tile, tile place 1,
            // beyond the one upper node's number.
            "an upper tile by a root tile",
            tiles(&[
                (TileLevel::Root, [0; 3], 0),
                (TileLevel::Upper, [8064, 0, 0], 0),
                (TileLevel::Root, [8192, 0, 0], 0),
            ]),
            true,
        ),
        (
            "a root tile by a node's tile of the other sign",
            tiles(&[(TileLevel::Root, [0; 3], 0), (TileLevel::Lower, [4096, 136, 8], 1)]),
            false,
        ),
    ]
}

/// Sets taken and sets refused, written unchecked and read back: the file's check decides
/// as the check in memory does.
#[test]
fn a_file_decides_as_memory_does() {
    let tile_of = |p: &Parts| p.topology.tile_numbers().next().unwrap();
    let mut bad_leaf = valid();
    apply(&mut bad_leaf, Edit::Sign([19, 10, 10], 2));
    let mut bad_tile = valid();
    let t = tile_of(&bad_tile);
    bad_tile.signs.set_tile(&bad_tile.topology, t, 2).unwrap();
    let mut inside_background = valid();
    inside_background.signs = Field::new(&inside_background.topology, 1);
    let cases = [
        ("valid", valid(), true),
        ("a sign's background inside", inside_background, false),
        ("crossing between leaves", parts(3, None, 4, 16), true),
        ("an outside tile by the background", parts(2, Some([8, 8, 8]), 2, 6), true),
        ("a sign of 2 in a later leaf", bad_leaf, false),
        ("a tile's sign of 2", bad_tile, false),
        ("inside reaching the leaves' lower face", parts(3, Some([8, 8, 8]), 0, 20), false),
        ("a hole by a tile's face", shell_with_hole(), false),
    ];
    for (name, p, taken) in cases.into_iter().chain(tile_sets()) {
        let bytes = to_bytes(&unchecked(&p)).unwrap();
        let in_memory = verdict(p.make());
        assert_eq!(in_memory.is_ok(), taken, "{name}: {in_memory:?}");
        let f = brodo::cache::from_bytes(&bytes).unwrap();
        assert_eq!(verdict(HermiteView::find(&f, "u")), in_memory, "{name}");
    }
}
