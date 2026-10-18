//! Technology effect tests, group g1: generic (non-faction) technologies (see mod.rs).

use super::{content, give, pid, seated};
use crate::choice::{FirstOption, Scripted, Table, Window as _};
use crate::fixtures::{game, hub_with_centre, plain_hub, put, put_on_planet};
use crate::game::{END_TURN_ID, Game, TACTICAL_ACTION_ID};
use ti4_model::content_types::POK;
use ti4_model::id::{PlanetId, SystemId, TechnologyId};
use ti4_model::state::{GameState, Phase};

/// A two-seat game in the action phase with a seven-system map; seat `a` is active.
fn tactical_fixture() -> (GameState, ti4_content::galaxy::Galaxy, Vec<SystemId>) {
    let mut state = game(&["a", "b"]);
    let ids = crate::fixtures::plain_systems(7);
    let refs: Vec<&str> = ids.iter().map(String::as_str).collect();
    let galaxy = ti4_content::galaxy::Galaxy::build(content(), &refs, POK, 1).unwrap();
    state.phase = Phase::Action;
    state.active = Some(pid("a"));
    (state, galaxy, ids.into_iter().map(SystemId::new).collect())
}

/// Movement rules for `mover` towards `target` on `hub`, with the mover's technology effects.
fn rules_for<'a>(
    hub: &'a crate::fixtures::Hub,
    state: &GameState,
    mover: &str,
    target: &str,
) -> crate::movement::MovementRules<'a> {
    let board = crate::movement::Board::for_player(state, content(), POK, &pid(mover));
    let mut rules = crate::movement::MovementRules::with_laws(
        &hub.galaxy,
        content(),
        POK,
        target,
        board,
        Some(state),
    );
    crate::action_cards::apply_movement_effects(&mut rules, state, &pid(mover));
    rules
}

// ---- Propulsion (blue) ----------------------------------------------------------------------

#[test]
fn amd_1_ships_move_into_and_through_asteroid_fields() {
    let asteroid = crate::fixtures::a_system_where("asteroid field");
    let hub = hub_with_centre(&asteroid);
    let mut state = game(&["a", "b"]);
    let near = hub.outer[0].clone();
    let far = hub.across(&near);
    assert!(!rules_for(&hub, &state, "a", &far).can_enter(&asteroid));
    give(&mut state, "a", "amd");
    let rules = rules_for(&hub, &state, "a", &far);
    assert!(rules.can_enter(&asteroid));
    assert!(
        rules.can_reach(&near, 2),
        "through the field to the far side"
    );
    assert!(
        !rules_for(&hub, &state, "b", &far).can_enter(&asteroid),
        "only the owner"
    );
}

#[test]
fn amd_2_space_cannon_against_the_owner_gets_minus_one() {
    // A PDS hits on 6: a rolled 6 hits the target, but with the -1 it is a 5 and misses.
    let hits_at = |owner: bool, version: u32| {
        let (mut state, _galaxy, ids) = tactical_fixture();
        state.rules_version = version;
        let system = ids[0].clone();
        put(&mut state, &system, "cruiser", &pid("a"), 1);
        put_on_planet(
            &mut state,
            &system,
            &ti4_model::id::PlanetId::new("p"),
            "pds",
            &pid("b"),
            1,
        );
        if owner {
            give(&mut state, "a", "amd");
        }
        let mut dice = crate::dice::Dice::from_faces([6u32]);
        let mut rng = crate::rng::GameRng::new(1);
        let fired = crate::combat::space_cannon_offense(
            &mut state,
            content(),
            POK,
            &mut dice,
            &mut rng,
            &system,
            &pid("a"),
            None,
        );
        fired.iter().map(|(_, hits, _)| *hits).sum::<usize>()
    };
    let now = ti4_model::state::RULES_TECH_EFFECTS;
    assert_eq!(hits_at(false, now), 1, "control: a 6 hits a PDS target");
    assert_eq!(
        hits_at(true, now),
        0,
        "Antimass Deflectors: the 6 becomes a 5"
    );
    assert_eq!(
        hits_at(true, now - 1),
        1,
        "legacy: no -1 before the rules version"
    );
}

#[test]
fn gd_1_adds_one_to_one_ships_move_once_per_activation() {
    let mut state = game(&["a"]);
    let a = pid("a");
    state.activation_seq = 3;
    assert!(
        !crate::technology::gravity_drive_available(&state, &a),
        "needs the card"
    );
    give(&mut state, "a", "gd");
    assert!(crate::technology::gravity_drive_available(&state, &a));
    let types = ti4_content::units::catalogue(content(), POK);
    let cruiser = types.get("cruiser").unwrap();
    let origin = SystemId::new("19");
    let plain = crate::tactical::effective_move_value_for_ship(
        &state, cruiser, &a, &origin, None, false, false,
    );
    let boosted = crate::tactical::effective_move_value_for_ship(
        &state, cruiser, &a, &origin, None, true, false,
    );
    assert_eq!(boosted, plain + 1);
    assert!(crate::technology::use_gravity_drive(&mut state, &a));
    assert!(
        !crate::technology::gravity_drive_available(&state, &a),
        "once per tactical action"
    );
    state.activation_seq = 4;
    assert!(
        crate::technology::gravity_drive_available(&state, &a),
        "renews with the next activation"
    );
}

#[test]
fn gd_1_a_ship_one_short_is_offered_with_the_boost_only_for_the_owner() {
    // A carrier (move 1) two systems from the target is out of reach until Gravity Drive.
    let hub = plain_hub();
    let mut state = game(&["a", "b"]);
    let a = pid("a");
    let from = SystemId::new(hub.outer[0].clone());
    let target = SystemId::new(
        hub.outer
            .iter()
            .find(|id| hub.galaxy.distance(id, &hub.outer[0]) == Some(2))
            .cloned()
            .unwrap(),
    );
    put(&mut state, &from, "carrier", &a, 1);
    let movable = |state: &GameState| {
        crate::tactical::movable_into(state, content(), POK, &hub.galaxy, &a, &target)
    };
    assert!(movable(&state).is_empty(), "out of reach without the drive");
    give(&mut state, "a", "gd");
    let found = movable(&state);
    assert_eq!(found.len(), 1);
    assert!(found[0].gravity_drive, "reachable only with the +1");
}

#[test]
fn fl_1_two_actions_in_one_turn() {
    let run = |owner: bool| {
        let (mut state, galaxy, ids) = tactical_fixture();
        if owner {
            give(&mut state, "a", "fl");
        }
        let script = [
            TACTICAL_ACTION_ID.to_owned(),
            ids[0].to_string(),
            "done_moving".to_owned(),
        ];
        let table = Table::with_default(Box::new(Scripted::new(script)));
        let mut game = Game::with_table(state, content(), table).with_galaxy(galaxy);
        let mut second_offered = false;
        for _ in 0..30 {
            if let Some(choice) = game.legal_options()
                && choice.options.first().is_some_and(|o| o.id == END_TURN_ID)
            {
                second_offered = choice.ids().contains(&TACTICAL_ACTION_ID);
                break;
            }
            assert_eq!(game.step().error, None);
        }
        second_offered
    };
    assert!(run(true), "Fleet Logistics offers a second action");
    assert!(!run(false), "without it the turn ends after one");
}

#[test]
fn lwd_1_ships_move_through_systems_with_other_players_ships() {
    let hub = plain_hub();
    let mut state = game(&["a", "b"]);
    let near = hub.outer[0].clone();
    let far = hub.across(&near);
    put(
        &mut state,
        &SystemId::new(hub.centre.clone()),
        "cruiser",
        &pid("b"),
        1,
    );
    assert!(!rules_for(&hub, &state, "a", &far).can_reach(&near, 2));
    give(&mut state, "a", "lwd");
    assert!(rules_for(&hub, &state, "a", &far).can_reach(&near, 2));
}

#[test]
fn det_1_a_tactical_action_in_a_frontier_system_explores_it() {
    let run = |owner: bool| {
        let (mut state, galaxy, ids) = tactical_fixture();
        if owner {
            give(&mut state, "a", "det");
        }
        put(&mut state, &ids[0], "carrier", &pid("a"), 1);
        let table = Table::with_default(Box::new(Scripted::new([
            TACTICAL_ACTION_ID.to_owned(),
            ids[0].to_string(),
            "done_moving".to_owned(),
        ])));
        let mut game = Game::with_table(state, content(), table).with_galaxy(galaxy);
        game.state.frontier_tokens.insert(ids[0].clone());
        for _ in 0..40 {
            assert_eq!(game.step().error, None);
            if game.events.iter().any(|e| e == "TACTICAL_ACTION_COMPLETE") {
                break;
            }
        }
        game.events.iter().any(|e| e == "FRONTIER_EXPLORED")
    };
    assert!(run(true));
    assert!(!run(false));
}

