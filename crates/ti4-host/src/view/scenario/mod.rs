//! Prepared games for testing a client: a seeded game, then a board that a real game only has
//! after hours of play, or a question that a game only reaches after several answers.
//!
//! A scenario is not a saved state. It is a function over the state of a new game, so a game
//! that starts from one is played again from its seed and its answers like any other. A scenario
//! may also have an opening ([`Opening`]): the answers of its first human seat that take the game
//! to the question that the scenario is about. A host plays them as ordinary, recorded decisions
//! of that seat, until the opening stops; the seat is then asked as usual.
//!
//! Both hosts start a scenario through the setup of a table ([`crate::setup`], `start_from`):
//! the server for a lobby and for `/api/dev/scenarios/launch`, the wasm host for the table of a
//! page.

mod three_seats;

use std::collections::BTreeMap;

use serde::Serialize;
use ti4_content::galaxy::Galaxy;
use ti4_content::{ActionCardTiming, ContentStore};
use ti4_engine::choice::{Choice, ChoiceOption};
use ti4_engine::objectives::{self, adjust_victory_points};
use ti4_engine::{phase, promissory, technology};
use ti4_model::content_types::{ContentType, SourceSet};
use ti4_model::id::{
    ActionCardId, LeaderId, ObjectiveId, PlanetId, PlayerId, RelicId, SecretObjectiveId,
    StrategyCardId, SystemId, TechnologyId, UnitTypeId,
};
use ti4_model::state::{GameState, LeaderStatus, Phase};
use ti4_model::units::Unit;

/// The map that a scenario is played on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum ScenarioMap {
    /// The recommended map template for its number of seats.
    Recommended,
    /// The seeded random board of its seed.
    Random,
}

/// What the opening of a scenario does with a question of its seat.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpeningAnswer {
    /// The option with this id.
    Answer(String),
    /// The opening is over: the seat is asked this question and every later one.
    Stop,
}

/// The answer of an opening to a question of its seat ([`Scenario::opening`]).
pub type OpeningFn = fn(&Choice, &Opening) -> OpeningAnswer;

/// A prepared game that a host can start.
#[derive(Debug, Clone, Copy, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Scenario {
    pub id: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    /// What the scenario is about, for a list of the scenarios ("Combat", "Strategy", ...).
    pub category: &'static str,
    /// The game that the scenario is made for: its board is placed on the map of this game.
    pub seed: u64,
    pub players: usize,
    /// The places of the seats that people play; the rule bot plays the others.
    pub humans: &'static [usize],
    /// The scenario finds its systems by the places of the seats, on any board: a table plays
    /// it on its own seed. `seed` is then the seed of a table that names none.
    pub any_seed: bool,
    pub map: ScenarioMap,
    /// The answers of the first seat of `humans` that take the game to the question of the
    /// scenario.
    #[serde(skip)]
    pub opening: Option<OpeningFn>,
}

impl Scenario {
    /// The seed of the game that a table of `seed` starts from this scenario.
    #[must_use]
    pub const fn game_seed(&self, seed: u64) -> u64 {
        if self.any_seed { seed } else { self.seed }
    }
}

pub const LATE_GAME: &str = "late-game";
pub const LATE_GAME_DEFLECTORS: &str = "late-game-deflectors";

const LATE_GAME_SCENARIO: Scenario = Scenario {
    id: LATE_GAME,
    title: "Late game",
    description: "Round 4, seat A (Sol) has the turn. Ten fleets with every ship type and \
        every ship upgrade, Gravity Drive, cargo on planets and in space on the way. Two \
        space docks with Space Dock II, and for what they produce: War Sun, Spec Ops II, \
        Sarween Tools, AI Development Algorithm, 6 trade goods and one exhausted planet. \
        Around them: a gravity rift, a nebula, a supernova, an asteroid field, a wormhole pair, \
        fleets of other seats, a PDS of seat C alone on Perimeter (#60) and a system with \
        Sol's command token. Asteroid fields and \
        the ships of other seats block. Every seat holds seven action cards, 56 different \
        cards in all, and 17 cards are discarded; the other cards of the deck are in the \
        hands of \"Late game with deflectors\". Every seat has three secret objectives, of \
        the action phase and of the other phases: one or two are scored.",
    category: "Board",
    seed: 42,
    players: 8,
    humans: &[0],
    any_seed: false,
    map: ScenarioMap::Recommended,
    opening: None,
};

pub const SCENARIOS: &[Scenario] = &[
    LATE_GAME_SCENARIO,
    Scenario {
        id: LATE_GAME_DEFLECTORS,
        title: "Late game with deflectors",
        description: "The late game, and seat A also has Light/Wave Deflector and Antimass \
            Deflectors: its ships move through the ships of other seats. The engine does not \
            open asteroid fields for Antimass Deflectors yet. The seats hold the action cards \
            that no seat of \"Late game\" holds, and 23 cards are discarded.",
        ..LATE_GAME_SCENARIO
    },
    three_seats::RESEARCH,
    three_seats::TACTICAL,
    three_seats::PRODUCTION,
    three_seats::PRODUCTION_PAYMENT,
    three_seats::PRODUCTION_AUTOSPEND,
    three_seats::SPACE_COMBAT,
    three_seats::ONGOING_COMBAT,
    three_seats::ONGOING_COMBAT_FOUR_VIEWS,
    three_seats::INVASION_FOUR_VIEWS,
    three_seats::INVASION_COEXISTENCE,
    three_seats::INVASION_PARLEY,
    three_seats::INVASION_DRAFT,
    three_seats::ENDGAME,
    three_seats::SCORE_STATUS,
    three_seats::SCORE_IMPERIAL,
    three_seats::FULL_GAME,
    three_seats::STRATEGY_LEADERSHIP,
    three_seats::STRATEGY_DIPLOMACY,
    three_seats::STRATEGY_DIPLOMACY_BASE,
    three_seats::STRATEGY_POLITICS,
    three_seats::STRATEGY_CONSTRUCTION,
    three_seats::STRATEGY_CONSTRUCTION_BASE,
    three_seats::STRATEGY_CONSTRUCTION_TE,
    three_seats::STRATEGY_TRADE,
    three_seats::STRATEGY_WARFARE,
    three_seats::STRATEGY_WARFARE_TE,
    three_seats::STRATEGY_TECHNOLOGY,
    three_seats::STRATEGY_IMPERIAL,
    three_seats::STRATEGY_IMPERIAL_MECATOL,
];

/// The scenario with this id.
#[must_use]
pub fn find(id: &str) -> Option<&'static Scenario> {
    SCENARIOS.iter().find(|scenario| scenario.id == id)
}

fn unknown(id: &str) -> String {
    let known: Vec<&str> = SCENARIOS.iter().map(|scenario| scenario.id).collect();
    format!("no scenario '{id}': the scenarios are {}", known.join(", "))
}

/// The game that the scenario `id` starts: the game of its seed ([`Scenario::game_seed`] of
/// `seed`) on its map, with its board set up. Both hosts start a scenario with this function, so
/// the game is the same in the page and on the server. `player_ids` are the seats in their
/// order; a scenario places its board by the place of a seat, not by its id.
///
/// # Errors
/// A message for an unknown scenario, for another number of seats, or when the map does not build.
pub fn start(
    content: &ContentStore,
    id: &str,
    player_ids: &[PlayerId],
    seed: u64,
) -> Result<(GameState, Galaxy), String> {
    let scenario = find(id).ok_or_else(|| unknown(id))?;
    if player_ids.len() != scenario.players {
        return Err(format!(
            "the scenario '{id}' is made for {} seats, the game has {}",
            scenario.players,
            player_ids.len()
        ));
    }
    let sources = ti4_model::content_types::POK;
    let template = match scenario.map {
        ScenarioMap::Recommended => {
            let loader = crate::maps::TemplateLoader::load()?;
            Some(
                crate::maps::default_template_for(content, &loader, scenario.players, sources)
                    .ok_or_else(|| {
                        format!("no map template builds for {} players", scenario.players)
                    })?,
            )
        }
        ScenarioMap::Random => None,
    };
    let (mut state, mut galaxy) = crate::map::create_game_with_template(
        content,
        player_ids,
        scenario.game_seed(seed),
        template.as_deref(),
    )?;
    apply(id, &mut state, &mut galaxy, content, sources)?;
    Ok((state, galaxy))
}

/// Turns the state of a new game into the scenario `id`. The game must be the one that the
/// scenario names: its seats and the map of its seed.
pub fn apply(
    id: &str,
    state: &mut GameState,
    galaxy: &mut Galaxy,
    content: &ContentStore,
    sources: SourceSet,
) -> Result<(), String> {
    let scenario = find(id).ok_or_else(|| unknown(id))?;
    if state.players.len() != scenario.players {
        return Err(format!(
            "the scenario '{id}' is made for {} seats, the game has {}",
            scenario.players,
            state.players.len()
        ));
    }
    match id {
        LATE_GAME | LATE_GAME_DEFLECTORS => {
            late_game(state, galaxy, content, sources, id == LATE_GAME_DEFLECTORS)
        }
        _ => three_seats::apply(id, state, galaxy, content),
    }
}

/// What the opening of a scenario plays by: its seat and what the scenario prepared.
#[derive(Debug, Clone)]
pub struct Opening {
    /// The seat whose questions the opening answers: the first seat of [`Scenario::humans`].
    pub seat: PlayerId,
    /// The system that the scenario prepared: the system of a battle or of an invasion.
    pub system: Option<String>,
    answer: OpeningFn,
}

/// The opening of a game that starts from `scenario`, at its opening `state` on `galaxy`. None
/// for a scenario without one.
#[must_use]
pub fn opening(scenario: &Scenario, state: &GameState, galaxy: &Galaxy) -> Option<Opening> {
    let answer = scenario.opening?;
    let seat = state.players.get(*scenario.humans.first()?)?.id.clone();
    Some(Opening {
        seat,
        system: three_seats::prepared_system(scenario.id, galaxy, ContentStore::embedded()),
        answer,
    })
}

