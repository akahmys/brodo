//! The shared remap against a reference written here: each region holds its values
//! directly, edits act on values, and nothing is numbered. After every commit, three fields
//! (M2 mean, M1 on vectors, M1 on bytes) must hold the same bits as the reference, and a
//! refusal must happen in both or in neither. Also: the same bits at any thread count.

use std::collections::BTreeMap;

use proptest::prelude::*;
use remap_spike::core::*;

trait FromK: Value {
    fn from_k(k: u32) -> Self;
}
impl FromK for f32 {
    fn from_k(k: u32) -> Self {
        k as f32 * 0.5 // multiples of 1/2: sums of 512 are exact, so a mean is exact
    }
}
impl FromK for [f32; 3] {
    fn from_k(k: u32) -> Self {
        [k as f32 * 0.5, 1.0, 0.0]
    }
}
impl FromK for u8 {
    fn from_k(k: u32) -> Self {
        k as u8
    }
}

/// A region's content in the reference. A merge is evaluated only when its value is needed,
/// as `remap_data` does: a merged leaf later removed is never asked.
#[derive(Clone, Debug)]
enum RefVal<T> {
    Leaf(Vec<T>),
    Tile(T),
    /// A tile whose value is the merge of these leaf values.
    MergedTile(Vec<T>),
    /// A leaf split from a `MergedTile`.
    SplitOfMerged(Vec<T>),
}

#[derive(Clone)]
struct RefField<T> {
    regions: BTreeMap<Region, RefVal<T>>,
    background: T,
}

impl<T: FromK> RefField<T> {
    fn commit(&self, ops: &[Op], merge: &MergeRule<T>) -> Option<Self> {
        let mut m = self.regions.clone();
        for op in ops {
            match *op {
                Op::Activate(v) => {
                    let r = region_of(v);
                    let next = match m.get(&r) {
                        None => RefVal::Leaf(vec![self.background; LEAF_VOXELS]),
                        Some(RefVal::Tile(t)) => RefVal::Leaf(vec![*t; LEAF_VOXELS]),
                        Some(RefVal::MergedTile(v)) => RefVal::SplitOfMerged(v.clone()),
                        Some(x @ (RefVal::Leaf(_) | RefVal::SplitOfMerged(_))) => x.clone(),
                    };
                    m.insert(r, next);
                }
                Op::Split(r) => {
                    let next = match m.get(&r)? {
                        RefVal::Tile(t) => RefVal::Leaf(vec![*t; LEAF_VOXELS]),
                        RefVal::MergedTile(v) => RefVal::SplitOfMerged(v.clone()),
                        RefVal::Leaf(_) | RefVal::SplitOfMerged(_) => return None,
                    };
                    m.insert(r, next);
                }
                Op::Merge(r) => {
                    let next = match m.get(&r)? {
                        RefVal::Leaf(v) | RefVal::SplitOfMerged(v) => RefVal::MergedTile(v.clone()),
                        RefVal::Tile(_) | RefVal::MergedTile(_) => return None,
                    };
                    m.insert(r, next);
                }
                Op::AddTile(r) => {
                    if m.contains_key(&r) {
                        return None;
                    }
                    m.insert(r, RefVal::Tile(self.background));
                }
                Op::Remove(r) => {
                    m.remove(&r)?;
                }
            }
        }
        let mut regions = BTreeMap::new();
        for (r, v) in m {
            let done = match v {
                RefVal::MergedTile(v) => RefVal::Tile(merge(&v)?),
                RefVal::SplitOfMerged(v) => RefVal::Leaf(vec![merge(&v)?; LEAF_VOXELS]),
                x @ (RefVal::Leaf(_) | RefVal::Tile(_)) => x,
            };
            regions.insert(r, done);
        }
        Some(Self { regions, background: self.background })
    }
}

