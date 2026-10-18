//! Technology effect tests, group g4: faction technologies of Xxcha, Yin, Yssaril, Keleres, Nekro,
//! and the Thunder's Edge factions (see mod.rs). Test names carry the row number of
//! `docs/TECH_EFFECT_COVERAGE.md`.

use std::collections::BTreeMap;

use ti4_model::content_types::{DEFAULT, POK};
use ti4_model::id::{PlanetId, SystemId, TechnologyId};
use ti4_model::state::GameState;

use super::{content, give, pid};

/// Seated and deployed at the default sources (Thunder's Edge factions need them).
fn seated(seats: &[(&str, &str)]) -> GameState {
    crate::fixtures::seated_game(seats, DEFAULT)
}
use crate::choice::{ChoiceOption, Scripted, Table};

// -- helpers -----------------------------------------------------------------------------------

fn scripted(answers: &[&str]) -> Table {
    Table::with_default(Box::new(Scripted::new(answers.iter().copied())))
}

/// Emit one typed event through a resolver armed as the game arms one.
fn emit(state: &mut GameState, answers: &[&str], kind: &str, pairs: &[(&str, serde_json::Value)]) {
    let mut resolver = crate::fixtures::armed_resolver(state);
    let mut table = scripted(answers);
    crate::fixtures::with_context(state, DEFAULT, None, &mut table, |ctx| {
        let payload: BTreeMap<String, serde_json::Value> = pairs
            .iter()
            .map(|(key, value)| ((*key).to_owned(), value.clone()))
            .collect();
        let event = ctx.event_sequence.next(kind, payload).expect("event id");
        resolver
            .emit_with_context(ctx, event, |_, _| {})
            .expect("window resolves");
    });
}

/// The component actions `player` is offered, by option id.
fn offered(state: &GameState, player: &str) -> Vec<String> {
    crate::faction_abilities::component_actions(state, content(), &pid(player))
        .into_iter()
        .map(|option| option.id)
        .collect()
}

/// Perform the faction component action `id` for `player`, answering `answers`.
fn perform(state: &mut GameState, player: &str, id: &str, answers: &[&str]) -> bool {
    let mut table = scripted(answers);
    let option = ChoiceOption::labelled(id, "component", id);
    crate::fixtures::with_context(state, POK, None, &mut table, |ctx| {
        crate::faction_abilities::perform_component(ctx, &pid(player), &option)
    })
}

fn units_in(state: &GameState, system: &str, kind: &str) -> usize {
    state.board.get(&SystemId::new(system)).map_or(0, |board| {
        board
            .units
            .iter()
            .filter(|u| u.type_id.as_str() == kind)
            .count()
    })
}

fn on_planet(state: &GameState, system: &str, planet: &str, kind: &str) -> usize {
    state
        .board
        .get(&SystemId::new(system))
        .and_then(|board| board.planet_units.get(&PlanetId::new(planet)))
        .map_or(0, |units| {
            units.iter().filter(|u| u.type_id.as_str() == kind).count()
        })
}

/// Fight the combat in `system` through the combat window, armed as the game arms it; returns the
/// `(player, unit)` of every `SHIP_DESTROYED`, in order.
fn fought(mut state: GameState, system: &SystemId, answers: &[&str]) -> Vec<(String, String)> {
    use crate::choice::{Resolving, TimingHandle, Window};
    use crate::timing::{Ability, Relation};
    use std::sync::{Arc, Mutex};
    let seen: Arc<Mutex<Vec<(String, String)>>> = Arc::default();
    let mut resolver = crate::fixtures::armed_resolver(&state);
    let log = Arc::clone(&seen);
    resolver.register([Ability::new(
        "probe:SHIP_DESTROYED",
        pid("a"),
        "SHIP_DESTROYED",
        Relation::After,
        Arc::new(move |event, _| {
            log.lock().unwrap().push((
                event.text("player").unwrap_or_default().to_owned(),
                event.text("unit").unwrap_or_default().to_owned(),
            ));
            Ok(())
        }),
    )]);
    let mut sequence = crate::event::EventSequence::new();
    let mut table = scripted(answers);
    let mut dice = crate::dice::Dice::new();
    let mut rng = crate::rng::GameRng::new(5);
    let mut window = crate::combat::CombatWindow::new(&state, content(), DEFAULT, system);
    let mut ctx = Resolving {
        content: content(),
        sources: DEFAULT,
        dice: &mut dice,
        rng: &mut rng,
        table: &mut table,
        timing: Some(TimingHandle {
            resolver: &mut resolver,
            sequence: &mut sequence,
            galaxy: None,
        }),
    };
    window.settle_open(&mut state, &mut ctx).expect("opens");
    while window.outcome().is_none() {
        window.drive(&mut state, &mut ctx).expect("drives");
        if window.outcome().is_some() {
            break;
        }
        let _ = window.take_scoring_occurrence();
        window.settle_open(&mut state, &mut ctx).expect("settles");
    }
    let guard = seen.lock().unwrap();
    guard.clone()
}

// -- Xxcha: Nullification Field ----------------------------------------------------------------

#[test]
fn nf_1_ends_the_activating_players_turn_for_a_token_and_exhausts() {
    let mut state = seated(&[("a", "sol"), ("b", "xxcha")]);
    let system = SystemId::new("19");
    crate::fixtures::put(&mut state, &system, "cruiser", &pid("b"), 1);
    give(&mut state, "b", "nf");
    state.player_mut(&pid("b")).unwrap().strategic_tokens = 2;
    let mut table = Table::with_default(Box::new(crate::choice::FirstOption));
    let used = crate::faction_techs::offer_nullification_field(
        &mut state,
        content(),
        POK,
        &mut table,
        None,
        &system,
        &pid("a"),
    );
    assert_eq!(used, Some(pid("b")), "the holder ends a's turn");
    let seat = state.player(&pid("b")).unwrap();
    assert_eq!(seat.strategic_tokens, 1);
    assert!(
        seat.exhausted_technologies
            .contains(&TechnologyId::new("nf"))
    );

    // Without the technology nothing is offered.
    let mut bare = seated(&[("a", "sol"), ("b", "xxcha")]);
    crate::fixtures::put(&mut bare, &system, "cruiser", &pid("b"), 1);
    bare.player_mut(&pid("b"))
        .unwrap()
        .technologies
        .remove(&TechnologyId::new("nf"));
    let mut table = Table::with_default(Box::new(crate::choice::FirstOption));
    assert_eq!(
        crate::faction_techs::offer_nullification_field(
            &mut bare,
            content(),
            POK,
            &mut table,
            None,
            &system,
            &pid("a"),
        ),
        None
    );
}

// -- Yin: Yin Spinner Omega, Impulse Core ------------------------------------------------------

fn produced(player: &str) -> Vec<(&'static str, serde_json::Value)> {
    vec![
        ("player", player.into()),
        ("system", "03".into()),
        ("source", "production".into()),
        ("count", 1.into()),
    ]
}