/// An opening while a host plays it. A host asks it before it asks the seat of the opening; the
/// answers are decisions of that seat like any other. Once the opening stops, it answers nothing.
#[derive(Debug, Clone, Default)]
pub struct OpeningRun(Option<Opening>);

impl OpeningRun {
    #[must_use]
    pub const fn new(opening: Option<Opening>) -> Self {
        Self(opening)
    }

    /// Whether the opening still answers.
    #[must_use]
    pub const fn running(&self) -> bool {
        self.0.is_some()
    }

    /// The answer of the opening to `choice`, or None when the seat is asked: a question of
    /// another seat, or the opening stopped (here, or with an option that `choice` does not have).
    pub fn answer(&mut self, choice: &Choice) -> Option<ChoiceOption> {
        let opening = self.0.as_ref()?;
        if choice.player != opening.seat {
            return None;
        }
        if let OpeningAnswer::Answer(id) = (opening.answer)(choice, opening)
            && let Some(option) = choice.option(&id)
        {
            return Some(option.clone());
        }
        self.0 = None;
        None
    }
}

/// The systems of the late game, on the map of eight seats. Seat A (Sol) is at home in `HOME`.
mod at {
    pub const HOME: &str = "01";
    pub const RIFT: &str = "41";
    pub const NEBULA: &str = "42";
    pub const SUPERNOVA: &str = "43";
    /// Two planets with a space dock each, between the home, the nebula and the middle.
    pub const DOCK: &str = "72";
    /// No ships of seat A, only cargo: every path from the home to the middle crosses it.
    pub const DEPOT: &str = "74";
    /// Behind the rift and behind the home of seat H. A beta wormhole is next to it.
    pub const NORTH: &str = "63";
    /// Seat A activated it: nothing leaves.
    pub const TOKEN: &str = "70";
    pub const WEST: &str = "62";
    pub const SOUTH: &str = "24";
    /// Replaced by `ASTEROIDS`: the map of eight seats has no asteroid field.
    pub const REPLACED: &str = "31";
    pub const ASTEROIDS: &str = "44";
    /// Two planets and no units, next to `NORTH`: its carrier lands there without a fight.
    pub const FREE: &str = "38";
    /// The other beta wormhole, on the far side of the map.
    pub const FAR: &str = "25";
    /// Ships of seat C, with ground forces and a PDS.
    pub const ENEMY_C: &str = "32";
    /// Ships of seat B, three planets.
    pub const ENEMY_B: &str = "75";
    /// A planet of seat A that is exhausted, in `FAR`.
    pub const SPENT: &str = "quann";
    /// Every other seat has a command token here, and seat C a PDS on its planet.
    pub const CROWDED: &str = "60";
    pub const ALL: [&str; 15] = [
        HOME, RIFT, NEBULA, SUPERNOVA, DOCK, DEPOT, NORTH, TOKEN, WEST, SOUTH, REPLACED, FAR,
        ENEMY_C, ENEMY_B, FREE,
    ];
}

/// Units of one seat in one system: `(base type, count)` in the space area and on planets.
struct Placement<'a> {
    system: &'a str,
    space: &'a [(&'a str, usize)],
    planets: &'a [(&'a str, &'a [(&'a str, usize)])],
}

fn late_game(
    state: &mut GameState,
    galaxy: &mut Galaxy,
    content: &ContentStore,
    sources: SourceSet,
    deflectors: bool,
) -> Result<(), String> {
    for system in at::ALL.into_iter().chain([at::CROWDED]) {
        if galaxy.coord_of(system).is_none() {
            return Err(format!(
                "the late game needs the system {system}, which is not on this map"
            ));
        }
    }
    let seats: Vec<PlayerId> = state.players.iter().map(|seat| seat.id.clone()).collect();
    let [a, b, c] = [&seats[0], &seats[1], &seats[2]];

    // An asteroid field next to the home of seat A.
    galaxy
        .replace_system(content, at::REPLACED, at::ASTEROIDS, sources)
        .map_err(|error| error.to_string())?;
    state.board.remove(&SystemId::new(at::REPLACED));
    state.system_mut(&SystemId::new(at::ASTEROIDS));

    // The action phase of round 4: every seat has a card, and seat A is first.
    let mut cards = std::mem::take(&mut state.unclaimed_strategy_cards);
    cards.sort_by_key(|card| {
        ti4_engine::strategy_cards::card_name(content, card.as_str()).as_deref()
            != Some("Leadership")
    });
    let mut cards = cards.into_iter();
    for seat in &seats {
        let card: StrategyCardId = cards
            .next()
            .ok_or_else(|| format!("no strategy card is left for seat {seat}"))?;
        state.deal_strategy_card(seat, card);
    }
    state.unclaimed_strategy_cards = cards.collect();
    state.round = 4;
    if state.phase == Phase::Strategy {
        phase::advance_phase(state);
    }
    if state.active.as_ref() != Some(a) {
        phase::begin_action_turn(state, a);
    }

    let mut technologies = vec![
        "gd", "cr2", "dd2", "dn2", "ff2", "ac2", "ws", // What a production uses.
        "st", "aida", "sd2", "so2",
    ];
    if deflectors {
        technologies.extend(["amd", "lwd"]);
    } else if let Some(seat) = state.player_mut(a) {
        // Sol starts with Antimass Deflectors, and the engine now gives it its effect: the game
        // without deflectors takes them off, so that its asteroid field is closed.
        for technology in ["amd", "lwd"] {
            seat.technologies.remove(&TechnologyId::new(technology));
        }
    }
    for technology in technologies {
        technology::grant(state, a, &TechnologyId::new(technology));
    }
    let sol = state.player_mut(a).ok_or("seat A is not seated")?;
    // 16 command tokens: 15 on the sheet and one in `TOKEN`.
    sol.tactic_tokens = 7;
    sol.fleet_tokens = 5;
    sol.strategic_tokens = 3;
    sol.trade_goods = 6;

    // The start units of seat A go: the fleets below are all that it has.
    for system in state.board.values_mut() {
        system.units.retain(|unit| unit.owner != *a);
        for units in system.planet_units.values_mut() {
            units.retain(|unit| unit.owner != *a);
        }
    }
    let fleets_of_a = [
        Placement {
            system: at::HOME,
            space: &[
                ("flagship", 1),
                ("warsun", 1),
                ("carrier", 1),
                ("fighter", 3),
            ],
            planets: &[(
                "jord",
                &[("infantry", 3), ("mech", 1), ("spacedock", 1), ("pds", 1)],
            )],
        },
        Placement {
            system: at::DOCK,
            space: &[
                ("carrier", 1),
                ("cruiser", 1),
                ("destroyer", 1),
                ("fighter", 2),
                ("infantry", 1),
            ],
            planets: &[
                ("lisis", &[("infantry", 2), ("spacedock", 1)]),
                ("velnor", &[("mech", 1), ("infantry", 1), ("spacedock", 1)]),
            ],
        },
        Placement {
            system: at::NEBULA,
            space: &[("dreadnought", 1), ("destroyer", 1)],
            planets: &[],
        },
        Placement {
            system: at::RIFT,
            space: &[
                ("carrier", 1),
                ("cruiser", 1),
                ("infantry", 2),
                ("fighter", 2),
            ],
            planets: &[],
        },
        Placement {
            system: at::NORTH,
            space: &[("dreadnought", 1), ("carrier", 1), ("fighter", 1)],
            planets: &[("vorhal", &[("infantry", 2), ("mech", 1)])],
        },
        Placement {
            system: at::FAR,
            space: &[("cruiser", 1), ("destroyer", 1)],
            planets: &[("quann", &[("infantry", 1)])],
        },
        Placement {
            system: at::TOKEN,
            space: &[("cruiser", 1), ("dreadnought", 1)],
            planets: &[("kraag", &[("infantry", 1)])],
        },
        Placement {
            system: at::WEST,
            space: &[("cruiser", 1), ("destroyer", 1)],
            planets: &[("semlore", &[("infantry", 1)])],
        },
        Placement {
            system: at::SOUTH,
            space: &[("dreadnought", 1)],
            planets: &[("meharxull", &[("infantry", 1)])],
        },
        Placement {
            system: at::DEPOT,
            space: &[("fighter", 2)],
            planets: &[
                ("vegamajor", &[("infantry", 2)]),
                ("vegaminor", &[("mech", 1)]),
            ],
        },
    ];
    for fleet in &fleets_of_a {
        place(state, content, sources, a, fleet)?;
    }
    state
        .system_mut(&SystemId::new(at::TOKEN))
        .place_token(a.clone());

    // The other seats, for the paths that they block and for the steps after the movement.
    place(
        state,
        content,
        sources,
        c,
        &Placement {
            system: at::ENEMY_C,
            space: &[("cruiser", 2), ("destroyer", 1)],
            planets: &[("dalbootha", &[("infantry", 2), ("pds", 1)])],
        },
    )?;
    place(
        state,
        content,
        sources,
        b,
        &Placement {
            system: at::ENEMY_B,
            space: &[("carrier", 1), ("cruiser", 1), ("fighter", 2)],
            planets: &[("loki", &[("infantry", 2)])],
        },
    )?;
    // A PDS alone on the planet of `CROWDED`: a movement there meets space cannon offense and
    // no ships, and the landings come after it.
    place(
        state,
        content,
        sources,
        c,
        &Placement {
            system: at::CROWDED,
            space: &[],
            planets: &[("perimeter", &[("pds", 1)])],
        },
    )?;
    for seat in &seats[1..] {
        state
            .system_mut(&SystemId::new(at::CROWDED))
            .place_token(seat.clone());
    }
    for seat in [b, &seats[6], &seats[7]] {
        state
            .system_mut(&SystemId::new(at::ENEMY_C))
            .place_token(seat.clone());
    }
    state
        .system_mut(&SystemId::new(at::DEPOT))
        .place_token(seats[7].clone());

    // What the exploration left: attachments, relic fragments, and the cards out of the decks.
    let drawn: [(&PlayerId, &str, &str, Option<&str>); 9] = [
        (a, "CULTURAL", "ds", Some("vegamajor")),
        (a, "INDUSTRIAL", "cybernetic", Some("velnor")),
        (a, "INDUSTRIAL", "biotic", Some("lisis")),
        (a, "CULTURAL", "crf1", Some("vorhal")),
        (a, "CULTURAL", "crf2", Some("semlore")),
        (a, "CULTURAL", "crf3", Some("vegaminor")),
        (a, "HAZARDOUS", "hrf1", Some("meharxull")),
        (b, "CULTURAL", "pw", Some("loki")),
        (b, "FRONTIER", "urf1", None),
    ];
    for (seat, deck, card, planet) in drawn {
        let cards = state
            .exploration_decks
            .get_mut(deck)
            .ok_or_else(|| format!("the game has no {deck} deck"))?;
        let at = cards
            .iter()
            .position(|held| held == card)
            .ok_or_else(|| format!("the {deck} deck has no card '{card}'"))?;
        cards.remove(at);
        let planet = planet.map(PlanetId::new);
        let outcome = match ti4_engine::exploration::resolution(content, card).as_deref() {
            Some("Attach") => {
                let planet = planet
                    .as_ref()
                    .ok_or_else(|| format!("the card '{card}' needs a planet"))?;
                let attachment = ti4_engine::exploration::attachment_for(content, card, planet);
                state
                    .planet_attachments
                    .entry(planet.clone())
                    .or_default()
                    .push(attachment.clone());
                format!("attached:{attachment}")
            }
            Some("Fragment") => {
                ti4_engine::exploration::gain_fragment(state, seat, deck);
                "fragment".to_owned()
            }
            other => return Err(format!("the card '{card}' resolves as {other:?}")),
        };
        state
            .exploration_log
            .push(ti4_model::state::ExplorationRecord {
                round: 2,
                player: seat.clone(),
                deck: deck.to_owned(),
                card: card.to_owned(),
                planet,
                outcome,
                system: None,
                result: String::new(),
                gains: Default::default(),
                placed: Vec::new(),
            });
    }

    // The upgrades of seat A are on its sheet: its units on the board are the upgraded ones.
    technology::apply_unit_upgrades(state, content, sources, a);

    // Cards that seat A used this round, and two relics: one ready, one exhausted.
    technology::grant(state, a, &TechnologyId::new("bs"));
    for relic in ["dynamiscore", "prophetstears"] {
        let relic = RelicId::new(relic);
        state.relic_deck.retain(|held| held != &relic);
        state
            .player_mut(a)
            .ok_or("seat A is not seated")?
            .relics
            .push(relic);
    }
    // One planet of seat A is spent: it cannot pay.
    state.exhaust_planet(PlanetId::new(at::SPENT));
    let sol = state.player_mut(a).ok_or("seat A is not seated")?;
    sol.exhausted_technologies.insert(TechnologyId::new("bs"));
    sol.exhausted_relics.insert(RelicId::new("prophetstears"));
    sol.leaders
        .insert(LeaderId::new("solagent"), LeaderStatus::Exhausted);

    deal_action_cards(state, content, !deflectors)?;
    deal_secret_objectives(state, content)?;
    deal_points(state, content)
}

