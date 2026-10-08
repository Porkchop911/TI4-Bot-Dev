//! The decision panel's data, separated from its widgets.
//!
//! R02-006c lifted "what happened this step, who chose it, how sure were they, what does the clicked
//! system hold" out of the egui code into [`ti4_review::view`]. The replayer will show the same
//! numbers beside a human choice, and a replayer that recomputed them itself would drift from the
//! reviewer within a week.
//!
//! Decisions only exist in a run that has played, so these tests advance a review on the committed
//! example inputs and read what the engine actually recorded, then bend a clone for the cases a real
//! run gets rarely: no choice, no probabilities, no context, no metadata.

use std::path::PathBuf;

use ti4_model::id::SystemId;
use ti4_review::panels;
use ti4_review::view::{
    DecisionPath, DecisionRow, action_summary, action_summary_in, decision_rows, event_rows,
    json_pretty, path_kind, precision, precision3, selected_system, step_view,
};
use ti4_review::{
    AdvanceUnit, LiveReview, ProfileTable, ReviewFrame, ReviewSession, SimulationConfig,
};

fn workspace_root() -> PathBuf {
    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    loop {
        if dir
            .join("examples/reviewer/checkpoint-473312/slots.json")
            .is_file()
        {
            return dir;
        }
        assert!(
            dir.pop(),
            "no workspace root carries the committed reviewer inputs"
        );
    }
}

/// A review that has actually played, because a decision panel with no decisions tests nothing.
fn played_session(steps: usize) -> ReviewSession {
    let root = workspace_root();
    let config = SimulationConfig {
        checkpoint: root.join("examples/reviewer/checkpoint-473312/slots.json"),
        map_pool: root.join("examples/reviewer/full_np8_12_holdout.json"),
        seed: 4_242,
        rotation: 1,
        table: ProfileTable::Learner,
        temperature: 0.5,
        diplomacy: false,
        lineup: None,
    };
    let mut review =
        LiveReview::start(&config).expect("the committed example inputs start a review");
    review.advance(AdvanceUnit::Step, steps);
    let session = review.session;
    session
        .validate()
        .expect("the run produced a valid session");
    session
}

fn frame_with_decisions(session: &ReviewSession) -> ReviewFrame {
    session
        .frames
        .iter()
        .rev()
        .find(|frame| !frame.decisions.is_empty())
        .cloned()
        .expect("a dozen engine steps settle at least one policy choice")
}

#[test]
fn precision_helpers_speak_for_the_missing_ones() {
    assert_eq!(precision(None), "—");
    assert_eq!(precision3(None), "—");
    assert_eq!(precision(Some(0.5)), "0.50000");
    assert_eq!(precision3(Some(0.5)), "0.500");
    // Rounding is presentation, and the reviewer's is five places: a reader comparing two policies
    // at the fourth decimal must not be shown a truncated number as if it were exact.
    assert_eq!(precision(Some(1.0 / 3.0)), "0.33333");
    assert_eq!(precision3(Some(1.0 / 3.0)), "0.333");
}

#[test]
fn step_view_reads_the_header_the_reviewer_shows() {
    let session = played_session(6);
    let frame = session.frames.last().expect("the run has frames");
    let step = step_view(frame);
    assert_eq!(step.engine_step, frame.engine_step);
    assert_eq!(step.decision_count, frame.decision_count);
    assert_eq!(step.action_count, frame.action_count);
    assert_eq!(step.round, frame.round);
    assert_eq!(step.phase, format!("{:?}", frame.phase));
    assert_eq!(
        step.active,
        frame.active.clone().unwrap_or_else(|| "—".to_owned())
    );
    // The em dash is the model's answer, not the widget's, so a replayer cannot invent a different
    // way of saying "the engine is not waiting on anybody".
    for field in [&step.active_system, &step.pending] {
        assert!(
            !field.is_empty() && !field.contains("None"),
            "a missing value leaked its Rust spelling"
        );
    }
    assert_eq!(step_view(frame), step, "the same frame answered twice");

    // Votes, predictions and staged rerolls appear in that order, each in the reviewer's words.
    let mut busy = frame.clone();
    busy.state.reroll_staging.insert(
        ti4_model::id::PlayerId::new("seat0"),
        ti4_model::state::RerollSet {
            kind: "ground".to_owned(),
            system: SystemId::new("whatever"),
            rolls: Vec::new(),
        },
    );
    busy.state
        .agenda_votes
        .insert(ti4_model::id::PlayerId::new("seat1"), "for".to_owned());
    busy.state
        .agenda_predictions
        .insert(ti4_model::id::PlayerId::new("seat2"), "against".to_owned());
    let busy = step_view(&busy);
    assert_eq!(
        busy.agenda_lines,
        vec![
            "Reroll staging: 1 player(s)".to_owned(),
            "Vote: seat1 → for".to_owned(),
            "Prediction: seat2 → against".to_owned(),
        ]
    );
}

