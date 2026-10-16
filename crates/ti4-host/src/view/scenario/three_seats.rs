//! The scenarios of one question on a game of three seats: Sol (seat A), Hacan (seat B) and
//! Letnev (seat C), in the action phase of round 1 with Sol to act, on the seeded random board.
//! Each finds its systems by the places of the seats, so it plays on the seed of the table.
//!
//! Most of them have an opening: the answers of Sol that take the game to the question of the
//! scenario (a research, a production, the first question of a battle or of an invasion).

use ti4_content::ContentStore;
use ti4_content::galaxy::Galaxy;
use ti4_engine::choice::Choice;
use ti4_engine::phase;
use ti4_model::content_types::POK;
use ti4_model::id::{
    ActionCardId, ObjectiveId, PlanetId, PlayerId, StrategyCardId, SystemId, TechnologyId,
    UnitTypeId,
};
use ti4_model::state::{GameState, Phase};
use ti4_model::units::Unit;

use super::{Opening, OpeningAnswer, Scenario, ScenarioMap};

pub const RESEARCH_ID: &str = "research_tech_skips";
pub const TACTICAL_ID: &str = "tactical_action";
pub const PRODUCTION_ID: &str = "production_batch";
pub const PRODUCTION_PAYMENT_ID: &str = "production_payment_batch";
pub const PRODUCTION_AUTOSPEND_ID: &str = "production_payment_autospend";
pub const SPACE_COMBAT_ID: &str = "space_combat";
pub const ONGOING_COMBAT_ID: &str = "ongoing_combat";
pub const ONGOING_COMBAT_FOUR_VIEWS_ID: &str = "ongoing_combat_four_views";
pub const INVASION_FOUR_VIEWS_ID: &str = "ongoing_invasion_four_views";
pub const INVASION_COEXISTENCE_ID: &str = "ongoing_invasion_coexistence";
pub const INVASION_PARLEY_ID: &str = "ongoing_invasion_parley";
pub const ENDGAME_ID: &str = "endgame";
pub const SCORE_STATUS_ID: &str = "score_objective_status";
pub const SCORE_IMPERIAL_ID: &str = "score_objective_imperial";
pub const FULL_GAME_ID: &str = "full_game";
pub const STRATEGY_IMPERIAL_MECATOL_ID: &str = "strategy_imperial_mecatol";

/// The scenarios of one strategy card each: the id of the scenario and the card that Sol plays.
pub const STRATEGY_CARDS: &[(&str, &str)] = &[
    ("strategy_leadership", "pok1leadership"),
    ("strategy_diplomacy", "pok2diplomacy"),
    ("strategy_diplomacy_base", "base2"),
    ("strategy_politics", "pok3politics"),
    ("strategy_construction", "pok4construction"),
    ("strategy_construction_base", "base4"),
    ("strategy_construction_te", "te4construction"),
    ("strategy_trade", "pok5trade"),
    ("strategy_warfare", "pok6warfare"),
    ("strategy_warfare_te", "te6warfare"),
    ("strategy_imperial", "pok8imperial"),
];

/// Sol is played by a person, Hacan and Letnev by the rule bot.
const SOL: &[usize] = &[0];
/// Every seat is played by a person.
const ALL: &[usize] = &[0, 1, 2];

const BASE: Scenario = Scenario {
    id: TACTICAL_ID,
    title: "Tactical Action & Movement",
    description: "Player 1 (Federation of Sol) is active in Round 1 Action Phase with full tactic \
        pool and unexhausted fleet at Jord (Home #01). Activate an adjacent sector, move Carrier, \
        Cruiser, Fighters, and Infantry, and resolve planetary exploration or landing.",
    category: "Tactical",
    seed: 42,
    players: 3,
    humans: SOL,
    any_seed: true,
    map: ScenarioMap::Random,
    opening: None,
};

pub const RESEARCH: Scenario = Scenario {
    id: RESEARCH_ID,
    title: "Technology Research & Skips",
    description: "Player 1 (Federation of Sol) has played the Technology strategy card. Choose \
        technologies to research, toggle ready tech specialty planets for skips, and pay for a \
        second technology.",
    category: "Strategy",
    opening: Some(research_opening),
    ..BASE
};

pub const TACTICAL: Scenario = BASE;

pub const PRODUCTION: Scenario = Scenario {
    id: PRODUCTION_ID,
    title: "Production Batch",
    description: "Sol is ready to produce at Jord with a free production use and another \
        controlled planet.",
    opening: Some(production_opening),
    ..BASE
};

pub const PRODUCTION_PAYMENT: Scenario = Scenario {
    id: PRODUCTION_PAYMENT_ID,
    title: "Production Payment Batch",
    description: "Sol is ready to produce at Jord and can pay using multiple controlled planets.",
    ..PRODUCTION
};

pub const PRODUCTION_AUTOSPEND: Scenario = Scenario {
    id: PRODUCTION_AUTOSPEND_ID,
    title: "Production Payment Auto-spend",
    description: "Sol can pay with Jord and Wren Terra; the last payment option is spent \
        automatically.",
    ..PRODUCTION
};

pub const SPACE_COMBAT: Scenario = Scenario {
    id: SPACE_COMBAT_ID,
    title: "Space Combat Encounter",
    description: "Hostile Barony of Letnev ships (Cruiser, Destroyer, 2 Fighters) are positioned \
        in the adjacent border system. Activate the contested sector and move your fleet in to \
        trigger the full space combat flow: anti-fighter barrage, combat rolls, sustain damage, \
        hit assignment, and retreat choices against the bot.",
    category: "Combat",
    ..BASE
};

