//! Technology effect tests, group g2: faction technologies `lw2` .. `mi` (see mod.rs).

use ti4_content::units::UnitType;
use ti4_model::content_types::POK;
use ti4_model::id::SystemId;

use std::collections::BTreeSet;

use ti4_content::galaxy::Galaxy;
use ti4_model::id::{PlanetId, TechnologyId};
use ti4_model::state::{GameState, RerollEntry, RerollSet};

use super::{content, give, owns, pid, seated};
use crate::fixtures::put;

/// An ordinary system holding no units yet.
fn empty_system(state: &ti4_model::state::GameState) -> SystemId {
    let id = crate::fixtures::plain_systems(60)
        .into_iter()
        .find(|id| {
            state
                .board
                .get(&SystemId::new(id.as_str()))
                .is_none_or(|s| s.units.is_empty() && s.planet_units.values().all(Vec::is_empty))
        })
        .expect("an empty system");
    SystemId::new(id)
}

/// Seat `a` of `faction` with one of the faction's own `base` units on the board; assert that
/// without `tech` the unit stays the printed one and that with it the card replaces it. Returns
/// the upgraded unit type.
fn upgrade(faction: &str, tech: &str, base: &str) -> UnitType<'static> {
    let mut state = seated(&[("a", faction)]);
    let system = empty_system(&state);
    let printed = ti4_content::units::catalogue(content(), POK)
        .values()
        .find(|u| {
            u.base_type() == base
                && u.faction() == Some(faction)
                && u.required_technology().is_none()
        })
        .expect("the faction has its own printed unit")
        .id()
        .to_owned();
    put(&mut state, &system, &printed, &pid("a"), 2);
    assert!(
        !owns(&state, "a", tech),
        "{faction} does not start with {tech}"
    );
    crate::technology::apply_unit_upgrades(&mut state, content(), POK, &pid("a"));
    assert!(
        state
            .system_state(&system)
            .units
            .iter()
            .all(|u| u.type_id.as_str() == printed),
        "without {tech} the printed unit {printed} stays: {:?}",
        state
            .system_state(&system)
            .units
            .iter()
            .map(|u| u.type_id.to_string())
            .collect::<Vec<_>>()
    );
    give(&mut state, "a", tech);
    crate::technology::apply_unit_upgrades(&mut state, content(), POK, &pid("a"));
    let now: Vec<String> = state
        .system_state(&system)
        .units
        .iter()
        .map(|u| u.type_id.to_string())
        .collect();
    assert_eq!(now.len(), 2);
    assert!(
        now.iter().all(|id| *id != printed),
        "{tech} replaces {printed}: {now:?}"
    );
    let upgraded = now[0].clone();
    ti4_content::units::unit_type(content(), &upgraded, POK).expect("the upgraded unit exists")
}

/// Cost, combat value and dice.
fn assert_combat(unit: &UnitType<'_>, cost: f64, hits_on: i64, dice: i64) {
    assert!(
        (unit.cost() - cost).abs() < 1e-9,
        "cost {} != {cost}",
        unit.cost()
    );
    assert_eq!(
        (unit.combat_hits_on(), unit.combat_dice()),
        (Some(hits_on), dice)
    );
}

/// Move and capacity.
fn assert_mobility(unit: &UnitType<'_>, move_value: i64, capacity: i64) {
    assert_eq!((unit.move_value(), unit.capacity()), (move_value, capacity));
}

#[test]
fn lw2_1_letani_warrior_ii_stats() {
    let unit = upgrade("arborec", "lw2", "infantry");
    assert_combat(&unit, 0.5, 7, 1);
    assert!(unit.produces_two(), "Cost 1(x2)");
}

#[test]
fn swa2_1_strike_wing_alpha_ii_stats() {
    let unit = upgrade("argent", "swa2", "destroyer");
    assert_combat(&unit, 1.0, 7, 1);
    assert_mobility(&unit, 2, 1);
}

#[test]
fn swa2_2_strike_wing_alpha_ii_anti_fighter_barrage() {
    let unit = upgrade("argent", "swa2", "destroyer");
    assert_eq!((unit.afb_hits_on(), unit.afb_dice()), (Some(6), 3));
}

#[test]
fn pws2_1_prototype_war_sun_ii_stats() {
    let unit = upgrade("muaat", "pws2", "warsun");
    assert_combat(&unit, 10.0, 3, 3);
    assert_mobility(&unit, 3, 6);
}

#[test]
fn pws2_2_prototype_war_sun_ii_sustain_and_bombardment() {
    let unit = upgrade("muaat", "pws2", "warsun");
    assert!(unit.sustain_damage());
    assert_eq!((unit.bombard_hits_on(), unit.bombard_dice()), (Some(3), 3));
}

#[test]
fn ac2_1_advanced_carrier_ii_stats() {
    let unit = upgrade("sol", "ac2", "carrier");
    assert_combat(&unit, 3.0, 9, 1);
    assert_mobility(&unit, 2, 8);
}

#[test]
fn ac2_2_advanced_carrier_ii_sustain_damage() {
    assert!(upgrade("sol", "ac2", "carrier").sustain_damage());
}

#[test]
fn sdn2_1_super_dreadnought_ii_stats() {
    let unit = upgrade("l1z1x", "sdn2", "dreadnought");
    assert_combat(&unit, 4.0, 4, 1);
    assert_mobility(&unit, 2, 2);
}

#[test]
fn sdn2_2_super_dreadnought_ii_sustain_and_bombardment() {
    let unit = upgrade("l1z1x", "sdn2", "dreadnought");
    assert!(unit.sustain_damage());
    assert_eq!((unit.bombard_hits_on(), unit.bombard_dice()), (Some(4), 1));
}

#[test]
fn cl2_1_crimson_legionnaire_ii_stats() {
    let unit = upgrade("mahact", "cl2", "infantry");
    assert_combat(&unit, 0.5, 7, 1);
    assert!(unit.produces_two());
}

#[test]
fn hcf2_1_hybrid_crystal_fighter_ii_stats() {
    let unit = upgrade("naalu", "hcf2", "fighter");
    assert_combat(&unit, 0.5, 7, 1);
    assert_eq!(unit.move_value(), 2);
}

#[test]
fn m2_1_memoria_ii_stats() {
    let unit = upgrade("nomad", "m2", "flagship");
    assert_combat(&unit, 8.0, 5, 2);
    assert_mobility(&unit, 2, 6);
}

