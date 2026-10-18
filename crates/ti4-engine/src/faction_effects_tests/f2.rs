//! Faction effect tests, group f2 (see mod.rs and docs/FACTION_EFFECT_COVERAGE.md).
//!
//! Most of this group's effects already have game-path tests next to their code (factions/nekro.rs,
//! saar.rs, sardakk.rs, winnu.rs, yin.rs, yssaril.rs, keleres*.rs, vote.rs, promissory.rs, game.rs);
//! the table points at them. This file adds what had none: the unlock conditions of Sol's and
//! Xxcha's commanders, the hero unlocks of every faction here, Sol's commander and agent, Xxcha's
//! PoK hero, and sweeps that fail when a printed ability or leader of these factions is wired to
//! nothing.

use ti4_content::galaxy::all_systems;
use ti4_model::id::{LeaderId, ObjectiveId, PlanetId, PlayerId, SystemId};
use ti4_model::state::{GameState, LeaderStatus};

use super::{ALL, content, pid, scripted, seated_all, emit_full, strs};
use crate::leaders;
use crate::production::{Spend, planet_value_now};

const FACTIONS: &[&str] = &[
    "nekro", "saar", "sardakk", "sol", "winnu", "xxcha", "yin", "yssaril", "keleresm", "keleresx",
    "keleresa",
];

fn a() -> PlayerId {
    pid("a")
}

fn set(state: &mut GameState, who: &str, leader: &str, status: LeaderStatus) {
    state
        .player_mut(&pid(who))
        .expect("seat")
        .leaders
        .insert(LeaderId::new(leader), status);
}

fn status(state: &GameState, who: &str, leader: &str) -> Option<LeaderStatus> {
    leaders::status(state, &pid(who), &LeaderId::new(leader))
}

/// Take unowned planets with a positive `kind` value until `commander` unlocks; the unlock must
/// not already hold before, and must hold after, adding exactly the planets that reach 12.
fn controls_until_unlocked(state: &mut GameState, commander: &str, kind: Spend) -> i64 {
    let id = LeaderId::new(commander);
    let check = |state: &GameState| {
        leaders::commander_unlocked(state, content(), ALL, None, &a(), &id)
            .expect("the commander has a registered unlock check")
    };
    assert!(!check(state), "{commander} starts locked");
    let owned: Vec<PlanetId> = state
        .board
        .values()
        .flat_map(|b| b.planet_control.keys().cloned())
        .collect();
    let mut total: i64 = state
        .controlled_planets(&a())
        .into_iter()
        .map(|(_, p)| planet_value_now(state, content(), ALL, p, kind))
        .sum();
    for (system, record) in all_systems(content(), ALL) {
        for planet in record.planets() {
            let planet = PlanetId::new(planet);
            let worth = planet_value_now(state, content(), ALL, &planet, kind);
            if owned.contains(&planet) || worth == 0 {
                continue;
            }
            state
                .system_mut(&SystemId::new(system))
                .planet_control
                .insert(planet, a());
            total += worth;
            if check(state) {
                return total;
            }
            assert!(total < 12, "the total passed 12 without unlocking");
        }
    }
    panic!("no planets left");
}

#[test]
fn solcommander_1_unlocks_at_twelve_resources() {
    let mut state = seated_all(&[("a", "sol"), ("b", "hacan")]);
    let total = controls_until_unlocked(&mut state, "solcommander", Spend::Resources);
    assert!(total >= 12, "unlocked with {total} resources");
}

#[test]
fn xxchacommander_1_unlocks_at_twelve_influence() {
    let mut state = seated_all(&[("a", "xxcha"), ("b", "hacan")]);
    let total = controls_until_unlocked(&mut state, "xxchacommander", Spend::Influence);
    assert!(total >= 12, "unlocked with {total} influence");
}

/// Heroes unlock on the third scored objective (not before), for every faction here.
fn hero_unlocks_at_three(faction: &str, hero: &str) {
    let mut state = seated_all(&[("a", faction), ("b", "hacan")]);
    assert_eq!(status(&state, "a", hero), Some(LeaderStatus::Locked), "{hero} starts locked");
    for objective in ["o1", "o2"] {
        state.record_score(&a(), ObjectiveId::new(objective));
    }
    leaders::check_unlocks(&mut state, content(), ALL, None, &a());
    assert_eq!(status(&state, "a", hero), Some(LeaderStatus::Locked), "two is not three");
    state.record_score(&a(), ObjectiveId::new("o3"));
    leaders::check_unlocks(&mut state, content(), ALL, None, &a());
    assert_eq!(status(&state, "a", hero), Some(LeaderStatus::Unlocked));
}

macro_rules! hero_unlock {
    ($($name:ident: $faction:literal, $hero:literal;)*) => {$(
        #[test]
        fn $name() {
            hero_unlocks_at_three($faction, $hero);
        }
    )*};
}

