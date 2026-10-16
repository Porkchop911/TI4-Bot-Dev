//! The setup of a game, for both hosts: what a host can choose for a table, what the table looks
//! like before it starts, and the game that it starts.
//!
//! The server calls this for the tables of its lobbies and the wasm hosts for the table of the
//! page, so a table of a seed is the same game on all of them. Nothing here knows a lobby record, a
//! credential or a socket: a host keeps its table as it likes and hands over a [`TableSetup`].

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use ti4_content::ContentStore;
use ti4_content::galaxy::Galaxy;
use ti4_engine::seating;
use ti4_model::id::{FactionId, PlayerId};
use ti4_model::state::GameState;

use crate::map::create_game_with_template_seated;
use crate::preset;
use crate::view::scenario::{self, Scenario};

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
/// [`crate::view`].
pub use crate::view::seats::{HUMAN_KIND, SeatFact, TableSeats};

/// Who opens a new game as speaker (and so picks the first strategy card).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum SpeakerOrder {
    /// A seat drawn from the game seed ([`ti4_engine::seeded_speaker`]): the same seed always
    /// opens with the same speaker. What a table gets unless it asks otherwise.
    #[default]
    Random,
    /// The first seat. Fixtures and tests pin their games to this.
    Seat,
}

impl SpeakerOrder {
    /// The stored/requested name: `"random"` or `"seat"`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Random => "random",
            Self::Seat => "seat",
        }
    }

    /// Parse a requested name.
    ///
    /// # Errors
    /// A message naming the accepted values.
    pub fn parse(name: &str) -> Result<Self, String> {
        match name {
            "random" => Ok(Self::Random),
            "seat" => Ok(Self::Seat),
            other => Err(format!(
                "unknown speaker_order {other:?} (expected \"random\" or \"seat\")"
            )),
        }
    }

    /// A stored value; `None` (a record written before the option) and anything unknown are
    /// [`SpeakerOrder::Random`].
    #[must_use]
    pub fn from_stored(name: Option<&str>) -> Self {
        name.and_then(|n| Self::parse(n).ok()).unwrap_or_default()
    }
}

fn default_card_set() -> String {
    card_set::DEFAULT.to_owned()
}

/// What a host chose for a table. The players and their factions are not a part of it: they are
/// given to [`draw`]. Both hosts start a table from it, so a table of a seed is the same game in
/// the page and on the server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct TableSetup {
    pub player_count: usize,
    /// The map of the table; a table that names none plays the seeded random board.
    #[serde(default = "random_map")]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub map: MapChoice,
    /// One of the sets of the content (a table offers [`card_set::OFFERED`]).
    #[serde(default = "default_card_set")]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub strategy_cards: String,
    /// Left out: `random`, drawn from the seed. `seat` is the first seat.
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub speaker_order: SpeakerOrder,
    /// The id of a scenario of [`crate::view::scenario`]: the table starts that game, with its seed,
    /// its seats, its map, its cards and its factions. `None` is round 1.
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub start_from: Option<String>,
    /// A start preset of [`crate::preset`] (a test table): it seats its own factions and sets its
    /// cases on the opening state. A scenario wins over it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset: Option<String>,
    /// The seed of the table. It tells the factions that nobody chose, so a host
    /// with several people keeps it to itself.
    pub seed: u64,
}

const fn random_map() -> MapChoice {
    MapChoice::Random
}

impl TableSetup {
    /// A table of `player_count` seats on the map that such a table starts with, with the usual
    /// cards.
    #[must_use]
    pub fn new(player_count: usize, seed: u64) -> Self {
        Self {
            player_count,
            map: MapChoice::from_stored(catalog::recommended_for(player_count).as_deref()),
            strategy_cards: default_card_set(),
            speaker_order: SpeakerOrder::default(),
            start_from: None,
            preset: None,
            seed,
        }
    }