#[test]
fn m2_2_memoria_ii_sustain_and_anti_fighter_barrage() {
    let unit = upgrade("nomad", "m2", "flagship");
    assert!(unit.sustain_damage());
    assert_eq!((unit.afb_hits_on(), unit.afb_dice()), (Some(5), 3));
}

#[test]
fn exo2_1_exotrireme_ii_stats() {
    let unit = upgrade("sardakk", "exo2", "dreadnought");
    assert_combat(&unit, 4.0, 5, 1);
    assert_mobility(&unit, 2, 1);
}

#[test]
fn exo2_2_exotrireme_ii_sustain_and_bombardment() {
    let unit = upgrade("sardakk", "exo2", "dreadnought");
    assert!(unit.sustain_damage());
    assert_eq!((unit.bombard_hits_on(), unit.bombard_dice()), (Some(4), 2));
}

#[test]
fn se2_1_saturn_engine_ii_stats() {
    let unit = upgrade("titans", "se2", "cruiser");
    assert_combat(&unit, 2.0, 6, 1);
    assert_mobility(&unit, 3, 2);
}

#[test]
fn se2_2_saturn_engine_ii_sustain_damage() {
    assert!(upgrade("titans", "se2", "cruiser").sustain_damage());
}

#[test]
fn dt2_1_dimensional_tear_ii_production_seven() {
    let unit = upgrade("cabal", "dt2", "spacedock");
    assert_eq!(unit.production(2), 7);
}

// -- shared helpers ---------------------------------------------------------------------------

/// Emit one typed event through a resolver armed exactly as the game arms it, answering prompts
/// from `answers` (then declining) and rolling `faces` first. Returns the dice rolled.
fn emit(
    state: &mut GameState,
    answers: &[&str],
    faces: &[u32],
    galaxy: Option<&Galaxy>,
    event_type: &str,
    pairs: &[(&str, &str)],
) -> usize {
    let decider = crate::choice::Scripted::with_fallback(
        answers.iter().map(|a| (*a).to_owned()),
        Box::new(crate::choice::AlwaysDecline),
    );
    emit_decider(state, Box::new(decider), faces, galaxy, event_type, pairs).1
}

/// [`emit`] with any decider; returns the resolved event and the dice rolled.
fn emit_decider(
    state: &mut GameState,
    decider: Box<dyn crate::choice::Decider>,
    faces: &[u32],
    galaxy: Option<&Galaxy>,
    event_type: &str,
    pairs: &[(&str, &str)],
) -> (crate::event::Event, usize) {
    let mut resolver = crate::fixtures::armed_resolver(state);
    let mut table = crate::choice::Table::with_default(decider);
    let mut dice = crate::dice::Dice::from_faces(faces.iter().copied());
    let mut rng = crate::rng::GameRng::new(0);
    let mut sequence = crate::event::EventSequence::new();
    let mut context = crate::timing::TimingContext {
        state,
        content: content(),
        sources: POK,
        table: &mut table,
        dice: &mut dice,
        rng: &mut rng,
        event_sequence: &mut sequence,
        galaxy,
    };
    let payload = pairs
        .iter()
        .map(|(k, v)| {
            let value = match *v {
                "true" => serde_json::Value::Bool(true),
                "false" => serde_json::Value::Bool(false),
                text => serde_json::Value::from(text),
            };
            ((*k).to_owned(), value)
        })
        .collect();
    let event = context
        .event_sequence
        .next(event_type, payload)
        .expect("an event id");
    let done = resolver
        .emit_with_context(&mut context, event, |_, _| {})
        .expect("the window resolves");
    (done, context.dice.history().len())
}

/// How many of `player`'s `kind` units stand in the space area of `system`.
fn count(state: &GameState, system: &SystemId, player: &str, kind: &str) -> usize {
    state
        .system_state(system)
        .units
        .iter()
        .filter(|u| u.owner == pid(player) && u.type_id.as_str() == kind)
        .count()
}

/// How many of `player`'s `kind` units stand on `planet` in `system`.
fn count_on(
    state: &GameState,
    system: &SystemId,
    planet: &PlanetId,
    player: &str,
    kind: &str,
) -> usize {
    state
        .system_state(system)
        .on_planet_of(planet, &pid(player))
        .iter()
        .filter(|u| u.type_id.as_str() == kind)
        .count()
}

/// The seat's home system and its first controlled planet there.
fn home_planet(state: &GameState, player: &str) -> (SystemId, PlanetId) {
    let home = state
        .player(&pid(player))
        .and_then(|s| s.home_system.clone())
        .expect("home");
    let planet = state
        .controlled_planets(&pid(player))
        .into_iter()
        .find(|(system, _)| **system == home)
        .map(|(_, planet)| planet.clone())
        .expect("a home planet");
    (home, planet)
}

/// Stage `faces` as `player`'s just-rolled barrage dice of `unit` in `system`.
fn stage_barrage(
    state: &mut GameState,
    player: &str,
    system: &SystemId,
    unit: &str,
    hits_on: u32,
    faces: &[u32],
) {
    state.reroll_staging.insert(
        pid(player),
        RerollSet {
            kind: "anti_fighter_barrage".to_owned(),
            system: system.clone(),
            rolls: vec![RerollEntry {
                unit: unit.to_owned(),
                planet: None,
                hits_on: Some(hits_on),
                faces: faces.to_vec(),
                rerolled: BTreeSet::new(),
                deltas: std::collections::BTreeMap::new(),
                unit_types: std::iter::once((unit.to_owned(), 1)).collect(),
            }],
        },
    );
}

fn learn(state: &mut GameState, player: &str, tech: &str) {
    state
        .player_mut(&pid(player))
        .unwrap()
        .technologies
        .insert(TechnologyId::new(tech));
}

// -- Arborec: Letani Warrior II -----------------------------------------------------------------

#[test]
fn lw2_2_letani_warrior_ii_has_production_two() {
    let mut state = seated(&[("a", "arborec")]);
    let system = empty_system(&state);
    let planet = PlanetId::new("lw2-test-planet");
    state
        .system_mut(&system)
        .set_control(planet.clone(), pid("a"));
    crate::fixtures::put_on_planet(
        &mut state,
        &system,
        &planet,
        "arborec_infantry",
        &pid("a"),
        1,
    );
    let before = crate::production::capacity(&state, content(), POK, &pid("a"), &system);
    assert_eq!(before, 1, "the printed Letani Warrior has PRODUCTION 1");
    state
        .system_mut(&system)
        .planet_units
        .get_mut(&planet)
        .unwrap()
        .clear();
    crate::fixtures::put_on_planet(
        &mut state,
        &system,
        &planet,
        "arborec_infantry2",
        &pid("a"),
        1,
    );
    assert_eq!(
        crate::production::capacity(&state, content(), POK, &pid("a"), &system),
        2
    );
}

