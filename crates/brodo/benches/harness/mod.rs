//! The benchmark harness ADR-0019 chose, shared by every member's benchmarks (TESTING.md
//! §1): std only, built to T5. A member's benchmark includes this file and calls `run`
//! with the data its items read and the items.
//!
//! `cargo bench -p <crate> --bench <crate>` runs two child processes, each a fresh process.
//! Each child runs every item once per round, in turn, starting from a different item each
//! round, for `ROUNDS` rounds, writing over an `EVICT_BYTES` buffer before each run, and
//! keeps each item's fastest time. The parent writes both children's times, how far they
//! agree, and the change from the baseline of this hardware model to
//! `out/bench/<crate>/report.tsv`, and fails when an item is more than `TOLERANCE` slower
//! than its baseline in both. With `--record` it instead replaces this model's rows of the
//! member's `benches/baseline.tsv` with the faster of the two times, dated; with
//! `--record NAME`, only that item's row, the others kept with their dates, so that an item
//! added later gains a baseline without the rest losing theirs.
//!
//! Run and record on a machine doing nothing else; a run waits, then refuses, while the
//! one-minute or the five-minute load average says otherwise. With other work running,
//! brodo's leaf-parallel update once failed its baseline at +21 to +27 % with no change to
//! the code; at rest it stayed within 12 % (ADR-0019). Dates, in the baseline and the
//! report, are UTC.
//!
//! Nothing here prints (CODING.md Rule 18): results go to files, and a failure is the error
//! `main` returns.

use std::collections::BTreeMap;
use std::fmt;
use std::hint::black_box;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Rounds per child: the two children of a pair agreed within 1.7 % from 20 rounds on
/// (ADR-0019).
const ROUNDS: usize = 20;
/// Bytes written over before each run: 256 MiB evicted the caches on an Apple M5 where
/// 32 MiB did not (ADR-0019).
const EVICT_BYTES: usize = 256 << 20;
/// How much slower than its baseline an item may be, in both children, before it fails
/// (ADR-0019).
const TOLERANCE: f64 = 0.10;
/// The one-minute load average above which the machine is taken to be busy: at rest an
/// Apple M5 read 1.53 to 2.13, and 3.86 just after a build and the tests (ADR-0019).
const BUSY_LOAD: f64 = 3.0;
/// The five-minute load average above which the machine is taken to be still settling:
/// after a build, the system's own scans of the new binaries held it near 3.5 to 10 while
/// the one-minute fell below `BUSY_LOAD`, and a pair then disagreed by up to 17 %; at rest
/// it read 1.6 to 3.8 (ADR-0019).
const SETTLED_LOAD: f64 = 4.0;
/// While busy, the load averages are read again after each wait, at most this many times,
/// so that a build just before the run can settle: ten minutes in all.
const LOAD_WAIT: Duration = Duration::from_secs(10);
const LOAD_RETRIES: u32 = 60;
/// A failure, shown by `main`'s return as its message alone.
pub struct Failure(String);

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

/// Where a member's benchmark writes and keeps its files.
struct Places {
    out: PathBuf,
    baseline: PathBuf,
}

impl Places {
    /// The member's, from its `CARGO_MANIFEST_DIR` and name.
    fn of(manifest_dir: &str, name: &str) -> Places {
        let dir = Path::new(manifest_dir);
        Places {
            out: dir.join("../../out/bench").join(name),
            baseline: dir.join("benches/baseline.tsv"),
        }
    }
}

/// Runs a member's benchmark: as the parent, two children and the comparison or the
/// recording; as a child, `items` over `data`'s once per round.
pub fn run<D>(
    manifest_dir: &str,
    name: &str,
    data: impl Fn() -> Result<D, Failure>,
    items: impl for<'a> Fn(&'a D) -> Vec<Item<'a>>,
) -> Result<(), Failure> {
    let places = Places::of(manifest_dir, name);
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let [mode, path, ..] = args.as_slice()
        && mode == "child"
    {
        let data = data()?;
        return child(&items(&data), Path::new(path));
    }
    let at = args.iter().position(|a| a == "--record");
    // The argument after `--record`, unless it is another flag, such as cargo's `--bench`.
    let only = at.and_then(|i| args.get(i + 1)).filter(|a| !a.starts_with("--")).cloned();
    std::fs::create_dir_all(&places.out)?;
    let load = wait_for_rest()?;
    let runs = [run_child(&places, 0)?, run_child(&places, 1)?];
    let model = hardware_model()?;
    if at.is_some() {
        record_baseline(&places, &runs, &model, only.as_deref())
    } else {
        compare(&places, &runs, &model, load)
    }
}

/// The one-minute and five-minute load averages: `vm.loadavg` on macOS, `/proc/loadavg`
/// on Linux.
fn load_averages() -> Result<(f64, f64), Failure> {
    let text = match Command::new("sysctl").args(["-n", "vm.loadavg"]).output() {
        Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout).into_owned(),
        _ => std::fs::read_to_string("/proc/loadavg")?,
    };
    let mut words = text.split_whitespace().filter(|w| *w != "{");
    let missing = || Failure(format!("no load average in {text:?}"));
    let one = words.next().ok_or_else(missing)?.parse()?;
    let five = words.next().ok_or_else(missing)?.parse()?;
    Ok((one, five))
}

