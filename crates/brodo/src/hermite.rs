//! Hermite data on a grid's voxel corners, a format any producer writes (ARCHITECTURE.md
//! §4.9, ADR-0032): a sign at each lattice point and, on each edge whose ends differ, where
//! the surface crosses it, its normal there and its group (Ju et al. 2002).
//!
//! Point v lies at the corner of voxel v, the image of v under the grid's transform. It owns
//! its +x, +y and +z edges. A set is a sign field on a topology, 1 inside and 0 outside,
//! and the crossings, held only on the edges that hold one (ADR-0051). An edge has a
//! crossing exactly when the stored signs of its ends differ, those of tiles and the
//! background included; its owner is then a leaf point. Its fraction is the crossing's place
//! from the edge's lower end, in [0, 1]; its normal a unit vector whose component along the
//! edge, where not zero, points from the inside end to the outside end; its group an id the
//! set's table names. The signs' background is 0.
//!
//! In a cache file, `GridMeta::extra` tells a set's entries apart and holds the group table,
//! as `docs/formats.md` §4 states, which this code follows: the sign grid and an entry of
//! bytes holding the crossings. A set of an earlier version, its keys beginning `hermite.`,
//! is refused, not read (ADR-0053).

use std::collections::{BTreeMap, BTreeSet};

use crate::cache::{CacheEntry, CacheFile};
use crate::error::{FormatError, HermiteError, Rule};
use crate::field::local_index;
use crate::field::{Field, LEAF_VOXELS};
use crate::meta::GridMeta;
use crate::topology::{LeafNumber, Location, Topology};
use crate::tree::{Axis, Coord, TileLevel};
use crate::view::{GridView, ViewLocation};

/// The format's version, in each entry's `brodo.hermite.version`.
pub const VERSION: &str = "3";
/// The field of a set's entry of bytes holding its crossings.
pub const CROSSINGS: &str = "crossings";
/// How far a normal's length may be from 1 (ADR-0032): each component of a unit `f64`
/// vector rounded to `f32` moves by at most 2⁻²⁴ of itself, so its length by at most 2⁻²⁴,
/// about 6 × 10⁻⁸, and the length worked in `f64` adds a rounding of 10⁻¹⁶; 10⁻⁶ holds
/// that sixteen times over, so that a producer rounding once more, as a normal averaged
/// then rounded does, is still read.
pub const NORMAL_TOLERANCE: f64 = 1e-6;
/// Bytes a leaf's masks take in a set's entry of bytes: 8 `u64` words an axis.
const MASK_BYTES: usize = 3 * 8 * 8;
/// Bytes a crossing takes there: a fraction, three normal components and a group.
const CROSSING_BYTES: usize = 20;

const KEY_VERSION: &str = "brodo.hermite.version";
const KEY_SET: &str = "brodo.hermite.set";
const KEY_FIELD: &str = "brodo.hermite.field";
const KEY_GROUP: &str = "brodo.hermite.group.";
/// The keys a Hermite set's entries hold, written by `Hermite::entries` alone.
const PREFIX: &str = "brodo.hermite.";
/// The keys of a set of version 1 or 2, which named the set and its version.
const OLD_SET: &str = "hermite.set";
const OLD_VERSION: &str = "hermite.version";

/// A crossing: where the surface crosses an edge, its normal there and its group.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Crossing {
    /// The crossing's place from the edge's lower end, in [0, 1].
    pub fraction: f32,
    /// The surface's unit normal there.
    pub normal: [f32; 3],
    /// Its group.
    pub group: u32,
}

/// A set's crossings, held only on the edges that hold one: per leaf and axis a mask of the
/// points whose edge holds one, and the crossings in leaf, axis and place order
/// (docs/formats.md §4.1). A place is a point's index in its leaf, x + 8y + 64z.
#[derive(Debug, Clone, PartialEq)]
pub struct Crossings {
    stamp: u64,
    masks: Vec<[[u64; 8]; 3]>,
    /// Where each leaf's crossings start, and after the last leaf where they end.
    starts: Vec<u32>,
    items: Vec<Crossing>,
}

/// Builds a set's crossings leaf by leaf, in leaf number order (`Crossings::builder`).
#[derive(Debug, Clone)]
pub struct CrossingsBuilder {
    leaves: usize,
    built: Crossings,
}

