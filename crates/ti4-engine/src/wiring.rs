//! Wiring checks: does the driver actually reach each subsystem?
//!
//! Five modules in this project have arrived correct, fully tested, and called by nothing —
//! combat, invasion, fleet enforcement, production and leaders. Every one had green unit tests
//! the whole time, because a unit test proves a module *works*, never that anything *uses* it.
//!
//! The registry ledger catches missing content. This catches missing wiring: it drives a real
//! game and asserts each subsystem announced itself. A module that quietly stops being called
//! fails here rather than in a game months later.

#![cfg(test)]

use ti4_content::ContentStore;
use ti4_model::content_types::POK;
use ti4_model::id::{PlayerId, SystemId};

use crate::choice::{Scripted, Table};
use crate::fixtures::{plain_hub, put};
use crate::game::{Game, TACTICAL_ACTION_ID};
use crate::setup::start_game;

/// Drive a tactical action to completion on a real map, and return the events it produced.
fn events_from_a_tactical_action() -> Vec<String> {
    let hub = plain_hub();
    let players = [PlayerId::new("a"), PlayerId::new("b")];
    let mut state = start_game(ContentStore::embedded(), &players, POK, None).unwrap();
    state.phase = ti4_model::state::Phase::Action;
    state.active = Some(PlayerId::new("a"));

    let origin = SystemId::new(hub.outer[0].clone());
    let target = SystemId::new(hub.centre.clone());
    // A carrier with troops aboard, moving onto a defended planet: this is the shortest run
    // that should touch movement, cargo, combat, invasion and production in one action.
    put(&mut state, &origin, "carrier", &PlayerId::new("a"), 1);
    put(&mut state, &origin, "infantry", &PlayerId::new("a"), 2);
    put(&mut state, &target, "fighter", &PlayerId::new("b"), 1);

    let table = Table::with_default(Box::new(Scripted::new([
        TACTICAL_ACTION_ID.to_owned(),
        target.to_string(),
        format!("move|{origin}|0"),
    ])));
    let mut game =
        Game::with_table(state, ContentStore::embedded(), table).with_galaxy(hub.galaxy.clone());

    for _ in 0..200 {
        if game
            .events
            .iter()
            .any(|event| event == "TACTICAL_ACTION_COMPLETE")
        {
            break;
        }
        if game.step().error.is_some() {
            break; // the script ran out, which is fine: we assert on what was reached
        }
    }
    game.events.clone()
}

#[test]
fn a_tactical_action_reaches_activation_and_movement() {
    let events = events_from_a_tactical_action();
    assert!(
        events.iter().any(|e| e == "TACTICAL_ACTION_BEGAN"),
        "the action opened; events {events:?}"
    );
    assert!(
        events.iter().any(|e| e.starts_with("SYSTEM_ACTIVATED:")),
        "activation was reached; events {events:?}"
    );
}

#[test]
fn a_driven_round_reaches_every_phase() {
    // The phase machine is the spine. If a phase stops being entered, every subsystem hanging
    // off it goes quiet at once and no unit test would notice.
    let players = [PlayerId::new("a"), PlayerId::new("b"), PlayerId::new("c")];
    let state = start_game(ContentStore::embedded(), &players, POK, None).unwrap();
    let mut game = Game::with_seeded_random(state, ContentStore::embedded(), 5);
    game.run(1, 500).expect("a round completes");

    for expected in [
        "ACTION_PHASE_BEGAN",
        "STATUS_PHASE_BEGAN",
        "STATUS_SCORING_BEGAN",
        "STATUS_PHASE_RESOLVED",
    ] {
        assert!(
            game.events.iter().any(|event| event == expected),
            "{expected} was never reached; events {:?}",
            game.events
        );
    }
}

#[test]
fn the_status_phase_reaches_the_token_gain() {
    // 81.5 is a real decision, and the only sign it happened is the event.
    let players = [PlayerId::new("a"), PlayerId::new("b")];
    let state = start_game(ContentStore::embedded(), &players, POK, None).unwrap();
    let mut game = Game::with_seeded_random(state, ContentStore::embedded(), 9);
    game.run(1, 500).expect("a round completes");

    assert!(
        game.events
            .iter()
            .any(|event| event.starts_with("COMMAND_TOKEN_GAINED")),
        "81.5 was never reached; events {:?}",
        game.events
    );
}

#[test]
fn every_subsystem_the_driver_owns_is_reachable() {
    // The list is deliberately explicit. Adding a subsystem without adding it here is the
    // omission this test exists to make loud, and a name that no longer appears anywhere in
    // `game.rs` means the wiring was removed.
    let driver = include_str!("game.rs");
    for subsystem in [
        "crate::fleet::enforce",
        "crate::combat::space_cannon_offense",
        "crate::combat::CombatWindow",
        "crate::invasion::InvasionWindow",
        "crate::production::ProductionWindow",
        "crate::agenda_effects::resolve",
    ] {
        assert!(
            driver.contains(subsystem),
            "{subsystem} is no longer called by the driver"
        );
    }
}

