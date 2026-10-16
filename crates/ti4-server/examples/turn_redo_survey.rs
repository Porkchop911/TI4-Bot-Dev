//! Survey the turn redo over saved game histories (H7 measurement).
//!
//! ```text
//! cargo run -p ti4-server --example turn_redo_survey -- [--stride S] [--variants V] \
//!     <server-data dir> [<server-data dir> ...]
//! ```
//!
//! Each argument is a store root (a `server-data` folder holding `game_*` directories). For every
//! game it replays the history once, then for every S-th complete turn of every seat plays V
//! different new turns (a pseudo-random player), and auto-plays the others' recorded decisions
//! twice: with the random streams forced to the original positions (the feature) and without
//! (a plain replay). It prints one JSON object per game plus a total. Nothing is written; the
//! store is only read.
//!
//! "Round survival" is measured against the decisions that follow the redone turn up to where
//! the auto-play hands control back: the same seat's next turn start (before the stop-rule change
//! it was the seat's next decision of any kind; `survives_old_span` counts a run that got at
//! least that far). Three variants of the same redo are run: the feature (dice forced and cards
//! keep their identity), `no_identity` (dice forced, cards drawn positionally) and `plain`.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Instant;

use ti4_content::ContentStore;
use ti4_content::galaxy::Galaxy;
use ti4_engine::choice::{Choice, ChoiceOption, Decider, DecisionRecord, IllegalChoice, Table};
use ti4_engine::game::Game;
use ti4_model::id::PlayerId;
use ti4_model::state::GameState;
use ti4_server::protocol::turn_redo::TurnRedoStop;
use ti4_server::session::turn_redo::{
    AutoplayResult, RedoWindow, TailBaseline, TailSource, autoplay, find_turns, next_turn_start,
    open_segment,
};
use ti4_server::session::{RngForce, RngMarks};
use ti4_server::storage::FileGameStore;

struct Lcg(u64);

impl Decider for Lcg {
    fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let roll = usize::try_from(self.0 >> 33).unwrap();
        Ok(choice.options[roll % choice.options.len()].clone())
    }
}

struct PrefixThen {
    prefix: Vec<String>,
    at: usize,
    then: Lcg,
    force: Option<RngForce>,
}

impl Decider for PrefixThen {
    fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
        if let Some(wanted) = self.prefix.get(self.at) {
            if let Some(force) = &self.force {
                force.before_answer(self.at);
            }
            self.at += 1;
            return choice
                .option(wanted)
                .cloned()
                .ok_or_else(|| IllegalChoice::ScriptDiverged {
                    player: choice.player.clone(),
                    wanted: wanted.clone(),
                    offered: choice.ids().into_iter().map(str::to_owned).collect(),
                });
        }
        if let Some(force) = &self.force {
            force.go_live();
        }
        self.then.choose(choice)
    }
}

/// Replay `prefix` (with `marks` forced), then a pseudo-random seat plays until `seat`'s turn
/// that starts at `prefix.len()` is complete.
fn new_turn(
    state: &GameState,
    galaxy: &Galaxy,
    prefix: &[DecisionRecord],
    seat: &PlayerId,
    marks: &RngMarks,
    variant: u64,
) -> Option<Vec<DecisionRecord>> {
    let start = prefix.len();
    let force = RngForce::new(marks);
    let table = Table::with_default(Box::new(PrefixThen {
        prefix: prefix.iter().map(|r| r.chosen.clone()).collect(),
        at: 0,
        then: Lcg(variant.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0x1234_5678),
        force: force.clone(),
    }));
    let mut game = Game::with_table(state.clone(), ContentStore::embedded(), table)
        .with_galaxy(galaxy.clone());
    if let Some(force) = &force {
        force.attach(&mut game);
    }
    loop {
        let log = &game.table.log.records;
        if log.len() > start
            && find_turns(log)
                .iter()
                .any(|t| &t.seat == seat && t.start == start && t.complete)
        {
            return Some(game.table.log.records);
        }
        if log.len() > start + 150 {
            return None;
        }
        let result = game.step();
        if result.error.is_some() || result.finished {
            return None;
        }
    }
}

