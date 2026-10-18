//! Tests of the faction effects gated by `RULES_FACTION_EFFECTS` (rules version 16): the new
//! behaviour at the current version, the old one below it, and the edge cases.

use ti4_model::id::{BreakthroughId, LeaderId, PlayerId, SystemId};
use ti4_model::state::{GameState, LeaderStatus, RULES_FACTION_EFFECTS};

use super::{ALL, content, emit_full, pid, scripted, seated_all, strs};
use crate::fixtures::{put, put_on_planet};

const OLD: u32 = RULES_FACTION_EFFECTS - 1;

fn a() -> PlayerId {
    pid("a")
}

fn b() -> PlayerId {
    pid("b")
}

fn set_leader(state: &mut GameState, who: &PlayerId, leader: &str, status: LeaderStatus) {
    state
        .player_mut(who)
        .expect("seat")
        .leaders
        .insert(LeaderId::new(leader), status);
}

// -- Gravleash Maneuvers ---------------------------------------------------------------------------

/// Hits from one roll of `ships` (type, count) for a Letnev with the card, with these faces.
fn letnev_hits(version: u32, card: bool, ships: &[(&str, usize)], faces: &[u32]) -> usize {
    let mut state = seated_all(&[("a", "letnev"), ("b", "sol")]);
    state.rules_version = version;
    if card {
        state.player_mut(&a()).unwrap().breakthrough = Some(BreakthroughId::new("letnevbt"));
    }
    let system = SystemId::new(crate::fixtures::plain_systems(1)[0].clone());
    for (kind, count) in ships {
        put(&mut state, &system, kind, &a(), *count);
    }
    let mut dice = crate::dice::Dice::from_faces(faces.iter().copied());
    let mut rng = crate::rng::GameRng::new(0);
    crate::combat::roll_fleet(&state, content(), ALL, &mut dice, &mut rng, &a(), &system)
}

#[test]
fn gravleash_combat_bonus_is_off_below_version_16() {
    let old = letnev_hits(OLD, true, &[("cruiser", 1), ("destroyer", 1)], &[6, 8]);
    assert_eq!(old, 0, "no +X before version 16");
}

#[test]
fn gravleash_adds_one_for_a_single_ship_type() {
    // Two cruisers (hit on 7): X = 1, one die rises from 6 to 7 and hits, the other stays at 6.
    assert_eq!(
        letnev_hits(RULES_FACTION_EFFECTS, true, &[("cruiser", 2)], &[6, 6]),
        1
    );
    assert_eq!(
        letnev_hits(RULES_FACTION_EFFECTS, false, &[("cruiser", 2)], &[6, 6]),
        0
    );
}

#[test]
fn gravleash_applies_to_exactly_one_roll_even_with_several_dice_and_ties() {
    // Three ship types (X = 3): cruiser 7, destroyer 9, dreadnought 5. Every die is a near miss
    // for X = 3 (4, 6, 8 would hit all of them if the bonus were per ship), so exactly one hits.
    let hits = letnev_hits(
        RULES_FACTION_EFFECTS,
        true,
        &[("cruiser", 1), ("destroyer", 1), ("dreadnought", 1)],
        &[4, 4, 8],
    );
    assert_eq!(hits, 1);
}

#[test]
fn gravleash_changes_nothing_without_ships_or_when_a_die_already_hits() {
    assert_eq!(letnev_hits(RULES_FACTION_EFFECTS, true, &[], &[]), 0);
    // A die that already hits is not made to hit twice.
    assert_eq!(
        letnev_hits(RULES_FACTION_EFFECTS, true, &[("cruiser", 1)], &[10]),
        1
    );
}

#[test]
fn gravleash_applies_again_in_every_round() {
    let first = letnev_hits(
        RULES_FACTION_EFFECTS,
        true,
        &[("cruiser", 1), ("destroyer", 1)],
        &[6, 8],
    );
    let second = letnev_hits(
        RULES_FACTION_EFFECTS,
        true,
        &[("cruiser", 1), ("destroyer", 1)],
        &[6, 8],
    );
    assert_eq!((first, second), (1, 1));
}