#[test]
fn only_test_support_modules_are_test_gated() {
    // `exploration` was accidentally `#[cfg(test)]` for several commits: it compiled, its own
    // tests passed because they run under cfg(test), and nothing outside tests could call it.
    // A module that vanishes from the library is invisible to every other check here.
    let lib = include_str!("lib.rs");
    let gated: Vec<&str> = lib
        .lines()
        .zip(lib.lines().skip(1))
        .filter(|(attr, _)| attr.trim() == "#[cfg(test)]")
        .filter_map(|(_, decl)| decl.trim().strip_prefix("pub mod "))
        .map(|name| name.trim_end_matches(';'))
        .collect();

    assert_eq!(
        gated,
        vec!["fixtures"],
        "only test-support modules may be test-gated"
    );
}

#[test]
fn the_status_phase_still_readies_leaders() {
    // Leaders were built, tested, and uncalled for four commits. This is the guard.
    let status = include_str!("status.rs");
    assert!(
        status.contains("crate::leaders::ready_all"),
        "81.6 no longer readies leaders"
    );
}

#[test]
fn taking_a_planet_still_explores_it() {
    // 35.1 only fires on a planet nobody held. Dropping the call would silently stop every
    // exploration in the game, and the invasion would still look correct.
    let invasion = include_str!("invasion.rs");
    assert!(
        invasion.contains("crate::exploration::explore"),
        "capturing a planet no longer explores it"
    );
}

#[test]
fn the_scoring_window_still_offers_secrets() {
    // 61.6 lets a player score one public and one secret. Dropping the secrets call would
    // leave satisfied secrets unscoreable all game with nothing failing.
    let objectives = include_str!("objectives.rs");
    assert!(
        objectives.contains("crate::secrets::scoreable"),
        "the scoring window no longer offers secrets"
    );
    assert!(
        objectives.contains("crate::secrets::award"),
        "a scored secret no longer leaves the hand"
    );
}

#[test]
fn scoring_still_checks_leader_unlocks() {
    let objectives = include_str!("objectives.rs");
    assert!(
        objectives.contains("crate::leaders::check_unlocks"),
        "51.7 no longer fires on scoring"
    );
}

#[test]
fn the_laws_that_bite_are_still_consulted() {
    // A law enforced by nothing is the quietest failure in this engine: state.laws looks like
    // a rule change even when no rule reads it.
    for (module, source, hook) in [
        (
            "fleet",
            include_str!("fleet.rs"),
            "crate::laws::fleet_pool_cap",
        ),
        (
            "action_cards",
            include_str!("action_cards.rs"),
            "crate::laws::action_card_limit",
        ),
        (
            "movement",
            include_str!("movement.rs"),
            "crate::laws::nebulae_passable",
        ),
    ] {
        assert!(source.contains(hook), "{module} no longer consults {hook}");
    }
}

#[test]
fn a_turn_can_open_and_close_a_transaction_without_ending() {
    // 94.1a: a transaction happens "at any time during your turn" and costs nothing. If closing
    // it advanced the turn, trading would silently cost a player their action — and every seeded
    // game would trade its turns away.
    let hub = plain_hub();
    let players = [PlayerId::new("a"), PlayerId::new("b")];
    let mut state = start_game(ContentStore::embedded(), &players, POK, None).unwrap();
    state.phase = ti4_model::state::Phase::Action;
    state.active = Some(PlayerId::new("a"));

    let centre = SystemId::new(hub.centre.clone());
    put(&mut state, &centre, "cruiser", &PlayerId::new("a"), 1);
    put(&mut state, &centre, "cruiser", &PlayerId::new("b"), 1);
    for player in &players {
        let seat = state.player_mut(player).unwrap();
        seat.commodities = 3;
        seat.trade_goods = 0;
    }

    let table = Table::with_default(Box::new(Scripted::new([
        "trade|b".to_owned(),
        "cc3".to_owned(),
        "accept".to_owned(),
    ])));
    let mut game =
        Game::with_table(state, ContentStore::embedded(), table).with_galaxy(hub.galaxy.clone());

    for _ in 0..8 {
        if game.events.iter().any(|event| event == "TRANSACTION") {
            break;
        }
        if game.step().error.is_some() {
            break;
        }
    }

    assert!(
        game.events
            .iter()
            .any(|event| event == "TRANSACTION_OPENED"),
        "the free action was never offered; events {:?}",
        game.events
    );
    assert!(
        game.events.iter().any(|event| event == "TRANSACTION"),
        "the deal never closed; events {:?}",
        game.events
    );
    assert!(
        !game.events.iter().any(|event| event == "TURN_PASSED"),
        "a transaction is free and must not end the turn; events {:?}",
        game.events
    );
    assert_eq!(
        game.state.active,
        Some(PlayerId::new("a")),
        "the trading player still holds the turn"
    );
    assert_eq!(
        game.state.player(&PlayerId::new("a")).unwrap().trade_goods,
        3,
        "21.5 turned the commodities into trade goods on the way over"
    );
}

