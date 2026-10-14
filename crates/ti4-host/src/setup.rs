//! The setup of a game, for both hosts: what a host can choose for a table, what the table looks
//! like before it starts, and the game that it starts.
//!
//! The server calls this for the tables of its lobbies and the wasm host for the table of the
//! page, so a table of a seed is the same game on both. Nothing here knows a lobby record, a
//! credential or a socket: a host keeps its table as it likes and hands over a [`TableSetup`].

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use ti4_content::ContentStore;
use ti4_content::galaxy::Galaxy;
use ti4_engine::seating;
use ti4_model::id::{FactionId, PlayerId};
use ti4_model::state::GameState;

use ti4_view::map::{SpeakerOrder, create_game_with_template_seated};
use ti4_view::scenario::{self, Scenario};

use crate::maps::catalog::{self, MapChoice, MapPreview};

/// The fewest and the most seats of a table.
pub const PLAYER_COUNTS: std::ops::RangeInclusive<usize> = 2..=8;

/// The sets of strategy cards that a table can play.
pub mod card_set {
    /// Thunder's Edge cards: Construction and Warfare as printed in Thunder's Edge.
    pub const TE: &str = "te";
    /// Prophecy of Kings cards (Codex I Diplomacy).
    pub const POK: &str = "pok";
    /// Base game with the Codex I Diplomacy: the original Construction text.
    pub const BASE_GAME_CODEX1: &str = "base_game_codex1";

    /// The set a table plays when it names none.
    pub const DEFAULT: &str = TE;
    /// Every set a player may choose, in the order the lobby lists them.
    ///
    /// `base_game` is not offered: its Diplomacy (`base2`) prints "ready each exhausted planet you
    /// control in that system" while the engine resolves the Codex I "ready up to 2 exhausted
    /// planets", so the text shown would not be the effect played.
    pub const OFFERED: [&str; 3] = [TE, POK, BASE_GAME_CODEX1];

    /// A set name nobody offers.
    #[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
    #[error("unknown strategy_card_set {0:?} (expected one of: te, pok, base_game_codex1)")]
    pub struct UnknownCardSet(pub String);

    /// Checks a name that a client gave.
    ///
    /// # Errors
    /// [`UnknownCardSet`] for anything outside [`OFFERED`].
    pub fn validate(name: &str) -> Result<&'static str, UnknownCardSet> {
        OFFERED
            .iter()
            .copied()
            .find(|offered| *offered == name)
            .ok_or_else(|| UnknownCardSet(name.to_owned()))
    }

    /// The name of a set for a player.
    #[must_use]
    pub fn name(set: &str) -> &'static str {
        match set {
            TE => "Thunder's Edge",
            POK => "Prophecy of Kings",
            BASE_GAME_CODEX1 => "Base game with Codex I",
            _ => "Strategy cards",
        }
    }
}

/// The kinds of a seat that a host plays itself, as a table names them.
pub mod bot_kind {
    /// The rule bot (`ti4-rule-bot`).
    pub const RULE: &str = "smart";
    /// A seat that decides at random, among what the game allows.
    pub const RANDOM: &str = "random";
}

/// Who sits in each seat of a running game. The update of a game carries it, so it is in
/// `ti4-view`.
pub use ti4_view::seats::{HUMAN_KIND, SeatFact, TableSeats};

/// How the players of a table get their seats at Start.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SeatOrder {
    /// Drawn from the seed of the game ([`seating::lobby_seat_order`]). What a table gets
    /// unless it asks otherwise: no host moves a seat.
    #[default]
    Random,
    /// The order in which the players came. Fixtures, tests and start presets use it, and a
    /// table that starts from a scenario (a case of a scenario is at a certain seat).
    Join,
}

impl SeatOrder {
    /// The stored and requested name: `"random"` or `"join"`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Random => "random",
            Self::Join => "join",
        }
    }

    /// Parse a requested name.
    ///
    /// # Errors
    /// A message naming the accepted values.
    pub fn parse(name: &str) -> Result<Self, String> {
        match name {
            "random" => Ok(Self::Random),
            "join" => Ok(Self::Join),
            other => Err(format!(
                "unknown seat_order {other:?} (expected \"random\" or \"join\")"
            )),
        }
    }

    /// A stored value; `None` (a record written before the option) and anything unknown are
    /// [`SeatOrder::Random`].
    #[must_use]
    pub fn from_stored(name: Option<&str>) -> Self {
        name.and_then(|n| Self::parse(n).ok()).unwrap_or_default()
    }
}