#[test]
fn det_2_ships_retreat_into_adjacent_systems_without_enemies_or_own_presence() {
    let hub = plain_hub();
    let state = {
        let mut s = game(&["a", "b"]);
        put(
            &mut s,
            &SystemId::new(hub.centre.clone()),
            "cruiser",
            &pid("a"),
            1,
        );
        s
    };
    let centre = SystemId::new(hub.centre.clone());
    let empty = SystemId::new(hub.outer[0].clone());
    let retreats = |state: &GameState| {
        crate::combat::eligible_retreats(state, content(), POK, &hub.galaxy, &pid("a"), &centre)
    };
    assert!(
        !retreats(&state).contains(&empty),
        "78.7c needs your units or a planet there"
    );
    let det = crate::combat::det_retreat_destinations(
        &state,
        content(),
        POK,
        &hub.galaxy,
        &pid("a"),
        &centre,
    );
    assert!(
        det.contains(&empty),
        "Dark Energy Tap's looser set includes it"
    );
    let mut enemy = state.clone();
    put(&mut enemy, &empty, "cruiser", &pid("b"), 1);
    let det = crate::combat::det_retreat_destinations(
        &enemy,
        content(),
        POK,
        &hub.galaxy,
        &pid("a"),
        &centre,
    );
    assert!(!det.contains(&empty), "other players' units still close it");
}

#[test]
fn sr_1_action_produces_one_ship_at_a_space_dock() {
    let mut state = seated(&[("a", "arborec"), ("b", "letnev")]);
    let a = pid("a");
    let home = state.player(&a).unwrap().home_system.clone().unwrap();
    state.player_mut(&a).unwrap().trade_goods = 10;
    let ships = |state: &GameState| state.system_state(&home).units_of(&a).len();
    let before = ships(&state);
    assert!(
        crate::technology::component_actions(&state, content(), POK, &a).is_empty(),
        "no card, no action"
    );
    give(&mut state, "a", "sr");
    let actions = crate::technology::component_actions(&state, content(), POK, &a);
    assert_eq!(actions.len(), 1);
    let mut table = Table::with_default(Box::new(FirstOption));
    let done = crate::technology::perform_component(
        &mut state,
        content(),
        POK,
        None,
        &mut table,
        &a,
        &actions[0],
    )
    .unwrap();
    assert!(done);
    assert_eq!(ships(&state), before + 1);
    assert!(
        state
            .player(&a)
            .unwrap()
            .exhausted_technologies
            .contains(&TechnologyId::new("sr"))
    );
    assert!(
        crate::technology::component_actions(&state, content(), POK, &a).is_empty(),
        "exhausted"
    );
}

// ---- Biotic (green) -------------------------------------------------------------------------

/// A placed planet carrying a technology specialty of `colour` (lower case, as the corpus has it).
fn specialty_planet(colour: &str) -> (SystemId, PlanetId) {
    ti4_content::galaxy::all_planets(content(), POK)
        .iter()
        .find(|(_, planet)| {
            planet.system_id().is_some()
                && !planet.is_placed_during_play()
                && planet
                    .tech_specialties()
                    .iter()
                    .any(|s| s.eq_ignore_ascii_case(colour))
        })
        .map(|(id, planet)| {
            (
                SystemId::new(planet.system_id().unwrap()),
                PlanetId::new(*id),
            )
        })
        .expect("the corpus has such a specialty planet")
}

/// Seat `a` controls `planet` in `system`.
fn control(state: &mut GameState, system: &SystemId, planet: &PlanetId) {
    state
        .system_mut(system)
        .set_control(planet.clone(), pid("a"));
}

fn scripted(answers: &[&str]) -> Table {
    Table::with_default(Box::new(Scripted::new(
        answers.iter().map(|a| (*a).to_owned()),
    )))
}

#[test]
fn nm_1_draws_two_action_cards_in_the_status_phase() {
    let draws = |owner: bool| {
        let mut state = game(&["a", "b"]);
        state.phase = Phase::Status;
        if owner {
            give(&mut state, "a", "nm");
        }
        let report = crate::status::resolve_status_phase(&mut state).unwrap();
        report
            .action_cards_drawn
            .iter()
            .find(|(p, _)| p == &pid("a"))
            .map(|(_, n)| *n)
    };
    assert_eq!(draws(true), Some(2));
    assert_eq!(draws(false), Some(1), "control: one card without it");
}

#[test]
fn dxa_1_the_winner_of_a_ground_combat_places_an_infantry() {
    let mut state = game(&["a"]);
    let winner = pid("a");
    let (system, planet) = crate::fixtures::a_placed_planet();
    assert!(!crate::faction_techs::dacxive_animators(
        &mut state,
        content(),
        POK,
        &winner,
        &system,
        &planet
    ));
    give(&mut state, "a", "dxa");
    assert!(crate::faction_techs::dacxive_animators(
        &mut state,
        content(),
        POK,
        &winner,
        &system,
        &planet
    ));
    assert_eq!(
        state
            .system_state(&system)
            .on_planet_of(&planet, &winner)
            .len(),
        1
    );
}

#[test]
fn hm_1_three_command_tokens_in_the_status_phase() {
    let mut state = game(&["a"]);
    let a = pid("a");
    let tokens =
        |state: &GameState| crate::faction_abilities::status_tokens(state, content(), &a, 2);
    assert_eq!(tokens(&state), 2);
    give(&mut state, "a", "hm");
    assert_eq!(tokens(&state), 3);
}

#[test]
fn pa_1_specialties_are_usable_without_exhausting_and_when_exhausted() {
    // Gravity Drive needs one blue; a controlled propulsion specialty stands in for it.
    let (system, planet) = specialty_planet("propulsion");
    let gd = TechnologyId::new("gd");
    let a = pid("a");
    let mut state = game(&["a"]);
    control(&mut state, &system, &planet);
    state.exhausted_planets.insert(planet.clone());
    assert!(
        !crate::technology::prerequisites_met(&state, content(), POK, &a, &gd),
        "control: exhausted, no pa"
    );
    give(&mut state, "a", "pa");
    assert!(
        crate::technology::prerequisites_met(&state, content(), POK, &a, &gd),
        "pa: an exhausted specialty still counts"
    );
    state.ready_planet(&planet);
    assert!(crate::technology::research(
        &mut state,
        content(),
        POK,
        &a,
        &gd
    ));
    assert!(
        !state.exhausted_planets.contains(&planet),
        "pa: using the specialty does not exhaust the planet"
    );
}

#[test]
fn pa_1_legacy_games_still_exhaust_and_skip_exhausted_specialties() {
    let (system, planet) = specialty_planet("propulsion");
    let gd = TechnologyId::new("gd");
    let a = pid("a");
    let mut state = game(&["a"]);
    state.rules_version = ti4_model::state::RULES_TECH_EFFECTS - 1;
    control(&mut state, &system, &planet);
    give(&mut state, "a", "pa");
    assert!(crate::technology::research(
        &mut state,
        content(),
        POK,
        &a,
        &gd
    ));
    assert!(
        state.exhausted_planets.contains(&planet),
        "legacy: the specialty is exhausted by the research"
    );
    let mut state = game(&["a"]);
    state.rules_version = ti4_model::state::RULES_TECH_EFFECTS - 1;
    control(&mut state, &system, &planet);
    state.exhausted_planets.insert(planet);
    give(&mut state, "a", "pa");
    assert!(!crate::technology::prerequisites_met(
        &state,
        content(),
        POK,
        &a,
        &gd
    ));
}

#[test]
fn pa_2_exhaust_a_specialty_planet_for_a_trade_good() {
    let (system, planet) = specialty_planet("biotic");
    let a = pid("a");
    let run = |owner: bool| {
        let mut state = game(&["a"]);
        control(&mut state, &system, &planet);
        if owner {
            give(&mut state, "a", "pa");
        }
        let before = state.player(&a).unwrap().trade_goods;
        let mut table = scripted(&[planet.as_str()]);
        crate::technology::start_turn(&mut state, content(), POK, None, &mut table, &a).unwrap();
        (
            state.player(&a).unwrap().trade_goods - before,
            state.exhausted_planets.contains(&planet),
        )
    };
    assert_eq!(run(true), (1, true));
    assert_eq!(run(false), (0, false));
}

