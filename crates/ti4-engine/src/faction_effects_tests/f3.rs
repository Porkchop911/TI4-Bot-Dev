//! Faction effect tests, group f3 (see mod.rs and docs/FACTION_EFFECT_COVERAGE.md).
//!
//! The Prophecy of Kings factions: Argent, Cabal, Empyrean, Mahact, Nomad, Naaz, Titans. Most of
//! their effects are asserted by the thorough module tests in `factions/*.rs` (cited in the
//! table); the tests here fill the gaps those leave: each hero's unlock on the third scored
//! objective, the purge after use for heroes whose module tests call the faction hook only (not
//! `leaders::use_leader`), the Mahact commander's "end your turn", and Edict's pool.

use ti4_content::ContentStore;
use ti4_model::id::{LeaderId, ObjectiveId, PlayerId, SystemId};
use ti4_model::state::{GameState, LeaderStatus};

use super::{ALL, scripted, seated_all};
use crate::fixtures::{put, with_context};

fn a() -> PlayerId {
    PlayerId::new("a")
}
fn b() -> PlayerId {
    PlayerId::new("b")
}

fn count(state: &GameState, system: &SystemId, owner: &PlayerId, kind: &str) -> usize {
    state
        .system_state(system)
        .units
        .iter()
        .filter(|unit| &unit.owner == owner && unit.type_id.as_str() == kind)
        .count()
}

/// Seat `a` as `faction` (b is Sol), with every one of a's heroes still locked.
fn table_of(faction: &str) -> GameState {
    seated_all(&[("a", faction), ("b", "sol")])
}

fn hero_status(state: &GameState, hero: &str) -> Option<LeaderStatus> {
    crate::leaders::status(state, &a(), &LeaderId::new(hero))
}

fn unlock_hero(state: &mut GameState, hero: &str) {
    state
        .player_mut(&a())
        .unwrap()
        .leaders
        .insert(LeaderId::new(hero), LeaderStatus::Unlocked);
}

/// "Have 3 scored objectives": locked at two, unlocked at three, and not for the other seat.
fn hero_unlocks_on_three(faction: &str, hero: &str) {
    let mut state = table_of(faction);
    let content = ContentStore::embedded();
    assert_eq!(hero_status(&state, hero), Some(LeaderStatus::Locked));
    for alias in ["o1", "o2"] {
        state.record_score(&a(), ObjectiveId::new(alias));
    }
    crate::leaders::check_unlocks(&mut state, content, ALL, None, &a());
    assert_eq!(
        hero_status(&state, hero),
        Some(LeaderStatus::Locked),
        "two is not three"
    );
    state.record_score(&a(), ObjectiveId::new("o3"));
    crate::leaders::check_unlocks(&mut state, content, ALL, None, &b());
    assert_eq!(
        hero_status(&state, hero),
        Some(LeaderStatus::Locked),
        "b's sweep is not a's"
    );
    crate::leaders::check_unlocks(&mut state, content, ALL, None, &a());
    assert_eq!(hero_status(&state, hero), Some(LeaderStatus::Unlocked));
}

#[test]
fn argenthero_1_unlocks_with_three_scored_objectives() {
    hero_unlocks_on_three("argent", "argenthero");
}
#[test]
fn cabalhero_1_unlocks_with_three_scored_objectives() {
    hero_unlocks_on_three("cabal", "cabalhero");
}
#[test]
fn empyreanhero_1_unlocks_with_three_scored_objectives() {
    hero_unlocks_on_three("empyrean", "empyreanhero");
}
#[test]
fn mahacthero_1_unlocks_with_three_scored_objectives() {
    hero_unlocks_on_three("mahact", "mahacthero");
}
#[test]
fn nomadhero_1_unlocks_with_three_scored_objectives() {
    hero_unlocks_on_three("nomad", "nomadhero");
}
#[test]
fn naazhero_1_unlocks_with_three_scored_objectives() {
    hero_unlocks_on_three("naaz", "naazhero");
}
#[test]
fn titanshero_1_unlocks_with_three_scored_objectives() {
    hero_unlocks_on_three("titans", "titanshero");
}