/// The bits set in `masks` before place `place` of `axis`, and whether its own is set.
fn rank(masks: &[[u64; 8]; 3], axis: usize, place: usize) -> Option<(u32, bool)> {
    let own = masks.get(axis)?;
    let word = *own.get(place / 64)?;
    let ones = |w: &[u64]| w.iter().map(|x| x.count_ones()).sum::<u32>();
    let before = masks.iter().take(axis).map(|m| ones(m)).sum::<u32>()
        + ones(own.get(..place / 64)?)
        + (word & ((1_u64 << (place % 64)) - 1)).count_ones();
    Some((before, word >> (place % 64) & 1 == 1))
}

impl Crossings {
    /// A builder of the crossings of a set on `t`.
    pub fn builder(t: &Topology) -> CrossingsBuilder {
        CrossingsBuilder {
            leaves: t.leaf_count(),
            built: Crossings {
                stamp: t.stamp(),
                masks: Vec::with_capacity(t.leaf_count()),
                starts: vec![0],
                items: Vec::new(),
            },
        }
    }

    /// The crossing on the edge along `axis`, 0 to 2 for x to z, of leaf `leaf`'s point at
    /// `place`, if the edge holds one.
    pub fn get(&self, leaf: LeafNumber, axis: Axis, place: usize) -> Option<Crossing> {
        let n = usize::try_from(leaf.get()).ok()?;
        let (before, set) = rank(self.masks.get(n)?, axis.index(), place)?;
        let at = self.starts.get(n)?.checked_add(before)?;
        set.then(|| self.items.get(usize::try_from(at).ok()?).copied()).flatten()
    }

    /// The number of crossings.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Whether no edge holds a crossing.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    fn belongs_to(&self, t: &Topology) -> bool {
        self.stamp == t.stamp() && self.masks.len() == t.leaf_count()
    }

    /// The bytes of a set's entry of crossings (docs/formats.md §4.1).
    fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.masks.len() * MASK_BYTES + self.items.len() * 20);
        for w in self.masks.iter().flatten().flatten() {
            out.extend_from_slice(&w.to_le_bytes());
        }
        for c in &self.items {
            out.extend_from_slice(&c.fraction.to_le_bytes());
            for x in c.normal {
                out.extend_from_slice(&x.to_le_bytes());
            }
            out.extend_from_slice(&c.group.to_le_bytes());
        }
        out
    }
}

impl CrossingsBuilder {
    /// Leaf `n`'s crossings, on each axis x, y and z as pairs of a place below 512 and a
    /// crossing, places strictly ascending. Leaves are given in number order, each once;
    /// another order, a place out of range or out of order is refused.
    pub fn leaf(
        &mut self,
        n: LeafNumber,
        axes: [&[(usize, Crossing)]; 3],
    ) -> Result<(), HermiteError> {
        let b = &mut self.built;
        if usize::try_from(n.get()).ok() != Some(b.masks.len()) || b.masks.len() >= self.leaves {
            return Err(HermiteError::Build("a leaf out of number order"));
        }
        // Every place checked before any crossing is kept, so that a leaf refused leaves
        // the builder as it was.
        let mut masks = [[0_u64; 8]; 3];
        for (mask, crossings) in masks.iter_mut().zip(axes) {
            let mut last = None;
            for (place, _) in crossings {
                let word = mask.get_mut(place / 64).ok_or(HermiteError::Build("a place"))?;
                if last.is_some_and(|l| l >= *place) {
                    return Err(HermiteError::Build("places out of order"));
                }
                last = Some(*place);
                *word |= 1 << (place % 64);
            }
        }
        b.items.extend(axes.iter().flat_map(|a| a.iter().map(|(_, c)| *c)));
        b.masks.push(masks);
        let end = u32::try_from(b.items.len()).map_err(|_| HermiteError::Build("a count"))?;
        b.starts.push(end);
        Ok(())
    }

    /// The crossings, once every leaf is given.
    pub fn build(self) -> Result<Crossings, HermiteError> {
        if self.built.masks.len() != self.leaves {
            return Err(HermiteError::Build("a leaf left out"));
        }
        Ok(self.built)
    }
}

/// A set of Hermite data on one topology, checked whole when made.
#[derive(Debug, Clone)]
pub struct Hermite {
    topology: Topology,
    signs: Field<u8>,
    crossings: Crossings,
    group_names: Vec<String>,
}

/// A set's crossings read in place from its entry of bytes, its leaves' starts worked out
/// from their masks.
#[derive(Debug, Clone)]
struct ViewCrossings<'a> {
    masks: &'a [u8],
    items: &'a [u8],
    starts: Vec<u32>,
}

