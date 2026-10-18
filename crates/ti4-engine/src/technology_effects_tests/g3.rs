//! Technology effect tests, group g3 (faction techs; see mod.rs and docs/TECH_EFFECT_COVERAGE.md).
//!
//! Faction technologies are mostly timing abilities armed per seat, so the tests drive them the
//! way the game does: an event through [`crate::fixtures::armed_resolver`] with a scripted table,
//! or the component-action entry points.

use std::collections::BTreeMap;

use ti4_content::units::catalogue;
use ti4_model::content_types::{DEFAULT, POK};
use ti4_model::id::{PlanetId, PlayerId, SystemId};
use ti4_model::state::GameState;

use super::{content, give, owns, pid, seated};
use crate::choice::{Scripted, Table};
use crate::fixtures::{put, put_on_planet};

fn a() -> PlayerId {
    pid("a")
}
fn b() -> PlayerId {
    pid("b")
}

fn scripted(answers: &[&str]) -> Table {
    Table::with_default(Box::new(Scripted::new(
        answers.iter().map(|answer| (*answer).to_owned()),
    )))
}

/// Emit one typed event (JSON payload) through a resolver armed exactly as the game arms it.
fn emit_full(
    state: &mut GameState,
    table: &mut Table,
    galaxy: Option<&ti4_content::galaxy::Galaxy>,
    event_type: &str,
    payload: BTreeMap<String, serde_json::Value>,
) {
    let mut resolver = crate::fixtures::armed_resolver(state);
    let mut dice = crate::dice::Dice::new();
    let mut rng = crate::rng::GameRng::new(0);
    let mut sequence = crate::event::EventSequence::new();
    let mut context = crate::timing::TimingContext {
        state,
        content: content(),
        sources: POK,
        table,
        dice: &mut dice,
        rng: &mut rng,
        event_sequence: &mut sequence,
        galaxy,
    };
    let event = context
        .event_sequence
        .next(event_type, payload)
        .expect("an event id");
    resolver
        .emit_with_context(&mut context, event, |_, _| {})
        .expect("the window resolves");
}

fn json(pairs: &[(&str, serde_json::Value)]) -> BTreeMap<String, serde_json::Value> {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_owned(), v.clone()))
        .collect()
}

fn emit(state: &mut GameState, answers: &[&str], event_type: &str, pairs: &[(&str, &str)]) {
    let payload = pairs
        .iter()
        .map(|(k, v)| ((*k).to_owned(), serde_json::Value::from(*v)))
        .collect();
    emit_full(state, &mut scripted(answers), None, event_type, payload);
}

fn on_planet(
    state: &GameState,
    system: &SystemId,
    planet: &str,
    who: &PlayerId,
    kind: &str,
) -> usize {
    state
        .system_state(system)
        .on_planet_of(&PlanetId::new(planet), who)
        .into_iter()
        .filter(|unit| unit.type_id.as_str() == kind)
        .count()
}

fn in_space(state: &GameState, system: &SystemId, who: &PlayerId, kind: &str) -> usize {
    state
        .system_state(system)
        .units
        .iter()
        .filter(|unit| &unit.owner == who && unit.type_id.as_str() == kind)
        .count()
}

/// Strip a seat of its units and planets so only what a test adds exists.
fn bare(state: &mut GameState, who: &PlayerId) {
    state.board.values_mut().for_each(|board| {
        board.planet_control.retain(|_, owner| owner != who);
        board
            .planet_units
            .values_mut()
            .for_each(|units| units.retain(|unit| &unit.owner != who));
        board.units.retain(|unit| &unit.owner != who);
    });
}

/// The unit card `id` as the catalogue prints it.
fn stats(id: &str) -> ti4_content::units::UnitType<'static> {
    *catalogue(content(), DEFAULT)
        .get(id)
        .unwrap_or_else(|| panic!("unit {id} in the catalogue"))
}

// -- Bioplasmosis ---------------------------------------------------------------------------------

#[test]
fn bio_1_moves_infantry_between_planets_at_the_end_of_the_status_phase() {
    const BIO: &str = "technology:arborec:bio:STATUS_PHASE_ENDED:after";
    let (system, _) = crate::fixtures::a_placed_planet();
    let run = |with_tech: bool| {
        let mut state = seated(&[("a", "arborec"), ("b", "sol")]);
        bare(&mut state, &a());
        if with_tech {
            give(&mut state, "a", "bio");
        }
        for (planet, n) in [("src", 2), ("dst", 0)] {
            state
                .system_mut(&system)
                .set_control(PlanetId::new(planet), a());
            put_on_planet(
                &mut state,
                &system,
                &PlanetId::new(planet),
                "arborec_infantry",
                &a(),
                n,
            );
        }
        emit(
            &mut state,
            &[BIO, "move|0", "move|0"],
            "STATUS_PHASE_ENDED",
            &[],
        );
        (
            on_planet(&state, &system, "src", &a(), "arborec_infantry"),
            on_planet(&state, &system, "dst", &a(), "arborec_infantry"),
        )
    };
    assert_eq!(run(true), (0, 2), "both infantry were moved");
    assert_eq!(run(false), (2, 0), "without the card nothing is offered");
}

// -- Aerie Hololattice ----------------------------------------------------------------------------

fn hololattice(with_tech: bool) -> (GameState, SystemId, PlanetId) {
    let mut state = seated(&[("a", "argent"), ("b", "sol")]);
    let (system, planet) = crate::fixtures::a_placed_planet();
    if with_tech {
        give(&mut state, "a", "ah");
    }
    put_on_planet(&mut state, &system, &planet, "pds", &a(), 1);
    (state, system, planet)
}