#[test]
fn yso_1_places_two_infantry_after_producing() {
    let mut state = seated(&[("a", "yin"), ("b", "sol")]);
    give(&mut state, "a", "yso");
    let before = on_planet(&state, "03", "darien", "infantry");
    emit(
        &mut state,
        &["technology:yin:yso:UNITS_PRODUCED:after", "03|darien"],
        "UNITS_PRODUCED",
        &produced("a"),
    );
    assert_eq!(on_planet(&state, "03", "darien", "infantry"), before + 2);

    // Without the technology, or when another player produced, nothing is placed.
    let mut bare = seated(&[("a", "yin"), ("b", "sol")]);
    bare.player_mut(&pid("a"))
        .unwrap()
        .technologies
        .remove(&TechnologyId::new("yso"));
    emit(&mut bare, &[], "UNITS_PRODUCED", &produced("a"));
    assert_eq!(on_planet(&bare, "03", "darien", "infantry"), before);
    emit(&mut state, &[], "UNITS_PRODUCED", &produced("b"));
    assert_eq!(on_planet(&state, "03", "darien", "infantry"), before + 2);
}

#[test]
fn ic_1_sacrifices_a_destroyer_at_combat_start_for_a_hit() {
    let mut state = seated(&[("a", "yin"), ("b", "sol")]);
    let system = SystemId::new("18");
    give(&mut state, "a", "ic");
    crate::fixtures::put(&mut state, &system, "destroyer", &pid("a"), 1);
    crate::fixtures::put(&mut state, &system, "dreadnought", &pid("b"), 1);
    let destroyed = fought(state, &system, &["destroyer"]);
    assert_eq!(
        destroyed.first(),
        Some(&("a".to_owned(), "destroyer".to_owned())),
        "the sacrifice comes before any die: {destroyed:?}"
    );

    // Without the technology the destroyer is not offered up (the script has no answer to spend).
    let mut bare = seated(&[("a", "yin"), ("b", "sol")]);
    crate::fixtures::put(&mut bare, &system, "destroyer", &pid("a"), 1);
    crate::fixtures::put(&mut bare, &system, "dreadnought", &pid("b"), 4);
    let plain = fought(bare, &system, &[]);
    assert!(plain.iter().all(|(player, _)| player == "a"), "{plain:?}");
}

// -- Yssaril: Transparasteel Plating -----------------------------------------------------------

#[test]
fn tp_1_passed_players_cannot_play_action_cards_on_the_owners_turn() {
    let mut state = seated(&[("a", "yssaril"), ("b", "sol")]);
    give(&mut state, "a", "tp");
    state.phase = ti4_model::state::Phase::Action;
    state.active = Some(pid("a"));
    state.player_mut(&pid("b")).unwrap().passed = true;
    assert!(crate::laws::action_cards_forbidden(&state, &pid("b")));
    assert!(!crate::laws::action_cards_forbidden(&state, &pid("a")));
    // Not the owner's turn: allowed.
    state.active = Some(pid("b"));
    assert!(!crate::laws::action_cards_forbidden(&state, &pid("b")));
    // Without the technology: allowed.
    state.active = Some(pid("a"));
    state
        .player_mut(&pid("a"))
        .unwrap()
        .technologies
        .remove(&TechnologyId::new("tp"));
    assert!(!crate::laws::action_cards_forbidden(&state, &pid("b")));
}

// -- Keleres: I.I.H.Q. Modernization, Agency Supply Network, Executive Order -------------------

#[test]
fn iihq_1_makes_neighbours_of_everyone_near_mecatol() {
    let hub = crate::fixtures::hub_with_centre("18");
    let mut state = seated(&[("a", "keleresm"), ("b", "sol")]);
    for sys in state.board.keys().cloned().collect::<Vec<_>>() {
        let board = state.system_mut(&sys);
        board.units.clear();
        board.planet_units.clear();
        board.planet_control.clear();
    }
    let (a, b) = (pid("a"), pid("b"));
    crate::fixtures::put(
        &mut state,
        &SystemId::new(hub.outer[0].as_str()),
        "carrier",
        &a,
        1,
    );
    crate::fixtures::put(
        &mut state,
        &SystemId::new(hub.outer[3].as_str()),
        "carrier",
        &b,
        1,
    );
    assert!(
        !crate::transactions::are_neighbours(&state, &hub.galaxy, &a, &b),
        "no card"
    );
    give(&mut state, "a", "iihq");
    assert!(crate::transactions::are_neighbours(
        &state,
        &hub.galaxy,
        &a,
        &b
    ));
    assert!(
        crate::transactions::are_neighbours(&state, &hub.galaxy, &b, &a),
        "mutual"
    );
    // A player with nothing near Mecatol is not made a neighbour.
    state
        .system_mut(&SystemId::new(hub.outer[3].as_str()))
        .units
        .clear();
    let far = crate::fixtures::plain_systems(20).pop().unwrap();
    crate::fixtures::put(&mut state, &SystemId::new(far), "carrier", &b, 1);
    assert!(!crate::transactions::are_neighbours(
        &state,
        &hub.galaxy,
        &a,
        &b
    ));
}

#[test]
fn iihq_2_gains_custodia_vigilia_and_cannot_lose_it() {
    let mut state = seated(&[("a", "keleresm"), ("b", "sol")]);
    crate::factions::keleres::reconcile(&mut state);
    assert!(
        !crate::factions::keleres::holds_custodia(&state, &pid("a")),
        "no card, no planet"
    );
    give(&mut state, "a", "iihq");
    crate::factions::keleres::reconcile(&mut state);
    assert!(crate::factions::keleres::holds_custodia(&state, &pid("a")));
    // Someone else taking control of it is undone.
    let home = state
        .player(&pid("a"))
        .unwrap()
        .home_system
        .clone()
        .unwrap();
    state
        .system_mut(&home)
        .set_control(PlanetId::new("custodiavigilia"), pid("b"));
    crate::factions::keleres::reconcile(&mut state);
    assert!(
        crate::factions::keleres::holds_custodia(&state, &pid("a")),
        "cannot be lost"
    );
}

#[test]
fn asn_1_resolves_a_second_production_in_any_system_once_per_action() {
    use crate::factions::keleres::asn_ready;
    let mut state = seated(&[("a", "keleresm"), ("b", "sol")]);
    give(&mut state, "a", "asn");
    state.player_mut(&pid("a")).unwrap().trade_goods = 20;
    let home = state
        .player(&pid("a"))
        .unwrap()
        .home_system
        .clone()
        .unwrap();
    let (other, planet) = crate::fixtures::a_placed_planet();
    state
        .system_mut(&other)
        .set_control(planet.clone(), pid("a"));
    crate::fixtures::put_on_planet(&mut state, &other, &planet, "spacedock", &pid("a"), 1);
    let ships = |s: &GameState, sys: &SystemId| {
        s.system_state(sys)
            .units
            .iter()
            .filter(|u| u.owner == pid("a") && u.type_id.as_str() == "destroyer")
            .count()
    };
    let home_before = ships(&state, &home);
    let target = other.to_string();
    let mut built = false;
    let mut table = Table::default();
    table.seat(
        pid("a"),
        Box::new(Pick(move |choice: &crate::choice::Choice| {
            if choice.prompt.starts_with("Agency Supply Network") {
                return choice.option(&target).cloned();
            }
            if !built
                && let Some(o) = choice
                    .options
                    .iter()
                    .find(|o| o.id.starts_with("build|destroyer"))
            {
                built = true;
                return Some(o.clone());
            }
            if let Some(o) = choice.option("trade_good") {
                return Some(o.clone());
            }
            choice.options.iter().find(|o| o.is_decline()).cloned()
        })),
    );
    let report = crate::production::resolve(
        &mut state,
        content(),
        DEFAULT,
        None,
        &mut table,
        &pid("a"),
        &home,
    );
    assert!(report.is_ok(), "{report:?}");
    assert_eq!(
        ships(&state, &other),
        1,
        "built where ASN pointed, not at the first dock"
    );
    assert_eq!(ships(&state, &home), home_before);
    assert!(!asn_ready(&state, &pid("a")), "once per action");
}

