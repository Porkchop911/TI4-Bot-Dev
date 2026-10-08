//! The replayer's game driver must finish rounds at a near-greedy temperature.
//!
//! Reported 2026-09-22: at low temperature the replayer loops forever. A greedy policy repeats the
//! same choice whenever the state it sees does not change, so any option that leaves the state
//! untouched and is offered again is a loop. This plays two rounds with and without diplomacy at
//! temperature 0.01 and requires every round to end within a step budget.

use std::path::PathBuf;

use ti4_review::{LiveReview, ProfileTable, SimulationConfig};

fn workspace_root() -> PathBuf {
    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    loop {
        if dir
            .join("examples/reviewer/checkpoint-473312/slots.json")
            .is_file()
        {
            return dir;
        }
        assert!(dir.pop(), "no workspace root carries the reviewer inputs");
    }
}

fn rounds_finish(diplomacy: bool, seed: u64) {
    let root = workspace_root();
    // TI4_LOW_TEMPERATURE_CHECKPOINT points the check at another bundle (an MLP checkpoint the
    // replayer is playing with); the committed example is the default.
    let checkpoint = std::env::var_os("TI4_LOW_TEMPERATURE_CHECKPOINT").map_or_else(
        || root.join("examples/reviewer/checkpoint-473312/slots.json"),
        PathBuf::from,
    );
    let config = SimulationConfig {
        checkpoint,
        map_pool: root.join("examples/reviewer/full_np8_12_holdout.json"),
        seed,
        rotation: 1,
        table: ProfileTable::Learner,
        temperature: 0.01,
        diplomacy,
        lineup: None,
    };
    let mut review = LiveReview::start(&config).expect("the example table starts");
    for round in 0..2 {
        let report = review.advance_to_next_round();
        let last = review.session.frames.last().expect("frames");
        let recent: Vec<String> = review
            .session
            .frames
            .iter()
            .rev()
            .take(6)
            .flat_map(|frame| {
                frame
                    .decisions
                    .iter()
                    .map(|d| format!("{} -> {:?}", d.prompt, d.chosen))
            })
            .collect();
        assert!(
            report.reached_target || last.finished,
            "diplomacy {diplomacy}, seed {seed}: round {} did not finish ({} steps); recent: {recent:?}",
            round + 1,
            report.steps
        );
    }
}

#[test]
fn greedy_rounds_finish_without_diplomacy() {
    rounds_finish(false, 4_242);
}

#[test]
fn greedy_rounds_finish_with_diplomacy() {
    rounds_finish(true, 4_242);
    rounds_finish(true, 7);
}

/// Diagnostic: a fixed step budget, then the prompts that repeat most among the last decisions.
#[test]
#[ignore = "diagnostic; set TI4_LOW_TEMPERATURE_CHECKPOINT"]
fn greedy_loop_probe() {
    let root = workspace_root();
    let checkpoint = std::env::var_os("TI4_LOW_TEMPERATURE_CHECKPOINT").map_or_else(
        || root.join("examples/reviewer/checkpoint-473312/slots.json"),
        PathBuf::from,
    );
    let config = SimulationConfig {
        checkpoint,
        map_pool: root.join("examples/reviewer/full_np8_12_holdout.json"),
        seed: 12,
        rotation: 0,
        table: ProfileTable::Learner,
        temperature: 0.01,
        diplomacy: true,
        lineup: None,
    };
    let mut review = LiveReview::start(&config).expect("the example table starts");
    for chunk in 0..8 {
        review.advance(ti4_review::AdvanceUnit::Step, 250);
        let last = review.session.frames.last().expect("frames");
        eprintln!(
            "after {} steps: round {} phase {:?} active {:?}",
            (chunk + 1) * 250,
            last.round,
            last.phase,
            last.active
        );
    }
    let mut counts = std::collections::BTreeMap::<String, usize>::new();
    for frame in review.session.frames.iter().rev().take(300) {
        for decision in &frame.decisions {
            *counts
                .entry(format!(
                    "{} | {} -> {:?}",
                    decision.player, decision.prompt, decision.chosen
                ))
                .or_default() += 1;
        }
    }
    let mut counts: Vec<_> = counts.into_iter().collect();
    counts.sort_by(|a, b| b.1.cmp(&a.1));
    for (line, count) in counts.into_iter().take(12) {
        eprintln!("{count:5}  {line}");
    }
}