pub const ONGOING_COMBAT: Scenario = Scenario {
    id: ONGOING_COMBAT_ID,
    title: "Ongoing Space Combat",
    description: "Space combat is already in progress between your Sol fleet and Letnev's armada \
        in a contested sector. Play combat action cards such as Direct Hit and Shields Holding \
        while resolving dice rolls, sustain damage, and casualties without setting up movement \
        first.",
    opening: Some(battle_opening),
    ..SPACE_COMBAT
};

pub const ONGOING_COMBAT_FOUR_VIEWS: Scenario = Scenario {
    id: ONGOING_COMBAT_FOUR_VIEWS_ID,
    title: "Four-view Space Combat",
    description: "Human-controlled Sol, Letnev and Hacan; Sol holds combat cards and Letnev holds \
        Sabotage.",
    humans: ALL,
    opening: Some(four_view_battle_opening),
    ..SPACE_COMBAT
};

pub const INVASION_FOUR_VIEWS: Scenario = Scenario {
    id: INVASION_FOUR_VIEWS_ID,
    title: "Four-view Invasion",
    description: "Three human seats and a spectator, starting before the invasion reaction on a \
        single contested planet.",
    category: "Combat",
    humans: ALL,
    opening: Some(invasion_opening),
    ..BASE
};

pub const INVASION_COEXISTENCE: Scenario = Scenario {
    id: INVASION_COEXISTENCE_ID,
    title: "Multi-planet Invasion",
    description: "Two contested planets with coexisting rival ground forces.",
    humans: SOL,
    ..INVASION_FOUR_VIEWS
};

pub const INVASION_PARLEY: Scenario = Scenario {
    id: INVASION_PARLEY_ID,
    title: "Interrupted Invasion Landing",
    description: "Single-planet invasion with a human defender holding Parley to interrupt \
        sequential landings.",
    ..INVASION_FOUR_VIEWS
};

pub const ENDGAME: Scenario = Scenario {
    id: ENDGAME_ID,
    title: "Endgame to Victory",
    description: "Three seats near completion: the seats pass final action turns, score status \
        objectives, and reach Game Over.",
    category: "Endgame",
    humans: ALL,
    ..BASE
};

pub const SCORE_STATUS: Scenario = Scenario {
    id: SCORE_STATUS_ID,
    title: "Score Objective (Status Phase)",
    description: "Player 1 (Federation of Sol) is in the Status Phase with multiple scoreable \
        public objectives (Lead From the Front and Negotiate Trade Routes) that have not been \
        scored yet. Choose which public objective to score.",
    category: "Status",
    ..BASE
};

pub const SCORE_IMPERIAL: Scenario = Scenario {
    id: SCORE_IMPERIAL_ID,
    title: "Score Objective (Imperial)",
    description: "Player 1 (Federation of Sol) plays the Imperial strategy card primary ability, \
        allowing them to score a public objective immediately during the Action Phase. Choose \
        which public objective to score.",
    category: "Strategy",
    opening: Some(imperial_opening),
    ..BASE
};

const STRATEGY_DESCRIPTION: &str = "Sol has the turn and holds the strategy card of the \
    title. Sol has 4 trade goods, controls a planet next to its home with its command token on \
    that system, and a second system next to its home whose planets are exhausted. Hacan has 3 \
    trade goods. Hacan and Letnev hold other cards; with Sol and Hacan played here, Hacan \
    resolves the secondary.";

const STRATEGY: Scenario = Scenario {
    category: "Strategy",
    description: STRATEGY_DESCRIPTION,
    ..BASE
};

pub const STRATEGY_LEADERSHIP: Scenario = Scenario {
    id: "strategy_leadership",
    title: "Strategy card: Leadership",
    ..STRATEGY
};

pub const STRATEGY_DIPLOMACY: Scenario = Scenario {
    id: "strategy_diplomacy",
    title: "Strategy card: Diplomacy",
    description: "Sol has the turn and holds Diplomacy. Sol controls Jord, a planet next to its \
        home with its command token on that system, and a second system next to its home; Jord \
        and the planets of the second system are exhausted. The planets of Hacan's home are \
        exhausted; with Sol and Hacan played here, Hacan resolves the secondary.",
    ..STRATEGY
};

pub const STRATEGY_DIPLOMACY_BASE: Scenario = Scenario {
    id: "strategy_diplomacy_base",
    title: "Strategy card: Diplomacy (base game)",
    description: "The scenario \"Strategy card: Diplomacy\" with the Diplomacy of the base game \
        (base2). The engine resolves it as the Codex I card: it readies up to 2 exhausted planets \
        anywhere, not each exhausted planet in the chosen system.",
    ..STRATEGY
};

pub const STRATEGY_POLITICS: Scenario = Scenario {
    id: "strategy_politics",
    title: "Strategy card: Politics",
    ..STRATEGY
};

const CONSTRUCTION_DESCRIPTION: &str = "Sol has the turn and holds Construction. Sol \
    controls its home with a space dock, a planet next to it with a PDS and Sol's command token \
    on that system, and a second system next to its home. With Sol and Hacan played here, Hacan \
    resolves the secondary.";

pub const STRATEGY_CONSTRUCTION: Scenario = Scenario {
    id: "strategy_construction",
    title: "Strategy card: Construction",
    description: CONSTRUCTION_DESCRIPTION,
    ..STRATEGY
};