#[test]
fn bs_1_ready_an_exhausted_specialty_planet_or_another_technology() {
    let (system, planet) = specialty_planet("biotic");
    let a = pid("a");
    // The planet branch.
    let mut state = game(&["a"]);
    control(&mut state, &system, &planet);
    state.exhausted_planets.insert(planet.clone());
    give(&mut state, "a", "bs");
    let mut table = scripted(&[&format!("ready|planet|{planet}")]);
    crate::technology::end_turn(&mut state, content(), POK, None, &mut table, &a).unwrap();
    assert!(!state.exhausted_planets.contains(&planet));
    assert!(
        state
            .player(&a)
            .unwrap()
            .exhausted_technologies
            .contains(&TechnologyId::new("bs")),
        "Bio-Stims exhausts itself"
    );
    // The technology branch.
    let mut state = game(&["a"]);
    give(&mut state, "a", "bs");
    give(&mut state, "a", "sr");
    state
        .player_mut(&a)
        .unwrap()
        .exhausted_technologies
        .insert(TechnologyId::new("sr"));
    let mut table = scripted(&["ready|technology|sr"]);
    crate::technology::end_turn(&mut state, content(), POK, None, &mut table, &a).unwrap();
    assert!(
        !state
            .player(&a)
            .unwrap()
            .exhausted_technologies
            .contains(&TechnologyId::new("sr"))
    );
    // Control: without the card nothing is offered.
    let mut state = game(&["a"]);
    state
        .player_mut(&a)
        .unwrap()
        .exhausted_technologies
        .insert(TechnologyId::new("sr"));
    crate::technology::end_turn(
        &mut state,
        content(),
        POK,
        None,
        &mut scripted(&["ready|technology|sr"]),
        &a,
    )
    .unwrap();
    assert!(
        state
            .player(&a)
            .unwrap()
            .exhausted_technologies
            .contains(&TechnologyId::new("sr"))
    );
}

/// A superseded printing is not in the active deck, so nobody can own it: nothing to implement.
fn assert_superseded(alias: &str) {
    let active = crate::technology::active_aliases(content(), POK);
    assert!(
        !active.contains(&TechnologyId::new(alias)),
        "{alias} is a superseded printing"
    );
    let mut state = game(&["a"]);
    for colour_tech in ["x89c4", "md"] {
        assert!(active.contains(&TechnologyId::new(colour_tech)));
    }
    let offered = crate::technology::researchable(&state, content(), POK, &pid("a"));
    assert!(!offered.contains(&TechnologyId::new(alias)));
    let _ = &mut state;
}

#[test]
fn x89_base_1_superseded_printing_is_not_in_play() {
    assert_superseded("x89_base");
}

#[test]
fn x89_1_superseded_printing_is_not_in_play() {
    assert_superseded("x89");
}

/// A defended planet and an invader bombarding it from space: `n_dice_faces` all hit.
fn bombard_kills(owner_tech: Option<&str>, ship: &str, faces: &[u32]) -> (usize, GameState) {
    let mut state = game(&["a", "b"]);
    let (system, planet) = crate::fixtures::a_placed_planet();
    put_on_planet(&mut state, &system, &planet, "infantry", &pid("b"), 4);
    put(&mut state, &system, ship, &pid("a"), 1);
    if let Some(tech) = owner_tech {
        give(&mut state, "a", tech);
    }
    let mut dice = crate::dice::Dice::from_faces(faces.iter().copied());
    let mut rng = crate::rng::GameRng::new(1);
    let mut table = Table::new();
    let killed = crate::invasion::bombardment(
        &mut state,
        content(),
        POK,
        &mut dice,
        &mut rng,
        &mut table,
        &system,
        &pid("a"),
    )
    .unwrap();
    (killed, state)
}

#[test]
fn x89c4_1_doubles_the_hits_of_bombardment_and_ground_combat() {
    // One dreadnought die, one hit: two infantry die with the card, one without.
    assert_eq!(bombard_kills(None, "dreadnought", &[10]).0, 1);
    assert_eq!(bombard_kills(Some("x89c4"), "dreadnought", &[10]).0, 2);
}

#[test]
fn x89c4_2_the_bombarded_planet_is_exhausted() {
    let (_, planet) = crate::fixtures::a_placed_planet();
    let (_, with) = bombard_kills(Some("x89c4"), "dreadnought", &[10]);
    assert!(with.exhausted_planets.contains(&planet));
    let (_, without) = bombard_kills(None, "dreadnought", &[10]);
    assert!(!without.exhausted_planets.contains(&planet), "control");
}

#[test]
fn ps_1_one_unit_rolls_an_extra_die_when_bombarding_or_firing_space_cannon() {
    // Bombardment: a single dreadnought rolls 1 die, 2 with Plasma Scoring.
    assert_eq!(bombard_kills(None, "dreadnought", &[10, 10]).0, 1);
    assert_eq!(bombard_kills(Some("ps"), "dreadnought", &[10, 10]).0, 2);
    // Space cannon: a PDS rolls 1 die, 2 with Plasma Scoring.
    let dice_rolled = |owner: bool| {
        let (mut state, _g, ids) = tactical_fixture();
        put(&mut state, &ids[0], "cruiser", &pid("b"), 1);
        put_on_planet(
            &mut state,
            &ids[0],
            &PlanetId::new("p"),
            "pds",
            &pid("a"),
            1,
        );
        if owner {
            give(&mut state, "a", "ps");
        }
        let mut dice = crate::dice::Dice::new();
        let mut rng = crate::rng::GameRng::new(3);
        crate::combat::space_cannon_offense(
            &mut state,
            content(),
            POK,
            &mut dice,
            &mut rng,
            &ids[0],
            &pid("b"),
            None,
        );
        dice.history()
            .iter()
            .map(|roll| roll.faces.len())
            .sum::<usize>()
    };
    assert_eq!(dice_rolled(false), 1);
    assert_eq!(dice_rolled(true), 2);
}

// ---- Warfare (red) --------------------------------------------------------------------------

/// A two-player game and the Mecatol system, for space combat.
fn arena() -> (GameState, SystemId) {
    (game(&["a", "b"]), SystemId::new("18"))
}

#[test]
fn md_base_1_superseded_printing_is_not_in_play() {
    assert_superseded("md_base");
}

#[test]
fn md_c1_1_superseded_printing_is_not_in_play() {
    assert_superseded("md_c1");
}

/// A galaxy of a real planet's system plus six plain ones, seat `a` active in the action phase.
fn planet_fixture() -> (
    GameState,
    ti4_content::galaxy::Galaxy,
    Vec<SystemId>,
    PlanetId,
) {
    let (planet_system, planet) = crate::fixtures::a_placed_planet();
    let mut ids = vec![planet_system.to_string()];
    ids.extend(
        crate::fixtures::plain_systems(8)
            .into_iter()
            .filter(|id| *id != planet_system.as_str())
            .take(6),
    );
    let refs: Vec<&str> = ids.iter().map(String::as_str).collect();
    let galaxy = ti4_content::galaxy::Galaxy::build(content(), &refs, POK, 1).unwrap();
    let mut state = game(&["a", "b"]);
    state.phase = Phase::Action;
    state.active = Some(pid("a"));
    (
        state,
        galaxy,
        ids.into_iter().map(SystemId::new).collect(),
        planet,
    )
}

#[test]
fn md_1_activating_a_system_with_the_owners_structures_places_infantry() {
    let (mut state, galaxy, ids, planet) = planet_fixture();
    state
        .system_mut(&ids[0])
        .set_control(planet.clone(), pid("b"));
    put_on_planet(&mut state, &ids[0], &planet, "pds", &pid("b"), 1);
    give(&mut state, "b", "md");
    let table = Table::with_default(Box::new(Scripted::new([
        TACTICAL_ACTION_ID.to_owned(),
        ids[0].to_string(),
        "done_moving".to_owned(),
    ])));
    let mut game = Game::with_table(state, content(), table).with_galaxy(galaxy);
    for _ in 0..40 {
        assert_eq!(game.step().error, None);
        if game.events.iter().any(|e| e == "TACTICAL_ACTION_COMPLETE") {
            break;
        }
    }
    let infantry = game
        .state
        .system_state(&ids[0])
        .on_planet_of(&planet, &pid("b"))
        .iter()
        .filter(|u| u.type_id.as_str() == "infantry")
        .count();
    assert_eq!(infantry, 1, "one infantry beside the PDS");
}

