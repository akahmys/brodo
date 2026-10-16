//! Every candidate against a dense grid. The dense grid holds a value per voxel of a box of
//! 2³ blocks (dim 2) and is updated by meaning alone: a commit leaves every leaf voxel as it
//! was, and gives every voxel of each new tile the merge rule applied to the old values of
//! the voxels the tile covers. A rule refusing anywhere refuses the commit. Values are small
//! integers, so sums and means are exact and M-a and M-b must agree to the bit.

use merge_spike::*;
use proptest::prelude::*;

const DIM: i32 = 2;
const RLO: i32 = -2;
const RHI: i32 = 2;
const VLO: i32 = RLO * 8;
const SIDE: usize = 32;
const BACKGROUND: f64 = 1.0;

#[derive(Clone)]
struct Dense(Vec<f64>);

impl Dense {
    fn idx(v: Voxel) -> usize {
        let [x, y, z] = v.map(|c| (c - VLO) as usize);
        x + SIDE * (y + SIDE * z)
    }
    fn voxels() -> impl Iterator<Item = Voxel> {
        let r = VLO..-VLO;
        r.clone().flat_map(move |z| {
            let r2 = VLO..-VLO;
            r2.flat_map(move |y| (VLO..-VLO).map(move |x| [x, y, z]))
        })
    }
}

fn region_voxels(r: Region) -> impl Iterator<Item = Voxel> {
    (0..LEAF_VOXELS).map(move |i| voxel_of(r, i))
}

fn tile_voxels(k: TileKey) -> Vec<Voxel> {
    match k {
        TileKey::Leaf(r) => region_voxels(r).collect(),
        TileKey::Block(b) => children(b, DIM).flat_map(region_voxels).collect(),
    }
}

#[derive(Clone, Copy, Debug)]
enum RuleKind {
    UniformVoxels,
    UniformCounted,
    MeanVoxels,
    MeanCounted,
}

const RULES: [RuleKind; 4] =
    [RuleKind::UniformVoxels, RuleKind::UniformCounted, RuleKind::MeanVoxels, RuleKind::MeanCounted];

fn rule(k: RuleKind) -> Rule<'static, f64> {
    match k {
        RuleKind::UniformVoxels => Rule::Voxels(&uniform_voxels::<f64>),
        RuleKind::UniformCounted => Rule::Counted(&uniform_counted::<f64>),
        RuleKind::MeanVoxels => Rule::Voxels(&mean_voxels),
        RuleKind::MeanCounted => Rule::Counted(&mean_counted),
    }
}

/// The rule's meaning, on the flat list of voxel values.
fn flat(k: RuleKind, v: &[f64]) -> Option<f64> {
    match k {
        RuleKind::UniformVoxels | RuleKind::UniformCounted => {
            v.iter().all(|x| x.to_bits() == v[0].to_bits()).then_some(v[0])
        }
        RuleKind::MeanVoxels | RuleKind::MeanCounted => Some(v.iter().sum::<f64>() / v.len() as f64),
    }
}

/// The dense grid after a commit to `t2`, or nothing if the rule refuses a tile.
fn expected(t2: &Topology, d: &Dense, k: RuleKind) -> Option<Dense> {
    let mut d2 = d.clone();
    for key in t2.tiles() {
        let vox = tile_voxels(*key);
        let vals: Vec<f64> = vox.iter().map(|v| d.0[Dense::idx(*v)]).collect();
        let x = flat(k, &vals)?;
        for v in vox {
            d2.0[Dense::idx(v)] = x;
        }
    }
    Some(d2)
}

struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.0 >> 33
    }
}

/// Writes the same values to the field and the dense grid: each leaf uniform or not, at
/// random, so that rules both accept and refuse.
fn write(t: &Topology, f: &mut FieldData<f64>, d: &mut Dense, seed: u64) {
    let mut g = Lcg(seed);
    // One round in three writes one value everywhere, so uniform rules accept merges.
    let everywhere = (g.next() % 3 == 0).then(|| (g.next() % 3) as f64);
    for (n, r) in t.leaves().iter().enumerate() {
        let uniform = g.next() % 2 == 0;
        let k = (g.next() % 3) as f64;
        for (i, x) in f.leaf_mut(n as u32).iter_mut().enumerate() {
            *x = everywhere.unwrap_or(if uniform { k } else { (g.next() % 3) as f64 });
            d.0[Dense::idx(voxel_of(*r, i))] = *x;
        }
    }
    for (n, key) in t.tiles().iter().enumerate() {
        let x = everywhere.unwrap_or((g.next() % 3) as f64);
        f.tile_values[n] = x;
        for v in tile_voxels(*key) {
            d.0[Dense::idx(v)] = x;
        }
    }
}