struct Pick<F>(F);
impl<F: FnMut(&crate::choice::Choice) -> Option<ChoiceOption>> crate::choice::Decider for Pick<F> {
    fn choose(
        &mut self,
        choice: &crate::choice::Choice,
    ) -> Result<ChoiceOption, crate::choice::IllegalChoice> {
        (self.0)(choice)
            .or_else(|| choice.options.first().cloned())
            .ok_or_else(|| crate::choice::IllegalChoice::NoOptions {
                player: choice.player.clone(),
                prompt: choice.prompt.clone(),
            })
    }
}

#[test]
fn executiveorder_1_draws_the_top_or_bottom_agenda_and_exhausts() {
    let mut state = seated(&[("a", "keleresm"), ("b", "sol")]);
    state.phase = ti4_model::state::Phase::Action;
    state.active = Some(pid("a"));
    state.agenda_deck = vec!["sanctions".to_owned(), "mutiny".to_owned()];
    assert!(offered(&state, "a").is_empty(), "no technology, no action");
    give(&mut state, "a", "executiveorder");
    let options = offered(&state, "a");
    assert!(
        options.iter().any(|o| o.ends_with("executiveorder|bottom")),
        "{options:?}"
    );
    let bottom = options
        .iter()
        .find(|o| o.ends_with("|bottom"))
        .unwrap()
        .clone();
    assert!(perform(&mut state, "a", &bottom, &[]));
    assert_eq!(
        state.agenda_deck,
        ["sanctions"],
        "the bottom card was drawn"
    );
    assert!(
        state
            .player(&pid("a"))
            .unwrap()
            .exhausted_technologies
            .contains(&TechnologyId::new("executiveorder"))
    );
    assert_eq!(
        crate::factions::keleres::take_executive_order(&mut state),
        Some((pid("a"), "mutiny".to_owned())),
        "the drawn agenda is handed to the vote"
    );
    assert!(offered(&state, "a").is_empty(), "exhausted");
}

// -- Nekro: ???_NULL_REFERENCE_???, ???_ERROR_ERROR_??? ----------------------------------------
// Both are implemented in nekro_units.rs but deliberately unclaimed (operator ruling 2026-10-07:
// an obscure variant), so they are tested at the hook, not through a faction-technology list.

fn ship_destroyed(state: &mut GameState, unit: &str) {
    emit(
        state,
        &[],
        "SHIP_DESTROYED",
        &[
            ("system", "18".into()),
            ("player", "a".into()),
            ("unit", unit.into()),
            ("last", false.into()),
            ("cause", "space_combat".into()),
            ("during_space_combat", true.into()),
        ],
    );
}

fn cruisers_at_home(state: &GameState) -> usize {
    let home = state
        .player(&pid("a"))
        .unwrap()
        .home_system
        .clone()
        .unwrap();
    state
        .system_state(&home)
        .units
        .iter()
        .filter(|u| u.owner == pid("a") && u.type_id.as_str() == "cruiser")
        .count()
}

#[test]
fn nekroc4y_1_produces_the_destroyed_ship_type_at_a_home_space_dock() {
    let mut state = seated(&[("a", "nekro"), ("b", "sol")]);
    give(&mut state, "a", "nekroc4y");
    state.player_mut(&pid("a")).unwrap().trade_goods = 10;
    let before = cruisers_at_home(&state);
    ship_destroyed(&mut state, "cruiser");
    assert_eq!(cruisers_at_home(&state), before + 1);

    // Without the card nothing is produced.
    let mut bare = seated(&[("a", "nekro"), ("b", "sol")]);
    bare.player_mut(&pid("a")).unwrap().trade_goods = 10;
    ship_destroyed(&mut bare, "cruiser");
    assert_eq!(cruisers_at_home(&bare), before);
}

fn with_c4r(state: &mut GameState) {
    give(state, "a", "nekroc4r");
}

fn damage_all(state: &mut GameState, who: &str) {
    for board in state.board.values_mut() {
        for unit in board
            .units
            .iter_mut()
            .chain(board.planet_units.values_mut().flatten())
            .filter(|u| u.owner == pid(who))
        {
            unit.sustained_damage = true;
        }
    }
}

fn pds_of(state: &GameState, who: &str) -> usize {
    state
        .board
        .values()
        .flat_map(|board| board.planet_units.values().flatten())
        .filter(|u| u.owner == pid(who) && u.type_id.as_str() == "pds")
        .count()
}

#[test]
fn nekroc4r_1_places_a_pds_on_a_controlled_planet_and_exhausts() {
    let mut state = seated(&[("a", "nekro"), ("b", "sol")]);
    assert!(
        !offered(&state, "a").iter().any(|o| o.contains("c4r")),
        "no card"
    );
    with_c4r(&mut state);
    let before = pds_of(&state, "a");
    assert!(perform(&mut state, "a", "faction|nekro|c4r|pds", &[]));
    assert_eq!(pds_of(&state, "a"), before + 1);
    assert!(
        state
            .player(&pid("a"))
            .unwrap()
            .exhausted_technologies
            .contains(&TechnologyId::new("nekroc4r"))
    );
    assert!(
        !perform(&mut state, "a", "faction|nekro|c4r|pds", &[]),
        "exhausted"
    );
}

#[test]
fn nekroc4r_2_repairs_every_damaged_unit_of_the_owner_only() {
    let mut state = seated(&[("a", "nekro"), ("b", "sol")]);
    with_c4r(&mut state);
    damage_all(&mut state, "a");
    damage_all(&mut state, "b");
    assert!(offered(&state, "a").contains(&"faction|nekro|c4r|repair".to_owned()));
    assert!(perform(&mut state, "a", "faction|nekro|c4r|repair", &[]));
    let damaged = |who: &str| {
        state
            .board
            .values()
            .flat_map(|b| b.units.iter().chain(b.planet_units.values().flatten()))
            .filter(|u| u.owner == pid(who) && u.sustained_damage)
            .count()
    };
    assert_eq!(damaged("a"), 0);
    assert!(damaged("b") > 0, "another player's damage stays");
}

