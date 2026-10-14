//! Engine-driven tests for one live branch: a real game on one thread, parked at real decisions.
//!
//! These run the reviewer's own `LiveReview` through [`LiveBranch`], so a pause is the engine paused
//! inside `Table::ask` and an answer is validated by `Table::settle`. The inputs are the committed
//! example bundle and map pool, so there is no skip path.

// Every test here drives a live branch, which only the `host` build has.
#![cfg(feature = "host")]

use std::collections::BTreeMap;
use std::env;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use ti4_model::id::PlayerId;
use ti4_replayer::live::{AdvanceGoal, LiveBranch, LiveError, LiveEvent, LiveState};
use ti4_replayer::{
    ManualSubmission, ModeEffect, PendingManualChoice, Provenance, SeatControl, SeatMode,
    SubmitOutcome,
};
use ti4_review::{ProfileTable, SimulationConfig};

const CHECKPOINT: &str = "examples/reviewer/checkpoint-473312/slots.json";
const MAP_POOL: &str = "examples/reviewer/full_np8_12_holdout.json";
const SEED: u64 = 4_242;
const ROTATION: usize = 1;
const TEMPERATURE: f64 = 0.5;
const ALL_SEATS: [&str; 6] = ["seat0", "seat1", "seat2", "seat3", "seat4", "seat5"];
/// A parked branch is waiting for the test, not for the CPU, so this is generous -- but it is
/// not infinite, because a pause that never happened should fail in minutes, not hours.
const WAIT: Duration = Duration::from_secs(120);

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crates/<name> sits directly under the workspace root")
        .to_path_buf()
}

fn env_path(name: &str, default: &str) -> PathBuf {
    env::var_os(name).map_or_else(|| workspace_root().join(default), PathBuf::from)
}

fn config() -> SimulationConfig {
    SimulationConfig {
        checkpoint: env_path("TI4_REPLAYER_CHECKPOINT", CHECKPOINT),
        map_pool: env_path("TI4_REPLAYER_POOL", MAP_POOL),
        seed: SEED,
        rotation: ROTATION,
        table: ProfileTable::Learner,
        temperature: TEMPERATURE,
        diplomacy: false,
        lineup: None,
        rule_seats: Vec::new(),
    }
}

fn seats_with_manual(names: &[&str]) -> SeatControl {
    let mut seats = SeatControl::all_auto();
    for name in names {
        seats.set_mode(&PlayerId::new(*name), SeatMode::Manual);
    }
    seats
}

/// Start one live branch with the named seats on Manual and the rest on Auto.
fn start(manual: &[&str]) -> LiveBranch {
    let config = config();
    for path in [&config.checkpoint, &config.map_pool] {
        assert!(
            path.is_file(),
            "live-branch input {} is missing from the checkout",
            path.display()
        );
    }
    let branch = LiveBranch::start(config, seats_with_manual(manual)).expect("spawn the branch");
    // The thread builds the game before it will accept an advance; wait for that rather than
    // racing it, because a setup failure must surface as a Failed state, not a lost command.
    let deadline = Instant::now() + WAIT;
    loop {
        match branch.gate().state() {
            LiveState::Ready => break,
            LiveState::Failed => {
                let reason = branch
                    .drain_events()
                    .into_iter()
                    .find_map(|event| match event {
                        LiveEvent::Failed(reason) => Some(reason),
                        _ => None,
                    })
                    .unwrap_or_else(|| "no reason reported".to_owned());
                panic!("the branch failed to start: {reason}");
            }
            _ if Instant::now() >= deadline => panic!("the branch never became ready"),
            _ => std::thread::sleep(Duration::from_millis(5)),
        }
    }
    branch.drain_events();
    branch
}

/// Everything one driven advance showed us.
#[derive(Debug, Default)]
struct Driven {
    /// Engine frames the branch reported.
    frames: usize,
    /// Every offer the branch parked on, in order.
    parks: Vec<PendingManualChoice>,
}

