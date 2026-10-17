//! A matrix in, a summary out.

use std::collections::BTreeMap;
use std::time::Instant;

use ti4_rule_bot::tuning::Tuning;

use crate::play::{GameResult, PlayOptions, TraceLine};
use crate::run::{Job, OnDone, run_jobs_cached};
use crate::spec::{GameSpec, Matrix};
use crate::stats::{GameLine, Meta, Summary, aggregate, problems};

/// The bot's numbers from `key=value` overrides.
///
/// # Errors
/// For an unknown key or a value that does not parse.
pub fn tuning_from(overrides: &BTreeMap<String, String>) -> Result<Tuning, String> {
    let mut t = Tuning::default();
    // `bot_version` first: it replaces every number, later keys override it (map order is
    // alphabetical, which would let it clobber keys sorted before it).
    if let Some(v) = overrides.get("bot_version") {
        t.set_key("bot_version", v)?;
    }
    for (k, v) in overrides {
        if k != "bot_version" {
            t.set_key(k, v)?;
        }
    }
    Ok(t)
}

/// All games of a matrix, in order, and the maps that were skipped.
///
/// # Errors
/// For a setup that cannot be made.
pub fn expand(matrix: &Matrix) -> Result<(Vec<GameSpec>, Vec<String>), String> {
    let (mut games, mut skipped) = (Vec::new(), Vec::new());
    let mut names = std::collections::BTreeSet::new();
    for setup in &matrix.setups {
        if !names.insert(setup.label()) {
            return Err(format!(
                "two setups are named '{}': give them names",
                setup.label()
            ));
        }
        let (g, s) = setup
            .expand()
            .map_err(|e| format!("setup '{}': {e}", setup.label()))?;
        games.extend(g);
        skipped.extend(s);
    }
    Ok((games, skipped))
}

fn git_rev() -> String {
    std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map_or_else(
            || "unknown".to_owned(),
            |o| String::from_utf8_lossy(&o.stdout).trim().to_owned(),
        )
}

/// Play the matrix and summarise it.
///
/// # Errors
/// For a setup that cannot be made or a bad override.
pub fn run_matrix(
    matrix: &Matrix,
    workers: usize,
    progress: bool,
    trace: bool,
    on_done: Option<OnDone<'_>>,
) -> Result<(Summary, Vec<GameResult>), String> {
    let (specs, skipped) = expand(matrix)?;
    let tuning = tuning_from(&matrix.bot)?;
    let jobs: Vec<Job> = specs
        .into_iter()
        .map(|spec| Job {
            spec,
            options: PlayOptions {
                limits: matrix.limits.clone(),
                tunings: vec![tuning],
                candidate_seats: None,
                trace,
                timeline: true,
            },
        })
        .collect();
    let started = Instant::now();
    let results = run_jobs_cached(&jobs, workers, progress, on_done);
    let wall = started.elapsed().as_secs_f64();
    Ok((summarise(matrix, &results, skipped, workers, wall), results))
}

#[must_use]
pub fn summarise(
    matrix: &Matrix,
    results: &[GameResult],
    skipped: Vec<String>,
    workers: usize,
    wall_s: f64,
) -> Summary {
    let all: Vec<&GameResult> = results.iter().collect();
    let mut setups = BTreeMap::new();
    for setup in &matrix.setups {
        let name = setup.label();
        let mine: Vec<&GameResult> = results.iter().filter(|g| g.spec.setup == name).collect();
        setups.insert(name, aggregate(&mine, true));
    }
    Summary {
        meta: Meta {
            schema: 1,
            tool: "ti4-selfplay".to_owned(),
            git_rev: git_rev(),
            cores: std::thread::available_parallelism().map_or(1, usize::from),
            workers,
            profile: if cfg!(debug_assertions) {
                "debug"
            } else {
                "release"
            }
            .to_owned(),
            wall_s,
            games_per_hour: if wall_s > 0.0 {
                results.len() as f64 / wall_s * 3600.0
            } else {
                0.0
            },
            started: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or_else(|_| String::new(), |d| format!("unix {}", d.as_secs())),
        },
        limits: matrix.limits.clone(),
        bot: matrix.bot.clone(),
        skipped,
        combined: aggregate(&all, true),
        setups,
        problems: problems(results),
        games: results.iter().map(GameLine::from).collect(),
    }
}

/// A readable form of a trace: one line per decision.
#[must_use]
pub fn trace_text(trace: &[TraceLine]) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    for t in trace {
        let _ = writeln!(
            out,
            "{:>6} r{:<2} {:<8} {} {:<30} {:<7} {} (of {})  | {}",
            t.i, t.round, t.phase, t.seat, t.subtype, t.by, t.chosen, t.offered, t.prompt
        );
    }
    out
}
