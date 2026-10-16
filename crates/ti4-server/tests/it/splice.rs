//! H7 phase 1: the read-only splice dry run.
//!
//! Histories are produced by playing real engine games with a reproducible pseudo-random
//! decider, so the tests exercise the actual question stream rather than hand-built records.

use ti4_content::ContentStore;
use ti4_content::galaxy::Galaxy;
use ti4_engine::choice::{Choice, ChoiceOption, Decider, DecisionRecord, IllegalChoice, Table};
use ti4_engine::game::Game;
use ti4_host::history::splice::{SpliceBaseline, SpliceError, splice_preview};
use ti4_host::protocol::splice::{AlignmentChange, ConflictKind, RngStatus, SpliceEdit};
use ti4_model::id::PlayerId;
use ti4_model::state::GameState;

/// A tiny LCG picks options, so histories are varied but reproducible. The aggressive flavour
/// marches fleets at each other, which is what makes dice.
struct Lcg(u64, bool);

impl Decider for Lcg {
    fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let roll = usize::try_from(self.0 >> 33).unwrap();
        if self.1 {
            let pushy: Vec<usize> = choice
                .options
                .iter()
                .enumerate()
                .filter(|(_, o)| {
                    o.id.starts_with("move|") || o.id == "tactical" || o.id.starts_with("build|")
                })
                .map(|(i, _)| i)
                .collect();
            if !pushy.is_empty() && roll % 10 < 8 {
                return Ok(choice.options[pushy[roll / 10 % pushy.len()]].clone());
            }
        }
        Ok(choice.options[roll % choice.options.len()].clone())
    }
}

/// Plays a fixed list of answers, then the first option.
struct Replaying {
    script: Vec<String>,
    at: usize,
}

impl Decider for Replaying {
    fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
        let wanted = self.script.get(self.at).cloned();
        self.at += 1;
        Ok(wanted
            .and_then(|w| choice.option(&w).cloned())
            .unwrap_or_else(|| choice.options[0].clone()))
    }
}

fn setup(seed: u64) -> (GameState, Galaxy) {
    let players = vec![
        PlayerId::new("p1"),
        PlayerId::new("p2"),
        PlayerId::new("p3"),
    ];
    let (mut state, galaxy) =
        ti4_host::map::create_game_with_template(ContentStore::embedded(), &players, seed, None)
            .unwrap();
    // These fixtures name decisions by index in games tuned against the notes a server game held
    // before notes were dealt after seating: four factionless notes in the last seat's hand.
    // Keep that opening so the indices still mean what they were chosen for.
    let last = players.last().unwrap().clone();
    state.promissory_notes = ti4_engine::promissory::GENERIC
        .iter()
        .map(|alias| {
            (
                ti4_engine::promissory::note_id(alias, "generic"),
                last.clone(),
            )
        })
        .collect();
    (state, galaxy)
}

fn play_with(
    state: &GameState,
    galaxy: &Galaxy,
    decider: Box<dyn Decider>,
    max: usize,
) -> (Vec<DecisionRecord>, Option<usize>) {
    let table = Table::with_default(decider);
    let mut game = Game::with_table(state.clone(), ContentStore::embedded(), table)
        .with_galaxy(galaxy.clone());
    let mut first_roll = None;
    while game.table.log.records.len() < max {
        let (logged, rolls) = (game.table.log.records.len(), game.dice().count());
        let result = game.step();
        if first_roll.is_none() && game.dice().count() > rolls {
            first_roll = Some(logged);
        }
        if result.error.is_some() || result.finished {
            break;
        }
    }
    (game.table.log.records, first_roll)
}

fn base_history() -> (GameState, Galaxy, Vec<DecisionRecord>) {
    let (state, galaxy) = setup(7);
    let (mut h, _) = play_with(&state, &galaxy, Box::new(Lcg(1, false)), 400);
    // One step can log several decisions, so the game may run a little past 400; the history
    // is the first 400 either way.
    assert!(
        h.len() >= 400,
        "the fixture game should reach 400 decisions"
    );
    h.truncate(400);
    (state, galaxy, h)
}

