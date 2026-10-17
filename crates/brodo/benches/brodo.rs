//! Benchmark tier (TESTING.md §1), on the harness ADR-0019 chose: std only, built to T5.
//!
//! `cargo bench -p brodo --bench brodo` runs two child processes, each a fresh process.
//! Each child runs every item once per round, in turn, starting from a different item each
//! round, for `ROUNDS` rounds, writing over an `EVICT_BYTES` buffer before each run, and
//! keeps each item's fastest time. The parent writes both children's times, how far they
//! agree, and the change from the baseline of this hardware model to
//! `out/bench/report.tsv`, and fails when an item is more than `TOLERANCE` slower than its
//! baseline in both. With `--record` it instead replaces this model's rows of
//! `benches/baseline.tsv` with the faster of the two times, dated.
//!
//! Run and record on a machine doing nothing else: with other work running, the
//! leaf-parallel update failed its baseline at +21 to +27 % with no change to the code;
//! at rest it stayed within 12 % (ADR-0019).
//!
//! Nothing here prints (CODING.md Rule 18): results go to files, and a failure is the error
//! `main` returns. The file items read and write through the operating system's page
//! cache, which the eviction buffer does not empty.
//!
//! The workload is a narrow-band sphere SDF of radius 80, the textbook |p| − r clamped to a
//! band of ±3: a leaf where the band passes, an inside tile where all of a leaf's region
//! lies below it.

use std::collections::BTreeMap;
use std::fmt;
use std::hint::black_box;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use brodo::accessor::ValueAccessor;
use brodo::cache::{self, CacheEntry, CacheFile};
use brodo::field::Field;
use brodo::frozen::freeze;
use brodo::gather::{GATHER_VOXELS, gather};
use brodo::meta::GridMeta;
use brodo::parallel::par_update;
use brodo::topology::Topology;
use brodo::tree::{Coord, TileLevel};

/// Rounds per child: the two children of a pair agreed within 1.7 % from 20 rounds on
/// (ADR-0019).
const ROUNDS: usize = 20;
/// Bytes written over before each run: 256 MiB evicted the caches on an Apple M5 where
/// 32 MiB did not (ADR-0019).
const EVICT_BYTES: usize = 256 << 20;
/// How much slower than its baseline an item may be, in both children, before it fails
/// (ADR-0019).
const TOLERANCE: f64 = 0.10;
/// The sphere's radius, and the band its SDF is clamped to.
const RADIUS: i32 = 80;
const BAND: f32 = 3.0;
/// Voxels read by the random-access items.
const LOOKUPS: usize = 100_000;

/// A failure, shown by `main`'s return as its message alone.
struct Failure(String);

impl fmt::Debug for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl<E: std::error::Error> From<E> for Failure {
    fn from(e: E) -> Self {
        Failure(e.to_string())
    }
}

fn out_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../out/bench")
}

fn baseline_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("benches/baseline.tsv")
}

fn main() -> Result<(), Failure> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let [mode, path, ..] = args.as_slice()
        && mode == "child"
    {
        return child(Path::new(path));
    }
    let record = args.iter().any(|a| a == "--record");
    std::fs::create_dir_all(out_dir())?;
    let runs = [run_child(0)?, run_child(1)?];
    let model = hardware_model()?;
    if record { record_baseline(&runs, &model) } else { compare(&runs, &model) }
}

/// Runs one child in a fresh process and reads its times: item name to fastest ns.
fn run_child(k: usize) -> Result<BTreeMap<String, u128>, Failure> {
    let path = out_dir().join(format!("child-{k}.tsv"));
    let status = Command::new(std::env::current_exe()?).arg("child").arg(&path).status()?;
    if !status.success() {
        return Err(Failure(format!("child {k} failed: {status}")));
    }
    let mut times = BTreeMap::new();
    for line in std::fs::read_to_string(&path)?.lines() {
        let (name, ns) = line.split_once('\t').ok_or(Failure(format!("bad line {line:?}")))?;
        times.insert(name.to_owned(), ns.parse()?);
    }
    Ok(times)
}