pub const STRATEGY_CONSTRUCTION_BASE: Scenario = Scenario {
    id: "strategy_construction_base",
    title: "Strategy card: Construction (base game)",
    description: "The scenario \"Strategy card: Construction\" on a table of the base game with \
        Codex I, whose Construction is base4.",
    ..STRATEGY
};

pub const STRATEGY_CONSTRUCTION_TE: Scenario = Scenario {
    id: "strategy_construction_te",
    title: "Strategy card: Construction (Thunder's Edge)",
    description: "The scenario \"Strategy card: Construction\" on a table of Thunder's Edge, \
        whose Construction places a structure or uses the PRODUCTION of a space dock, then places \
        a structure.",
    ..STRATEGY
};

pub const STRATEGY_TRADE: Scenario = Scenario {
    id: "strategy_trade",
    title: "Strategy card: Trade",
    description: "Sol has the turn and holds Trade. Sol has 4 trade goods and 1 commodity; Hacan \
        and Letnev have no commodities, so both can be chosen to replenish. With Sol and Hacan \
        played here, Hacan resolves the secondary.",
    ..STRATEGY
};

pub const STRATEGY_WARFARE: Scenario = Scenario {
    id: "strategy_warfare",
    title: "Strategy card: Warfare",
    description: "Sol has the turn and holds Warfare, with its command token on a system next to \
        its home to recall. With Sol and Hacan played here, Hacan resolves the secondary: a \
        production at the space dock of its home.",
    ..STRATEGY
};

pub const STRATEGY_WARFARE_TE: Scenario = Scenario {
    id: "strategy_warfare_te",
    title: "Strategy card: Warfare (Thunder's Edge)",
    description: "The scenario \"Strategy card: Warfare\" on a table of Thunder's Edge, whose \
        Warfare is a tactical action in any system without a command token, also in the system \
        next to Sol's home that has its token, with a redistribution before and after.",
    ..STRATEGY
};

pub const STRATEGY_IMPERIAL: Scenario = Scenario {
    id: "strategy_imperial",
    title: "Strategy card: Imperial",
    description: "Sol has the turn and holds Imperial. Lead From the Front, Negotiate Trade \
        Routes and Corner the Market are revealed; Sol can score the first two. Sol does not \
        control Mecatol Rex, so Imperial draws a secret objective. With Sol and Hacan played \
        here, Hacan resolves the secondary.",
    ..STRATEGY
};

pub const STRATEGY_IMPERIAL_MECATOL: Scenario = Scenario {
    id: STRATEGY_IMPERIAL_MECATOL_ID,
    title: "Strategy card: Imperial (Mecatol Rex)",
    description: "The scenario \"Strategy card: Imperial\" where Sol controls Mecatol Rex, with \
        its infantry there and the custodians token taken: Imperial gains a victory point in place \
        of the secret objective.",
    ..STRATEGY
};

pub const FULL_GAME: Scenario = Scenario {
    id: FULL_GAME_ID,
    title: "Full Game Match",
    description: "Fresh 3-player match from Round 1 setup through to Game Over, with people \
        in all seats.",
    category: "Match",
    humans: ALL,
    ..BASE
};

/// Turns a new game of three seats into the scenario `id`.
pub(super) fn apply(
    id: &str,
    state: &mut GameState,
    galaxy: &Galaxy,
    content: &ContentStore,
) -> Result<(), String> {
    let [sol, hacan, letnev] = base(state)?;
    match id {
        RESEARCH_ID => research(state, galaxy, content, &sol),
        TACTICAL_ID => {
            tactical(state, galaxy, content, &sol, &hacan);
            Ok(())
        }
        PRODUCTION_ID | PRODUCTION_PAYMENT_ID | PRODUCTION_AUTOSPEND_ID => {
            production(state, galaxy, content, &sol, id)
        }
        SPACE_COMBAT_ID | ONGOING_COMBAT_ID => {
            space_combat(state, galaxy, content, &sol, &letnev);
            Ok(())
        }
        ONGOING_COMBAT_FOUR_VIEWS_ID => {
            space_combat(state, galaxy, content, &sol, &letnev);
            four_view_battle(state, &sol, &letnev);
            Ok(())
        }
        INVASION_FOUR_VIEWS_ID | INVASION_COEXISTENCE_ID | INVASION_PARLEY_ID => {
            invasion(state, galaxy, content, [&sol, &hacan, &letnev], id)
        }
        ENDGAME_ID => {
            endgame(state, [&sol, &hacan, &letnev]);
            Ok(())
        }
        SCORE_STATUS_ID => {
            score_status(state, &sol);
            Ok(())
        }
        SCORE_IMPERIAL_ID => {
            score_imperial(state, &sol);
            Ok(())
        }
        FULL_GAME_ID => Ok(()),
        STRATEGY_IMPERIAL_MECATOL_ID => {
            strategy_card(
                state,
                galaxy,
                content,
                [&sol, &hacan, &letnev],
                "pok8imperial",
            )?;
            hold_mecatol(state, galaxy, content, &sol)
        }
        other => match STRATEGY_CARDS.iter().find(|(id, _)| *id == other) {
            Some((_, card)) => strategy_card(state, galaxy, content, [&sol, &hacan, &letnev], card),
            None => Err(format!("no scenario '{other}' of three seats")),
        },
    }
}