/// One edit, or a block's every region filled with leaves or tiles, so that blocks that
/// hold no empty region are common.
fn op() -> impl Strategy<Value = Vec<Op>> {
    let r = [RLO..RHI, RLO..RHI, RLO..RHI];
    let b = [-1..1_i32, -1..1, -1..1];
    let fill = |b: Block, tiles: bool| -> Vec<Op> {
        children(b, DIM).map(|c| if tiles { Op::AddTile(c) } else { Op::Activate(c) }).collect()
    };
    prop_oneof![
        4 => r.clone().prop_map(|x| vec![Op::Activate(x)]),
        2 => r.clone().prop_map(|x| vec![Op::AddTile(x)]),
        1 => b.clone().prop_map(|x| vec![Op::AddBlockTile(x)]),
        1 => r.clone().prop_map(|x| vec![Op::SplitTile(x)]),
        1 => b.clone().prop_map(|x| vec![Op::SplitBlock(x)]),
        2 => r.prop_map(|x| vec![Op::MergeLeaf(x)]),
        3 => b.clone().prop_map(|x| vec![Op::MergeBlock(x)]),
        2 => (b, any::<bool>()).prop_map(move |(x, tiles)| fill(x, tiles)),
    ]
}

/// Drops the edits the shape refuses, one by one, so that most commits apply.
fn valid(t: &Topology, ops: &[Op], p: Policy) -> Vec<Op> {
    let mut s = t.shape().clone();
    let mut kept = Vec::new();
    for op in ops {
        if let Ok(next) = s.apply(&[*op], p) {
            s = next;
            kept.push(*op);
        }
    }
    kept
}

fn run(p: Policy, r2: bool, k: RuleKind, commits: &[(Vec<Op>, u64)]) -> Result<(), TestCaseError> {
    let mut t = Topology::new(DIM);
    let mut f = FieldData::new(&t, BACKGROUND);
    let mut d = Dense(vec![BACKGROUND; SIDE * SIDE * SIDE]);
    for (ops, seed) in commits {
        let ops = valid(&t, ops, p);
        let (t2, values) = if r2 {
            let (t2, r) = commit_r2(&t, &ops, p).unwrap();
            let v = remap(&f, &r, rule(k));
            (t2, v)
        } else {
            let (t2, r) = commit_r1(&t, &ops, p).unwrap();
            let v = remap(&f, &r, rule(k));
            (t2, v)
        };
        match (values, expected(&t2, &d, k)) {
            (Ok(f2), Some(d2)) => {
                for v in Dense::voxels() {
                    prop_assert_eq!(f2.value_at(&t2, v).to_bits(), d2.0[Dense::idx(v)].to_bits(), "at {:?}", v);
                }
                (t, f, d) = (t2, f2, d2);
            }
            (Err(SpikeError::Refused(_)), None) => {}
            (got, want) => prop_assert!(false, "{:?} {:?} {:?}: remap {:?}, dense {}", p, r2, k, got.err(), want.is_some()),
        }
        write(&t, &mut f, &mut d, *seed);
    }
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    #[test]
    fn every_candidate_matches_the_dense_grid(
        commits in prop::collection::vec((prop::collection::vec(op(), 1..10).prop_map(|v| v.concat()), any::<u64>()), 1..6)
    ) {
        for p in POLICIES {
            for r2 in [false, true] {
                for k in RULES {
                    run(p, r2, k, &commits)?;
                }
            }
        }
    }
}

#[test]
fn stepwise_refuses_a_leaf_and_refuse_refuses_an_empty_region() {
    let t = Topology::new(DIM);
    let s1 = POLICIES[0];
    let (t, _) = commit_r2(&t, &[Op::Activate([0, 0, 0]), Op::AddTile([1, 0, 0])], s1).unwrap();
    let merge = [Op::MergeBlock([0, 0, 0])];
    assert_eq!(t.shape().apply(&merge, s1).err(), Some(SpikeError::NotAllTiles([0, 0, 0])));
    let s2_refuse = POLICIES[2];
    assert_eq!(t.shape().apply(&merge, s2_refuse).err(), Some(SpikeError::HoldsBackground([0, 0, 0])));
    let s2_include = POLICIES[3];
    let (t2, r) = commit_r2(&t, &merge, s2_include).unwrap();
    assert_eq!(t2.tiles(), &[TileKey::Block([0, 0, 0])]);
    assert_eq!(r.parts, vec![Part::Leaf(0), Part::Tile(0), Part::Background(6)]);
}

#[test]
fn same_bits_at_any_thread_count() {
    let p = POLICIES[3];
    let fill: Vec<Op> = (0..8).flat_map(|i| children([i % 2 - 1, i / 2 % 2 - 1, i / 4 - 1], DIM)).map(Op::Activate).collect();
    let (t, _) = commit_r2(&Topology::new(DIM), &fill, p).unwrap();
    let mut f = FieldData::new(&t, BACKGROUND);
    let mut d = Dense(vec![BACKGROUND; SIDE * SIDE * SIDE]);
    write(&t, &mut f, &mut d, 5);
    let ops: Vec<Op> = (0..8).map(|i| Op::MergeBlock([i % 2 - 1, i / 2 % 2 - 1, i / 4 - 1])).collect();
    let (_, r) = commit_r2(&t, &ops, p).unwrap();
    let run = |threads| {
        let pool = rayon::ThreadPoolBuilder::new().num_threads(threads).build().unwrap();
        pool.install(|| {
            let g = remap(&f, &r, rule(RuleKind::MeanCounted)).unwrap();
            g.leaf_values.iter().chain(&g.tile_values).map(|x| x.to_bits()).collect::<Vec<u64>>()
        })
    };
    let one = run(1);
    assert_eq!(one.len(), 8);
    assert_eq!(run(2), one);
    assert_eq!(run(8), one);
}