fn default_card_set() -> String {
    card_set::DEFAULT.to_owned()
}

/// What a host chose for a table. The players and their factions are not a part of it: they are
/// given to [`draw`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TableSetup {
    pub player_count: usize,
    /// The map of the table; a table that names none plays the seeded random board.
    #[serde(default = "random_map")]
    pub map: MapChoice,
    /// One of the sets of the content (a table offers [`card_set::OFFERED`]).
    #[serde(default = "default_card_set")]
    pub strategy_cards: String,
    #[serde(default)]
    pub seat_order: SeatOrder,
    #[serde(default)]
    pub speaker_order: SpeakerOrder,
    /// The id of a scenario of [`ti4_view::scenario`]: the table starts that game, with its seed,
    /// its seats, its map, its cards and its factions. `None` is round 1.
    #[serde(default)]
    pub start_from: Option<String>,
    /// The seed of the table. It tells the seats and the factions that nobody chose, so a host
    /// with several people keeps it to itself.
    pub seed: u64,
}

const fn random_map() -> MapChoice {
    MapChoice::Random
}

impl TableSetup {
    /// A table of `player_count` seats on the map that such a table starts with, with the usual
    /// cards and a drawn order.
    #[must_use]
    pub fn new(player_count: usize, seed: u64) -> Self {
        Self {
            player_count,
            map: MapChoice::from_stored(catalog::recommended_for(player_count).as_deref()),
            strategy_cards: default_card_set(),
            seat_order: SeatOrder::default(),
            speaker_order: SpeakerOrder::default(),
            start_from: None,
            seed,
        }
    }

    /// The scenario that the table starts from.
    ///
    /// # Errors
    /// A message that lists the scenarios, for an id that is none of them.
    pub fn scenario(&self) -> Result<Option<&'static Scenario>, String> {
        let Some(id) = self.start_from.as_deref() else {
            return Ok(None);
        };
        scenario::find(id).map(Some).ok_or_else(|| {
            let known: Vec<&str> = scenario::SCENARIOS.iter().map(|s| s.id).collect();
            format!("no scenario '{id}': the scenarios are {}", known.join(", "))
        })
    }

    /// The seed of the game that the table starts: a scenario is the game of its own seed.
    #[must_use]
    pub fn game_seed(&self) -> u64 {
        self.scenario()
            .ok()
            .flatten()
            .map_or(self.seed, |scenario| scenario.seed)
    }

    /// Checks the values that a client can give: the number of seats, the set of strategy
    /// cards, the scenario with its number of seats, and that the map builds for the count.
    ///
    /// # Errors
    /// A message that names the value and what it can be.
    pub fn check(&self) -> Result<(), String> {
        if !PLAYER_COUNTS.contains(&self.player_count) {
            return Err(format!(
                "player_count must be {}-{}",
                PLAYER_COUNTS.start(),
                PLAYER_COUNTS.end()
            ));
        }
        card_set::validate(&self.strategy_cards).map_err(|error| error.to_string())?;
        match self.scenario()? {
            Some(scenario) if scenario.players != self.player_count => Err(format!(
                "\"{}\" is a game of {} players",
                scenario.title, scenario.players
            )),
            Some(_) => Ok(()),
            None => catalog::validate(&self.map, self.player_count),
        }
    }
}

