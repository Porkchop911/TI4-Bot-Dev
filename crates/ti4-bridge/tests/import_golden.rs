//! M11-010 — the importer against real six-player telemetry.
//!
//! `tests/golden/telemetry/*.json` are whole uploads from live games, carried over from the
//! historical repository's fixtures at its pinned commit. The board captures alone could not test
//! this: the importer reads seating, factions, scores, tokens and technologies from the payload
//! around the board, and those only exist in the full upload.
//!
//! What is asserted is deliberately not "the state equals X". The table is the authority and the
//! fixtures are ordinary mid-game positions, so the tests check *invariants an import must hold* —
//! every seat resolved, every seat has pieces on the board, control is attributed to somebody who
//! is playing, and every unobservable thing is reported as a gap rather than filled in.

use std::collections::BTreeMap;
use std::path::PathBuf;

use ti4_bridge::import::{Gap, Telemetry, import, seats_with_nothing};
use ti4_model::content_types::FULL;

fn telemetry() -> BTreeMap<String, Telemetry> {
    let directory: PathBuf = [env!("CARGO_MANIFEST_DIR"), "tests", "golden", "telemetry"]
        .iter()
        .collect();
    let mut out = BTreeMap::new();
    for entry in std::fs::read_dir(&directory)
        .unwrap_or_else(|error| panic!("reading {}: {error}", directory.display()))
    {
        let path = entry.expect("a directory entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .expect("a file name")
            .to_owned();
        let text = std::fs::read_to_string(&path).expect("readable");
        let parsed: Telemetry = serde_json::from_str(&text)
            .unwrap_or_else(|error| panic!("{name} is not telemetry this bridge reads: {error}"));
        out.insert(name, parsed);
    }
    assert!(!out.is_empty(), "no telemetry fixtures found");
    out
}

#[test]
fn every_capture_imports_with_all_six_seats_resolved() {
    let store = ti4_content::ContentStore::embedded();
    for (name, payload) in telemetry() {
        let imported = import(store, &payload, FULL)
            .unwrap_or_else(|error| panic!("{name} did not import: {error}"));

        assert_eq!(
            imported.state.players.len(),
            payload.players.len(),
            "{name} lost a seat"
        );
        let unresolved: Vec<&Gap> = imported
            .gaps
            .iter()
            .filter(|gap| matches!(gap, Gap::UnknownFaction { .. }))
            .collect();
        assert!(
            unresolved.is_empty(),
            "{name}: the mod drops the leading article, and these did not match: {unresolved:?}"
        );
        assert_eq!(
            imported.seats.len(),
            payload.players.len(),
            "{name} did not map every colour to a seat"
        );
    }
}

#[test]
fn every_imported_seat_keeps_its_resolved_faction_identity() {
    // `start_game` creates generic seats. The imported player id already is the resolved faction
    // alias, but transaction partners and every faction ability read `Player::faction`, not the
    // id. Leaving it generic makes all six players one faction and can turn a transaction into a
    // repeatable no-op with oneself.
    let store = ti4_content::ContentStore::embedded();
    for (name, payload) in telemetry() {
        let imported = import(store, &payload, FULL).expect("imports");
        for player in &imported.state.players {
            assert_eq!(
                player.faction.as_str(),
                player.id.as_str(),
                "{name}: {} retained the setup placeholder faction",
                player.id
            );
        }
    }
}

#[test]
fn every_seat_has_pieces_somewhere_on_the_board() {
    // The sharpest single check on the whole chain. A seat with nothing anywhere means the
    // summary decoded, the tiles placed, and the colours still failed to reach the board -- which
    // is exactly what the tile zero-padding bug did before it was found.
    let store = ti4_content::ContentStore::embedded();
    for (name, payload) in telemetry() {
        let imported = import(store, &payload, FULL).expect("imports");
        let empty = seats_with_nothing(&imported.state);
        assert!(
            empty.is_empty(),
            "{name}: {empty:?} hold nothing anywhere on the board"
        );
    }
}

#[test]
fn scores_tokens_and_technologies_come_across() {
    let store = ti4_content::ContentStore::embedded();
    for (name, payload) in telemetry() {
        let imported = import(store, &payload, FULL).expect("imports");
        for (index, reported) in payload.players.iter().enumerate() {
            let seat = &imported.state.players[index];
            assert_eq!(seat.victory_points, reported.score, "{name} score");
            assert_eq!(seat.trade_goods, reported.trade_goods, "{name} trade goods");
            assert_eq!(seat.commodities, reported.commodities, "{name} commodities");
            assert_eq!(
                seat.tactic_tokens, reported.command_tokens.tactics,
                "{name} tactic tokens"
            );
            assert_eq!(
                seat.fleet_tokens, reported.command_tokens.fleet,
                "{name} fleet tokens"
            );
            assert_eq!(
                seat.strategic_tokens, reported.command_tokens.strategy,
                "{name} strategic tokens"
            );
            if !reported.technologies.is_empty() {
                assert_eq!(
                    seat.technologies.len(),
                    reported.technologies.len(),
                    "{name}: {} technologies reported, {} imported",
                    reported.technologies.len(),
                    seat.technologies.len()
                );
            }
        }
    }
}

#[test]
fn the_round_speaker_and_active_seat_come_across() {
    let store = ti4_content::ContentStore::embedded();
    for (name, payload) in telemetry() {
        let imported = import(store, &payload, FULL).expect("imports");
        assert_eq!(imported.state.round, payload.round.max(1), "{name} round");

        let speaker_index = payload
            .players
            .iter()
            .position(|player| player.color == payload.speaker);
        if let Some(index) = speaker_index {
            assert_eq!(
                imported.state.speaker, imported.state.players[index].id,
                "{name} speaker"
            );
        }
        let turn_index = payload
            .players
            .iter()
            .position(|player| player.color == payload.turn);
        if let Some(index) = turn_index {
            assert_eq!(
                imported.state.active.as_ref(),
                Some(&imported.state.players[index].id),
                "{name} active seat"
            );
        }
    }
}

#[test]
fn control_is_only_ever_attributed_to_a_seat_that_is_playing() {
    // Control is inferred, not read: units on the planet, else an owner token (LRR 25.4). An
    // inference that produces a controller nobody is playing is a mapping bug wearing a rules hat.
    let store = ti4_content::ContentStore::embedded();
    for (name, payload) in telemetry() {
        let imported = import(store, &payload, FULL).expect("imports");
        let seats: Vec<_> = imported
            .state
            .players
            .iter()
            .map(|p| p.id.clone())
            .collect();
        let mut controlled = 0usize;
        for system in imported.state.board.values() {
            for (planet, controller) in &system.planet_control {
                assert!(
                    seats.contains(controller),
                    "{name}: {planet} is controlled by {controller}, who is not playing"
                );
                controlled += 1;
            }
        }
        assert!(
            controlled > 0,
            "{name}: nobody controls anything, which no mid-game table looks like"
        );
    }
}

#[test]
fn hidden_state_is_reported_as_a_gap_and_never_invented() {
    // The property the whole module is built around. `handSummary` gives counts; if a seat's hand
    // were ever populated it would have been guessed, and a guessed action card is a bot playing
    // a card it does not hold.
    let store = ti4_content::ContentStore::embedded();
    for (name, payload) in telemetry() {
        let imported = import(store, &payload, FULL).expect("imports");

        for seat in &imported.state.players {
            assert!(
                seat.action_cards.is_empty(),
                "{name}: {} was given action cards the table never showed",
                seat.id
            );
            assert!(
                seat.secret_objectives.is_empty(),
                "{name}: {} was given secret objectives the table never showed",
                seat.id
            );
        }

        // And every seat the mod said holds cards must appear in the gap list.
        for (index, reported) in payload.players.iter().enumerate() {
            let seat = &imported.state.players[index].id;
            let cards = reported.hand_summary.get("Actions").copied().unwrap_or(0);
            if cards > 0 {
                assert!(
                    imported.gaps.contains(&Gap::HandUnknown {
                        seat: seat.clone(),
                        cards
                    }),
                    "{name}: {seat} holds {cards} action cards and no gap says so"
                );
            }
        }

        assert!(imported.gaps.contains(&Gap::ExhaustionUnknown), "{name}");
        assert!(
            imported
                .gaps
                .iter()
                .any(|gap| matches!(gap, Gap::PhaseInferred(_))),
            "{name}: the phase is inferred and must say so"
        );
        assert!(imported.gaps.contains(&Gap::DecksUnknown), "{name}");
    }
}

#[test]
fn the_custodians_flag_follows_the_points_that_prove_it() {
    // LRR 27.4: once the token is lifted every round has an agenda phase. The mod reports the
    // points it paid, which is the only observable trace that it happened.
    let store = ti4_content::ContentStore::embedded();
    for (name, payload) in telemetry() {
        let imported = import(store, &payload, FULL).expect("imports");
        let lifted = payload
            .players
            .iter()
            .any(|player| player.custodians_points > 0);
        assert_eq!(
            imported.state.custodians_removed, lifted,
            "{name} custodians"
        );
    }
}

#[test]
fn the_phase_follows_whether_the_strategy_cards_have_been_dealt() {
    // The table never reports its phase, and the engine offers entirely different decisions in
    // each. Cards dealt is the one observable signal: before they are, the table is still
    // choosing them.
    use ti4_model::state::Phase;
    let store = ti4_content::ContentStore::embedded();
    for (name, payload) in telemetry() {
        let imported = import(store, &payload, FULL).expect("imports");
        let cards_per_seat = if payload.players.len() <= 4 { 2 } else { 1 };
        let dealt = !payload.players.is_empty()
            && payload
                .players
                .iter()
                .all(|player| player.strategy_cards.len() >= cards_per_seat);
        let expected = if dealt {
            Phase::Action
        } else {
            Phase::Strategy
        };
        assert_eq!(imported.state.phase, expected, "{name}");
        assert!(
            imported.gaps.contains(&Gap::PhaseInferred(expected)),
            "{name}: the inference must be declared"
        );
    }
}

#[test]
fn a_partial_live_draft_keeps_held_cards_out_of_the_unclaimed_pool() {
    let store = ti4_content::ContentStore::embedded();
    let mut payload = telemetry().into_values().next().expect("a fixture");
    for player in &mut payload.players {
        player.strategy_cards.clear();
        player.strategy_cards_face_down.clear();
    }
    payload.players[0]
        .strategy_cards
        .push("Technology".to_owned());

    let imported = import(store, &payload, FULL).expect("partial draft imports");
    let held = imported.state.players[0].strategy_cards[0].clone();
    assert_eq!(imported.state.phase, ti4_model::state::Phase::Strategy);
    assert!(!imported.state.unclaimed_strategy_cards.contains(&held));
}

// -- regressions for the 2026-09-09 review findings ------------------------------------------

/// F-TTS-1: an unresolved faction must not shift every later seat.
#[test]
fn an_unresolved_faction_does_not_reattach_every_later_seat() {
    let store = ti4_content::ContentStore::embedded();
    let mut payload = telemetry()
        .into_values()
        .next()
        .expect("at least one fixture");

    // Break the *first* seat's faction, which is the worst case: without position-preserving
    // seats every following record slides up one and the whole table is misattributed.
    let displaced: Vec<(String, i32, i32)> = payload
        .players
        .iter()
        .skip(1)
        .map(|p| (p.faction_short.clone(), p.score, p.trade_goods))
        .collect();
    payload.players[0].faction_name = "The Cartographers of Nowhere".to_owned();
    payload.players[0].faction_short = "Nowhere".to_owned();
    // Make the first record's numbers unmistakable if they leak onto somebody.
    payload.players[0].score = 99;
    payload.players[0].trade_goods = 98;

    let imported = import(store, &payload, FULL).expect("the other five still seat a game");
    assert!(
        imported
            .gaps
            .iter()
            .any(|gap| matches!(gap, Gap::UnknownFaction { .. })),
        "the unresolved faction must be reported"
    );
    assert_eq!(
        imported.state.players.len(),
        payload.players.len() - 1,
        "only the unresolved seat is dropped"
    );
    for (short, score, goods) in displaced {
        let seat = imported
            .state
            .players
            .iter()
            .find(|p| p.id.as_str().eq_ignore_ascii_case(&short.replace('-', "")))
            .unwrap_or_else(|| panic!("{short} lost its seat"));
        assert_eq!(seat.victory_points, score, "{short} score slid");
        assert_eq!(seat.trade_goods, goods, "{short} trade goods slid");
    }
    assert!(
        imported
            .state
            .players
            .iter()
            .all(|p| p.victory_points != 99),
        "the unresolved record's score reached a real seat"
    );
}

/// F-TTS-2: setup-dealt public objectives must not survive an import.
#[test]
fn setup_dealt_objectives_never_survive_the_import() {
    let store = ti4_content::ContentStore::embedded();
    for (name, payload) in telemetry() {
        let imported = import(store, &payload, FULL).expect("imports");
        assert!(
            imported.state.revealed_objectives.is_empty(),
            "{name}: {:?} were revealed by setup, not by the table",
            imported.state.revealed_objectives
        );
        assert!(
            imported
                .gaps
                .iter()
                .any(|gap| matches!(gap, Gap::ObjectivesNotTranslated(_))),
            "{name}: clearing them has to be declared"
        );
    }
}

/// F-TTS-3: a seat that has passed must not look active to the policy.
#[test]
fn a_passed_seat_is_imported_as_passed() {
    let store = ti4_content::ContentStore::embedded();
    let mut payload = telemetry()
        .into_values()
        .next()
        .expect("at least one fixture");
    for (index, player) in payload.players.iter_mut().enumerate() {
        player.active = index % 2 == 0;
    }
    let imported = import(store, &payload, FULL).expect("imports");
    for (index, seat) in imported.state.players.iter().enumerate() {
        assert_eq!(
            seat.passed,
            index % 2 != 0,
            "{} has the wrong passed status",
            seat.id
        );
    }
}
