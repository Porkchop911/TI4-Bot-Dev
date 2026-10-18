//! Faction effect tests, group f1 (see mod.rs and docs/FACTION_EFFECT_COVERAGE.md).

use ti4_model::content_types::POK;
use ti4_model::id::{LeaderId, PlanetId, PlayerId, SystemId};
use ti4_model::state::{GameState, LeaderStatus};

use super::{content, pid, scripted, seated};
use crate::fixtures::{put, put_on_planet};

fn a() -> PlayerId {
    pid("a")
}
fn b() -> PlayerId {
    pid("b")
}

fn set_leader(state: &mut GameState, who: &PlayerId, leader: &str, status: LeaderStatus) -> LeaderId {
    let id = LeaderId::new(leader);
    state
        .player_mut(who)
        .expect("seat")
        .leaders
        .insert(id.clone(), status);
    id
}

fn unlocked(state: &GameState, leader: &LeaderId, galaxy: Option<&ti4_content::galaxy::Galaxy>) -> bool {
    crate::leaders::commander_unlocked(state, content(), POK, galaxy, &a(), leader)
        .expect("the unlock condition is implemented")
}

/// Use `leader` for seat `a` with these scripted answers.
fn use_leader(state: &mut GameState, leader: &LeaderId, answers: &[&str]) -> bool {
    let mut table = scripted(answers);
    crate::fixtures::with_context(state, POK, None, &mut table, |context| {
        crate::leaders::use_leader(context, &a(), leader)
    })
}

// -- Hacan ---------------------------------------------------------------------------------------

#[test]
fn master_of_trade_1_trade_secondary_costs_hacan_no_command_token() {
    let state = seated(&[("a", "hacan"), ("b", "sol")]);
    let c = content();
    assert!(crate::faction_abilities::secondary_is_free(&state, c, &a(), "trade"));
    assert!(
        !crate::faction_abilities::secondary_is_free(&state, c, &a(), "technology"),
        "only the Trade card"
    );
    assert!(
        !crate::faction_abilities::secondary_is_free(&state, c, &b(), "trade"),
        "a player without the ability pays"
    );
}

#[test]
fn guild_ships_1_hacan_negotiates_with_a_player_who_is_not_a_neighbour() {
    // Existing: faction_abilities::tests::guild_ships_makes_the_whole_table_a_partner.
    let state = seated(&[("a", "hacan"), ("b", "sol")]);
    let hub = crate::fixtures::plain_hub();
    assert!(crate::faction_abilities::ignores_neighbours(&state, content(), &a()));
    assert!(!crate::faction_abilities::ignores_neighbours(&state, content(), &b()));
    assert!(crate::transactions::partners(&state, content(), &hub.galaxy, &a()).contains(&b()));
    assert!(crate::transactions::partners(&state, content(), &hub.galaxy, &b()).is_empty());
}

#[test]
fn arbiters_1_action_cards_can_be_part_of_hacans_transactions() {
    let state = seated(&[("a", "hacan"), ("b", "sol")]);
    assert!(crate::faction_abilities::trades_action_cards(&state, content(), &a()));
    assert!(!crate::faction_abilities::trades_action_cards(&state, content(), &b()));
}

#[test]
fn hacanagent_1_exhausts_to_gain_two_commodities() {
    let mut state = seated(&[("a", "hacan"), ("b", "sol")]);
    let agent = set_leader(&mut state, &a(), "hacanagent", LeaderStatus::Readied);
    state.player_mut(&a()).unwrap().commodities = 0;
    assert!(use_leader(&mut state, &agent, &["self"]));
    let seat = state.player(&a()).unwrap();
    assert_eq!(seat.commodities, 2);
    assert_eq!(seat.leaders.get(&agent), Some(&LeaderStatus::Exhausted));
    assert!(
        !use_leader(&mut state, &agent, &["self"]),
        "an exhausted agent cannot be used again"
    );
}

#[test]
fn hacancommander_1_unlocks_with_ten_trade_goods() {
    let mut state = seated(&[("a", "hacan"), ("b", "sol")]);
    let cmd = LeaderId::new("hacancommander");
    state.player_mut(&a()).unwrap().trade_goods = 9;
    assert!(!unlocked(&state, &cmd, None));
    state.player_mut(&a()).unwrap().trade_goods = 10;
    assert!(unlocked(&state, &cmd, None));
}

// -- Jol-Nar -------------------------------------------------------------------------------------

#[test]
fn fragile_1_jolnar_units_hit_on_a_higher_number_than_the_same_unit_of_another_player() {
    let state = seated(&[("a", "jolnar"), ("b", "sol")]);
    let unit = |who: &PlayerId| {
        ti4_model::units::Unit::new(ti4_model::id::UnitTypeId::new("cruiser"), who.clone())
    };
    let jolnar = crate::combat::effective_hits_on(&state, content(), POK, &a(), &unit(&a())).unwrap();
    let sol = crate::combat::effective_hits_on(&state, content(), POK, &b(), &unit(&b())).unwrap();
    assert_eq!(jolnar, sol + 1, "-1 to the roll is +1 to the number needed");
}