fn replace(cursor: usize, option_id: &str) -> SpliceEdit {
    SpliceEdit::Replace {
        cursor,
        option_id: option_id.to_owned(),
    }
}

// ---- determinism --------------------------------------------------------------------------

#[test]
fn slow_the_same_input_twice_gives_an_identical_report() {
    let (state, galaxy, h) = base_history();
    for edit in [
        SpliceEdit::Remove { cursor: 44 },
        replace(40, "36"),
        replace(54, "1|6|1"),
    ] {
        let a = splice_preview(&state, Some(&galaxy), &h, &edit).unwrap();
        let b = splice_preview(&state, Some(&galaxy), &h, &edit).unwrap();
        assert_eq!(a, b);
        let base = SpliceBaseline::new(&state, Some(&galaxy), &h);
        assert_eq!(
            base.preview(&edit).unwrap(),
            a,
            "a shared baseline changes nothing"
        );
        assert_eq!(
            serde_json::to_string(&a).unwrap(),
            serde_json::to_string(&b).unwrap()
        );
    }
}

#[test]
fn slow_a_no_op_edit_survives_to_the_end_and_is_rng_neutral() {
    let (state, galaxy, h) = base_history();
    let base = SpliceBaseline::new(&state, Some(&galaxy), &h);
    assert_eq!(
        base.replayed(),
        h.len(),
        "the baseline replays the whole history"
    );
    for k in [0, 7, 26, 44, 71, 100, 250, 399] {
        let p = base.preview(&replace(k, &h[k].chosen)).unwrap();
        assert!(p.survives_to_end, "k={k}: {:?}", p.first_conflict);
        assert_eq!(p.kept_later, p.later_decisions);
        assert_eq!(p.dropped_later, 0);
        assert!(p.first_conflict.is_none() && p.alignment.is_empty());
        assert!(p.kept_reordered.is_empty());
        assert_ne!(p.rng.status, RngStatus::NotNeutral, "k={k}: {:?}", p.rng);
    }
}

#[test]
fn a_replacement_that_changes_nothing_downstream_keeps_every_later_decision() {
    // Which command-token pool a token went to does not change any later question.
    let (state, galaxy, h) = base_history();
    assert_eq!(h[44].chosen, "tactic_tokens");
    let p = splice_preview(&state, Some(&galaxy), &h, &replace(44, "fleet_tokens")).unwrap();
    assert!(p.survives_to_end);
    assert_eq!(p.kept_later, 355);
    assert_eq!(p.rng.status, RngStatus::Neutral);
    assert!(p.rng.compared > 0);
}

// ---- conflicts ----------------------------------------------------------------------------

#[test]
fn removing_a_decision_the_engine_still_asks_makes_it_ask_the_removed_question_again() {
    let (state, galaxy, h) = base_history();
    // Decision 7 ("activate a system"): without it the engine asks that very question again.
    let p = splice_preview(&state, Some(&galaxy), &h, &SpliceEdit::Remove { cursor: 7 }).unwrap();
    let c = p.first_conflict.expect("a conflict");
    assert_eq!(c.cursor, 8);
    assert_eq!(c.kind, ConflictKind::Prompt);
    assert!(!c.soft);
    assert!(c.asks_removed_decision);
    assert_eq!(c.found_prompt.as_deref(), Some("activate a system"));
    assert_eq!(c.prompt, "movement");
    assert_eq!(c.expected_chosen, "done_moving");
    assert_eq!((p.kept_later, p.dropped_later), (0, p.later_decisions));
    assert!(!p.survives_to_end);
}

#[test]
fn a_different_seat_asked_is_an_actor_conflict() {
    let (state, galaxy, h) = base_history();
    let p = splice_preview(&state, Some(&galaxy), &h, &SpliceEdit::Remove { cursor: 0 }).unwrap();
    let c = p.first_conflict.unwrap();
    assert_eq!((c.cursor, c.kind), (1, ConflictKind::Actor));
    assert_eq!(c.seat, "p2");
    assert_eq!(c.found_seat.as_deref(), Some("p1"));
}