/// A set read in place from a cache file, checked whole when found.
#[derive(Debug, Clone)]
pub struct HermiteView<'a> {
    signs: GridView<'a, u8>,
    crossings: ViewCrossings<'a>,
    group_names: Vec<String>,
}

/// The unit step along an axis.
fn step(axis: usize) -> Coord {
    let mut e = [0; 3];
    if let Some(c) = e.get_mut(axis) {
        *c = 1;
    }
    e
}

fn plus(v: Coord, d: Coord) -> Coord {
    let ([a, b, c], [x, y, z]) = (v, d);
    [a.wrapping_add(x), b.wrapping_add(y), c.wrapping_add(z)]
}

fn minus(v: Coord, d: Coord) -> Coord {
    let ([a, b, c], [x, y, z]) = (v, d);
    [a.wrapping_sub(x), b.wrapping_sub(y), c.wrapping_sub(z)]
}

/// The point a step `d` from `v`, forward or back, or nothing beyond the `i32` range, which
/// is the background: sign 0, in no leaf (docs/formats.md §4.2).
fn neighbour(v: Coord, d: Coord, forward: bool) -> Option<Coord> {
    let step = |c: i32, k: i32| if forward { c.checked_add(k) } else { c.checked_sub(k) };
    Some([step(v[0], d[0])?, step(v[1], d[1])?, step(v[2], d[2])?])
}

fn violation(at: Option<Coord>, axis: Option<usize>, rule: Rule) -> HermiteError {
    HermiteError::Violation { at, axis: axis.and_then(|a| Axis::ALL.get(a).copied()), rule }
}

/// Refuses a group table whose first name is not `""`, or whose other names are empty or
/// repeated; empty, it stands for `[""]`.
fn check_names(names: &[String]) -> Result<(), HermiteError> {
    let mut seen = BTreeSet::new();
    for (id, name) in names.iter().enumerate() {
        let bad = if id == 0 { !name.is_empty() } else { name.is_empty() || !seen.insert(name) };
        if bad {
            return Err(HermiteError::BadGroupName(u32::try_from(id).unwrap_or(u32::MAX)));
        }
    }
    Ok(())
}

/// What the check reads, from a set made in memory or read from a file.
trait Source {
    fn leaves(&self) -> Result<Vec<Coord>, HermiteError>;
    /// Each tile's level, origin and the number its values are read by.
    fn tiles(&self) -> Result<Vec<(TileLevel, Coord, u32)>, HermiteError>;
    fn is_leaf(&self, v: Coord) -> Result<bool, HermiteError>;
    /// Whether a node holds the voxel's region of `level`.
    fn holds_node(&self, level: TileLevel, v: Coord) -> Result<bool, HermiteError>;
    fn sign_at(&self, v: Coord) -> Result<u8, HermiteError>;
    fn sign_leaf(&self, n: u32) -> Result<&[u8; LEAF_VOXELS], HermiteError>;
    /// The crossing on the edge along `axis` of leaf `n`'s point at `place`, if it holds one.
    fn crossing(&self, axis: usize, n: u32, place: usize)
    -> Result<Option<Crossing>, HermiteError>;
    fn sign_tile(&self, t: u32) -> Result<u8, HermiteError>;
    fn sign_background(&self) -> u8;
}

/// Checks one crossed edge's values.
fn check_crossing(c: Crossing, axis: usize, lower_inside: bool, groups: usize) -> Result<(), Rule> {
    let Crossing { fraction, normal, group } = c;
    if !(0.0..=1.0).contains(&fraction) {
        return Err(Rule::Fraction);
    }
    let length = normal.iter().map(|x| f64::from(*x) * f64::from(*x)).sum::<f64>().sqrt();
    let near = (length - 1.0).abs().partial_cmp(&NORMAL_TOLERANCE);
    if !matches!(near, Some(std::cmp::Ordering::Less | std::cmp::Ordering::Equal)) {
        return Err(Rule::NormalLength);
    }
    let along = normal.get(axis).copied().unwrap_or(0.0);
    if (lower_inside && along < 0.0) || (!lower_inside && along > 0.0) {
        return Err(Rule::NormalInward);
    }
    if usize::try_from(group).map_or(true, |g| g >= groups) {
        return Err(Rule::UnnamedGroup);
    }
    Ok(())
}

/// The voxel at index `i` of a leaf at `origin`, x fastest.
fn voxel(origin: Coord, i: usize) -> Coord {
    let i = i32::try_from(i).unwrap_or(0);
    plus(origin, [i & 7, i >> 3 & 7, i >> 6])
}