/// How many secret objectives a seat of the late game has, scored and unscored: the limit.
const SECRETS: usize = ti4_engine::secrets::HAND_LIMIT;

/// Gives every seat three secret objectives: one that is scored in the action phase, one of the
/// status phase, and a third of the agenda phase for the first seats and of the status phase for
/// the others. One of the three is scored for a seat with an even place at the table and two for
/// an odd one, a different one from seat to seat; a scored objective is one victory point. Seat A
/// has scored its objective of the status phase and holds one of the action phase and one of the
/// agenda phase.
///
/// The objectives that the seats had go back under the deck first, and the deck is taken in the
/// order of the ids, so the hands do not depend on the shuffle.
fn deal_secret_objectives(state: &mut GameState, content: &ContentStore) -> Result<(), String> {
    for seat in 0..state.players.len() {
        let hand = std::mem::take(&mut state.players[seat].secret_objectives);
        state.secret_deck.extend(hand);
    }
    let mut ids = state.secret_deck.clone();
    ids.sort();
    let of_phase = |phase: &str| -> Vec<SecretObjectiveId> {
        ids.iter()
            .filter(|id| {
                content
                    .get(ContentType::SecretObjectives, id.as_str())
                    .and_then(|record| record.text("phase"))
                    .is_some_and(|printed| printed.eq_ignore_ascii_case(phase))
            })
            .cloned()
            .collect()
    };
    let (action, status, agenda) = (of_phase("action"), of_phase("status"), of_phase("agenda"));
    let seats = state.players.len();
    if action.len() < seats || status.len() + agenda.len() < 2 * seats {
        return Err(format!(
            "the late game needs {seats} secret objectives of the action phase and {} others, \
             the deck has {} and {}",
            2 * seats,
            action.len(),
            status.len() + agenda.len()
        ));
    }
    // The third objective: the agenda phase while it has one, then the rest of the status phase.
    let third = agenda.iter().chain(status.iter().skip(seats));
    for (seat, third) in (0..seats).zip(third) {
        let hand = [action[seat].clone(), status[seat].clone(), third.clone()];
        debug_assert_eq!(hand.len(), SECRETS);
        state.secret_deck.retain(|held| !hand.contains(held));
        let scored = 1 + seat % 2;
        let player = state.players[seat].id.clone();
        for (index, id) in hand.into_iter().enumerate() {
            // The scored ones begin at another place of the hand from seat to seat.
            if (index + SECRETS - (seat + 1) % SECRETS) % SECRETS < scored {
                state.record_score(&player, ObjectiveId::new(id.as_str()));
                adjust_victory_points(state, &player, 1, "secret_objective");
            } else {
                state.players[seat].secret_objectives.push(id);
            }
        }
    }
    Ok(())
}

/// The public objectives that each seat of the late game has scored, by its place at the table:
/// places in the revealed objectives, five of stage I and then two of stage II. Seat F has scored
/// none. Seat A has scored one, so with its secret objective its hero stays locked (three scored
/// objectives unlock it).
const PUBLIC_SCORED: [&[usize]; 8] = [
    &[5],
    &[0, 2],
    &[1, 3, 4],
    &[0],
    &[0, 1, 2, 6],
    &[],
    &[0, 3],
    &[0, 2, 4, 6],
];

/// Gives the seats the points of a late game, so that every source of a point has a seat: five
/// public objectives of stage I and two of stage II, scored unevenly ([`PUBLIC_SCORED`]), and
///
/// - the custodians token for seat E,
/// - Imperial with Mecatol Rex twice for seat A and once for seat B,
/// - Support for the Throne of seat C with seat F and of seat G with seat D,
/// - Political Censure for seat C,
/// - Mutiny against: seats F and H voted for and lost a point,
/// - the Shard of the Throne (the relic) for seat G.
///
/// With the secret objectives that is 5, 5, 5, 4, 7, 2, 4 and 6 points. Every point is a row of
/// the ledger, as in a game that was played.
fn deal_points(state: &mut GameState, content: &ContentStore) -> Result<(), String> {
    let seats: Vec<PlayerId> = state.players.iter().map(|seat| seat.id.clone()).collect();
    if seats.len() != PUBLIC_SCORED.len() {
        return Err(format!(
            "the points of the late game are for {} seats",
            PUBLIC_SCORED.len()
        ));
    }
    let [a, b, c, d, e, f, g, h] = [0, 1, 2, 3, 4, 5, 6, 7].map(|place| seats[place].clone());

    // The objectives that the game began with stay, and the decks give the others.
    let revealed = |state: &GameState, stage: u8| {
        state
            .revealed_objectives
            .iter()
            .filter(|id| objectives::stage_of(content, id) == Some(stage))
            .cloned()
            .collect::<Vec<_>>()
    };
    for (stage, count) in [(1, 5), (2, 2)] {
        while revealed(state, stage).len() < count {
            objectives::reveal_stage(state, content, stage)
                .ok_or_else(|| format!("the deck has no objective of stage {stage} left"))?;
        }
    }
    let public: Vec<ObjectiveId> = revealed(state, 1)
        .into_iter()
        .take(5)
        .chain(revealed(state, 2).into_iter().take(2))
        .collect();
    for (seat, scored) in seats.iter().zip(PUBLIC_SCORED) {
        for place in scored {
            let id = &public[*place];
            let points = objectives::points_for(content, id)
                .ok_or_else(|| format!("the objective {id} has no printed points"))?;
            state.record_score(seat, id.clone());
            adjust_victory_points(state, seat, points, "objective");
        }
    }

    state.custodians_removed = true;
    adjust_victory_points(state, &e, 1, "custodians");
    for seat in [&a, &a, &b] {
        adjust_victory_points(state, seat, 1, "imperial_primary");
    }
    for (owner, holder) in [(&c, &f), (&g, &d)] {
        let note = promissory::support(&promissory::faction_name(state, owner));
        if !promissory::receive(state, holder, &note) {
            return Err(format!("seat {holder} cannot have the note {note}"));
        }
    }
    state.enact_law("censure", c.as_str());
    adjust_victory_points(state, &c, 1, "agenda:censure");
    for seat in [&f, &h] {
        adjust_victory_points(state, seat, -1, "agenda:mutiny");
    }
    // A law of each kind of outcome that the deck has: an elected player, and "For". Homeland
    // Defense Act changes no screen of the scenario.
    state.enact_law("defense_act", "for");
    state
        .agenda_deck
        .retain(|agenda| !["censure", "defense_act", "mutiny"].contains(&agenda.as_str()));
    let shard = RelicId::new(ti4_engine::relics::SHARD);
    state.relic_deck.retain(|held| held != &shard);
    state
        .player_mut(&g)
        .ok_or("seat G is not seated")?
        .relics
        .push(shard);
    adjust_victory_points(state, &g, 1, ti4_engine::relics::SHARD);
    Ok(())
}

/// How many action cards a seat of the late game holds: the hand limit.
const HAND: usize = 7;