#[test]
fn a_plan_is_not_mistaken_for_a_choice() {
    for path in ["seeing-mlp", "seeing", "blind"] {
        assert_eq!(
            path_kind(path),
            DecisionPath::Policy,
            "{path} is the policy"
        );
        assert_eq!(path_kind(path).annotation(), None);
    }
    let plan = path_kind("fleet decision");
    assert_eq!(plan, DecisionPath::FleetPlan);
    assert_eq!(plan.annotation(), Some("fleet decision"));
    // An unknown path is shown verbatim: a new engine path must be visible, not silently classified
    // as one of the three known spellings.
    let unknown = path_kind("caretaker intervention");
    assert_eq!(
        unknown,
        DecisionPath::Other("caretaker intervention".to_owned())
    );
    assert_eq!(unknown.annotation(), Some("caretaker intervention"));
}

#[test]
fn a_decision_row_carries_the_prompt_the_choice_and_the_rank() {
    let session = played_session(12);
    let frame = frame_with_decisions(&session);
    let rows = decision_rows(&frame);
    assert_eq!(
        rows.len(),
        frame.decisions.len(),
        "the panel dropped a decision of this step"
    );
    for (row, detail) in rows.iter().zip(&frame.decisions) {
        assert_eq!(row.sequence, detail.sequence);
        assert_eq!(row.player, detail.player);
        assert_eq!(row.faction, detail.faction);
        assert_eq!(row.prompt, detail.prompt);
        assert_eq!(row.path, path_kind(&detail.path));
        assert_eq!(
            row.summary,
            format!(
                "{} → {} · temperature {:?} · chosen {}",
                detail.requested_head,
                detail.resolved_head,
                detail.temperature,
                detail.chosen.as_deref().unwrap_or("ERROR")
            )
        );
        let selected: Vec<&str> = row
            .options
            .iter()
            .filter(|option| option.selected)
            .map(|option| option.id.as_str())
            .collect();
        match &detail.chosen {
            Some(chosen) if row.options.iter().any(|option| option.id == *chosen) => {
                assert_eq!(
                    selected,
                    vec![chosen.as_str()],
                    "exactly one option is ticked"
                );
                assert!(
                    row.options
                        .iter()
                        .find(|option| option.selected)
                        .is_some_and(|option| option.title.starts_with("✓ ")),
                    "the chosen option's header carries the tick"
                );
            }
            _ => assert!(
                selected.is_empty(),
                "a choice ticked an option it was not given"
            ),
        }
        // Rank is the position among the options that have probabilities, which is the only ranking
        // the reader can compare against.
        if let Some(rank) = &row.rank {
            let ranked: Vec<f64> = detail
                .options
                .iter()
                .filter_map(|option| option.probability)
                .collect();
            assert_eq!(rank.ranked, ranked.len());
            assert!(rank.position >= 1 && rank.position <= rank.ranked);
            let chosen = detail
                .options
                .iter()
                .find(|option| Some(option.id.as_str()) == detail.chosen.as_deref())
                .and_then(|option| option.probability)
                .expect("a rank implies the choice had a probability");
            assert_eq!(rank.probability, format!("{chosen:.5}"));
            let best = ranked.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            assert_eq!(rank.best, format!("{best:.5}"));
            assert_eq!(rank.below_greedy, rank.position > 1);
        }
    }
}