/// Seat `b` owns `structures` (unit type ids) on the fixture planet; returns b's infantry on it
/// after `a` activates the system.
fn md_activation(structures: &[&str], version: u32, with_card: bool) -> usize {
    let (mut state, _galaxy, ids, planet) = planet_fixture();
    state.rules_version = version;
    state
        .system_mut(&ids[0])
        .set_control(planet.clone(), pid("b"));
    for kind in structures {
        put_on_planet(&mut state, &ids[0], &planet, kind, &pid("b"), 1);
    }
    if with_card {
        give(&mut state, "b", "md");
    }
    let mut table = Table::with_default(Box::new(FirstOption));
    super::emit_event(
        &mut state,
        &mut table,
        &[],
        "SYSTEM_ACTIVATED",
        &[("player", "a"), ("system", ids[0].as_str())],
    );
    state
        .system_state(&ids[0])
        .on_planet_of(&planet, &pid("b"))
        .iter()
        .filter(|u| u.type_id.as_str() == "infantry")
        .count()
}

#[test]
fn md_1_places_one_infantry_per_structure_and_only_with_the_card_and_version() {
    let now = ti4_model::state::RULES_TECH_EFFECTS;
    assert_eq!(
        md_activation(&["pds", "spacedock"], now, true),
        2,
        "one per structure"
    );
    assert_eq!(md_activation(&["pds"], now, false), 0, "no card");
    assert_eq!(md_activation(&["pds"], now - 1, true), 0, "legacy");
    assert_eq!(
        md_activation(&[], now, true),
        0,
        "no structure, no infantry"
    );
}

#[test]
fn md_2_the_owner_assigns_the_hit_and_a_lone_mech_sustains_it() {
    let (mut state, _g, ids, planet) = planet_fixture();
    state
        .system_mut(&ids[0])
        .set_control(planet.clone(), pid("b"));
    put_on_planet(&mut state, &ids[0], &planet, "pds", &pid("b"), 1);
    put_on_planet(&mut state, &ids[0], &planet, "infantry", &pid("a"), 2);
    put_on_planet(&mut state, &ids[0], &planet, "mech", &pid("a"), 1);
    give(&mut state, "b", "md");
    // Two kinds of target: b is asked and names the infantry (the first option), a is not asked
    // about sustain for infantry.
    let mut table = Table::with_default(Box::new(FirstOption));
    super::emit_event(
        &mut state,
        &mut table,
        &[],
        "GROUND_COMBAT_STARTED",
        &[
            ("system", ids[0].as_str()),
            ("planet", planet.as_str()),
            ("attacker", "a"),
            ("defender", "b"),
        ],
    );
    let left: Vec<String> = state
        .system_state(&ids[0])
        .on_planet_of(&planet, &pid("a"))
        .iter()
        .map(|u| u.type_id.to_string())
        .collect();
    assert_eq!(left.len(), 2, "exactly one unit was lost: {left:?}");

    // A lone mech: no choice to make, and its owner sustains the hit instead of losing it.
    let (mut state, _g, ids, planet) = planet_fixture();
    state
        .system_mut(&ids[0])
        .set_control(planet.clone(), pid("b"));
    put_on_planet(&mut state, &ids[0], &planet, "pds", &pid("b"), 1);
    put_on_planet(&mut state, &ids[0], &planet, "mech", &pid("a"), 1);
    give(&mut state, "b", "md");
    let mut table = Table::with_default(Box::new(FirstOption));
    super::emit_event(
        &mut state,
        &mut table,
        &[],
        "GROUND_COMBAT_STARTED",
        &[
            ("system", ids[0].as_str()),
            ("planet", planet.as_str()),
            ("attacker", "a"),
            ("defender", "b"),
        ],
    );
    let system = state.system_state(&ids[0]);
    let mechs = system.on_planet_of(&planet, &pid("a"));
    assert_eq!(mechs.len(), 1);
    assert!(mechs[0].sustained_damage, "sustained, not destroyed");
}

#[test]
fn md_2_ground_combat_starts_with_a_free_hit_on_the_attacker() {
    let fight = |owner: bool, version: u32| {
        let (mut state, _g, ids, planet) = planet_fixture();
        state.rules_version = version;
        state
            .system_mut(&ids[0])
            .set_control(planet.clone(), pid("b"));
        put_on_planet(&mut state, &ids[0], &planet, "pds", &pid("b"), 1);
        put_on_planet(&mut state, &ids[0], &planet, "infantry", &pid("b"), 1);
        put_on_planet(&mut state, &ids[0], &planet, "infantry", &pid("a"), 1);
        if owner {
            give(&mut state, "b", "md");
        }
        // The live invasion announces this event before the first round.
        let mut table = Table::with_default(Box::new(FirstOption));
        let dice = super::emit_event(
            &mut state,
            &mut table,
            &[],
            "GROUND_COMBAT_STARTED",
            &[
                ("system", ids[0].as_str()),
                ("planet", planet.as_str()),
                ("attacker", "a"),
                ("defender", "b"),
            ],
        );
        assert_eq!(dice, 0, "a hit is produced, not rolled");
        (
            state
                .system_state(&ids[0])
                .on_planet_of(&planet, &pid("a"))
                .len(),
            state
                .system_state(&ids[0])
                .on_planet_of(&planet, &pid("b"))
                .iter()
                .filter(|u| u.type_id.as_str() == "infantry")
                .count(),
        )
    };
    let now = ti4_model::state::RULES_TECH_EFFECTS;
    assert_eq!(fight(false, now), (1, 1), "control: no card, no hit");
    assert_eq!(
        fight(true, now),
        (0, 1),
        "the grid's hit kills the lone attacker"
    );
    assert_eq!(fight(true, now - 1), (1, 1), "legacy: no hit");
}

#[test]
fn da_1_repairs_a_unit_that_did_not_sustain_this_round() {
    // The defender's cruiser hits once (round 1) and the attacker assigns it to the cruiser; the
    // dreadnought that was already damaged is repaired by Duranium Armor and so survives the
    // second round's hit by sustaining again. Without the card the damaged dreadnought dies.
    let survivors = |owner: bool| {
        let (mut state, system) = arena();
        put(&mut state, &system, "cruiser", &pid("a"), 1);
        put(&mut state, &system, "dreadnought", &pid("a"), 1);
        let dread = state
            .system_state(&system)
            .units
            .iter()
            .find(|u| u.type_id.as_str() == "dreadnought")
            .cloned()
            .unwrap();
        state
            .system_mut(&system)
            .replace_unit(&dread, dread.sustained());
        put(&mut state, &system, "cruiser", &pid("b"), 1);
        if owner {
            give(&mut state, "a", "da");
        }
        // Defender's single die: hit (10), hit (10), then misses; attacker never hits.
        let mut faces = vec![1u32, 1, 10, 1, 10];
        faces.extend(vec![1u32; 4000]);
        let mut dice = crate::dice::Dice::from_faces(faces);
        let mut rng = crate::rng::GameRng::new(1);
        let mut table = Table::with_default(Box::new(FirstOption));
        let _ = crate::combat::resolve(
            &mut state,
            content(),
            POK,
            &mut table,
            &mut dice,
            &mut rng,
            &system,
        );
        state
            .system_state(&system)
            .units_of(&pid("a"))
            .iter()
            .filter(|u| u.type_id.as_str() == "dreadnought")
            .count()
    };
    assert_eq!((survivors(false), survivors(true)), (0, 1));
}

#[test]
fn asc_1_opponent_destroys_a_non_fighter_when_three_non_fighters_attack() {
    let defender_ships = |owner: bool, cruisers: usize| {
        let (mut state, system) = arena();
        put(&mut state, &system, "cruiser", &pid("a"), cruisers);
        put(&mut state, &system, "destroyer", &pid("b"), 1);
        if owner {
            give(&mut state, "a", "asc");
        }
        let mut table = Table::with_default(Box::new(FirstOption));
        let mut dice = crate::dice::Dice::new();
        let mut rng = crate::rng::GameRng::new(3);
        crate::combat::resolve(
            &mut state,
            content(),
            POK,
            &mut table,
            &mut dice,
            &mut rng,
            &system,
        )
        .unwrap();
        (
            crate::combat::ships_of(&state, content(), POK, &pid("b"), &system).len(),
            dice.rolled("space combat").len(),
        )
    };
    assert_eq!(defender_ships(true, 3), (0, 0), "destroyed before any die");
    assert_ne!(
        defender_ships(false, 3).1,
        0,
        "control: without the card the dice roll"
    );
    assert_ne!(defender_ships(true, 2).1, 0, "two ships are not enough");
}