#[derive(Default)]
struct Tally {
    trials: usize,
    reached_handoff: usize,
    kept_fraction_sum: f64,
    some_kept: usize,
    old_span_ok: usize,
    kept_sum: usize,
    span_sum: usize,
    kinds: BTreeMap<String, usize>,
}

impl Tally {
    fn add(&mut self, r: &AutoplayResult, to_handoff: usize, to_old: usize) {
        self.trials += 1;
        let handed = matches!(
            r.stop,
            TurnRedoStop::Handoff { .. } | TurnRedoStop::TailExhausted
        );
        if handed {
            self.reached_handoff += 1;
        }
        self.kept_fraction_sum += if to_handoff == 0 {
            1.0
        } else {
            (r.kept.min(to_handoff)) as f64 / to_handoff as f64
        };
        if r.kept > 0 {
            self.some_kept += 1;
        }
        self.old_span_ok += usize::from(r.kept >= to_old);
        self.kept_sum += r.kept.min(to_handoff);
        self.span_sum += to_handoff;
        let kind = match &r.stop {
            TurnRedoStop::Handoff { .. } => "handoff".to_owned(),
            TurnRedoStop::TailExhausted => "tail_exhausted".to_owned(),
            TurnRedoStop::Conflict { conflict } => format!("{:?}", conflict.kind),
        };
        *self.kinds.entry(kind).or_default() += 1;
    }

    fn merge(&mut self, o: &Tally) {
        self.trials += o.trials;
        self.reached_handoff += o.reached_handoff;
        self.kept_fraction_sum += o.kept_fraction_sum;
        self.some_kept += o.some_kept;
        self.old_span_ok += o.old_span_ok;
        self.kept_sum += o.kept_sum;
        self.span_sum += o.span_sum;
        for (k, v) in &o.kinds {
            *self.kinds.entry(k.clone()).or_default() += v;
        }
    }

    fn json(&self) -> serde_json::Value {
        let n = self.trials.max(1) as f64;
        serde_json::json!({
            "trials": self.trials,
            "round_survives_to_handoff": self.reached_handoff,
            "round_survives_pct": 100.0 * self.reached_handoff as f64 / n,
            "mean_kept_fraction": self.kept_fraction_sum / n,
            "some_kept": self.some_kept,
            "survives_old_span": self.old_span_ok,
            "survives_old_span_pct": 100.0 * self.old_span_ok as f64 / n,
            "kept_decisions": self.kept_sum,
            "span_decisions": self.span_sum,
            "stops": self.kinds,
        })
    }
}

#[derive(Default)]
struct Game3 {
    /// The feature: random positions forced.
    forced: Tally,
    /// Dice forced, cards drawn positionally (identity off).
    no_identity: Tally,
    /// A plain replay of the same decisions.
    plain: Tally,
    /// The subset where the new turn consumed different random numbers than the old one.
    forced_shifted: Tally,
    plain_shifted: Tally,
    shifted: usize,
    no_baseline: usize,
    nanos: u128,
}