/// Sol holds `card` and has the turn, with what each card of the strategy scenarios needs: trade
/// goods and ready planets to buy command tokens, a command token on the board to recall, a
/// controlled system for Diplomacy and exhausted planets to ready, planets for structures.
fn strategy_card(
    state: &mut GameState,
    galaxy: &Galaxy,
    content: &ContentStore,
    [sol, hacan, letnev]: [&PlayerId; 3],
    card: &str,
) -> Result<(), String> {
    let name = |id: &str| ti4_engine::strategy_cards::card_name(content, id);
    let others: Vec<StrategyCardId> = ["pok3politics", "pok8imperial", "pok1leadership"]
        .into_iter()
        .filter(|other| name(other) != name(card))
        .take(2)
        .map(StrategyCardId::new)
        .collect();
    for (seat, held) in [
        (sol, StrategyCardId::new(card)),
        (hacan, others[0].clone()),
        (letnev, others[1].clone()),
    ] {
        state
            .player_mut(seat)
            .ok_or("a seat is missing")?
            .strategy_cards = vec![held];
    }
    let dealt = [card, others[0].as_str(), others[1].as_str()];
    // A card of another set is played on a table of that set.
    if !state
        .unclaimed_strategy_cards
        .iter()
        .any(|unclaimed| unclaimed.as_str() == card)
    {
        let set = ["te", "base_game_codex1", "base_game"]
            .into_iter()
            .filter_map(|set| ti4_engine::setup::strategy_card_setup_for_set(content, set).ok())
            .map(|(cards, _)| cards)
            .find(|cards| cards.iter().any(|held| held.as_str() == card))
            .ok_or_else(|| format!("no set of strategy cards has {card}"))?;
        state.unclaimed_strategy_cards = set;
    }
    state
        .unclaimed_strategy_cards
        .retain(|unclaimed| !dealt.contains(&unclaimed.as_str()));
    state
        .player_mut(sol)
        .ok_or("Sol is not seated")?
        .trade_goods = 4;
    // Enough for Hacan to buy a command token in the secondary of Leadership.
    state
        .player_mut(hacan)
        .ok_or("Hacan is not seated")?
        .trade_goods = 3;

    // Two systems next to Sol's home with planets: the first is Sol's with its command token,
    // the planets of the second are Sol's and exhausted.
    let with_planets: Vec<&str> = galaxy
        .adjacent("01")
        .into_iter()
        .filter(|id| {
            ti4_content::galaxy::system(content, id, POK)
                .is_some_and(|tile| !tile.planets().is_empty())
        })
        .take(2)
        .collect();
    let [first, second] = with_planets[..] else {
        return Err("the strategy scenarios need two systems with planets next to Sol".to_owned());
    };
    for (system, exhausted) in [(first, false), (second, true)] {
        let planets = ti4_content::galaxy::system(content, system, POK)
            .map(|tile| tile.planets())
            .unwrap_or_default();
        let board = state.system_mut(&SystemId::new(system));
        board.units.clear();
        board.command_tokens.clear();
        for planet in &planets {
            board.planet_units.remove(&PlanetId::new(*planet));
            board.land(
                &PlanetId::new(*planet),
                &[Unit::new(UnitTypeId::new("infantry"), sol.clone())],
            );
            board.set_control(PlanetId::new(*planet), sol.clone());
        }
        if !exhausted {
            board.command_tokens.insert(sol.clone());
        }
        if exhausted {
            for planet in planets {
                state.exhaust_planet(PlanetId::new(planet));
            }
        }
    }
    if name(card).as_deref() == Some("Construction") {
        // A PDS on the first planet of the first system: a second PDS there fills it.
        let planet = ti4_content::galaxy::system(content, first, POK)
            .and_then(|tile| tile.planets().first().map(|planet| PlanetId::new(*planet)))
            .ok_or("the first system has no planet")?;
        state
            .system_mut(&SystemId::new(first))
            .planet_units
            .entry(planet)
            .or_default()
            .push(Unit::new(UnitTypeId::new("pds"), sol.clone()));
    }
    if name(card).as_deref() == Some("Imperial") {
        // Two public objectives that Sol fulfils, and one that it does not.
        revealed_objectives(state, sol);
    }
    if name(card).as_deref() == Some("Trade") {
        // Below its limit, so that the replenish of Sol shows a change.
        state
            .player_mut(sol)
            .ok_or("Sol is not seated")?
            .commodities = 1;
    }
    if name(card).as_deref() == Some("Diplomacy") {
        // More exhausted planets than Diplomacy readies, for Sol and for Hacan's secondary.
        for seat in [sol, hacan] {
            let home = state
                .player(seat)
                .and_then(|player| player.home_system.clone())
                .ok_or("a seat has no home")?;
            let planets = ti4_content::galaxy::system(content, home.as_str(), POK)
                .map(|tile| tile.planets())
                .unwrap_or_default();
            for planet in planets {
                state.exhaust_planet(PlanetId::new(planet));
            }
        }
    }
    Ok(())
}

/// The system that the scenario `id` prepared on `galaxy` for its opening: the border system of a
/// battle, the system of an invasion.
pub(super) fn prepared_system(id: &str, galaxy: &Galaxy, content: &ContentStore) -> Option<String> {
    match id {
        SPACE_COMBAT_ID | ONGOING_COMBAT_ID | ONGOING_COMBAT_FOUR_VIEWS_ID => {
            Some(border_system(galaxy, content).to_owned())
        }
        INVASION_FOUR_VIEWS_ID | INVASION_COEXISTENCE_ID | INVASION_PARLEY_ID => {
            invasion_system(galaxy, content, id == INVASION_COEXISTENCE_ID)
                .ok()
                .map(str::to_owned)
        }
        _ => None,
    }
}