#[test]
fn options_render_their_titles_payloads_and_features() {
    let session = played_session(12);
    let frame = frame_with_decisions(&session);
    let rows = decision_rows(&frame);
    for (row, detail) in rows.iter().zip(&frame.decisions) {
        for option in &row.options {
            let detail_option = detail
                .options
                .iter()
                .find(|candidate| candidate.id == option.id)
                .expect("the row invented an option");
            assert_eq!(
                option.title,
                format!(
                    "{}{} · score {} · p {}",
                    if option.selected { "✓ " } else { "" },
                    option.label,
                    precision(option.score),
                    precision(option.probability)
                )
            );
            assert_eq!(option.payload.is_some(), !detail_option.payload.is_empty());
            assert_eq!(option.preview.is_some(), detail_option.preview.is_some());
            assert_eq!(option.features.len(), detail_option.features.len());
            for (feature, detail_feature) in option.features.iter().zip(&detail_option.features) {
                assert_eq!(feature.name, detail_feature.name);
                assert_eq!(feature.value, format!("{:.3}", detail_feature.value));
                assert_eq!(
                    feature.weight,
                    detail_feature
                        .weight
                        .map_or_else(|| "nonlinear".to_owned(), |weight| format!("{weight:.5}"))
                );
                assert_eq!(
                    feature.contribution,
                    detail_feature.contribution.map_or_else(
                        || "—".to_owned(),
                        |contribution| format!("{contribution:.5}")
                    )
                );
            }
        }
    }
}

#[test]
fn a_row_survives_the_pieces_a_frame_might_not_have() {
    let session = played_session(12);

    // No choice at all: the row says ERROR and offers no rank.
    let mut frame = frame_with_decisions(&session);
    for decision in &mut frame.decisions {
        decision.chosen = None;
    }
    for row in decision_rows(&frame) {
        assert!(row.summary.ends_with("chosen ERROR"));
        assert!(
            row.rank.is_none(),
            "nothing was chosen, so nothing is ranked"
        );
        assert!(
            row.options.iter().all(|option| !option.selected),
            "an unmade choice ticked an option"
        );
    }

    // Probabilities missing: the ranking disappears rather than ranking everything equal at zero.
    let mut frame = frame_with_decisions(&session);
    for decision in &mut frame.decisions {
        for option in &mut decision.options {
            option.probability = None;
        }
    }
    for row in decision_rows(&frame) {
        assert!(
            row.rank.is_none(),
            "a rank was invented out of missing probabilities"
        );
        for option in &row.options {
            assert!(option.title.ends_with("p —"));
        }
    }

    // Context missing, said plainly.
    let mut frame = frame_with_decisions(&session);
    for decision in &mut frame.decisions {
        decision.context = None;
    }
    assert!(
        decision_rows(&frame)
            .iter()
            .all(|row| row.context.is_none())
    );

    // A choice the frame does not list cannot be ranked either: the engine's answer and the offered
    // set can disagree in a legacy session, and the row must not claim a rank it cannot show.
    let mut frame = frame_with_decisions(&session);
    for decision in &mut frame.decisions {
        decision.chosen = Some("not-offered".to_owned());
    }
    for row in decision_rows(&frame) {
        assert!(row.rank.is_none());
        assert!(
            row.options.iter().all(|option| !option.selected),
            "a choice outside the offered set ticked something"
        );
    }
}

/// The sheets can be handed their history as a separate frame list, because the replayer's store keeps
/// a frame-stripped session shell per branch. Moving where the frames come from is allowed; changing
/// what R01 reads out of them is not.
#[test]
fn scanning_a_supplied_history_answers_exactly_as_scanning_the_session() {
    let session = played_session(12);
    for frame in &session.frames {
        assert_eq!(
            action_summary_in(&session.frames, frame),
            action_summary(&session, frame),
            "frame {} reads a different action history out of its own list",
            frame.index
        );
    }
    // And the previous frame found by index is the previous frame found by position, which is what lets
    // the players sheet drop `session.frames[frame.index - 1]` without changing its speaker row.
    let sheets = panels::Sheets::whole(&session);
    for frame in session.frames.iter().skip(1) {
        assert_eq!(
            sheets.previous(frame).map(|previous| previous.index),
            Some(frame.index - 1),
            "frame {} found a different neighbour",
            frame.index
        );
    }
    assert_eq!(sheets.previous(&session.frames[0]), None);
}