/// Waits until the one-minute load average is at most `BUSY_LOAD` and the five-minute at
/// most `SETTLED_LOAD`, reading them again after each `LOAD_WAIT` up to `LOAD_RETRIES`
/// times; refuses to run if they stay above. Returns the loads the run starts at.
fn wait_for_rest() -> Result<(f64, f64), Failure> {
    let rested = |(one, five): (f64, f64)| one <= BUSY_LOAD && five <= SETTLED_LOAD;
    let mut load = load_averages()?;
    for _ in 0..LOAD_RETRIES {
        if rested(load) {
            return Ok(load);
        }
        std::thread::sleep(LOAD_WAIT);
        load = load_averages()?;
    }
    if rested(load) {
        return Ok(load);
    }
    Err(Failure(format!(
        "the load averages stayed at {} and {} (one and five minutes) above {BUSY_LOAD} or {SETTLED_LOAD} for {} s; stop other work and run again",
        load.0,
        load.1,
        LOAD_WAIT.as_secs() * u64::from(LOAD_RETRIES)
    )))
}

/// Runs one child in a fresh process and reads its times: item name to fastest ns.
fn run_child(places: &Places, k: usize) -> Result<BTreeMap<String, u128>, Failure> {
    let path = places.out.join(format!("child-{k}.tsv"));
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

fn read_baseline(places: &Places) -> Result<Baseline, Failure> {
    let mut rows = Baseline::new();
    let Ok(text) = std::fs::read_to_string(&places.baseline) else {
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

/// Replaces this model's rows with the faster of the two runs, dated today; with `only`,
/// that item's row alone, refused if no item has the name.
fn record_baseline(
    places: &Places,
    runs: &[BTreeMap<String, u128>; 2],
    model: &str,
    only: Option<&str>,
) -> Result<(), Failure> {
    let mut rows = read_baseline(places)?;
    if let Some(name) = only
        && !runs[0].contains_key(name)
    {
        return Err(Failure(format!("no item named {name} to record")));
    }
    rows.retain(|(n, m), _| m != model || only.is_some_and(|o| o != n));
    let date = today()?;
    for (name, a) in runs[0].iter().filter(|(n, _)| only.is_none_or(|o| o == n.as_str())) {
        let b = runs[1].get(name).ok_or(Failure(format!("{name} missing from run 2")))?;
        rows.insert((name.clone(), model.to_owned()), (*a.min(b), date.clone()));
    }
    let mut text = String::from("name\tfastest_ns\tdate\tmodel\n");
    for ((name, model), (ns, date)) in &rows {
        text.push_str(&format!("{name}\t{ns}\t{date}\t{model}\n"));
    }
    std::fs::write(&places.baseline, text)?;
    Ok(())
}

/// The change from `base` to `ns`, in percent.
fn change(ns: u128, base: u128) -> f64 {
    (ns as f64 / base as f64 - 1.0) * 100.0
}

/// Writes the report, its header naming the hardware, the date and the one-minute and
/// five-minute load averages the run started at, and fails on every item more than
/// `TOLERANCE` slower in both runs.
fn compare(
    places: &Places,
    runs: &[BTreeMap<String, u128>; 2],
    model: &str,
    load: (f64, f64),
) -> Result<(), Failure> {
    let baseline = read_baseline(places)?;
    let mut text = format!(
        "name\trun1_ns\trun2_ns\tpair_spread_pct\tbaseline_ns\tbaseline_date\tchange1_pct\tchange2_pct\tverdict\t{model}\t{}\tload {} {}\n",
        today()?,
        load.0,
        load.1
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
    std::fs::write(places.out.join("report.tsv"), text)?;
    if slower.is_empty() {
        return Ok(());
    }
    Err(Failure(format!(
        "more than {} % slower than the baseline in both runs ({}):\n{}",
        TOLERANCE * 100.0,
        places.out.join("report.tsv").display(),
        slower.join("\n")
    )))
}

/// One run of an item, which returns something to keep from the optimiser.
pub type Run<'a> = Box<dyn Fn() -> Result<f64, Failure> + 'a>;
/// One item: its name and its run.
pub type Item<'a> = (String, Run<'a>);

/// An item whose run does `run` `times` times over, so that a run lasts 3 ms or more: a
/// shorter one disagreed by up to 14 % between the two children. The count is part of the
/// name, so that a baseline taken at another count is not compared.
pub fn repeated<'a>(name: &str, times: u32, run: Run<'a>) -> Item<'a> {
    let name = if times == 1 { name.to_owned() } else { format!("{name}_x{times}") };
    let repeated: Run<'a> = Box::new(move || (0..times).map(|_| run()).sum());
    (name, repeated)
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
fn child(items: &[Item<'_>], path: &Path) -> Result<(), Failure> {
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