#[test]
fn ah_1_other_players_cannot_move_ships_through_systems_with_your_structures() {
    let blocks = |with_tech: bool, mover: &PlayerId| {
        let (state, system, _) = hololattice(with_tech);
        crate::factions::hooks_movement::blocks_passage(&state, content(), POK, mover, &system)
    };
    assert!(blocks(true, &b()));
    assert!(!blocks(true, &a()), "the owner passes");
    assert!(!blocks(false, &b()), "no card, no wall");
}

#[test]
fn ah_2_a_planet_with_your_structure_gains_production_one() {
    let production = |with_tech: bool, who: &PlayerId| {
        let (state, system, planet) = hololattice(with_tech);
        crate::factions::hooks_economy::extra_production_planet(
            &state,
            content(),
            POK,
            who,
            &system,
            &planet,
        )
    };
    assert_eq!(production(true, &a()), 1);
    assert_eq!(production(true, &b()), 0);
    assert_eq!(production(false, &a()), 0);
}

// -- Non-Euclidean Shielding ----------------------------------------------------------------------

#[test]
fn nes_1_one_sustain_cancels_two_hits() {
    let run = |with_tech: bool| {
        let mut state = seated(&[("a", "letnev"), ("b", "sol")]);
        let system = SystemId::new("19");
        put(&mut state, &system, "dreadnought", &a(), 1);
        put(&mut state, &system, "cruiser", &a(), 1);
        if with_tech {
            give(&mut state, "a", "nes");
        }
        let mut table = Table::with_default(Box::new(crate::choice::FirstOption));
        crate::combat::absorb_hits(
            &mut state,
            content(),
            POK,
            &mut table,
            &a(),
            &system,
            &b(),
            2,
        )
        .expect("hits resolve");
        (
            state.system_state(&system).units_of(&a()).len(),
            state
                .system_state(&system)
                .units
                .iter()
                .filter(|u| u.sustained_damage)
                .count(),
        )
    };
    assert_eq!(run(true), (2, 1), "the dreadnought alone absorbs both hits");
    assert_eq!(
        run(false).0,
        1,
        "without the card the second hit costs a ship"
    );
}

// -- Floating Factory II --------------------------------------------------------------------------

#[test]
fn ffac2_1_has_move_two_and_capacity_five_and_replaces_the_first_factory() {
    let (old, new) = (stats("saar_spacedock"), stats("saar_spacedock2"));
    assert_eq!((old.move_value(), old.capacity()), (1, 4));
    assert_eq!((new.move_value(), new.capacity()), (2, 5));
    assert_eq!(new.upgrades_from(), Some("saar_spacedock"));
    assert_eq!(new.required_technology(), Some("ffac2"));
    let mut state = seated(&[("a", "saar"), ("b", "sol")]);
    let system = SystemId::new("19");
    put(&mut state, &system, "saar_spacedock", &a(), 1);
    give(&mut state, "a", "ffac2");
    crate::technology::apply_unit_upgrades(&mut state, content(), DEFAULT, &a());
    assert_eq!(in_space(&state, &system, &a(), "saar_spacedock2"), 1);
    assert_eq!(in_space(&state, &system, &a(), "saar_spacedock"), 0);
}

#[test]
fn ffac2_2_production_is_seven() {
    assert_eq!(stats("saar_spacedock2").production(0), 7);
    assert_eq!(stats("saar_spacedock").production(0), 5);
}

#[test]
fn ffac2_3_is_placed_in_the_space_area() {
    assert!(stats("saar_spacedock2").is_space_only_structure());
}

#[test]
fn ffac2_4_moves_like_a_ship() {
    let mut state = seated(&[("a", "saar"), ("b", "sol")]);
    give(&mut state, "a", "ffac2");
    let hub = crate::fixtures::plain_hub();
    let origin = SystemId::new(hub.outer[0].clone());
    let active = SystemId::new(hub.centre.clone());
    put(&mut state, &origin, "saar_spacedock2", &a(), 1);
    let moves =
        crate::tactical::movable_into(&state, content(), DEFAULT, &hub.galaxy, &a(), &active);
    assert!(stats("saar_spacedock2").moves_as_ship());
    assert!(
        moves
            .iter()
            .any(|m| m.origin == origin && m.unit.type_id.as_str() == "saar_spacedock2")
    );
}

#[test]
fn ffac2_5_is_destroyed_when_blockaded() {
    let mut state = seated(&[("a", "saar"), ("b", "sol")]);
    let system = SystemId::new("19");
    put(&mut state, &system, "saar_spacedock2", &a(), 1);
    put(&mut state, &system, "cruiser", &b(), 1);
    let gone =
        crate::production::destroy_blockaded_mobile_docks(&mut state, content(), DEFAULT, &system);
    assert_eq!(gone.len(), 1);
    assert_eq!(in_space(&state, &system, &a(), "saar_spacedock2"), 0);
}

// -- Magmus Reactor -------------------------------------------------------------------------------