    /// The start preset that the table plays: none for a table that starts from a scenario.
    #[must_use]
    pub fn start_preset(&self) -> Option<&str> {
        self.preset.as_deref().filter(|_| self.start_from.is_none())
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

    /// The seed of the game that the table starts: a scenario of a fixed board is the game of its
    /// own seed ([`Scenario::game_seed`]).
    #[must_use]
    pub fn game_seed(&self) -> u64 {
        self.scenario()
            .ok()
            .flatten()
            .map_or(self.seed, |scenario| scenario.game_seed(self.seed))
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
        if let Some(name) = self.start_preset()
            && !preset::is_known(name)
        {
            return Err(format!("unknown start_preset {name:?}"));
        }
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
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct CardSetOption {
    pub id: &'static str,
    pub name: &'static str,
    /// The table plays this set when it names none.
    pub default: bool,
    /// The eight cards, by initiative.
    pub cards: Vec<CardOption>,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct CardOption {
    pub id: String,
    pub name: String,
    pub initiative: i32,
}

/// The factions of one product, in the order of [`seating::LOBBY_FACTIONS`].
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct FactionGroup {
    pub name: &'static str,
    pub factions: Vec<FactionOption>,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct FactionOption {
    pub id: &'static str,
    pub name: String,
}

/// One kind of seat that the host plays itself.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct BotKindOption {
    /// The kind as a table names it: `smart`, `random`, or `mlp:<name>` for a model
    /// (`ti4_host::bots`).
    #[cfg_attr(feature = "ts", ts(type = "\"smart\" | \"random\" | `mlp:${string}`"))]
    pub id: String,
    /// The name of the kind; the label of a model.
    pub name: String,
    pub description: String,
}

/// What a host can choose for a table. The page writes none of these names.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
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
/// Each of `models` is the kind `mlp:<name>`, after the rule bot and the random seat.
#[must_use]
pub fn setup_options(
    host: &str,
    default_bot_delay_ms: u64,
    models: &[crate::bots::ModelEntry],
) -> SetupOptions {
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
        bot_kinds: [
            BotKindOption {
                id: bot_kind::RULE.to_owned(),
                name: "Rule bot".to_owned(),
                description: format!(
                    "{host} plays this seat by rules: it builds, moves and scores."
                ),
            },
            BotKindOption {
                id: bot_kind::RANDOM.to_owned(),
                name: "Random".to_owned(),
                description: format!(
                    "{host} plays this seat at random, among what the game allows."
                ),
            },
        ]
        .into_iter()
        .chain(models.iter().map(|model| BotKindOption {
            id: format!("{}{}", crate::bots::MLP_PREFIX, model.name),
            name: model.label.clone(),
            description: format!(
                "{host} plays this seat with the trained model {}.",
                model.label
            ),
        }))
        .collect(),
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
        Some(scenario) => catalog::scenario_preview(&setup.map, scenario.id, setup.game_seed()),
        None => catalog::preview(&setup.map, setup.player_count, setup.seed),
    }
}

/// What Start draws for a table: the faction of every seat. The players sit in the order of the
/// table (the rows of the lobby), so nothing else is drawn.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Drawn {
    /// The faction of each seat. Empty for a table that starts from a scenario: the scenario
    /// seats its own factions.
    pub factions: Vec<FactionId>,
}

/// Draws the factions of a full table. `chosen[i]` is the faction that the player of seat `i`
/// asked for, or `None` for any: a chosen faction stays with its player, and the others are drawn
/// from the seed ([`seating::lobby_faction_draw`]). The seats are the order of the table, as its
/// host shows and arranges it. A table that starts from a scenario seats the factions of the
/// scenario, and a table with a start preset the factions of the preset (the choices of the
/// players do not apply to it).
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
            factions: Vec::new(),
        });
    }
    let seed = setup.game_seed();
    if let Some(name) = setup.start_preset() {
        return Ok(Drawn {
            factions: preset_factions(name, setup.player_count, seed),
        });
    }
    let by_seat: Vec<Option<&str>> = chosen.iter().map(Option::as_deref).collect();
    let factions = seating::lobby_faction_draw(&by_seat, seed).map_err(|e| e.to_string())?;
    Ok(Drawn { factions })
}