/// Whether the core field and the reference agree on every region and value.
fn agrees<T: Value>(shape: &Shape, f: &FieldData<T>, r: &RefField<T>) -> bool {
    let regions: Vec<Region> = shape.leaves.iter().chain(&shape.tiles).copied().collect();
    if regions.len() != r.regions.len() {
        return false;
    }
    regions.iter().all(|reg| match (shape.slot(*reg), r.regions.get(reg)) {
        (Slot::Leaf(i), Some(RefVal::Leaf(v))) => {
            f.leaf(i).iter().zip(v).all(|(a, b)| a.same_bits(b))
        }
        (Slot::Tile(j), Some(RefVal::Tile(t))) => f.tile_values[j as usize].same_bits(t),
        _ => false,
    })
}

struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> u32 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (self.0 >> 33) as u32
    }
}

/// Writes the same values to the core field and the reference: each leaf uniform or not,
/// at random, so merges are both accepted and refused.
fn write<T: FromK>(shape: &Shape, f: &mut FieldData<T>, r: &mut RefField<T>, seed: u64) {
    let mut g = Lcg(seed);
    for (i, reg) in shape.leaves.iter().enumerate() {
        let uniform = g.next() % 2 == 0;
        let k = g.next() % 4;
        let vals: Vec<T> =
            (0..LEAF_VOXELS).map(|_| T::from_k(if uniform { k } else { g.next() % 4 })).collect();
        f.leaf_mut(i as u32).copy_from_slice(&vals);
        r.regions.insert(*reg, RefVal::Leaf(vals));
    }
    for (j, reg) in shape.tiles.iter().enumerate() {
        let v = T::from_k(g.next() % 4);
        f.tile_values[j] = v;
        r.regions.insert(*reg, RefVal::Tile(v));
    }
}

fn op_strategy() -> impl Strategy<Value = Op> {
    (0..5_u8, 0..3_i32, 0..3_i32, 0..3_i32, 0..8_i32).prop_map(|(k, x, y, z, l)| {
        let r = [x, y, z];
        match k {
            0 => Op::Activate([x * 8 + l, y * 8 + (7 - l), z * 8]),
            1 => Op::Split(r),
            2 => Op::Merge(r),
            3 => Op::AddTile(r),
            _ => Op::Remove(r),
        }
    })
}

/// Drops the edits that would be refused, given the regions so far, so most commits apply
/// and exercise splits and merges; `keep` leaves them in, to exercise the refusal.
fn sanitize(shape: &Shape, ops: Vec<Op>, keep: bool) -> Vec<Op> {
    if keep {
        return ops;
    }
    let mut kinds: BTreeMap<Region, bool> = BTreeMap::new(); // true: leaf, false: tile
    kinds.extend(shape.leaves.iter().map(|r| (*r, true)));
    kinds.extend(shape.tiles.iter().map(|r| (*r, false)));
    ops.into_iter()
        .filter(|op| {
            let (r, ok, next) = match *op {
                Op::Activate(v) => (region_of(v), true, Some(true)),
                Op::Split(r) => (r, kinds.get(&r) == Some(&false), Some(true)),
                Op::Merge(r) => (r, kinds.get(&r) == Some(&true), Some(false)),
                Op::AddTile(r) => (r, !kinds.contains_key(&r), Some(false)),
                Op::Remove(r) => (r, kinds.contains_key(&r), None),
            };
            if ok {
                match next {
                    Some(k) => kinds.insert(r, k),
                    None => kinds.remove(&r),
                };
            }
            ok
        })
        .collect()
}

struct State<T> {
    f: FieldData<T>,
    r: RefField<T>,
}

impl<T: FromK> State<T> {
    fn new(background: T) -> Self {
        let r = RefField { regions: BTreeMap::new(), background };
        Self { f: FieldData::new(&Shape::default(), background), r }
    }
}