#[test]
fn aida_1_ignores_one_prerequisite_when_researching_a_unit_upgrade() {
    // Dreadnought II needs two blue and one yellow: own one blue and one yellow, and the card
    // (a red technology, which counts for nothing here) stands in for the missing blue.
    let a = pid("a");
    let dn2 = TechnologyId::new("dn2");
    let mut state = game(&["a"]);
    for t in ["amd", "st"] {
        give(&mut state, "a", t);
    }
    assert!(
        !crate::technology::prerequisites_met(&state, content(), POK, &a, &dn2),
        "control: one blue short"
    );
    give(&mut state, "a", "aida");
    assert!(
        crate::technology::research(&mut state, content(), POK, &a, &dn2),
        "aida waives the missing prerequisite"
    );
    assert!(
        state
            .player(&a)
            .unwrap()
            .exhausted_technologies
            .contains(&TechnologyId::new("aida"))
    );
}

#[test]
fn aida_1_only_for_unit_upgrades_once_and_only_from_the_new_version() {
    let a = pid("a");
    let dn2 = TechnologyId::new("dn2");
    let build = |version: u32| {
        let mut state = game(&["a"]);
        state.rules_version = version;
        for t in ["amd", "st", "aida"] {
            give(&mut state, "a", t);
        }
        state
    };
    let now = ti4_model::state::RULES_TECH_EFFECTS;
    assert!(
        !crate::technology::prerequisites_met(&build(now - 1), content(), POK, &a, &dn2),
        "legacy: no waiver"
    );
    // Not a unit upgrade: Light/Wave Deflector needs 3 blue and the player has 2.
    let lwd = TechnologyId::new("lwd");
    let mut two_blue = build(now);
    give(&mut two_blue, "a", "gd");
    assert!(
        !crate::technology::prerequisites_met(&two_blue, content(), POK, &a, &lwd),
        "a plain technology gets no waiver"
    );
    // Exhausted card: no waiver.
    let mut state = build(now);
    state
        .player_mut(&a)
        .unwrap()
        .exhausted_technologies
        .insert(TechnologyId::new("aida"));
    assert!(!crate::technology::prerequisites_met(
        &state,
        content(),
        POK,
        &a,
        &dn2
    ));
    // Prerequisites already met: the card stays ready.
    let mut state = build(now);
    give(&mut state, "a", "gd");
    assert!(crate::technology::research(
        &mut state,
        content(),
        POK,
        &a,
        &dn2
    ));
    assert!(
        !state
            .player(&a)
            .unwrap()
            .exhausted_technologies
            .contains(&TechnologyId::new("aida")),
        "not needed, not spent"
    );
}

/// The cost of a carrier in a fresh production window, after `production_used` has set the discount.
fn carrier_cost_after_production(techs: &[&str], answers: &[&str]) -> (i64, GameState) {
    let (mut state, system, planet) = {
        let mut state = game(&["a"]);
        let (system, planet) = crate::fixtures::a_placed_planet();
        state
            .system_mut(&system)
            .set_control(planet.clone(), pid("a"));
        put_on_planet(&mut state, &system, &planet, "spacedock", &pid("a"), 1);
        (state, system, planet)
    };
    let _ = planet;
    for t in techs {
        give(&mut state, "a", t);
    }
    let mut table = scripted(answers);
    crate::technology::production_used(&mut state, content(), POK, None, &mut table, &pid("a"))
        .unwrap();
    let mut window =
        crate::production::ProductionWindow::new(&state, content(), POK, &pid("a"), &system);
    window.refresh(&state, content(), POK);
    let choice = window
        .pending_choice(&state, content(), POK)
        .expect("a production choice");
    let carrier = choice
        .options
        .iter()
        .find(|o| o.id.starts_with("build|carrier|"))
        .expect("carrier");
    (
        carrier
            .payload
            .get("cost")
            .and_then(serde_json::Value::as_i64)
            .unwrap(),
        state,
    )
}

#[test]
fn aida_2_exhaust_to_reduce_production_cost_by_owned_unit_upgrades() {
    assert_eq!(carrier_cost_after_production(&[], &[]).0, 3, "control");
    let (cost, state) = carrier_cost_after_production(&["aida", "cr2", "dn2"], &["exhaust"]);
    assert_eq!(cost, 1, "3 minus two owned unit upgrades");
    assert!(
        state
            .player(&pid("a"))
            .unwrap()
            .exhausted_technologies
            .contains(&TechnologyId::new("aida"))
    );
}

#[test]
fn sar_1_production_lets_the_owner_place_a_mech_on_a_planet_in_that_system() {
    let mut state = seated(&[("a", "arborec"), ("b", "letnev")]);
    let a = pid("a");
    let home = state.player(&a).unwrap().home_system.clone().unwrap();
    give(&mut state, "a", "sar");
    state.player_mut(&a).unwrap().trade_goods = 10;
    let mechs = |s: &GameState| {
        let types = ti4_content::units::catalogue(content(), POK);
        s.system_state(&home)
            .planet_units
            .values()
            .flatten()
            .filter(|u| {
                u.owner == a
                    && types
                        .get(u.type_id.as_str())
                        .is_some_and(|k| k.base_type() == "mech")
            })
            .count()
    };
    let before = mechs(&state);
    // A use of PRODUCTION (not a single-unit ability): build one infantry, finish, and agree to
    // the Self-Assembly Routines offer.
    struct Eager {
        built: bool,
    }
    impl crate::choice::Decider for Eager {
        fn choose(
            &mut self,
            choice: &crate::choice::Choice,
        ) -> Result<crate::choice::ChoiceOption, crate::choice::IllegalChoice> {
            if choice.prompt.starts_with("Self-Assembly Routines") {
                return Ok(choice.options[0].clone());
            }
            if !self.built
                && let Some(build) = choice.options.iter().find(|o| o.id == "build|infantry|2")
            {
                self.built = true;
                return Ok(build.clone());
            }
            if self.built
                && let Some(done) = choice.options.iter().find(|o| o.id == "done_producing")
            {
                return Ok(done.clone());
            }
            Ok(choice.options[0].clone())
        }
    }
    let mut table = Table::with_default(Box::new(Eager { built: false }));
    crate::production::resolve(&mut state, content(), POK, None, &mut table, &a, &home).unwrap();
    assert_eq!(mechs(&state), before + 1);
}

#[test]
fn sar_2_a_destroyed_mech_pays_one_trade_good() {
    let tg_after = |owner: bool| {
        let mut state = seated(&[("a", "arborec"), ("b", "letnev")]);
        let a = pid("a");
        if owner {
            give(&mut state, "a", "sar");
        }
        let home = state.player(&a).unwrap().home_system.clone().unwrap();
        let before = state.player(&a).unwrap().trade_goods;
        let mut resolver = crate::fixtures::armed_resolver(&state);
        let mut dice = crate::dice::Dice::new();
        let mut rng = crate::rng::GameRng::new(0);
        let mut sequence = crate::event::EventSequence::new();
        let mut table = Table::with_default(Box::new(FirstOption));
        let mut context = crate::timing::TimingContext {
            state: &mut state,
            content: content(),
            sources: POK,
            table: &mut table,
            dice: &mut dice,
            rng: &mut rng,
            event_sequence: &mut sequence,
            galaxy: None,
        };
        let payload = [
            ("player", "a"),
            ("unit", "arborec_mech"),
            ("system", home.as_str()),
            ("planet", "nestphar"),
        ]
        .iter()
        .map(|(k, v)| ((*k).to_owned(), serde_json::Value::from(*v)))
        .collect();
        let event = context
            .event_sequence
            .next("GROUND_FORCE_DESTROYED", payload)
            .expect("an event id");
        resolver
            .emit_with_context(&mut context, event, |_, _| {})
            .expect("resolves");
        state.player(&a).unwrap().trade_goods - before
    };
    assert_eq!(tg_after(true), 1);
    assert_eq!(tg_after(false), 0);
}

// ---- Cybernetic (yellow) --------------------------------------------------------------------

#[test]
fn st_1_production_costs_one_less() {
    assert_eq!(carrier_cost_after_production(&[], &[]).0, 3, "control");
    assert_eq!(carrier_cost_after_production(&["st"], &[]).0, 2);
}