/// Checks a set against the format (ADR-0032).
fn check(s: &impl Source, names: &[String]) -> Result<(), HermiteError> {
    check_names(names)?;
    let groups = names.len().max(1);
    if s.sign_background() != 0 {
        return Err(violation(None, None, Rule::Sign));
    }
    let tiles = s.tiles()?;
    for (_, origin, t) in &tiles {
        if s.sign_tile(*t)? > 1 {
            return Err(violation(Some(*origin), None, Rule::Sign));
        }
    }
    let leaves = s.leaves()?;
    for (n, origin) in (0_u32..).zip(&leaves) {
        let bad = s.sign_leaf(n)?.iter().position(|sign| *sign > 1);
        if let Some(i) = bad {
            return Err(violation(Some(voxel(*origin, i)), None, Rule::Sign));
        }
    }
    // Tiles' faces before leaves, so that a change only a tile's face meets is the one
    // reported.
    for (level, origin, t) in &tiles {
        check_tile_faces(s, *level, *origin, s.sign_tile(*t)?)?;
    }
    for (n, origin) in (0_u32..).zip(leaves) {
        check_leaf(s, n, origin, groups)?;
    }
    Ok(())
}

/// Checks a leaf's points: signs, the crossings they own, and that an edge into the leaf
/// from a point outside every leaf crosses nothing.
fn check_leaf(s: &impl Source, n: u32, origin: Coord, groups: usize) -> Result<(), HermiteError> {
    let signs = s.sign_leaf(n)?;
    let sign_of = |v: Coord| -> Result<u8, HermiteError> {
        let local = minus(v, origin);
        if local.iter().all(|c| (0..8).contains(c)) {
            return signs.get(local_index(v)).copied().ok_or(HermiteError::Violation {
                at: Some(v),
                axis: None,
                rule: Rule::Sign,
            });
        }
        s.sign_at(v)
    };
    for axis in 0..3 {
        let e = step(axis);
        for (i, sign) in signs.iter().enumerate() {
            let v = voxel(origin, i);
            let crossing = s.crossing(axis, n, i)?;
            let beyond = match neighbour(v, e, true) {
                Some(w) => sign_of(w)?,
                None => 0,
            };
            match (crossing, beyond == *sign) {
                (None, true) => {}
                (Some(c), false) => check_crossing(c, axis, *sign == 1, groups)
                    .map_err(|rule| violation(Some(v), Some(axis), rule))?,
                (Some(_), true) | (None, false) => {
                    return Err(violation(Some(v), Some(axis), Rule::NoCrossing));
                }
            }
            // The point before v owns the edge into it; beyond the range it is the
            // background, which owns nothing, and the error names v.
            match neighbour(v, e, false) {
                Some(u) => {
                    let in_leaf = minus(u, origin).iter().all(|c| (0..8).contains(c));
                    if !in_leaf && !s.is_leaf(u)? && sign_of(u)? != *sign {
                        return Err(violation(Some(u), Some(axis), Rule::Owner));
                    }
                }
                None if *sign != 0 => return Err(violation(Some(v), Some(axis), Rule::Owner)),
                None => {}
            }
        }
    }
    Ok(())
}

/// Checks that no edge a tile's point owns, or that reaches the tile from outside every
/// leaf, crosses anything. Edges between a tile and a leaf are the leaf's to check. Across
/// each face, the region of the tile's level beyond it is one value unless a node holds it,
/// and then each of the node's regions along the face is checked in turn, so the work
/// follows the nodes the face touches, not its area.
fn check_tile_faces(
    s: &impl Source,
    level: TileLevel,
    origin: Coord,
    sign: u8,
) -> Result<(), HermiteError> {
    let far = level.edge() - 1;
    for axis in 0..3 {
        let e = step(axis);
        // The tile's upper face owns the edges out of it; the point below its lower face
        // owns the edges into it. Beyond the range lies the background, sign 0, in no leaf;
        // the error then names the tile's point.
        let upper = plus(origin, e.map(|c| c * far));
        let face = Face { axis, sign, forward: true };
        match neighbour(upper, e, true) {
            Some(o) => face.check_region(s, level, o)?,
            None if sign != 0 => return Err(violation(Some(upper), Some(axis), Rule::Owner)),
            None => {}
        }
        let face = Face { forward: false, ..face };
        match neighbour(origin, e, false) {
            Some(o) => face.check_region(s, level, o)?,
            None if sign != 0 => return Err(violation(Some(origin), Some(axis), Rule::Owner)),
            None => {}
        }
    }
    Ok(())
}