#[test]
fn a_changed_menu_that_still_contains_the_answer_is_a_soft_conflict_with_the_difference() {
    let (state, galaxy, h) = base_history();
    // A different strategy card in the draft changes what the next pickers are offered.
    let p = splice_preview(&state, Some(&galaxy), &h, &replace(0, "pok1leadership")).unwrap();
    let c = p.first_conflict.unwrap();
    assert_eq!(c.kind, ConflictKind::OptionsChanged);
    assert!(c.soft);
    assert!(!c.added.is_empty() || !c.removed.is_empty());
    assert!(c.expected_offered.contains(&c.expected_chosen));
}

#[test]
fn slow_an_answer_that_is_no_longer_offered_is_a_hard_conflict() {
    let (state, galaxy, h) = base_history();
    let base = SpliceBaseline::new(&state, Some(&galaxy), &h);
    // Researching a different technology can strand a later pick.
    let found = h[..60]
        .iter()
        .enumerate()
        .filter(|(_, r)| r.prompt == "research a technology")
        .flat_map(|(k, r)| r.offered.iter().map(move |o| (k, o.clone())))
        .filter_map(|(k, o)| base.preview(&replace(k, &o)).ok())
        .find(|p| {
            p.first_conflict
                .as_ref()
                .is_some_and(|c| c.kind == ConflictKind::ChosenNotOffered)
        });
    let c = found
        .expect("some tech swap strands a later pick")
        .first_conflict
        .unwrap();
    assert!(!c.soft);
    assert!(!c.expected_offered.is_empty());
    assert!(
        c.found_offered
            .unwrap()
            .iter()
            .all(|o| *o != c.expected_chosen)
    );
}

#[test]
fn bad_edits_are_refused_before_any_replay() {
    let (state, galaxy, h) = base_history();
    assert_eq!(
        splice_preview(
            &state,
            Some(&galaxy),
            &h,
            &SpliceEdit::Remove { cursor: 400 }
        )
        .unwrap_err(),
        SpliceError::CursorOutOfRange {
            cursor: 400,
            len: 400
        }
    );
    assert!(matches!(
        splice_preview(&state, Some(&galaxy), &h, &replace(0, "not-an-option")).unwrap_err(),
        SpliceError::OptionNotOffered { cursor: 0, .. }
    ));
}

// ---- cursor alignment (single-option questions are never logged) ----------------------------

#[test]
fn a_single_option_question_that_appears_is_reported() {
    let (state, galaxy, h) = base_history();
    // Deploying the mech adds a one-option payment question that was never asked before.
    assert_eq!(h[96].prompt, "Orbital Drop: deploy a mech on jord");
    let p = splice_preview(&state, Some(&galaxy), &h, &replace(96, "deploy|sol_mech|1")).unwrap();
    let a = p
        .alignment
        .iter()
        .find(|a| a.change == AlignmentChange::Appeared)
        .expect("an auto-resolved question appears");
    assert_eq!(a.cursor, 97);
    assert_eq!(a.prompt, "pay 3 more resources");
    assert_eq!(a.option_id, "exhaust|jord");
    // The log never held it, so the later decisions all still fit.
    assert!(p.survives_to_end);
}

#[test]
fn a_single_option_question_that_vanishes_is_reported() {
    // The same game recorded WITH the deployment, then edited back to declining: the payment
    // question is in the original run and not in the edited one.
    let (state, galaxy, h) = base_history();
    let mut script: Vec<String> = h.iter().map(|r| r.chosen.clone()).collect();
    script[96] = "deploy|sol_mech|1".to_owned();
    let (with_mech, _) = play_with(&state, &galaxy, Box::new(Replaying { script, at: 0 }), 400);
    assert_eq!(with_mech[96].chosen, "deploy|sol_mech|1");
    let p = splice_preview(&state, Some(&galaxy), &with_mech, &replace(96, "decline")).unwrap();
    let v = p
        .alignment
        .iter()
        .find(|a| a.change == AlignmentChange::Vanished)
        .expect("the auto-resolved question vanishes");
    assert_eq!(v.prompt, "pay 3 more resources");
}