#[test]
fn gls_1_space_cannon_hits_must_go_to_non_fighters_once_per_readying() {
    let (mut state, system) = arena();
    put(&mut state, &system, "fighter", &pid("b"), 2);
    put(&mut state, &system, "destroyer", &pid("b"), 1);
    let (a, b) = (pid("a"), pid("b"));
    assert!(
        !crate::combat::use_graviton(&mut state, content(), POK, &a, &b, &system, 1),
        "control: no card"
    );
    give(&mut state, "a", "gls");
    assert!(crate::combat::use_graviton(
        &mut state,
        content(),
        POK,
        &a,
        &b,
        &system,
        1
    ));
    assert!(
        !crate::combat::use_graviton(&mut state, content(), POK, &a, &b, &system, 1),
        "exhausted"
    );

    let mut table = Table::with_default(Box::new(FirstOption));
    let mut dice = crate::dice::Dice::new();
    let mut rng = crate::rng::GameRng::new(1);
    let mut ctx = crate::choice::Resolving {
        content: content(),
        sources: POK,
        dice: &mut dice,
        rng: &mut rng,
        table: &mut table,
        timing: None,
    };
    crate::combat::absorb_hits_seeing_with(
        &mut state,
        content(),
        POK,
        None,
        &mut ctx,
        &b,
        &system,
        &a,
        1,
        true,
        crate::combat::HitOrigin::UnitAbility,
    )
    .unwrap();
    assert_eq!(
        crate::combat::non_fighter_ships_of(&state, content(), POK, &b, &system),
        0,
        "the destroyer took the hit"
    );
    assert_eq!(
        crate::combat::ships_of(&state, content(), POK, &b, &system).len(),
        2,
        "both fighters survive"
    );
}

#[test]
fn td_1_redeploy_up_to_four_ground_forces_at_turn_start() {
    let (sys_a, planet_a) = crate::fixtures::a_placed_planet();
    let (sys_b, planet_b) = crate::fixtures::non_home_planets(2)
        .into_iter()
        .map(PlanetId::new)
        .filter(|p| *p != planet_a)
        .find_map(|p| {
            ti4_content::galaxy::all_planets(content(), POK)
                .get(p.as_str())
                .and_then(|pl| pl.system_id())
                .map(|s| (SystemId::new(s), p.clone()))
        })
        .unwrap();
    let a = pid("a");
    let run = |owner: bool| {
        let mut state = game(&["a"]);
        control(&mut state, &sys_a, &planet_a);
        control(&mut state, &sys_b, &planet_b);
        put_on_planet(&mut state, &sys_a, &planet_a, "infantry", &a, 3);
        if owner {
            give(&mut state, "a", "td");
        }
        let id = format!("transit|{sys_a}|{planet_a}|infantry|{sys_b}|{planet_b}");
        let mut table = scripted(&[&id, &id, "decline"]);
        crate::technology::start_turn(&mut state, content(), POK, None, &mut table, &a).unwrap();
        (
            state.system_state(&sys_b).on_planet_of(&planet_b, &a).len(),
            state
                .player(&a)
                .unwrap()
                .exhausted_technologies
                .contains(&TechnologyId::new("td")),
        )
    };
    assert_eq!(run(true), (2, true), "two moved, card exhausted");
    assert_eq!(run(false), (0, false));
}

#[test]
fn ie_1_gaining_a_planet_lets_the_owner_produce_up_to_its_resources() {
    let (system, planet) = crate::fixtures::a_placed_planet();
    let a = pid("a");
    let run = |owner: bool| {
        let mut state = game(&["a"]);
        if owner {
            give(&mut state, "a", "ie");
        }
        control(&mut state, &system, &planet);
        state.player_mut(&a).unwrap().trade_goods = 1;
        let mut table = scripted(&["build|destroyer|1", "trade_good", "done_producing"]);
        let built = crate::technology::control_gained(
            &mut state,
            content(),
            POK,
            None,
            &mut table,
            &a,
            &system,
            &planet,
        )
        .unwrap();
        (
            built,
            state
                .system_state(&system)
                .units_of(&a)
                .iter()
                .any(|u| u.type_id.as_str() == "destroyer"),
        )
    };
    assert_eq!(run(true), (true, true));
    assert_eq!(run(false), (false, false));
}

/// A placed planet that has an exploration trait.
fn traited_planet() -> (SystemId, PlanetId) {
    ti4_content::galaxy::all_planets(content(), POK)
        .iter()
        .filter(|(_, p)| p.system_id().is_some() && !p.is_placed_during_play())
        .find(|(id, _)| {
            crate::exploration::trait_of(content(), POK, &PlanetId::new(**id)).is_some()
        })
        .map(|(id, p)| (SystemId::new(p.system_id().unwrap()), PlanetId::new(*id)))
        .expect("a planet with a trait")
}

#[test]
fn sdn_1_activating_a_system_offers_to_explore_a_planet_with_your_units() {
    let (system, planet) = traited_planet();
    let a = pid("a");
    let offer = |owner: bool, with_units: bool| {
        let mut state = game(&["a"]);
        if owner {
            give(&mut state, "a", "sdn");
        }
        if with_units {
            put_on_planet(&mut state, &system, &planet, "infantry", &a, 1);
        }
        let mut table = Table::with_default(Box::new(FirstOption));
        crate::faction_techs::offer_scanlink(&state, content(), POK, &mut table, None, &system, &a)
    };
    assert_eq!(offer(true, true), Some(planet.clone()));
    assert_eq!(offer(true, false), None, "needs a unit on the planet");
    assert_eq!(offer(false, true), None, "needs the card");
}

#[test]
fn pi_1_redistribute_command_tokens_at_the_end_of_the_turn() {
    let a = pid("a");
    let run = |owner: bool| {
        let mut state = game(&["a"]);
        if owner {
            give(&mut state, "a", "pi");
        }
        {
            let seat = state.player_mut(&a).unwrap();
            seat.tactic_tokens = 3;
            seat.fleet_tokens = 3;
            seat.strategic_tokens = 2;
        }
        let mut table = scripted(&["tactic|strategy", "done"]);
        crate::technology::end_turn(&mut state, content(), POK, None, &mut table, &a).unwrap();
        let seat = state.player(&a).unwrap();
        (
            (seat.tactic_tokens, seat.fleet_tokens, seat.strategic_tokens),
            seat.exhausted_technologies
                .contains(&TechnologyId::new("pi")),
        )
    };
    assert_eq!(run(true), ((2, 3, 3), true));
    assert_eq!(run(false), ((3, 3, 2), false));
}