/// The outside of one face of a tile: the voxels beyond it along `axis`, after it if
/// `forward`, which must have the tile's `sign` wherever no leaf holds them.
#[derive(Clone, Copy)]
struct Face {
    axis: usize,
    sign: u8,
    forward: bool,
}

impl Face {
    /// Checks the voxels beyond the face in the region of `level` whose corner is `at`.
    // Bounded recursion: each call goes one tile level finer, root to upper to lower, so
    // three deep at most.
    fn check_region(
        self,
        s: &impl Source,
        level: TileLevel,
        at: Coord,
    ) -> Result<(), HermiteError> {
        if !s.holds_node(level, at)? {
            if s.sign_at(at)? == self.sign {
                return Ok(());
            }
            let e = step(self.axis);
            let owner = if self.forward { minus(at, e) } else { at };
            return Err(violation(Some(owner), Some(self.axis), Rule::Owner));
        }
        let finer = match level {
            TileLevel::Root => TileLevel::Upper,
            TileLevel::Upper => TileLevel::Lower,
            // A leaf's edges to a tile are the leaf's to check.
            TileLevel::Lower => return Ok(()),
        };
        let [a, b] = [step((self.axis + 1) % 3), step((self.axis + 2) % 3)];
        let edge = usize::try_from(finer.edge()).unwrap_or(usize::MAX);
        for i in (0..level.edge()).step_by(edge) {
            for j in (0..level.edge()).step_by(edge) {
                self.check_region(s, finer, plus(at, plus(a.map(|c| c * i), b.map(|c| c * j))))?;
            }
        }
        Ok(())
    }
}

/// A set in memory.
struct Owned<'a>(&'a Hermite);

impl Source for Owned<'_> {
    fn leaves(&self) -> Result<Vec<Coord>, HermiteError> {
        let t = &self.0.topology;
        Ok(t.leaf_numbers().map(|n| t.leaf_origin(n)).collect::<Result<_, _>>()?)
    }

    fn tiles(&self) -> Result<Vec<(TileLevel, Coord, u32)>, HermiteError> {
        let t = &self.0.topology;
        let mut out = Vec::with_capacity(t.tile_count());
        for n in t.tile_numbers() {
            let (level, origin) = t.tile(n)?;
            out.push((level, origin, n.get()));
        }
        Ok(out)
    }

    fn is_leaf(&self, v: Coord) -> Result<bool, HermiteError> {
        Ok(matches!(self.0.topology.locate(v), Location::Leaf(_)))
    }

    fn holds_node(&self, level: TileLevel, v: Coord) -> Result<bool, HermiteError> {
        let t = &self.0.topology;
        Ok(match level {
            TileLevel::Root => t.has_upper_node(v),
            TileLevel::Upper => t.has_lower_node(v),
            TileLevel::Lower => matches!(t.locate(v), Location::Leaf(_)),
        })
    }

    fn sign_at(&self, v: Coord) -> Result<u8, HermiteError> {
        Ok(self.0.signs.value_at(&self.0.topology, v)?)
    }

    fn sign_leaf(&self, n: u32) -> Result<&[u8; LEAF_VOXELS], HermiteError> {
        Ok(self.0.signs.leaf(&self.0.topology, crate::topology::LeafNumber(n))?)
    }

    fn crossing(
        &self,
        axis: usize,
        n: u32,
        place: usize,
    ) -> Result<Option<Crossing>, HermiteError> {
        let axis = Axis::ALL.get(axis).copied().ok_or(HermiteError::Missing("an axis"))?;
        Ok(self.0.crossings.get(LeafNumber(n), axis, place))
    }

    fn sign_tile(&self, t: u32) -> Result<u8, HermiteError> {
        Ok(self.0.signs.tile_value_at(t)?)
    }

    fn sign_background(&self) -> u8 {
        self.0.signs.background()
    }
}

impl Hermite {
    /// A set of `signs` and `crossings`, both on `topology`, with the group table
    /// `group_names`, indexed by id; refused unless it keeps to the format. A refused set's
    /// parts are dropped, not handed back: a caller who would mend them clones them first.
    pub fn new(
        topology: Topology,
        signs: Field<u8>,
        crossings: Crossings,
        group_names: Vec<String>,
    ) -> Result<Hermite, HermiteError> {
        if signs.belongs_to(&topology).is_err() {
            return Err(HermiteError::OtherTopology("sign"));
        }
        if !crossings.belongs_to(&topology) {
            return Err(HermiteError::OtherTopology(CROSSINGS));
        }
        // `[""]` names group 0 alone, as an empty table does; it is kept empty, as a file
        // reads it back.
        let group_names = if group_names == [""] { Vec::new() } else { group_names };
        let set = Hermite { topology, signs, crossings, group_names };
        check(&Owned(&set), &set.group_names)?;
        Ok(set)
    }