#[test]
fn mr_1_ships_can_move_into_supernovas() {
    let reach = |with_tech: bool| {
        let mut state = seated(&[("a", "muaat"), ("b", "sol")]);
        if with_tech {
            give(&mut state, "a", "mr");
        }
        let supernova = crate::fixtures::a_system_where("supernova");
        let hub = crate::fixtures::hub_with_centre(&supernova);
        let origin = SystemId::new(hub.outer[0].clone());
        put(&mut state, &origin, "cruiser", &a(), 1);
        crate::tactical::activate(&mut state, &a(), &SystemId::new(hub.centre.clone())).unwrap();
        crate::tactical::movable(&state, content(), POK, &hub.galaxy, &a()).len()
    };
    assert!(reach(true) > 0);
    assert_eq!(reach(false), 0);
}

#[test]
fn mr_2_a_supernova_with_your_units_gains_production_five() {
    let production = |with_tech: bool, units: bool| {
        let mut state = seated(&[("a", "muaat"), ("b", "sol")]);
        if with_tech {
            give(&mut state, "a", "mr");
        }
        let supernova = SystemId::new(crate::fixtures::a_system_where("supernova"));
        if units {
            put(&mut state, &supernova, "cruiser", &a(), 1);
        }
        crate::factions::hooks_economy::extra_production(&state, content(), POK, &a(), &supernova)
    };
    assert_eq!(production(true, true), 5);
    assert_eq!(production(true, false), 0, "needs a unit of yours there");
    assert_eq!(production(false, true), 0);
}

// -- Production Biomes ----------------------------------------------------------------------------

#[test]
fn pm_1_exhaust_and_a_token_for_four_trade_goods_and_two_to_another_player() {
    let mut state = seated(&[("a", "hacan"), ("b", "sol")]);
    let offered = |state: &GameState| {
        crate::faction_abilities::component_actions(state, content(), &a())
            .into_iter()
            .find(|option| option.id == "faction|production_biomes")
    };
    assert!(offered(&state).is_none(), "not without the card");
    give(&mut state, "a", "pm");
    state.player_mut(&a()).unwrap().strategic_tokens = 1;
    state.player_mut(&a()).unwrap().trade_goods = 0;
    state.player_mut(&b()).unwrap().trade_goods = 0;
    let option = offered(&state).expect("offered with the card and a token");
    let mut table = scripted(&["b"]);
    let done = crate::fixtures::with_context(&mut state, POK, None, &mut table, |context| {
        crate::faction_abilities::perform_component(context, &a(), &option)
    });
    assert!(done);
    let seat = state.player(&a()).unwrap();
    assert_eq!((seat.trade_goods, seat.strategic_tokens), (4, 0));
    assert_eq!(state.player(&b()).unwrap().trade_goods, 2);
    assert!(offered(&state).is_none(), "exhausted");
    assert!(owns(&state, "a", "pm"));
}

// -- Voidwatch ------------------------------------------------------------------------------------

#[test]
fn vw_1_a_player_moving_in_beside_your_units_gives_you_a_promissory_note() {
    let run = |with_tech: bool| {
        let mut state = seated(&[("a", "empyrean"), ("b", "sol")]);
        if with_tech {
            give(&mut state, "a", "vw");
        }
        let home = state.player(&a()).unwrap().home_system.clone().unwrap();
        let hand = crate::promissory::available_notes(&state, content(), &b());
        let pick = hand
            .iter()
            .find(|note| !note.starts_with(crate::promissory::SUPPORT_PREFIX))
            .cloned()
            .expect("a note in the mover's hand");
        let answer = format!("note|{pick}");
        emit_full(
            &mut state,
            &mut scripted(&[answer.as_str()]),
            None,
            "MOVEMENT_FINISHED",
            json(&[
                ("player", "b".into()),
                ("system", home.as_str().into()),
                ("ships_moved", 1.into()),
            ]),
        );
        state.promissory_notes.get(&pick) == Some(&a())
    };
    assert!(run(true), "the mover's chosen note went to the Empyrean");
    assert!(!run(false), "without the card nothing is taken");
}

// -- Spec Ops II ----------------------------------------------------------------------------------

#[test]
fn so2_1_prints_cost_one_for_two_and_combat_six_and_replaces_spec_ops() {
    let (old, new) = (stats("sol_infantry"), stats("sol_infantry2"));
    assert_eq!(old.combat_hits_on(), Some(7));
    assert_eq!(new.combat_hits_on(), Some(6));
    assert!((new.cost() - 0.5).abs() < f64::EPSILON, "1 for two units");
    assert_eq!(new.upgrades_from(), Some("sol_infantry"));
    assert_eq!(new.required_technology(), Some("so2"));
    let (system, planet) = crate::fixtures::a_placed_planet();
    let mut state = seated(&[("a", "sol"), ("b", "hacan")]);
    put_on_planet(&mut state, &system, &planet, "sol_infantry", &a(), 2);
    give(&mut state, "a", "so2");
    crate::technology::apply_unit_upgrades(&mut state, content(), DEFAULT, &a());
    let on = |kind: &str| {
        state
            .system_state(&system)
            .on_planet_of(&planet, &a())
            .iter()
            .filter(|u| u.type_id.as_str() == kind)
            .count()
    };
    assert_eq!((on("sol_infantry2"), on("sol_infantry")), (2, 0));
}