#[test]
fn lw2_3_letani_warrior_ii_returns_home_after_a_six() {
    let (mut state, home, planet) = {
        let state = seated(&[("a", "arborec"), ("b", "sol")]);
        let (home, planet) = home_planet(&state, "a");
        (state, home, planet)
    };
    learn(&mut state, "a", "lw2");
    let pairs = [
        ("player", "a"),
        ("unit", "arborec_infantry2"),
        ("system", home.as_str()),
        ("planet", planet.as_str()),
    ];
    let before = count_on(&state, &home, &planet, "a", "arborec_infantry2");
    assert_eq!(
        emit(
            &mut state,
            &[],
            &[6],
            None,
            "GROUND_FORCE_DESTROYED",
            &pairs
        ),
        1
    );
    emit(&mut state, &[], &[], None, "TURN_BEGAN", &[("player", "a")]);
    assert_eq!(
        count_on(&state, &home, &planet, "a", "arborec_infantry2"),
        before + 1
    );
    // A 5 does not save it, and a seat without the card does not roll at all.
    let mut miss = seated(&[("a", "arborec"), ("b", "sol")]);
    learn(&mut miss, "a", "lw2");
    emit(&mut miss, &[], &[5], None, "GROUND_FORCE_DESTROYED", &pairs);
    emit(&mut miss, &[], &[], None, "TURN_BEGAN", &[("player", "a")]);
    assert_eq!(count_on(&miss, &home, &planet, "a", "arborec_infantry2"), 0);
    let mut bare = seated(&[("a", "arborec"), ("b", "sol")]);
    assert_eq!(
        emit(&mut bare, &[], &[6], None, "GROUND_FORCE_DESTROYED", &pairs),
        0
    );
}

// -- Argent: Strike Wing Alpha II ---------------------------------------------------------------

#[test]
fn swa2_3_strike_wing_alpha_ii_barrage_nines_destroy_infantry() {
    let build = |tech: bool| {
        let mut state = seated(&[("a", "argent"), ("b", "sol")]);
        let system = empty_system(&state);
        if tech {
            learn(&mut state, "a", "swa2");
        }
        put(&mut state, &system, "argent_destroyer2", &pid("a"), 1);
        put(&mut state, &system, "cruiser", &pid("b"), 1);
        put(&mut state, &system, "infantry", &pid("b"), 3);
        stage_barrage(
            &mut state,
            "a",
            &system,
            "argent_destroyer2",
            6,
            &[9, 10, 4],
        );
        (state, system)
    };
    let (mut state, system) = build(true);
    let pairs = [
        ("player", "a"),
        ("system", system.as_str()),
        ("kind", "anti_fighter_barrage"),
    ];
    emit(&mut state, &[], &[], None, "UNIT_ABILITY_ROLLED", &pairs);
    assert_eq!(
        count(&state, &system, "b", "infantry"),
        1,
        "the 9 and the 10 each kill one"
    );
    assert_eq!(count(&state, &system, "b", "cruiser"), 1);
    let (mut state, system) = build(false);
    emit(&mut state, &[], &[], None, "UNIT_ABILITY_ROLLED", &pairs);
    assert_eq!(
        count(&state, &system, "b", "infantry"),
        3,
        "no card, no kill"
    );
}

// -- Letnev: L4 Disruptors ----------------------------------------------------------------------

/// Answers a commit prompt with the first unit, anything else with its first option.
struct CommitAll;

impl crate::choice::Decider for CommitAll {
    fn choose(
        &mut self,
        choice: &crate::choice::Choice,
    ) -> Result<crate::choice::ChoiceOption, crate::choice::IllegalChoice> {
        let pick = choice
            .options
            .iter()
            .find(|o| o.id.starts_with("commit|"))
            .or_else(|| choice.options.iter().find(|o| o.id == "done_committing"))
            .or_else(|| choice.options.first());
        pick.cloned()
            .ok_or(crate::choice::IllegalChoice::NoOptions {
                player: choice.player.clone(),
                prompt: choice.prompt.clone(),
            })
    }
}

/// A placed planet that is not in Mecatol Rex's system.
fn planet_off_mecatol() -> (SystemId, PlanetId) {
    let planets = ti4_content::galaxy::all_planets(content(), POK);
    let (id, p) = planets
        .iter()
        .find(|(_, p)| {
            p.system_id().is_some_and(|sys| {
                planets
                    .iter()
                    .filter(|(_, q)| q.system_id() == Some(sys))
                    .count()
                    == 1
            }) && !p.is_placed_during_play()
                && p.system_id() != Some(crate::seating::MECATOL)
                && p.homeworld_of().is_none()
        })
        .expect("a placed planet");
    (SystemId::new(p.system_id().unwrap()), PlanetId::new(*id))
}

#[test]
fn l4_1_l4_disruptors_silence_space_cannon_defense_during_an_invasion() {
    // Seven landing infantry against one PDS that hits on every die: without L4 the PDS kills
    // some of them, with L4 none.
    let landed = |tech: bool| {
        let mut state = crate::fixtures::game(&["a", "b"]);
        let (system, planet) = planet_off_mecatol();
        crate::fixtures::put_on_planet(&mut state, &system, &planet, "pds", &pid("b"), 1);
        state
            .system_mut(&system)
            .set_control(planet.clone(), pid("b"));
        put(&mut state, &system, "carrier", &pid("a"), 2);
        put(&mut state, &system, "infantry", &pid("a"), 7);
        if tech {
            learn(&mut state, "a", "l4");
        }
        let mut table = crate::choice::Table::with_default(Box::new(CommitAll));
        let mut dice = crate::dice::Dice::from_faces([10u32; 40]);
        let mut rng = crate::rng::GameRng::new(3);
        let report = crate::invasion::resolve(
            &mut state,
            content(),
            POK,
            &mut table,
            &mut dice,
            &mut rng,
            &system,
            &pid("a"),
        )
        .expect("the invasion resolves");
        assert!(!report.committed.is_empty());
        state
            .system_state(&system)
            .planet_units
            .values()
            .flatten()
            .filter(|u| u.owner == pid("a") && u.type_id.as_str() == "infantry")
            .count()
    };
    assert_eq!(landed(true), 7, "no space cannon fire at the Letnev forces");
    assert!(landed(false) < 7, "the PDS shoots without L4 Disruptors");
}

// -- Saar: Chaos Mapping ------------------------------------------------------------------------