fn main() {
    let mut stride = 3usize;
    let mut variants = 2u64;
    let mut roots = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--stride" => {
                stride = args
                    .next()
                    .and_then(|v| v.parse().ok())
                    .expect("--stride S")
            }
            "--variants" => {
                variants = args
                    .next()
                    .and_then(|v| v.parse().ok())
                    .expect("--variants V");
            }
            _ => roots.push(PathBuf::from(arg)),
        }
    }
    let content = ContentStore::embedded();
    let mut total = Game3::default();
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
                    continue;
                };
            let (decisions, marks) = match store.load_history(&id) {
                Ok(Some(h)) => (h.decisions, h.rng_marks),
                _ => (
                    store.load_decisions(&id).unwrap_or_default(),
                    RngMarks::new(),
                ),
            };
            let Ok((_, galaxy)) = ti4_server::map::create_game_with_template(
                content,
                &player_ids,
                seed,
                template.as_deref(),
            ) else {
                continue;
            };
            let started = Instant::now();
            let base = TailBaseline::new(&initial, Some(&galaxy), &decisions, &marks, &[]);
            let mut g = Game3::default();
            if base.replayed() < decisions.len() {
                eprintln!(
                    "{id}: baseline replays {} of {}",
                    base.replayed(),
                    decisions.len()
                );
            }
            let spans = find_turns(&decisions);
            for span in spans.iter().filter(|s| s.complete).step_by(stride) {
                let window = RedoWindow {
                    seat: span.seat.clone(),
                    start: span.start,
                    end: span.end,
                    turns: 1,
                };
                let Ok(source) = base.tail(&window) else {
                    g.no_baseline += 1;
                    continue;
                };
                let to_handoff = next_turn_start(&source.decisions, &span.seat)
                    .unwrap_or(source.decisions.len());
                let to_old = source
                    .decisions
                    .iter()
                    .position(|r| r.player == span.seat)
                    .unwrap_or(source.decisions.len());
                let mut unforced: TailSource = source.clone();
                unforced.marks.clear();
                unforced.prev_mark = None;
                unforced.reserved.clear();
                let mut no_identity: TailSource = source.clone();
                no_identity.reserved.clear();
                let mut prefix_marks = marks.clone();
                prefix_marks.truncate_to(window.start);
                let mut live_marks = prefix_marks.clone();
                live_marks
                    .deck_plan
                    .push(open_segment(window.start, &source));
                for variant in 1..=variants {
                    let Some(current) = new_turn(
                        &initial,
                        &galaxy,
                        &decisions[..window.start],
                        &span.seat,
                        &live_marks,
                        variant,
                    ) else {
                        continue;
                    };
                    let run = |src: &TailSource| {
                        autoplay(
                            &initial,
                            Some(&galaxy),
                            &current,
                            &prefix_marks,
                            window.start,
                            &span.seat,
                            src,
                        )
                    };
                    let (Ok(f), Ok(p), Ok(n)) = (run(&source), run(&unforced), run(&no_identity))
                    else {
                        continue;
                    };
                    g.forced.add(&f, to_handoff, to_old);
                    g.no_identity.add(&n, to_handoff, to_old);
                    g.plain.add(&p, to_handoff, to_old);
                    // The new turn shifted the random streams when its natural position at the
                    // join differs from the original's.
                    if p.join_positions != source.prev_mark {
                        g.shifted += 1;
                        g.forced_shifted.add(&f, to_handoff, to_old);
                        g.plain_shifted.add(&p, to_handoff, to_old);
                    }
                }
            }
            g.nanos = started.elapsed().as_nanos();
            println!(
                "{}",
                serde_json::json!({
                    "game": id,
                    "decisions": decisions.len(),
                    "turns": spans.len(),
                    "baseline_replayed": base.replayed(),
                    "seconds": g.nanos as f64 / 1e9,
                    "forced": g.forced.json(),
                    "no_identity": g.no_identity.json(),
                    "plain": g.plain.json(),
                    "rng_shifted_trials": g.shifted,
                    "forced_when_shifted": g.forced_shifted.json(),
                    "plain_when_shifted": g.plain_shifted.json(),
                })
            );
            total.forced.merge(&g.forced);
            total.no_identity.merge(&g.no_identity);
            total.plain.merge(&g.plain);
            total.forced_shifted.merge(&g.forced_shifted);
            total.plain_shifted.merge(&g.plain_shifted);
            total.shifted += g.shifted;
        }
    }
    println!(
        "{}",
        serde_json::json!({ "TOTAL": {
            "forced": total.forced.json(),
            "no_identity": total.no_identity.json(),
            "plain": total.plain.json(),
            "rng_shifted_trials": total.shifted,
            "forced_when_shifted": total.forced_shifted.json(),
            "plain_when_shifted": total.plain_shifted.json(),
        }})
    );
}