    /// The topology the signs are on.
    pub fn topology(&self) -> &Topology {
        &self.topology
    }

    /// The signs, 1 inside, 0 outside.
    pub fn signs(&self) -> &Field<u8> {
        &self.signs
    }

    /// The crossings.
    pub fn crossings(&self) -> &Crossings {
        &self.crossings
    }

    /// Each group's name, by id; empty, every group is 0.
    pub fn group_names(&self) -> &[String] {
        &self.group_names
    }

    /// `meta` with the keys of the set named `set` for field `field`; refused if `meta`
    /// holds a key beginning `brodo.`, the SDF convention's among them.
    fn keyed(meta: &GridMeta, set: &str, field: &str) -> Result<GridMeta, HermiteError> {
        if meta.extra.contains_key(crate::meta::SDF_KEY) {
            return Err(HermiteError::ClaimsSdf);
        }
        if let Some(key) = meta.extra.keys().find(|k| crate::cache::reserved(k)) {
            return Err(HermiteError::ReservedKey(key.clone()));
        }
        let mut m = meta.clone();
        m.extra.insert(KEY_VERSION.to_owned(), VERSION.to_owned());
        m.extra.insert(KEY_SET.to_owned(), set.to_owned());
        m.extra.insert(KEY_FIELD.to_owned(), field.to_owned());
        Ok(m)
    }

    /// The sign grid's entry, with the group table.
    fn sign_entry(&self, set: &str, meta: &GridMeta) -> Result<CacheEntry, HermiteError> {
        let mut sign_meta = Self::keyed(meta, set, "sign")?;
        for (id, name) in self.group_names.iter().enumerate().skip(1) {
            sign_meta.extra.insert(format!("{KEY_GROUP}{id}"), name.clone());
        }
        Ok(CacheEntry::with_reserved_keys(&sign_meta, &self.topology, &self.signs)?)
    }

    /// The cache entries of the set named `set`: its sign grid, with `meta`, and its entry
    /// of bytes holding the crossings, named as `meta` is (docs/formats.md §4), for a cache
    /// file of version 2. `meta` may hold no key beginning `brodo.`, and so does not state
    /// the SDF convention (`SDF_KEY`).
    pub fn entries(&self, set: &str, meta: &GridMeta) -> Result<Vec<CacheEntry>, HermiteError> {
        let sign = self.sign_entry(set, meta)?;
        let mut extra = BTreeMap::new();
        extra.insert(KEY_VERSION.to_owned(), VERSION.to_owned());
        extra.insert(KEY_SET.to_owned(), set.to_owned());
        extra.insert(KEY_FIELD.to_owned(), CROSSINGS.to_owned());
        let bytes =
            CacheEntry::bytes_with_reserved_keys(&meta.name, extra, self.crossings.to_bytes());
        Ok(vec![sign, bytes])
    }
}

/// A set read from a file.
struct Viewed<'a, 'b> {
    set: &'b HermiteView<'a>,
    regions: crate::view::Regions,
}

impl Source for Viewed<'_, '_> {
    fn leaves(&self) -> Result<Vec<Coord>, HermiteError> {
        Ok(self.regions.leaves.clone())
    }

    fn tiles(&self) -> Result<Vec<(TileLevel, Coord, u32)>, HermiteError> {
        Ok(self.regions.tiles.clone())
    }

    fn is_leaf(&self, v: Coord) -> Result<bool, HermiteError> {
        Ok(matches!(self.set.signs.locate(v)?, ViewLocation::Leaf(_)))
    }

    fn holds_node(&self, level: TileLevel, v: Coord) -> Result<bool, HermiteError> {
        Ok(self.set.signs.holds_node(level, v)?)
    }

    fn sign_at(&self, v: Coord) -> Result<u8, HermiteError> {
        Ok(self.set.signs.get(v)?)
    }

    fn sign_leaf(&self, n: u32) -> Result<&[u8; LEAF_VOXELS], HermiteError> {
        Ok(self.set.signs.leaf(n)?)
    }