#[test]
fn so2_2_survives_on_a_five_and_comes_back_at_the_next_turn() {
    let mut state = seated(&[("a", "sol")]);
    let unit = ti4_model::units::Unit::new(ti4_model::id::UnitTypeId::new("sol_infantry2"), a());
    crate::faction_techs::note_destroyed(&mut state, &unit);
    crate::faction_techs::note_destroyed(&mut state, &unit);
    let mut dice = crate::dice::Dice::from_faces([5, 4]);
    let mut rng = crate::rng::GameRng::new(1);
    let rolled = crate::faction_techs::roll_spec_ops(&mut state, &mut dice, &mut rng);
    assert_eq!(
        rolled,
        vec![(a(), true), (a(), false)],
        "5 survives, 4 does not"
    );
    assert_eq!(state.player(&a()).unwrap().spec_ops_card, 1);
    let home = state.player(&a()).unwrap().home_system.clone().unwrap();
    let planet = state
        .controlled_planets(&a())
        .into_iter()
        .find(|(s, _)| **s == home)
        .map(|(_, p)| p.clone())
        .expect("a home planet");
    assert_eq!(crate::faction_techs::return_spec_ops(&mut state, &a()), 1);
    assert_eq!(
        state
            .system_state(&home)
            .on_planet_of(&planet, &a())
            .iter()
            .filter(|u| u.type_id.as_str() == "sol_infantry2")
            .count(),
        1
    );
}

// -- Wormhole Generator ---------------------------------------------------------------------------

fn ghost_galaxy() -> ti4_content::galaxy::Galaxy {
    let mut galaxy = ti4_content::galaxy::Galaxy::build(
        content(),
        &["18", "17", "39", "40", "26", "25", "19", "20", "21"],
        DEFAULT,
        3,
    )
    .expect("builds");
    crate::seating::place_creuss_home(&mut galaxy, content(), DEFAULT).expect("home");
    galaxy
}

#[test]
fn wg_1_exhaust_to_place_or_move_a_creuss_wormhole_token() {
    let mut state = seated(&[("a", "ghost"), ("b", "sol")]);
    let offered = |state: &GameState| {
        crate::factions::component_actions(state, content(), &a())
            .into_iter()
            .find(|option| option.id == "faction|ghost|wg")
    };
    assert!(offered(&state).is_none(), "no card, no action");
    give(&mut state, "a", "wg");
    let option = offered(&state).expect("offered with the card ready");
    let galaxy = ghost_galaxy();
    let mut table = scripted(&["ALPHA|19"]);
    let done =
        crate::fixtures::with_context(&mut state, DEFAULT, Some(&galaxy), &mut table, |context| {
            crate::factions::perform_component(context, &a(), &option)
        });
    assert!(done);
    assert_eq!(
        state.wormhole_tokens.get("ALPHA"),
        Some(&SystemId::new("19"))
    );
    assert!(offered(&state).is_none(), "exhausted");
}

// -- Dimensional Splicer --------------------------------------------------------------------------

#[test]
fn ds_1_produces_a_hit_at_the_start_of_combat_in_a_wormhole_system() {
    use crate::factions::hooks_combat::{CombatMoment, HitSite, ProducedHits};
    let hits = |with_tech: bool, system: &str| {
        let mut state = seated(&[("a", "ghost"), ("b", "sol")]);
        if with_tech {
            give(&mut state, "a", "ds");
        }
        let system = SystemId::new(system);
        put(&mut state, &system, "cruiser", &a(), 1);
        put(&mut state, &system, "destroyer", &b(), 1);
        let site = HitSite {
            player: &a(),
            opponent: &b(),
            system: &system,
            round: 1,
            moment: CombatMoment::CombatStart,
        };
        let mut table = scripted(&["produce_hit"]);
        crate::fixtures::with_context(&mut state, DEFAULT, None, &mut table, |context| {
            crate::factions::hooks_combat::produced_hits(context, &site)
        })
        .expect("legal")
    };
    assert_eq!(hits(true, "39").producer_assigned, 1);
    assert_eq!(hits(false, "39"), ProducedHits::NONE, "no card");
    assert_eq!(
        hits(true, "19"),
        ProducedHits::NONE,
        "no wormhole in the system"
    );
}

// -- Inheritance Systems --------------------------------------------------------------------------

#[test]
fn is_1_pay_two_resources_to_ignore_a_technologys_prerequisites() {
    let run = |with_tech: bool| {
        let mut state = seated(&[("a", "l1z1x")]);
        bare(&mut state, &a());
        if with_tech {
            give(&mut state, "a", "is");
        }
        state.player_mut(&a()).unwrap().trade_goods = 2;
        let war_sun = ti4_model::id::TechnologyId::new("ws");
        let done = crate::technology::research(&mut state, content(), POK, &a(), &war_sun);
        (
            done,
            owns(&state, "a", "ws"),
            state.player(&a()).unwrap().trade_goods,
        )
    };
    assert_eq!(
        run(true),
        (true, true, 0),
        "two resources paid, War Sun gained"
    );
    assert!(
        !run(false).1,
        "without the card the prerequisites are a wall"
    );
}

// -- Genetic Recombination ------------------------------------------------------------------------