/// The processor's model, never the host's name: `machdep.cpu.brand_string` on macOS, the
/// first `model name` of `/proc/cpuinfo` on Linux.
fn hardware_model() -> Result<String, Failure> {
    if let Ok(out) = Command::new("sysctl").args(["-n", "machdep.cpu.brand_string"]).output()
        && out.status.success()
    {
        let model = String::from_utf8_lossy(&out.stdout).trim().to_owned();
        if !model.is_empty() {
            return Ok(model);
        }
    }
    let cpuinfo = std::fs::read_to_string("/proc/cpuinfo").unwrap_or_default();
    cpuinfo
        .lines()
        .find_map(|l| l.strip_prefix("model name").and_then(|r| r.split_once(':')))
        .map(|(_, m)| m.trim().to_owned())
        .filter(|m| !m.is_empty() && !m.contains('\t'))
        .ok_or(Failure("the processor's model could not be read".to_owned()))
}

/// Today's date in UTC, from the days since 1970-01-01 by the civil-from-days algorithm.
fn today() -> Result<String, Failure> {
    let secs = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    let z = i64::try_from(secs / 86_400)? + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    Ok(format!("{year:04}-{month:02}-{day:02}"))
}

/// Baseline rows: (name, model) to (fastest ns, date).
type Baseline = BTreeMap<(String, String), (u128, String)>;

fn read_baseline() -> Result<Baseline, Failure> {
    let mut rows = Baseline::new();
    let Ok(text) = std::fs::read_to_string(baseline_path()) else {
        return Ok(rows);
    };
    for line in text.lines().skip(1) {
        let cols: Vec<&str> = line.split('\t').collect();
        let [name, ns, date, model] = cols.as_slice() else {
            return Err(Failure(format!("bad baseline line {line:?}")));
        };
        rows.insert(((*name).to_owned(), (*model).to_owned()), (ns.parse()?, (*date).to_owned()));
    }
    Ok(rows)
}

/// Replaces this model's rows with the faster of the two runs, dated today.
fn record_baseline(runs: &[BTreeMap<String, u128>; 2], model: &str) -> Result<(), Failure> {
    let mut rows = read_baseline()?;
    rows.retain(|(_, m), _| m != model);
    let date = today()?;
    for (name, a) in &runs[0] {
        let b = runs[1].get(name).ok_or(Failure(format!("{name} missing from run 2")))?;
        rows.insert((name.clone(), model.to_owned()), (*a.min(b), date.clone()));
    }
    let mut text = String::from("name\tfastest_ns\tdate\tmodel\n");
    for ((name, model), (ns, date)) in &rows {
        text.push_str(&format!("{name}\t{ns}\t{date}\t{model}\n"));
    }
    std::fs::write(baseline_path(), text)?;
    Ok(())
}

/// The change from `base` to `ns`, in percent.
fn change(ns: u128, base: u128) -> f64 {
    (ns as f64 / base as f64 - 1.0) * 100.0
}

/// Writes the report and fails on every item more than `TOLERANCE` slower in both runs.
fn compare(runs: &[BTreeMap<String, u128>; 2], model: &str) -> Result<(), Failure> {
    let baseline = read_baseline()?;
    let mut text = format!(
        "name\trun1_ns\trun2_ns\tpair_spread_pct\tbaseline_ns\tbaseline_date\tchange1_pct\tchange2_pct\tverdict\t{model}\t{}\n",
        today()?
    );
    let mut slower = Vec::new();
    for (name, a) in &runs[0] {
        let b = *runs[1].get(name).ok_or(Failure(format!("{name} missing from run 2")))?;
        let spread = change((*a).max(b), (*a).min(b));
        let row = match baseline.get(&(name.clone(), model.to_owned())) {
            Some((base, date)) => {
                let (ca, cb) = (change(*a, *base), change(b, *base));
                let limit = TOLERANCE * 100.0;
                let verdict = if ca > limit && cb > limit { "slower" } else { "ok" };
                if verdict == "slower" {
                    slower.push(format!("{name}: {ca:+.1} % and {cb:+.1} % against {date}"));
                }
                format!("{base}\t{date}\t{ca:.1}\t{cb:.1}\t{verdict}")
            }
            None => "\t\t\t\tno baseline".to_owned(),
        };
        text.push_str(&format!("{name}\t{a}\t{b}\t{spread:.1}\t{row}\n"));
    }
    std::fs::write(out_dir().join("report.tsv"), text)?;
    if slower.is_empty() {
        return Ok(());
    }
    Err(Failure(format!(
        "more than {} % slower than the baseline in both runs (out/bench/report.tsv):\n{}",
        TOLERANCE * 100.0,
        slower.join("\n")
    )))
}