#[test]
fn nekroc4r_3_discards_an_action_card_to_draw_one() {
    let mut state = seated(&[("a", "nekro"), ("b", "sol")]);
    with_c4r(&mut state);
    state.player_mut(&pid("a")).unwrap().action_cards =
        vec![ti4_model::id::ActionCardId::new("sabo1")];
    let deck = state.action_card_deck.len();
    assert!(perform(&mut state, "a", "faction|nekro|c4r|draw", &[]));
    let seat = state.player(&pid("a")).unwrap();
    assert_eq!(seat.action_cards.len(), 1, "discarded 1, drew 1");
    assert_ne!(seat.action_cards[0].as_str(), "sabo1");
    assert_eq!(state.action_card_deck.len(), deck - 1);
}

// -- Last Bastion: Proxima Targeting VI, 4X41C "Helios" V2 ---------------------------------------

fn bastion_arena(
    owner_of_planet: &str,
    tech_holder: Option<&str>,
    seats: &[(&str, &str)],
) -> (GameState, SystemId, PlanetId) {
    let mut state = seated(seats);
    let (system, planet) = crate::fixtures::a_placed_planet();
    let board = state.system_mut(&system);
    board.units.clear();
    board.planet_units.clear();
    board.planet_control.clear();
    board
        .planet_control
        .insert(planet.clone(), pid(owner_of_planet));
    if let Some(holder) = tech_holder {
        give(&mut state, holder, "proxima");
    }
    (state, system, planet)
}

fn bombard_three_dreadnoughts(state: &mut GameState, system: &SystemId) {
    let mut dice = crate::dice::Dice::from_faces([10, 10, 10]);
    let mut rng = crate::rng::GameRng::new(0);
    crate::invasion::bombardment(
        state,
        content(),
        DEFAULT,
        &mut dice,
        &mut rng,
        &mut scripted(&[]),
        system,
        &pid("a"),
    )
    .expect("bombards");
}

#[test]
fn proxima_1_cancels_one_bombardment_hit_per_galvanized_unit_present() {
    let seats = [("a", "sol"), ("b", "bastion")];
    let setup = |tech: bool| {
        let (mut state, system, planet) = bastion_arena("b", tech.then_some("b"), &seats);
        crate::fixtures::put(&mut state, &system, "dreadnought", &pid("a"), 3);
        crate::fixtures::put_on_planet(&mut state, &system, &planet, "infantry", &pid("b"), 4);
        state
            .system_mut(&system)
            .planet_units
            .entry(planet.clone())
            .or_default()
            .push(
                ti4_model::units::Unit::new(ti4_model::id::UnitTypeId::new("infantry"), pid("b"))
                    .galvanized(),
            );
        (state, system, planet)
    };
    let (mut state, system, planet) = setup(true);
    bombard_three_dreadnoughts(&mut state, &system);
    assert_eq!(
        state
            .system_state(&system)
            .on_planet_of(&planet, &pid("b"))
            .len(),
        5 - 2,
        "3 hits, 1 cancelled"
    );
    let (mut bare, system, planet) = setup(false);
    bombard_three_dreadnoughts(&mut bare, &system);
    assert_eq!(
        bare.system_state(&system)
            .on_planet_of(&planet, &pid("b"))
            .len(),
        5 - 3,
        "no Proxima: all 3 land"
    );
}