#[test]
fn cm_1_chaos_mapping_bars_other_players_from_activating_your_asteroid_field() {
    let field = crate::fixtures::a_system_where("asteroid field");
    let hub = crate::fixtures::hub_with_outer(&field);
    let mut state = crate::fixtures::game(&["a", "b"]);
    let barred = |state: &GameState, who: &str| {
        !crate::tactical::activatable_with(state, content(), POK, &hub.galaxy, &pid(who))
            .contains(&SystemId::new(field.as_str()))
    };
    put(
        &mut state,
        &SystemId::new(field.as_str()),
        "destroyer",
        &pid("a"),
        1,
    );
    assert!(!barred(&state, "b"), "without the card the field is open");
    learn(&mut state, "a", "cm");
    assert!(barred(&state, "b"), "other players cannot activate it");
    assert!(!barred(&state, "a"), "the holder still can");
    // Only a field holding one of the holder's ships.
    let mut empty = crate::fixtures::game(&["a", "b"]);
    learn(&mut empty, "a", "cm");
    assert!(!barred(&empty, "b"), "no ship of the holder's there");
}

#[test]
fn cm_2_chaos_mapping_produces_a_unit_at_the_start_of_the_turn() {
    let build = |tech: bool| {
        let mut state = crate::fixtures::game(&["a"]);
        if tech {
            learn(&mut state, "a", "cm");
        }
        let (system, planet) = crate::fixtures::a_placed_planet();
        state
            .system_mut(&system)
            .set_control(planet.clone(), pid("a"));
        crate::fixtures::put_on_planet(&mut state, &system, &planet, "spacedock", &pid("a"), 1);
        state.player_mut(&pid("a")).unwrap().trade_goods = 1;
        let mut table =
            crate::choice::Table::with_default(Box::new(crate::choice::Scripted::new([
                system.to_string(),
                "build|destroyer|1".to_owned(),
                "trade_good".to_owned(),
            ])));
        crate::technology::start_turn(&mut state, content(), POK, None, &mut table, &pid("a"))
            .unwrap();
        count(&state, &system, "a", "destroyer")
    };
    assert_eq!(build(true), 1);
    assert_eq!(build(false), 0);
}

// -- Muaat: Prototype War Sun II ----------------------------------------------------------------

#[test]
fn pws2_3_prototype_war_sun_ii_ignores_planetary_shield() {
    let (system, planet) = planet_off_mecatol();
    let mut state = seated(&[("a", "muaat"), ("b", "sol")]);
    crate::fixtures::put_on_planet(&mut state, &system, &planet, "pds", &pid("b"), 1);
    put(&mut state, &system, "dreadnought", &pid("a"), 1);
    let bombardable = |state: &GameState| {
        crate::invasion::bombardable(state, content(), POK, &system, &planet, &pid("a"))
    };
    assert!(
        !bombardable(&state),
        "a PDS shields the planet from a dreadnought"
    );
    give(&mut state, "a", "pws2");
    crate::technology::apply_unit_upgrades(&mut state, content(), POK, &pid("a"));
    put(&mut state, &system, "muaat_warsun2", &pid("a"), 1);
    assert!(
        bombardable(&state),
        "the upgraded war sun removes the shield"
    );
}

// -- Hacan: Quantum Datahub Node ----------------------------------------------------------------

#[test]
fn qdn_1_quantum_datahub_node_swaps_strategy_cards_for_three_trade_goods_and_a_token() {
    let build = |tech: bool| {
        let mut state = crate::fixtures::game(&["a", "b"]);
        let (high, low) = (
            ti4_model::id::StrategyCardId::new("imperial"),
            ti4_model::id::StrategyCardId::new("leadership"),
        );
        if tech {
            learn(&mut state, "a", "qdn");
        }
        let seat = state.player_mut(&pid("a")).unwrap();
        seat.trade_goods = 5;
        seat.strategic_tokens = 1;
        seat.strategy_cards = vec![high.clone()];
        state.player_mut(&pid("b")).unwrap().strategy_cards = vec![low.clone()];
        (state, high, low)
    };
    let (mut state, high, low) = build(true);
    let mut table =
        crate::choice::Table::with_default(Box::new(crate::choice::Scripted::new([format!(
            "qdn|b|{high}|{low}"
        )])));
    let used =
        crate::faction_techs::offer_quantum_datahub(&mut state, content(), POK, &mut table, None);
    assert_eq!(used, Some((pid("a"), pid("b"))));
    let (a, b) = (
        state.player(&pid("a")).unwrap(),
        state.player(&pid("b")).unwrap(),
    );
    assert_eq!(
        (a.trade_goods, a.strategic_tokens, b.trade_goods),
        (2, 0, 3)
    );
    assert_eq!(a.strategy_cards, vec![low.clone()]);
    assert_eq!(b.strategy_cards, vec![high]);
    let (mut state, _, _) = build(false);
    let mut table = crate::choice::Table::with_default(Box::new(crate::choice::FirstOption));
    assert_eq!(
        crate::faction_techs::offer_quantum_datahub(&mut state, content(), POK, &mut table, None),
        None
    );
}

// -- Empyrean: Aetherstream ---------------------------------------------------------------------

#[test]
fn as_1_aetherstream_adds_one_move_next_to_an_anomaly() {
    let build = |tech: bool| {
        let mut state = seated(&[("a", "empyrean"), ("b", "sol")]);
        for system in state.board.keys().cloned().collect::<Vec<_>>() {
            let board = state.system_mut(&system);
            board.units.clear();
            board.planet_units.clear();
            board.planet_control.clear();
        }
        let asteroid = crate::fixtures::a_system_where("asteroid field");
        let hub = crate::fixtures::hub_with_centre(&asteroid);
        put(
            &mut state,
            &SystemId::new(hub.outer[0].as_str()),
            "carrier",
            &pid("a"),
            1,
        );
        put(
            &mut state,
            &SystemId::new(hub.outer[1].as_str()),
            "carrier",
            &pid("b"),
            1,
        );
        if tech {
            learn(&mut state, "a", "as");
        }
        (state, hub)
    };
    let move_of_b = |state: &GameState, hub: &crate::fixtures::Hub| {
        let kind = ti4_content::units::unit_type(content(), "carrier", POK).unwrap();
        let origin = SystemId::new(hub.outer[1].as_str());
        crate::tactical::effective_move_value(state, &kind, &pid("b"), &origin)
    };
    for (tech, bonus) in [(true, 1), (false, 0)] {
        let (mut state, hub) = build(tech);
        let printed = move_of_b(&state, &hub);
        state.activation_seq += 1;
        let target = hub.outer[3].clone();
        emit(
            &mut state,
            if tech {
                &["technology:empyrean:as:SYSTEM_ACTIVATED:after"]
            } else {
                &[]
            },
            &[],
            Some(&hub.galaxy),
            "SYSTEM_ACTIVATED",
            &[("player", "b"), ("system", target.as_str())],
        );
        assert_eq!(move_of_b(&state, &hub), printed + bonus, "tech {tech}");
    }
}