// ---- randomness ---------------------------------------------------------------------------

#[test]
fn removing_a_decision_that_drew_cards_is_flagged_not_neutral() {
    let (state, galaxy, h) = base_history();
    assert_eq!(
        h[71].prompt,
        "spend a strategy token to draw two action cards"
    );
    let p = splice_preview(
        &state,
        Some(&galaxy),
        &h,
        &SpliceEdit::Remove { cursor: 71 },
    )
    .unwrap();
    assert_eq!(p.rng.status, RngStatus::NotNeutral);
    let consumed = p.rng.removed_consumed.unwrap();
    assert_eq!(consumed.decks.get("action_card"), Some(&-2), "{consumed:?}");
    // A secret-objective draw too.
    let p = splice_preview(
        &state,
        Some(&galaxy),
        &h,
        &SpliceEdit::Remove { cursor: 79 },
    )
    .unwrap();
    assert_eq!(p.rng.status, RngStatus::NotNeutral);
    assert_eq!(
        p.rng.removed_consumed.unwrap().decks.get("secret"),
        Some(&-1)
    );
}

#[test]
fn replacing_a_decline_with_a_draw_is_caught_without_a_removal() {
    let (state, galaxy, h) = base_history();
    // Politics' secondary, declined: the history is found by what it asks, not by its index.
    let k = h
        .iter()
        .position(|r| {
            r.prompt
                .starts_with("spend a strategy token to draw two action cards")
                && r.chosen == "no"
                && r.offered.iter().any(|o| o == "yes")
        })
        .expect("the fixture game declines Politics' secondary draw");
    let p = splice_preview(&state, Some(&galaxy), &h, &replace(k, "yes")).unwrap();
    assert_eq!(p.rng.status, RngStatus::NotNeutral);
    assert_eq!(p.rng.first_divergence_cursor, Some(k));
    assert_eq!(
        p.rng.first_divergence.unwrap().decks.get("action_card"),
        Some(&-2)
    );
}

#[test]
fn a_neutral_segment_is_reported_neutral() {
    let (state, galaxy, h) = base_history();
    // A token-pool choice draws nothing and rolls nothing.
    let p = splice_preview(
        &state,
        Some(&galaxy),
        &h,
        &SpliceEdit::Remove { cursor: 44 },
    )
    .unwrap();
    let consumed = p.rng.removed_consumed.clone().unwrap();
    assert!(consumed.is_zero(), "{consumed:?}");
    assert_eq!(p.rng.status, RngStatus::Neutral);
}

#[test]
fn slow_dice_rolled_around_a_decision_are_counted() {
    // The aggressive policy reaches a roll on seed 2; a removal at or just before the step
    // that rolled must report consumed dice. The budget is generous so that a shift in the
    // seeded game (e.g. a card no longer offered) does not push the first roll out of reach.
    let (state, galaxy) = setup(2);
    let (h, first_roll) = play_with(&state, &galaxy, Box::new(Lcg(2, true)), 900);
    let k = first_roll.expect("the fixture game rolls dice");
    let base = SpliceBaseline::new(&state, Some(&galaxy), &h);
    let flagged = (k.saturating_sub(3)..(k + 2).min(h.len()))
        .map(|c| base.preview(&SpliceEdit::Remove { cursor: c }).unwrap())
        .any(|p| {
            p.rng.status == RngStatus::NotNeutral
                && p.rng
                    .removed_consumed
                    .as_ref()
                    .is_some_and(|c| c.dice_faces != 0)
        });
    assert!(flagged, "a removal around cursor {k} should consume dice");
}

// ---- the dry run changes nothing ------------------------------------------------------------