/// The sphere's leaf and tile origins.
fn sphere_regions() -> (Vec<Coord>, Vec<Coord>) {
    let lim = RADIUS + 16;
    let (mut leaves, mut tiles) = (Vec::new(), Vec::new());
    for z in (-lim..lim).step_by(8) {
        for y in (-lim..lim).step_by(8) {
            for x in (-lim..lim).step_by(8) {
                let ds = (0..512).map(|i| distance([x + i % 8, y + i / 8 % 8, z + i / 64]));
                let (lo, hi) = ds.fold((f32::MAX, f32::MIN), |(lo, hi), d| (lo.min(d), hi.max(d)));
                if hi <= -BAND {
                    tiles.push([x, y, z]);
                } else if lo < BAND {
                    leaves.push([x, y, z]);
                }
            }
        }
    }
    (leaves, tiles)
}

fn distance(v: Coord) -> f32 {
    let [x, y, z] = v.map(f64::from);
    #[expect(
        clippy::cast_possible_truncation,
        reason = "a distance within the box, far below f32::MAX"
    )]
    let d = ((x * x + y * y + z * z).sqrt() - f64::from(RADIUS)) as f32;
    d
}

fn sphere_topology(leaves: &[Coord], tiles: &[Coord]) -> Result<Topology, Failure> {
    let empty = Topology::new();
    let mut e = empty.edit();
    for v in leaves {
        e.activate(*v)?;
    }
    for v in tiles {
        e.add_tile(TileLevel::Lower, *v)?;
    }
    Ok(e.commit()?.0)
}

fn sphere_field(t: &Topology) -> Result<Field<f32>, Failure> {
    let mut f = Field::new(t, BAND);
    par_update(t, &mut f, |n, values| -> Result<(), Failure> {
        let o = t.leaf_origin(n)?;
        for (i, v) in (0..).zip(values.iter_mut()) {
            *v = distance([o[0] + i % 8, o[1] + i / 8 % 8, o[2] + i / 64]).clamp(-BAND, BAND);
        }
        Ok(())
    })?;
    for n in t.tile_numbers().collect::<Vec<_>>() {
        f.set_tile(t, n, -BAND)?;
    }
    Ok(f)
}

/// Voxels spread over the sphere's box by a fixed linear congruential sequence.
fn random_voxels() -> Vec<Coord> {
    let side = 2 * (RADIUS + 16);
    let mut g = 0x2545_f491_4f6c_dd1d_u64;
    let mut next = || {
        g = g.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        i32::try_from((g >> 33) % u64::try_from(side).unwrap_or(1)).unwrap_or(0) - side / 2
    };
    (0..LOOKUPS).map(|_| [next(), next(), next()]).collect()
}

/// One run of an item, which returns something to keep from the optimiser.
type Run<'a> = Box<dyn Fn() -> Result<f64, Failure> + 'a>;
/// One item: its name and its run.
type Item<'a> = (String, Run<'a>);

/// An item whose run does `run` `times` times over, so that a run lasts 3 ms or more: a
/// shorter one disagreed by up to 14 % between the two children. The count is part of the
/// name, so that a baseline taken at another count is not compared.
fn repeated<'a>(name: &str, times: u32, run: Run<'a>) -> Item<'a> {
    let name = if times == 1 { name.to_owned() } else { format!("{name}_x{times}") };
    let repeated: Run<'a> = Box::new(move || (0..times).map(|_| run()).sum());
    (name, repeated)
}

/// The data every item reads, built once per child.
struct Data {
    leaves: Vec<Coord>,
    tiles: Vec<Coord>,
    t: Topology,
    f: Field<f32>,
    voxels: Vec<Coord>,
    entry: CacheEntry,
    file: PathBuf,
}