// -- Mentak: Mirror Computing -------------------------------------------------------------------

#[test]
fn mc_1_mirror_computing_makes_a_trade_good_worth_two() {
    let plans = |tech: bool| {
        let mut state = crate::fixtures::game(&["a"]);
        state.player_mut(&pid("a")).unwrap().trade_goods = 2;
        if tech {
            learn(&mut state, "a", "mc");
        }
        crate::payment::plans(
            &state,
            content(),
            POK,
            &pid("a"),
            3,
            crate::production::Spend::Influence,
        )
    };
    assert!(!plans(true).is_empty(), "two goods pay up to four");
    assert!(
        plans(false).is_empty(),
        "without the card two goods pay two"
    );
}

// -- Mahact: Crimson Legionnaire II -------------------------------------------------------------

#[test]
fn cl2_2_crimson_legionnaire_ii_pays_a_commodity_then_returns_home() {
    let infantry_at_home = |state: &GameState, home: &SystemId| {
        state
            .system_state(home)
            .planet_units
            .values()
            .flatten()
            .filter(|u| u.owner == pid("a") && u.type_id.as_str().ends_with("infantry"))
            .count()
    };
    let mut state = seated(&[("a", "mahact"), ("b", "sol")]);
    let (home, planet) = home_planet(&state, "a");
    state.player_mut(&pid("a")).unwrap().commodities = 0;
    let pairs = |unit: &'static str| {
        [
            ("player", "a"),
            ("unit", unit),
            ("system", "x"),
            ("planet", "y"),
        ]
    };
    // The first-generation Legionnaire pays out but does not come back.
    emit(
        &mut state,
        &[],
        &[],
        None,
        "GROUND_FORCE_DESTROYED",
        &pairs("mahact_infantry"),
    );
    assert_eq!(state.player(&pid("a")).unwrap().commodities, 1);
    let before = infantry_at_home(&state, &home);
    emit(
        &mut state,
        &[planet.as_str()],
        &[],
        None,
        "TURN_BEGAN",
        &[("player", "a")],
    );
    assert_eq!(
        infantry_at_home(&state, &home),
        before,
        "nothing on the card"
    );
    // The II pays out, goes on the card and lands at the start of the next turn.
    emit(
        &mut state,
        &[],
        &[],
        None,
        "GROUND_FORCE_DESTROYED",
        &pairs("mahact_infantry2"),
    );
    assert_eq!(state.player(&pid("a")).unwrap().commodities, 2);
    emit(
        &mut state,
        &[planet.as_str()],
        &[],
        None,
        "TURN_BEGAN",
        &[("player", "b")],
    );
    assert_eq!(
        infantry_at_home(&state, &home),
        before,
        "not on another player's turn"
    );
    emit(
        &mut state,
        &[planet.as_str()],
        &[],
        None,
        "TURN_BEGAN",
        &[("player", "a")],
    );
    assert_eq!(infantry_at_home(&state, &home), before + 1);
}

// -- Naalu: Hybrid Crystal Fighter II -----------------------------------------------------------

#[test]
fn hcf2_2_hybrid_crystal_fighter_ii_moves_without_being_transported() {
    let hub = crate::fixtures::plain_hub();
    let movable_fighters = |kind: &str| {
        let mut state = crate::fixtures::game(&["a"]);
        let origin = SystemId::new(hub.outer[0].clone());
        let target = SystemId::new(hub.centre.clone());
        put(&mut state, &origin, kind, &pid("a"), 1);
        crate::tactical::activate(&mut state, &pid("a"), &target).unwrap();
        crate::tactical::movable(&state, content(), POK, &hub.galaxy, &pid("a")).len()
    };
    assert_eq!(movable_fighters("naalu_fighter2"), 1, "it moves on its own");
    assert_eq!(
        movable_fighters("naalu_fighter"),
        0,
        "the printed fighter needs a carrier"
    );
}

#[test]
fn hcf2_3_excess_hybrid_crystal_fighter_ii_counts_half_a_ship() {
    let charged = |faction: &str, kind: &str| {
        let mut state = seated(&[("a", faction), ("b", "hacan")]);
        let system = empty_system(&state);
        put(&mut state, &system, kind, &pid("a"), 4);
        crate::fleet::standing(&state, content(), POK, &pid("a"), &system, None).fleet_charged
    };
    assert_eq!(
        charged("naalu", "naalu_fighter2"),
        2,
        "four excess fighters cost two ships"
    );
    assert_eq!(
        charged("sol", "fighter2"),
        4,
        "an ordinary Fighter II costs a ship each"
    );
}

// -- Direct Hit immunity (Super Dreadnought II, Exotrireme II) ----------------------------------

#[test]
fn sdn2_3_super_dreadnought_ii_cannot_be_destroyed_by_direct_hit() {
    assert!(crate::combat::direct_hittable(
        content(),
        POK,
        "l1z1x_dreadnought"
    ));
    assert!(!crate::combat::direct_hittable(
        content(),
        POK,
        "l1z1x_dreadnought2"
    ));
}

#[test]
fn exo2_3_exotrireme_ii_cannot_be_destroyed_by_direct_hit() {
    assert!(crate::combat::direct_hittable(
        content(),
        POK,
        "sardakk_dreadnought"
    ));
    assert!(!crate::combat::direct_hittable(
        content(),
        POK,
        "sardakk_dreadnought2"
    ));
}

// -- Sardakk: Exotrireme II ---------------------------------------------------------------------