#[test]
fn gr_1_binds_a_voter_to_an_outcome_or_costs_them_a_fleet_token() {
    use crate::vote::VoteWindow;
    let mut state = seated(&[("a", "mahact"), ("b", "xxcha"), ("c", "hacan")]);
    give(&mut state, "a", "gr");
    state.speaker = a();
    let mut window = VoteWindow::new(&state, "x", vec!["for".into(), "against".into()]);
    window.open(&state, content(), DEFAULT);
    let pick = |window: &mut VoteWindow, state: &mut GameState, id: &str| {
        let q = window.pending_choice(state, content(), DEFAULT).unwrap();
        let option = q
            .options
            .iter()
            .find(|o| o.id == id)
            .unwrap_or_else(|| panic!("{id} not offered"))
            .clone();
        window.resolve(state, content(), DEFAULT, option).unwrap();
    };
    assert_eq!(
        window
            .pending_choice(&state, content(), DEFAULT)
            .unwrap()
            .player,
        a(),
        "the holder is asked before b casts"
    );
    pick(&mut window, &mut state, "recombine|against");
    assert!(
        state
            .player(&a())
            .unwrap()
            .exhausted_technologies
            .contains(&ti4_model::id::TechnologyId::new("gr"))
    );
    let tokens = state.player(&b()).unwrap().fleet_tokens;
    pick(&mut window, &mut state, "tribute|token");
    assert_eq!(
        state.player(&b()).unwrap().fleet_tokens,
        tokens - 1,
        "or pay a fleet token"
    );
    // Without the card nobody is asked.
    let plain = seated(&[("a", "mahact"), ("b", "xxcha"), ("c", "hacan")]);
    let mut window = VoteWindow::new(&plain, "x", vec!["for".into(), "against".into()]);
    window.open(&plain, content(), DEFAULT);
    assert_ne!(
        window
            .pending_choice(&plain, content(), DEFAULT)
            .unwrap()
            .player,
        a()
    );
}

// -- Salvage Operations ---------------------------------------------------------------------------

fn combat_ended(system: &SystemId, winner: Option<&str>) -> BTreeMap<String, serde_json::Value> {
    let mut payload = json(&[
        ("system", system.as_str().into()),
        ("attacker", "a".into()),
        ("defender", "b".into()),
        (
            "destroyed",
            serde_json::json!([{"player": "b", "unit": "destroyer"}, {"player": "a", "unit": "cruiser"}]),
        ),
    ]);
    if let Some(winner) = winner {
        payload.insert("winner".to_owned(), winner.into());
    }
    payload
}

#[test]
fn so_1_a_trade_good_after_winning_or_losing_a_space_combat() {
    let run = |with_tech: bool, winner: Option<&str>| {
        let mut state = seated(&[("a", "mentak"), ("b", "sol")]);
        if with_tech {
            give(&mut state, "a", "so");
        }
        state.player_mut(&a()).unwrap().trade_goods = 0;
        let system = SystemId::new("18");
        emit_full(
            &mut state,
            &mut scripted(&[]),
            None,
            "SPACE_COMBAT_ENDED",
            combat_ended(&system, winner),
        );
        state.player(&a()).unwrap().trade_goods
    };
    assert_eq!(run(true, Some("b")), 1, "losing pays");
    assert_eq!(run(true, Some("a")), 1, "winning pays");
    assert_eq!(run(true, None), 0, "a draw is neither");
    assert_eq!(run(false, Some("b")), 0, "no card");
}

#[test]
fn so_1_the_winner_may_produce_a_ship_of_a_type_destroyed() {
    let mut state = seated(&[("a", "mentak"), ("b", "sol")]);
    give(&mut state, "a", "so");
    state.player_mut(&a()).unwrap().trade_goods = 1;
    let system = SystemId::new("18");
    let before = in_space(&state, &system, &a(), "cruiser");
    emit_full(
        &mut state,
        &mut scripted(&[
            "technology:mentak:so:SPACE_COMBAT_ENDED:produce",
            "cruiser",
            "trade_good",
            "trade_good",
        ]),
        None,
        "SPACE_COMBAT_ENDED",
        combat_ended(&system, Some("a")),
    );
    assert_eq!(in_space(&state, &system, &a(), "cruiser"), before + 1);
}

// -- Neuroglaive ----------------------------------------------------------------------------------

#[test]
fn ng_1_an_activator_beside_your_ships_loses_a_fleet_token() {
    let run = |with_tech: bool, activator: &str| {
        let mut state = seated(&[("a", "naalu"), ("b", "sol")]);
        if with_tech {
            give(&mut state, "a", "ng");
        }
        let system = SystemId::new("19");
        put(&mut state, &system, "cruiser", &a(), 1);
        let before = state.player(&pid(activator)).unwrap().fleet_tokens;
        emit(
            &mut state,
            &[],
            "SYSTEM_ACTIVATED",
            &[("player", activator), ("system", "19")],
        );
        before - state.player(&pid(activator)).unwrap().fleet_tokens
    };
    assert_eq!(run(true, "b"), 1);
    assert_eq!(run(true, "a"), 0, "not your own activation");
    assert_eq!(run(false, "b"), 0, "no card");
}

// -- Supercharge ----------------------------------------------------------------------------------

#[test]
fn sc_1_exhaust_for_plus_one_to_each_combat_roll_this_round() {
    let threshold = |with_tech: bool, answer: &str| {
        let mut state = seated(&[("a", "naaz"), ("b", "sol")]);
        if with_tech {
            give(&mut state, "a", "sc");
        }
        let system = SystemId::new("19");
        put(&mut state, &system, "cruiser", &a(), 1);
        state.active_system = Some(system.clone());
        crate::faction_abilities::space_combat_round_started(
            &mut state,
            content(),
            POK,
            &mut scripted(&[answer]),
            &a(),
        );
        let unit = state
            .system_state(&system)
            .units_of(&a())
            .into_iter()
            .find(|u| u.type_id.as_str() == "cruiser")
            .unwrap()
            .clone();
        let ready = crate::technology::technology_text_ready(&state, &a(), "sc");
        (
            crate::combat::effective_hits_on(&state, content(), POK, &a(), &unit),
            ready,
        )
    };
    assert_eq!(
        threshold(true, "sc"),
        (Some(6), false),
        "7 becomes 6 and the card is exhausted"
    );
    assert_eq!(threshold(true, "decline"), (Some(7), true));
    assert_eq!(
        threshold(false, "sc"),
        (Some(7), false),
        "no card: nothing exhausted, no bonus"
    );
}