/// The game of every scenario of three seats: Sol has Leadership, Hacan Trade and Letnev
/// Warfare, and Sol has the first turn of the action phase.
fn base(state: &mut GameState) -> Result<[PlayerId; 3], String> {
    let [sol, hacan, letnev] = match state.players.as_slice() {
        [a, b, c] => [a.id.clone(), b.id.clone(), c.id.clone()],
        seats => return Err(format!("a game of three seats has {}", seats.len())),
    };
    let cards = [
        (&sol, StrategyCardId::new("leadership")),
        (&hacan, StrategyCardId::new("trade")),
        (&letnev, StrategyCardId::new("warfare")),
    ];
    for (seat, card) in &cards {
        state
            .player_mut(seat)
            .ok_or("a seat is missing")?
            .strategy_cards = vec![card.clone()];
    }
    state
        .unclaimed_strategy_cards
        .retain(|card| cards.iter().all(|(_, dealt)| dealt != card));
    phase::advance_phase(state);
    phase::begin_action_turn(state, &sol);
    let seat = state.player_mut(&sol).ok_or("Sol is not seated")?;
    seat.tactic_tokens = 3;
    seat.fleet_tokens = 3;
    seat.strategic_tokens = 2;
    Ok([sol, hacan, letnev])
}

/// Whether a ship can end its move in the system `id` (no supernova, no asteroid field).
fn passable(content: &ContentStore, id: &str) -> bool {
    ti4_content::galaxy::system(content, id, POK)
        .is_some_and(|tile| !tile.is_supernova() && !tile.is_asteroid_field())
}

/// The first system next to Sol's home that ships can enter.
fn border_system<'a>(galaxy: &'a Galaxy, content: &ContentStore) -> &'a str {
    galaxy
        .adjacent("01")
        .into_iter()
        .find(|id| passable(content, id))
        .unwrap_or("18")
}

fn research(
    state: &mut GameState,
    galaxy: &Galaxy,
    content: &ContentStore,
    sol: &PlayerId,
) -> Result<(), String> {
    let technology = StrategyCardId::new("pok7technology");
    if let Some(seat) = state.player_mut(sol) {
        seat.strategy_cards = vec![technology.clone()];
        seat.trade_goods = 6;
        seat.technologies = [TechnologyId::new("amd"), TechnologyId::new("nm")]
            .into_iter()
            .collect();
    }
    state
        .unclaimed_strategy_cards
        .retain(|card| *card != technology);
    state
        .unclaimed_strategy_cards
        .push(StrategyCardId::new("leadership"));

    // One ready Biotic skip (Sol's Spec Ops II needs a second green prerequisite) and one
    // exhausted skip of any colour, whichever planets this map carries.
    let all_planets = ti4_content::galaxy::all_planets(content, POK);
    let mut skips = Vec::new();
    for system in galaxy.system_ids() {
        if system == "01" {
            continue;
        }
        let Some(tile) = ti4_content::galaxy::system(content, system, POK) else {
            continue;
        };
        for planet in tile.planets() {
            if let Some(record) = all_planets.get(planet) {
                let specialties = record.tech_specialties();
                if !specialties.is_empty() {
                    let biotic = specialties
                        .iter()
                        .any(|specialty| specialty.eq_ignore_ascii_case("biotic"));
                    skips.push((system.to_owned(), PlanetId::new(planet), biotic));
                }
            }
        }
    }
    let ready = skips
        .iter()
        .position(|(_, _, biotic)| *biotic)
        .ok_or("this map has no Biotic technology specialty")?;
    let (system, planet, _) = skips.remove(ready);
    state
        .system_mut(&SystemId::new(&system))
        .set_control(planet, sol.clone());
    if let Some((system, planet, _)) = skips.into_iter().next() {
        state
            .system_mut(&SystemId::new(&system))
            .set_control(planet.clone(), sol.clone());
        state.exhaust_planet(planet);
    }
    Ok(())
}

fn tactical(
    state: &mut GameState,
    galaxy: &Galaxy,
    content: &ContentStore,
    sol: &PlayerId,
    hacan: &PlayerId,
) {
    // A system next to Sol's home for Hacan's fleet, and a second one next to it for Sol's.
    let border = border_system(galaxy, content);
    let second = galaxy
        .adjacent(border)
        .into_iter()
        .find(|id| *id != "01" && *id != border && passable(content, id))
        .unwrap_or("19");

    let board = state.system_mut(&SystemId::new(border));
    board.command_tokens.clear();
    board.units.clear();
    for unit in ["cruiser", "destroyer", "fighter", "fighter"] {
        board
            .units
            .push(Unit::new(UnitTypeId::new(unit), hacan.clone()));
    }
    if let Some(planet) = board.planet_units.keys().next().cloned() {
        board.land(
            &planet,
            &[Unit::new(UnitTypeId::new("infantry"), hacan.clone())],
        );
        board.set_control(planet, hacan.clone());
    }

    let board = state.system_mut(&SystemId::new(second));
    board.command_tokens.clear();
    board.units.clear();
    for unit in ["cruiser", "dreadnought", "fighter"] {
        board
            .units
            .push(Unit::new(UnitTypeId::new(unit), sol.clone()));
    }
    let infantry = vec![
        Unit::new(UnitTypeId::new("infantry"), sol.clone()),
        Unit::new(UnitTypeId::new("infantry"), sol.clone()),
    ];
    if let Some(planet) = board.planet_units.keys().next().cloned() {
        board.land(&planet, &infantry);
        board.set_control(planet, sol.clone());
    } else {
        board.units.extend(infantry);
    }
}