/// Run a goal, answering every manual offer with its first option until the goal comes to rest.
fn drive(branch: &LiveBranch, goal: AdvanceGoal) -> Driven {
    let mut out = Driven::default();
    branch.gate().run(goal).expect("an advance is accepted");
    let deadline = Instant::now() + WAIT;
    loop {
        out.frames += branch
            .drain_events()
            .into_iter()
            .filter(|event| matches!(event, LiveEvent::Frame(_)))
            .count();
        if let Some(pending) = branch.gate().pending() {
            let _ = branch.drain_events();
            let option_id = pending
                .options
                .first()
                .expect("a panel always offers something")
                .id
                .clone();
            let outcome = branch.gate().submit(&ManualSubmission {
                offer: pending.offer,
                fingerprint: pending.fingerprint.clone(),
                option_id: option_id.clone(),
            });
            assert_eq!(
                outcome,
                SubmitOutcome::Accepted { option_id },
                "clicking an option on the offer currently on screen must be accepted"
            );
            out.parks.push(pending);
            continue;
        }
        if branch.wait_until_idle(Duration::from_millis(5)) {
            // Drain once more: a frame published just before the goal came to rest would otherwise
            // be counted as missing, which is a test artefact and not a lost step.
            out.frames += branch
                .drain_events()
                .into_iter()
                .filter(|event| matches!(event, LiveEvent::Frame(_)))
                .count();
            return out;
        }
        assert!(
            Instant::now() < deadline,
            "the branch never came to rest; state {:?}",
            branch.gate().state()
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// Wait for the branch to park on a manual offer.
fn park(branch: &LiveBranch) -> PendingManualChoice {
    branch
        .wait_until_parked(WAIT)
        .expect("the branch parks on a manual decision")
}

/// Wait for the log entry recording how this seat answered this exact offer.
///
/// The entry is written by the simulation thread after the panel closes, so reading it immediately
/// would race in both directions: too early and it is missing, too late and the seat has answered
/// something else. Matching on the fingerprint waits for the fact rather than for a sleep.
fn answer_to(
    branch: &LiveBranch,
    actor: &PlayerId,
    fingerprint: &ti4_replayer::ChoiceFingerprint,
) -> ti4_replayer::AnsweredDecision {
    let deadline = Instant::now() + WAIT;
    loop {
        if let Some(entry) = branch
            .gate()
            .log()
            .entries()
            .into_iter()
            .find(|entry| &entry.actor == actor && &entry.fingerprint == fingerprint)
        {
            return entry;
        }
        assert!(
            Instant::now() < deadline,
            "seat {actor} never answered the choice {fingerprint:?}"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn a_manual_seat_parks_before_any_mutation_and_play_resumes_when_answered() {
    let branch = start(&ALL_SEATS);
    branch
        .gate()
        .run(AdvanceGoal::Steps(1))
        .expect("one step is accepted");
    let pending = park(&branch);
    assert!(
        ALL_SEATS.contains(&pending.actor.as_str()),
        "the panel names a physical seat, got {}",
        pending.actor
    );
    assert!(!pending.options.is_empty());
    assert_eq!(branch.gate().state(), LiveState::WaitingForHuman);

    // Parked means parked: nothing advances and the offer does not drift.
    let frames_before = branch
        .drain_events()
        .into_iter()
        .filter(|event| matches!(event, LiveEvent::Frame(_)))
        .count();
    std::thread::sleep(Duration::from_millis(120));
    let after = branch.gate().pending().expect("still parked");
    assert_eq!(
        after.fingerprint, pending.fingerprint,
        "the panel must not drift"
    );
    assert_eq!(
        branch
            .drain_events()
            .into_iter()
            .filter(|event| matches!(event, LiveEvent::Frame(_)))
            .count(),
        frames_before,
        "a parked branch must not mutate state"
    );
    // Reading the gate must not answer it.
    let _ = branch.gate().snapshot();
    let _ = branch.gate().pending();
    assert!(branch.gate().pending().is_some());

    let answer = pending
        .options
        .first()
        .expect("a panel offers something")
        .id
        .clone();
    assert_eq!(
        branch.gate().submit(&ManualSubmission {
            offer: pending.offer,
            fingerprint: pending.fingerprint.clone(),
            option_id: answer.clone(),
        }),
        SubmitOutcome::Accepted { option_id: answer },
        "the offered option for the choice on screen is accepted"
    );
    assert!(branch.gate().pending().is_none(), "the panel closes");
    assert!(branch.wait_until_idle(WAIT), "the engine resumes");
    assert!(
        branch
            .gate()
            .log()
            .provenance()
            .contains(&Provenance::Human),
        "the decision is recorded as one a person made"
    );
}

#[test]
fn nested_asks_inside_one_engine_step_each_get_their_own_panel() {
    // The measured reference session settles more than one decision on 6.1% of steps, so a probe at
    // the step boundary could never pause for the second one. With every seat manual, every ask in
    // the same step must still stop the engine and ask separately.
    let branch = start(&ALL_SEATS);
    let driven = drive(&branch, AdvanceGoal::Steps(120));
    let mut by_frame: BTreeMap<u64, Vec<&PendingManualChoice>> = BTreeMap::new();
    for pending in &driven.parks {
        by_frame.entry(pending.frame).or_default().push(pending);
    }
    let nested = by_frame
        .iter()
        .filter(|(_, asks)| asks.len() > 1)
        .max_by_key(|(_, asks)| asks.len())
        .expect("no engine step raised more than one manual ask in 120 steps");
    let (frame, asks) = (*nested.0, nested.1);
    assert!(
        asks.len() > 1,
        "frame {frame} should carry several asks, got {}",
        asks.len()
    );
    for (ordinal, pending) in asks.iter().enumerate() {
        assert_eq!(
            u64::from(pending.ask),
            ordinal as u64,
            "asks within frame {frame} are numbered from 0 in the order they were raised"
        );
    }
    // What must hold is that each ask got its *own* panel, and the ask ordinal is what says so: the
    // gate's clock increments once per ask within a frame, so a gate that republished one offer
    // several times instead of parking again would repeat an ordinal. That is checked above and
    // again here as a set.
    //
    // This deliberately does *not* assert that the fingerprints differ, which it used to. A
    // fingerprint binds actor, prompt, ordered options and context and excludes the frame and
    // ordinal on purpose, because that is what lets a rebuild match a recorded answer to the offer
    // it was made against. The engine legitimately raises asks that are identical in all of those
    // fields: a seat six units over capacity is asked "remove a unit: over capacity in 14" six times
    // in one step, every time with the single option `remove|0` and no context, so seven asks
    // collapsed to two distinct fingerprints and the old assertion failed on correct behaviour.
    // The real defect that hid behind it - the app suppressing every ask after the first, leaving a
    // parked engine with no panel on screen - is fixed in `ReplayApp` and pinned by
    // `app.rs::an_identical_looking_ask_at_the_next_ordinal_is_a_new_question`.
    let ordinals: std::collections::BTreeSet<u32> =
        asks.iter().map(|pending| pending.ask).collect();
    assert_eq!(
        ordinals.len(),
        asks.len(),
        "each ask in frame {frame} got its own panel, rather than one panel being republished"
    );
    assert!(
        asks.iter().all(|pending| pending.frame == frame),
        "every ask grouped under frame {frame} reports that frame"
    );
    assert!(driven.frames > 1, "the branch advanced through them");

    // R02-002's one uncovered path: an ask delivered *with* a bound seat observation. No test outside
    // the engine can build one (`SeatObservation::bind` is `pub(crate)`), so the proof is that a real
    // game delivered some, and that every one of them was still answered through the gate.
    let (viewless, bound) = branch.gate().delivery();
    assert!(
        viewless + bound >= driven.parks.len(),
        "every parked ask came from one of the two entry points: {} deliveries for {} parks",
        viewless + bound,
        driven.parks.len()
    );
    assert!(
        bound > 0,
        "no ask in {} engine steps was delivered with a bound observation (viewless {viewless})",
        driven.frames
    );
}

#[test]
fn a_stale_or_invented_answer_is_refused_and_the_panel_stays_up() {
    let branch = start(&ALL_SEATS);
    branch
        .gate()
        .run(AdvanceGoal::Steps(50))
        .expect("steps are accepted");
    let first = park(&branch);
    assert!(branch.gate().snapshot().pending.is_some());
    branch.gate().submit(&ManualSubmission {
        offer: first.offer,
        fingerprint: first.fingerprint.clone(),
        option_id: first.options.first().expect("an option").id.clone(),
    });
    let second = park(&branch);
    assert_ne!(
        second.fingerprint, first.fingerprint,
        "the second ask is a different choice"
    );

    // A click aimed at the panel that was already answered.
    assert_eq!(
        branch.gate().submit(&ManualSubmission {
            offer: first.offer,
            fingerprint: first.fingerprint.clone(),
            option_id: first.options.first().expect("an option").id.clone(),
        }),
        SubmitOutcome::Stale {
            current: second.fingerprint.clone()
        },
        "an old click is refused, and says what is current so the panel can redraw"
    );
    // An invented option on the current panel.
    let refused = branch.gate().submit(&ManualSubmission {
        offer: second.offer,
        fingerprint: second.fingerprint.clone(),
        option_id: "an-option-nobody-offered".to_owned(),
    });
    match refused {
        SubmitOutcome::NotOffered { offered } => assert_eq!(
            offered,
            second
                .ids()
                .into_iter()
                .map(str::to_owned)
                .collect::<Vec<_>>(),
            "an invented option is refused with what was actually offered"
        ),
        other => panic!("an invented option must be refused as not offered, got {other:?}"),
    }
    assert_eq!(
        branch.gate().pending().expect("still parked").fingerprint,
        second.fingerprint,
        "a refusal must not consume the pending choice"
    );

    let answer = second.options.first().expect("an option").id.clone();
    assert_eq!(
        branch.gate().submit(&ManualSubmission {
            offer: second.offer,
            fingerprint: second.fingerprint.clone(),
            option_id: answer.clone(),
        }),
        SubmitOutcome::Accepted { option_id: answer },
        "after the refusals the same panel still accepts its real answer"
    );
    // The engine took that answer and moved on to another decision. Every seat is manual, so it
    // parks again rather than running to rest, and the third panel is the proof it resumed.
    let third = park(&branch);
    assert_ne!(third.fingerprint, second.fingerprint, "a later decision");
    let logged = branch.gate().log().entries();
    assert!(
        logged.len() >= 2
            && logged
                .iter()
                .any(|entry| entry.fingerprint == second.fingerprint
                    && entry.provenance == Provenance::Human),
        "both answered decisions are logged as a person's"
    );
}

#[test]
fn switching_the_waiting_seat_to_auto_lets_its_bot_answer_it() {
    let branch = start(&ALL_SEATS);
    branch
        .gate()
        .run(AdvanceGoal::Steps(3))
        .expect("steps are accepted");
    let pending = park(&branch);
    let actor = pending.actor.clone();
    let other = ALL_SEATS
        .iter()
        .map(|seat| PlayerId::new(*seat))
        .find(|seat| seat != &actor)
        .expect("six seats");

    // Changing a different seat must leave this panel alone.
    assert_eq!(
        branch.gate().set_mode(&other, SeatMode::Auto),
        ModeEffect::Changed {
            previous: SeatMode::Manual
        },
        "touching another seat changes only that seat"
    );
    assert_eq!(
        branch
            .gate()
            .pending()
            .expect("the panel is untouched")
            .fingerprint,
        pending.fingerprint,
        "changing another seat must not disturb a pending choice"
    );

    // Turning the waiting seat itself over to Auto releases its own panel to the bot.
    let effect = branch.gate().set_mode(&actor, SeatMode::Auto);
    assert_eq!(
        effect,
        ModeEffect::ReleasedBot {
            previous: SeatMode::Manual,
            choice: pending.fingerprint.clone(),
        },
        "the waiting seat's own panel goes to its bot"
    );
    assert_ne!(
        branch.gate().pending().map(|panel| panel.fingerprint),
        Some(pending.fingerprint.clone()),
        "that panel is closed"
    );
    let released = answer_to(&branch, &actor, &pending.fingerprint);
    assert_eq!(
        released.provenance,
        Provenance::Policy,
        "the bot answered the released decision"
    );
    let driven = drive(&branch, AdvanceGoal::Steps(2));
    assert!(driven.frames >= 1, "the engine moved on after the release");
}
#[test]
fn delegating_answers_one_decision_and_keeps_the_seat_manual() {
    let branch = start(&ALL_SEATS);
    branch
        .gate()
        .run(AdvanceGoal::Steps(3))
        .expect("steps are accepted");
    let pending = park(&branch);
    let actor = pending.actor.clone();

    assert_eq!(
        branch
            .gate()
            .delegate_pending(pending.offer)
            .expect("a panel was open"),
        actor,
        "delegation answers the panel that is open"
    );
    assert!(
        branch.gate().snapshot().seats.is_manual(&actor),
        "the seat stays manual"
    );
    let delegated = answer_to(&branch, &actor, &pending.fingerprint);
    assert_eq!(delegated.provenance, Provenance::DelegatedToPolicy);

    // The next decision for that seat must be a panel again, not a second silent delegation.
    let driven = drive(&branch, AdvanceGoal::Steps(120));
    let later = driven
        .parks
        .iter()
        .find(|panel| panel.actor == actor)
        .expect("the manual seat asks again after a delegation");
    assert_ne!(
        later.fingerprint, pending.fingerprint,
        "a different decision"
    );
    assert!(
        branch.gate().snapshot().seats.is_manual(&actor),
        "still manual after the delegation was spent"
    );
}

#[test]
fn a_run_of_n_steps_stops_at_exactly_n_when_every_seat_is_auto() {
    let branch = start(&[]);
    let mut expected = 1_usize; // frame 0, recorded at setup
    for wanted in [1_usize, 3, 7] {
        let driven = drive(&branch, AdvanceGoal::Steps(wanted));
        assert_eq!(
            driven.frames, wanted,
            "a run of {wanted} steps reports {wanted} frames"
        );
        assert!(
            driven.parks.is_empty(),
            "no seat is manual, so nothing may park"
        );
        expected += wanted;
    }
    let session = branch
        .into_session()
        .expect("the thread hands the session back");
    assert_eq!(
        session.frames.len(),
        expected,
        "the session agrees with the events about how far the branch got"
    );
    assert_eq!(session.manifest.seed, SEED);
    assert_eq!(session.manifest.rotation, ROTATION);
}

#[test]
fn pause_ends_the_goal_on_a_step_boundary_and_stop_is_terminal() {
    let branch = start(&[]);
    branch
        .gate()
        .run(AdvanceGoal::Steps(10_000))
        .expect("a long run is accepted");
    branch.gate().pause();
    assert!(
        branch.wait_until_idle(WAIT),
        "pause ended the goal instead of running to its target"
    );
    let frames = branch
        .drain_events()
        .into_iter()
        .filter(|event| matches!(event, LiveEvent::Frame(_)))
        .count();
    assert!(frames < 10_000, "the pause landed, but {frames} steps ran");
    assert_eq!(
        branch.gate().state(),
        LiveState::Ready,
        "ready, not stopped"
    );

    // Still alive: a fresh, smaller goal runs to completion.
    let driven = drive(&branch, AdvanceGoal::Steps(3));
    assert_eq!(driven.frames, 3, "the branch advanced again after pausing");

    branch.gate().stop();
    assert_eq!(branch.gate().state(), LiveState::Stopped);
    assert_eq!(
        branch.gate().run(AdvanceGoal::Steps(1)),
        Err(LiveError::NotRunnable(LiveState::Stopped)),
        "a stopped branch refuses to run"
    );
    std::thread::sleep(Duration::from_millis(150));
    assert_eq!(
        branch
            .drain_events()
            .into_iter()
            .filter(|event| matches!(event, LiveEvent::Frame(_)))
            .count(),
        0,
        "a stopped branch does not advance"
    );
}

/// A compact but decision-sensitive view of a session: enough to tell two playthroughs apart.
fn digest(session: &ti4_review::ReviewSession) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    for frame in &session.frames {
        let _ = write!(
            out,
            "{}|{}|{}|{:?}|{}|",
            frame.index,
            frame.engine_step,
            frame.round,
            frame.phase,
            frame.active.as_deref().unwrap_or("-"),
        );
        for decision in &frame.decisions {
            let _ = write!(
                out,
                "{}>{}:{:?};",
                decision.player,
                decision.path,
                decision.chosen.as_deref().unwrap_or("-"),
            );
        }
        out.push('\n');
    }
    out
}

/// Run the same script twice, byte for byte: three separate advance commands of ten steps each.
fn script(branch: &LiveBranch) -> usize {
    let mut frames = 0;
    for _ in 0..3 {
        frames += drive(branch, AdvanceGoal::Steps(10)).frames;
    }
    frames
}

#[test]
fn the_same_command_script_produces_the_same_session() {
    // A deterministic command script on an all-Auto branch must reproduce the reviewer's own
    // outcome: the gate must not perturb the engine's ordering or RNG.
    let first = start(&[]);
    let frames_first = script(&first);
    let session_first = first.into_session().expect("first session");

    let second = start(&[]);
    let frames_second = script(&second);
    let session_second = second.into_session().expect("second session");

    assert_eq!(frames_first, frames_second);
    assert_eq!(
        digest(&session_first),
        digest(&session_second),
        "the same script produced different play"
    );
    // Cross-check against the reviewer's own single-call form of the same walk.
    let third = start(&[]);
    let one_call = drive(&third, AdvanceGoal::Steps(30)).frames;
    let session_third = third.into_session().expect("third session");
    assert_eq!(
        one_call, frames_first,
        "three commands of ten cover thirty steps"
    );
    assert_eq!(
        digest(&session_first),
        digest(&session_third),
        "batching the same steps into three commands changed the game"
    );
}

/// UI-05: a person answering for a seat sees the numbers the policy would have sampled from.
#[test]
fn a_manual_offer_carries_the_policys_numbers() {
    let branch = start(&ALL_SEATS);
    branch
        .gate()
        .run(AdvanceGoal::Steps(1))
        .expect("one step is accepted");
    let pending = park(&branch);
    let probabilities: Vec<f64> = pending
        .options
        .iter()
        .map(|option| {
            option
                .probability
                .unwrap_or_else(|| panic!("option {} has no probability", option.id))
        })
        .collect();
    let total: f64 = probabilities.iter().sum();
    assert!(
        (total - 1.0).abs() < 1e-6,
        "the shares are a distribution over the offered options, got {total}"
    );
    assert!(
        pending.options.iter().all(|option| option.score.is_some()),
        "every option carries its score"
    );
}

/// Drive the real Ssruu copied-round timing callback through the same engine Table and manual
/// Gate used by LiveBranch. This is intentionally an event-callback acceptance test: the public
/// LiveBranch API starts from a checkpoint/map pool and cannot accept a caller-built GameState.
#[test]
fn a_manual_ssruu_round_copy_parks_with_target_labels_and_resumes_on_the_chosen_unit() {
    use ti4_engine::choice::{AlwaysDecline, Table};
    use ti4_engine::event::Event;
    use ti4_engine::fixtures;
    use ti4_content::ContentStore;
    use ti4_model::content_types::DEFAULT;
    use ti4_model::id::LeaderId;
    use ti4_model::state::LeaderStatus;
    use ti4_replayer::decider::ControlledDecider;
    use ti4_replayer::live::Gate;

    let borrower = PlayerId::new("a");
    let source = PlayerId::new("b");
    let target_owner = PlayerId::new("c");
    let (system, _) = fixtures::a_placed_planet();
    let mut state = fixtures::seated_game(
        &[("a", "yssaril"), ("b", "letnev"), ("c", "sol")],
        DEFAULT,
    );
    state.combat_round_seq = 4;
    state
        .player_mut(&borrower)
        .expect("Yssaril is seated")
        .leaders
        .insert(LeaderId::new("yssarilagent"), LeaderStatus::Readied);
    state
        .player_mut(&source)
        .expect("Letnev is seated")
        .leaders
        .insert(LeaderId::new("letnevagent"), LeaderStatus::Exhausted);
    fixtures::put(&mut state, &system, "cruiser", &target_owner, 2);

    let mut payload = BTreeMap::new();
    payload.insert("system".to_owned(), system.to_string().into());
    payload.insert("attacker".to_owned(), borrower.to_string().into());
    payload.insert("defender".to_owned(), target_owner.to_string().into());
    payload.insert("round_seq".to_owned(), 4_i64.into());
    let event = Event::new(1, "COMBAT_ROUND_STARTED", payload);

    let mut seat_control = SeatControl::all_auto();
    seat_control.set_mode(&borrower, SeatMode::Manual);
    let gate = std::sync::Arc::new(Gate::live(seat_control));
    let _shutdown_on_failure = ShutdownGateOnDrop(gate.clone());
    let callback_gate = gate.clone();
    let callback_borrower = borrower.clone();
    let callback = std::thread::spawn(move || {
        let mut resolver = fixtures::armed_resolver(&state);
        let mut table = Table::with_default(Box::new(AlwaysDecline));
        table.seat(
            callback_borrower.clone(),
            Box::new(ControlledDecider::new(
                Box::new(AlwaysDecline),
                callback_borrower,
                callback_gate,
            )),
        );
        let result = fixtures::with_context(&mut state, DEFAULT, None, &mut table, |context| {
            resolver.emit_with_context(context, event, |_, _| {})
        });
        (state, result)
    });

    let wait_for = |matches: &dyn Fn(&PendingManualChoice) -> bool| {
        let deadline = Instant::now() + WAIT;
        loop {
            if let Some(pending) = gate.pending().filter(matches) {
                return pending;
            }
            assert!(Instant::now() < deadline, "resolver did not park the expected offer");
            std::thread::sleep(Duration::from_millis(5));
        }
    };

    let ability_offer = wait_for(&|offer| {
        offer.actor == borrower
            && offer.options.iter().any(|option| {
                option.id.contains("leader:ssruu-copy:a:letnevagent:b:COMBAT_ROUND_STARTED")
            })
    });
    let copy_id = ability_offer
        .options
        .iter()
        .find(|option| {
            option.id.contains("leader:ssruu-copy:a:letnevagent:b:COMBAT_ROUND_STARTED")
        })
        .expect("actual resolver offered the copied Letnev round ability")
        .id
        .clone();
    assert!(matches!(
        gate.submit(&ManualSubmission::to(&ability_offer, copy_id)),
        SubmitOutcome::Accepted { .. }
    ));

    let target_offer = wait_for(&|offer| {
        offer.actor == borrower
            && offer.prompt.contains("choose one unit for +1 combat die")
    });
    assert_eq!(
        target_offer.context.as_ref().and_then(|context| context["subtype"].as_str()),
        Some("leader_ssruu_round_agent_unit")
    );
    assert_eq!(target_offer.options.len(), 2);
    assert!(target_offer
        .options
        .iter()
        .any(|option| option.label.contains("c's cruiser (unit 1)")));
    let selected = target_offer
        .options
        .iter()
        .find(|option| option.label.contains("c's cruiser (unit 2)"))
        .expect("the second eligible unit has its own readable choice")
        .id
        .clone();
    assert!(matches!(
        gate.submit(&ManualSubmission::to(&target_offer, selected)),
        SubmitOutcome::Accepted { .. }
    ));

    let (state, result) = callback.join().expect("timing callback worker completes");
    result.expect("the engine settles the submitted target through Table::ask");
    let content = ContentStore::embedded();
    assert_eq!(
        ti4_engine::factions::borrowed_round_agents::extra_die_for(
            &state,
            content,
            DEFAULT,
            &system,
            None,
            &target_owner,
            "cruiser",
            0,
            4,
        ),
        0,
        "the unselected first cruiser receives no copied die"
    );
    assert_eq!(
        ti4_engine::factions::borrowed_round_agents::extra_die_for(
            &state,
            content,
            DEFAULT,
            &system,
            None,
            &target_owner,
            "cruiser",
            1,
            4,
        ),
        1,
        "the human's selected second cruiser receives the copied die"
    );
    assert_eq!(
        state.player(&borrower).expect("borrower remains seated").leaders
            [&LeaderId::new("yssarilagent")],
        LeaderStatus::Exhausted
    );
    assert_eq!(
        state.player(&source).expect("source remains seated").leaders
            [&LeaderId::new("letnevagent")],
        LeaderStatus::Exhausted,
        "copying does not ready or exhaust the source again"
    );
}

/// The second genuine BF callback in scope: Ssruu borrows L1Z1X's agent on the active player's
/// activation and resumes the actual replacement on the planet selected from the manual offer.
#[test]
fn a_manual_ssruu_l1z1x_copy_preserves_planet_choice_and_replaces_that_planets_infantry() {
    use ti4_content::ContentStore;
    use ti4_engine::choice::{AlwaysDecline, Table};
    use ti4_engine::event::Event;
    use ti4_engine::fixtures;
    use ti4_model::content_types::DEFAULT;
    use ti4_model::id::{LeaderId, PlanetId, SystemId};
    use ti4_model::state::LeaderStatus;
    use ti4_replayer::decider::ControlledDecider;
    use ti4_replayer::live::Gate;

    let borrower = PlayerId::new("a");
    let source = PlayerId::new("b");
    let beneficiary = PlayerId::new("c");
    let content = ContentStore::embedded();
    let (system_record_id, system_record) =
        ti4_content::galaxy::all_systems(content, DEFAULT)
            .into_iter()
            .find(|(_, record)| record.planets().len() >= 2)
            .expect("content has a system with two planets");
    let system = SystemId::new(system_record_id);
    let first_planet = PlanetId::new(system_record.planets()[0]);
    let second_planet = PlanetId::new(system_record.planets()[1]);
    let sol_mech = ti4_content::units::faction_unit(content, "sol", "mech", DEFAULT)
        .expect("the Sol faction has a mech")
        .id()
        .to_owned();
    let mut state = fixtures::seated_game(
        &[("a", "yssaril"), ("b", "l1z1x"), ("c", "sol")],
        DEFAULT,
    );
    state.active = Some(beneficiary.clone());
    state.active_system = Some(system.clone());
    state
        .player_mut(&borrower)
        .expect("Yssaril is seated")
        .leaders
        .insert(LeaderId::new("yssarilagent"), LeaderStatus::Readied);
    state
        .player_mut(&source)
        .expect("L1Z1X is seated")
        .leaders
        .insert(LeaderId::new("l1z1xagent"), LeaderStatus::Exhausted);
    fixtures::put_on_planet(
        &mut state,
        &system,
        &first_planet,
        "infantry",
        &beneficiary,
        1,
    );
    fixtures::put_on_planet(
        &mut state,
        &system,
        &second_planet,
        "infantry",
        &beneficiary,
        1,
    );

    let mut payload = BTreeMap::new();
    payload.insert("player".to_owned(), beneficiary.to_string().into());
    payload.insert("system".to_owned(), system.to_string().into());
    let event = Event::new(1, "SYSTEM_ACTIVATED", payload);

    let mut seat_control = SeatControl::all_auto();
    seat_control.set_mode(&borrower, SeatMode::Manual);
    seat_control.set_mode(&beneficiary, SeatMode::Manual);
    let gate = std::sync::Arc::new(Gate::live(seat_control));
    let _shutdown_on_failure = ShutdownGateOnDrop(gate.clone());
    let callback_gate = gate.clone();
    let callback_borrower = borrower.clone();
    let callback_beneficiary = beneficiary.clone();
    let callback = std::thread::spawn(move || {
        let mut resolver = fixtures::armed_resolver(&state);
        let mut table = Table::with_default(Box::new(AlwaysDecline));
        table.seat(
            callback_borrower.clone(),
            Box::new(ControlledDecider::new(
                Box::new(AlwaysDecline),
                callback_borrower,
                callback_gate.clone(),
            )),
        );
        table.seat(
            callback_beneficiary.clone(),
            Box::new(ControlledDecider::new(
                Box::new(AlwaysDecline),
                callback_beneficiary,
                callback_gate,
            )),
        );
        let result = fixtures::with_context(&mut state, DEFAULT, None, &mut table, |context| {
            resolver.emit_with_context(context, event, |_, _| {})
        });
        (state, result)
    });

    let wait_for = |matches: &dyn Fn(&PendingManualChoice) -> bool| {
        let deadline = Instant::now() + WAIT;
        loop {
            if let Some(pending) = gate.pending().filter(matches) {
                return pending;
            }
            assert!(Instant::now() < deadline, "resolver did not park the expected offer");
            std::thread::sleep(Duration::from_millis(5));
        }
    };
    let ability_offer = wait_for(&|offer| {
        offer.actor == borrower
            && offer.options.iter().any(|option| {
                option.id.contains("leader:ssruu-copy:a:l1z1xagent:b:SYSTEM_ACTIVATED")
            })
    });
    let copy_id = ability_offer
        .options
        .iter()
        .find(|option| {
            option.id.contains("leader:ssruu-copy:a:l1z1xagent:b:SYSTEM_ACTIVATED")
        })
        .expect("actual resolver offered the copied L1Z1X activation ability")
        .id
        .clone();
    assert!(matches!(
        gate.submit(&ManualSubmission::to(&ability_offer, copy_id)),
        SubmitOutcome::Accepted { .. }
    ));

    let target_offer = wait_for(&|offer| {
        offer.actor == beneficiary
            && offer.prompt.contains("replace which infantry in the active system")
    });
    assert_eq!(
        target_offer.context.as_ref().and_then(|context| context["subtype"].as_str()),
        Some("leader_l1z1xagent_copy_planet")
    );
    assert_eq!(target_offer.options.len(), 2);
    assert!(target_offer.options.iter().all(|option| {
        option.label.starts_with("replace infantry on ") && option.label.ends_with(" with a mech")
    }));
    let chosen_planet_id = second_planet.to_string();
    let chosen = target_offer
        .options
        .iter()
        .find(|option| option.id == chosen_planet_id)
        .expect("the offer preserves the second planet ID")
        .id
        .clone();
    assert!(matches!(
        gate.submit(&ManualSubmission::to(&target_offer, chosen)),
        SubmitOutcome::Accepted { .. }
    ));

    let (state, result) = callback.join().expect("timing callback worker completes");
    result.expect("the engine settles the submitted planet through Table::ask");
    let board = state.board.get(&system).expect("callback system remains on board");
    let first_units = board.planet_units.get(&first_planet).expect("first planet units");
    let second_units = board.planet_units.get(&second_planet).expect("second planet units");
    assert!(first_units.iter().any(|unit| {
        unit.owner == beneficiary && unit.type_id.as_str() == "infantry"
    }));
    assert!(second_units.iter().any(|unit| {
        unit.owner == beneficiary && unit.type_id.as_str() == sol_mech
    }));
    assert_eq!(
        state.player(&borrower).expect("borrower remains seated").leaders
            [&LeaderId::new("yssarilagent")],
        LeaderStatus::Exhausted
    );
    assert_eq!(
        state.player(&source).expect("source remains seated").leaders
            [&LeaderId::new("l1z1xagent")],
        LeaderStatus::Exhausted,
        "copying leaves the source status unchanged"
    );
}

/// A direct ground-combat callback parks the defender's ground-casualty choice, then resumes
/// through the submitted damaged-mech option. This does not claim full invasion-window coverage.
#[test]
fn a_manual_ground_combat_casualty_submits_the_selected_damaged_mech() {
    use ti4_content::ContentStore;
    use ti4_engine::choice::{FirstOption, Scripted, Table};
    use ti4_engine::dice::Dice;
    use ti4_engine::fixtures;
    use ti4_engine::invasion;
    use ti4_engine::rng::GameRng;
    use ti4_model::content_types::DEFAULT;
    use ti4_model::id::{PlanetId, SystemId};
    use ti4_replayer::decider::ControlledDecider;
    use ti4_replayer::live::Gate;

    let invader = PlayerId::new("a");
    let defender = PlayerId::new("b");
    let content = ContentStore::embedded();
    let systems = ti4_content::galaxy::all_systems(content, DEFAULT);
    let (planet_id, system_id) = ti4_content::galaxy::all_planets(content, DEFAULT)
        .iter()
        .find_map(|(id, planet)| {
            let system = planet.system_id()?;
            (system != ti4_engine::seating::MECATOL && !planet.is_placed_during_play())
                .then(|| systems.get(system).filter(|record| !record.is_anomaly()))??;
            Some((PlanetId::new(*id), SystemId::new(system)))
        })
        .expect("the corpus has an ordinary non-Mecatol planet");

    let mut state = fixtures::seated_game(&[("a", "sol"), ("b", "hacan")], DEFAULT);
    let staging = ti4_engine::supply::staging_enabled(&state);
    state.board.remove(&system_id);
    fixtures::put_on_planet(&mut state, &system_id, &planet_id, "infantry", &invader, 1);
    fixtures::put_on_planet(
        &mut state,
        &system_id,
        &planet_id,
        "hacan_mech",
        &defender,
        1,
    );
    fixtures::put_on_planet(&mut state, &system_id, &planet_id, "infantry", &defender, 1);
    state
        .system_mut(&system_id)
        .planet_units
        .get_mut(&planet_id)
        .expect("defender units were placed")
        .iter_mut()
        .find(|unit| unit.type_id.as_str() == "hacan_mech")
        .expect("defender mech was placed")
        .sustained_damage = true;

    let mut seat_control = SeatControl::all_auto();
    seat_control.set_mode(&defender, SeatMode::Manual);
    let gate = std::sync::Arc::new(Gate::live(seat_control));
    let _shutdown_on_failure = ShutdownGateOnDrop(gate.clone());
    let callback_gate = gate.clone();
    let callback_defender = defender.clone();
    let callback_system = system_id.clone();
    let callback_planet = planet_id.clone();
    let callback_invader = invader.clone();
    let callback = std::thread::spawn(move || {
        let mut table = Table::with_default(Box::new(FirstOption));
        table.seat(
            callback_defender.clone(),
            Box::new(ControlledDecider::new(
                Box::new(Scripted::new(["destroy|1"])),
                callback_defender,
                callback_gate,
            )),
        );
        let mut dice = Dice::new();
        // Seed 43 gives attacker infantry 8, defender mech 1 and infantry 10.
        let mut rng = GameRng::new(43);
        let result = invasion::ground_combat(
            &mut state,
            ContentStore::embedded(),
            DEFAULT,
            &mut table,
            &mut dice,
            &mut rng,
            &callback_system,
            &callback_planet,
            &callback_invader,
        );
        (state, result)
    });

    let wait_for = |matches: &dyn Fn(&PendingManualChoice) -> bool| {
        let deadline = Instant::now() + WAIT;
        loop {
            if let Some(pending) = gate.pending().filter(matches) {
                return pending;
            }
            assert!(
                !callback.is_finished(),
                "ground-combat worker finished before the expected offer"
            );
            assert!(
                Instant::now() < deadline,
                "ground combat did not park the expected offer"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    };
    let casualty = wait_for(&|offer| {
        offer.actor == defender
            && offer
                .context
                .as_ref()
                .and_then(|context| context["subtype"].as_str())
                == Some("assign_ground_casualty")
            && offer
                .options
                .iter()
                .any(|option| option.label.contains("mech"))
            && offer
                .options
                .iter()
                .any(|option| option.label.contains("infantry"))
    });
    assert_eq!(
        casualty
            .context
            .as_ref()
            .and_then(|context| context["subtype"].as_str()),
        Some("assign_ground_casualty")
    );
    assert_eq!(
        casualty
            .context
            .as_ref()
            .and_then(|context| context["target"]["Planet"]["planet"].as_str()),
        Some(planet_id.as_str())
    );
    assert_eq!(
        casualty
            .context
            .as_ref()
            .and_then(|context| context["target"]["Planet"]["system"].as_str()),
        Some(system_id.as_str())
    );
    assert_eq!(casualty.options.len(), 2);
    let mech = casualty
        .options
        .iter()
        .find(|option| option.payload["unit"] == "hacan_mech")
        .expect("the damaged mech has a distinct manual option");
    let infantry = casualty
        .options
        .iter()
        .find(|option| option.payload["unit"] == "infantry")
        .expect("the infantry has a distinct manual option");
    assert_ne!(mech.label, infantry.label);
    assert_eq!(mech.kind, invasion::GROUND_CASUALTY_KIND);
    assert_eq!(infantry.kind, invasion::GROUND_CASUALTY_KIND);
    assert_eq!(mech.payload["damaged"], true);
    assert_eq!(infantry.payload["damaged"], false);
    if staging {
        assert_eq!(mech.label, "destroy hacan_mech (damaged)");
        assert_eq!(infantry.label, "destroy infantry (undamaged)");
    }
    assert!(
        casualty
            .options
            .iter()
            .any(|option| option.label.contains("infantry"))
    );
    let chosen = mech.id.clone();
    assert!(matches!(
        gate.submit(&ManualSubmission::to(&casualty, chosen)),
        SubmitOutcome::Accepted { .. }
    ));
    let _ = gate.set_mode(&defender, SeatMode::Auto);

    let (state, result) = callback.join().expect("ground-combat worker completes");
    assert_eq!(
        result.expect("ground combat settles the submitted casualty"),
        None,
        "the defender's hit removes the sole invading infantry"
    );
    let standing = state
        .system_state(&system_id)
        .on_planet(&planet_id)
        .to_vec();
    assert!(
        !standing
            .iter()
            .any(|unit| unit.owner == invader && unit.type_id.as_str() == "infantry")
    );
    assert!(
        standing
            .iter()
            .any(|unit| unit.owner == defender && unit.type_id.as_str() == "infantry")
    );
    assert!(
        !standing
            .iter()
            .any(|unit| unit.owner == defender && unit.type_id.as_str() == "hacan_mech")
    );
}

/// A real armed resolver routes Ssruu's copied Garv and Gunn callback to the ending player,
/// who chooses between two controlled planets before the selected planet is explored.
#[test]
fn a_manual_ssruu_copied_garv_and_gunn_explores_the_selected_planet() {
    use ti4_content::ContentStore;
    use ti4_engine::choice::{AlwaysDecline, Table};
    use ti4_engine::event::Event;
    use ti4_engine::fixtures;
    use ti4_model::content_types::{ContentType, DEFAULT};
    use ti4_model::id::{LeaderId, PlanetId, SystemId};
    use ti4_model::state::LeaderStatus;
    use ti4_replayer::decider::ControlledDecider;
    use ti4_replayer::live::Gate;

    let borrower = PlayerId::new("a");
    let source = PlayerId::new("b");
    let ending_player = PlayerId::new("c");
    let content = ContentStore::embedded();
    let systems = ti4_content::galaxy::all_systems(content, DEFAULT);
    let mut by_deck: BTreeMap<String, Vec<(PlanetId, SystemId, String)>> = BTreeMap::new();
    for (id, planet) in ti4_content::galaxy::all_planets(content, DEFAULT) {
        let Some(system) = planet.system_id() else {
            continue;
        };
        let Some(system_record) = systems.get(system) else {
            continue;
        };
        let Some(name) = planet.name() else {
            continue;
        };
        if planet.is_placed_during_play() || system_record.is_anomaly() {
            continue;
        }
        let traits = ti4_engine::exploration::traits_of(content, DEFAULT, &PlanetId::new(id));
        if traits.len() != 1 {
            continue;
        }
        let deck = traits.into_iter().next().expect("one planet trait");
        by_deck.entry(deck).or_default().push((
            PlanetId::new(id),
            SystemId::new(system),
            name.to_owned(),
        ));
    }
    let (deck, planets) = by_deck
        .into_iter()
        .find_map(|(deck, planets)| {
            let first = planets.first()?.clone();
            let second = planets
                .iter()
                .find(|candidate| candidate.2 != first.2)?
                .clone();
            Some((deck, [first, second]))
        })
        .expect("the corpus has two named ordinary planets sharing an exploration deck");
    let first_planet = planets[0].clone();
    let second_planet = planets[1].clone();
    let guaranteed_cards: Vec<String> = content
        .records(ContentType::Explores)
        .iter()
        .filter(|record| record.in_sources(DEFAULT))
        .filter(|record| record.text("resolution") == Some("Fragment"))
        .filter(|record| {
            record
                .text("type")
                .unwrap_or(deck.as_str())
                .eq_ignore_ascii_case(deck.as_str())
        })
        .filter_map(|record| record.id().map(ToOwned::to_owned))
        .take(2)
        .collect();
    assert_eq!(
        guaranteed_cards.len(),
        2,
        "the selected exploration deck has two benign fragment cards"
    );

    let mut state =
        fixtures::seated_game(&[("a", "yssaril"), ("b", "naaz"), ("c", "sol")], DEFAULT);
    for board in state.board.values_mut() {
        board
            .planet_control
            .retain(|_, owner| owner != &ending_player);
    }
    for (system, planet) in [
        (&first_planet.1, &first_planet.0),
        (&second_planet.1, &second_planet.0),
    ] {
        state
            .system_mut(system)
            .set_control(planet.clone(), ending_player.clone());
    }
    let deck_cards = state
        .exploration_decks
        .get_mut(&deck)
        .expect("the selected exploration deck exists");
    *deck_cards = guaranteed_cards;
    state
        .player_mut(&source)
        .expect("Naaz is seated")
        .leaders
        .insert(LeaderId::new("naazagent"), LeaderStatus::Exhausted);
    state
        .player_mut(&borrower)
        .expect("Yssaril is seated")
        .leaders
        .insert(LeaderId::new("yssarilagent"), LeaderStatus::Readied);
    let source_status = state.player(&source).unwrap().leaders[&LeaderId::new("naazagent")];
    let log_before = state.exploration_log.len();
    let ability_id = "leader:naaz:b:yssarilagent:naazagent:TURN_PASSED:after";

    let mut payload = BTreeMap::new();
    payload.insert("player".to_owned(), ending_player.to_string().into());
    let event = Event::new(1, "TURN_PASSED", payload);
    let mut seat_control = SeatControl::all_auto();
    seat_control.set_mode(&borrower, SeatMode::Manual);
    seat_control.set_mode(&ending_player, SeatMode::Manual);
    let gate = std::sync::Arc::new(Gate::live(seat_control));
    let _shutdown_on_failure = ShutdownGateOnDrop(gate.clone());
    let callback_gate = gate.clone();
    let callback_borrower = borrower.clone();
    let callback_ending_player = ending_player.clone();
    let callback = std::thread::spawn(move || {
        let mut resolver = fixtures::armed_resolver(&state);
        let mut table = Table::with_default(Box::new(AlwaysDecline));
        table.seat(
            callback_borrower.clone(),
            Box::new(ControlledDecider::new(
                Box::new(AlwaysDecline),
                callback_borrower,
                callback_gate.clone(),
            )),
        );
        table.seat(
            callback_ending_player.clone(),
            Box::new(ControlledDecider::new(
                Box::new(AlwaysDecline),
                callback_ending_player,
                callback_gate,
            )),
        );
        let result = fixtures::with_context(&mut state, DEFAULT, None, &mut table, |context| {
            resolver.emit_with_context(context, event, |_, _| {})
        });
        (state, result)
    });

    let wait_for = |matches: &dyn Fn(&PendingManualChoice) -> bool| {
        let deadline = Instant::now() + WAIT;
        loop {
            if let Some(pending) = gate.pending().filter(matches) {
                return pending;
            }
            assert!(
                !callback.is_finished(),
                "resolver worker finished before the expected offer"
            );
            assert!(
                Instant::now() < deadline,
                "resolver did not park the expected offer"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    };
    let ability_offer = wait_for(&|offer| {
        offer.actor == borrower && offer.options.iter().any(|option| option.id == ability_id)
    });
    assert!(matches!(
        gate.submit(&ManualSubmission::to(&ability_offer, ability_id)),
        SubmitOutcome::Accepted { .. }
    ));

    let planet_offer = wait_for(&|offer| {
        offer.actor == ending_player
            && offer
                .context
                .as_ref()
                .and_then(|context| context["subtype"].as_str())
                == Some("borrowed_agent_planet")
    });
    assert_eq!(
        planet_offer
            .context
            .as_ref()
            .and_then(|context| context["source"]["FactionAbility"].as_str()),
        Some("naazagent")
    );
    assert_eq!(planet_offer.options.len(), 2);
    for planet in [&first_planet, &second_planet] {
        let option = planet_offer
            .options
            .iter()
            .find(|option| option.id == planet.0.as_str())
            .expect("each controlled planet has an option");
        assert_eq!(option.kind, "planet");
        assert_eq!(option.label, format!("explore {}", planet.2));
    }
    assert_ne!(planet_offer.options[0].label, planet_offer.options[1].label);
    assert!(matches!(
        gate.submit(&ManualSubmission::to(
            &planet_offer,
            second_planet.0.as_str().to_owned()
        )),
        SubmitOutcome::Accepted { .. }
    ));
    let _ = gate.set_mode(&ending_player, SeatMode::Auto);

    let (state, result) = callback.join().expect("resolver callback worker completes");
    result.expect("copied Garv and Gunn settles through Table::ask");
    assert_eq!(state.exploration_log.len(), log_before + 1);
    let record = state
        .exploration_log
        .last()
        .expect("exploration was logged");
    assert_eq!(record.player, ending_player);
    assert_eq!(record.planet.as_ref(), Some(&second_planet.0));
    assert_eq!(record.deck, deck);
    assert!(
        state
            .exploration_log
            .iter()
            .skip(log_before)
            .all(|record| record.planet.as_ref() != Some(&first_planet.0))
    );
    assert_eq!(
        state.player(&borrower).unwrap().leaders[&LeaderId::new("yssarilagent")],
        LeaderStatus::Exhausted
    );
    assert_eq!(
        state.player(&source).unwrap().leaders[&LeaderId::new("naazagent")],
        source_status,
        "copying leaves the exhausted source agent unchanged"
    );
}

/// A real armed resolver parks both Absolute Synergy survivor choices, then retains the selected
/// damaged plain-form mech as the Eidolon Maximum.
#[test]
fn a_manual_naaz_absolute_synergy_keeps_the_selected_damaged_form() {
    use ti4_engine::choice::{AlwaysDecline, Table};
    use ti4_engine::event::Event;
    use ti4_engine::fixtures;
    use ti4_model::content_types::DEFAULT;
    use ti4_model::id::{BreakthroughId, LeaderId, PlanetId, SystemId, UnitTypeId};
    use ti4_model::state::LeaderStatus;
    use ti4_replayer::decider::ControlledDecider;
    use ti4_replayer::live::Gate;

    let player = PlayerId::new("a");
    let system = SystemId::new("18");
    let planet = PlanetId::new("mr");
    let mut state = fixtures::seated_game(&[("a", "naaz"), ("b", "sol")], DEFAULT);
    state.player_mut(&player).unwrap().breakthrough = Some(BreakthroughId::new("naazbt"));
    for board in state.board.values_mut() {
        board
            .units
            .retain(|unit| !unit.type_id.as_str().starts_with("naaz_mech"));
        for units in board.planet_units.values_mut() {
            units.retain(|unit| !unit.type_id.as_str().starts_with("naaz_mech"));
        }
    }
    fixtures::put_on_planet(&mut state, &system, &planet, "naaz_mech_space", &player, 1);
    fixtures::put(&mut state, &system, "naaz_mech_space", &player, 2);
    fixtures::put(&mut state, &system, "naaz_mech", &player, 1);
    state
        .system_mut(&system)
        .units
        .iter_mut()
        .find(|unit| unit.type_id.as_str() == "naaz_mech")
        .expect("the plain-form mech was placed")
        .sustained_damage = true;

    let ability_id = "breakthrough:naaz:naazbt:BREAKTHROUGH_GAINED:after";
    let mut payload = BTreeMap::new();
    payload.insert("player".to_owned(), player.to_string().into());
    payload.insert("breakthrough".to_owned(), "naazbt".into());
    let event = Event::new(1, "BREAKTHROUGH_GAINED", payload);
    let mut seat_control = SeatControl::all_auto();
    seat_control.set_mode(&player, SeatMode::Manual);
    let gate = std::sync::Arc::new(Gate::live(seat_control));
    let _shutdown_on_failure = ShutdownGateOnDrop(gate.clone());
    let callback_gate = gate.clone();
    let callback_player = player.clone();
    let callback = std::thread::spawn(move || {
        let mut resolver = fixtures::armed_resolver(&state);
        let mut table = Table::with_default(Box::new(AlwaysDecline));
        table.seat(
            callback_player.clone(),
            Box::new(ControlledDecider::new(
                Box::new(AlwaysDecline),
                callback_player,
                callback_gate,
            )),
        );
        let result = fixtures::with_context(&mut state, DEFAULT, None, &mut table, |context| {
            resolver.emit_with_context(context, event, |_, _| {})
        });
        (state, result)
    });

    let wait_for = |matches: &dyn Fn(&PendingManualChoice) -> bool| {
        let deadline = Instant::now() + WAIT;
        loop {
            if let Some(pending) = gate.pending().filter(matches) {
                return pending;
            }
            assert!(
                !callback.is_finished(),
                "resolver worker finished before the expected offer"
            );
            assert!(
                Instant::now() < deadline,
                "resolver did not park the expected offer"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    };
    let ability_offer = wait_for(&|offer| {
        offer.actor == player && offer.options.iter().any(|option| option.id == ability_id)
    });
    assert!(matches!(
        gate.submit(&ManualSubmission::to(&ability_offer, ability_id)),
        SubmitOutcome::Accepted { .. }
    ));

    let spot_offer = wait_for(&|offer| {
        offer.actor == player
            && offer
                .context
                .as_ref()
                .and_then(|context| context["subtype"].as_str())
                == Some("synergy_survivor")
    });
    assert_eq!(spot_offer.options.len(), 2);
    let space_spot = spot_offer
        .options
        .iter()
        .find(|option| option.id == "space")
        .expect("the space spot is offered");
    assert_eq!(space_spot.kind, "spot");
    assert_eq!(space_spot.label, "keep the mech in the space area");
    let planet_spot = spot_offer
        .options
        .iter()
        .find(|option| option.id == planet.as_str())
        .expect("the planet spot is offered");
    assert_eq!(planet_spot.kind, "spot");
    assert_eq!(planet_spot.label, format!("keep the mech on {planet}"));
    assert!(matches!(
        gate.submit(&ManualSubmission::to(&spot_offer, "space")),
        SubmitOutcome::Accepted { .. }
    ));

    let state_offer = wait_for(&|offer| {
        offer.actor == player
            && offer
                .context
                .as_ref()
                .and_then(|context| context["subtype"].as_str())
                == Some("synergy_survivor_state")
    });
    assert_eq!(state_offer.options.len(), 2);
    let chosen = "survivor|naaz_mech|damaged|plain";
    let damaged = state_offer
        .options
        .iter()
        .find(|option| option.id == chosen)
        .expect("the damaged plain-form survivor is offered");
    assert_eq!(damaged.kind, "survivor");
    assert_eq!(damaged.label, "keep the damaged ground-form Eidolon");
    let undamaged = state_offer
        .options
        .iter()
        .find(|option| option.id == "survivor|naaz_mech_space|undamaged|plain")
        .expect("the undamaged space-form survivor is offered");
    assert_eq!(undamaged.kind, "survivor");
    assert_eq!(undamaged.label, "keep the undamaged space-form Eidolon");
    assert!(matches!(
        gate.submit(&ManualSubmission::to(&state_offer, chosen)),
        SubmitOutcome::Accepted { .. }
    ));
    let _ = gate.set_mode(&player, SeatMode::Auto);

    let (state, result) = callback.join().expect("resolver callback worker completes");
    result.expect("Absolute Synergy settles through Table::ask");
    let maximum = state
        .system_state(&system)
        .units
        .iter()
        .find(|unit| unit.type_id.as_str() == "naaz_voltron")
        .cloned()
        .expect("the selected mech becomes the Eidolon Maximum");
    assert_eq!(maximum.type_id, UnitTypeId::new("naaz_voltron"));
    assert!(
        maximum.sustained_damage,
        "the selected damage state survives"
    );
    assert_eq!(
        state
            .system_state(&system)
            .units
            .iter()
            .filter(|unit| unit.owner == player && unit.type_id.as_str() == "naaz_voltron")
            .count(),
        1,
        "exactly one Eidolon Maximum remains"
    );
    assert_eq!(
        state.player(&player).unwrap().leaders[&LeaderId::new("naazagent")],
        LeaderStatus::Readied,
        "Absolute Synergy does not spend the Naaz agent"
    );
    assert_eq!(
        state
            .system_state(&system)
            .units
            .iter()
            .filter(|unit| unit.owner == player && unit.type_id.as_str().starts_with("naaz_mech"))
            .count(),
        0,
        "the other three mechs returned after both selections"
    );
    assert_eq!(
        state
            .system_state(&system)
            .planet_units
            .values()
            .flatten()
            .filter(|unit| unit.owner == player && unit.type_id.as_str().starts_with("naaz_mech"))
            .count(),
        0,
        "no Naaz mech remains on a planet after the flip"
    );
}

/// A real Game Action-phase Saar Deorbit action stages the event, then lets the selected Naaz
/// Maximum answer the effect-specific sustain ask. Strategy-phase setup remains out of scope.
#[test]
fn a_manual_saar_deorbit_offers_naaz_maximum_ground_effect_sustain() {
    use ti4_content::ContentStore;
    use ti4_engine::choice::{AlwaysDecline, Table};
    use ti4_engine::fixtures;
    use ti4_engine::game::Game;
    use ti4_model::content_types::DEFAULT;
    use ti4_model::id::{BreakthroughId, PlanetId, SystemId, UnitTypeId};
    use ti4_model::state::Phase;
    use ti4_model::units::Unit;
    use ti4_replayer::decider::ControlledDecider;
    use ti4_replayer::live::Gate;

    let saar = PlayerId::new("a");
    let naaz = PlayerId::new("b");
    let content = ContentStore::embedded();
    let field = ti4_content::galaxy::all_systems(content, ti4_model::content_types::POK)
        .iter()
        .find_map(|(id, tile)| tile.is_asteroid_field().then(|| (*id).to_owned()))
        .expect("the corpus has an asteroid-field system");
    let hub = fixtures::hub_with_centre(&field);
    let field = SystemId::new(field);
    let target_system = SystemId::new(hub.outer[0].clone());
    let tile = ti4_content::galaxy::system(content, target_system.as_str(), DEFAULT)
        .expect("the target system exists in the fixture galaxy");
    let planet = PlanetId::new(tile.planets()[0]);
    let mut state = fixtures::seated_game(&[("a", "saar"), ("b", "naaz")], DEFAULT);
    state.phase = Phase::Action;
    state.active = Some(saar.clone());
    state.rng_seed = 43;
    state.player_mut(&saar).unwrap().breakthrough = Some(BreakthroughId::new("saarbt"));
    state.player_mut(&saar).unwrap().trade_goods = 1;
    state
        .system_mut(&field)
        .units
        .push(Unit::new(UnitTypeId::new("cruiser"), saar.clone()));
    fixtures::put_on_planet(
        &mut state,
        &target_system,
        &planet,
        "naaz_voltron",
        &naaz,
        1,
    );

    let mut seat_control = SeatControl::all_auto();
    seat_control.set_mode(&saar, SeatMode::Manual);
    seat_control.set_mode(&naaz, SeatMode::Manual);
    let gate = std::sync::Arc::new(Gate::live(seat_control));
    let _shutdown_on_failure = ShutdownGateOnDrop(gate.clone());
    let callback_gate = gate.clone();
    let callback_saar = saar.clone();
    let callback_naaz = naaz.clone();
    let callback = std::thread::spawn(move || {
        let mut table = Table::with_default(Box::new(AlwaysDecline));
        table.seat(
            callback_saar.clone(),
            Box::new(ControlledDecider::new(
                Box::new(AlwaysDecline),
                callback_saar,
                callback_gate.clone(),
            )),
        );
        table.seat(
            callback_naaz.clone(),
            Box::new(ControlledDecider::new(
                Box::new(AlwaysDecline),
                callback_naaz,
                callback_gate,
            )),
        );
        let mut game = Game::with_table(state, ContentStore::embedded(), table)
            .with_sources(DEFAULT)
            .with_galaxy(hub.galaxy);
        let mut steps = Vec::new();
        for _ in 0..8 {
            let step = game.step();
            assert!(step.error.is_none(), "Game::step failed: {:?}", step.error);
            let sustained = game
                .timing
                .applied_events()
                .iter()
                .any(|event| event.event_type == "GROUND_FORCE_SUSTAINED");
            steps.push(step);
            if sustained {
                break;
            }
        }
        (
            game.state,
            game.table.log,
            game.timing.applied_events().to_vec(),
            steps,
        )
    });

    let wait_for = |matches: &dyn Fn(&PendingManualChoice) -> bool| {
        let deadline = Instant::now() + WAIT;
        loop {
            if let Some(pending) = gate.pending().filter(matches) {
                return pending;
            }
            assert!(
                !callback.is_finished(),
                "resolver worker finished before the expected offer"
            );
            assert!(
                Instant::now() < deadline,
                "resolver did not park the expected offer"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    };

    let action_offer = wait_for(&|offer| {
        offer.actor == saar
            && offer
                .options
                .iter()
                .any(|option| option.id == "faction|saar|deorbit_barrage")
    });
    assert!(matches!(
        gate.submit(&ManualSubmission::to(
            &action_offer,
            "faction|saar|deorbit_barrage"
        )),
        SubmitOutcome::Accepted { .. }
    ));
    let spend_offer = wait_for(&|offer| {
        offer.actor == saar
            && offer.options.iter().any(|option| option.id == "1")
            && offer
                .context
                .as_ref()
                .and_then(|context| context["subtype"].as_str())
                == Some("deorbit_spend")
    });
    assert!(matches!(
        gate.submit(&ManualSubmission::to(&spend_offer, "1")),
        SubmitOutcome::Accepted { .. }
    ));
    let target_id = format!("{target_system}|{planet}");
    let target_offer = wait_for(&|offer| {
        offer.actor == saar
            && offer.options.iter().any(|option| option.id == target_id)
            && offer
                .context
                .as_ref()
                .and_then(|context| context["subtype"].as_str())
                == Some("deorbit_target")
    });
    assert!(matches!(
        gate.submit(&ManualSubmission::to(&target_offer, target_id.clone())),
        SubmitOutcome::Accepted { .. }
    ));
    let payment_offer = wait_for(&|offer| {
        offer.actor == saar && offer.options.iter().any(|option| option.id == "trade_good")
    });
    assert!(matches!(
        gate.submit(&ManualSubmission::to(&payment_offer, "trade_good")),
        SubmitOutcome::Accepted { .. }
    ));

    let deorbit_offer = wait_for(&|offer| {
        offer.actor == saar
            && offer
                .context
                .as_ref()
                .and_then(|context| context["subtype"].as_str())
                == Some("deorbit_hit")
    });
    assert_eq!(deorbit_offer.options.len(), 1);
    let deorbit = &deorbit_offer.options[0];
    assert_eq!(deorbit.kind, "deorbit_hit");
    assert_eq!(deorbit.id, "0");
    assert_eq!(deorbit.label, "assign hit to b's naaz_voltron");
    assert!(matches!(
        gate.submit(&ManualSubmission::to(&deorbit_offer, "0")),
        SubmitOutcome::Accepted { .. }
    ));
    let _ = gate.set_mode(&saar, SeatMode::Auto);

    let sustain_offer = wait_for(&|offer| {
        offer.actor == naaz
            && offer
                .context
                .as_ref()
                .and_then(|context| context["subtype"].as_str())
                == Some("ground_effect_sustain")
    });
    assert_eq!(sustain_offer.options.len(), 2);
    assert_eq!(sustain_offer.options[0].id, "sustain");
    assert_eq!(sustain_offer.options[0].kind, "ground_effect_sustain");
    assert_eq!(sustain_offer.options[0].label, "use SUSTAIN DAMAGE");
    assert_eq!(sustain_offer.options[1].id, "decline");
    assert_eq!(
        sustain_offer
            .context
            .as_ref()
            .and_then(|context| context["target"]["Planet"]["system"].as_str()),
        Some(target_system.as_str())
    );
    assert_eq!(
        sustain_offer
            .context
            .as_ref()
            .and_then(|context| context["target"]["Planet"]["planet"].as_str()),
        Some(planet.as_str())
    );
    assert!(matches!(
        gate.submit(&ManualSubmission::to(&sustain_offer, "sustain")),
        SubmitOutcome::Accepted { .. }
    ));
    let _ = gate.set_mode(&naaz, SeatMode::Auto);

    let (state, log, events, steps) = callback.join().expect("Game worker completes");
    assert!(!steps.is_empty(), "Game::step made no progress");
    assert!(
        steps.iter().all(|step| step.error.is_none()),
        "Game::step returned an error: {steps:?}"
    );
    assert_eq!(state.player(&saar).unwrap().trade_goods, 0);
    assert!(
        state
            .faction_marks
            .contains_key("saar:deorbit_barrage:exhausted:a")
    );
    assert!(log.records.iter().any(|record| {
        record.chosen == "sustain"
            && record
                .context
                .as_ref()
                .is_some_and(|context| context.subtype == "ground_effect_sustain")
    }));
    assert!(
        events
            .iter()
            .any(|event| event.event_type == "GROUND_FORCE_SUSTAINED")
    );
    assert!(
        !events
            .iter()
            .any(|event| event.event_type == "GROUND_FORCE_DESTROYED")
    );
    let maximum = state
        .system_state(&target_system)
        .on_planet(&planet)
        .into_iter()
        .find(|unit| unit.owner == naaz && unit.type_id.as_str() == "naaz_voltron")
        .cloned()
        .expect("the sustained Naaz Maximum remains on the planet");
    assert!(
        maximum.sustained_damage,
        "SUSTAIN DAMAGE marks the Maximum damaged"
    );
    assert_eq!(
        state.system_state(&target_system).on_planet(&planet).len(),
        1,
        "the selected effect hit did not destroy the sole ground unit"
    );
}

/// A failed assertion must release a callback parked in a manual ask before the test exits.
struct ShutdownGateOnDrop(std::sync::Arc<ti4_replayer::live::Gate>);

impl Drop for ShutdownGateOnDrop {
    fn drop(&mut self) {
        self.0.shutdown();
    }
}