/// Gives every seat a hand of action cards with no card twice at the table, so that the two
/// late games together have every card of the deck in a hand.
///
/// The printed names of the deck are put in the order of their groups
/// ([`ti4_content::ActionCardTiming`]), and every name goes to one of the two games, spread
/// evenly over that order: `HAND` names for each seat to the first game, the rest to the second.
/// The names of a game are dealt around the table one by one, so a seat has cards of many
/// groups. The hands that the seats had go back under the deck first. Then some cards of the
/// deck are the discard pile: 17 in the first game and 23 in the second.
fn deal_action_cards(
    state: &mut GameState,
    content: &ContentStore,
    first: bool,
) -> Result<(), String> {
    for seat in 0..state.players.len() {
        let hand = std::mem::take(&mut state.players[seat].action_cards);
        state.action_card_deck.extend(hand);
    }
    // One id for each printed name: the smallest of its copies.
    let mut names: BTreeMap<(Option<ActionCardTiming>, String), ActionCardId> = BTreeMap::new();
    let mut ids = state.action_card_deck.clone();
    ids.sort();
    for id in ids {
        let record = content.get(ContentType::ActionCards, id.as_str());
        let printed = |key: &str| {
            record
                .and_then(|record| record.text(key))
                .unwrap_or_default()
        };
        let timing = ti4_content::action_card_timing(printed("phase"), printed("window"));
        names
            .entry((timing, printed("name").to_owned()))
            .or_insert(id);
    }
    let seats = state.players.len();
    let in_first = HAND * seats;
    if names.len() < in_first {
        return Err(format!(
            "the late game needs {in_first} action cards of different names, the deck has {}",
            names.len()
        ));
    }
    let total = names.len();
    let mut dealt = 0;
    for (index, id) in names.into_values().enumerate() {
        // The name is of the first game when the share of the first game moves on with it.
        let of_first = (index + 1) * in_first / total > index * in_first / total;
        if of_first != first {
            continue;
        }
        state.action_card_deck.retain(|held| *held != id);
        state.players[dealt % seats].action_cards.push(id);
        dealt += 1;
    }
    // Cards that were played: the top of what is left of the deck is the discard pile.
    let discarded = if first { 17 } else { 23 };
    if state.action_card_deck.len() < discarded {
        return Err(format!(
            "the late game discards {discarded} action cards, the deck has {}",
            state.action_card_deck.len()
        ));
    }
    let pile = state.action_card_deck.drain(..discarded);
    state.discarded_action_cards.extend(pile);
    Ok(())
}