fn production(
    state: &mut GameState,
    galaxy: &Galaxy,
    content: &ContentStore,
    sol: &PlayerId,
    id: &str,
) -> Result<(), String> {
    let autospend = id == PRODUCTION_AUTOSPEND_ID;
    let planets: Vec<_> = galaxy
        .system_ids()
        .into_iter()
        .filter(|system| *system != "01")
        .filter_map(|system| {
            ti4_content::galaxy::system(content, system, POK)?
                .planets()
                .into_iter()
                .next()
                .map(|planet| (system, planet))
        })
        .take(if autospend { 0 } else { 2 })
        .collect();
    if !autospend && planets.len() != 2 {
        return Err("the production scenario needs two more planets".to_owned());
    }
    for (system, planet) in planets {
        state
            .system_mut(&SystemId::new(system))
            .set_control(PlanetId::new(planet), sol.clone());
    }
    if autospend {
        if !galaxy.system_ids().contains(&"10") {
            return Err("the production payment scenario needs Wren Terra's system".to_owned());
        }
        state
            .system_mut(&SystemId::new("10"))
            .set_control(PlanetId::new("wrenterra"), sol.clone());
    }
    if id == PRODUCTION_ID {
        let sequence = state.production_seq;
        state
            .player_mut(sol)
            .ok_or("Sol is not seated")?
            .free_production_use = Some(sequence + 1);
    }
    Ok(())
}

fn space_combat(
    state: &mut GameState,
    galaxy: &Galaxy,
    content: &ContentStore,
    sol: &PlayerId,
    letnev: &PlayerId,
) {
    // Cards for the different timing windows of a battle, and the fleet tokens for it.
    if let Some(seat) = state.player_mut(sol) {
        seat.fleet_tokens = 4;
        seat.action_cards.extend(
            ["dh1", "sh1", "courageous", "salvage"]
                .into_iter()
                .map(ActionCardId::new),
        );
    }
    if let Some(seat) = state.player_mut(letnev) {
        seat.fleet_tokens = 4;
    }
    state
        .system_mut(&SystemId::new("01"))
        .units
        .push(Unit::new(UnitTypeId::new("dreadnought"), sol.clone()));
    let board = state.system_mut(&SystemId::new(border_system(galaxy, content)));
    board.command_tokens.clear();
    board.units.clear();
    for unit in [
        "dreadnought",
        "dreadnought",
        "cruiser",
        "destroyer",
        "fighter",
        "fighter",
    ] {
        board
            .units
            .push(Unit::new(UnitTypeId::new(unit), letnev.clone()));
    }
}

/// The battle with a person in every seat: Sol keeps ships after the return fire (so the Salvage
/// window after SPACE_COMBAT_WON opens), fires an anti-fighter barrage, and holds a second
/// Shields Holding; Letnev holds Sabotage.
fn four_view_battle(state: &mut GameState, sol: &PlayerId, letnev: &PlayerId) {
    if let Some(seat) = state.player_mut(sol) {
        seat.fleet_tokens = 8;
        seat.action_cards.push(ActionCardId::new("sh2"));
    }
    let home = state.system_mut(&SystemId::new("01"));
    for unit in ["cruiser", "cruiser", "destroyer"] {
        home.units
            .push(Unit::new(UnitTypeId::new(unit), sol.clone()));
    }
    if let Some(seat) = state.player_mut(letnev) {
        seat.action_cards.push(ActionCardId::new("sabo1"));
    }
}

/// The system of an invasion: next to Sol's home if one fits, else any, with one planet (two
/// for `coexistence`).
fn invasion_system<'a>(
    galaxy: &'a Galaxy,
    content: &ContentStore,
    coexistence: bool,
) -> Result<&'a str, String> {
    let mut candidates: Vec<&str> = galaxy.adjacent("01").into_iter().collect();
    candidates.extend(galaxy.system_ids());
    candidates
        .iter()
        .copied()
        .find(|id| {
            ti4_content::galaxy::system(content, id, POK).is_some_and(|tile| {
                !tile.is_supernova()
                    && !tile.is_asteroid_field()
                    && if coexistence {
                        tile.planets().len() >= 2
                    } else {
                        tile.planets().len() == 1
                    }
            })
        })
        .ok_or_else(|| {
            format!(
                "no system for the invasion: coexistence={coexistence} candidates={candidates:?}"
            )
        })
}