#[test]
fn an_action_in_progress_is_not_history() {
    let session = played_session(20);
    // The last frame whose turn is over: since OP-08 a turn may pause on "end turn" after its
    // action, and that pause is still the turn in progress.
    let frame = session
        .frames
        .iter()
        .rev()
        .find(|frame| frame.action_in_progress.is_none() && frame.action_summary.is_some())
        .expect("the run completes a turn");
    let summary = action_summary(&session, frame);
    assert!(
        summary.headline.is_some(),
        "a played turn should have something to report"
    );
    assert_eq!(summary.title, "Latest completed action");
    let span = summary
        .span
        .clone()
        .expect("a completed action spans frames");
    let end = span
        .split('–')
        .nth(1)
        .and_then(|rest| rest.split(char::is_whitespace).next())
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or_else(|| panic!("unparsable span {span:?}"));
    assert!(end <= frame.index, "the summary points ahead of itself");

    let mut in_progress = frame.clone();
    in_progress.action_summary = None;
    let none = action_summary(&session, &in_progress);
    assert_eq!(none.title, "Latest completed action");

    in_progress.action_in_progress = Some(ti4_review::ActionSummary {
        actor: "seat0".to_owned(),
        faction: "sol".to_owned(),
        headline: "Move a fleet".to_owned(),
        start_frame: frame.index,
        end_frame: frame.index,
        details: vec!["cruiser moves".to_owned()],
        in_progress: true,
    });
    let active = action_summary(&session, &in_progress);
    assert_eq!(active.title, "Action in progress");
    assert_eq!(active.headline.as_deref(), Some("Move a fleet"));
    assert!(
        active
            .span
            .as_deref()
            .is_some_and(|span| span.ends_with(" · IN PROGRESS")),
        "a half-played turn must say so"
    );
    assert_eq!(active.details, vec!["cruiser moves".to_owned()]);
}

#[test]
fn event_rows_match_the_frame() {
    let session = played_session(12);
    let mut frame = session.frames.last().expect("the run has frames").clone();
    frame.structured_events.clear();
    frame.new_events.clear();
    assert!(event_rows(&frame).is_empty());
    assert_eq!(decision_rows(&frame).len(), frame.decisions.len());

    frame.structured_events.push(ti4_review::ReviewEvent {
        id: 7,
        event_type: "combat.started".to_owned(),
        payload: std::iter::once(("system".to_owned(), serde_json::json!("123"))).collect(),
        cancelled: false,
    });
    frame.structured_events.push(ti4_review::ReviewEvent {
        id: 8,
        event_type: "tactic.combat".to_owned(),
        payload: std::iter::empty().collect(),
        cancelled: true,
    });
    frame.new_events.push("legacy trace line".to_owned());
    let rows = event_rows(&frame);
    assert_eq!(rows.len(), 2, "both events are listed");
    assert_eq!(rows[0].title, "#7 combat.started");
    assert_eq!(rows[1].title, "#8 tactic.combat · CANCELLED");
    assert_eq!(rows[0].id, 7);
    assert!(!rows[0].cancelled && rows[1].cancelled);
    assert!(
        rows[0].payload.contains("\"system\""),
        "the payload is the JSON the reviewer prints: {:?}",
        rows[0].payload
    );
    assert_eq!(rows[1].payload, "{}");
    // The legacy name trace stays where it is: the model lists structured events, the panel still
    // knows to offer the legacy trace beside them.
    assert_eq!(frame.new_events.len(), 1);
}