/// One set of strategy cards that a table can play.
#[derive(Debug, Clone, Serialize)]
pub struct CardSetOption {
    pub id: &'static str,
    pub name: &'static str,
    /// The table plays this set when it names none.
    pub default: bool,
    /// The eight cards, by initiative.
    pub cards: Vec<CardOption>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CardOption {
    pub id: String,
    pub name: String,
    pub initiative: i32,
}

/// The factions of one product, in the order of [`seating::LOBBY_FACTIONS`].
#[derive(Debug, Clone, Serialize)]
pub struct FactionGroup {
    pub name: &'static str,
    pub factions: Vec<FactionOption>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FactionOption {
    pub id: &'static str,
    pub name: String,
}

/// One kind of seat that the host plays itself.
#[derive(Debug, Clone, Serialize)]
pub struct BotKindOption {
    pub id: &'static str,
    pub name: &'static str,
    pub description: String,
}

/// What a host can choose for a table. The page writes none of these names.
#[derive(Debug, Clone, Serialize)]
pub struct SetupOptions {
    pub min_players: usize,
    pub max_players: usize,
    pub strategy_card_sets: Vec<CardSetOption>,
    pub faction_groups: Vec<FactionGroup>,
    pub bot_kinds: Vec<BotKindOption>,
    /// The longest wait of a seat that the host plays, in milliseconds.
    pub max_bot_delay_ms: u64,
    /// The wait of a table that set none: a value of the host.
    pub default_bot_delay_ms: u64,
    /// The games that a table can start from, as the Dev sheet of a local game lists them. A
    /// host that starts none (the public server) empties the list.
    pub scenarios: Vec<Scenario>,
}

/// The longest wait of a seat that the host plays, in milliseconds.
pub const MAX_BOT_DELAY_MS: u64 = 30_000;

/// See [`SetupOptions`]. `host` is who plays a seat that no person has, as the texts of the
/// kinds name it ("The server", "The page"); `default_bot_delay_ms` is the wait of that host.
#[must_use]
pub fn setup_options(host: &str, default_bot_delay_ms: u64) -> SetupOptions {
    let content = ContentStore::embedded();
    let strategy_card_sets = card_set::OFFERED
        .iter()
        .map(|set| {
            let (ids, initiative) =
                ti4_engine::setup::strategy_card_setup_for_set(content, set).unwrap_or_default();
            let mut cards: Vec<CardOption> = ids
                .iter()
                .map(|id| CardOption {
                    id: id.as_str().to_owned(),
                    name: content
                        .get(
                            ti4_model::content_types::ContentType::StrategyCards,
                            id.as_str(),
                        )
                        .and_then(|card| card.text("name"))
                        .unwrap_or(id.as_str())
                        .to_owned(),
                    initiative: initiative.get(id).copied().unwrap_or(99),
                })
                .collect();
            cards.sort_by_key(|card| card.initiative);
            CardSetOption {
                id: set,
                name: card_set::name(set),
                default: *set == card_set::DEFAULT,
                cards,
            }
        })
        .collect();
    let option = |faction: &&'static str| FactionOption {
        id: faction,
        name: ti4_content::factions::get(content, faction)
            .and_then(|record| record.name().map(str::to_owned))
            .unwrap_or_else(|| (*faction).to_owned()),
    };
    let (base, pok) = seating::LOBBY_FACTIONS.split_at(seating::LOBBY_BASE_FACTIONS);
    SetupOptions {
        min_players: *PLAYER_COUNTS.start(),
        max_players: *PLAYER_COUNTS.end(),
        strategy_card_sets,
        faction_groups: vec![
            FactionGroup {
                name: "Base game",
                factions: base.iter().map(option).collect(),
            },
            FactionGroup {
                name: "Prophecy of Kings",
                factions: pok.iter().map(option).collect(),
            },
        ],
        bot_kinds: vec![
            BotKindOption {
                id: bot_kind::RULE,
                name: "Rule bot",
                description: format!(
                    "{host} plays this seat by rules: it builds, moves and scores."
                ),
            },
            BotKindOption {
                id: bot_kind::RANDOM,
                name: "Random",
                description: format!(
                    "{host} plays this seat at random, among what the game allows."
                ),
            },
        ],
        max_bot_delay_ms: MAX_BOT_DELAY_MS,
        default_bot_delay_ms,
        scenarios: scenario::SCENARIOS.to_vec(),
    }
}

/// The board of the table and the places of its seats, with no faction: nobody knows before
/// Start who sits where. A table that starts from a scenario shows the board of that game.
///
/// # Errors
/// A message for an unknown scenario or template, or a map that does not build for the count.
pub fn preview(setup: &TableSetup) -> Result<MapPreview, String> {
    match setup.scenario()? {
        Some(scenario) => catalog::scenario_preview(&setup.map, scenario.id),
        None => catalog::preview(&setup.map, setup.player_count, setup.seed),
    }
}

/// The places of the join in the order of the seats: seat `i` is the player who came as number
/// `seat_places(..)[i]`. `seed` is the seed of the game.
#[must_use]
pub fn seat_places(order: SeatOrder, count: usize, seed: u64) -> Vec<usize> {
    match order {
        SeatOrder::Join => (0..count).collect(),
        SeatOrder::Random => seating::lobby_seat_order(count, seed),
    }
}

/// What Start draws for a table: who sits where, and the faction of every seat.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Drawn {
    /// For each seat, the place of its player in the order of the join.
    pub order: Vec<usize>,
    /// The faction of each seat. Empty for a table that starts from a scenario: the scenario
    /// seats its own factions.
    pub factions: Vec<FactionId>,
}