#[test]
fn gravleash_is_not_given_to_a_seat_without_the_card() {
    let mut state = seated_all(&[("a", "letnev"), ("b", "sol")]);
    let system = SystemId::new(crate::fixtures::plain_systems(30).pop().unwrap());
    assert!(state.system_state(&system).units.is_empty());
    put(&mut state, &system, "cruiser", &b(), 1);
    put(&mut state, &system, "destroyer", &b(), 1);
    state.player_mut(&a()).unwrap().breakthrough = Some(BreakthroughId::new("letnevbt"));
    let mut dice = crate::dice::Dice::from_faces([6, 8]);
    let mut rng = crate::rng::GameRng::new(0);
    let hits =
        crate::combat::roll_fleet(&state, content(), ALL, &mut dice, &mut rng, &b(), &system);
    assert_eq!(
        hits, 0,
        "another seat's fleet gets nothing from the Letnev's card"
    );
}

// -- Sol agent -------------------------------------------------------------------------------------

fn ground_dice(version: u32, mark: bool) -> usize {
    let mut state = seated_all(&[("a", "sol"), ("b", "hacan")]);
    state.rules_version = version;
    let (system, planet) = crate::fixtures::a_placed_planet();
    put_on_planet(&mut state, &system, &planet, "sol_infantry", &a(), 2);
    put_on_planet(&mut state, &system, &planet, "infantry", &b(), 2);
    if mark {
        let seat = state.player_mut(&a()).unwrap();
        seat.extra_die_round = Some(1);
        seat.extra_die_unit = Some(ti4_model::id::UnitTypeId::new("sol_infantry"));
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
fn sol_agent_extra_die_is_not_read_below_version_16() {
    assert_eq!(ground_dice(OLD, true), ground_dice(OLD, false));
    assert_eq!(
        ground_dice(RULES_FACTION_EFFECTS, true),
        ground_dice(RULES_FACTION_EFFECTS, false) + 1
    );
}

#[test]
fn sol_agent_extra_die_belongs_to_one_round_only() {
    let mut state = seated_all(&[("a", "sol"), ("b", "hacan")]);
    let (system, planet) = crate::fixtures::a_placed_planet();
    put_on_planet(&mut state, &system, &planet, "sol_infantry", &a(), 3);
    put_on_planet(&mut state, &system, &planet, "infantry", &b(), 3);
    let seat = state.player_mut(&a()).unwrap();
    seat.extra_die_round = Some(1);
    seat.extra_die_unit = Some(ti4_model::id::UnitTypeId::new("sol_infantry"));
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
    let rolled: Vec<usize> = dice.history().iter().map(|roll| roll.faces.len()).collect();
    // Round 1 (invader first): 3 + 1 for a, 3 for b; the later rounds have no extra die.
    assert_eq!(rolled[0], 4);
    assert_eq!(rolled[1], 3);
    assert!(
        rolled.iter().skip(2).step_by(2).all(|dice| *dice <= 3),
        "{rolled:?}"
    );
}

fn round_event(
    state: &GameState,
    planet: &str,
    system: &str,
) -> std::collections::BTreeMap<String, serde_json::Value> {
    let mut payload = strs(&[
        ("system", system),
        ("planet", planet),
        ("attacker", "b"),
        ("defender", "a"),
    ]);
    payload.insert(
        "round_seq".to_owned(),
        i64::from(state.combat_round_seq).into(),
    );
    payload
}

fn sol_round_game(version: u32) -> (GameState, SystemId, ti4_model::id::PlanetId) {
    let mut state = seated_all(&[("a", "sol"), ("b", "hacan")]);
    state.rules_version = version;
    let (system, planet) = crate::fixtures::a_placed_planet();
    put_on_planet(&mut state, &system, &planet, "sol_infantry", &a(), 2);
    put_on_planet(&mut state, &system, &planet, "infantry", &b(), 2);
    state.combat_round_seq = 3;
    (state, system, planet)
}

const SOL_WINDOW: &str = "leader:sol:solagent:GROUND_COMBAT_ROUND_STARTED:after";

#[test]
fn the_sol_owner_chooses_a_ground_force_at_the_start_of_a_ground_round() {
    let (mut state, system, planet) = sol_round_game(RULES_FACTION_EFFECTS);
    let payload = round_event(&state, planet.as_str(), system.as_str());
    let pick = format!("a|{planet}|sol_infantry|1|0");
    emit_full(
        &mut state,
        &mut scripted(&[SOL_WINDOW, &pick]),
        ALL,
        "GROUND_COMBAT_ROUND_STARTED",
        payload,
    );
    assert_eq!(
        crate::leaders::status(&state, &a(), &LeaderId::new("solagent")),
        Some(LeaderStatus::Exhausted)
    );
    let bonus = |index| {
        crate::factions::borrowed_round_agents::extra_die_for(
            &state,
            content(),
            ALL,
            &system,
            Some(&planet),
            &a(),
            "sol_infantry",
            index,
            3,
        )
    };
    assert_eq!(
        (bonus(0), bonus(1)),
        (0, 1),
        "the chosen force, and only it"
    );
    assert_eq!(
        crate::factions::borrowed_round_agents::extra_die_for(
            &state,
            content(),
            ALL,
            &system,
            Some(&planet),
            &a(),
            "sol_infantry",
            1,
            4,
        ),
        0,
        "the next round it is gone"
    );
}

#[test]
fn the_sol_agent_may_be_declined_and_is_not_offered_twice() {
    let (mut state, system, planet) = sol_round_game(RULES_FACTION_EFFECTS);
    let payload = round_event(&state, planet.as_str(), system.as_str());
    emit_full(
        &mut state,
        &mut scripted(&["decline"]),
        ALL,
        "GROUND_COMBAT_ROUND_STARTED",
        payload.clone(),
    );
    assert_eq!(
        crate::leaders::status(&state, &a(), &LeaderId::new("solagent")),
        Some(LeaderStatus::Readied)
    );
    // Exhausted: the window opens with no question (an unscripted question would fail).
    set_leader(&mut state, &a(), "solagent", LeaderStatus::Exhausted);
    emit_full(
        &mut state,
        &mut scripted(&[]),
        ALL,
        "GROUND_COMBAT_ROUND_STARTED",
        payload,
    );
}

#[test]
fn the_sol_agent_has_no_ground_round_window_below_version_16() {
    let (mut state, system, planet) = sol_round_game(OLD);
    let payload = round_event(&state, planet.as_str(), system.as_str());
    emit_full(
        &mut state,
        &mut scripted(&[]),
        ALL,
        "GROUND_COMBAT_ROUND_STARTED",
        payload,
    );
    assert_eq!(
        crate::leaders::status(&state, &a(), &LeaderId::new("solagent")),
        Some(LeaderStatus::Readied)
    );
}

// -- Promise of Protection -------------------------------------------------------------------------

fn pop_game(version: u32) -> (GameState, String) {
    let mut state = seated_all(&[("a", "mentak"), ("b", "sol")]);
    state.rules_version = version;
    let note = crate::promissory::note_id("pop", "mentak");
    crate::promissory::take(&mut state, content(), &b(), &note);
    (state, note)
}

#[test]
fn pop_goes_faceup_on_receipt_below_version_16() {
    let (state, note) = pop_game(OLD);
    assert!(state.promissory_faceup.contains(&note));
    assert!(crate::promissory::action_notes_in_hand(&state, &b()).is_empty());
}

#[test]
fn pop_waits_in_hand_for_its_action_and_the_holder_is_offered_it() {
    let (mut state, note) = pop_game(RULES_FACTION_EFFECTS);
    assert!(!state.promissory_faceup.contains(&note));
    let offered = crate::factions::component_actions(&state, content(), &b());
    assert!(
        offered
            .iter()
            .any(|option| option.id == "faction|mentak|pop")
    );
    // Not to the Mentak, who holds the note's home copy only.
    assert!(
        !crate::factions::component_actions(&state, content(), &a())
            .iter()
            .any(|option| option.id == "faction|mentak|pop")
    );
    let option = offered
        .into_iter()
        .find(|option| option.id == "faction|mentak|pop")
        .unwrap();
    let done = crate::fixtures::with_context(&mut state, ALL, None, &mut scripted(&[]), |ctx| {
        crate::factions::perform_component(ctx, &b(), &option)
    });
    assert!(done);
    assert!(state.promissory_faceup.contains(&note));
    assert_eq!(
        state.promissory_notes.get(&note),
        Some(&b()),
        "still the holder's, in play"
    );
    // Played once: no longer offered.
    assert!(
        !crate::factions::component_actions(&state, content(), &b())
            .iter()
            .any(|option| option.id == "faction|mentak|pop")
    );
}

#[test]
fn a_faceup_pop_returns_to_the_mentak_when_its_holder_activates_their_system() {
    let (mut state, note) = pop_game(RULES_FACTION_EFFECTS);
    assert!(crate::promissory::play_action_note(&mut state, &b(), &note));
    let system = SystemId::new(crate::fixtures::plain_systems(1)[0].clone());
    put(&mut state, &system, "destroyer", &a(), 1);
    emit_full(
        &mut state,
        &mut scripted(&[]),
        ALL,
        "SYSTEM_ACTIVATED",
        strs(&[("player", "b"), ("system", system.as_str())]),
    );
    assert_eq!(state.promissory_notes.get(&note), Some(&a()));
    assert!(!state.promissory_faceup.contains(&note));
}

#[test]
fn an_unplayed_pop_does_not_return_on_activation_and_does_not_protect() {
    let (mut state, note) = pop_game(RULES_FACTION_EFFECTS);
    let system = SystemId::new(crate::fixtures::plain_systems(1)[0].clone());
    put(&mut state, &system, "destroyer", &a(), 1);
    emit_full(
        &mut state,
        &mut scripted(&[]),
        ALL,
        "SYSTEM_ACTIVATED",
        strs(&[("player", "b"), ("system", system.as_str())]),
    );
    assert_eq!(
        state.promissory_notes.get(&note),
        Some(&b()),
        "in hand: nothing returns"
    );
}

// -- Watchful Ojz ----------------------------------------------------------------------------------

fn ralnel_game(version: u32) -> GameState {
    let mut state = seated_all(&[("a", "ralnel"), ("b", "sol")]);
    state.rules_version = version;
    state
}

fn last_to_pass(state: &mut GameState, who: &PlayerId) {
    let occurrence = state.begin_feat_occurrence();
    state.record_event_feat(who, ti4_model::state::Feat::LastToPass, occurrence);
}

#[test]
fn watchful_ojz_stays_locked_below_version_16() {
    let mut state = ralnel_game(OLD);
    last_to_pass(&mut state, &a());
    crate::leaders::check_unlocks(&mut state, content(), ALL, None, &a());
    assert_eq!(
        crate::leaders::status(&state, &a(), &LeaderId::new("ralnelcommander")),
        Some(LeaderStatus::Locked)
    );
}

#[test]
fn watchful_ojz_unlocks_only_for_the_last_to_pass_and_only_once() {
    let mut state = ralnel_game(RULES_FACTION_EFFECTS);
    let id = LeaderId::new("ralnelcommander");
    assert!(crate::leaders::check_unlocks(&mut state, content(), ALL, None, &a()).is_empty());
    // Another seat being last does not open this one.
    last_to_pass(&mut state, &b());
    assert!(crate::leaders::check_unlocks(&mut state, content(), ALL, None, &a()).is_empty());
    last_to_pass(&mut state, &a());
    assert_eq!(
        crate::leaders::check_unlocks(&mut state, content(), ALL, None, &a()),
        vec![id.clone()]
    );
    assert!(
        crate::leaders::check_unlocks(&mut state, content(), ALL, None, &a()).is_empty(),
        "reported once"
    );
    assert_eq!(
        crate::leaders::status(&state, &a(), &id),
        Some(LeaderStatus::Unlocked)
    );
}

// -- Rear Admiral Farran -----------------------------------------------------------------------------

fn farran_gain(version: u32, answers: &[&str]) -> i32 {
    let mut state = seated_all(&[("a", "letnev"), ("b", "sol")]);
    state.rules_version = version;
    let system = SystemId::new("27");
    put(&mut state, &system, "dreadnought", &a(), 1);
    set_leader(&mut state, &a(), "letnevcommander", LeaderStatus::Unlocked);
    let before = state.player(&a()).unwrap().trade_goods;
    let mut table = scripted(answers);
    crate::combat::absorb_hits(
        &mut state,
        content(),
        ALL,
        &mut table,
        &a(),
        &system,
        &b(),
        1,
    )
    .unwrap();
    assert!(
        state.system_state(&system).units[0].sustained_damage,
        "the dreadnought sustained"
    );
    i32::try_from(state.player(&a()).unwrap().trade_goods - before).unwrap()
}

#[test]
fn farran_pays_automatically_below_version_16() {
    assert_eq!(farran_gain(OLD, &["sustain|space:0"]), 1);
}

#[test]
fn farran_trade_good_is_a_choice_from_version_16() {
    assert_eq!(
        farran_gain(RULES_FACTION_EFFECTS, &["sustain|space:0", "gain"]),
        1
    );
    assert_eq!(
        farran_gain(RULES_FACTION_EFFECTS, &["sustain|space:0", "decline"]),
        0
    );
}
