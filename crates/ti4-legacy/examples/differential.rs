//! How far a Python-oracle game replays through the Rust engine before they diverge.
//!
//! The differential test this project exists to pass, run across the whole retained corpus rather
//! than the single trace the unit test covers. Each trace is a bounded game played by the Python
//! oracle at a pinned commit; its initial public state is imported, its decisions are replayed
//! through the Rust engine, and the run stops at the first decision Rust cannot honour.
//!
//! The number that matters is not how many traces pass — none do yet — but *what stops them*.
//! Every stop names one unimplemented thing, and the tally says which of them is worth building
//! next if the goal is parity rather than coverage for its own sake.
//!
//! `cargo run -p ti4-legacy --example differential --release`

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use ti4_content::ContentStore;
use ti4_engine::choice::{Scripted, Table};
use ti4_engine::game::Game;
use ti4_legacy::source_trace::parse_source_trace_states;
use ti4_legacy::state_import::{import_initial_public_state, import_map};
use ti4_model::content_types::{Source, SourceSet};

fn main() {
    let dir =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/legacy_entropy/bounded-v1");
    let mut traces: Vec<_> = fs::read_dir(&dir)
        .expect("corpus directory")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "ndjson"))
        .collect();
    traces.sort();

    let mut blocked_at: BTreeMap<String, usize> = BTreeMap::new();
    let mut reached: Vec<usize> = Vec::new();
    let mut offered: Vec<usize> = Vec::new();
    let (mut unparsed, mut unimportable, mut completed) = (0usize, 0usize, 0usize);
    let mut mapless = 0usize;

    for path in &traces {
        let text = fs::read_to_string(path).expect("trace readable");
        let Ok(trace) = parse_source_trace_states(&text) else {
            unparsed += 1;
            continue;
        };
        let decisions = trace.trace.decisions.clone();
        offered.push(decisions.len());

        let Ok(state) = import_initial_public_state(&trace.initial) else {
            unimportable += 1;
            continue;
        };

        // The board the oracle played on. Without it no tactical action is ever offered, and a
        // tactical action is most of what a game of this is.
        let records: Vec<serde_json::Value> = text
            .lines()
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect();
        let mut game = Game::with_table(
            state,
            ContentStore::embedded(),
            Table::with_default(Box::new(Scripted::new(decisions.clone()))),
        );
        match import_map(&records, ContentStore::embedded(), every_source()) {
            Ok(galaxy) => game = game.with_galaxy(galaxy),
            Err(error) => {
                mapless += 1;
                if mapless <= 3 {
                    println!("  map failed for {}: {error}", path.display());
                }
            }
        }
        let mut stopped = None;
        for _ in 0..decisions.len().max(1) * 4 {
            if let Some(error) = game.step().error {
                stopped = Some(error.to_string());
                break;
            }
            if game.state.finished {
                break;
            }
        }
        reached.push(game.table.log.len());
        match stopped {
            Some(error) => {
                // The option the script wanted and the engine did not offer. That name is the
                // unimplemented thing, and it is what the tally is for.
                let wanted = error
                    .split("wanted")
                    .nth(1)
                    .and_then(|rest| rest.split('"').nth(1))
                    .map_or_else(|| error.clone(), ToOwned::to_owned);
                *blocked_at.entry(wanted).or_default() += 1;
            }
            None => completed += 1,
        }
    }

    let total = traces.len();
    let sum: usize = reached.iter().sum();
    let asked: usize = offered.iter().sum();
    println!("corpus: {total} bounded Python games at oracle 37061c51");
    println!("  unparsable:                  {unparsed}");
    println!("  parsed but not importable:   {unimportable}");
    println!("  replayed without a board:    {mapless}");
    println!("  replayed to the end:         {completed}");
    println!();
    println!(
        "decisions replayed: {sum} of {asked} the oracle recorded ({:.1}%)",
        100.0 * f64::from(u32::try_from(sum).unwrap_or(u32::MAX))
            / f64::from(u32::try_from(asked.max(1)).unwrap_or(u32::MAX))
    );
    println!(
        "  per trace: median {}, best {}",
        median(&mut reached.clone()),
        reached.iter().copied().max().unwrap_or(0)
    );
    println!();
    println!("what stops them, commonest first:");
    let mut rows: Vec<(&String, usize)> = blocked_at.iter().map(|(k, v)| (k, *v)).collect();
    rows.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
    for (what, count) in rows.iter().take(12) {
        println!("  {count:>3} traces  {what}");
    }
}

/// Every source, for reconstructing a board rather than scoping a game.
fn every_source() -> SourceSet {
    Source::Base
        | Source::Codex1
        | Source::Codex2
        | Source::Codex3
        | Source::Codex4
        | Source::Pok
        | Source::ThundersEdge
}

fn median(values: &mut [usize]) -> usize {
    if values.is_empty() {
        return 0;
    }
    values.sort_unstable();
    values[values.len() / 2]
}