fn invasion(
    state: &mut GameState,
    galaxy: &Galaxy,
    content: &ContentStore,
    [invader, third, defender]: [&PlayerId; 3],
    id: &str,
) -> Result<(), String> {
    let coexistence = id == INVASION_COEXISTENCE_ID;
    let system = invasion_system(galaxy, content, coexistence)?;
    let planets: Vec<PlanetId> = ti4_content::galaxy::system(content, system, POK)
        .ok_or("the system of the invasion is not in the content")?
        .planets()
        .into_iter()
        .take(if coexistence { 2 } else { 1 })
        .map(PlanetId::new)
        .collect();
    // Enough fleet tokens for the carriers: otherwise the fleet check after the battle removes
    // carriers and then their passengers before the landing.
    state
        .player_mut(invader)
        .ok_or("the invader is not seated")?
        .fleet_tokens = if coexistence { 4 } else { 3 };
    let board = state.system_mut(&SystemId::new(system));
    board.units.clear();
    board.command_tokens.clear();
    board.planet_units.clear();
    for _ in 0..(if coexistence { 4 } else { 3 }) {
        board
            .units
            .push(Unit::new(UnitTypeId::new("carrier"), invader.clone()));
    }
    for _ in 0..(if coexistence { 6 } else { 4 }) {
        board
            .units
            .push(Unit::new(UnitTypeId::new("infantry"), invader.clone()));
    }
    board
        .units
        .push(Unit::new(UnitTypeId::new("mech"), invader.clone()));
    for planet in &planets {
        board.set_control(planet.clone(), defender.clone());
        board.planet_units.insert(
            planet.clone(),
            vec![
                Unit::new(UnitTypeId::new("infantry"), defender.clone()),
                Unit::new(UnitTypeId::new("infantry"), defender.clone()),
                Unit::new(UnitTypeId::new("pds"), defender.clone()),
            ],
        );
    }
    if coexistence {
        board
            .planet_units
            .get_mut(&planets[0])
            .ok_or("the first planet of the invasion")?
            .push(Unit::new(UnitTypeId::new("infantry"), third.clone()));
        board
            .coexisting
            .entry(planets[0].clone())
            .or_default()
            .insert(third.clone());
    }
    // A card of the start of an invasion holds the game at the first public boundary.
    state
        .player_mut(invader)
        .ok_or("the invader is not seated")?
        .action_cards
        .push(ActionCardId::new("blitz"));
    if id == INVASION_PARLEY_ID {
        state
            .player_mut(defender)
            .ok_or("the defender is not seated")?
            .action_cards
            .push(ActionCardId::new("parley"));
    }
    Ok(())
}

fn endgame(state: &mut GameState, seats: [&PlayerId; 3]) {
    for (seat, points) in seats.into_iter().zip([9, 9, 8]) {
        if let Some(player) = state.player_mut(seat) {
            player.victory_points = points;
            player.tactic_tokens = 0;
            player.strategic_tokens = 0;
            player.strategy_cards.clear();
        }
    }
    state.objective_deck.clear();
    state.round = 5;
}

/// Lead From the Front, Negotiate Trade Routes and Corner the Market are revealed and nobody
/// scored; Sol can score the first two.
fn revealed_objectives(state: &mut GameState, sol: &PlayerId) {
    state.revealed_objectives = vec![
        ObjectiveId::new("lead"),
        ObjectiveId::new("trade_routes"),
        ObjectiveId::new("corner"),
    ];
    state.scored_objectives.clear();
    if let Some(seat) = state.player_mut(sol) {
        // Four tokens for Lead From the Front, six trade goods for Negotiate Trade Routes.
        seat.tactic_tokens = 2;
        seat.strategic_tokens = 2;
        seat.trade_goods = 6;
        seat.victory_points = 2;
    }
}

/// Sol controls Mecatol Rex with an infantry, and took the custodians token for its point.
fn hold_mecatol(
    state: &mut GameState,
    galaxy: &Galaxy,
    content: &ContentStore,
    sol: &PlayerId,
) -> Result<(), String> {
    let system = galaxy
        .system_ids()
        .into_iter()
        .find(|id| ti4_engine::seating::is_mecatol(id))
        .ok_or("the board has no Mecatol Rex")?
        .to_owned();
    let planet = ti4_content::galaxy::system(content, &system, POK)
        .and_then(|tile| tile.planets().first().map(|planet| PlanetId::new(*planet)))
        .ok_or("Mecatol Rex has no planet")?;
    let board = state.system_mut(&SystemId::new(&system));
    board.planet_units.remove(&planet);
    board.land(
        &planet,
        &[Unit::new(UnitTypeId::new("infantry"), sol.clone())],
    );
    board.set_control(planet, sol.clone());
    state.custodians_removed = true;
    ti4_engine::objectives::adjust_victory_points(state, sol, 1, "custodians");
    Ok(())
}

fn score_status(state: &mut GameState, sol: &PlayerId) {
    state.phase = Phase::Status;
    state.active = None;
    revealed_objectives(state, sol);
}

fn score_imperial(state: &mut GameState, sol: &PlayerId) {
    let imperial = StrategyCardId::new("pok8imperial");
    if let Some(seat) = state.player_mut(sol) {
        seat.strategy_cards = vec![imperial.clone()];
    }
    state
        .unclaimed_strategy_cards
        .retain(|card| *card != imperial);
    state
        .unclaimed_strategy_cards
        .push(StrategyCardId::new("leadership"));
    revealed_objectives(state, sol);
    state.active = Some(sol.clone());
}

// --- the openings ------------------------------------------------------------------------------

fn subtype(choice: &Choice) -> &str {
    choice
        .context
        .as_ref()
        .map_or("", |context| context.subtype.as_str())
}

/// The first of `ids` that `choice` offers.
fn first_of(choice: &Choice, ids: &[&str]) -> OpeningAnswer {
    ids.iter()
        .find(|id| choice.option(id).is_some())
        .map_or(OpeningAnswer::Stop, |id| {
            OpeningAnswer::Answer((*id).to_owned())
        })
}

/// The first option whose id starts with `prefix`.
fn starting_with(choice: &Choice, prefix: &str) -> Option<OpeningAnswer> {
    choice
        .options
        .iter()
        .find(|option| option.id.starts_with(prefix))
        .map(|option| OpeningAnswer::Answer(option.id.clone()))
}