#[test]
fn pi_2_three_additional_votes() {
    // Seat a controls a 1-influence planet; seat b a 3-influence planet. Without the card b wins
    // 3-1 (or a's 1 vote is outweighed); with Predictive Intelligence a casts 1 + 3 = 4.
    let planets: Vec<(SystemId, PlanetId, i64)> = ti4_content::galaxy::all_planets(content(), POK)
        .iter()
        .filter(|(_, p)| {
            p.system_id().is_some() && !p.is_placed_during_play() && p.homeworld_of().is_none()
        })
        .map(|(id, p)| {
            (
                SystemId::new(p.system_id().unwrap()),
                PlanetId::new(*id),
                p.influence(),
            )
        })
        .collect();
    let low = planets
        .iter()
        .find(|p| p.2 == 1)
        .expect("a 1-influence planet")
        .clone();
    let high = planets
        .iter()
        .find(|p| p.2 == 3)
        .expect("a 3-influence planet")
        .clone();
    let second = planets
        .iter()
        .find(|p| p.2 == 2 && p.1 != high.1 && p.1 != low.1)
        .expect("a 2-influence planet")
        .clone();
    // Returns the winner, whether the card was exhausted at the end, and whether the question
    // was asked. `a` takes the extra votes when `take` is set.
    let vote = |owner: bool, version: u32, take: bool, rich_b: bool| {
        let a_backs = "for";
        let mut state = game(&["a", "b"]);
        state.rules_version = version;
        state
            .system_mut(&low.0)
            .set_control(low.1.clone(), pid("a"));
        state
            .system_mut(&high.0)
            .set_control(high.1.clone(), pid("b"));
        if rich_b {
            state
                .system_mut(&second.0)
                .set_control(second.1.clone(), pid("b"));
        }
        if owner {
            give(&mut state, "a", "pi");
        }
        let mut window = crate::vote::VoteWindow::new(
            &state,
            "mutiny",
            vec!["for".to_owned(), "against".to_owned()],
        );
        window.open(&state, content(), POK);
        let mut asked = false;
        for _ in 0..20 {
            let Some(choice) = window.pending_choice(&state, content(), POK) else {
                break;
            };
            let who = choice.player.clone();
            let pick = if choice
                .options
                .iter()
                .any(|o| o.kind == crate::vote::PREDICTIVE_KIND)
            {
                asked = true;
                choice
                    .options
                    .iter()
                    .find(|o| o.is_decline() != take)
                    .unwrap()
                    .clone()
            } else {
                let want = if who == pid("a") { a_backs } else { "against" };
                choice
                    .options
                    .iter()
                    .find(|o| o.id == want)
                    .or_else(|| choice.options.iter().find(|o| !o.is_decline()))
                    .unwrap()
                    .clone()
            };
            window.resolve(&mut state, content(), POK, pick).unwrap();
        }
        let exhausted = state
            .player(&pid("a"))
            .unwrap()
            .exhausted_technologies
            .contains(&TechnologyId::new("pi"));
        (window.winner().map(ToOwned::to_owned), exhausted, asked)
    };
    let now = ti4_model::state::RULES_TECH_EFFECTS;
    assert_eq!(
        vote(false, now, true, false),
        (Some("against".to_owned()), false, false),
        "control: 3 beats 1, no card, no question"
    );
    assert_eq!(
        vote(true, now, true, false),
        (Some("for".to_owned()), false, true),
        "1 + 3 beats 3; the outcome won, the card stays ready"
    );
    assert_eq!(
        vote(true, now, false, false),
        (Some("against".to_owned()), false, true),
        "declined: just 1 vote, nothing exhausted"
    );
    assert_eq!(
        vote(true, now - 1, true, false),
        (Some("against".to_owned()), false, false),
        "legacy: no question before the rules version"
    );
    // b has 3 + 2 = 5 votes: a's 1 + 3 = 4 still loses, so the outcome a backed is not resolved
    // and the card exhausts. Without taking the extra votes the card stays ready.
    assert_eq!(
        vote(true, now, true, true),
        (Some("against".to_owned()), true, true),
        "the backed outcome lost: Predictive Intelligence exhausts"
    );
    assert_eq!(
        vote(true, now, false, true),
        (Some("against".to_owned()), false, true),
        "declined the extra votes: nothing to exhaust"
    );
}

// ---- Unit upgrades --------------------------------------------------------------------------

/// The printed values of a unit type, as the card states them: cost per unit, combat (hits on,
/// dice), move and capacity. `None` leaves a value unchecked.
fn assert_card(
    unit: &str,
    cost: f64,
    combat: Option<(i64, i64)>,
    mv: Option<i64>,
    capacity: Option<i64>,
) {
    let types = ti4_content::units::catalogue(content(), POK);
    let kind = types.get(unit).unwrap_or_else(|| panic!("unit {unit}"));
    assert!(
        (kind.cost() - cost).abs() < f64::EPSILON,
        "{unit} cost {} != {cost}",
        kind.cost()
    );
    if let Some((hits_on, dice)) = combat {
        assert_eq!(
            (kind.combat_hits_on(), kind.combat_dice()),
            (Some(hits_on), dice),
            "{unit} combat"
        );
    }
    if let Some(mv) = mv {
        assert_eq!(kind.move_value(), mv, "{unit} move");
    }
    if let Some(capacity) = capacity {
        assert_eq!(kind.capacity(), capacity, "{unit} capacity");
    }
}

/// Researching `tech` replaces `old` with `new` on the board (on a planet for structures and
/// ground forces) and in what can be built; without the card nothing changes.
fn assert_upgrade_replaces(tech: &str, old: &str, new: &str, on_planet: bool) {
    let a = pid("a");
    let mut state = game(&["a"]);
    let (system, planet) = crate::fixtures::a_placed_planet();
    if on_planet {
        put_on_planet(&mut state, &system, &planet, old, &a, 2);
    } else {
        put(&mut state, &system, old, &a, 2);
    }
    let kinds = |state: &GameState| -> Vec<String> {
        let board = state.system_state(&system);
        let units = if on_planet {
            board.on_planet_of(&planet, &a).to_vec()
        } else {
            board.units_of(&a)
        };
        units.iter().map(|u| u.type_id.to_string()).collect()
    };
    crate::technology::apply_unit_upgrades(&mut state, content(), POK, &a);
    assert_eq!(
        kinds(&state),
        vec![old.to_owned(); 2],
        "without the card nothing changes"
    );
    // Structures are placed by Construction rather than produced, so only units are checked here.
    let produced = !matches!(old, "pds" | "spacedock");
    assert!(
        !crate::production::buildable_for(&state, content(), POK, &a).contains(&new.to_owned())
    );
    give(&mut state, "a", tech);
    crate::technology::apply_unit_upgrades(&mut state, content(), POK, &a);
    assert_eq!(
        kinds(&state),
        vec![new.to_owned(); 2],
        "the new card replaces the old on the board"
    );
    if produced {
        let buildable = crate::production::buildable_for(&state, content(), POK, &a);
        assert!(
            buildable.contains(&new.to_owned()),
            "and new ones are built upgraded"
        );
        assert!(!buildable.contains(&old.to_owned()), "the old card is gone");
    }
}

#[test]
fn ws_1_war_sun_stats_and_only_buildable_with_the_card() {
    assert_card("warsun", 12.0, Some((3, 3)), Some(2), Some(6));
    let mut state = game(&["a"]);
    let a = pid("a");
    assert!(
        !crate::production::buildable_for(&state, content(), POK, &a)
            .contains(&"warsun".to_owned())
    );
    give(&mut state, "a", "ws");
    assert!(
        crate::production::buildable_for(&state, content(), POK, &a).contains(&"warsun".to_owned())
    );
}

#[test]
fn ws_2_war_sun_sustains_and_bombards_with_three_dice() {
    let types = ti4_content::units::catalogue(content(), POK);
    let ws = types.get("warsun").unwrap();
    assert!(ws.sustain_damage());
    assert_eq!((ws.bombard_hits_on(), ws.bombard_dice()), (Some(3), 3));
}

#[test]
fn ws_3_other_players_units_lose_planetary_shield() {
    let (system, planet) = crate::fixtures::a_placed_planet();
    let bombardable = |ship: &str| {
        let mut state = game(&["a", "b"]);
        put_on_planet(&mut state, &system, &planet, "pds", &pid("b"), 1);
        put_on_planet(&mut state, &system, &planet, "infantry", &pid("b"), 1);
        put(&mut state, &system, ship, &pid("a"), 1);
        crate::invasion::bombardable(&state, content(), POK, &system, &planet, &pid("a"))
    };
    assert!(!bombardable("dreadnought"), "a PDS shields the planet");
    assert!(bombardable("warsun"), "a war sun ignores the shield");
}

#[test]
fn sd2_1_has_production_and_replaces_space_dock_i() {
    let types = ti4_content::units::catalogue(content(), POK);
    assert!(types.get("spacedock2").unwrap().has_production());
    assert_upgrade_replaces("sd2", "spacedock", "spacedock2", true);
}

#[test]
fn sd2_2_production_is_four_more_than_the_planets_resources() {
    let a = pid("a");
    let (system, planet) = crate::fixtures::a_placed_planet();
    let mut state = game(&["a"]);
    control(&mut state, &system, &planet);
    put_on_planet(&mut state, &system, &planet, "spacedock", &a, 1);
    let resources = crate::production::planet_value_now(
        &state,
        content(),
        POK,
        &planet,
        crate::production::Spend::Resources,
    );
    assert_eq!(
        crate::production::capacity(&state, content(), POK, &a, &system),
        resources + 2,
        "Space Dock I"
    );
    give(&mut state, "a", "sd2");
    crate::technology::apply_unit_upgrades(&mut state, content(), POK, &a);
    assert_eq!(
        crate::production::capacity(&state, content(), POK, &a, &system),
        resources + 4,
        "Space Dock II"
    );
}

#[test]
fn sd2_3_three_fighters_do_not_count_against_capacity() {
    let a = pid("a");
    let (system, planet) = crate::fixtures::a_placed_planet();
    let mut state = game(&["a"]);
    control(&mut state, &system, &planet);
    put_on_planet(&mut state, &system, &planet, "spacedock", &a, 1);
    give(&mut state, "a", "sd2");
    crate::technology::apply_unit_upgrades(&mut state, content(), POK, &a);
    put(&mut state, &system, "fighter", &a, 3);
    let standing = crate::fleet::standing(&state, content(), POK, &a, &system, None);
    assert_eq!(
        (standing.consumed, standing.capacity_excess),
        (0, 0),
        "three fighters ride free"
    );
    put(&mut state, &system, "fighter", &a, 1);
    let standing = crate::fleet::standing(&state, content(), POK, &a, &system, None);
    assert_eq!(standing.capacity_excess, 1, "the fourth does not");
}