#[test]
fn argenthero_2_flock_migration_moves_a_ship_and_the_hero_is_purged() {
    let mut state = table_of("argent");
    let content = ContentStore::embedded();
    let ids: Vec<String> = crate::fixtures::plain_systems(14)
        .into_iter()
        .filter(|id| !state.board.contains_key(&SystemId::new(id.as_str())))
        .take(6)
        .collect();
    let mut tiles = vec!["18"];
    tiles.extend(ids.iter().map(String::as_str));
    let galaxy = ti4_content::galaxy::Galaxy::build(content, &tiles, ALL, 2).unwrap();
    let (from, safe, held) = (
        SystemId::new(ids[0].as_str()),
        SystemId::new(ids[2].as_str()),
        SystemId::new(ids[4].as_str()),
    );
    put(&mut state, &from, "carrier", &a(), 1);
    state.system_mut(&safe).command_tokens.insert(a());
    state.system_mut(&held).command_tokens.insert(a());
    put(&mut state, &held, "cruiser", &b(), 1);
    // Locked: the hero does nothing.
    let hero = LeaderId::new("argenthero");
    let used = with_context(&mut state, ALL, Some(&galaxy), &mut scripted(&[]), |ctx| {
        crate::leaders::use_leader(ctx, &a(), &hero)
    });
    assert!(!used && count(&state, &from, &a(), "carrier") == 1);
    unlock_hero(&mut state, "argenthero");
    let answers = [
        "ship|0".to_owned(),
        format!("to|{safe}"),
        "decline".into(),
        "decline".into(),
    ];
    let answers: Vec<&str> = answers.iter().map(String::as_str).collect();
    let used = with_context(
        &mut state,
        ALL,
        Some(&galaxy),
        &mut scripted(&answers),
        |ctx| crate::leaders::use_leader(ctx, &a(), &hero),
    );
    assert!(used);
    assert_eq!(count(&state, &safe, &a(), "carrier"), 1);
    assert_eq!(count(&state, &from, &a(), "carrier"), 0);
    assert_eq!(
        hero_status(&state, "argenthero"),
        Some(LeaderStatus::Purged)
    );
}

#[test]
fn empyreanhero_2_frontier_tokens_explore_and_the_hero_is_purged() {
    let mut state = table_of("empyrean");
    let nebula = crate::fixtures::a_system_where("nebula");
    let ring = crate::fixtures::hub_with_outer(&nebula);
    state.frontier_tokens.clear();
    let under = SystemId::new(nebula.as_str());
    put(&mut state, &under, "cruiser", &a(), 1);
    unlock_hero(&mut state, "empyreanhero");
    let hero = LeaderId::new("empyreanhero");
    let used = with_context(
        &mut state,
        ALL,
        Some(&ring.galaxy),
        &mut scripted(&[]),
        |ctx| crate::leaders::use_leader(ctx, &a(), &hero),
    );
    assert!(used);
    assert!(
        !state.frontier_tokens.contains(&under),
        "the token under the ship was explored"
    );
    assert_eq!(state.exploration_log.len(), 1);
    assert_eq!(
        hero_status(&state, "empyreanhero"),
        Some(LeaderStatus::Purged)
    );
}

#[test]
fn naazhero_2_gains_a_relic_and_the_hero_is_purged() {
    let mut state = table_of("naaz");
    let card = state.unclaimed_strategy_cards[0].to_string();
    unlock_hero(&mut state, "naazhero");
    let relics = state.player(&a()).unwrap().relics.len();
    let hero = LeaderId::new("naazhero");
    let used = with_context(
        &mut state,
        ALL,
        None,
        &mut scripted(&[&card, "decline"]),
        |ctx| crate::leaders::use_leader(ctx, &a(), &hero),
    );
    assert!(used);
    assert_eq!(state.player(&a()).unwrap().relics.len(), relics + 1);
    assert_eq!(hero_status(&state, "naazhero"), Some(LeaderStatus::Purged));
}

fn tactical_game(
    state: GameState,
    galaxy: ti4_content::galaxy::Galaxy,
    answers: &[String],
) -> crate::game::Game<'static> {
    let table = crate::choice::Table::with_default(Box::new(crate::choice::Scripted::new(
        answers.iter().cloned(),
    )));
    crate::game::Game::with_table(state, ContentStore::embedded(), table)
        .with_sources(ALL)
        .with_galaxy(galaxy)
}