#[test]
fn exo2_4_exotrireme_ii_destroys_itself_and_up_to_two_ships_after_a_combat_round() {
    let build = |tech: bool| {
        let mut state = seated(&[("a", "sardakk"), ("b", "sol")]);
        let (home, _) = home_planet(&state, "a");
        state.system_mut(&home).units.clear();
        put(&mut state, &home, "sardakk_dreadnought2", &pid("a"), 1);
        put(&mut state, &home, "cruiser", &pid("b"), 3);
        if tech {
            give(&mut state, "a", "exo2");
        }
        (state, home)
    };
    let pairs = |home: &SystemId| {
        [
            ("system", home.to_string()),
            ("round", "1".into()),
            ("attacker", "a".into()),
            ("defender", "b".into()),
        ]
    };
    for (tech, answers, cruisers_left) in [
        (
            true,
            &[
                "technology:sardakk:exo2:SPACE_COMBAT_ROUND_ENDED:after",
                "ship|0",
                "ship|0",
            ][..],
            1,
        ),
        (
            false,
            &["technology:sardakk:exo2:SPACE_COMBAT_ROUND_ENDED:after"][..],
            3,
        ),
    ] {
        let (mut state, home) = build(tech);
        let owned = pairs(&home);
        let refs: Vec<(&str, &str)> = owned.iter().map(|(k, v)| (*k, v.as_str())).collect();
        emit(
            &mut state,
            if tech { answers } else { &[] },
            &[],
            None,
            "SPACE_COMBAT_ROUND_ENDED",
            &refs,
        );
        assert_eq!(
            count(&state, &home, "b", "cruiser"),
            cruisers_left,
            "tech {tech}"
        );
        assert_eq!(
            count(&state, &home, "a", "sardakk_dreadnought2"),
            usize::from(!tech)
        );
    }
}

// -- Nomad: Memoria II --------------------------------------------------------------------------

#[test]
fn m2_3_memoria_ii_is_adjacent_to_systems_with_your_mechs() {
    let hub = crate::fixtures::plain_hub();
    let origin = hub.outer[0].clone();
    let active = hub.across(&origin);
    let beside = hub
        .galaxy
        .adjacent(&active)
        .into_iter()
        .map(ToOwned::to_owned)
        .find(|id| *id != hub.centre)
        .expect("a ring neighbour");
    let flagships = |flagship: &str, with_mech: bool| {
        let mut state = seated(&[("a", "nomad"), ("b", "sol")]);
        put(
            &mut state,
            &SystemId::new(origin.clone()),
            flagship,
            &pid("a"),
            1,
        );
        put(
            &mut state,
            &SystemId::new(hub.centre.clone()),
            "destroyer",
            &pid("b"),
            1,
        );
        if with_mech {
            crate::fixtures::put_on_planet(
                &mut state,
                &SystemId::new(beside.clone()),
                &PlanetId::new("m2-test"),
                "nomad_mech",
                &pid("a"),
                1,
            );
        }
        crate::tactical::movable_into(
            &state,
            content(),
            POK,
            &hub.galaxy,
            &pid("a"),
            &SystemId::new(active.clone()),
        )
        .into_iter()
        .filter(|m| m.unit.type_id.as_str() == flagship && m.origin.as_str() == origin)
        .count()
    };
    assert_eq!(flagships("nomad_flagship2", false), 0, "blockaded, no mech");
    assert_eq!(
        flagships("nomad_flagship2", true),
        1,
        "adjacent to the mech"
    );
}

// -- Cabal: Dimensional Tear II -----------------------------------------------------------------

#[test]
fn dt2_2_dimensional_tear_ii_is_a_gravity_rift_its_owner_does_not_roll_for() {
    let (dock_system, dock_planet) = crate::fixtures::a_placed_planet();
    let hub = crate::fixtures::hub_with_outer(dock_system.as_str());
    let mut state = seated(&[("a", "cabal"), ("b", "sol")]);
    for board in state.board.values_mut() {
        board.units.clear();
    }
    crate::fixtures::put_on_planet(
        &mut state,
        &dock_system,
        &dock_planet,
        "cabal_spacedock2",
        &pid("a"),
        1,
    );
    let rules = |who: &str| {
        let mut rules = crate::movement::MovementRules::new(
            &hub.galaxy,
            content(),
            POK,
            &hub.centre,
            crate::movement::Board::default(),
        );
        crate::action_cards::apply_movement_effects(&mut rules, &state, &pid(who));
        rules
    };
    let path = vec![dock_system.to_string(), hub.centre.clone()];
    let theirs = rules("b");
    assert!(
        theirs.is_rift(dock_system.as_str()),
        "the system is a gravity rift"
    );
    assert_eq!(
        crate::transit::rifts_exited(&theirs, &path),
        vec![dock_system.to_string()]
    );
    let ours = rules("a");
    assert!(ours.is_rift(dock_system.as_str()));
    assert!(
        crate::transit::rifts_exited(&ours, &path).is_empty(),
        "no roll for the owner's ships"
    );
}

#[test]
fn dt2_3_dimensional_tear_ii_carries_twelve_fighters_free() {
    let over = |dock: &str, fighters: usize| {
        let (system, planet) = crate::fixtures::a_placed_planet();
        let mut state = seated(&[("a", "cabal"), ("b", "sol")]);
        for board in state.board.values_mut() {
            board.units.retain(|u| u.owner != pid("a"));
            for units in board.planet_units.values_mut() {
                units.retain(|u| u.owner != pid("a"));
            }
        }
        crate::fixtures::put_on_planet(&mut state, &system, &planet, dock, &pid("a"), 1);
        put(&mut state, &system, "fighter", &pid("a"), fighters);
        crate::fleet::over_capacity(&state, content(), POK, &pid("a"), &system)
    };
    assert_eq!(over("cabal_spacedock2", 12), 0);
    assert_eq!(over("cabal_spacedock2", 13), 1);
    assert_eq!(
        over("cabal_spacedock", 7),
        1,
        "the first-generation tear carries only six"
    );
}

// -- Naaz-Rokha: Pre-Fab Arcologies -------------------------------------------------------------

#[test]
fn pfa_1_pre_fab_arcologies_readies_a_planet_after_exploring_it() {
    let explore = |tech: bool| {
        let mut state = seated(&[("a", "naaz"), ("b", "sol")]);
        let catalogue = ti4_content::galaxy::all_planets(content(), POK);
        let planet = crate::fixtures::non_home_planets(200)
            .into_iter()
            .map(PlanetId::new)
            .find(|p| {
                crate::exploration::trait_of(content(), POK, p).is_some()
                    && catalogue
                        .get(p.as_str())
                        .is_some_and(|c| c.system_id().is_some())
            })
            .expect("an explorable planet");
        let system = SystemId::new(catalogue[planet.as_str()].system_id().unwrap());
        state
            .system_mut(&system)
            .set_control(planet.clone(), pid("a"));
        state.exhausted_planets.insert(planet.clone());
        if tech {
            give(&mut state, "a", "pfa");
        }
        let deck = crate::exploration::trait_of(content(), POK, &planet).unwrap();
        crate::exploration::explore(&mut state, content(), &pid("a"), &deck, Some(&planet))
            .expect("a card");
        state.exhausted_planets.contains(&planet)
    };
    assert!(!explore(true), "the planet is ready again");
    assert!(explore(false), "without the card it stays exhausted");
}