// -- Valefar Assimilator Y ------------------------------------------------------------------------

/// A Nekro `a` whose opponent `b` (Yssaril) owns `mi`, in the middle of a space combat.
fn nekro_scene() -> (GameState, SystemId) {
    let mut state = seated(&[("a", "nekro"), ("b", "yssaril")]);
    for tech in ["mi", "tp", "sar"] {
        give(&mut state, "b", tech);
    }
    let system = state.player(&a()).unwrap().home_system.clone().unwrap();
    emit(
        &mut state,
        &[],
        "SPACE_COMBAT_STARTED",
        &[
            ("system", system.as_str()),
            ("attacker", "a"),
            ("defender", "b"),
        ],
    );
    (state, system)
}

/// Destroy a ship of `who`'s, answering the Singularity offer with `answers`; returns the options
/// the first question offered.
fn destroy_ship(state: &mut GameState, system: &SystemId, answers: &[&str]) -> Vec<String> {
    let (decider, seen) = crate::choice::Capturing::new(Box::new(Scripted::new(
        answers.iter().map(|s| (*s).to_owned()),
    )));
    let mut table = Table::with_default(Box::new(decider));
    let payload = json(&[
        ("system", system.as_str().into()),
        ("player", "b".into()),
        ("unit", "cruiser".into()),
        ("cause", "combat".into()),
        ("during_space_combat", true.into()),
    ]);
    emit_full(state, &mut table, None, "SHIP_DESTROYED", payload);
    let ids = seen
        .borrow()
        .iter()
        .flat_map(|c| c.ids().into_iter().map(str::to_owned).collect::<Vec<_>>())
        .collect();
    ids
}

const SINGULARITY: &str = "ability:nekro:technological_singularity:SHIP_DESTROYED:after";

#[test]
fn vay_1_a_token_may_go_on_a_faction_technology_instead_of_gaining_it() {
    let (mut state, system) = nekro_scene();
    destroy_ship(&mut state, &system, &[SINGULARITY, "y|b|mi"]);
    let nekro = state.player(&a()).unwrap();
    assert_eq!(
        nekro
            .assimilated_technologies
            .get("vay")
            .map(|t| t.as_str()),
        Some("mi")
    );
    assert!(!owns(&state, "a", "mi"), "assimilated, never owned");
}

#[test]
fn vay_2_the_card_gains_the_text_of_the_technology_with_its_token() {
    let (mut state, system) = nekro_scene();
    assert!(!crate::technology::has_technology_text(&state, &a(), "mi"));
    destroy_ship(&mut state, &system, &[SINGULARITY, "y|b|mi"]);
    assert!(crate::technology::has_technology_text(&state, &a(), "mi"));
    assert!(crate::technology::exhaust_technology_text(
        &mut state,
        &a(),
        "mi"
    ));
    let exhausted = &state.player(&a()).unwrap().exhausted_technologies;
    assert!(
        exhausted.contains(&ti4_model::id::TechnologyId::new("vay")),
        "it exhausts the Valefar card"
    );
}

#[test]
fn vay_3_a_technology_with_a_token_takes_no_second_token() {
    let (mut state, system) = nekro_scene();
    destroy_ship(&mut state, &system, &[SINGULARITY, "x|b|mi"]);
    // The next combat opens the once-per-combat offer afresh.
    emit(
        &mut state,
        &[],
        "SPACE_COMBAT_ENDED",
        &[
            ("system", system.as_str()),
            ("attacker", "a"),
            ("defender", "b"),
        ],
    );
    emit(
        &mut state,
        &[],
        "SPACE_COMBAT_STARTED",
        &[
            ("system", system.as_str()),
            ("attacker", "a"),
            ("defender", "b"),
        ],
    );
    let offered = destroy_ship(&mut state, &system, &[SINGULARITY, "y|b|tp"]);
    assert!(
        offered.iter().any(|id| id == "y|b|tp"),
        "other technologies still take the token: {offered:?}"
    );
    assert!(
        !offered.iter().any(|id| id.ends_with("|mi")),
        "mi already carries the X token: {offered:?}"
    );
}

// -- Temporal Command Suite -----------------------------------------------------------------------

#[test]
fn tcs_1_readies_an_exhausted_agent_and_may_open_a_transaction() {
    use ti4_model::state::LeaderStatus;
    let hub = crate::fixtures::plain_hub();
    let run = |with_tech: bool| {
        let mut state = seated(&[("a", "nomad"), ("b", "sol")]);
        if with_tech {
            give(&mut state, "a", "tcs");
        }
        state.player_mut(&b()).unwrap().leaders.insert(
            ti4_model::id::LeaderId::new("solagent"),
            LeaderStatus::Exhausted,
        );
        let mut table = scripted(&[
            "technology:nomad:tcs:AGENT_EXHAUSTED:after",
            "transact",
            "decline",
        ]);
        emit_full(
            &mut state,
            &mut table,
            Some(&hub.galaxy),
            crate::factions::nomad::AGENT_EXHAUSTED,
            json(&[("player", "b".into()), ("leader", "solagent".into())]),
        );
        let status =
            crate::leaders::status(&state, &b(), &ti4_model::id::LeaderId::new("solagent"));
        (status, state.transacted_with(&a()).contains(&b()))
    };
    assert_eq!(
        run(true),
        (Some(LeaderStatus::Readied), true),
        "ready, and a transaction with a non-neighbour"
    );
    assert_eq!(run(false).0, Some(LeaderStatus::Exhausted), "no card");
}

