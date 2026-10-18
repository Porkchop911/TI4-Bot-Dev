//! Stored games: every game that `run` and `compare` play is kept, with its per-round timeline,
//! in a results file; the same run is never played twice.
//!
//! A run is a list of jobs (game spec, the bot numbers of every seat, limits, seat mask). Its key
//! is a hash of that list, of [`HARNESS_VERSION`] and of the rule bot's [`BOT_VERSION`]
//! (`ti4_rule_bot::tuning`), so a changed knob, seed list, map, bot default or harness gives a
//! new key. The file `<dir>/<key>.jsonl` holds a header line (the key's ingredients) and one
//! [`GameResult`] per line in job order; results are the same on any number of workers, so the
//! file is too. `compare` and `run` read the file instead of playing when it exists;
//! `selfplay metrics FILE` prints the A/B metrics from a stored file without any rerun.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use crate::play::GameResult;
use crate::run::Job;

/// Bump when the harness changes what a stored game means (the engine's or bot's own changes are
/// covered by the bot version and, for a changed default, by the knobs in the key; a changed
/// engine needs a bump or `--no-cache`).
pub const HARNESS_VERSION: u32 = 1;

static DIR: OnceLock<Option<PathBuf>> = OnceLock::new();

/// Set the results directory for this process (`None`: no cache). The first call wins.
pub fn configure(dir: Option<PathBuf>) {
    let _ = DIR.set(dir);
}

/// The results directory in force.
#[must_use]
pub fn dir() -> Option<&'static Path> {
    DIR.get_or_init(default_dir).as_deref()
}

fn default_dir() -> Option<PathBuf> {
    match std::env::var("SELFPLAY_RESULTS") {
        Ok(v) if v.is_empty() || v == "off" => None,
        Ok(v) => Some(PathBuf::from(v)),
        Err(_) => Some(PathBuf::from("selfplay-results")),
    }
}

/// The key of a run, with the text it hashes.
#[must_use]
pub fn key_of(jobs: &[Job]) -> (String, String) {
    let mut text = format!(
        "harness {HARNESS_VERSION} bot {}\n",
        ti4_rule_bot::tuning::BOT_VERSION
    );
    // The shipped numbers: a changed default is a changed bot.
    text.push_str(&format!(
        "{:?}\n",
        ti4_rule_bot::tuning::current().entries()
    ));
    if let Ok(salt) = std::env::var("SELFPLAY_SALT") {
        text.push_str(&format!("salt {salt}\n"));
    }
    for job in jobs {
        text.push_str(&serde_json::to_string(&job.spec).unwrap_or_default());
        text.push_str(&format!(
            " {:?} {:?}",
            job.options.limits, job.options.candidate_seats
        ));
        for t in &job.options.tunings {
            text.push_str(&format!(" {:?}", t.entries()));
        }
        text.push('\n');
    }
    // FNV-1a, 64 bit, twice with different offsets for a 128-bit name.
    let hash = |seed: u64| {
        text.bytes().fold(seed, |h, b| {
            (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3)
        })
    };
    (
        format!(
            "{:016x}{:016x}",
            hash(0xcbf2_9ce4_8422_2325),
            hash(0x84222325_cbf29ce4)
        ),
        text,
    )
}

/// The stored results of the run, if its file exists and holds one result per job.
#[must_use]
pub fn load(jobs: &[Job]) -> Option<Vec<GameResult>> {
    let dir = dir()?;
    if jobs.iter().any(|j| j.options.trace) {
        return None;
    }
    let (key, _) = key_of(jobs);
    let text = std::fs::read_to_string(dir.join(format!("{key}.jsonl"))).ok()?;
    let mut lines = text.lines();
    lines.next()?;
    let results: Vec<GameResult> = lines
        .map(serde_json::from_str)
        .collect::<Result<_, _>>()
        .ok()?;
    (results.len() == jobs.len()).then_some(results)
}

/// Store the results of the run (best effort: a failure is reported, not fatal).
pub fn save(jobs: &[Job], results: &[GameResult], wall_s: f64) {
    let Some(dir) = dir() else { return };
    if jobs.iter().any(|j| j.options.trace) {
        return;
    }
    let (key, text) = key_of(jobs);
    let write = || -> std::io::Result<()> {
        std::fs::create_dir_all(dir)?;
        let tmp = dir.join(format!("{key}.tmp"));
        let mut out = std::io::BufWriter::new(std::fs::File::create(&tmp)?);
        let header = serde_json::json!({
            "key": key, "harness": HARNESS_VERSION,
            "bot_version": ti4_rule_bot::tuning::BOT_VERSION,
            "games": results.len(), "wall_s": wall_s,
            "ingredients": text,
        });
        writeln!(out, "{header}")?;
        for r in results {
            writeln!(out, "{}", serde_json::to_string(r).unwrap_or_default())?;
        }
        out.flush()?;
        std::fs::rename(tmp, dir.join(format!("{key}.jsonl")))
    };
    match write() {
        Ok(()) => eprintln!(
            "stored {} games as {}/{key}.jsonl",
            results.len(),
            dir.display()
        ),
        Err(e) => eprintln!("could not store the games: {e}"),
    }
}

/// Read a stored results file (header skipped).
///
/// # Errors
/// A message when the file cannot be read or holds a line that is not a game.
pub fn read_file(path: &Path) -> Result<Vec<GameResult>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    text.lines()
        .skip(1)
        .map(|l| serde_json::from_str(l).map_err(|e| format!("{}: {e}", path.display())))
        .collect()
}