impl Drawn {
    /// `joined` (the players in the order in which they came) in the order of their seats.
    ///
    /// # Panics
    /// When `joined` has fewer entries than the table has seats.
    #[must_use]
    pub fn seated<T: Clone>(&self, joined: &[T]) -> Vec<T> {
        self.order
            .iter()
            .map(|place| joined[*place].clone())
            .collect()
    }
}

/// Draws the seats and the factions of a full table. `chosen[i]` is the faction that the player
/// who came as number `i` asked for, or `None` for any.
///
/// The seat order comes from the seed first (a stream of its own); the factions are then drawn
/// for that order ([`seating::lobby_faction_draw`]), so a chosen faction stays with its player.
/// A table that starts from a scenario keeps the order of the join: a case of a scenario is at a
/// certain seat.
///
/// # Errors
/// A message when `chosen` is not one entry for each seat, for an unknown scenario, or for an
/// unknown or repeated faction.
pub fn draw(setup: &TableSetup, chosen: &[Option<String>]) -> Result<Drawn, String> {
    if chosen.len() != setup.player_count {
        return Err("one faction entry per seat is required".to_owned());
    }
    if setup.scenario()?.is_some() {
        return Ok(Drawn {
            order: seat_places(SeatOrder::Join, setup.player_count, setup.seed),
            factions: Vec::new(),
        });
    }
    let seed = setup.game_seed();
    let order = seat_places(setup.seat_order, setup.player_count, seed);
    let by_seat: Vec<Option<&str>> = order
        .iter()
        .map(|place| chosen[*place].as_deref())
        .collect();
    let factions = seating::lobby_faction_draw(&by_seat, seed).map_err(|e| e.to_string())?;
    Ok(Drawn { order, factions })
}