/// Puts the units of `fleet` on the board. A planet with units of the seat is then its planet.
fn place(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    seat: &PlayerId,
    fleet: &Placement<'_>,
) -> Result<(), String> {
    let faction = state
        .player(seat)
        .ok_or_else(|| format!("seat {seat} is not seated"))?
        .faction
        .to_string();
    let units = |kinds: &[(&str, usize)]| -> Result<Vec<Unit>, String> {
        let mut units = Vec::new();
        for (base, count) in kinds {
            // The unit of the faction where it has its own (Sol's carrier), or the common one.
            let kind = ti4_content::units::faction_unit(content, &faction, base, sources)
                .or_else(|| ti4_content::units::unit_type(content, base, sources))
                .ok_or_else(|| format!("no unit type '{base}'"))?;
            for _ in 0..*count {
                units.push(Unit::new(UnitTypeId::new(kind.id()), seat.clone()));
            }
        }
        Ok(units)
    };
    let known = ti4_content::galaxy::system(content, fleet.system, sources)
        .ok_or_else(|| format!("no system '{}'", fleet.system))?
        .planets();
    let space = units(fleet.space)?;
    let mut landed = Vec::new();
    for (planet, kinds) in fleet.planets {
        if !known.contains(planet) {
            return Err(format!(
                "the system {} has no planet '{planet}'",
                fleet.system
            ));
        }
        landed.push((PlanetId::new(*planet), units(kinds)?));
    }
    let system = state.system_mut(&SystemId::new(fleet.system));
    system.add(&space);
    for (planet, units) in landed {
        system.land(&planet, &units);
        system.set_control(planet, seat.clone());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view::tactical::{
        ACTIVATION_SUBTYPE, Blocked, CombatFact, FREE_ACTIVATION_SUBTYPE, INVASION_SUBTYPE,
        MOVEMENT_SUBTYPE, MovementFacts, ProductionFacts, ShipFact, TacticalFacts, Unavailable,
        project_tactical_facts,
    };
    use ti4_engine::choice::Window;
    use ti4_engine::choice::{Choice, ChoiceOption};
    use ti4_engine::decision_context::{DecisionContext, DecisionSource};
    use ti4_engine::production::{PRODUCE_KIND, ProductionWindow};
    use ti4_model::content_types::POK;

    fn seat() -> PlayerId {
        PlayerId::new("a")
    }

    fn game(id: &str) -> (GameState, Galaxy) {
        let content = ContentStore::embedded();
        let scenario = find(id).expect("a scenario");
        let ids: Vec<PlayerId> = ["a", "b", "c", "d", "e", "f", "g", "h"][..scenario.players]
            .iter()
            .map(|name| PlayerId::new(*name))
            .collect();
        let loader = crate::maps::TemplateLoader::load().expect("the templates load");
        let template = crate::maps::default_template_for(content, &loader, scenario.players, POK)
            .expect("a template");
        let (mut state, mut galaxy) =
            crate::map::create_game_with_template(content, &ids, scenario.seed, Some(&template))
                .expect("a game");
        apply(id, &mut state, &mut galaxy, content, POK).expect("the scenario applies");
        (state, galaxy)
    }

    /// What the movement into `active` shows, in a copy of the game.
    fn facts(state: &GameState, galaxy: &Galaxy, active: &str) -> MovementFacts {
        let mut state = state.clone();
        state.active_system = Some(SystemId::new(active));
        let choice = Choice::new(seat(), "movement", vec![ChoiceOption::decline()]).contextualized(
            DecisionContext::new(
                seat(),
                DecisionSource::Rule("89.2".to_owned()),
                MOVEMENT_SUBTYPE,
                Phase::Action,
                1,
            ),
        );
        match project_tactical_facts(&state, ContentStore::embedded(), POK, galaxy, &choice) {
            Some(TacticalFacts::Movement(facts)) => facts,
            other => panic!("no movement facts: {other:?}"),
        }
    }

    fn ships<'a>(facts: &'a MovementFacts, origin: &str) -> Vec<&'a ShipFact> {
        let ships: Vec<&ShipFact> = facts
            .ships
            .iter()
            .filter(|ship| ship.origin == origin)
            .collect();
        assert!(!ships.is_empty(), "no ship in {origin}");
        ships
    }

    fn ship<'a>(facts: &'a MovementFacts, origin: &str, unit: &str) -> &'a ShipFact {
        ships(facts, origin)
            .into_iter()
            .find(|ship| ship.unit == unit)
            .unwrap_or_else(|| panic!("no {unit} in {origin}: {:?}", ships(facts, origin)))
    }

    #[test]
    fn an_unknown_scenario_names_the_known_ones() {
        let (mut state, mut galaxy) = game(LATE_GAME);
        let error = apply(
            "nope",
            &mut state,
            &mut galaxy,
            ContentStore::embedded(),
            POK,
        )
        .expect_err("no such scenario");
        assert!(error.contains(LATE_GAME), "{error}");
    }

    /// The printed names of the action cards in the hands, seat by seat.
    fn hands(state: &GameState) -> Vec<Vec<String>> {
        let content = ContentStore::embedded();
        state
            .players
            .iter()
            .map(|seat| {
                seat.action_cards
                    .iter()
                    .map(|id| {
                        content
                            .get(ContentType::ActionCards, id.as_str())
                            .and_then(|record| record.text("name"))
                            .expect("a printed card")
                            .to_owned()
                    })
                    .collect()
            })
            .collect()
    }

    #[test]
    fn every_seat_has_three_secret_objectives_scored_and_not_of_several_phases() {
        use std::collections::BTreeSet;

        let content = ContentStore::embedded();
        let (state, _) = game(LATE_GAME);
        let facts = crate::view::secret_objectives::project_secret_objective_facts(&state, content);
        let phase = |id: &ti4_model::id::SecretObjectiveId| {
            content
                .get(ContentType::SecretObjectives, id.as_str())
                .and_then(|record| record.text("phase"))
                .expect("a printed objective")
        };
        let mut seen = BTreeSet::new();
        let mut held_phases = BTreeSet::new();
        for (place, seat) in state.players.iter().enumerate() {
            let scored = facts.scored.get(&seat.id).cloned().unwrap_or_default();
            assert_eq!(scored.len(), 1 + place % 2, "{} scored", seat.id);
            assert_eq!(
                seat.secret_objectives.len() + scored.len(),
                3,
                "{}",
                seat.id
            );
            // One of the action phase and two of the others, in the hand or scored.
            let phases: Vec<&str> = seat
                .secret_objectives
                .iter()
                .chain(&scored)
                .map(phase)
                .collect();
            assert_eq!(phases.iter().filter(|phase| **phase == "Action").count(), 1);
            held_phases.extend(seat.secret_objectives.iter().map(phase));
            seen.extend(seat.secret_objectives.iter().chain(&scored).cloned());
        }
        // No objective twice, and the rest is the deck.
        assert_eq!(seen.len(), 24);
        assert_eq!(facts.deck, 16);
        assert!(state.secret_deck.iter().all(|id| !seen.contains(id)));
        assert_eq!(held_phases, BTreeSet::from(["Action", "Agenda", "Status"]));
        // Seat A has scored its objective of the status phase and holds the two others.
        let a = &state.players[0];
        assert_eq!(
            facts.scored[&a.id].iter().map(phase).collect::<Vec<_>>(),
            ["Status"]
        );
        let held: Vec<&str> = a.secret_objectives.iter().map(phase).collect();
        assert_eq!(held, ["Action", "Agenda"]);
    }

    #[test]
    fn the_points_of_a_late_game_come_from_every_source() {
        use crate::view::score::{PointSourceKind, project_score_facts};

        let content = ContentStore::embedded();
        for id in [LATE_GAME, LATE_GAME_DEFLECTORS] {
            let (state, _) = game(id);
            let facts = project_score_facts(&state, content);
            let seat = |place: usize| state.players[place].id.clone();

            let stages: Vec<i32> = facts.public.iter().map(|public| public.points).collect();
            assert_eq!(stages.iter().filter(|points| **points == 1).count(), 5);
            assert_eq!(stages.iter().filter(|points| **points == 2).count(), 2);
            // An objective that most seats scored, and one that a single seat scored.
            let scored: Vec<usize> = facts.public.iter().map(|p| p.scored.len()).collect();
            assert!(scored.contains(&5) && scored.contains(&1), "{scored:?}");
            let points = |kind, name: &str| {
                facts
                    .sources
                    .iter()
                    .find(|source| source.kind == kind && source.name == name)
                    .unwrap_or_else(|| panic!("no source {kind:?} {name:?}"))
                    .points
                    .clone()
            };
            assert_eq!(points(PointSourceKind::Secrets, "").len(), 8);
            assert_eq!(
                points(PointSourceKind::Custodians, ""),
                BTreeMap::from([(seat(4), 1)])
            );
            assert_eq!(
                points(PointSourceKind::Imperial, ""),
                BTreeMap::from([(seat(0), 2), (seat(1), 1)])
            );
            assert_eq!(
                points(PointSourceKind::Support, ""),
                BTreeMap::from([(seat(3), 1), (seat(5), 1)])
            );
            let support = &facts.sources[3];
            assert_eq!(
                support.from,
                BTreeMap::from([(seat(3), vec![seat(6)]), (seat(5), vec![seat(2)])])
            );
            assert_eq!(
                points(PointSourceKind::Agenda, "Political Censure"),
                BTreeMap::from([(seat(2), 1)])
            );
            assert_eq!(
                points(PointSourceKind::Agenda, "Mutiny"),
                BTreeMap::from([(seat(5), -1), (seat(7), -1)])
            );
            assert_eq!(
                points(PointSourceKind::Relic, "Shard of the Throne"),
                BTreeMap::from([(seat(6), 1)])
            );
            // Every point has its row, and the ledger says the same.
            assert!(facts.unexplained.is_empty(), "{:?}", facts.unexplained);
            let totals: Vec<i32> = state.players.iter().map(|p| p.victory_points).collect();
            assert_eq!(totals, [5, 5, 5, 4, 7, 2, 4, 6]);
            for player in &state.players {
                let ledger: i32 = state
                    .vp_ledger
                    .iter()
                    .filter(|(seat, _, _)| *seat == player.id)
                    .map(|(_, delta, _)| *delta)
                    .sum();
                assert_eq!(ledger, player.victory_points, "{}", player.id);
            }
        }
    }

    #[test]
    fn the_late_game_has_two_laws_and_a_thinner_agenda_deck() {
        let (state, _) = game("late-game");
        assert_eq!(
            state.laws,
            BTreeMap::from([
                ("censure".to_owned(), "c".to_owned()),
                ("defense_act".to_owned(), "for".to_owned()),
            ])
        );
        let printed =
            crate::view::components::component_catalog(ContentStore::embedded(), POK).agendas;
        assert_eq!(
            crate::view::agendas::project_agenda_facts(&state).deck,
            printed.len() - 3,
            "the two laws and Mutiny are not in the deck"
        );
    }

    #[test]
    fn the_two_late_games_have_every_action_card_in_a_hand_once() {
        use std::collections::BTreeSet;

        let (first, _) = game(LATE_GAME);
        let (second, _) = game(LATE_GAME_DEFLECTORS);
        let names = |state: &GameState| -> BTreeSet<String> {
            hands(state).into_iter().flatten().collect()
        };
        // Seven cards for each seat, and no name twice at the table.
        assert!(hands(&first).iter().all(|hand| hand.len() == 7));
        assert_eq!(names(&first).len(), 56);
        // The other game has the names that are left, and no seat has more than seven.
        let printed: BTreeSet<String> =
            crate::view::components::component_catalog(ContentStore::embedded(), POK)
                .action_cards
                .into_iter()
                .map(|card| card.name)
                .collect();
        assert!(names(&first).is_disjoint(&names(&second)));
        let both: BTreeSet<String> = names(&first).union(&names(&second)).cloned().collect();
        assert_eq!(both, printed);
        let held: usize = hands(&second).iter().map(Vec::len).sum();
        assert_eq!(held, printed.len() - 56);
        assert!(
            hands(&second)
                .iter()
                .all(|hand| (1..=7).contains(&hand.len()))
        );
        // Nothing is lost: a card is in a hand, in the deck or in the discard pile.
        assert_eq!(first.discarded_action_cards.len(), 17);
        assert_eq!(second.discarded_action_cards.len(), 23);
        for state in [&first, &second] {
            let held: usize = state
                .players
                .iter()
                .map(|seat| seat.action_cards.len())
                .sum();
            let discarded = state.discarded_action_cards.len();
            assert_eq!(held + state.action_card_deck.len() + discarded, 122);
        }
        // A hand has cards of several groups: seat A can play some at once and some in a window.
        let seat_a = &hands(&first)[0];
        for name in ["War Effort", "Skilled Retreat", "Hack Election"] {
            assert!(seat_a.contains(&name.to_owned()), "no {name} in {seat_a:?}");
        }
    }

    #[test]
    fn a_pds_alone_waits_for_the_fleet_of_the_rift() {
        let (state, galaxy) = game(LATE_GAME);
        let crowded = state.system_state(&SystemId::new(at::CROWDED));
        assert!(crowded.units.is_empty());
        let guns: Vec<(&str, &str)> = crowded
            .planet_units
            .values()
            .flatten()
            .map(|unit| (unit.owner.as_str(), unit.type_id.as_str()))
            .collect();
        assert_eq!(guns, [("c", "pds")]);
        // The ships in the rift reach it, and roll on their way.
        let facts = facts(&state, &galaxy, at::CROWDED);
        for ship in ships(&facts, at::RIFT) {
            let route = ship.r#move.as_ref().expect("a ship of the rift arrives");
            assert_eq!(route.rifts, [at::RIFT], "{ship:?}");
        }
    }

    #[test]
    fn seat_a_has_the_turn_with_every_ship_type_upgraded() {
        let (state, _) = game(LATE_GAME);
        assert_eq!(state.phase, Phase::Action);
        assert_eq!(state.active, Some(seat()));
        assert!(
            state
                .players
                .iter()
                .all(|seat| seat.strategy_cards.len() == 1)
        );
        assert_eq!(state.tokens_in_reinforcements(&seat()), 0);
        let kinds: std::collections::BTreeSet<String> = state
            .board
            .values()
            .flat_map(|system| system.units_of(&seat()))
            .map(|unit| unit.type_id.to_string())
            .collect();
        for kind in [
            "sol_flagship",
            "warsun",
            "sol_carrier2",
            "cruiser2",
            "destroyer2",
            "dreadnought2",
            "fighter2",
        ] {
            assert!(kinds.contains(kind), "no {kind} in {kinds:?}");
        }
    }

    #[test]
    fn many_fleets_reach_the_middle_and_a_token_holds_one_back() {
        let (state, galaxy) = game(LATE_GAME);
        let facts = facts(&state, &galaxy, at::DEPOT);
        let origins: std::collections::BTreeSet<&str> = facts
            .ships
            .iter()
            .filter(|ship| ship.r#move.is_some())
            .map(|ship| ship.origin.as_str())
            .collect();
        assert!(origins.len() >= 5, "{origins:?}");
        for ship in ships(&facts, at::TOKEN) {
            assert_eq!(ship.blocked, Some(Blocked::CommandToken), "{ship:?}");
        }
        for ship in ships(&facts, at::FAR) {
            assert_eq!(ship.blocked, Some(Blocked::Range), "{ship:?}");
        }
        // Five tokens in the fleet pool, and more ships than that can arrive.
        assert_eq!(facts.fleet_supply.limit, 5);
        let arriving = facts
            .ships
            .iter()
            .filter(|ship| ship.fleet && ship.r#move.is_some())
            .count();
        assert!(arriving > 5, "{arriving}");
    }

    #[test]
    fn the_flagship_needs_gravity_drive_and_loads_on_its_way() {
        let (state, galaxy) = game(LATE_GAME);
        let facts = facts(&state, &galaxy, at::ENEMY_B);
        assert!(facts.gravity_drive);
        let flagship = ship(&facts, at::HOME, "sol_flagship");
        assert_eq!(flagship.move_value, 1);
        let route = flagship.r#move.as_ref().expect("the flagship arrives");
        assert!(route.gravity_drive, "{route:?}");
        assert_eq!(route.path.len(), 3, "{route:?}");
        // The game chooses the path, and the flagship loads in the system that it passes.
        let passed = route.path[1].as_str();
        let systems: std::collections::BTreeSet<&str> = flagship
            .loads
            .iter()
            .map(|index| facts.cargo[*index].system.as_str())
            .collect();
        // Nothing is loaded in a system with a command token of the seat.
        assert!(systems.contains(at::HOME), "{systems:?}");
        assert!(
            systems
                .iter()
                .all(|system| [at::HOME, passed].contains(system))
        );
    }

    #[test]
    fn a_fleet_of_another_seat_in_the_active_system_is_a_battle() {
        let (state, galaxy) = game(LATE_GAME);
        let content = ContentStore::embedded();
        let base = |units: &BTreeMap<String, usize>| {
            let mut kinds: BTreeMap<String, usize> = BTreeMap::new();
            for (unit, count) in units {
                let kind = ti4_content::units::unit_type(content, unit, POK).expect("a unit");
                *kinds.entry(kind.base_type().to_owned()).or_default() += count;
            }
            kinds
        };
        let counts = |pairs: &[(&str, usize)]| -> BTreeMap<String, usize> {
            pairs
                .iter()
                .map(|(unit, n)| ((*unit).to_owned(), *n))
                .collect()
        };

        let into_b = facts(&state, &galaxy, at::ENEMY_B);
        assert!(into_b.ships.iter().any(|ship| ship.r#move.is_some()));
        let battle = into_b.battle.expect("seat B defends");
        assert_eq!(battle.defender, "b");
        assert_eq!(battle.attacker.faction, "sol");
        assert!(battle.attacker.units.is_empty(), "{battle:?}");
        assert_eq!(
            base(&battle.defender_side.units),
            counts(&[("carrier", 1), ("cruiser", 1), ("fighter", 2)])
        );
        assert!(battle.defender_side.damaged.is_empty());
        assert!(battle.defender_side.guns.is_empty(), "{battle:?}");
        let faction_b = state
            .player(&PlayerId::new("b"))
            .expect("seat B")
            .faction
            .as_str();
        assert_eq!(battle.defender_side.faction, faction_b);

        // The PDS of seat C on its planet fires before the combat.
        let battle = facts(&state, &galaxy, at::ENEMY_C)
            .battle
            .expect("seat C defends");
        assert_eq!(battle.defender, "c");
        assert_eq!(
            base(&battle.defender_side.units),
            counts(&[("cruiser", 2), ("destroyer", 1)])
        );
        assert_eq!(base(&battle.defender_side.guns), counts(&[("pds", 1)]));

        // No other seat has ships in the depot: no combat.
        assert_eq!(facts(&state, &galaxy, at::DEPOT).battle, None);
    }

    #[test]
    fn the_battle_is_on_the_wire_with_its_sides() {
        let (state, galaxy) = game(LATE_GAME);
        let facts = facts(&state, &galaxy, at::ENEMY_C);
        let wire = serde_json::to_value(&facts).expect("serializes");
        let battle = &wire["battle"];
        assert_eq!(battle["defender"], "c");
        assert_eq!(battle["attacker"]["faction"], "sol");
        assert!(battle["attacker"].get("units").is_none(), "{battle}");
        // A side of the fact is a side that `ti4_calc` reads.
        let side: ti4_calc::odds::SideInput =
            serde_json::from_value(battle["defender_side"].clone()).expect("a side");
        let odds = ti4_calc::odds::space_odds_of(
            ContentStore::embedded(),
            POK,
            &serde_json::from_value(battle["attacker"].clone()).expect("a side"),
            &side,
            ti4_calc::odds::SpaceOptions {
                simulations: Some(10),
                seed: Some(1),
                ..Default::default()
            },
        )
        .expect("odds");
        assert!(
            (odds.defender_win_rate - 1.0).abs() < 1e-9,
            "no ship attacks"
        );
        let back: MovementFacts = serde_json::from_value(wire).expect("reads back");
        assert_eq!(back, facts);
    }

    #[test]
    fn the_depot_is_on_the_way_of_a_ship() {
        let (state, galaxy) = game(LATE_GAME);
        // Space area and two planets, for some ship that passes: the game chooses the paths.
        let found = [at::ENEMY_B, at::ENEMY_C, at::HOME, at::DOCK, at::TOKEN]
            .into_iter()
            .any(|active| {
                let facts = facts(&state, &galaxy, active);
                facts.ships.iter().any(|ship| {
                    let places: std::collections::BTreeSet<Option<&str>> = ship
                        .loads
                        .iter()
                        .map(|index| &facts.cargo[*index])
                        .filter(|pool| pool.system == at::DEPOT)
                        .map(|pool| pool.source.as_deref())
                        .collect();
                    places.len() == 3
                })
            });
        assert!(found, "no ship passes the depot");
    }

    #[test]
    fn a_fleet_loads_at_three_places_of_its_system() {
        let (state, galaxy) = game(LATE_GAME);
        let facts = facts(&state, &galaxy, at::DEPOT);
        let carrier = ship(&facts, at::DOCK, "sol_carrier2");
        let places: std::collections::BTreeSet<Option<&str>> = carrier
            .loads
            .iter()
            .map(|index| &facts.cargo[*index])
            .filter(|pool| pool.system == at::DOCK)
            .map(|pool| pool.source.as_deref())
            .collect();
        assert_eq!(
            places,
            [None, Some("lisis"), Some("velnor")].into_iter().collect()
        );
    }

    #[test]
    fn the_rift_the_nebula_and_the_supernova_shape_the_paths() {
        let (state, galaxy) = game(LATE_GAME);
        // Out of the rift: every ship that starts there rolls.
        let into_nebula = facts(&state, &galaxy, at::NEBULA);
        for ship in ships(&into_nebula, at::RIFT) {
            let route = ship.r#move.as_ref().expect("a ship leaves the rift");
            assert_eq!(route.rifts, [at::RIFT], "{ship:?}");
        }
        // From the north the rift is on the way, and the other way has the ships of seat H.
        let dreadnought = ship(&into_nebula, at::NORTH, "dreadnought2");
        let route = dreadnought
            .r#move
            .as_ref()
            .expect("the dreadnought arrives");
        assert_eq!(route.path, [at::NORTH, at::RIFT, at::NEBULA]);
        assert_eq!(route.rifts, [at::RIFT]);
        // Out of the nebula a ship moves 1.
        let dock = facts(&state, &galaxy, at::DOCK);
        let nebula = ship(&dock, at::NEBULA, "dreadnought2");
        assert!(nebula.nebula);
        assert!(nebula.r#move.is_some());
        let depot = facts(&state, &galaxy, at::DEPOT);
        assert_eq!(
            ship(&depot, at::NEBULA, "dreadnought2").blocked,
            Some(Blocked::Range)
        );
        // The supernova is the short way from the west to the nebula.
        // The cruiser goes around it, and the destroyer needs Gravity Drive for that.
        for (unit, boosted) in [("cruiser2", false), ("destroyer2", true)] {
            let ship = ship(&into_nebula, at::WEST, unit);
            let route = ship.r#move.as_ref().expect("the ship goes around");
            assert_eq!(route.gravity_drive, boosted, "{ship:?}");
            assert_eq!(route.path.len(), 4, "{ship:?}");
            assert!(!route.path.contains(&at::SUPERNOVA.to_owned()), "{ship:?}");
        }
    }

    #[test]
    fn a_wormhole_brings_the_far_fleet_to_the_north() {
        let (state, galaxy) = game(LATE_GAME);
        let facts = facts(&state, &galaxy, at::NORTH);
        for ship in ships(&facts, at::FAR) {
            let route = ship.r#move.as_ref().expect("the far fleet arrives");
            assert_eq!(route.path.len(), 3, "{ship:?}");
        }
    }

    #[test]
    fn the_carrier_of_the_north_lands_on_two_free_planets() {
        let (state, galaxy) = game(LATE_GAME);
        let content = ContentStore::embedded();
        let free = SystemId::new(at::FREE);
        let planets = ti4_engine::planets::in_system(&state, content, POK, &free);
        assert_eq!(planets.len(), 2);
        let there = state.system_state(&free);
        assert!(there.units.is_empty() && there.planet_units.values().all(Vec::is_empty));

        // The carrier arrives with no roll on its way, and loads on the planet that it leaves.
        let moving = facts(&state, &galaxy, at::FREE);
        let carrier = ship(&moving, at::NORTH, "sol_carrier2");
        let route = carrier.r#move.as_ref().expect("the carrier arrives");
        assert_eq!(route.path, [at::NORTH, at::FREE]);
        assert!(route.rifts.is_empty());
        let loads: Vec<&str> = carrier
            .loads
            .iter()
            .map(|index| moving.cargo[*index].unit.as_str())
            .collect();
        assert!(
            loads.contains(&"sol_infantry2") && loads.contains(&"sol_mech"),
            "{loads:?}"
        );

        // In space of the free system, each kind can land on each planet.
        let mut state = state;
        state.active_system = Some(free.clone());
        let kinds = [("sol_infantry2", 2), ("sol_mech", 1)];
        for (kind, count) in kinds {
            for _ in 0..count {
                state
                    .system_mut(&free)
                    .units
                    .push(Unit::new(UnitTypeId::new(kind), seat()));
            }
        }
        let mut options = Vec::new();
        for (index, (kind, _)) in kinds.iter().enumerate() {
            for planet in &planets {
                options.push(
                    ChoiceOption::labelled(
                        format!("commit|{index}|{planet}"),
                        ti4_engine::invasion::COMMIT_KIND,
                        "land",
                    )
                    .with("planet", planet.to_string())
                    .with("unit", (*kind).to_owned())
                    .with("damaged", false),
                );
            }
        }
        options.push(ChoiceOption::decline());
        let choice = Choice::new(seat(), "commit", options).contextualized(DecisionContext::new(
            seat(),
            DecisionSource::Rule("49".to_owned()),
            INVASION_SUBTYPE,
            Phase::Action,
            4,
        ));
        let Some(TacticalFacts::Invasion(landing)) =
            project_tactical_facts(&state, content, POK, &galaxy, &choice)
        else {
            panic!("no invasion facts");
        };
        assert_eq!(landing.active, at::FREE);
        let forces: Vec<(&str, usize)> = landing
            .forces
            .iter()
            .map(|force| (force.unit.as_str(), force.count))
            .collect();
        assert_eq!(forces, kinds);
        assert_eq!(landing.planets.len(), 2);
        for planet in &landing.planets {
            assert_eq!(planet.lands, [0, 1], "{planet:?}");
        }
    }

    /// The first choice of a production of seat A in `system`, and its facts.
    fn production(state: &GameState, galaxy: &Galaxy, system: &str) -> (Choice, ProductionFacts) {
        let content = ContentStore::embedded();
        let mut state = state.clone();
        let system = SystemId::new(system);
        state.active_system = Some(system.clone());
        // Sarween Tools, as the game sets it when the units use PRODUCTION.
        state.production_discount_remaining = 1;
        let mut window = ProductionWindow::new(&state, content, POK, &seat(), &system);
        window.refresh(&state, content, POK);
        let choice = window
            .pending_choice(&state, content, POK)
            .expect("the seat can produce");
        match project_tactical_facts(&state, content, POK, galaxy, &choice) {
            Some(TacticalFacts::Production(facts)) => (choice, facts),
            other => panic!("no production facts: {other:?}"),
        }
    }

    #[test]
    fn the_docks_produce_every_unit_with_its_price_and_its_places() {
        let (state, galaxy) = game(LATE_GAME);
        for system in [at::DOCK, at::HOME] {
            let (choice, facts) = production(&state, &galaxy, system);
            assert_eq!(facts.system, system);
            assert_eq!(facts.capacity.used, 0);
            assert_eq!(facts.discount, 1, "Sarween Tools");
            assert_eq!((facts.credit, facts.free), (0, false));
            assert_eq!((facts.trade_goods, facts.trade_good_worth), (6, 1));
            assert_eq!(facts.fleet_supply.limit, 5);

            // One fact for each unit that the game offers, with the price of the offer.
            let offered: Vec<&ChoiceOption> = choice
                .options
                .iter()
                .filter(|option| option.kind == PRODUCE_KIND)
                .collect();
            let open: Vec<_> = facts
                .units
                .iter()
                .filter(|unit| unit.unavailable.is_none())
                .collect();
            assert_eq!(open.len(), offered.len());
            for option in offered {
                let unit = open
                    .iter()
                    .find(|unit| option.payload["unit"] == unit.unit.as_str())
                    .unwrap_or_else(|| panic!("no fact for {:?}", option.payload["unit"]));
                assert_eq!(option.payload["printed_cost"], unit.cost);
                assert_eq!(option.payload["yield"], unit.per);
                assert!(unit.supply.is_none_or(|left| left > 0), "{unit:?}");
                assert!(!unit.places.is_empty(), "{unit:?}");
            }
            // Every unit of a production has a row: the one flagship is on the board.
            assert_eq!(facts.units.len(), 9);
            let flagship = facts
                .units
                .iter()
                .find(|unit| unit.fleet && unit.supply == Some(0))
                .expect("the flagship");
            assert_eq!(flagship.unavailable, Some(Unavailable::Supply));
            let unit = |id: &str| {
                facts
                    .units
                    .iter()
                    .find(|unit| unit.unit == id)
                    .unwrap_or_else(|| panic!("no {id} in {:?}", facts.units))
            };
            // The war sun needs its technology, and the upgrades are what is produced.
            assert_eq!(unit("warsun").cost, 12);
            assert!(unit("warsun").fleet && !unit("warsun").cargo);
            assert_eq!(unit("warsun").places, ["space"]);
            let fighter = unit("fighter2");
            assert_eq!((fighter.cost, fighter.per), (1, 2));
            assert!(!fighter.fleet && fighter.cargo);
            let infantry = unit("sol_infantry2");
            assert_eq!((infantry.cost, infantry.per), (1, 2));
            assert!(infantry.cargo);
            // Fighters and infantry have no limit in the box.
            assert_eq!((fighter.supply, infantry.supply), (None, None));
            assert!(unit("warsun").supply.is_some());
            // A ground force goes to a planet with a dock: the dock system has two.
            let docks: &[&str] = if system == at::DOCK {
                &["lisis", "velnor"]
            } else {
                &["jord"]
            };
            assert_eq!(infantry.places, docks);

            // Every ready planet of the seat pays, and the spent one does not.
            let pays: Vec<&str> = facts
                .planets
                .iter()
                .map(|planet| planet.planet.as_str())
                .collect();
            assert!(
                pays.contains(&"jord") && pays.contains(&"lisis"),
                "{pays:?}"
            );
            assert!(!pays.contains(&at::SPENT), "{pays:?}");
            assert!(facts.planets.iter().all(|planet| planet.worth > 0));
        }
        // Space Dock II: the resources of the planet and 4.
        assert_eq!(production(&state, &galaxy, at::HOME).1.capacity.limit, 8);
    }

    #[test]
    fn a_unit_that_cannot_be_produced_stays_listed_and_says_why() {
        let (state, galaxy) = game(LATE_GAME);
        let find = |facts: &ProductionFacts, id: &str| {
            facts
                .units
                .iter()
                .find(|unit| unit.unit == id)
                .unwrap_or_else(|| panic!("no {id} in {:?}", facts.units))
                .clone()
        };
        // What is printed on the unit.
        let (_, facts) = production(&state, &galaxy, at::DOCK);
        let cruiser = find(&facts, "cruiser2");
        assert_eq!(
            cruiser.combat,
            Some(CombatFact {
                hits_on: 6,
                dice: 1
            })
        );
        assert_eq!((cruiser.move_value, cruiser.capacity), (3, 1));
        let sun = find(&facts, "warsun");
        assert_eq!(sun.combat.map(|combat| combat.dice), Some(3));
        assert!(
            sun.abilities.iter().any(|text| text == "SUSTAIN DAMAGE"),
            "{sun:?}"
        );
        assert!(
            sun.abilities
                .iter()
                .any(|text| text == "BOMBARDMENT 3+ (x3)"),
            "{sun:?}"
        );
        assert_eq!(
            sun.rules.as_deref(),
            Some("Other player's units in this system lose PLANETARY SHIELD.")
        );

        // Without its technology the war sun is the printed one, and it is not produced.
        let mut plain = state.clone();
        plain
            .player_mut(&seat())
            .expect("seat A")
            .technologies
            .retain(|technology| technology.as_str() != "ws");
        let (_, facts) = production(&plain, &galaxy, at::DOCK);
        let sun = find(&facts, "warsun");
        assert_eq!(sun.unavailable, Some(Unavailable::Technology));
        assert_eq!(sun.cost, 12);

        // Ships of another player: no ship is produced, the ground forces are.
        let mut blockaded = state.clone();
        blockaded
            .system_mut(&SystemId::new(at::DOCK))
            .units
            .iter_mut()
            .for_each(|unit| unit.owner = PlayerId::new("b"));
        let (_, facts) = production(&blockaded, &galaxy, at::DOCK);
        assert_eq!(
            find(&facts, "cruiser2").unavailable,
            Some(Unavailable::Blockade)
        );
        assert_eq!(find(&facts, "sol_infantry2").unavailable, None);

        // A unit that the seat only cannot pay for has no reason: the client shows the resources.
        let mut poor = state.clone();
        poor.player_mut(&seat()).expect("seat A").trade_goods = 0;
        let (_, rich) = production(&state, &galaxy, at::DOCK);
        for planet in &rich.planets {
            poor.exhaust_planet(ti4_model::id::PlanetId::new(planet.planet.as_str()));
        }
        let (choice, facts) = production(&poor, &galaxy, at::DOCK);
        assert!(facts.planets.is_empty());
        assert!(!choice.options.iter().any(|option| {
            option
                .payload
                .get("unit")
                .is_some_and(|unit| unit == "warsun")
        }));
        assert_eq!(find(&facts, "warsun").unavailable, None);
    }

    #[test]
    fn every_scenario_of_three_seats_deals_cards_that_the_engine_knows() {
        let content = ContentStore::embedded();
        let ids: Vec<PlayerId> = ["a", "b", "c"].map(PlayerId::new).to_vec();
        for scenario in SCENARIOS.iter().filter(|scenario| scenario.players == 3) {
            let (state, _) = start(content, scenario.id, &ids, 42).expect("it starts");
            let held: Vec<&StrategyCardId> = state
                .players
                .iter()
                .flat_map(|seat| &seat.strategy_cards)
                .collect();
            for card in &held {
                assert!(
                    ti4_engine::strategy_cards::card_name(content, card.as_str()).is_some(),
                    "{}: {card}",
                    scenario.id
                );
                assert!(
                    !state.unclaimed_strategy_cards.contains(card),
                    "{}: {card} is held and unclaimed",
                    scenario.id
                );
            }
        }
    }

    #[test]
    fn the_strategy_scenario_gives_sol_its_card_and_what_the_card_needs() {
        let content = ContentStore::embedded();
        let ids: Vec<PlayerId> = ["a", "b", "c"].map(PlayerId::new).to_vec();
        let (state, _) = start(content, "strategy_leadership", &ids, 42).expect("it starts");
        let [sol, hacan] = [&ids[0], &ids[1]].map(|id| state.player(id).expect("a seat"));
        assert_eq!(sol.strategy_cards, [StrategyCardId::new("pok1leadership")]);
        assert!(
            hacan
                .strategy_cards
                .iter()
                .all(|card| card.as_str() != "pok1leadership")
        );
        assert_eq!((sol.trade_goods, hacan.trade_goods), (4, 3));
        assert_eq!(state.active.as_ref(), Some(&ids[0]));
        // Its command token on a system next to its home, and exhausted planets of another.
        assert_eq!(state.systems_with_token(&ids[0]).len(), 1);
        assert!(
            state
                .controlled_planets(&ids[0])
                .iter()
                .any(|(_, planet)| state.exhausted_planets.contains(*planet))
        );
    }

    #[test]
    fn the_diplomacy_scenarios_exhaust_more_planets_than_diplomacy_readies() {
        let content = ContentStore::embedded();
        let ids: Vec<PlayerId> = ["a", "b", "c"].map(PlayerId::new).to_vec();
        for (scenario, card) in [
            ("strategy_diplomacy", "pok2diplomacy"),
            ("strategy_diplomacy_base", "base2"),
        ] {
            let (state, _) = start(content, scenario, &ids, 42).expect("it starts");
            let sol = state.player(&ids[0]).expect("a seat");
            assert_eq!(sol.strategy_cards, [StrategyCardId::new(card)]);
            let exhausted = |seat: &PlayerId| {
                state
                    .controlled_planets(seat)
                    .iter()
                    .filter(|(_, planet)| state.exhausted_planets.contains(*planet))
                    .count()
            };
            assert!(exhausted(&ids[0]) > 2, "{scenario}");
            assert!(exhausted(&ids[1]) > 2, "{scenario}");
        }
    }

    #[test]
    fn the_construction_scenarios_play_the_card_on_a_table_of_its_set() {
        let content = ContentStore::embedded();
        let ids: Vec<PlayerId> = ["a", "b", "c"].map(PlayerId::new).to_vec();
        for (scenario, card, warfare) in [
            ("strategy_construction", "pok4construction", "pok6warfare"),
            ("strategy_construction_base", "base4", "pok6warfare"),
            ("strategy_construction_te", "te4construction", "te6warfare"),
        ] {
            let (state, _) = start(content, scenario, &ids, 42).expect("it starts");
            let sol = state.player(&ids[0]).expect("a seat");
            assert_eq!(sol.strategy_cards, [StrategyCardId::new(card)]);
            assert!(
                state
                    .unclaimed_strategy_cards
                    .contains(&StrategyCardId::new(warfare)),
                "{scenario}"
            );
            assert_eq!(state.unclaimed_strategy_cards.len(), 5, "{scenario}");
            let pds = state
                .board
                .values()
                .flat_map(|system| system.planet_units.values().flatten())
                .filter(|unit| unit.owner == ids[0] && unit.type_id.as_str() == "pds")
                .count();
            assert_eq!(pds, 1, "{scenario}");
        }
    }

    #[test]
    fn the_imperial_scenarios_reveal_objectives_and_one_holds_mecatol_rex() {
        let content = ContentStore::embedded();
        let ids: Vec<PlayerId> = ["a", "b", "c"].map(PlayerId::new).to_vec();
        for (scenario, mecatol) in [
            ("strategy_imperial", false),
            ("strategy_imperial_mecatol", true),
        ] {
            let (state, _) = start(content, scenario, &ids, 42).expect("it starts");
            let sol = state.player(&ids[0]).expect("a seat");
            assert_eq!(sol.strategy_cards, [StrategyCardId::new("pok8imperial")]);
            assert_eq!(state.revealed_objectives.len(), 3, "{scenario}");
            let holds = state
                .controlled_planets(&ids[0])
                .into_iter()
                .any(|(system, _)| ti4_engine::seating::is_mecatol(system.as_str()));
            assert_eq!(holds, mecatol, "{scenario}");
            assert_eq!(sol.victory_points, 2 + i32::from(mecatol), "{scenario}");
        }
    }

    /// Technology: Sol holds the card and can pay the second research of its primary in more than
    /// one way.
    #[test]
    fn the_technology_scenario_gives_sol_the_card_and_a_payment_to_choose() {
        let content = ContentStore::embedded();
        let ids: Vec<PlayerId> = ["a", "b", "c"].map(PlayerId::new).to_vec();
        let (state, _) = start(content, "strategy_technology", &ids, 42).expect("it starts");
        let sol = state.player(&ids[0]).expect("a seat");
        assert_eq!(sol.strategy_cards, [StrategyCardId::new("pok7technology")]);
        let plans = ti4_engine::payment::plans(
            &state,
            content,
            ti4_model::content_types::POK,
            &ids[0],
            ti4_engine::strategy_cards::TECHNOLOGY_PRIMARY_SECOND_COST,
            ti4_engine::production::Spend::Resources,
        );
        assert!(plans.len() > 1, "several ways to pay six resources");
    }

    /// Thunder's Edge Warfare is a tactical action in any system, with no command token: its
    /// question has the facts of an activation, which say that it costs no token, also for the
    /// system that has the seat's token.
    #[test]
    fn the_warfare_scenarios_and_the_free_activation() {
        let content = ContentStore::embedded();
        let ids: Vec<PlayerId> = ["a", "b", "c"].map(PlayerId::new).to_vec();
        for (scenario, card) in [
            ("strategy_warfare", "pok6warfare"),
            ("strategy_warfare_te", "te6warfare"),
        ] {
            let (state, galaxy) = start(content, scenario, &ids, 42).expect("it starts");
            let sol = state.player(&ids[0]).expect("a seat");
            assert_eq!(sol.strategy_cards, [StrategyCardId::new(card)]);
            let tokens: Vec<SystemId> = state
                .systems_with_token(&ids[0])
                .into_iter()
                .cloned()
                .collect();
            assert_eq!(tokens.len(), 1, "{scenario}");
            if card != "te6warfare" {
                continue;
            }
            let free = |subtype: &str| {
                let choice = Choice::new(
                    seat(),
                    "Warfare: a tactical action without a command token",
                    ["01", tokens[0].as_str()]
                        .into_iter()
                        .map(|system| ChoiceOption::labelled(system, "activate", system))
                        .collect(),
                )
                .contextualized(DecisionContext::new(
                    seat(),
                    DecisionSource::Content(card.to_owned()),
                    subtype,
                    Phase::Action,
                    1,
                ));
                match project_tactical_facts(&state, content, POK, &galaxy, &choice) {
                    Some(TacticalFacts::Activation(facts)) => facts,
                    other => panic!("no activation facts: {other:?}"),
                }
            };
            let facts = free(FREE_ACTIVATION_SUBTYPE);
            assert!(!facts.costs_token);
            assert_eq!(facts.systems.len(), 2);
            assert!(free(ACTIVATION_SUBTYPE).costs_token);
        }
    }

    #[test]
    fn the_production_facts_are_tagged_on_the_wire() {
        let (state, galaxy) = game(LATE_GAME);
        let (_, facts) = production(&state, &galaxy, at::DOCK);
        let value =
            serde_json::to_value(TacticalFacts::Production(facts.clone())).expect("it serializes");
        assert_eq!(value["kind"], "production");
        assert_eq!(value["system"], at::DOCK);
        let back: TacticalFacts = serde_json::from_value(value).expect("it reads back");
        assert_eq!(back, TacticalFacts::Production(facts));
    }

    #[test]
    fn the_exploration_left_attachments_fragments_and_thinner_decks() {
        let (state, galaxy) = game(LATE_GAME);
        let content = ContentStore::embedded();
        let facts =
            crate::view::exploration::project_exploration_facts(&state, content, POK, &galaxy);
        let full = crate::view::exploration::exploration_catalog(content, POK).decks;

        assert_eq!(facts.drawn.len(), 9);
        assert_eq!(facts.decks["CULTURAL"].len() + 5, full["CULTURAL"].len());
        assert_eq!(
            facts.decks["INDUSTRIAL"].len() + 2,
            full["INDUSTRIAL"].len()
        );
        assert!(!facts.decks["CULTURAL"].contains(&"ds".to_owned()));
        let attached = |planet: &str| state.planet_attachments[&PlanetId::new(planet)].clone();
        assert_eq!(attached("vegamajor"), ["dysonsphere"]);
        // Velnor has a technology specialty of its own, Lisis has none.
        assert_eq!(attached("velnor"), ["cyberneticstat"]);
        assert_eq!(attached("lisis"), ["biotic"]);
        assert_eq!(attached("loki"), ["paradiseworld"]);

        let view = crate::projection::project_player_view(state.player(&seat()).unwrap());
        assert_eq!(view.relic_fragments["CULTURAL"], 3);
        assert_eq!(view.relic_fragments["HAZARDOUS"], 1);
        let other =
            crate::projection::project_player_view(state.player(&PlayerId::new("b")).unwrap());
        assert_eq!(other.relic_fragments["FRONTIER"], 1);

        // What seat A used, for the cards of its sheet.
        assert_eq!(
            view.leaders[&LeaderId::new("solagent")],
            LeaderStatus::Exhausted
        );
        assert_eq!(
            view.leaders[&LeaderId::new("solhero")],
            LeaderStatus::Locked
        );
        assert!(
            view.exhausted_technologies
                .contains(&TechnologyId::new("bs"))
        );
        assert_eq!(view.relics.len(), 2);
        assert!(
            view.exhausted_relics
                .contains(&RelicId::new("prophetstears"))
        );
        assert!(!state.relic_deck.contains(&RelicId::new("dynamiscore")));

        // A planet of each deck is free: the two of the free system, and one of the industrial
        // deck. The planet where every other seat has a token is held by the PDS of seat C.
        let explores = |planet: &str| facts.explores.get(&PlanetId::new(planet)).cloned();
        assert_eq!(explores("abyz"), Some(vec!["HAZARDOUS".to_owned()]));
        assert_eq!(explores("abaddon"), Some(vec!["CULTURAL".to_owned()]));
        assert_eq!(explores("arnor"), Some(vec!["INDUSTRIAL".to_owned()]));
        assert_eq!(explores("perimeter"), None);
        assert_eq!(explores("vegamajor"), None);
    }

    #[test]
    fn the_asteroid_field_is_closed_without_deflectors_and_the_dreadnought_goes_around() {
        // South to west: through the asteroid field it is 2, around it 3.
        for id in [LATE_GAME, LATE_GAME_DEFLECTORS] {
            let (state, galaxy) = game(id);
            let facts = facts(&state, &galaxy, at::WEST);
            let dreadnought = ship(&facts, at::SOUTH, "dreadnought2");
            let route = dreadnought
                .r#move
                .as_ref()
                .expect("the dreadnought arrives");
            if id == LATE_GAME {
                assert!(route.gravity_drive, "{id}: {route:?}");
                assert!(!route.path.contains(&at::ASTEROIDS.to_owned()), "{id}");
            } else {
                // Antimass Deflectors opens the field: the second game goes through it.
                assert!(!route.gravity_drive, "{id}: {route:?}");
                assert!(route.path.contains(&at::ASTEROIDS.to_owned()), "{id}");
            }
        }
    }

    #[test]
    fn light_wave_deflector_opens_the_home_of_seat_h() {
        let (plain, galaxy) = game(LATE_GAME);
        let (open, _) = game(LATE_GAME_DEFLECTORS);
        // North to the dock: only through the home of seat H, where its ships are.
        let to_dock = |state: &GameState| {
            let facts = facts(state, &galaxy, at::DOCK);
            ship(&facts, at::NORTH, "dreadnought2").clone()
        };
        assert_eq!(to_dock(&plain).blocked, Some(Blocked::Range));
        let route = to_dock(&open).r#move.expect("the dreadnought arrives");
        assert_eq!(route.path.len(), 3, "{route:?}");
        assert!(route.rifts.is_empty());
    }
}