#[test]
fn the_selected_system_card_says_what_it_knows() {
    let session = played_session(2);
    let frame = session.frames.last().expect("the run has a frame").clone();
    let tile = session
        .board
        .iter()
        .find(|tile| !tile.planets.is_empty())
        .expect("the board has a planeted system");
    let card = selected_system(&session, &frame, &tile.system);
    assert!(card.has_metadata);
    assert_eq!(
        card.title,
        format!("Selected system {} [{}]", tile.label, tile.system)
    );
    assert!(
        card.lines
            .contains(&format!("Map coordinate: {}, {}", tile.q, tile.r)),
        "the card forgot where the system is"
    );
    assert_eq!(card.planets.len(), tile.planets.len());
    for line in &card.planets {
        assert!(
            line.starts_with("• "),
            "planet line {line:?} lost its bullet"
        );
        assert!(
            line.contains(" · trait "),
            "planet line {line:?} has no traits"
        );
        assert!(
            line.contains(" · specialty "),
            "planet line {line:?} has no specialty"
        );
    }

    // A system the session has no metadata for is said out loud, not shown as an empty card.
    let unknown = selected_system(&session, &frame, "no-such-system");
    assert!(!unknown.has_metadata);
    assert_eq!(unknown.title, "Selected system no-such-system");
    assert_eq!(
        unknown.lines,
        vec!["Map metadata unavailable in this legacy review.".to_owned()]
    );
    assert!(unknown.planets.is_empty());
    assert!(unknown.dynamic.is_none(), "nothing is in an unknown system");

    // Dynamic state appears as JSON when there is any.
    let mut busy = frame.clone();
    busy.state.board.insert(
        SystemId::new(&tile.system),
        ti4_model::state::SystemState::default(),
    );
    let card = selected_system(&session, &busy, &tile.system);
    let dynamic = card.dynamic.expect("a system state was placed");
    assert!(dynamic.starts_with('{') && dynamic.ends_with('}'));
    assert_eq!(
        dynamic,
        json_pretty(
            &serde_json::to_value(ti4_model::state::SystemState::default()).unwrap_or_default()
        )
    );
}

#[test]
fn the_models_are_pure_functions_of_the_frame() {
    let session = played_session(12);
    let frame = frame_with_decisions(&session);
    let first: Vec<DecisionRow> = decision_rows(&frame);
    assert_eq!(first, decision_rows(&frame), "the same frame, two answers");
    assert_eq!(step_view(&frame), step_view(&frame));
    assert_eq!(
        action_summary(&session, &frame),
        action_summary(&session, &frame)
    );
    // And a frame does not answer with the latest news: an earlier frame reports its own decisions.
    let earlier = session
        .frames
        .iter()
        .rev()
        .find(|candidate| candidate.index + 1 < frame.index && candidate.decisions.len() > 1)
        .cloned();
    if let Some(earlier) = earlier {
        assert_ne!(
            decision_rows(&earlier),
            first,
            "two frames collapsed to one answer"
        );
    }
}

/// BUG-09 (operator, 2026-09-20): "Warfare asks production before the strategy spend". Pinned on real
/// play: a follower is asked whether to spend a strategy token to produce at home, and what to
/// produce is asked only afterwards, and only of a follower who said yes.
#[test]
fn warfare_asks_the_strategy_spend_before_production() {
    let root = workspace_root();
    let config = SimulationConfig {
        checkpoint: root.join("examples/reviewer/checkpoint-473312/slots.json"),
        map_pool: root.join("examples/reviewer/full_np8_12_holdout.json"),
        seed: 4_242,
        rotation: 1,
        table: ProfileTable::Learner,
        temperature: 0.5,
        diplomacy: false,
        lineup: None,
    };
    let mut review = LiveReview::start(&config).expect("the example table starts");
    let mut decisions = Vec::new();
    for _ in 0..1_500 {
        let frame = review.step_once().clone();
        decisions.extend(frame.decisions.iter().cloned());
        if frame.finished {
            break;
        }
    }
    let mut followed = 0;
    for (index, decision) in decisions.iter().enumerate() {
        if decision.prompt != "spend a strategy token to produce at home" {
            continue;
        }
        let next = decisions[index + 1..]
            .iter()
            .find(|later| later.player == decision.player)
            .map(|later| later.prompt.as_str())
            .unwrap_or_default();
        if decision.chosen.as_deref() == Some("yes") {
            followed += 1;
            // Usually the production ask; a home with nothing affordable may produce nothing, but
            // production is never what came *before* the spend.
            assert!(
                next.starts_with("produce in") || !next.contains("produce"),
                "{}: after following Warfare, got {next:?}",
                decision.player
            );
        } else {
            assert!(
                !next.starts_with("produce in"),
                "{} declined Warfare and was still asked to produce: {next:?}",
                decision.player
            );
        }
    }
    assert!(
        followed > 0,
        "no seat followed Warfare; the check is vacuous"
    );
}