// -- Valkyrie Particle Weave ----------------------------------------------------------------------

#[test]
fn vpw_1_one_additional_ground_hit_when_the_opponent_produced_hits() {
    let (system, planet) = crate::fixtures::a_placed_planet();
    let extra = |with_tech: bool, who: &PlayerId, own: usize, theirs: usize| {
        let mut state = seated(&[("a", "sardakk"), ("b", "sol")]);
        if with_tech {
            give(&mut state, "a", "vpw");
        }
        crate::factions::hooks_ground::ground_rolls_extra_hits(
            &state,
            content(),
            DEFAULT,
            who,
            &system,
            &planet,
            own,
            theirs,
        )
    };
    assert_eq!(extra(true, &a(), 0, 1), 1);
    assert_eq!(extra(true, &a(), 4, 2), 1, "one more, not one per hit");
    assert_eq!(extra(true, &a(), 3, 0), 0, "the opponent produced none");
    assert_eq!(extra(true, &b(), 0, 2), 0, "only the holder");
    assert_eq!(extra(false, &a(), 0, 2), 0, "no card");
}

// -- Hel-Titan II ---------------------------------------------------------------------------------

#[test]
fn ht2_1_combat_six_and_it_fights_as_a_ground_force_and_replaces_hel_titan() {
    let (old, new) = (stats("titans_pds"), stats("titans_pds2"));
    assert_eq!(
        (old.combat_hits_on(), new.combat_hits_on()),
        (Some(7), Some(6))
    );
    assert_eq!(new.upgrades_from(), Some("titans_pds"));
    assert_eq!(new.required_technology(), Some("ht2"));
    let (system, planet) = crate::fixtures::a_placed_planet();
    let mut state = seated(&[("a", "titans"), ("b", "sol")]);
    put_on_planet(&mut state, &system, &planet, "titans_pds", &a(), 1);
    give(&mut state, "a", "ht2");
    crate::technology::apply_unit_upgrades(&mut state, content(), DEFAULT, &a());
    assert_eq!(
        on_planet(&state, &system, planet.as_str(), &a(), "titans_pds2"),
        1
    );
    put_on_planet(&mut state, &system, &planet, "infantry", &b(), 2);
    let mut dice = crate::dice::Dice::new();
    let mut rng = crate::rng::GameRng::new(0);
    crate::invasion::ground_combat(
        &mut state,
        content(),
        DEFAULT,
        &mut scripted(&[]),
        &mut dice,
        &mut rng,
        &system,
        &planet,
        &b(),
    )
    .expect("resolves");
    assert!(
        dice.history()
            .iter()
            .any(|roll| roll.by.as_deref() == Some("a") && roll.hits_on == Some(6))
    );
}

#[test]
fn ht2_2_has_planetary_shield_space_cannon_five_sustain_and_production_one() {
    let pds = stats("titans_pds2");
    assert!(pds.planetary_shield());
    assert_eq!(pds.space_cannon_hits_on(), Some(5));
    assert!(pds.sustain_damage());
    assert_eq!(pds.production(3), 1, "PRODUCTION 1 whatever the planet");
    let (system, planet) = crate::fixtures::a_placed_planet();
    let mut state = seated(&[("a", "titans"), ("b", "sol")]);
    bare(&mut state, &a());
    put_on_planet(&mut state, &system, &planet, "titans_pds2", &a(), 1);
    state.system_mut(&system).set_control(planet, a());
    assert_eq!(
        crate::production::capacity(&state, content(), DEFAULT, &a(), &system),
        1
    );
}

#[test]
fn ht2_3_is_a_structure_and_a_ground_force_and_cannot_be_transported() {
    let pds = stats("titans_pds2");
    assert!(pds.is_structure() && pds.is_ground_force());
    let (system, planet) = crate::fixtures::a_placed_planet();
    let mut state = seated(&[("a", "titans"), ("b", "sol")]);
    put_on_planet(&mut state, &system, &planet, "titans_pds2", &a(), 1);
    put_on_planet(&mut state, &system, &planet, "infantry", &a(), 1);
    let loadable = crate::transit::loadable(&state, content(), DEFAULT, &a(), &system);
    assert!(
        loadable
            .iter()
            .all(|c| c.unit.type_id.as_str() != "titans_pds2")
    );
    assert!(
        loadable
            .iter()
            .any(|c| c.unit.type_id.as_str() == "infantry")
    );
}

#[test]
fn ht2_4_space_cannon_reaches_ships_in_adjacent_systems() {
    let hub = crate::fixtures::plain_hub();
    let centre = SystemId::new(hub.centre.clone());
    let beside = SystemId::new(hub.outer[0].clone());
    let fired = |with_unit: bool| {
        let mut state = seated(&[("a", "titans"), ("b", "sol")]);
        put(&mut state, &centre, "cruiser", &b(), 1);
        if with_unit {
            put_on_planet(
                &mut state,
                &beside,
                &PlanetId::new("mellon"),
                "titans_pds2",
                &a(),
                1,
            );
        }
        let mut dice = crate::dice::Dice::new();
        let mut rng = crate::rng::GameRng::new(0);
        crate::combat::space_cannon_offense(
            &mut state,
            content(),
            DEFAULT,
            &mut dice,
            &mut rng,
            &centre,
            &b(),
            Some(&hub.galaxy),
        );
        dice.history()
            .iter()
            // Sol starts with Antimass Deflectors, so the PDS II's 5 is asked as a 6.
            .filter(|roll| roll.by.as_deref() == Some("a") && roll.hits_on == Some(6))
            .count()
    };
    assert_eq!(fired(true), 1, "next door, as the card says");
    assert_eq!(fired(false), 0);
}