/// Diagnostic: near-greedy, the option a seat takes must be the one its trace ranks first. A
/// diplomacy decision traced on the wrong head showed "Decline p=1.0" beside an accept.
#[test]
#[ignore = "diagnostic; set TI4_LOW_TEMPERATURE_CHECKPOINT to an MLP bundle"]
fn traced_choices_match_the_greedy_option() {
    let root = workspace_root();
    let checkpoint = std::env::var_os("TI4_LOW_TEMPERATURE_CHECKPOINT").map_or_else(
        || root.join("examples/reviewer/checkpoint-473312/slots.json"),
        PathBuf::from,
    );
    let config = SimulationConfig {
        checkpoint,
        map_pool: root.join("examples/reviewer/full_np8_12_holdout.json"),
        seed: 12,
        rotation: 0,
        table: ProfileTable::Learner,
        temperature: 0.01,
        diplomacy: true,
        lineup: None,
    };
    let mut review = LiveReview::start(&config).expect("the table starts");
    review.advance(ti4_review::AdvanceUnit::Step, 1500);
    let (mut checked, mut diplomacy, mut mismatched) = (0, 0, Vec::new());
    for frame in &review.session.frames {
        for decision in &frame.decisions {
            let Some(best) = decision
                .options
                .iter()
                .filter_map(|option| option.probability.map(|p| (p, option.id.clone())))
                .max_by(|a, b| a.0.total_cmp(&b.0))
            else {
                continue;
            };
            if best.0 < 0.999 || decision.path.contains("plan") {
                continue; // no clear greedy choice, or a fleet plan answered
            }
            checked += 1;
            if decision.resolved_head == "diplomacy" {
                diplomacy += 1;
            }
            if decision.chosen.as_deref() != Some(best.1.as_str()) {
                mismatched.push(format!(
                    "{} [{}]: chose {:?}, trace ranks {} first ({:.3})",
                    decision.prompt, decision.resolved_head, decision.chosen, best.1, best.0
                ));
            }
        }
    }
    eprintln!("{checked} clear decisions checked, {diplomacy} on the diplomacy head");
    assert!(mismatched.is_empty(), "{mismatched:#?}");
}

/// Diagnostic, 2026-09-23: command tokens, exhausted planets and Xxcha's commander across the first
/// agenda phase of a replayer table (seed and rotation by env, checkpoint by
/// TI4_LOW_TEMPERATURE_CHECKPOINT).
#[test]
#[ignore = "diagnostic; set TI4_LOW_TEMPERATURE_CHECKPOINT"]
fn agenda_phase_probe() {
    let root = workspace_root();
    let checkpoint = std::env::var_os("TI4_LOW_TEMPERATURE_CHECKPOINT").map_or_else(
        || root.join("examples/reviewer/checkpoint-473312/slots.json"),
        PathBuf::from,
    );
    let seed: u64 = std::env::var("TI4_PROBE_SEED")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(411);
    let config = SimulationConfig {
        checkpoint,
        map_pool: root.join("examples/reviewer/full_np8_12_holdout.json"),
        seed,
        rotation: 0,
        table: ProfileTable::Learner,
        temperature: 0.01,
        diplomacy: true,
        lineup: None,
    };
    let mut review = LiveReview::start(&config).expect("the table starts");
    let mut last: Option<(
        ti4_model::state::Phase,
        u32,
        Vec<(String, i32, i32, i32)>,
        usize,
    )> = None;
    let mut seen_agenda = false;
    for _ in 0..60_000 {
        let frame = review.step_once().clone();
        let state = &frame.state;
        let tokens: Vec<(String, i32, i32, i32)> = state
            .players
            .iter()
            .map(|p| {
                (
                    p.faction.to_string(),
                    p.tactic_tokens,
                    p.fleet_tokens,
                    p.strategic_tokens,
                )
            })
            .collect();
        let now = (
            state.phase,
            state.round,
            tokens,
            state.exhausted_planets.len(),
        );
        if state.phase == ti4_model::state::Phase::Agenda {
            seen_agenda = true;
        }
        let interesting = frame.new_events.iter().any(|e| {
            e.starts_with("AGENDA")
                || e.starts_with("COMMAND_TOKEN")
                || e.starts_with("STATUS")
                || e.starts_with("ROUND")
                || e.starts_with("VOTE")
                || e.starts_with("LAW")
                || e.contains("xxchacommander")
        });
        if seen_agenda || interesting {
            if last.as_ref() != Some(&now) || interesting {
                eprintln!(
                    "#{} {:?} r{} tokens {:?} exhausted {} | {:?}",
                    frame.index, now.0, now.1, now.2, now.3, frame.new_events
                );
                for d in &frame.decisions {
                    eprintln!("     {} | {} -> {:?}", d.player, d.prompt, d.chosen);
                }
            }
        }
        last = Some(now);
        if seen_agenda && state.phase == ti4_model::state::Phase::Action {
            break;
        }
        if frame.finished {
            break;
        }
    }
    let xxcha = review.session.frames.last().and_then(|f| {
        f.state
            .players
            .iter()
            .find(|p| p.faction.as_str() == "xxcha")
            .cloned()
    });
    if let Some(seat) = xxcha {
        eprintln!("xxcha leaders {:?}", seat.leaders);
    }
}