hero_unlock! {
    nekrohero_1_unlocks_with_three_scored_objectives: "nekro", "nekrohero";
    saarhero_1_unlocks_with_three_scored_objectives: "saar", "saarhero";
    sardakkhero_1_unlocks_with_three_scored_objectives: "sardakk", "sardakkhero";
    solhero_1_unlocks_with_three_scored_objectives: "sol", "solhero";
    winnuhero_1_unlocks_with_three_scored_objectives: "winnu", "winnuhero";
    xxchahero_te_1_unlocks_with_three_scored_objectives: "xxcha", "xxchahero-te";
    yinhero_1_unlocks_with_three_scored_objectives: "yin", "yinhero";
    yssarilhero_1_unlocks_with_three_scored_objectives: "yssaril", "yssarilhero";
    keleresheroharka_1_unlocks_with_three_scored_objectives: "keleresm", "keleresheroharka";
    keleresheroodlynn_1_unlocks_with_three_scored_objectives: "keleresx", "keleresheroodlynn";
    keleresherokuuasi_1_unlocks_with_three_scored_objectives: "keleresa", "keleresherokuuasi";
}

#[test]
fn every_leader_of_these_factions_has_a_handler() {
    let missing = leaders::unimplemented(content(), ti4_model::content_types::DEFAULT, FACTIONS);
    assert!(missing.is_empty(), "leaders wired to nothing: {missing:?}");
}

#[test]
fn every_ability_of_these_factions_is_implemented_or_gated() {
    let printed = crate::faction_abilities::unimplemented(content(), ALL);
    let mine: Vec<String> = FACTIONS
        .iter()
        .flat_map(|f| {
            ti4_content::factions::get(content(), f)
                .expect("faction")
                .abilities()
                .into_iter()
                .map(ToOwned::to_owned)
        })
        .filter(|ability| printed.contains(ability))
        .collect();
    assert!(mine.is_empty(), "abilities answered by nothing: {mine:?}");
}

/// Claire Gibson: "At the start of a ground combat on a planet you control: You may place 1
/// infantry from your reinforcements on that planet."
fn claire(unlocked: bool, answer: &str) -> usize {
    let mut state = seated_all(&[("a", "sol"), ("b", "hacan")]);
    let (system, planet) = crate::fixtures::a_placed_planet();
    state.system_mut(&system).planet_control.insert(planet.clone(), a());
    if unlocked {
        set(&mut state, "a", "solcommander", LeaderStatus::Unlocked);
    }
    emit_full(
        &mut state,
        &mut scripted(&[answer]),
        ALL,
        "GROUND_COMBAT_STARTED",
        strs(&[
            ("system", system.as_str()),
            ("planet", planet.as_str()),
            ("attacker", "b"),
            ("defender", "a"),
        ]),
    );
    state
        .system_state(&system)
        .planet_units
        .get(&planet)
        .map_or(0, Vec::len)
}

#[test]
fn solcommander_2_places_an_infantry_when_a_ground_combat_starts_on_its_planet() {
    let ability = "leader:sol:solcommander:GROUND_COMBAT_STARTED:after";
    assert_eq!(claire(true, ability), 1, "unlocked: one infantry arrives");
    assert_eq!(claire(true, "decline"), 0, "optional: declined adds nothing");
    assert_eq!(claire(false, ability), 0, "locked: nothing is offered");
}

/// Sol's agent (Evelyn Delouis): the chosen ground force rolls 1 additional die this round.
fn first_ground_roll_dice(marked: bool) -> usize {
    let mut state = seated_all(&[("a", "sol"), ("b", "hacan")]);
    let (system, planet) = crate::fixtures::a_placed_planet();
    crate::fixtures::put_on_planet(&mut state, &system, &planet, "sol_infantry", &a(), 2);
    crate::fixtures::put_on_planet(&mut state, &system, &planet, "infantry", &pid("b"), 2);
    if marked {
        let used = crate::fixtures::with_context(
            &mut state,
            ALL,
            None,
            &mut crate::choice::Table::new(),
            |ctx| {
                // the agent is used at the start of the first round (the counter ticks to 1)
                ctx.state.combat_round_seq = 1;
                leaders::use_leader(ctx, &a(), &LeaderId::new("solagent"))
            },
        );
        assert!(used || status(&state, "a", "solagent") != Some(LeaderStatus::Readied));
        state.combat_round_seq = 0;
    }
    let mut dice = crate::dice::Dice::new();
    let mut rng = crate::rng::GameRng::new(1);
    crate::invasion::ground_combat(
        &mut state,
        content(),
        ALL,
        &mut crate::choice::Table::new(),
        &mut dice,
        &mut rng,
        &system,
        &planet,
        &a(),
    )
    .expect("the fight resolves");
    dice.history()[0].faces.len()
}

#[test]
fn solagent_1_the_chosen_ground_force_rolls_an_additional_die() {
    let plain = first_ground_roll_dice(false);
    let marked = first_ground_roll_dice(true);
    assert_eq!(marked, plain + 1, "plain {plain}, with the agent {marked}");
}

/// Xxcha PoK hero (Xxekir Grom): exhausted planets count resources and influence together.
#[test]
fn xxchahero_1_exhausting_planets_combines_resources_and_influence() {
    let mut state = seated_all(&[("a", "xxcha"), ("b", "hacan")]);
    let plain = (
        crate::production::available(&state, content(), ALL, &a(), Spend::Resources),
        crate::production::available(&state, content(), ALL, &a(), Spend::Influence),
    );
    set(&mut state, "a", "xxchahero", LeaderStatus::Unlocked);
    let with_hero = crate::production::available(&state, content(), ALL, &a(), Spend::Resources);
    assert!(
        with_hero > plain.0,
        "resources {} -> {with_hero} (influence was {})",
        plain.0,
        plain.1
    );
}