// -- Winnu: Lazax Gate Folding ------------------------------------------------------------------

fn mecatol_planet() -> PlanetId {
    let planets = ti4_content::galaxy::all_planets(content(), POK);
    let (id, _) = planets
        .iter()
        .find(|(_, p)| p.system_id() == Some(crate::seating::MECATOL))
        .expect("Mecatol Rex");
    PlanetId::new(*id)
}

#[test]
fn lgf_1_lazax_gate_folding_gives_mecatol_rex_both_wormholes_on_a_tactical_action() {
    // Mecatol Rex at the centre and 39 two rings out, so the two are not physically adjacent.
    let mut tiles: Vec<String> = vec!["18".to_owned()];
    tiles.extend(
        crate::fixtures::plain_systems(12)
            .into_iter()
            .filter(|id| id != "39" && id != "18")
            .take(6),
    );
    tiles.push("39".to_owned());
    let refs: Vec<&str> = tiles.iter().map(String::as_str).collect();
    let galaxy = Galaxy::build(content(), &refs, POK, 2).expect("galaxy");
    assert!(!galaxy.adjacent("18").contains("39"));
    let adjacent = |tech: bool, holds: bool| {
        let mut state = seated(&[("a", "winnu"), ("b", "sol")]);
        if tech {
            give(&mut state, "a", "lgf");
        }
        if holds {
            state
                .system_mut(&SystemId::new(crate::seating::MECATOL))
                .set_control(mecatol_planet(), pid("a"));
        } else {
            state
                .system_mut(&SystemId::new(crate::seating::MECATOL))
                .planet_control
                .clear();
        }
        state.active = Some(pid("a"));
        state.active_system = Some(SystemId::new("39"));
        crate::movement::PlayerAdjacency::new(&state, content(), POK, &galaxy, &pid("a"))
            .are_adjacent("18", "39")
    };
    assert!(adjacent(true, false), "without Mecatol Rex the gate folds");
    assert!(!adjacent(true, true), "holding Mecatol Rex it does not");
    assert!(!adjacent(false, false), "no technology, no wormholes");
}

#[test]
fn lgf_2_lazax_gate_folding_action_places_an_infantry_on_mecatol_rex() {
    let mut state = seated(&[("a", "winnu"), ("b", "sol")]);
    give(&mut state, "a", "lgf");
    let mecatol = SystemId::new(crate::seating::MECATOL);
    let planet = mecatol_planet();
    let actions = |state: &GameState| {
        crate::faction_abilities::component_actions(state, content(), &pid("a"))
    };
    assert!(
        actions(&state).iter().all(|o| !o.id.contains("lgf")),
        "not without Mecatol Rex"
    );
    state
        .system_mut(&mecatol)
        .set_control(planet.clone(), pid("a"));
    let option = actions(&state)
        .into_iter()
        .find(|o| o.id.contains("lgf"))
        .expect("the action is offered");
    let before = count_on(&state, &mecatol, &planet, "a", "infantry");
    let mut table = crate::choice::Table::with_default(Box::new(crate::choice::FirstOption));
    let done = crate::fixtures::with_context(&mut state, POK, None, &mut table, |ctx| {
        crate::faction_abilities::perform_component(ctx, &pid("a"), &option)
    });
    assert!(done);
    assert_eq!(
        count_on(&state, &mecatol, &planet, "a", "infantry"),
        before + 1
    );
    assert!(
        !crate::technology::technology_text_ready(&state, &pid("a"), "lgf"),
        "exhausted"
    );
    assert!(
        actions(&state).iter().all(|o| !o.id.contains("lgf")),
        "once per exhaust"
    );
}

// -- Xxcha: Instinct Training -------------------------------------------------------------------

#[test]
fn it_1_instinct_training_cancels_another_players_action_card() {
    let play = |tech: bool| {
        let mut state = seated(&[("a", "sol"), ("b", "xxcha")]);
        if tech {
            give(&mut state, "b", "it");
        }
        state.player_mut(&pid("b")).unwrap().strategic_tokens = 2;
        let answers: &[&str] = if tech {
            &["technology:xxcha:it:ACTION_CARD_PLAYED:when", "use"]
        } else {
            &[]
        };
        let (event, _) = emit_decider(
            &mut state,
            Box::new(crate::choice::Scripted::with_fallback(
                answers.iter().map(|a| (*a).to_owned()),
                Box::new(crate::choice::AlwaysDecline),
            )),
            &[],
            None,
            "ACTION_CARD_PLAYED",
            &[("player", "a"), ("card", "fs1")],
        );
        (event.cancelled, state)
    };
    let (cancelled, state) = play(true);
    assert!(cancelled);
    let seat = state.player(&pid("b")).unwrap();
    assert_eq!(seat.strategic_tokens, 1, "a strategy token is spent");
    assert!(
        seat.exhausted_technologies
            .contains(&TechnologyId::new("it"))
    );
    assert!(!play(false).0, "no technology, no cancel");
}

// -- Yssaril: Mageon Implants -------------------------------------------------------------------

#[test]
fn mi_1_mageon_implants_takes_a_card_from_another_players_hand() {
    let mut state = seated(&[("a", "yssaril"), ("b", "sol")]);
    let hand = |state: &GameState, who: &str| -> Vec<String> {
        state
            .player(&pid(who))
            .unwrap()
            .action_cards
            .iter()
            .map(ToString::to_string)
            .collect()
    };
    state.player_mut(&pid("a")).unwrap().action_cards.clear();
    state.player_mut(&pid("b")).unwrap().action_cards = ["sabotage", "bribery"]
        .iter()
        .map(|c| ti4_model::id::ActionCardId::new(*c))
        .collect();
    let id = "faction|yssaril|mi|b";
    let offered = |state: &GameState| {
        crate::faction_abilities::component_actions(state, content(), &pid("a"))
            .iter()
            .any(|o| o.id == id)
    };
    assert!(!offered(&state), "not without the technology");
    give(&mut state, "a", "mi");
    assert!(offered(&state));
    let option = crate::faction_abilities::component_actions(&state, content(), &pid("a"))
        .into_iter()
        .find(|o| o.id == id)
        .unwrap();
    let mut table =
        crate::choice::Table::with_default(Box::new(crate::choice::Scripted::new(["bribery"])));
    assert!(crate::fixtures::with_context(
        &mut state,
        POK,
        None,
        &mut table,
        |ctx| { crate::faction_abilities::perform_component(ctx, &pid("a"), &option) }
    ));
    assert_eq!(hand(&state, "a"), ["bribery"]);
    assert_eq!(hand(&state, "b"), ["sabotage"]);
    assert!(!offered(&state), "exhausted");
}