/// The game that the table starts, with what [`draw`] drew. `players` are the ids of the seats
/// in their order ([`Drawn::seated`]).
///
/// The speaker is a part of the returned state, so a game that is played again from its stored
/// first state keeps it.
///
/// # Errors
/// A message when the ids are not one for each seat, for an unknown scenario, template or set of
/// cards, or when the map does not build.
pub fn create(
    content: &ContentStore,
    setup: &TableSetup,
    players: &[PlayerId],
    drawn: &Drawn,
) -> Result<(GameState, Galaxy), String> {
    if players.len() != setup.player_count {
        return Err("one player id per seat is required".to_owned());
    }
    if let Some(scenario) = setup.scenario()? {
        return scenario::start(content, scenario.id, players);
    }
    if drawn.factions.len() != players.len() {
        return Err("one faction per seat is required".to_owned());
    }
    let seed = setup.game_seed();
    let assignments: BTreeMap<PlayerId, FactionId> = players
        .iter()
        .cloned()
        .zip(drawn.factions.iter().cloned())
        .collect();
    let (mut state, galaxy) = create_game_with_template_seated(
        content,
        players,
        seed,
        setup.map.stored().as_deref(),
        &assignments,
        Some(&setup.strategy_cards),
    )?;
    if setup.speaker_order == SpeakerOrder::Random
        && let Some(speaker) = ti4_engine::seeded_speaker(players, seed)
    {
        state.speaker = speaker;
    }
    Ok((state, galaxy))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(count: usize) -> Vec<PlayerId> {
        (1..=count)
            .map(|i| PlayerId::new(format!("p{i}")))
            .collect()
    }

    fn started(setup: &TableSetup, chosen: &[Option<String>]) -> (Drawn, GameState) {
        let drawn = draw(setup, chosen).expect("the table is drawn");
        let players = drawn.seated(&ids(setup.player_count));
        let (state, _) =
            create(ContentStore::embedded(), setup, &players, &drawn).expect("the game starts");
        (drawn, state)
    }

    #[test]
    fn seat_order_names_round_trip_and_a_missing_value_is_random() {
        for order in [SeatOrder::Random, SeatOrder::Join] {
            assert_eq!(SeatOrder::parse(order.as_str()), Ok(order));
            assert_eq!(
                serde_json::to_value(order).unwrap(),
                serde_json::Value::from(order.as_str())
            );
        }
        assert!(SeatOrder::parse("host").is_err());
        assert_eq!(SeatOrder::from_stored(None), SeatOrder::Random);
    }

    #[test]
    fn the_same_seed_gives_the_same_table_and_another_seed_another() {
        let nobody = vec![None; 6];
        let setup = TableSetup::new(6, 11);
        let (drawn, state) = started(&setup, &nobody);
        let (again, same) = started(&setup, &nobody);
        assert_eq!(drawn, again);
        assert_eq!(state, same);
        assert!(
            (0..20).any(|seed| started(&TableSetup::new(6, seed), &nobody).0 != drawn),
            "some seed draws another table"
        );
    }

    #[test]
    fn every_seat_has_another_faction_and_a_chosen_faction_stays_with_its_player() {
        for seed in 0..12 {
            let setup = TableSetup::new(6, seed);
            let mut chosen = vec![None; 6];
            chosen[1] = Some("hacan".to_owned());
            chosen[4] = Some("ghost".to_owned());
            let (drawn, state) = started(&setup, &chosen);
            let mut places = drawn.order.clone();
            places.sort_unstable();
            assert_eq!(
                places,
                (0..6).collect::<Vec<_>>(),
                "every player has a seat"
            );
            let factions: std::collections::BTreeSet<&FactionId> = drawn.factions.iter().collect();
            assert_eq!(factions.len(), 6, "seed {seed}: {:?}", drawn.factions);
            for (place, faction) in [(1, "hacan"), (4, "ghost")] {
                let player = &ids(6)[place];
                assert_eq!(
                    state.player(player).expect("seated").faction.as_str(),
                    faction,
                    "seed {seed}"
                );
            }
            // The state has the seats in the drawn order.
            let seated: Vec<&PlayerId> = state.players.iter().map(|seat| &seat.id).collect();
            assert_eq!(
                seated,
                drawn.seated(&ids(6)).iter().collect::<Vec<_>>(),
                "seed {seed}"
            );
        }
    }

    #[test]
    fn join_keeps_the_order_and_the_speaker_follows_its_setting() {
        let nobody = vec![None; 5];
        let mut setup = TableSetup::new(5, 3);
        setup.seat_order = SeatOrder::Join;
        setup.speaker_order = SpeakerOrder::Seat;
        let (drawn, state) = started(&setup, &nobody);
        assert_eq!(drawn.order, vec![0, 1, 2, 3, 4]);
        assert_eq!(state.speaker, ids(5)[0]);

        setup.speaker_order = SpeakerOrder::Random;
        let speakers: std::collections::BTreeSet<PlayerId> = (0..20)
            .map(|seed| {
                setup.seed = seed;
                let (_, state) = started(&setup, &nobody);
                assert_eq!(
                    Some(state.speaker.clone()),
                    ti4_engine::seeded_speaker(&ids(5), seed)
                );
                state.speaker
            })
            .collect();
        assert!(speakers.len() > 1, "the speaker is drawn");
    }

    #[test]
    fn a_table_is_the_game_of_the_functions_that_it_is_made_of() {
        // What the server started before this module was there: the factions of the seed for the
        // drawn order, on the template, with the cards of the set.
        let mut setup = TableSetup::new(4, 9);
        setup.strategy_cards = card_set::POK.to_owned();
        let chosen = vec![None, Some("sol".to_owned()), None, None];
        let (drawn, state) = started(&setup, &chosen);

        let players = drawn.seated(&ids(4));
        let by_seat: Vec<Option<String>> = drawn.seated(&chosen);
        let factions = ti4_view::map::lobby_seat_factions(&by_seat, 9).unwrap();
        let assignments: BTreeMap<PlayerId, FactionId> =
            players.iter().cloned().zip(factions).collect();
        let (mut expected, _) = create_game_with_template_seated(
            ContentStore::embedded(),
            &players,
            9,
            catalog::recommended_for(4).as_deref(),
            &assignments,
            Some("pok"),
        )
        .unwrap();
        expected.speaker = ti4_engine::seeded_speaker(&players, 9).unwrap();
        // Every field, also those that the equality of a state leaves out.
        assert_eq!(
            serde_json::to_value(&state).unwrap(),
            serde_json::to_value(&expected).unwrap()
        );
    }

    #[test]
    fn a_scenario_is_its_own_game_whatever_the_table_says() {
        let late = scenario::find(scenario::LATE_GAME).unwrap();
        let mut setup = TableSetup::new(late.players, 77);
        setup.start_from = Some(late.id.to_owned());
        setup.check().unwrap();
        assert_eq!(setup.game_seed(), late.seed);
        let chosen = vec![Some("hacan".to_owned()); late.players];
        let (drawn, state) = started(&setup, &chosen);
        assert_eq!(drawn.order, (0..late.players).collect::<Vec<_>>());
        let (expected, _) =
            scenario::start(ContentStore::embedded(), late.id, &ids(late.players)).unwrap();
        assert_eq!(state, expected);
        assert_eq!(
            preview(&setup).unwrap(),
            catalog::scenario_preview(&setup.map, late.id).unwrap()
        );

        setup.player_count = 3;
        assert!(setup.check().is_err());
        setup.start_from = Some("nope".to_owned());
        assert!(setup.check().is_err());
        assert!(draw(&setup, &[None, None, None]).is_err());
    }

    #[test]
    fn the_values_of_a_client_are_checked() {
        let table = TableSetup::new(4, 1);
        table.check().unwrap();
        let wrong = [
            TableSetup {
                player_count: 9,
                ..table.clone()
            },
            TableSetup {
                strategy_cards: "base_game".to_owned(),
                ..table.clone()
            },
            TableSetup {
                map: MapChoice::Template {
                    alias: "nope".to_owned(),
                },
                ..table.clone()
            },
        ];
        for setup in wrong {
            assert!(setup.check().is_err(), "{setup:?}");
        }
        assert!(draw(&table, &[None, None]).is_err());
        assert!(
            draw(
                &table,
                &[Some("sol".to_owned()), Some("sol".to_owned()), None, None]
            )
            .is_err()
        );
        // A table of a client names as little as its seats and its seed.
        let short: TableSetup = serde_json::from_str(r#"{"player_count":4,"seed":1}"#).unwrap();
        assert_eq!(short.map, MapChoice::Random);
        assert_eq!(short.strategy_cards, card_set::DEFAULT);
    }

    #[test]
    fn the_preview_is_the_board_of_the_seed_with_a_place_for_each_seat() {
        let setup = TableSetup::new(6, 3);
        let pictured = preview(&setup).unwrap();
        assert_eq!(pictured.seats.len(), 6);
        assert_eq!(pictured, catalog::preview(&setup.map, 6, 3).unwrap());
    }

    #[test]
    fn the_setup_options_name_every_set_faction_and_kind() {
        let options = setup_options("The page", 0);
        assert_eq!(options.strategy_card_sets.len(), 3);
        assert!(
            options
                .strategy_card_sets
                .iter()
                .all(|set| set.cards.len() == 8)
        );
        assert_eq!(
            options
                .strategy_card_sets
                .iter()
                .filter(|set| set.default)
                .count(),
            1
        );
        let factions: usize = options
            .faction_groups
            .iter()
            .map(|group| group.factions.len())
            .sum();
        assert_eq!(factions, seating::LOBBY_FACTIONS.len());
        assert!(
            options
                .faction_groups
                .iter()
                .flat_map(|group| &group.factions)
                .all(|faction| faction.name != faction.id)
        );
        assert!(
            options
                .bot_kinds
                .iter()
                .all(|kind| kind.description.starts_with("The page plays"))
        );
        assert!(!options.scenarios.is_empty());
    }
}