fn commander_board(commander: bool) -> (crate::game::Game<'static>, SystemId) {
    let mut state = table_of("mahact");
    let ids: Vec<String> = ti4_content::galaxy::all_systems(ContentStore::embedded(), ALL)
        .iter()
        .filter(|(_, s)| !s.is_anomaly() && !s.is_hyperlane())
        .map(|(id, _)| (*id).to_owned())
        .filter(|id| !state.board.contains_key(&SystemId::new(id.as_str())))
        .take(7)
        .collect();
    let refs: Vec<&str> = ids.iter().map(String::as_str).collect();
    let galaxy =
        ti4_content::galaxy::Galaxy::build(ContentStore::embedded(), &refs, ALL, 1).unwrap();
    let held = SystemId::new(ids[0].as_str());
    state.phase = ti4_model::state::Phase::Action;
    state.active = Some(a());
    state.system_mut(&held).command_tokens.insert(a());
    // Mahact starts with Predictive Intelligence and Bio-Stims, whose end-of-turn windows would
    // otherwise take the scripted table's default answer.
    for tech in ["pi", "bs"] {
        state
            .player_mut(&a())
            .unwrap()
            .exhausted_technologies
            .insert(ti4_model::id::TechnologyId::new(tech));
    }
    if commander {
        assert!(crate::promissory::grant_commander_ability(
            &mut state,
            ContentStore::embedded(),
            &a(),
            "mahactcommander"
        ));
    }
    let answers = [crate::game::TACTICAL_ACTION_ID.to_owned(), held.to_string()];
    (tactical_game(state, galaxy, &answers), held)
}

#[test]
fn mahactcommander_2_activating_a_tokened_system_returns_both_tokens_and_ends_the_turn() {
    let (mut game, held) = commander_board(true);
    let tactic = game.state.player(&a()).unwrap().tactic_tokens;
    for _ in 0..2 {
        assert_eq!(game.step().error, None);
    }
    assert!(
        game.events
            .iter()
            .any(|e| e == "TURN_ENDED_BY_MAHACT_COMMANDER:a"),
        "{:?}",
        game.events
    );
    assert!(
        !game.state.systems_with_token(&a()).contains(&held),
        "the board token went home"
    );
    assert_eq!(
        game.state.player(&a()).unwrap().tactic_tokens,
        tactic - 1,
        "the tactic token was spent"
    );
    assert_ne!(game.state.active, Some(a()), "the turn passed on");
}

#[test]
fn mahactcommander_3_a_locked_commander_cannot_activate_a_tokened_system() {
    let (mut game, held) = commander_board(false);
    assert_eq!(game.step().error, None);
    assert!(
        game.step().error.is_some(),
        "no commander, no second activation"
    );
    assert!(game.state.systems_with_token(&a()).contains(&held));
}

#[test]
fn mahact_edict_pool_tokens_raise_the_fleet_limit_and_leave_the_sheet_pools_alone() {
    let mut state = table_of("mahact");
    let sheet = |s: &GameState| {
        let seat = s.player(&a()).unwrap();
        (seat.tactic_tokens, seat.fleet_tokens, seat.strategic_tokens)
    };
    let (before, limit) = (
        sheet(&state),
        crate::fleet::limit(&state, ContentStore::embedded(), &a()),
    );
    assert!(crate::factions::mahact::add_token(&mut state, &a(), &b()));
    assert_eq!(
        sheet(&state),
        before,
        "a foreign token is not on a's sheet, so it cannot be redistributed"
    );
    assert_eq!(
        crate::fleet::limit(&state, ContentStore::embedded(), &a()),
        limit + 1
    );
}

const FACTIONS: [&str; 7] = [
    "argent", "cabal", "empyrean", "mahact", "nomad", "naaz", "titans",
];

#[test]
fn f3_every_leader_of_the_pok_factions_is_handled_by_the_engine() {
    let missing = crate::leaders::unimplemented(ContentStore::embedded(), &FACTIONS);
    assert!(missing.is_empty(), "leaders nothing answers: {missing:?}");
}

#[test]
fn f3_every_ability_of_the_pok_factions_is_registered_and_none_is_blocked() {
    let content = ContentStore::embedded();
    let unanswered = crate::faction_abilities::unimplemented(content, ALL);
    let blocked = crate::faction_abilities::blocked();
    for faction in FACTIONS {
        let abilities = ti4_content::factions::get(content, faction)
            .unwrap()
            .abilities();
        for ability in abilities {
            let id = ability.to_string();
            assert!(
                !unanswered.contains(&id),
                "{faction}: {id} is answered by nothing"
            );
            assert!(
                !blocked.contains_key(id.as_str()),
                "{faction}: {id} is blocked"
            );
        }
    }
}