#[test]
fn cr2_1_cruiser_ii_stats_and_replaces_cruiser() {
    assert_card("cruiser2", 2.0, Some((6, 1)), Some(3), Some(1));
    assert_upgrade_replaces("cr2", "cruiser", "cruiser2", false);
}

#[test]
fn dn2_1_dreadnought_ii_stats_and_replaces_dreadnought() {
    assert_card("dreadnought2", 4.0, Some((5, 1)), Some(2), Some(1));
    assert_upgrade_replaces("dn2", "dreadnought", "dreadnought2", false);
}

#[test]
fn dn2_2_sustains_damage_and_bombards_at_five() {
    let types = ti4_content::units::catalogue(content(), POK);
    let dn = types.get("dreadnought2").unwrap();
    assert!(dn.sustain_damage());
    assert_eq!((dn.bombard_hits_on(), dn.bombard_dice()), (Some(5), 1));
}

#[test]
fn dn2_3_cannot_be_destroyed_by_direct_hit() {
    assert!(crate::combat::direct_hittable(
        content(),
        POK,
        "dreadnought"
    ));
    assert!(!crate::combat::direct_hittable(
        content(),
        POK,
        "dreadnought2"
    ));
}

#[test]
fn dd2_1_destroyer_ii_stats_and_replaces_destroyer() {
    assert_card("destroyer2", 1.0, Some((8, 1)), Some(2), None);
    assert_upgrade_replaces("dd2", "destroyer", "destroyer2", false);
}

#[test]
fn dd2_2_anti_fighter_barrage_six_with_three_dice() {
    let types = ti4_content::units::catalogue(content(), POK);
    let dd = types.get("destroyer2").unwrap();
    assert_eq!((dd.afb_hits_on(), dd.afb_dice()), (Some(6), 3));
}

#[test]
fn pds2_1_planetary_shield_and_space_cannon_five_and_replaces_pds() {
    let types = ti4_content::units::catalogue(content(), POK);
    let pds = types.get("pds2").unwrap();
    assert!(pds.planetary_shield());
    assert_eq!(
        (pds.space_cannon_hits_on(), pds.space_cannon_dice()),
        (Some(5), 1)
    );
    assert_upgrade_replaces("pds2", "pds", "pds2", true);
}

#[test]
fn pds2_2_space_cannon_reaches_ships_in_adjacent_systems() {
    let hub = plain_hub();
    let active = SystemId::new(&hub.centre);
    let next_door = SystemId::new(&hub.outer[0]);
    let planet = PlanetId::new("mellon");
    let guns = |tech: bool| {
        let mut state = game(&["a", "b"]);
        for id in std::iter::once(&hub.centre).chain(hub.outer.iter()) {
            state.board.entry(SystemId::new(id)).or_default();
        }
        put(&mut state, &active, "cruiser", &pid("a"), 1);
        put_on_planet(&mut state, &next_door, &planet, "pds", &pid("b"), 1);
        if tech {
            give(&mut state, "b", "pds2");
            crate::technology::apply_unit_upgrades(&mut state, content(), POK, &pid("b"));
        }
        crate::combat::space_cannon_guns(
            &state,
            content(),
            POK,
            &active,
            &pid("a"),
            Some(&hub.galaxy),
        )
        .units
        .len()
    };
    assert_eq!(guns(false), 0, "PDS I stays home");
    assert_eq!(guns(true), 1, "PDS II reaches next door");
}

#[test]
fn cv2_1_carrier_ii_stats_and_replaces_carrier() {
    assert_card("carrier2", 3.0, Some((9, 1)), Some(2), Some(6));
    assert_upgrade_replaces("cv2", "carrier", "carrier2", false);
}

#[test]
fn ff2_1_fighter_ii_stats_and_replaces_fighter() {
    assert_card("fighter2", 0.5, Some((8, 1)), Some(2), None);
    assert_upgrade_replaces("ff2", "fighter", "fighter2", false);
}

#[test]
fn ff2_2_moves_without_being_transported() {
    let hub = plain_hub();
    let a = pid("a");
    let target = SystemId::new(&hub.centre);
    let from = SystemId::new(&hub.outer[0]);
    let movable = |tech: bool| {
        let mut state = game(&["a"]);
        put(&mut state, &from, "fighter", &a, 1);
        if tech {
            give(&mut state, "a", "ff2");
            crate::technology::apply_unit_upgrades(&mut state, content(), POK, &a);
        }
        crate::tactical::movable_into(&state, content(), POK, &hub.galaxy, &a, &target).len()
    };
    assert_eq!(movable(false), 0, "Fighter I needs a carrier");
    assert_eq!(movable(true), 1, "Fighter II moves on its own");
}

#[test]
fn ff2_3_fighters_in_excess_of_capacity_count_against_the_fleet_pool() {
    let a = pid("a");
    let charged = |upgraded: bool| {
        let mut state = game(&["a"]);
        let system = SystemId::new(crate::fixtures::plain_systems(1)[0].clone());
        state.board.entry(system.clone()).or_default();
        put(
            &mut state,
            &system,
            if upgraded { "fighter2" } else { "fighter" },
            &a,
            3,
        );
        let standing = crate::fleet::standing(&state, content(), POK, &a, &system, None);
        (standing.fighters_charged, standing.fleet_charged)
    };
    assert_eq!(
        charged(true),
        (3, 3),
        "Fighter II: excess fighters are ships against the pool"
    );
    assert_eq!(charged(false).0, 0, "Fighter I: they are removed instead");
}

#[test]
fn inf2_1_infantry_ii_stats_and_replaces_infantry() {
    assert_card("infantry2", 0.5, Some((7, 1)), None, None);
    assert_upgrade_replaces("inf2", "infantry", "infantry2", true);
}

#[test]
fn inf2_2_a_destroyed_infantry_ii_returns_home_on_a_six() {
    let returned = |face: u32, tech: bool| {
        let mut state = seated(&[("a", "arborec"), ("b", "letnev")]);
        let a = pid("a");
        // Arborec holds its own infantry, so use a generic-infantry faction.
        state.player_mut(&a).unwrap().faction = ti4_model::id::FactionId::new("hacan");
        if tech {
            give(&mut state, "a", "inf2");
        }
        let home = state.player(&a).unwrap().home_system.clone().unwrap();
        let planet = state
            .controlled_planets(&a)
            .into_iter()
            .find(|(s, _)| **s == home)
            .map(|(_, p)| p.clone())
            .unwrap();
        let count = |s: &GameState| {
            s.system_state(&home)
                .on_planet_of(&planet, &a)
                .iter()
                .filter(|u| u.type_id.as_str() == "infantry2")
                .count()
        };
        let before = count(&state);
        let emit = |state: &mut GameState, face: &[u32], kind: &str, pairs: &[(&str, &str)]| {
            let mut resolver = crate::fixtures::armed_resolver(state);
            let mut dice = crate::dice::Dice::from_faces(face.iter().copied());
            let mut rng = crate::rng::GameRng::new(0);
            let mut sequence = crate::event::EventSequence::new();
            let mut table = Table::with_default(Box::new(FirstOption));
            let mut context = crate::timing::TimingContext {
                state,
                content: content(),
                sources: POK,
                table: &mut table,
                dice: &mut dice,
                rng: &mut rng,
                event_sequence: &mut sequence,
                galaxy: None,
            };
            let payload = pairs
                .iter()
                .map(|(k, v)| ((*k).to_owned(), serde_json::Value::from(*v)))
                .collect();
            let event = context
                .event_sequence
                .next(kind, payload)
                .expect("an event id");
            resolver
                .emit_with_context(&mut context, event, |_, _| {})
                .expect("resolves");
        };
        emit(
            &mut state,
            &[face],
            "GROUND_FORCE_DESTROYED",
            &[
                ("player", "a"),
                ("unit", "infantry2"),
                ("system", home.as_str()),
                ("planet", planet.as_str()),
            ],
        );
        emit(&mut state, &[], "TURN_BEGAN", &[("player", "a")]);
        count(&state) - before
    };
    assert_eq!(returned(6, true), 1);
    assert_eq!(returned(5, true), 0);
    assert_eq!(returned(6, false), 0);
}