#[test]
fn the_driver_still_offers_transactions() {
    // Every other subsystem here failed this way first: correct, tested, and called by nothing.
    let driver = include_str!("game.rs");
    assert!(
        driver.contains("crate::transactions::available_actions"),
        "no turn offers a transaction any more"
    );
    assert!(
        driver.contains("crate::transactions::TradeWindow::open"),
        "opening a transaction no longer opens a negotiation"
    );
}

#[test]
fn the_driver_still_offers_relic_actions() {
    // Relics were built, tested, and reachable from nothing: `use_relic` was called only by its
    // own tests, so a relic could be drawn, held and counted while being unusable all game.
    let driver = include_str!("game.rs");
    assert!(
        driver.contains("crate::relics::available_actions"),
        "no turn offers a relic action any more"
    );
    assert!(
        driver.contains("crate::relics::perform"),
        "taking a relic action no longer resolves it"
    );
}

#[test]
fn every_relic_arrives_through_one_door() {
    // A relic can be worth a point the moment it arrives, so a path that takes one off the deck
    // by hand scores nobody the Shard. Exploration did exactly that.
    let exploration = include_str!("exploration.rs");
    assert!(
        exploration.contains("crate::relics::gain"),
        "exploration draws relics without going through relics::gain"
    );
}

#[test]
fn a_driven_round_lets_somebody_actually_score() {
    // The strongest guard the scoring path can have: not that `ScoringWindow` is mentioned in
    // the driver, but that a player who has met a revealed objective takes the point.
    //
    // A text guard cannot see the difference. Opening the window over an *empty* initiative
    // order leaves the name in place, kills scoring outright, and every test in this crate
    // still passed when that was tried.
    let players = [PlayerId::new("a"), PlayerId::new("b")];
    let mut state = start_game(ContentStore::embedded(), &players, POK, None).unwrap();

    // Engineer a Marvel: have your flagship or a war sun on the board. Chosen because it is
    // satisfied by placing one unit, so this test fails for wiring reasons and nothing else.
    state
        .revealed_objectives
        .push(ti4_model::id::ObjectiveId::new("engineer_marvel"));
    let system = SystemId::new(crate::fixtures::plain_systems(1)[0].clone());
    put(&mut state, &system, "warsun", &PlayerId::new("a"), 1);

    // The default table takes the first option offered, and `decline` is appended last, so a
    // player who *can* score does. A sampling decider might refuse and the guard would go
    // quiet for a reason that has nothing to do with wiring.
    let mut game = Game::new(state, ContentStore::embedded());
    game.run(1, 800).expect("a round completes");

    assert!(
        game.events
            .iter()
            .any(|event| event.starts_with("OBJECTIVE_SCORED:")),
        "nobody could score a satisfied objective; events {:?}",
        game.events
    );
}

#[test]
fn a_driven_round_deals_strategy_cards() {
    // The draft is the first thing a round does, and everything downstream — initiative order,
    // who is active, which secondaries are offered — is built on it.
    let players = [PlayerId::new("a"), PlayerId::new("b")];
    let state = start_game(ContentStore::embedded(), &players, POK, None).unwrap();
    let mut game = Game::with_seeded_random(state, ContentStore::embedded(), 11);
    game.run(1, 800).expect("a round completes");

    assert!(
        game.events
            .iter()
            .any(|event| event == "STRATEGY_CARD_CHOSEN"),
        "no strategy card was drafted; events {:?}",
        game.events
    );
}

#[test]
fn the_driver_still_reaches_the_subsystems_with_no_behavioural_guard() {
    // Kept as a text guard only for the subsystems a driven round cannot be made to prove
    // cheaply. It is the weakest kind of check here: it cannot tell a live call from a dead
    // one, so anything provable by driving a game is proved that way above instead.
    let driver = include_str!("game.rs");
    for call in [
        "CargoWindow::for_ship(",
        "VoteWindow::new(&self.state, &alias, choices)",
        "resolve_agenda_phase(",
        "ScoringWindow::new(&self.state.initiative_order())",
    ] {
        assert!(
            driver.contains(call),
            "{call} is no longer reached by the driver"
        );
    }
}