impl Data {
    fn new() -> Result<Self, Failure> {
        let (leaves, tiles) = sphere_regions();
        let t = sphere_topology(&leaves, &tiles)?;
        let f = sphere_field(&t)?;
        let entry = CacheEntry::new(&GridMeta::new("sdf", [1.0; 3], "voxel")?, &t, &f)?;
        let file = out_dir().join("sphere.cache");
        cache::write(&file, std::slice::from_ref(&entry))?;
        Ok(Self { leaves, tiles, t, f, voxels: random_voxels(), entry, file })
    }

    /// The items of the Benchmark tier (TESTING.md §1).
    fn items(&self) -> Vec<Item<'_>> {
        let (t, f) = (&self.t, &self.f);
        let items: Vec<Item<'_>> = vec![
            repeated(
                "activation",
                1,
                Box::new(|| Ok(sphere_topology(&self.leaves, &self.tiles)?.leaf_count() as f64)),
            ),
            repeated(
                "random_access_accessor",
                1,
                Box::new(|| {
                    let mut acc = ValueAccessor::new(t, f)?;
                    Ok(self.voxels.iter().map(|v| f64::from(acc.get(*v))).sum())
                }),
            ),
            repeated(
                "random_access_value_at",
                1,
                Box::new(|| self.voxels.iter().map(|v| Ok(f64::from(f.value_at(t, *v)?))).sum()),
            ),
            repeated(
                "leaf_parallel_update",
                32,
                Box::new(|| {
                    let mut out = Field::new(t, 0.0_f32);
                    par_update(t, &mut out, |n, values| -> Result<(), Failure> {
                        for (o, v) in values.iter_mut().zip(f.leaf(t, n)?) {
                            *o = 2.0 * v + 1.0;
                        }
                        Ok(())
                    })?;
                    Ok(f64::from(out.background()))
                }),
            ),
            repeated(
                "gather",
                1,
                Box::new(|| {
                    let mut g = [0.0_f32; GATHER_VOXELS];
                    let mut sum = 0.0;
                    for n in t.leaf_numbers() {
                        gather(t, f, n, &mut g)?;
                        sum += f64::from(g[GATHER_VOXELS / 2]);
                    }
                    Ok(sum)
                }),
            ),
        ];
        items.into_iter().chain(self.file_items()).collect()
    }

    /// `freeze` and the cache profile's file I/O.
    fn file_items(&self) -> Vec<Item<'_>> {
        vec![
            repeated(
                "freeze",
                8,
                Box::new(|| Ok(freeze(&self.t, &self.f)?.as_bytes().len() as f64)),
            ),
            repeated(
                "cache_write",
                8,
                Box::new(|| {
                    cache::write(&self.file, std::slice::from_ref(&self.entry))?;
                    Ok(0.0)
                }),
            ),
            repeated(
                "cache_read",
                16,
                Box::new(|| Ok(CacheFile::read(&self.file)?.grid_count() as f64)),
            ),
            repeated(
                "cache_map",
                256,
                Box::new(|| Ok(CacheFile::map(&self.file)?.grid_count() as f64)),
            ),
        ]
    }
}

/// Writes over the buffer a cache line at a time, so that what the last run left in the
/// caches is gone.
fn evict(buffer: &mut [u8]) {
    for (i, b) in (0_u8..=255).cycle().zip(buffer.iter_mut().step_by(64)) {
        *b = b.wrapping_add(i);
    }
    black_box(&*buffer);
}

/// Runs every item once per round, in turn, after an eviction, and writes each item's
/// fastest time to `path`.
fn child(path: &Path) -> Result<(), Failure> {
    let data = Data::new()?;
    let items = data.items();
    let mut buffer = vec![0_u8; EVICT_BYTES];
    let mut fastest = vec![u128::MAX; items.len()];
    for round in 0..ROUNDS {
        for k in 0..items.len() {
            let j = (round + k) % items.len();
            evict(&mut buffer);
            let start = Instant::now();
            black_box(items[j].1()?);
            fastest[j] = fastest[j].min(start.elapsed().as_nanos());
        }
    }
    let text: String =
        items.iter().zip(&fastest).map(|((name, _), ns)| format!("{name}\t{ns}\n")).collect();
    std::fs::write(path, text)?;
    Ok(())
}