/// Sol plays Technology and declines what comes before its research.
fn research_opening(choice: &Choice, _: &Opening) -> OpeningAnswer {
    if subtype(choice) == "research_technology" {
        return OpeningAnswer::Stop;
    }
    starting_with(choice, "strategic|")
        .unwrap_or_else(|| first_of(choice, &["strategic", "decline"]))
}

/// Sol plays Imperial and declines what comes before its score.
fn imperial_opening(choice: &Choice, _: &Opening) -> OpeningAnswer {
    if subtype(choice) == "imperial_score_objective" {
        return OpeningAnswer::Stop;
    }
    starting_with(choice, "strategic|")
        .unwrap_or_else(|| first_of(choice, &["strategic", "decline"]))
}

/// Sol activates its home and moves nothing: the production at Jord.
fn production_opening(choice: &Choice, _: &Opening) -> OpeningAnswer {
    if subtype(choice) == "produce_unit" {
        return OpeningAnswer::Stop;
    }
    first_of(choice, &["tactical", "01", "done_loading", "done_moving"])
}

/// Sol activates the system of the invasion with its fleet there, up to the first question of
/// the invasion.
fn invasion_opening(choice: &Choice, opening: &Opening) -> OpeningAnswer {
    if choice
        .context
        .as_ref()
        .is_some_and(|context| context.invasion_seq.is_some())
    {
        return OpeningAnswer::Stop;
    }
    let system = opening.system.as_deref().unwrap_or_default();
    match first_of(choice, &["tactical", system, "done_loading", "done_moving"]) {
        OpeningAnswer::Stop => choice
            .options
            .first()
            .map_or(OpeningAnswer::Stop, |option| {
                OpeningAnswer::Answer(option.id.clone())
            }),
        answer => answer,
    }
}

/// Sol activates the border system and moves and loads every ship of its home into it, up to
/// its first question of the battle that holds a retreat, a sustain or a casualty.
fn battle_opening(choice: &Choice, opening: &Opening) -> OpeningAnswer {
    battle(choice, opening, false)
}

/// As [`battle_opening`], and it stops at the opening of the battle too.
fn four_view_battle_opening(choice: &Choice, opening: &Opening) -> OpeningAnswer {
    battle(choice, opening, true)
}

fn battle(choice: &Choice, opening: &Opening, stop_at_start: bool) -> OpeningAnswer {
    let started = subtype(choice).contains("SPACE_COMBAT_STARTED")
        || subtype(choice).contains("COMBAT_ROUND_STARTED");
    if (stop_at_start && started)
        || choice
            .options
            .iter()
            .any(|option| matches!(option.kind.as_str(), "retreat" | "sustain" | "casualty"))
    {
        return OpeningAnswer::Stop;
    }
    let system = opening.system.as_deref().unwrap_or_default();
    if let OpeningAnswer::Answer(id) = first_of(choice, &["tactical", system]) {
        return OpeningAnswer::Answer(id);
    }
    if let Some(answer) = starting_with(choice, "load|0") {
        return answer;
    }
    if choice.option("done_loading").is_some() {
        return OpeningAnswer::Answer("done_loading".to_owned());
    }
    starting_with(choice, "move|").unwrap_or_else(|| first_of(choice, &["done_moving"]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::setup::{self, TableSetup};

    #[test]
    fn a_table_plays_a_scenario_of_three_seats_on_its_own_seed() {
        let content = ContentStore::embedded();
        let players: Vec<PlayerId> = ["a", "b", "c"].into_iter().map(PlayerId::new).collect();
        for scenario in super::super::SCENARIOS.iter().filter(|s| s.players == 3) {
            let mut boards = std::collections::BTreeSet::new();
            for seed in [7, 42, 12_345] {
                let table = TableSetup {
                    start_from: Some(scenario.id.to_owned()),
                    ..TableSetup::new(3, seed)
                };
                table.check().unwrap();
                assert_eq!(table.game_seed(), seed, "{}", scenario.id);
                let drawn = setup::draw(&table, &[None, None, None]).unwrap();
                let (state, galaxy) = setup::create(content, &table, &players, &drawn)
                    .unwrap_or_else(|error| panic!("{} seed {seed}: {error}", scenario.id));
                assert_eq!(state.players[0].faction.as_str(), "sol");
                assert_eq!(
                    state.active.as_ref(),
                    (state.phase == Phase::Action).then_some(&players[0])
                );
                let opening = super::super::opening(scenario, &state, &galaxy);
                assert_eq!(
                    opening.is_some(),
                    scenario.opening.is_some(),
                    "{}",
                    scenario.id
                );
                boards.insert(format!("{:?}", galaxy.system_ids()));
            }
            assert_eq!(
                boards.len(),
                3,
                "{}: each seed is its own board",
                scenario.id
            );
        }
    }

    #[test]
    fn a_battle_and_an_invasion_open_in_the_system_that_they_prepared() {
        let content = ContentStore::embedded();
        let players: Vec<PlayerId> = ["a", "b", "c"].into_iter().map(PlayerId::new).collect();
        for id in [ONGOING_COMBAT_ID, INVASION_COEXISTENCE_ID] {
            let (state, galaxy) = super::super::start(content, id, &players, 42).unwrap();
            let scenario = super::super::find(id).unwrap();
            let opening = super::super::opening(scenario, &state, &galaxy).unwrap();
            assert_eq!(opening.seat, players[0]);
            let system = opening.system.expect("a prepared system");
            let units = &state.board[&SystemId::new(&system)].units;
            assert!(!units.is_empty(), "{id}: units in {system}");
        }
    }
}