#[test]
fn brilliant_1_jolnar_resolves_the_technology_primary_in_place_of_the_secondary() {
    let state = seated(&[("a", "jolnar"), ("b", "sol")]);
    let c = content();
    assert!(crate::faction_abilities::substitutes_primary(&state, c, &a(), "technology"));
    assert!(!crate::faction_abilities::substitutes_primary(&state, c, &a(), "trade"));
    assert!(!crate::faction_abilities::substitutes_primary(&state, c, &b(), "technology"));
}

#[test]
fn jolnarcommander_1_unlocks_with_eight_technologies() {
    let mut state = seated(&[("a", "jolnar"), ("b", "sol")]);
    let cmd = LeaderId::new("jolnarcommander");
    state.player_mut(&a()).unwrap().technologies.clear();
    for tech in ["amd", "gd", "fl", "lwd", "det", "nm", "dxa"] {
        super::give(&mut state, "a", tech);
    }
    assert!(!unlocked(&state, &cmd, None), "seven technologies");
    super::give(&mut state, "a", "hm");
    assert!(unlocked(&state, &cmd, None), "eight technologies");
}

// -- L1Z1X ---------------------------------------------------------------------------------------

#[test]
fn l1z1xcommander_1_unlocks_with_four_dreadnoughts_on_the_board() {
    let mut state = seated(&[("a", "l1z1x"), ("b", "sol")]);
    let cmd = LeaderId::new("l1z1xcommander");
    state.board.clear();
    let system = SystemId::new("27");
    put(&mut state, &system, "dreadnought", &a(), 3);
    assert!(!unlocked(&state, &cmd, None));
    put(&mut state, &SystemId::new("28"), "dreadnought", &b(), 1);
    assert!(!unlocked(&state, &cmd, None), "another player's do not count");
    put(&mut state, &SystemId::new("28"), "dreadnought", &a(), 1);
    assert!(unlocked(&state, &cmd, None));
}

#[test]
fn l1z1xcommander_2_bombardment_ignores_planetary_shield_only_once_unlocked() {
    let mut state = seated(&[("a", "l1z1x"), ("b", "sol")]);
    let (system, planet) = crate::fixtures::a_placed_planet();
    put_on_planet(&mut state, &system, &planet, "pds", &b(), 1);
    put(&mut state, &system, "dreadnought", &a(), 1);
    let can = |state: &GameState| crate::invasion::bombardable(state, content(), POK, &system, &planet, &a());
    set_leader(&mut state, &a(), "l1z1xcommander", LeaderStatus::Locked);
    assert!(!can(&state), "the PDS shields the planet");
    set_leader(&mut state, &a(), "l1z1xcommander", LeaderStatus::Unlocked);
    assert!(can(&state), "an unlocked commander ignores PLANETARY SHIELD");
}

// -- Letnev --------------------------------------------------------------------------------------

#[test]
fn letnevcommander_2_a_sustain_pays_a_trade_good_only_once_unlocked() {
    for (status, gain) in [(LeaderStatus::Locked, 0), (LeaderStatus::Unlocked, 1)] {
        let mut state = seated(&[("a", "letnev"), ("b", "sol")]);
        let system = SystemId::new("27");
        put(&mut state, &system, "dreadnought", &a(), 1);
        set_leader(&mut state, &a(), "letnevcommander", status);
        let before = state.player(&a()).unwrap().trade_goods;
        let mut table = crate::choice::Table::with_default(Box::new(crate::choice::FirstOption));
        crate::combat::absorb_hits(&mut state, content(), POK, &mut table, &a(), &system, &b(), 1)
            .unwrap();
        assert!(
            state.system_state(&system).units[0].sustained_damage,
            "the dreadnought sustained"
        );
        assert_eq!(state.player(&a()).unwrap().trade_goods, before + gain, "{status:?}");
    }
}

// -- Naalu ---------------------------------------------------------------------------------------

#[test]
fn naalucommander_1_unlocks_with_ground_forces_in_or_adjacent_to_mecatol() {
    let mut state = seated(&[("a", "naalu"), ("b", "sol")]);
    let hub = crate::fixtures::plain_hub();
    let cmd = LeaderId::new("naalucommander");
    assert!(!unlocked(&state, &cmd, Some(&hub.galaxy)));
    let mecatol = SystemId::new(crate::seating::mecatol_in_galaxy(&hub.galaxy));
    put_on_planet(&mut state, &mecatol, &PlanetId::new("mecatolrex"), "infantry", &a(), 1);
    assert!(unlocked(&state, &cmd, Some(&hub.galaxy)));
}

// -- Mentak --------------------------------------------------------------------------------------

#[test]
fn pop_1_action_places_promise_of_protection_faceup_in_the_holders_play_area() {
    let mut state = seated(&[("a", "mentak"), ("b", "sol")]);
    let note = crate::promissory::note_id("pop", "mentak");
    crate::promissory::take(&mut state, content(), &b(), &note);
    assert!(
        crate::promissory::action_notes_in_hand(&state, &b()).contains(&note),
        "the holder can play it with an ACTION"
    );
    assert!(crate::promissory::play_action_note(&mut state, &b(), &note));
    assert!(state.promissory_faceup.contains(&note));
}