// -- E-Res Siphons --------------------------------------------------------------------------------

#[test]
fn ers_1_four_trade_goods_when_another_player_activates_your_ships_system() {
    let run = |with_tech: bool, activator: &PlayerId| {
        let mut state = seated(&[("a", "jolnar"), ("b", "sol")]);
        if with_tech {
            give(&mut state, "a", "ers");
        }
        let system = SystemId::new("19");
        put(&mut state, &system, "cruiser", &a(), 1);
        let before = state.player(&a()).unwrap().trade_goods;
        crate::faction_techs::e_res_siphons(&mut state, content(), POK, &system, activator);
        state.player(&a()).unwrap().trade_goods - before
    };
    assert_eq!(run(true, &b()), 4);
    assert_eq!(run(true, &a()), 0, "not your own activation");
    assert_eq!(run(false, &b()), 0, "no card");
}

// -- Vortex ---------------------------------------------------------------------------------------

#[test]
fn vtx_1_captures_a_unit_from_another_players_reinforcements() {
    let run = |with_tech: bool| {
        let (dock_system, dock_planet) = crate::fixtures::a_placed_planet();
        let hub = crate::fixtures::hub_with_centre(dock_system.as_str());
        let mut state = seated(&[("a", "cabal"), ("b", "sol")]);
        for id in hub.galaxy.system_ids() {
            let system = state.system_mut(&SystemId::new(id));
            system.units.clear();
            system.planet_units.clear();
        }
        for id in state.board.keys().cloned().collect::<Vec<SystemId>>() {
            let system = state.system_mut(&id);
            system
                .units
                .retain(|u| !(u.owner == a() && u.type_id.as_str().contains("spacedock")));
            for units in system.planet_units.values_mut() {
                units.retain(|u| !(u.owner == a() && u.type_id.as_str().contains("spacedock")));
            }
        }
        if with_tech {
            give(&mut state, "a", "vtx");
        }
        put_on_planet(
            &mut state,
            &dock_system,
            &dock_planet,
            "cabal_spacedock",
            &a(),
            1,
        );
        put(
            &mut state,
            &SystemId::new(hub.outer[0].clone()),
            "cruiser",
            &b(),
            1,
        );
        let option = crate::factions::mapped_component_actions(
            &state,
            content(),
            DEFAULT,
            &hub.galaxy,
            &a(),
        )
        .into_iter()
        .find(|option| option.id == "faction|cabal|vortex");
        let Some(option) = option else {
            return (false, vec![]);
        };
        let mut table = scripted(&[]);
        let done = crate::fixtures::with_context(
            &mut state,
            DEFAULT,
            Some(&hub.galaxy),
            &mut table,
            |context| crate::factions::perform_component(context, &a(), &option),
        );
        (done, crate::supply::captured_by(&state, &a()).to_vec())
    };
    let (done, captured) = run(true);
    assert!(done);
    assert_eq!(
        captured,
        vec![(b(), ti4_model::id::UnitTypeId::new("cruiser"))]
    );
    assert_eq!(run(false), (false, vec![]), "no card, no action");
}

// -- Hegemonic Trade Policy -----------------------------------------------------------------------

#[test]
fn htp_1_swaps_resource_and_influence_of_one_planet_for_one_use_of_production() {
    use crate::production::Spend;
    let mut state = seated(&[("a", "winnu"), ("b", "sol")]);
    let value = |state: &GameState, planet: &PlanetId, kind| {
        crate::production::planet_value_now(state, content(), DEFAULT, planet, kind)
    };
    let planet = state
        .controlled_planets(&a())
        .into_iter()
        .map(|(_, p)| p.clone())
        .find(|p| value(&state, p, Spend::Resources) != value(&state, p, Spend::Influence))
        .expect("a home planet with unequal values");
    let system = state.player(&a()).unwrap().home_system.clone().unwrap();
    state.production_seq += 1;
    emit(
        &mut state,
        &["decline"],
        "PRODUCTION_USED",
        &[("player", "a"), ("system", system.as_str())],
    );
    assert!(
        crate::production::swapped_planet(&state).is_none(),
        "no card, no swap"
    );
    give(&mut state, "a", "htp");
    state.production_seq += 1;
    emit(
        &mut state,
        &[
            "technology:winnu:htp:PRODUCTION_USED:after",
            planet.as_str(),
        ],
        "PRODUCTION_USED",
        &[("player", "a"), ("system", system.as_str())],
    );
    assert_eq!(crate::production::swapped_planet(&state), Some(&planet));
    let res = value(&state, &planet, Spend::Resources);
    let inf = value(&state, &planet, Spend::Influence);
    assert_eq!(
        crate::production::swapped_value(&state, content(), DEFAULT, &planet, Spend::Resources),
        Some(inf)
    );
    assert_eq!(
        crate::production::swapped_value(&state, content(), DEFAULT, &planet, Spend::Influence),
        Some(res)
    );
    assert!(
        !crate::technology::technology_text_ready(&state, &a(), "htp"),
        "exhausted"
    );
}