    fn crossing(
        &self,
        axis: usize,
        n: u32,
        place: usize,
    ) -> Result<Option<Crossing>, HermiteError> {
        let c = &self.set.crossings;
        sparse_crossing(c.masks, c.items, &c.starts, n, axis, place)
    }

    fn sign_tile(&self, t: u32) -> Result<u8, HermiteError> {
        Ok(self.set.signs.tile_value(t)?)
    }

    fn sign_background(&self) -> u8 {
        self.set.signs.background()
    }
}

/// Leaf `n`'s masks in a set's entry of bytes.
fn leaf_masks(masks: &[u8], n: u32) -> Option<[[u64; 8]; 3]> {
    let at = usize::try_from(n).ok()?.checked_mul(MASK_BYTES)?;
    let bytes = masks.get(at..at.checked_add(MASK_BYTES)?)?;
    let mut out = [[0_u64; 8]; 3];
    for (w, b) in out.iter_mut().flatten().zip(bytes.as_chunks::<8>().0) {
        *w = u64::from_le_bytes(*b);
    }
    Some(out)
}

/// The crossing an entry of bytes holds on leaf `n`'s point at `place` and edge along
/// `axis`, if its mask's bit is set.
fn sparse_crossing(
    masks: &[u8],
    items: &[u8],
    starts: &[u32],
    n: u32,
    axis: usize,
    place: usize,
) -> Result<Option<Crossing>, HermiteError> {
    let lost = || HermiteError::OtherTopology(CROSSINGS);
    let m = leaf_masks(masks, n).ok_or_else(lost)?;
    let (before, set) = rank(&m, axis, place).ok_or_else(lost)?;
    if !set {
        return Ok(None);
    }
    let k = starts.get(usize::try_from(n).map_err(|_| lost())?).ok_or_else(lost)?;
    let at = usize::try_from(k.checked_add(before).ok_or_else(lost)?).map_err(|_| lost())?;
    let at = at.checked_mul(CROSSING_BYTES).ok_or_else(lost)?;
    let b = items.get(at..at.checked_add(CROSSING_BYTES).ok_or_else(lost)?).ok_or_else(lost)?;
    let mut words = b.as_chunks::<4>().0.iter().copied();
    let mut next = || words.next().ok_or_else(lost);
    let fraction = f32::from_le_bytes(next()?);
    let normal =
        [f32::from_le_bytes(next()?), f32::from_le_bytes(next()?), f32::from_le_bytes(next()?)];
    Ok(Some(Crossing { fraction, normal, group: u32::from_le_bytes(next()?) }))
}

/// An entry of crossings checked against `leaves` leaves (docs/formats.md §4.2, rule 3): its
/// masks and its crossings, and where each leaf's start.
fn sparse(bytes: &[u8], leaves: usize) -> Result<ViewCrossings<'_>, HermiteError> {
    let lost = || HermiteError::OtherTopology(CROSSINGS);
    let (masks, items) = bytes
        .split_at_checked(leaves.checked_mul(MASK_BYTES).ok_or_else(lost)?)
        .ok_or_else(lost)?;
    let mut starts = Vec::with_capacity(leaves.saturating_add(1));
    let mut total = 0_u32;
    starts.push(total);
    for leaf in masks.as_chunks::<MASK_BYTES>().0 {
        let ones: u32 =
            leaf.as_chunks::<8>().0.iter().map(|w| u64::from_le_bytes(*w).count_ones()).sum();
        total = total.checked_add(ones).ok_or_else(lost)?;
        starts.push(total);
    }
    let expected = usize::try_from(total).ok().and_then(|n| n.checked_mul(CROSSING_BYTES));
    if expected != Some(items.len()) {
        return Err(lost());
    }
    Ok(ViewCrossings { masks, items, starts })
}

/// The group table in a sign grid's `extra`: ids from 1 with no gap.
fn group_table(meta: &GridMeta) -> Result<Vec<String>, HermiteError> {
    let mut by_id = BTreeMap::new();
    for (key, name) in &meta.extra {
        let Some(id) = key.strip_prefix(KEY_GROUP) else { continue };
        let n: u32 = id.parse().map_err(|_| HermiteError::BadGroupKey(key.clone()))?;
        if n == 0 || n.to_string() != id {
            return Err(HermiteError::BadGroupKey(key.clone()));
        }
        by_id.insert(n, name.clone());
    }
    let mut names = vec![String::new()];
    for (expected, (id, name)) in (1_u32..).zip(by_id) {
        if id != expected {
            return Err(HermiteError::BadGroupName(expected));
        }
        names.push(name);
    }
    if names.len() == 1 {
        names.clear();
    }
    Ok(names)
}