fn tree(path: &std::path::Path) -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    let mut stack = vec![path.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let p = entry.unwrap().path();
            if p.is_dir() {
                stack.push(p);
            } else {
                out.push((p.display().to_string(), std::fs::read(&p).unwrap()));
            }
        }
    }
    out.sort();
    out
}

#[test]
fn the_dry_run_leaves_the_game_its_history_and_storage_untouched() {
    use std::sync::Arc;
    use std::time::{Duration, Instant};
    use ti4_server::session::registry::HistoryError;
    use ti4_server::session::{GameRegistry, SeatController, SessionConfig};
    use ti4_server::storage::FileGameStore;

    let path = std::env::temp_dir().join(format!("ti4_splice_{:032x}", rand::random::<u128>()));
    let store = Arc::new(FileGameStore::new(&path).unwrap());
    let registry = GameRegistry::new().with_store(store.clone());
    let host = PlayerId::new("p1");
    let guest = PlayerId::new("p2");
    let players = vec![host.clone(), guest.clone()];
    let (state, galaxy) =
        ti4_host::map::create_game_with_template(ContentStore::embedded(), &players, 42, None)
            .unwrap();
    let tiles = ti4_host::map::build_board_tiles(ContentStore::embedded(), &galaxy);
    let config = SessionConfig::new("splice_probe", state)
        .with_seed(42)
        .with_player_ids(players)
        .with_galaxy(galaxy, tiles)
        .with_seat(host.clone(), SeatController::Human)
        .with_seat(guest.clone(), SeatController::Human);
    let session = registry.create_game(config).unwrap();
    let host_token = session.seat_tokens()[&host].clone();
    let guest_token = session.seat_tokens()[&guest].clone();
    let wait_pending = || {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(p) = session.current_pending_decision() {
                return p;
            }
            assert!(Instant::now() < deadline, "timed out waiting for a choice");
            std::thread::sleep(Duration::from_millis(5));
        }
    };
    for _ in 0..4 {
        let (seat, nonce, version) = wait_pending();
        let option = session
            .get_snapshot(&ti4_host::protocol::status::ViewerRole::Player(
                seat.clone(),
            ))
            .pending_choice
            .unwrap()
            .choice
            .options[0]
            .id
            .clone();
        session
            .submit_choice(&seat, &nonce, version, &option)
            .unwrap();
    }
    let _ = wait_pending();
    let log = session.decision_log();
    assert!(log.len() >= 4);
    let (version, current, redo) = (
        session.game_version(),
        session.current_state(),
        session.redo_decisions(),
    );
    let files = tree(&path);

    let edit = SpliceEdit::Remove { cursor: 1 };
    let preview = registry
        .splice_preview("splice_probe", &host_token, &edit)
        .unwrap();
    assert_eq!(preview.original_decisions, log.len());
    let again = registry
        .splice_preview("splice_probe", &host_token, &edit)
        .unwrap();
    assert_eq!(preview, again);

    assert_eq!(session.decision_log(), log);
    assert_eq!(session.redo_decisions(), redo);
    assert_eq!(session.game_version(), version);
    assert_eq!(session.current_state(), current);
    assert_eq!(tree(&path), files, "nothing was written to storage");
    assert!(session.history_ready());

    // Host only.
    let denied = registry
        .splice_preview("splice_probe", &guest_token, &edit)
        .unwrap_err();
    assert!(matches!(denied, HistoryError::Forbidden(_)), "{denied:?}");
    let missing = registry
        .splice_preview("no_such_game", &host_token, &edit)
        .unwrap_err();
    assert!(matches!(missing, HistoryError::NotFound), "{missing:?}");
    let out_of_range = registry
        .splice_preview(
            "splice_probe",
            &host_token,
            &SpliceEdit::Remove { cursor: 99 },
        )
        .unwrap_err();
    assert!(
        matches!(out_of_range, HistoryError::InvalidTarget(_)),
        "{out_of_range:?}"
    );
    assert_eq!(tree(&path), files);
    session.stop();
    let _ = std::fs::remove_dir_all(&path);
}