/// The factions of the seats of a table with the start preset `name`: those it names (`+fac`,
/// `world`), else the in-scope order, rotated as the preset says (`+rot`, `leaders`), and then the
/// faction that the preset needs on a seat of its own ([`preset::seat_roster`]).
fn preset_factions(name: &str, count: usize, seed: u64) -> Vec<FactionId> {
    // The preset seats by the place of a seat, not by its id.
    let seats: Vec<PlayerId> = (0..count)
        .map(|seat| PlayerId::new(format!("seat_{seat:02}")))
        .collect();
    let mut assignments = preset::roster(name, seed, &seats)
        .unwrap_or_else(|| seating::seat_in_scope_rotated(&seats, preset::rotation(name, seed)));
    preset::seat_roster(name, seed, &seats, &mut assignments);
    seats.iter().map(|seat| assignments[seat].clone()).collect()
}

/// The game that the table starts, with what [`draw`] drew. `players` are the ids of the seats
/// in the order of the table.
///
/// The speaker and the cases of a start preset are a part of the returned state, so a game that is
/// played again from its stored first state keeps them.
///
/// # Errors
/// A message when the ids are not one for each seat, for an unknown scenario, template, preset or
/// set of cards, or when the map does not build or the preset cannot be placed.
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
        return scenario::start(content, scenario.id, players, setup.game_seed());
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
    if let Some(name) = setup.start_preset() {
        preset::apply(content, &mut state, &galaxy, players, seed, name)?;
    }
    Ok((state, galaxy))
}