/// Every key/value set of `file`'s entries, grids first, then entries of bytes.
fn extras(file: &CacheFile) -> Result<Vec<&BTreeMap<String, String>>, HermiteError> {
    let mut out = Vec::with_capacity(file.grid_count() + file.bytes_entries().len());
    for i in 0..file.grid_count() {
        out.push(&file.meta(i)?.extra);
    }
    out.extend(file.bytes_entries().into_iter().map(|b| b.extra));
    Ok(out)
}

/// Checks that every entry of the set named `set` in `file` states this version, and that
/// no set of that name was written by an earlier one, whose keys began `hermite.`.
fn check_version(file: &CacheFile, set: &str) -> Result<(), HermiteError> {
    for extra in extras(file)? {
        if extra.get(KEY_SET).map(String::as_str) == Some(set) {
            let v = extra.get(KEY_VERSION).map(String::as_str).unwrap_or_default();
            if v != VERSION {
                return Err(HermiteError::Version(v.to_owned()));
            }
        } else if extra.get(OLD_SET).map(String::as_str) == Some(set) {
            let v = extra.get(OLD_VERSION).cloned().unwrap_or_default();
            return Err(HermiteError::Version(v));
        }
    }
    Ok(())
}

impl<'a> HermiteView<'a> {
    /// The set named `set` in `file`, its sign grid and its entry of crossings found by
    /// their keys and checked whole, as docs/formats.md §4.2 states; of several broken
    /// rules, which is named is not promised. A set written by an earlier version, its keys
    /// beginning `hermite.`, is refused with `Version`.
    pub fn find(file: &'a CacheFile, set: &str) -> Result<HermiteView<'a>, HermiteError> {
        check_version(file, set)?;
        let mut sign = None;
        for i in 0..file.grid_count() {
            let extra = &file.meta(i)?.extra;
            if extra.get(KEY_SET).map(String::as_str) != Some(set) {
                continue;
            }
            let field = extra.get(KEY_FIELD).cloned().unwrap_or_default();
            if field != "sign" {
                return Err(HermiteError::UnknownField(field));
            }
            if sign.replace(i).is_some() {
                return Err(HermiteError::Twice("sign"));
            }
        }
        let sign = sign.ok_or(HermiteError::Missing("sign"))?;
        let mut crossings = None;
        for b in file.bytes_entries() {
            if b.extra.get(KEY_SET).map(String::as_str) != Some(set) {
                continue;
            }
            let field = b.extra.get(KEY_FIELD).cloned().unwrap_or_default();
            if field != CROSSINGS {
                return Err(HermiteError::UnknownField(field));
            }
            let ours = [KEY_VERSION, KEY_SET, KEY_FIELD];
            if let Some(k) =
                b.extra.keys().find(|k| k.starts_with(PREFIX) && !ours.contains(&k.as_str()))
            {
                return Err(HermiteError::ReservedKey(k.clone()));
            }
            if crossings.replace(b.bytes).is_some() {
                return Err(HermiteError::Twice(CROSSINGS));
            }
        }
        let crossings = crossings.ok_or(HermiteError::Missing(CROSSINGS))?;
        let signs: GridView<'a, u8> = file.view(sign)?;
        let view = HermiteView {
            crossings: sparse(crossings, signs.leaf_count())?,
            signs,
            group_names: group_table(file.meta(sign)?)?,
        };
        let regions = view.signs.regions()?;
        check(&Viewed { set: &view, regions }, &view.group_names)?;
        Ok(view)
    }

    /// The signs, 1 inside, 0 outside.
    pub fn signs(&self) -> &GridView<'a, u8> {
        &self.signs
    }

    /// The crossing on the edge along `axis` of leaf `leaf`'s point at `place`
    /// (x + 8y + 64z), if the edge holds one.
    pub fn crossing(
        &self,
        leaf: u32,
        axis: Axis,
        place: usize,
    ) -> Result<Option<Crossing>, HermiteError> {
        let c = &self.crossings;
        sparse_crossing(c.masks, c.items, &c.starts, leaf, axis.index(), place)
    }

    /// Each group's name, by id; empty, every group is 0.
    pub fn group_names(&self) -> &[String] {
        &self.group_names
    }
}

impl From<FormatError> for HermiteError {
    fn from(e: FormatError) -> Self {
        HermiteError::Format(e)
    }
}