/// Takes the named option when offered, else declines, else the first option.
struct Prefer(&'static str);
impl crate::choice::Decider for Prefer {
    fn choose(
        &mut self,
        choice: &crate::choice::Choice,
    ) -> Result<ChoiceOption, crate::choice::IllegalChoice> {
        choice
            .option(self.0)
            .or_else(|| choice.options.iter().find(|o| o.is_decline()))
            .or_else(|| choice.options.first())
            .cloned()
            .ok_or_else(|| crate::choice::IllegalChoice::NoOptions {
                player: choice.player.clone(),
                prompt: choice.prompt.clone(),
            })
    }
}

/// The live invasion over `system` armed as the game arms it; returns the dice for their rolls.
fn invade(
    state: &mut GameState,
    system: &SystemId,
    prefer: &'static str,
    faces: &[u32],
) -> crate::dice::Dice {
    use crate::choice::{Resolving, TimingHandle, Window};
    let mut resolver = crate::fixtures::armed_resolver(state);
    let mut sequence = crate::event::EventSequence::new();
    let mut table = Table::with_default(Box::new(Prefer(prefer)));
    let mut dice =
        crate::dice::Dice::from_faces(faces.iter().copied().chain(std::iter::repeat_n(1, 60)));
    let mut rng = crate::rng::GameRng::new(3);
    let mut window = crate::invasion::InvasionWindow::at_commit_step(state, &pid("a"), system);
    let mut ctx = Resolving {
        content: content(),
        sources: DEFAULT,
        dice: &mut dice,
        rng: &mut rng,
        table: &mut table,
        timing: Some(TimingHandle {
            resolver: &mut resolver,
            sequence: &mut sequence,
            galaxy: None,
        }),
    };
    window.settle(state, &mut ctx);
    for _ in 0..30 {
        let Some(choice) = window.pending_choice(state, content(), DEFAULT) else {
            break;
        };
        let option = choice.options[0].clone();
        window.resolve(state, &mut ctx, option).expect("resolves");
        window.settle(state, &mut ctx);
    }
    drop(ctx);
    dice
}

#[test]
fn proxima_2_bombards_at_ground_combat_round_start_and_rolls_back() {
    const PROXIMA_ID: &str = "technology:bastion:proxima:GROUND_COMBAT_ROUND_BEGAN:after";
    let setup = |tech: bool| {
        let (mut state, system, planet) =
            bastion_arena("b", tech.then_some("a"), &[("a", "bastion"), ("b", "sol")]);
        crate::fixtures::put(&mut state, &system, "infantry", &pid("a"), 3);
        crate::fixtures::put_on_planet(&mut state, &system, &planet, "infantry", &pid("b"), 2);
        (state, system, planet)
    };
    let (mut state, system, planet) = setup(true);
    let dice = invade(&mut state, &system, PROXIMA_ID, &[8, 8, 1, 8, 1, 1]);
    let rolls: Vec<_> = dice
        .history()
        .iter()
        .filter(|r| r.reason == "proxima targeting")
        .collect();
    assert_eq!(
        rolls.len(),
        2,
        "BOMBARDMENT 8 (x3) and the roll back, each three dice"
    );
    assert_eq!(rolls[0].faces.len(), 3);
    assert_eq!(rolls[1].faces.len(), 3);
    assert!(
        state
            .system_state(&system)
            .on_planet_of(&planet, &pid("b"))
            .is_empty(),
        "both defenders fell"
    );

    let (mut bare, system, _) = setup(false);
    let dice = invade(&mut bare, &system, PROXIMA_ID, &[10; 30]);
    assert!(
        dice.history()
            .iter()
            .all(|r| r.reason != "proxima targeting"),
        "no technology, no bombardment"
    );
}

fn helios_planet() -> (GameState, SystemId, PlanetId) {
    let mut state = seated(&[("a", "bastion"), ("b", "sol")]);
    let (system, planet) = crate::fixtures::a_placed_planet();
    let board = state.system_mut(&system);
    board.units.clear();
    board.planet_units.clear();
    board.planet_control.clear();
    state
        .system_mut(&system)
        .set_control(planet.clone(), pid("a"));
    crate::fixtures::put_on_planet(
        &mut state,
        &system,
        &planet,
        "bastion_spacedock",
        &pid("a"),
        1,
    );
    (state, system, planet)
}

fn resources_now(state: &GameState, planet: &PlanetId) -> i64 {
    crate::production::planet_value_now(
        state,
        content(),
        DEFAULT,
        planet,
        crate::production::Spend::Resources,
    )
}

fn upgrade_helios(state: &mut GameState, system: &SystemId, planet: &PlanetId) {
    give(state, "a", "helios2");
    crate::technology::apply_unit_upgrades(state, content(), DEFAULT, &pid("a"));
    assert!(
        state
            .system_state(system)
            .on_planet(planet)
            .iter()
            .any(|u| u.type_id.as_str() == "bastion_spacedock2"),
        "the dock is upgraded in place"
    );
}

#[test]
fn helios2_1_production_is_four_more_than_the_planets_resources() {
    let (mut state, system, planet) = helios_planet();
    let v1 = crate::production::capacity(&state, content(), DEFAULT, &pid("a"), &system);
    assert_eq!(
        v1,
        resources_now(&state, &planet) + 2,
        "V1: 2 more than the resource value"
    );
    upgrade_helios(&mut state, &system, &planet);
    assert_eq!(
        crate::production::capacity(&state, content(), DEFAULT, &pid("a"), &system),
        resources_now(&state, &planet) + 4
    );
}

#[test]
fn helios2_2_raises_the_planets_resource_value_by_two() {
    let (mut state, system, planet) = helios_planet();
    let printed = crate::production::planet_value(
        content(),
        DEFAULT,
        &planet,
        crate::production::Spend::Resources,
    );
    assert_eq!(resources_now(&state, &planet), printed + 1, "V1 adds 1");
    upgrade_helios(&mut state, &system, &planet);
    assert_eq!(resources_now(&state, &planet), printed + 2);
    assert_eq!(
        crate::factions::bastion_units::resource_bonus(&state, &PlanetId::new("mellon")),
        0
    );
}

#[test]
fn helios2_3_three_fighters_in_the_system_do_not_count_against_capacity() {
    let (mut state, system, planet) = helios_planet();
    let excess = |s: &GameState| {
        crate::fleet::standing(s, content(), DEFAULT, &pid("a"), &system, None).capacity_excess
    };
    crate::fixtures::put(&mut state, &system, "fighter", &pid("a"), 3);
    assert_eq!(excess(&state), 0, "3 fighters are supported");
    crate::fixtures::put(&mut state, &system, "fighter", &pid("a"), 1);
    assert_eq!(excess(&state), 1, "the fourth is not");
    upgrade_helios(&mut state, &system, &planet);
    assert_eq!(excess(&state), 1, "V2 still frees exactly three");
}

// -- Deepwrought: Hydrothermal Mining, Radical Advancement ---------------------------------------

/// Make `count` oceans: `a` (Deepwrought) coexisting on invented planets that `b` controls.
fn make_oceans(state: &mut GameState, count: usize) {
    for n in 0..count {
        let (system, planet) = (
            SystemId::new(format!("9{n}")),
            PlanetId::new(format!("px{n}")),
        );
        state
            .system_mut(&system)
            .set_control(planet.clone(), pid("b"));
        crate::fixtures::put_on_planet(state, &system, &planet, "infantry", &pid("b"), 1);
        crate::fixtures::put_on_planet(state, &system, &planet, "infantry", &pid("a"), 1);
        assert!(
            crate::factions::deepwrought::begin_coexisting(
                state,
                &pid("a"),
                &system,
                &planet,
                None
            )
            .expect("begins")
        );
    }
}

fn goods(state: &GameState, who: &str) -> i32 {
    state.player(&pid(who)).map_or(0, |seat| seat.trade_goods)
}

#[test]
fn hydrothermal_1_gains_a_trade_good_per_ocean_in_play_at_status_phase_start() {
    let mut state = seated(&[("a", "deepwrought"), ("b", "sol"), ("c", "hacan")]);
    give(&mut state, "a", "hydrothermal");
    make_oceans(&mut state, 3);
    let before = goods(&state, "a");
    emit(&mut state, &[], "STATUS_PHASE_BEGAN", &[]);
    assert_eq!(goods(&state, "a"), before + 3);

    // Without the technology the oceans pay nothing; with no oceans neither does the card.
    let mut bare = seated(&[("a", "deepwrought"), ("b", "sol"), ("c", "hacan")]);
    make_oceans(&mut bare, 3);
    let before = goods(&bare, "a");
    emit(&mut bare, &[], "STATUS_PHASE_BEGAN", &[]);
    assert_eq!(goods(&bare, "a"), before);
}

#[test]
fn radical_1_replaces_a_technology_with_one_with_exactly_one_more_prerequisite() {
    const RADICAL_ID: &str = "technology:deepwrought:radical:STATUS_PHASE_BEGAN:after";
    let mut state = seated(&[("a", "deepwrought"), ("b", "sol"), ("c", "hacan")]);
    state.player_mut(&pid("a")).unwrap().technologies.clear();
    give(&mut state, "a", "radical");
    give(&mut state, "a", "nm");
    emit(
        &mut state,
        &[RADICAL_ID, "nm|dxa"],
        "STATUS_PHASE_BEGAN",
        &[],
    );
    assert!(
        crate::technology::has_technology_text(&state, &pid("a"), "dxa"),
        "dxa (one more green) replaced nm"
    );
    assert!(
        !crate::technology::has_technology_text(&state, &pid("a"), "nm"),
        "the old card went back"
    );
    assert!(crate::technology::has_technology_text(
        &state,
        &pid("a"),
        "radical"
    ));

    // Two more prerequisites is not allowed: hm is never on offer for nm.
    let mut state = seated(&[("a", "deepwrought"), ("b", "sol"), ("c", "hacan")]);
    state.player_mut(&pid("a")).unwrap().technologies.clear();
    give(&mut state, "a", "radical");
    give(&mut state, "a", "nm");
    let ids: Vec<String> =
        crate::factions::deepwrought::replacements(&state, content(), DEFAULT, &pid("a"))
            .iter()
            .map(|r| format!("{}|{}", r.old, r.new))
            .collect();
    assert!(ids.contains(&"nm|dxa".to_owned()), "{ids:?}");
    assert!(!ids.contains(&"nm|hm".to_owned()), "{ids:?}");
}

// -- unit upgrade helper -------------------------------------------------------------------------

/// `(cost, combat hits-on, move, capacity, anti-fighter barrage hits-on, afb dice)` of a unit card.
fn card(unit: &str) -> (f64, Option<i64>, i64, i64, Option<i64>, i64) {
    let types = ti4_content::units::catalogue(content(), DEFAULT);
    let t = types
        .get(unit)
        .unwrap_or_else(|| panic!("no unit card {unit}"));
    (
        t.cost(),
        t.combat_hits_on(),
        t.move_value(),
        t.capacity(),
        t.afb_hits_on(),
        t.afb_dice(),
    )
}

/// Owning `tech` turns every `old` unit of seat `a` (faction `faction`) into `new`, and not before.
fn upgrade_replaces(faction: &str, tech: &str, old: &str, new: &str) {
    let mut state = seated(&[("a", faction), ("b", "sol")]);
    let count = |s: &GameState, kind: &str| -> usize {
        s.board
            .values()
            .flat_map(|b| b.units.iter().chain(b.planet_units.values().flatten()))
            .filter(|u| u.owner == pid("a") && u.type_id.as_str() == kind)
            .count()
    };
    let before = count(&state, old);
    assert!(before > 0, "the starting force carries {old}");
    crate::technology::apply_unit_upgrades(&mut state, content(), DEFAULT, &pid("a"));
    assert_eq!(
        count(&state, old),
        before,
        "without the technology nothing changes"
    );
    give(&mut state, "a", tech);
    crate::technology::apply_unit_upgrades(&mut state, content(), DEFAULT, &pid("a"));
    assert_eq!(
        (count(&state, old), count(&state, new)),
        (0, before),
        "the new card replaces the old"
    );
    assert!(
        crate::production::buildable_for(&state, content(), DEFAULT, &pid("a"))
            .contains(&new.to_owned()),
        "and new ones are built upgraded"
    );
}

// -- Crimson Rebellion: Subatomic Splicer, Exile II ----------------------------------------------

fn crimson_ship_destroyed(state: &mut GameState, owner: &str, unit: &str, answers: &[&str]) {
    emit(
        state,
        answers,
        "SHIP_DESTROYED",
        &[
            ("system", "18".into()),
            ("player", owner.into()),
            ("unit", unit.into()),
            ("last", false.into()),
            ("cause", "space_combat".into()),
            ("during_space_combat", true.into()),
        ],
    );
}

fn at_home(state: &GameState, who: &str, unit: &str) -> usize {
    let home = state
        .player(&pid(who))
        .unwrap()
        .home_system
        .clone()
        .unwrap();
    state
        .system_state(&home)
        .units
        .iter()
        .filter(|u| u.owner == pid(who) && u.type_id.as_str() == unit)
        .count()
}

#[test]
fn subatomic_1_produces_the_destroyed_ship_type_at_a_home_space_dock() {
    const SPLICER: &str = "technology:crimson:subatomic:SHIP_DESTROYED:after";
    let mut state = seated(&[("a", "crimson"), ("b", "sol"), ("c", "hacan")]);
    give(&mut state, "a", "subatomic");
    state.player_mut(&pid("a")).unwrap().trade_goods = 12;
    let before = at_home(&state, "a", "crimson_destroyer");
    crimson_ship_destroyed(&mut state, "a", "crimson_destroyer", &[SPLICER]);
    assert_eq!(at_home(&state, "a", "crimson_destroyer"), before + 1);
    // Another player's ship, and a ground force, are not "one of your ships".
    let snapshot = state.clone();
    crimson_ship_destroyed(&mut state, "b", "destroyer", &[]);
    crimson_ship_destroyed(&mut state, "a", "infantry", &[]);
    assert_eq!(state, snapshot);
    // Without the card nothing is produced.
    let mut bare = seated(&[("a", "crimson"), ("b", "sol"), ("c", "hacan")]);
    bare.player_mut(&pid("a")).unwrap().trade_goods = 12;
    let snapshot = bare.clone();
    crimson_ship_destroyed(&mut bare, "a", "crimson_destroyer", &[]);
    assert_eq!(bare, snapshot);
}

#[test]
fn exile2_1_unit_card_stats_and_replaces_exile_i() {
    assert_eq!(card("crimson_destroyer2"), (1.0, Some(7), 2, 0, Some(6), 3));
    upgrade_replaces(
        "crimson",
        "exile2",
        "crimson_destroyer",
        "crimson_destroyer2",
    );
}

#[test]
fn exile2_2_places_an_active_or_inactive_breach_up_to_two_systems_from_a_combat() {
    const EXILE: &str = "unit:crimson:crimson_destroyer:SPACE_COMBAT_ENDED:after";
    let ids = crate::factions::crimson::testkit::plain(7);
    let ring: Vec<&str> = ids[..6].iter().map(String::as_str).collect();
    let mut galaxy = ti4_content::galaxy::Galaxy::build(
        content(),
        &[
            &ids[6], ring[0], ring[1], ring[2], ring[3], ring[4], ring[5],
        ],
        DEFAULT,
        1,
    )
    .expect("map");
    crate::seating::place_crimson_home(&mut galaxy, content(), DEFAULT).expect("home");
    let (fought, far) = (
        SystemId::new(ids[0].as_str()),
        SystemId::new(ids[2].as_str()),
    );
    let combat = vec![("system", serde_json::Value::from(fought.to_string()))];
    let run = |kind: &str, tech_unit: &str, answers: &[&str]| {
        let mut state = seated(&[("a", "crimson"), ("b", "sol"), ("c", "hacan")]);
        crate::fixtures::put(&mut state, &far, tech_unit, &pid("a"), 1);
        let mut table = scripted(answers);
        let mut resolver = crate::fixtures::armed_resolver(&state);
        crate::fixtures::with_context(&mut state, DEFAULT, Some(&galaxy), &mut table, |ctx| {
            let payload = combat
                .iter()
                .map(|(k, v)| ((*k).to_owned(), v.clone()))
                .collect();
            let event = ctx.event_sequence.next(kind, payload).expect("event");
            resolver
                .emit_with_context(ctx, event, |_, _| {})
                .expect("emits");
        });
        state
    };
    // Exile II reaches two systems and may choose active.
    let active = run(
        "SPACE_COMBAT_ENDED",
        "crimson_destroyer2",
        &[EXILE, "active"],
    );
    assert!(crate::factions::crimson::is_active(&active, &fought));
    let inactive = run(
        "SPACE_COMBAT_ENDED",
        "crimson_destroyer2",
        &[EXILE, "inactive"],
    );
    assert!(
        crate::factions::crimson::has_breach(&inactive, &fought)
            && !crate::factions::crimson::is_active(&inactive, &fought)
    );
    // Exile I two systems away is out of reach.
    let none = run("SPACE_COMBAT_ENDED", "crimson_destroyer", &[EXILE]);
    assert!(!crate::factions::crimson::has_breach(&none, &fought));
}

// -- Ral Nel Consortium: Nanomachines, Linkship II -----------------------------------------------

fn ralnel() -> GameState {
    let mut state = seated(&[("a", "ralnel"), ("b", "sol"), ("c", "hacan")]);
    give(&mut state, "a", "nanomachines");
    state
}

fn nano_exhausted(state: &GameState) -> bool {
    !offered(state, "a").iter().any(|o| o.contains("nano_"))
}

#[test]
fn nanomachines_1_places_a_pds_on_a_controlled_planet_and_exhausts() {
    let mut state = ralnel();
    let before = pds_of(&state, "a");
    assert!(perform(&mut state, "a", "faction|ralnel|nano_pds", &[]));
    assert_eq!(pds_of(&state, "a"), before + 1);
    assert!(
        nano_exhausted(&state),
        "one exhaustion covers all three actions"
    );
    assert!(!perform(&mut state, "a", "faction|ralnel|nano_pds", &[]));
    // Without the technology nothing is offered.
    let bare = seated(&[("a", "ralnel"), ("b", "sol")]);
    assert!(nano_exhausted(&bare));
}

#[test]
fn nanomachines_2_repairs_all_of_your_damaged_units() {
    let mut state = ralnel();
    damage_all(&mut state, "a");
    damage_all(&mut state, "b");
    assert!(offered(&state, "a").contains(&"faction|ralnel|nano_repair".to_owned()));
    assert!(perform(&mut state, "a", "faction|ralnel|nano_repair", &[]));
    let damaged = |who: &str| {
        state
            .board
            .values()
            .flat_map(|b| b.units.iter().chain(b.planet_units.values().flatten()))
            .filter(|u| u.owner == pid(who) && u.sustained_damage)
            .count()
    };
    assert_eq!(damaged("a"), 0);
    assert!(damaged("b") > 0, "only your own units");
    assert!(nano_exhausted(&state));
    // The status phase readies the card.
    emit(&mut state, &[], "STATUS_PHASE_ENDED", &[]);
    assert!(!nano_exhausted(&state));
}

#[test]
fn nanomachines_3_discards_an_action_card_to_draw_one() {
    let mut state = ralnel();
    state.player_mut(&pid("a")).unwrap().action_cards =
        vec![ti4_model::id::ActionCardId::new("sabo1")];
    let deck = state.action_card_deck.len();
    assert!(perform(
        &mut state,
        "a",
        "faction|ralnel|nano_draw",
        &["sabo1"]
    ));
    let seat = state.player(&pid("a")).unwrap();
    assert_eq!(seat.action_cards.len(), 1);
    assert_ne!(seat.action_cards[0].as_str(), "sabo1");
    assert_eq!(state.action_card_deck.len(), deck - 1);
    assert!(nano_exhausted(&state));
}

#[test]
fn linkship2_1_unit_card_stats_and_replaces_linkship_i() {
    assert_eq!(card("ralnel_destroyer2"), (1.0, Some(8), 4, 0, Some(6), 3));
    upgrade_replaces(
        "ralnel",
        "linkship2",
        "ralnel_destroyer",
        "ralnel_destroyer2",
    );
}

fn cannon_dice(state: &mut GameState) -> usize {
    let mut dice = crate::dice::Dice::new();
    let mut rng = crate::rng::GameRng::new(7);
    let _ = crate::combat::space_cannon_offense(
        state,
        content(),
        DEFAULT,
        &mut dice,
        &mut rng,
        &SystemId::new("18"),
        &pid("b"),
        None,
    );
    dice.count()
}

#[test]
fn linkship2_2_each_linkship_ii_uses_the_same_structures_space_cannon() {
    let setup = |kind: &str| {
        let mut state = seated(&[("a", "ralnel"), ("b", "sol")]);
        let sys = SystemId::new("18");
        crate::fixtures::put(&mut state, &sys, "cruiser", &pid("b"), 1);
        crate::fixtures::put(&mut state, &sys, "pds", &pid("a"), 1);
        crate::fixtures::put(&mut state, &sys, kind, &pid("a"), 2);
        state
    };
    assert_eq!(
        cannon_dice(&mut setup("ralnel_destroyer2")),
        2,
        "each Linkship II fires the structure"
    );
    assert_eq!(
        cannon_dice(&mut setup("ralnel_destroyer")),
        1,
        "Linkship I: each structure once"
    );
}

// -- Firmament / Obsidian: Planesplitter, Neural Parasite ----------------------------------------

use crate::factions::firmament::testkit as fk;

fn firmament_game() -> GameState {
    seated(&[("a", "firmament"), ("b", "sol"), ("c", "hacan")])
}

fn firmament_infantry(state: &GameState, planet: &str) -> usize {
    let home = state
        .player(&pid("a"))
        .unwrap()
        .home_system
        .clone()
        .unwrap();
    state
        .system_state(&home)
        .on_planet_of(&PlanetId::new(planet), &pid("a"))
        .iter()
        .filter(|u| u.type_id.as_str().contains("infantry"))
        .count()
}

#[test]
fn planesplitter_firm_1_puts_the_fracture_into_play_when_gained() {
    let gained = [
        ("player", serde_json::Value::from("a")),
        ("technology", "planesplitter-firm".into()),
    ];
    let mut state = firmament_game();
    give(&mut state, "a", "planesplitter-firm");
    assert!(!state.fracture_in_play);
    emit(&mut state, &[], "TECHNOLOGY_GAINED", &gained);
    assert!(state.fracture_in_play);
    assert!(!state.ingress_tokens.is_empty());
    // Another card gained does not do it.
    let mut other = firmament_game();
    emit(
        &mut other,
        &[],
        "TECHNOLOGY_GAINED",
        &[("player", "a".into()), ("technology", "ac2".into())],
    );
    assert!(!other.fracture_in_play);
}

/// The Firmament (`a`) with a plot in play, a map holding its home tile, and `techs` owned.
fn flip_ready(techs: &[&str]) -> (GameState, ti4_content::galaxy::Galaxy) {
    let ids = fk::plain(6);
    let mut ring_ids: Vec<&str> = ids[1..].iter().map(String::as_str).collect();
    ring_ids[0] = "96a";
    let galaxy = fk::ring(&ids[0], &ring_ids);
    let mut state = firmament_game();
    fk::place_plot(&mut state, &pid("a"), &pid("b"));
    for tech in techs {
        give(&mut state, "a", tech);
    }
    (state, galaxy)
}

fn become_obsidian(state: &mut GameState, galaxy: &ti4_content::galaxy::Galaxy) -> bool {
    let Some(option) =
        crate::factions::mapped_component_actions(state, content(), DEFAULT, galaxy, &pid("a"))
            .into_iter()
            .find(|o| o.id == "faction|firmament|become_obsidian")
    else {
        return false;
    };
    let mut table = scripted(&[]);
    crate::fixtures::with_context(state, DEFAULT, Some(galaxy), &mut table, |ctx| {
        crate::factions::perform_component(ctx, &pid("a"), &option)
    })
}

#[test]
fn planesplitter_firm_2_flips_to_the_obsidian_card_when_the_faction_flips() {
    let (mut state, galaxy) = flip_ready(&["planesplitter-firm", "parasite-firm"]);
    assert!(become_obsidian(&mut state, &galaxy));
    assert!(crate::technology::has_technology_text(
        &state,
        &pid("a"),
        "planesplitter-obs"
    ));
    assert!(!crate::technology::has_technology_text(
        &state,
        &pid("a"),
        "planesplitter-firm"
    ));
}

#[test]
fn parasite_firm_1_places_an_infantry_on_a_home_planet_at_status_phase_start() {
    let mut state = firmament_game();
    give(&mut state, "a", "parasite-firm");
    let before = firmament_infantry(&state, "tallin");
    emit(&mut state, &["tallin"], "STATUS_PHASE_BEGAN", &[]);
    assert_eq!(firmament_infantry(&state, "tallin"), before + 1);
    // A may; and nothing without the card.
    let mut declined = firmament_game();
    give(&mut declined, "a", "parasite-firm");
    emit(&mut declined, &["decline"], "STATUS_PHASE_BEGAN", &[]);
    assert_eq!(firmament_infantry(&declined, "tallin"), before);
    let mut bare = firmament_game();
    emit(&mut bare, &["tallin"], "STATUS_PHASE_BEGAN", &[]);
    assert_eq!(firmament_infantry(&bare, "tallin"), before);
}

#[test]
fn parasite_firm_2_flips_to_the_obsidian_card_when_the_faction_flips() {
    let (mut state, galaxy) = flip_ready(&["planesplitter-firm", "parasite-firm"]);
    assert!(become_obsidian(&mut state, &galaxy));
    assert!(crate::technology::has_technology_text(
        &state,
        &pid("a"),
        "parasite-obs"
    ));
    assert!(!crate::technology::has_technology_text(
        &state,
        &pid("a"),
        "parasite-firm"
    ));
}

/// Seven plain systems and a ring map for the Obsidian tests.
fn obsidian_map() -> (Vec<String>, ti4_content::galaxy::Galaxy) {
    let ids = fk::plain(7);
    let galaxy = fk::ring(
        &ids[0],
        &ids[1..].iter().map(String::as_str).collect::<Vec<_>>(),
    );
    (ids, galaxy)
}

fn planet_in(system: &str) -> PlanetId {
    PlanetId::new(
        *ti4_content::galaxy::system(content(), system, DEFAULT)
            .unwrap()
            .planets()
            .first()
            .expect("a planet"),
    )
}

fn obsidian_game() -> GameState {
    let mut state = seated(&[("a", "obsidian"), ("b", "sol"), ("c", "hacan")]);
    crate::promissory::deal(&mut state, content(), DEFAULT);
    state
}

#[test]
fn planesplitter_obs_1_moves_an_ingress_token_next_to_the_owners_units() {
    const WINDOW: &str = "technology:obsidian:planesplitter-obs:STRATEGIC_ACTION_BEGAN:when";
    let (ids, galaxy) = obsidian_map();
    let mut state = obsidian_game();
    give(&mut state, "a", "planesplitter-obs");
    state.fracture_in_play = true;
    state.ingress_tokens.insert(SystemId::new(ids[4].as_str()));
    crate::fixtures::put(
        &mut state,
        &SystemId::new(ids[1].as_str()),
        "cruiser",
        &pid("a"),
        1,
    );
    let mut bare = state.clone();
    bare.player_mut(&pid("a")).unwrap().technologies.clear();
    let (from, to) = (ids[4].as_str(), ids[2].as_str());
    let pick = format!("{from}>{to}");
    let strategic = [("player", serde_json::Value::from("a"))];
    let run = |state: &mut GameState, answers: &[&str]| {
        let mut resolver = crate::fixtures::armed_resolver(state);
        let mut table = scripted(answers);
        crate::fixtures::with_context(state, DEFAULT, Some(&galaxy), &mut table, |ctx| {
            let payload = strategic
                .iter()
                .map(|(k, v)| ((*k).to_owned(), v.clone()))
                .collect();
            let event = ctx
                .event_sequence
                .next("STRATEGIC_ACTION_BEGAN", payload)
                .expect("event");
            resolver
                .emit_with_context(ctx, event, |_, _| {})
                .expect("emits");
        });
    };
    run(&mut state, &[WINDOW, pick.as_str()]);
    assert_eq!(
        state.ingress_tokens,
        std::collections::BTreeSet::from([SystemId::new(to)]),
        "moved, not copied"
    );
    run(&mut bare, &[]);
    assert!(
        bare.ingress_tokens.contains(&SystemId::new(from)),
        "no card, no move"
    );
}

#[test]
fn planesplitter_obs_2_cannot_be_researched() {
    let state = obsidian_game();
    let open = crate::technology::researchable(&state, content(), DEFAULT, &pid("a"));
    assert!(!open.contains(&TechnologyId::new("planesplitter-obs")));
    assert!(!open.contains(&TechnologyId::new("parasite-obs")));
    assert!(!crate::technology::can_research(
        &state,
        content(),
        DEFAULT,
        &pid("a"),
        &TechnologyId::new("planesplitter-obs")
    ));
}

#[test]
fn parasite_obs_1_destroys_an_adjacent_players_infantry_at_the_start_of_the_owners_turn() {
    let (ids, galaxy) = obsidian_map();
    let setup = |tech: bool| {
        let mut state = obsidian_game();
        if tech {
            give(&mut state, "a", "parasite-obs");
        }
        let mine = SystemId::new(ids[1].as_str());
        crate::fixtures::put_on_planet(
            &mut state,
            &mine,
            &planet_in(&ids[1]),
            "infantry",
            &pid("a"),
            1,
        );
        let next = SystemId::new(ids[2].as_str());
        crate::fixtures::put_on_planet(
            &mut state,
            &next,
            &planet_in(&ids[2]),
            "infantry",
            &pid("b"),
            2,
        );
        state
    };
    let infantry_of = |state: &GameState, system: &str, who: &str| {
        let board = state.system_state(&SystemId::new(system));
        board
            .planet_units
            .values()
            .flatten()
            .filter(|u| u.owner == pid(who) && u.type_id.as_str().contains("infantry"))
            .count()
    };
    let turn = |state: &mut GameState, player: &str| {
        let mut resolver = crate::fixtures::armed_resolver(state);
        let mut table = scripted(&[]);
        crate::fixtures::with_context(state, DEFAULT, Some(&galaxy), &mut table, |ctx| {
            let payload =
                std::iter::once(("player".to_owned(), serde_json::Value::from(player))).collect();
            let event = ctx
                .event_sequence
                .next("TURN_BEGAN", payload)
                .expect("event");
            resolver
                .emit_with_context(ctx, event, |_, _| {})
                .expect("emits");
        });
    };
    let mut state = setup(true);
    turn(&mut state, "a");
    assert_eq!(
        infantry_of(&state, &ids[2], "b"),
        1,
        "exactly one destroyed"
    );
    assert_eq!(
        infantry_of(&state, &ids[1], "a"),
        1,
        "its own are never targets"
    );
    let mut bare = setup(false);
    turn(&mut bare, "a");
    assert_eq!(infantry_of(&bare, &ids[2], "b"), 2, "no card, no effect");
    let mut other_turn = setup(true);
    turn(&mut other_turn, "b");
    assert_eq!(
        infantry_of(&other_turn, &ids[2], "b"),
        2,
        "only at the owner's turn"
    );
}

#[test]
fn parasite_obs_2_cannot_be_researched() {
    let state = obsidian_game();
    assert!(!crate::technology::can_research(
        &state,
        content(),
        DEFAULT,
        &pid("a"),
        &TechnologyId::new("parasite-obs")
    ));
}