// -- Nekro Virus: Valefar Assimilator X ---------------------------------------------------------

const SINGULARITY: &str = "ability:nekro:technological_singularity:SHIP_DESTROYED:after";

/// A Nekro `a` holding the Valefar cards against a Yssaril `b` who owns Mageon Implants.
fn nekro_vs_yssaril() -> (GameState, SystemId) {
    let mut state = seated(&[("a", "nekro"), ("b", "yssaril")]);
    give(&mut state, "a", "vax");
    give(&mut state, "a", "vay");
    give(&mut state, "b", "mi");
    let (home, _) = home_planet(&state, "a");
    (state, home)
}

/// Start a space combat in `home` and destroy one of `b`'s ships, answering `decider`.
fn singularity(state: &mut GameState, home: &SystemId, decider: Box<dyn crate::choice::Decider>) {
    emit(
        state,
        &[],
        &[],
        None,
        "SPACE_COMBAT_STARTED",
        &[
            ("system", home.as_str()),
            ("attacker", "a"),
            ("defender", "b"),
        ],
    );
    let (_, _) = emit_decider(
        state,
        decider,
        &[],
        None,
        "SHIP_DESTROYED",
        &[
            ("system", home.as_str()),
            ("player", "b"),
            ("unit", "cruiser"),
            ("cause", "combat"),
            ("during_space_combat", "true"),
        ],
    );
}

fn scripted_decider(answers: &[&str]) -> Box<dyn crate::choice::Decider> {
    Box::new(crate::choice::Scripted::with_fallback(
        answers.iter().map(|a| (*a).to_owned()),
        Box::new(crate::choice::AlwaysDecline),
    ))
}

#[test]
fn vax_1_valefar_assimilator_x_token_goes_on_another_players_faction_technology() {
    let (mut state, home) = nekro_vs_yssaril();
    // The during-combat flag is part of the real event.
    singularity(
        &mut state,
        &home,
        scripted_decider(&[SINGULARITY, "x|b|mi"]),
    );
    let seat = state.player(&pid("a")).unwrap();
    assert_eq!(
        seat.assimilated_technologies.get("vax"),
        Some(&TechnologyId::new("mi"))
    );
    assert!(!owns(&state, "a", "mi"), "assimilated, never owned");
}

#[test]
fn vax_2_valefar_assimilator_x_gains_the_text_of_the_technology_it_sits_on() {
    let (mut state, _) = nekro_vs_yssaril();
    state.player_mut(&pid("b")).unwrap().action_cards =
        vec![ti4_model::id::ActionCardId::new("sabotage")];
    let mageon = |state: &GameState| {
        crate::faction_abilities::component_actions(state, content(), &pid("a"))
            .iter()
            .any(|o| o.id.starts_with("faction|yssaril|mi|"))
    };
    assert!(!crate::technology::has_technology_text(
        &state,
        &pid("a"),
        "mi"
    ));
    assert!(!mageon(&state));
    state
        .player_mut(&pid("a"))
        .unwrap()
        .assimilated_technologies
        .insert("vax".to_owned(), TechnologyId::new("mi"));
    assert!(crate::technology::has_technology_text(
        &state,
        &pid("a"),
        "mi"
    ));
    assert!(
        mageon(&state),
        "Mageon Implants works for the Nekro through vax"
    );
    state
        .player_mut(&pid("b"))
        .unwrap()
        .technologies
        .remove(&TechnologyId::new("mi"));
    assert!(
        !crate::technology::has_technology_text(&state, &pid("a"), "mi"),
        "lost with its owner's technology"
    );
}

#[test]
fn vax_3_an_assimilator_token_cannot_go_on_a_technology_that_already_has_one() {
    let offered = |token_on_mi: bool| -> Vec<String> {
        let (mut state, home) = nekro_vs_yssaril();
        if token_on_mi {
            state
                .player_mut(&pid("a"))
                .unwrap()
                .assimilated_technologies
                .insert("vax".to_owned(), TechnologyId::new("mi"));
        }
        let (capturing, seen) =
            crate::choice::Capturing::new(scripted_decider(&[SINGULARITY, "gain|b|nm"]));
        singularity(&mut state, &home, Box::new(capturing));
        let ids = seen
            .borrow()
            .iter()
            .flat_map(|c| c.options.iter().map(|o| o.id.clone()))
            .collect();
        ids
    };
    assert!(
        offered(false).contains(&"y|b|mi".to_owned()),
        "control: a free token may go on it"
    );
    assert!(
        !offered(true).contains(&"y|b|mi".to_owned()),
        "but not on a technology that has a token"
    );
}

// -- Jol-Nar: Spatial Conduit Cylinders ---------------------------------------------------------

#[test]
fn scc_1_spatial_conduit_cylinders_link_the_active_system_to_every_system_with_your_units() {
    let build = |tech: bool| {
        let mut state = crate::fixtures::game(&["a"]);
        let (here, far) = (SystemId::new("19"), SystemId::new("40"));
        put(&mut state, &here, "cruiser", &pid("a"), 1);
        put(&mut state, &far, "carrier", &pid("a"), 1);
        if tech {
            learn(&mut state, "a", "scc");
        }
        state.active = Some(pid("a"));
        state.active_system = Some(here.clone());
        state.activation_seq = 7;
        (state, here, far)
    };
    let (mut state, here, far) = build(true);
    assert!(
        crate::faction_techs::spatial_conduit_links(&state).is_empty(),
        "nothing until it is used"
    );
    let mut table = crate::choice::Table::with_default(Box::new(crate::choice::FirstOption));
    assert!(crate::faction_techs::offer_spatial_conduit(
        &mut state,
        content(),
        POK,
        &mut table,
        None,
        &here,
        &pid("a")
    ));
    let links = crate::faction_techs::spatial_conduit_links(&state);
    assert!(links[here.as_str()].contains(far.as_str()));
    assert!(links[far.as_str()].contains(here.as_str()));
    let (mut state, here, _) = build(false);
    let mut table = crate::choice::Table::with_default(Box::new(crate::choice::FirstOption));
    assert!(!crate::faction_techs::offer_spatial_conduit(
        &mut state,
        content(),
        POK,
        &mut table,
        None,
        &here,
        &pid("a")
    ));
}
