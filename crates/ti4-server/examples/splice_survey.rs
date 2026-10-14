//! Survey the splice dry run over saved game histories (H7 phase 1 measurement).
//!
//! ```text
//! cargo run --release -p ti4-server --example splice_survey -- [--cursors N] [--stride S] \
//!     <server-data dir> [<server-data dir> ...]
//! ```
//!
//! Each argument is a store root (a `server-data` folder holding `game_*` directories). For every
//! game it replays the history once, then previews removing, and answering differently, each of
//! the first N decisions (every S-th), and prints one JSON object per game plus a total. Nothing
//! is written anywhere; the store is only read.

use std::path::PathBuf;
use std::time::Instant;

use ti4_content::ContentStore;
use ti4_server::protocol::splice::{RngStatus, SpliceEdit};
use ti4_server::session::splice::SpliceBaseline;
use ti4_server::storage::FileGameStore;

#[derive(Default)]
struct Tally {
    previews: usize,
    survives_to_end: usize,
    some_survive: usize,
    kept_fraction_sum: f64,
    neutral: usize,
    not_neutral: usize,
    unknown: usize,
    neutral_and_survives: usize,
    kinds: std::collections::BTreeMap<String, usize>,
    asks_removed: usize,
    alignment_notes: usize,
    micros: u128,
}

impl Tally {
    fn add(&mut self, p: &ti4_server::protocol::splice::SplicePreview, micros: u128) {
        self.previews += 1;
        self.micros += micros;
        if p.survives_to_end {
            self.survives_to_end += 1;
        }
        if p.kept_later > 0 {
            self.some_survive += 1;
        }
        if p.later_decisions > 0 {
            self.kept_fraction_sum += p.kept_later as f64 / p.later_decisions as f64;
        } else {
            self.kept_fraction_sum += 1.0;
        }
        match p.rng.status {
            RngStatus::Neutral => self.neutral += 1,
            RngStatus::NotNeutral => self.not_neutral += 1,
            RngStatus::Unknown => self.unknown += 1,
        }
        if p.survives_to_end && p.rng.status != RngStatus::NotNeutral {
            self.neutral_and_survives += 1;
        }
        if let Some(c) = &p.first_conflict {
            *self.kinds.entry(format!("{:?}", c.kind)).or_default() += 1;
            if c.asks_removed_decision {
                self.asks_removed += 1;
            }
        }
        self.alignment_notes += p.alignment.len();
    }

    fn json(&self) -> serde_json::Value {
        serde_json::json!({
            "previews": self.previews,
            "survives_to_end": self.survives_to_end,
            "some_later_survive": self.some_survive,
            "mean_kept_fraction": if self.previews == 0 { 0.0 } else { self.kept_fraction_sum / self.previews as f64 },
            "rng_neutral": self.neutral,
            "rng_not_neutral": self.not_neutral,
            "rng_unknown": self.unknown,
            "survives_and_not_flagged": self.neutral_and_survives,
            "first_conflict_kinds": self.kinds,
            "conflicts_asking_the_removed_question_again": self.asks_removed,
            "alignment_notes": self.alignment_notes,
            "mean_preview_ms": if self.previews == 0 { 0.0 } else { self.micros as f64 / self.previews as f64 / 1000.0 },
        })
    }
}

fn main() {
    let mut cursors = 40usize;
    let mut stride = 1usize;
    let mut roots = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--cursors" => {
                cursors = args
                    .next()
                    .and_then(|v| v.parse().ok())
                    .expect("--cursors N")
            }
            "--stride" => {
                stride = args
                    .next()
                    .and_then(|v| v.parse().ok())
                    .expect("--stride S")
            }
            _ => roots.push(PathBuf::from(arg)),
        }
    }
    let content = ContentStore::embedded();
    let (mut remove_total, mut replace_total) = (Tally::default(), Tally::default());
    for root in roots {
        let store = FileGameStore::new(&root).expect("open store");
        let mut ids: Vec<String> = std::fs::read_dir(&root)
            .expect("read store")
            .filter_map(Result::ok)
            .filter_map(|e| e.file_name().into_string().ok())
            .filter(|n| n.starts_with("game_"))
            .collect();
        ids.sort();
        for id in ids {
            let (initial, player_ids, seed, template) =
                if let Ok(init) = store.load_player_init(&id) {
                    (
                        init.initial_state,
                        init.player_ids,
                        init.seed,
                        init.map_template,
                    )
                } else if let Ok(init) = store.load_init(&id) {
                    let Some(seed) = init.seed else { continue };
                    (init.initial_state, init.player_ids, seed, init.map_template)
                } else {
                    eprintln!("{id}: no readable init, skipped");
                    continue;
                };
            let decisions = match store.load_history(&id) {
                Ok(Some(h)) => h.decisions,
                _ => store.load_decisions(&id).unwrap_or_default(),
            };
            let Ok((_, galaxy)) = ti4_server::map::create_game_with_template(
                content,
                &player_ids,
                seed,
                template.as_deref(),
            ) else {
                eprintln!("{id}: map rebuild failed, skipped");
                continue;
            };
            let started = Instant::now();
            let base = SpliceBaseline::new(&initial, Some(&galaxy), &decisions);
            let baseline_ms = started.elapsed().as_secs_f64() * 1000.0;
            if let Some(c) = base.replay_conflict() {
                eprintln!(
                    "{id}: the unedited history stops replaying at decision {}: {:?} ({}) seat {} prompt {:?} expected {:?} found {:?}/{:?}",
                    c.cursor,
                    c.kind,
                    c.detail,
                    c.seat,
                    c.prompt,
                    c.expected_chosen,
                    c.found_seat,
                    c.found_prompt
                );
            }
            let (mut remove, mut replace) = (Tally::default(), Tally::default());
            for k in (0..decisions.len().min(cursors)).step_by(stride) {
                let t = Instant::now();
                let p = base
                    .preview(&SpliceEdit::Remove { cursor: k })
                    .expect("remove");
                remove.add(&p, t.elapsed().as_micros());
                for alt in decisions[k]
                    .offered
                    .iter()
                    .filter(|o| **o != decisions[k].chosen)
                {
                    let t = Instant::now();
                    let p = base
                        .preview(&SpliceEdit::Replace {
                            cursor: k,
                            option_id: alt.clone(),
                        })
                        .expect("replace");
                    replace.add(&p, t.elapsed().as_micros());
                }
            }
            println!(
                "{}",
                serde_json::json!({
                    "game": id,
                    "decisions": decisions.len(),
                    "baseline_replayed": base.replayed(),
                    "baseline_ms": baseline_ms,
                    "remove": remove.json(),
                    "replace": replace.json(),
                })
            );
            merge(&mut remove_total, &remove);
            merge(&mut replace_total, &replace);
        }
    }
    println!(
        "{}",
        serde_json::json!({ "TOTAL": { "remove": remove_total.json(), "replace": replace_total.json() } })
    );
}

fn merge(into: &mut Tally, from: &Tally) {
    into.previews += from.previews;
    into.survives_to_end += from.survives_to_end;
    into.some_survive += from.some_survive;
    into.kept_fraction_sum += from.kept_fraction_sum;
    into.neutral += from.neutral;
    into.not_neutral += from.not_neutral;
    into.unknown += from.unknown;
    into.neutral_and_survives += from.neutral_and_survives;
    into.asks_removed += from.asks_removed;
    into.alignment_notes += from.alignment_notes;
    into.micros += from.micros;
    for (kind, n) in &from.kinds {
        *into.kinds.entry(kind.clone()).or_default() += n;
    }
}
