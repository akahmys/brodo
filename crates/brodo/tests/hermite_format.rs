//! Format tier (TESTING.md §1) for Hermite data (ADR-0032): a set written to a cache file
//! reads back with the same bits and the same group table; the committed file keeps its
//! bytes; each rule of the format, broken alone, is refused for that rule, in memory and
//! from a file; a file's set is found by its keys and refused when they do not fit.

use brodo::cache::{CacheEntry, CacheFile, to_bytes};
use brodo::error::{HermiteError, Rule};
use brodo::field::Field;
use brodo::hermite::{Crossings, FIELDS, Hermite, HermiteView, NO_CROSSING};
use brodo::meta::GridMeta;
use brodo::topology::{Location, Topology};
use brodo::tree::{Coord, TileLevel};

/// A set's parts before they are checked.
struct Parts {
    topology: Topology,
    signs: Field<u8>,
    axes: [Crossings; 3],
    names: Vec<String>,
}

impl Parts {
    fn make(self) -> Result<Hermite, HermiteError> {
        Hermite::new(self.topology, self.signs, self.axes, self.names)
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
    let none = |t: &Topology| Crossings {
        fraction: Field::new(t, NO_CROSSING.0),
        normal: Field::new(t, NO_CROSSING.1),
        group: Field::new(t, NO_CROSSING.2),
    };
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
fn rule(r: Result<Hermite, HermiteError>) -> Result<(Option<Coord>, Option<usize>, Rule), String> {
    match r {
        Err(HermiteError::Violation { at, axis, rule }) => Ok((at, axis, rule)),
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
    let none = |t: &Topology| Crossings {
        fraction: Field::new(t, NO_CROSSING.0),
        normal: Field::new(t, NO_CROSSING.1),
        group: Field::new(t, NO_CROSSING.2),
    };
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
    let none = |t: &Topology| Crossings {
        fraction: Field::new(t, NO_CROSSING.0),
        normal: Field::new(t, NO_CROSSING.1),
        group: Field::new(t, NO_CROSSING.2),
    };
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
    let none = |t: &Topology| Crossings {
        fraction: Field::new(t, NO_CROSSING.0),
        normal: Field::new(t, NO_CROSSING.1),
        group: Field::new(t, NO_CROSSING.2),
    };
    let mut signs = Field::new(&topology, 0_u8);
    for t in topology.tile_numbers() {
        let (level, o) = topology.tile(t).unwrap();
        let sign = tiles.iter().find(|(l, p, _)| (*l, *p) == (level, o)).unwrap().2;
        signs.set_tile(&topology, t, sign).unwrap();
    }
    let axes = [none(&topology), none(&topology), none(&topology)];
    Parts { topology, signs, axes, names: Vec::new() }
}

#[test]
fn a_root_tiles_face_reaches_the_tiles_of_the_node_beside_it() {
    // An outside root tile, and beside its +x face, in an upper node, an inside lower tile:
    // the root tile is checked first, and its face is found to meet the lower tile there,
    // not left to the lower tile, whose own first change is on its +x face, at x = 4103.
    let p = tiles(&[(TileLevel::Root, [0; 3], 0), (TileLevel::Lower, [4096, 136, 8], 1)]);
    assert_eq!(rule(p.make()), Ok((Some([4095, 136, 8]), Some(0), Rule::Owner)));
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
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || tx.send(tiles(&all).make().is_ok()).unwrap_or(()));
    assert_eq!(rx.recv_timeout(std::time::Duration::from_secs(60)), Ok(true));
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
    // lacks.
    let mut p = valid();
    let t = p.topology.tile_numbers().next().unwrap();
    p.signs.set_tile(&p.topology, t, 0).unwrap();
    assert_eq!(rule(p.make()).map(|r| r.2), Ok(Rule::Fraction));
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

#[test]
fn tiles_and_backgrounds_hold_the_values_of_no_crossing() {
    let mut p = valid();
    let t = p.topology.tile_numbers().next().unwrap();
    p.signs.set_tile(&p.topology, t, 2).unwrap();
    assert_eq!(rule(p.make()), Ok((Some([8, 8, 8]), None, Rule::Sign)));
    let mut p = valid();
    p.axes[2].group.set_tile(&p.topology, t, 1).unwrap();
    assert_eq!(rule(p.make()), Ok((Some([8, 8, 8]), Some(2), Rule::NoCrossing)));
    let mut p = valid();
    p.signs = Field::new(&p.topology, 1);
    assert_eq!(rule(p.make()), Ok((None, None, Rule::Sign)));
    let mut p = valid();
    p.axes[1].normal = Field::new(&p.topology, [0.0, 0.0, -0.0]);
    assert_eq!(rule(p.make()), Ok((None, Some(1), Rule::NoCrossing)));
}

#[test]
fn a_tile_face_owning_a_change_is_refused() {
    // One inside tile and no leaf: its upper faces own edges into the outside background.
    let empty = Topology::new();
    let mut e = empty.edit();
    e.add_tile(TileLevel::Lower, [8, 0, 0]).unwrap();
    let topology = e.commit().unwrap().0;
    let mut p = parts(0, None, 0, 0);
    p.signs = Field::new(&topology, 0);
    let t = topology.tile_numbers().next().unwrap();
    p.signs.set_tile(&topology, t, 1).unwrap();
    p.axes = std::array::from_fn(|_| Crossings {
        fraction: Field::new(&topology, NO_CROSSING.0),
        normal: Field::new(&topology, NO_CROSSING.1),
        group: Field::new(&topology, NO_CROSSING.2),
    });
    p.topology = topology;
    assert_eq!(rule(p.make()), Ok((Some([15, 0, 0]), Some(0), Rule::Owner)));
}

#[test]
fn group_tables_and_topologies_that_do_not_fit_are_refused() {
    for (names, bad) in [(vec!["x"], 0), (vec!["", "a", ""], 2), (vec!["", "a", "a"], 2)] {
        let mut p = valid();
        p.names = names.into_iter().map(String::from).collect();
        assert!(matches!(p.make(), Err(HermiteError::BadGroupName(b)) if b == bad));
    }
    let mut p = valid();
    let other = topology(3, Some([8, 8, 8]));
    p.axes[1].normal = Field::new(&other, NO_CROSSING.1);
    assert!(matches!(p.make(), Err(HermiteError::OtherTopology("y.normal"))));
}

fn file(entries: &[CacheEntry]) -> CacheFile {
    CacheFile::from_bytes(&to_bytes(entries).unwrap()).unwrap()
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
    for n in t.leaf_numbers() {
        for v in t.leaf_voxels(n).unwrap() {
            assert_eq!(read.signs().get(v).unwrap(), set.signs().value_at(t, v).unwrap());
            for (r, c) in read.axes().iter().zip(set.axes()) {
                let bits = |x: [f32; 3]| x.map(f32::to_bits);
                assert_eq!(
                    r.fraction.get(v).unwrap().to_bits(),
                    c.fraction.value_at(t, v).unwrap().to_bits()
                );
                assert_eq!(bits(r.normal.get(v).unwrap()), bits(c.normal.value_at(t, v).unwrap()));
                assert_eq!(r.group.get(v).unwrap(), c.group.value_at(t, v).unwrap());
            }
        }
    }
    assert_eq!(HermiteView::find(&f, "small").unwrap().signs().get([2, 2, 2]).unwrap(), 1);
    assert!(matches!(HermiteView::find(&f, "none"), Err(HermiteError::Missing("sign"))));
}

/// The small set's entries, each with `extra` changed by `edit` before it is written.
fn edited(edit: impl Fn(usize, &mut GridMeta)) -> Vec<CacheEntry> {
    let set = parts(1, None, 2, 6).make().unwrap();
    let t = set.topology();
    let mut out = Vec::new();
    for (i, field) in FIELDS.iter().enumerate() {
        let mut m = meta("s");
        m.extra.insert("hermite.version".into(), "1".into());
        m.extra.insert("hermite.set".into(), "s".into());
        m.extra.insert("hermite.field".into(), (*field).into());
        if i == 0 {
            m.extra.insert("hermite.group.1".into(), "a".into());
            m.extra.insert("hermite.group.2".into(), "b".into());
        }
        edit(i, &mut m);
        let axis = set.axes().get(i.saturating_sub(1) / 3);
        let e = match (i, axis) {
            (0, _) => CacheEntry::new(&m, t, set.signs()),
            (i, Some(c)) if i % 3 == 1 => CacheEntry::new(&m, t, &c.fraction),
            (i, Some(c)) if i % 3 == 2 => CacheEntry::new(&m, t, &c.normal),
            (_, Some(c)) => CacheEntry::new(&m, t, &c.group),
            _ => unreachable!(),
        };
        out.push(e.unwrap());
    }
    out
}

#[test]
fn a_files_keys_that_do_not_fit_are_refused() {
    let found = |e: Vec<CacheEntry>| HermiteView::find(&file(&e), "s").map(|_| ());
    assert!(found(edited(|_, _| {})).is_ok());
    let mut missing = edited(|_, _| {});
    missing.remove(5);
    assert!(matches!(found(missing), Err(HermiteError::Missing("y.normal"))));
    let mut twice = edited(|_, _| {});
    twice.push(edited(|_, _| {}).remove(3));
    assert!(matches!(found(twice), Err(HermiteError::Twice("x.group"))));
    let e = edited(|i, m| {
        if i == 4 {
            m.extra.insert("hermite.version".into(), "2".into());
        }
    });
    assert!(matches!(found(e), Err(HermiteError::Version(v)) if v == "2"));
    let e = edited(|i, m| {
        if i == 7 {
            m.extra.insert("hermite.field".into(), "z.fractions".into());
        }
    });
    assert!(matches!(found(e), Err(HermiteError::UnknownField(f)) if f == "z.fractions"));
    let e = edited(|i, m| {
        if i == 9 {
            m.unit = "meter".into();
        }
    });
    assert!(matches!(found(e), Err(HermiteError::OtherMeta("z.group"))));
    let e = edited(|i, m| {
        if i == 0 {
            m.extra.remove("hermite.group.1");
        }
    });
    assert!(matches!(found(e), Err(HermiteError::BadGroupName(1))));
    let e = edited(|i, m| {
        if i == 0 {
            m.extra.insert("hermite.group.03".into(), "c".into());
        }
    });
    assert!(matches!(found(e), Err(HermiteError::BadGroupKey(k)) if k == "hermite.group.03"));
    let mut reserved = meta("s");
    reserved.extra.insert("hermite.set".into(), "t".into());
    let set = parts(1, None, 2, 6).make().unwrap();
    assert!(
        matches!(set.entries("s", &reserved), Err(HermiteError::ReservedKey(k)) if k == "hermite.set")
    );
}

#[test]
fn a_file_with_a_broken_rule_or_another_topology_is_refused() {
    let set = parts(1, None, 2, 6).make().unwrap();
    let mut entries = set.entries("s", &meta("s")).unwrap();
    // A sign grid of 2s in the place of the signs, written with the same keys.
    let t = set.topology();
    let mut m = meta("s");
    for (k, v) in [("hermite.version", "1"), ("hermite.set", "s"), ("hermite.field", "sign")] {
        m.extra.insert(k.into(), v.into());
    }
    m.extra.insert("hermite.group.1".into(), "a".into());
    m.extra.insert("hermite.group.2".into(), "b".into());
    let mut wrong = Field::new(t, 0_u8);
    let n = t.leaf_numbers().next().unwrap();
    wrong.leaf_mut(t, n).unwrap()[index([3, 3, 3])] = 2;
    entries[0] = CacheEntry::new(&m, t, &wrong).unwrap();
    let f = file(&entries);
    assert!(matches!(
        HermiteView::find(&f, "s"),
        Err(HermiteError::Violation { at: Some([3, 3, 3]), axis: None, rule: Rule::Sign })
    ));
    let other = topology(2, None);
    m.extra.insert("hermite.field".into(), "x.normal".into());
    m.extra.retain(|k, _| !k.starts_with("hermite.group."));
    let mut entries = set.entries("s", &meta("s")).unwrap();
    entries[2] = CacheEntry::new(&m, &other, &Field::new(&other, NO_CROSSING.1)).unwrap();
    assert!(matches!(
        HermiteView::find(&file(&entries), "s"),
        Err(HermiteError::OtherTopology("x.normal"))
    ));
}

/// The bytes of a file of `entries` whose last grid holds one tile value more, reached by
/// no node: its frozen buffer grown by a block (docs/formats.md §2.1, §3.2).
fn with_a_tile_more(entries: &[CacheEntry]) -> Vec<u8> {
    let mut b = to_bytes(entries).unwrap();
    let word = |b: &[u8], at: usize| u32::from_le_bytes(b[at..at + 4].try_into().unwrap());
    let put = |b: &mut [u8], at: usize, v: u32| b[at..at + 4].copy_from_slice(&v.to_le_bytes());
    let entry = 32 * entries.len();
    let frozen = 32 * word(&b, entry + 8) as usize;
    let blocks = word(&b, frozen + 56);
    assert_eq!(word(&b, frozen + 32), 0, "a grid with no tile");
    put(&mut b, frozen + 32, 1);
    put(&mut b, frozen + 52, blocks);
    put(&mut b, frozen + 56, blocks + 1);
    put(&mut b, entry + 12, blocks + 1);
    b.extend([0; 32]);
    b
}

#[test]
fn a_field_holding_values_no_node_reaches_is_refused() {
    // The z groups hold a tile value more than the signs: the same nodes, another tree.
    let set = parts(1, None, 2, 6).make().unwrap();
    let entries = set.entries("s", &meta("s")).unwrap();
    assert_eq!(FIELDS.last(), Some(&"z.group"));
    let f = CacheFile::from_bytes(&with_a_tile_more(&entries)).unwrap();
    assert!(matches!(HermiteView::find(&f, "s"), Err(HermiteError::OtherTopology("z.group"))));
}

/// The committed file: the one-leaf set inside [2, 6)³.
fn golden() -> Vec<u8> {
    let set = parts(1, None, 2, 6).make().unwrap();
    to_bytes(&set.entries("golden", &meta("cube")).unwrap()).unwrap()
}

const GOLDEN: &[u8] = include_bytes!("data/hermite_v1.bin");

#[test]
fn format_version_1_keeps_its_bytes() {
    let bytes = golden();
    assert_eq!(bytes.len(), GOLDEN.len(), "length");
    assert!(bytes == GOLDEN, "the bytes of Hermite format version 1 changed; raise the version");
    let f = CacheFile::from_bytes(GOLDEN).unwrap();
    let read = HermiteView::find(&f, "golden").unwrap();
    assert_eq!(read.axes()[0].fraction.get([5, 3, 3]).unwrap().to_bits(), 0.5_f32.to_bits());
    assert_eq!(read.axes()[0].group.get([5, 3, 3]).unwrap(), 1);
}

/// A set's parts written as they are, unchecked, under the keys `entries` would give.
fn unchecked(p: &Parts) -> Vec<CacheEntry> {
    let t = &p.topology;
    let mut out = Vec::new();
    for (i, field) in FIELDS.iter().enumerate() {
        let mut m = meta("u");
        for (k, v) in [("hermite.version", "1"), ("hermite.set", "u"), ("hermite.field", field)] {
            m.extra.insert(k.into(), v.into());
        }
        if i == 0 {
            for (id, name) in p.names.iter().enumerate().skip(1) {
                m.extra.insert(format!("hermite.group.{id}"), name.clone());
            }
        }
        let e = match (i, p.axes.get(i.saturating_sub(1) / 3)) {
            (0, _) => CacheEntry::new(&m, t, &p.signs),
            (i, Some(c)) if i % 3 == 1 => CacheEntry::new(&m, t, &c.fraction),
            (i, Some(c)) if i % 3 == 2 => CacheEntry::new(&m, t, &c.normal),
            (_, Some(c)) => CacheEntry::new(&m, t, &c.group),
            _ => unreachable!(),
        };
        out.push(e.unwrap());
    }
    out
}

/// What a check decided: taken, or the violation it named.
fn verdict<T>(r: Result<T, HermiteError>) -> Result<(), String> {
    r.map(|_| ()).map_err(|e| format!("{e:?}"))
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
    let mut bad_tile_crossing = valid();
    let t = tile_of(&bad_tile_crossing);
    bad_tile_crossing.axes[2].group.set_tile(&bad_tile_crossing.topology, t, 1).unwrap();
    let mut bad_background = valid();
    bad_background.axes[1].normal = Field::new(&bad_background.topology, [0.0, 0.0, -0.0]);
    let mut inside_background = valid();
    inside_background.signs = Field::new(&inside_background.topology, 1);
    let cases = [
        ("valid", valid(), true),
        ("a crossing on a tile", bad_tile_crossing, false),
        ("a crossing's background other than none", bad_background, false),
        ("a sign's background inside", inside_background, false),
        ("crossing between leaves", parts(3, None, 4, 16), true),
        ("an outside tile by the background", parts(2, Some([8, 8, 8]), 2, 6), true),
        ("a sign of 2 in a later leaf", bad_leaf, false),
        ("a tile's sign of 2", bad_tile, false),
        ("inside reaching the leaves' lower face", parts(3, Some([8, 8, 8]), 0, 20), false),
        ("a hole by a tile's face", shell_with_hole(), false),
        (
            "a root tile by a node's tile of its sign",
            tiles(&[(TileLevel::Root, [0; 3], 0), (TileLevel::Upper, [4096, 128, 0], 0)]),
            true,
        ),
        (
            "a root tile by a node's tile of the other sign",
            tiles(&[(TileLevel::Root, [0; 3], 0), (TileLevel::Lower, [4096, 136, 8], 1)]),
            false,
        ),
    ];
    for (name, p, taken) in cases {
        let bytes = to_bytes(&unchecked(&p)).unwrap();
        let in_memory = verdict(p.make());
        assert_eq!(in_memory.is_ok(), taken, "{name}: {in_memory:?}");
        let f = CacheFile::from_bytes(&bytes).unwrap();
        assert_eq!(verdict(HermiteView::find(&f, "u")), in_memory, "{name}");
    }
}