/// The game of a test table: `players` in their order, the first seat opens, the Prophecy of Kings
/// cards, with the start preset `preset`.
#[cfg(test)]
pub(crate) fn preset_game(
    content: &ContentStore,
    players: &[PlayerId],
    seed: u64,
    template: Option<&str>,
    preset: &str,
) -> Result<(GameState, Galaxy), String> {
    let setup = TableSetup {
        map: MapChoice::from_stored(template),
        strategy_cards: card_set::POK.to_owned(),
        speaker_order: SpeakerOrder::Seat,
        preset: Some(preset.to_owned()),
        ..TableSetup::new(players.len(), seed)
    };
    let drawn = draw(&setup, &vec![None; players.len()])?;
    create(content, &setup, players, &drawn)
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
        let players = ids(setup.player_count);
        let (state, _) =
            create(ContentStore::embedded(), setup, &players, &drawn).expect("the game starts");
        (drawn, state)
    }

    #[test]
    fn speaker_order_names_round_trip() {
        for order in [SpeakerOrder::Random, SpeakerOrder::Seat] {
            assert_eq!(SpeakerOrder::parse(order.as_str()), Ok(order));
        }
        assert!(SpeakerOrder::parse("first").is_err());
        assert_eq!(SpeakerOrder::from_stored(None), SpeakerOrder::Random);
    }

    #[test]
    fn a_random_speaker_follows_the_seed_and_changes_nothing_else() {
        for n in [3, 4, 5, 6] {
            let nobody = vec![None; n];
            let mut speakers = std::collections::BTreeSet::new();
            for seed in 0..24_u64 {
                let mut setup = TableSetup::new(n, seed);
                setup.speaker_order = SpeakerOrder::Seat;
                let (_, seat) = started(&setup, &nobody);
                let players = ids(n);
                assert_eq!(
                    seat.speaker, players[0],
                    "seat order opens with the first seat"
                );
                setup.speaker_order = SpeakerOrder::Random;
                let (_, random) = started(&setup, &nobody);
                assert_eq!(
                    Some(random.speaker.clone()),
                    ti4_engine::seeded_speaker(&players, seed)
                );
                // Seats, factions, board and decks are untouched.
                let mut reseated = random.clone();
                reseated.speaker = seat.speaker.clone();
                assert_eq!(reseated, seat, "{n}p seed {seed}");
                speakers.insert(random.speaker);
            }
            assert!(speakers.len() > 1, "{n}p: the speaker varies across seeds");
        }
        // A preset game draws the same speaker.
        let mut setup = TableSetup::new(4, 5);
        setup.preset = Some(preset::COMBAT.to_owned());
        let (_, state) = started(&setup, &[None, None, None, None]);
        assert_eq!(Some(state.speaker), ti4_engine::seeded_speaker(&ids(4), 5));
    }

    #[test]
    fn a_preset_table_seats_the_factions_of_the_preset() {
        let mut setup = TableSetup::new(4, 9);
        setup.preset = Some("combat+fac:naalu:mentak".to_owned());
        setup.check().unwrap();
        let chosen = vec![Some("sol".to_owned()), None, None, None];
        let (drawn, state) = started(&setup, &chosen);
        assert_eq!(drawn.factions[0].as_str(), "naalu");
        assert_eq!(drawn.factions[1].as_str(), "mentak");
        assert_eq!(state.player(&ids(4)[0]).unwrap().faction.as_str(), "naalu");

        setup.preset = Some("nope".to_owned());
        assert!(setup.check().is_err());
        // A scenario wins over a preset.
        let late = scenario::find(scenario::LATE_GAME).unwrap();
        let mut setup = TableSetup::new(late.players, 77);
        setup.start_from = Some(late.id.to_owned());
        setup.preset = Some("nope".to_owned());
        setup.check().unwrap();
        assert_eq!(setup.start_preset(), None);
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
            // The state has the seats in the order of the table.
            let seated: Vec<&PlayerId> = state.players.iter().map(|seat| &seat.id).collect();
            assert_eq!(seated, ids(6).iter().collect::<Vec<_>>(), "seed {seed}");
        }
    }

    #[test]
    fn the_seats_keep_the_order_of_the_table_and_the_speaker_follows_its_setting() {
        let nobody = vec![None; 5];
        let mut setup = TableSetup::new(5, 3);
        setup.speaker_order = SpeakerOrder::Seat;
        let (_, state) = started(&setup, &nobody);
        let seated: Vec<&PlayerId> = state.players.iter().map(|seat| &seat.id).collect();
        assert_eq!(seated, ids(5).iter().collect::<Vec<_>>());
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
        // The factions of the seed for the order of the table, on the template, with the cards of
        // the set.
        let mut setup = TableSetup::new(4, 9);
        setup.strategy_cards = card_set::POK.to_owned();
        let chosen = vec![None, Some("sol".to_owned()), None, None];
        let (_, state) = started(&setup, &chosen);

        let players = ids(4);
        let by_seat: Vec<Option<&str>> = chosen.iter().map(Option::as_deref).collect();
        let factions = seating::lobby_faction_draw(&by_seat, 9).unwrap();
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
        assert!(drawn.factions.is_empty(), "the scenario seats its factions");
        let (expected, _) =
            scenario::start(ContentStore::embedded(), late.id, &ids(late.players), 77).unwrap();
        assert_eq!(state, expected);
        assert_eq!(
            preview(&setup).unwrap(),
            catalog::scenario_preview(&setup.map, late.id, 77).unwrap()
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
        let model = crate::bots::ModelEntry {
            name: "stage2".to_owned(),
            label: "Stage 2".to_owned(),
        };
        let options = setup_options("The page", 0, std::slice::from_ref(&model));
        let kinds: Vec<(&str, &str)> = options
            .bot_kinds
            .iter()
            .map(|kind| (kind.id.as_str(), kind.name.as_str()))
            .collect();
        assert_eq!(
            kinds,
            [
                ("smart", "Rule bot"),
                ("random", "Random"),
                ("mlp:stage2", "Stage 2")
            ]
        );
        assert_eq!(setup_options("The page", 0, &[]).bot_kinds.len(), 2);
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