/// One field's remap and the reference's commit; returns the new pair, or `None` for a
/// refusal, after checking both refused or both accepted.
fn step<T: FromK>(
    s: &State<T>,
    remap: &Remap,
    ops: &[Op],
    merge: &MergeRule<T>,
) -> Result<Option<State<T>>, TestCaseError> {
    let core = remap_data(&s.f, remap, merge);
    let reference = s.r.commit(ops, merge);
    prop_assert_eq!(core.is_ok(), reference.is_some(), "refused in one only: {:?}", core.err());
    Ok(core.ok().zip(reference).map(|(f, r)| State { f, r }))
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(4096))]

    #[test]
    fn remap_keeps_every_fields_values(
        commits in prop::collection::vec((prop::collection::vec(op_strategy(), 0..10), any::<u64>(), 0..8_u8), 1..8)
    ) {
        let mut shape = Shape::default();
        let mut a = State::new(0.0_f32);
        let mut b = State::new([0.0_f32; 3]);
        let mut c = State::new(0_u8);
        let mean = |v: &[f32]| Some(v.iter().sum::<f32>() / LEAF_VOXELS as f32);
        for (ops, seed, keep) in commits {
            let ops = sanitize(&shape, ops, keep == 0);
            let Ok((next, remap)) = apply(&shape, &ops) else {
                prop_assert!(a.r.commit(&ops, &mean).is_none(), "the reference accepts {:?}", ops);
                continue;
            };
            let (na, nb, nc) = (
                step(&a, &remap, &ops, &mean)?,
                step(&b, &remap, &ops, &uniform)?,
                step(&c, &remap, &ops, &uniform)?,
            );
            // The commit is all or nothing, as candidate B makes it.
            let (Some(na), Some(nb), Some(nc)) = (na, nb, nc) else { continue };
            shape = next;
            (a, b, c) = (na, nb, nc);
            prop_assert!(agrees(&shape, &a.f, &a.r) && agrees(&shape, &b.f, &b.r) && agrees(&shape, &c.f, &c.r));
            write(&shape, &mut a.f, &mut a.r, seed);
            write(&shape, &mut b.f, &mut b.r, seed);
            write(&shape, &mut c.f, &mut c.r, seed);
        }
    }
}

#[test]
fn same_bits_at_any_thread_count() {
    let ops: Vec<Op> = (0..400).map(|i| Op::Activate([i % 20 * 8, i / 20 * 8, 0])).collect();
    let (shape, _) = apply(&Shape::default(), &ops).unwrap();
    let mut f = FieldData::new(&shape, 0.0_f32);
    let mut r = RefField { regions: BTreeMap::new(), background: 0.0_f32 };
    write(&shape, &mut f, &mut r, 7);
    let edit: Vec<Op> = (0..400)
        .filter(|i| i % 3 == 0)
        .map(|i| Op::Merge([i % 20, i / 20, 0]))
        .chain((0..50).map(|i| Op::Activate([i * 8, 200, 0])))
        .collect();
    let (shape2, remap) = apply(&shape, &edit).unwrap();
    let mean = |v: &[f32]| Some(v.iter().sum::<f32>() / LEAF_VOXELS as f32);
    let run = |threads: usize| {
        let pool = rayon::ThreadPoolBuilder::new().num_threads(threads).build().unwrap();
        pool.install(|| {
            let mut g = remap_data(&f, &remap, &mean).unwrap();
            par_update(&mut g, |leaf, out| {
                for (k, v) in out.iter_mut().enumerate() {
                    *v = *v * 0.75 + (leaf as f32 + k as f32).sin();
                }
            });
            g
        })
    };
    let one = run(1);
    assert_eq!(one.leaf_values.len(), shape2.leaves.len() * LEAF_VOXELS);
    for threads in [2, 8] {
        let other = run(threads);
        let bits = |g: &FieldData<f32>| -> Vec<u32> {
            g.leaf_values.iter().chain(&g.tile_values).map(|v| v.to_bits()).collect()
        };
        assert_eq!(bits(&one), bits(&other), "{threads} threads");
    }
}
