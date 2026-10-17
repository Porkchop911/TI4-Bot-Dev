//! How a hosted game is set up: the game a page asks for, the game it makes, and what the setup
//! form of `web/` may offer (card sets, factions, maps, presets).
//!
//! A page asks for one of three games ([`Start`]):
//!
//! - [`Setup`], the form of `web/`: it becomes a [`TableSetup`] and the faction the person chose.
//! - [`TableRequest`], the table of web2's single player: what the host chose, and its players.
//! - [`Local`], a game of a fixed setup (web2's `?local=` and `?scenario=`, for tests and tools).
//!
//! The first two are made by `ti4_host::setup` (`draw`, `create`), as for a lobby game of the
//! server, so a seed gives the same board and the same opening state in all.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use ti4_content::ContentStore;
use ti4_content::galaxy::Galaxy;
use ti4_engine::game::Game;
use ti4_engine::seating;
use ti4_host::maps::catalog::{self, MapChoice};
use ti4_host::setup::{self, SpeakerOrder, TableSeats, TableSetup, bot_kind, card_set};
use ti4_model::content_types::POK;
use ti4_model::id::PlayerId;
use ti4_model::state::ReactionMode;

use crate::SEATS;
use crate::host_session::SeatSwitch;

fn local_game_id() -> String {
    "local".to_owned()
}

/// Who plays the seats of the others: `smart` (the rule bot, also `rule`, its name in the form
/// of `web/`), `random`, or `mlp:<name>` (`ti4_host::bots`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Bots(
    #[cfg_attr(feature = "ts", ts(type = "\"smart\" | \"random\" | `mlp:${string}`"))] pub String,
);

impl Bots {
    /// The kind of the seats, as a table names it.
    fn kind(&self) -> &str {
        if self.0 == "rule" {
            bot_kind::RULE
        } else {
            &self.0
        }
    }
}

/// The table of a game. Everything the game's opening state is made from; a save carries it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Setup {
    #[serde(default = "local_game_id")]
    pub game_id: String,
    pub seed: u64,
    pub players: usize,
    /// 0-based place of the person. Left out: drawn from the seed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub human_seat: Option<usize>,
    /// A faction id of `seating::LOBBY_FACTIONS`. Left out: drawn.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub human_faction: Option<String>,
    /// One of `card_set::OFFERED`. Left out: `card_set::DEFAULT`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub card_set: Option<String>,
    /// A template alias, `"random"` for the seeded random board. Left out: the template that is
    /// recommended for the number of players.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub map: Option<String>,
    /// A start preset of `ti4_host::preset` (a test table). Not offered by the form.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset: Option<String>,
    /// `"seat"` (the first seat opens) or `"random"`. Left out: random.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speaker: Option<SpeakerOrder>,
    /// A prepared game of `ti4_host::view::scenario` (its players win, and its seed unless it
    /// plays on any seed).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scenario: Option<String>,
    /// Who plays the seats of the others. Left out: the rule bot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bots: Option<Bots>,
    /// The rule bot's behaviour ([`ti4_rule_bot::tuning::for_version`]). A resumed save
    /// recomputes the rule seats' answers, so it must play the bot it was saved with. Left out:
    /// a new game plays [`ti4_rule_bot::tuning::BOT_VERSION`]; a save made before this field
    /// existed played version 1 ([`Start::settle_bot_version`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bot_version: Option<u32>,
    /// Whether the seats that choose their starting technology are asked
    /// ([`ti4_host::setup::TableSetup::starting_tech`]). Left out: on for a game that plays a
    /// bot of version 3 or later ([`Start::settle_bot_version`]), so a save made before the choice
    /// (bot version 2 or less) resumes without it. Not written while off, like the table's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub starting_tech: Option<bool>,
}

/// The format of the save of [`crate::host_session::HostSession::save_state`]. A save of another
/// format is not read.
pub const SAVE_FORMAT: u64 = 2;

/// What a save (or the `restart` of a history change) brings to a new run of the same game.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Resume {
    /// The game the save plays. A request that names no game of its own plays this one.
    #[serde(default)]
    pub game: Option<GameRequest>,
    /// The person's answers to give again: a `restart` of a change of the history, which has no
    /// `log`. A save has them too, and is played from its `decisions`.
    #[serde(default)]
    pub answers: Vec<String>,
    /// The answers after them that a redo can bring back.
    #[serde(default)]
    pub redo: Vec<String>,
    /// The cards the person is never offered (applied when the replay ends).
    #[serde(default)]
    pub never: Vec<String>,
    /// The time of day of each event, so a game that is played again keeps the times it had.
    #[serde(default)]
    pub event_times: Vec<String>,
    /// The version each event was made at.
    #[serde(default)]
    pub event_versions: Vec<Option<u64>>,
    /// The version of the game when it was saved: the next one follows it.
    #[serde(default)]
    pub version: u64,
    #[serde(default)]
    pub generation: u64,
    /// The hash of the saved decisions: checked when the replay ends.
    #[serde(default)]
    pub decisions_hash: Option<String>,
    /// Every decision of the saved game, of every seat, in order (`decisions` of a save, `log`
    /// of a `restart`). With it the game is given each of them again, and no seat decides again
    /// (`answers` is then not read); without it the person's `answers` are given, and the other
    /// seats decide again from the seed.
    #[serde(default, alias = "decisions")]
    pub log: Vec<String>,
    /// The changes of the seats that the page plays, in order.
    #[serde(default)]
    pub switches: Vec<SeatSwitch>,
    /// The steps that the undo before this run took back (`Restart::undone`).
    #[serde(default)]
    pub undone: Vec<ti4_host::planning::steps::PlanStep>,
    /// The rule bot version the game was played with (a save and a restart carry it besides
    /// the setup's own; see [`Start::settle_bot_version`]).
    #[serde(default)]
    pub bot_version: Option<u32>,
    /// The rules version the game was played with ([`ti4_model::state::GameState::rules_version`]).
    /// A save made before the field existed has none: version 0, the rules it was recorded with.
    /// Only a start without a resume (a new game) plays the current version.
    #[serde(default)]
    pub rules_version: Option<u32>,
    /// What each seat has staged and not sent (`ti4_host::staged`).
    #[serde(default)]
    pub staged: std::collections::BTreeMap<ti4_model::id::PlayerId, ti4_host::staged::SeatStaged>,
}

/// The game of a request or of a save: one of `setup`, `table` and `local`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct GameRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub setup: Option<Setup>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub table: Option<TableRequest>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local: Option<Local>,
}

impl GameRequest {
    /// The game this request asks for.
    ///
    /// # Errors
    /// A message when the request names no game or two, or the game is not one the host offers.
    pub fn build(&self) -> Result<Built, String> {
        match (&self.setup, &self.table, &self.local) {
            (Some(setup), None, None) => setup.build(),
            (None, Some(table), None) => table.build(),
            (None, None, Some(local)) => local.build(),
            _ => Err("start: give one of setup, table and local".to_owned()),
        }
    }
}

/// The request of `ti4_host_start`: one of `setup`, `table` and `local` (or none, and a save
/// that names its game), and what a save brings.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Start {
    #[serde(default)]
    pub setup: Option<Setup>,
    #[serde(default)]
    pub table: Option<TableRequest>,
    #[serde(default)]
    pub local: Option<Local>,
    #[serde(default)]
    pub resume: Option<Resume>,
    /// The mode of each reaction card of the seats that the page plays, from the first
    /// decision on (`resume.never` of a save of `web/` is added to it).
    #[serde(default)]
    pub modes: BTreeMap<String, ReactionMode>,
    /// Whether the game waits for the page before a decision of a seat that it does not play:
    /// 0 (no), 1 (before each) or 2 (before each that takes the wait of a table). See the local
    /// command `stepping`.
    #[serde(default)]
    pub stepping: u32,
    /// The page takes part in the staged work of its seats: it is sent `seat_staged` and may
    /// send `put_staged`, as a connection of the server that subscribed with `staged`.
    #[serde(default)]
    pub staged: bool,
}

impl Start {
    /// The game this request asks for.
    ///
    /// # Errors
    /// A message when the request names no game or two, or the game is not one the host offers.
    pub fn build(&self) -> Result<Built, String> {
        let saved = self.resume.as_ref().and_then(|resume| resume.game.as_ref());
        match (&self.setup, &self.table, &self.local, saved) {
            (None, None, None, Some(game)) => game.build(),
            _ => GameRequest {
                setup: self.setup.clone(),
                table: self.table.clone(),
                local: self.local.clone(),
            }
            .build(),
        }
    }
}

/// Who plays a seat of a table ([`TableRequest`]), by the names of a lobby: `human` (the page),
/// or a kind of `ti4_host::bots`: `smart`, `random` or `mlp:<name>`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct SeatKind(
    #[cfg_attr(
        feature = "ts",
        ts(type = "\"human\" | \"smart\" | \"random\" | `mlp:${string}`")
    )]
    pub String,
);

impl Default for SeatKind {
    fn default() -> Self {
        Self(setup::HUMAN_KIND.to_owned())
    }
}

impl SeatKind {
    /// The kind as a fact of the update names it (`ti4_host::setup::SeatFact::kind`).
    fn as_str(&self) -> &str {
        &self.0
    }

    /// Whether the page answers the seat.
    fn is_human(&self) -> bool {
        self.0 == setup::HUMAN_KIND
    }

    /// Whether this is a kind of seat: `human`, `smart`, `random` or `mlp:<name>`.
    fn is_known(&self) -> bool {
        self.is_human()
            || self.0 == bot_kind::RULE
            || self.0 == bot_kind::RANDOM
            || ti4_host::bots::model_name(&self.0).is_some()
    }
}

/// One player of a table: who plays the seat, and the faction that was chosen for it.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct SeatRequest {
    /// `human` is played by the page, `smart` by the rule bot, `random` at random, `mlp:<name>`
    /// by that model.
    #[serde(default)]
    pub kind: SeatKind,
    /// The faction that was chosen for the player. Left out: drawn from the seed.
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub faction: Option<String>,
    /// The name of the player at the table. With a name for every player, the updates of the
    /// game say who sits in each seat.
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub name: Option<String>,
}

/// The table of web2's single player: what the host chose (`ti4_host::setup::TableSetup`), and
/// its players in the order of their seats. The player of seat `i` has the id of [`SEATS`]`[i]`:
/// the masks of this host name a player by that letter.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct TableRequest {
    pub setup: TableSetup,
    /// Missing entries are seats of the page with no chosen faction.
    #[serde(default)]
    pub seats: Vec<SeatRequest>,
}

impl TableRequest {
    /// The table with its values checked, and a player for every seat.
    ///
    /// # Errors
    /// A message when the table is not one the host offers.
    pub fn checked(&self) -> Result<Self, String> {
        let mut table = self.clone();
        table.setup.check()?;
        let count = table.setup.player_count;
        if count > SEATS.len() || table.seats.len() > count {
            return Err(format!(
                "a table of {count} seats has {} players",
                table.seats.len()
            ));
        }
        table.seats.resize_with(count, SeatRequest::default);
        if let Some(seat) = table.seats.iter().find(|seat| !seat.kind.is_known()) {
            return Err(format!("no kind of seat '{}'", seat.kind.as_str()));
        }
        Ok(table)
    }

    /// The ids of the players in the order of their seats.
    #[must_use]
    pub fn joined(&self) -> Vec<PlayerId> {
        SEATS[..self.setup.player_count]
            .iter()
            .map(|name| PlayerId::new(*name))
            .collect()
    }

    /// What the start draws: the faction of every seat.
    ///
    /// # Errors
    /// A message when a chosen faction is not offered.
    pub fn drawn(&self) -> Result<setup::Drawn, String> {
        let chosen: Vec<Option<String>> =
            self.seats.iter().map(|seat| seat.faction.clone()).collect();
        setup::draw(&self.setup, &chosen)
    }

    /// Who sits in each seat, by the id of its player. `None` when a player has no name.
    #[must_use]
    pub fn seats(&self) -> Option<TableSeats> {
        self.joined()
            .into_iter()
            .zip(&self.seats)
            .map(|(player, seat)| {
                Some((
                    player,
                    setup::SeatFact {
                        nickname: seat.name.clone()?,
                        kind: seat.kind.as_str().to_owned(),
                    },
                ))
            })
            .collect()
    }

    /// The players that the page plays as a mask (bit 0 is the player who came first).
    #[must_use]
    pub fn humans(&self) -> u32 {
        self.seats
            .iter()
            .enumerate()
            .filter(|(_, seat)| seat.kind.is_human())
            .fold(0, |mask, (place, _)| mask | (1 << place))
    }

    /// The kind of the bot of each seat; none for a seat of the page.
    #[must_use]
    pub fn bots(&self) -> Vec<Option<String>> {
        self.seats
            .iter()
            .map(|seat| (!seat.kind.is_human()).then(|| seat.kind.0.clone()))
            .collect()
    }

    /// The game that the table starts, as the server starts it for the table of a lobby: the
    /// page answers its players, the rule bot and the random seats decide as on the server, so
    /// a table of a seed with the same kinds is the same game on both hosts.
    ///
    /// # Errors
    /// A message when the table is not one the host offers.
    pub fn build(&self) -> Result<Built, String> {
        let table = self.checked()?;
        let content = ContentStore::embedded();
        let drawn = table.drawn()?;
        let players = table.joined();
        let (state, galaxy) = setup::create(content, &table.setup, &players, &drawn)?;
        let opening = table
            .setup
            .scenario()?
            .and_then(|scenario| ti4_host::view::scenario::opening(scenario, &state, &galaxy));
        let seed = table.setup.game_seed();
        let game = Game::with_seeded_random(state, content, seed)
            .with_sources(POK)
            .with_galaxy(galaxy.clone());
        Ok(Built {
            game,
            galaxy: Some(galaxy),
            seed,
            players: table.setup.player_count,
            humans: table.humans(),
            bots: table.bots(),
            table: true,
            seats: table.seats(),
            game_id: local_game_id(),
            setup: Some(table.setup.clone()),
            scenario: table.setup.start_from.clone(),
            request: GameRequest {
                table: Some(table),
                ..GameRequest::default()
            },
            opening,
        })
    }
}

/// A game of a fixed setup: a seeded game on the recommended map for its count, or a scenario
/// of `ti4_host::view::scenario` (its seed and its count win). Seat `i` has the id
/// [`SEATS`]`[i]`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Local {
    #[serde(default)]
    pub seed: u64,
    #[serde(default = "eight")]
    pub players: usize,
    /// The seats that the page plays (bit 0 is seat `a`).
    pub humans: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scenario: Option<String>,
    /// The kind of the bot of each seat where the page does not play it (`smart`, `mlp:<name>`):
    /// it decides from the seed, the seat and the number of the decision, as on the server. A
    /// seat with none (or `random`, or past the end) decides at random.
    #[serde(default)]
    #[cfg_attr(
        feature = "ts",
        ts(type = "(\"smart\" | \"random\" | `mlp:${string}` | null)[]")
    )]
    pub bots: Vec<Option<String>>,
    /// The mask of the rule bot's seats (bit 0 is seat `a`) of a save before `bots`: those seats
    /// are `smart`.
    #[serde(default, skip_serializing)]
    #[cfg_attr(feature = "ts", ts(skip))]
    pub rule_seats: u32,
}

const fn eight() -> usize {
    8
}

impl Local {
    /// The game of this setup.
    ///
    /// # Errors
    /// A message when the scenario is not known, or the count is not one of a game.
    pub fn build(&self) -> Result<Built, String> {
        let scenario = match &self.scenario {
            Some(id) => Some(ti4_host::view::scenario::find(id).ok_or_else(|| {
                let known: Vec<&str> = ti4_host::view::scenario::SCENARIOS
                    .iter()
                    .map(|scenario| scenario.id)
                    .collect();
                format!("no scenario '{id}': the scenarios are {}", known.join(", "))
            })?),
            None => None,
        };
        let (seed, players) = scenario.map_or((self.seed, self.players), |scenario| {
            (scenario.seed, scenario.players)
        });
        let game = crate::new_game(
            ContentStore::embedded(),
            seed,
            players,
            self.scenario.as_deref(),
        )
        .map_err(|error| error.to_string())?;
        let opening = scenario.zip(game.galaxy()).and_then(|(scenario, galaxy)| {
            ti4_host::view::scenario::opening(scenario, &game.state, galaxy)
        });
        let all = (1_u32 << players) - 1;
        let bots = (0..players)
            .map(|seat| {
                self.bots
                    .get(seat)
                    .cloned()
                    .flatten()
                    .or_else(|| {
                        (self.rule_seats >> seat & 1 == 1).then(|| bot_kind::RULE.to_owned())
                    })
                    .filter(|kind| kind != bot_kind::RANDOM)
            })
            .collect();
        Ok(Built {
            galaxy: game.galaxy().cloned(),
            game,
            seed,
            players,
            humans: self.humans & all,
            bots,
            table: false,
            seats: None,
            game_id: local_game_id(),
            setup: None,
            scenario: self.scenario.clone(),
            request: GameRequest {
                local: Some(self.clone()),
                ..GameRequest::default()
            },
            opening,
        })
    }
}

/// The first rule bot version of the games that ask for a starting technology (a new game plays
/// the current version; a save records the one it was made with).
pub const STARTING_TECH_FROM_BOT_VERSION: u32 = 3;

/// Whether this build plays the rule bot of `version`.
fn check_bot_version(version: u32) -> Result<(), String> {
    if (1..=ti4_rule_bot::tuning::MAX_BOT_VERSION).contains(&version) {
        Ok(())
    } else {
        Err(format!(
            "bot version {version}: this build plays 1 to {}",
            ti4_rule_bot::tuning::MAX_BOT_VERSION
        ))
    }
}

impl Start {
    /// The rule bot version of the game: the setup's own; else the resume's (a save or a restart
    /// of this host carries it); a resumed save without one was made by version 1 (the field
    /// came with version 2); a new game plays the current bot. A setup gets it filled in.
    ///
    /// # Errors
    /// A message when this build does not play that version.
    pub fn settle_bot_version(&mut self) -> Result<u32, String> {
        let own = self.setup.as_ref().and_then(|setup| setup.bot_version);
        let version = own.unwrap_or_else(|| match &self.resume {
            Some(resume) => resume.bot_version.unwrap_or(1),
            None => ti4_rule_bot::tuning::BOT_VERSION,
        });
        check_bot_version(version)?;
        // The starting-technology choice came with bot version 3: a game of an older bot is
        // played as it was recorded, without it.
        let choice = version >= STARTING_TECH_FROM_BOT_VERSION;
        if let Some(setup) = &mut self.setup {
            setup.bot_version = Some(version);
            if setup.starting_tech.is_none() && choice {
                setup.starting_tech = Some(true);
            }
        }
        if let Some(table) = &mut self.table
            && choice
        {
            table.setup.starting_tech = true;
        }
        Ok(version)
    }
}

fn mix(seed: u64) -> u64 {
    let mut x = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

/// A game ready to be played.
pub struct Built {
    pub game: Game<'static>,
    pub galaxy: Option<Galaxy>,
    /// The seed the seats decide from.
    pub seed: u64,
    pub players: usize,
    /// The seats that the page plays (bit 0 is seat `a`).
    pub humans: u32,
    /// The kind of the bot of each seat where the page does not play it (`ti4_host::bots`); none
    /// for a seat that decides at random (in a game of a table, as on the server; else from the
    /// stream of the random seats).
    pub bots: Vec<Option<String>>,
    /// The game is of a table: a random seat decides from the seed, as on the server.
    pub table: bool,
    /// Who sits in each seat, when the game is of a table whose players have names.
    pub seats: Option<TableSeats>,
    pub game_id: String,
    /// The table of `ti4_host::setup` that the game was made from; none for a game of a fixed
    /// setup ([`Local`]).
    pub setup: Option<TableSetup>,
    /// The scenario the game starts from.
    pub scenario: Option<String>,
    /// The request with the choices that were left open filled in: a save replays with it.
    pub request: GameRequest,
    /// The opening of the scenario that the game starts from (`ti4_host::view::scenario`).
    pub opening: Option<ti4_host::view::scenario::Opening>,
}

impl Setup {
    /// The table of `ti4_host::setup` that this request asks for. The person sits at the place
    /// that they asked for.
    #[must_use]
    pub fn table(&self) -> TableSetup {
        let map = match self.map.as_deref() {
            None => MapChoice::from_stored(catalog::recommended_for(self.players).as_deref()),
            Some("random") => MapChoice::Random,
            Some(alias) => MapChoice::from_stored(Some(alias)),
        };
        TableSetup {
            player_count: self.players,
            map,
            strategy_cards: self
                .card_set
                .clone()
                .unwrap_or_else(|| card_set::DEFAULT.to_owned()),
            speaker_order: self.speaker.unwrap_or_default(),
            start_from: self.scenario.clone(),
            preset: self.preset.clone(),
            starting_tech: self.starting_tech == Some(true),
            seed: self.seed,
        }
    }

    /// Checks the table and fills in what was left open.
    ///
    /// # Errors
    /// A message for the person when the table is not one the host offers.
    pub fn resolved(&self) -> Result<Self, String> {
        let mut setup = self.clone();
        if let Some(id) = &setup.scenario {
            let scenario =
                ti4_host::view::scenario::find(id).ok_or_else(|| format!("no scenario '{id}'"))?;
            setup.seed = scenario.game_seed(setup.seed);
            setup.players = scenario.players;
        }
        setup.table().check()?;
        let human = match setup.human_seat {
            Some(seat) if seat < setup.players => seat,
            Some(seat) => {
                return Err(format!(
                    "seat {seat} is not at a table of {}",
                    setup.players
                ));
            }
            None => usize::try_from(mix(setup.seed) % setup.players as u64)
                .expect("a seat fits a usize"),
        };
        setup.human_seat = Some(human);
        if let Some(faction) = &setup.human_faction
            && !seating::LOBBY_FACTIONS.contains(&faction.as_str())
        {
            return Err(format!("faction '{faction}' is not played here"));
        }
        if let Some(version) = setup.bot_version {
            check_bot_version(version)?;
        }
        Ok(setup)
    }

    /// The game this table plays.
    ///
    /// # Errors
    /// A message when the table is not offered, or the map does not build.
    pub fn build(&self) -> Result<Built, String> {
        let setup = self.resolved()?;
        let content = ContentStore::embedded();
        let players = setup.players;
        let human = setup.human_seat.expect("resolved sets the seat");
        let table = setup.table();
        let chosen: Vec<Option<String>> = (0..players)
            .map(|seat| {
                (seat == human)
                    .then(|| setup.human_faction.clone())
                    .flatten()
            })
            .collect();
        let joined: Vec<PlayerId> = SEATS[..players].iter().map(|s| PlayerId::new(*s)).collect();
        let drawn = setup::draw(&table, &chosen)?;
        let (state, galaxy) = setup::create(content, &table, &joined, &drawn)?;
        let opening = table
            .scenario()?
            .and_then(|scenario| ti4_host::view::scenario::opening(scenario, &state, &galaxy));
        let kind = setup
            .bots
            .as_ref()
            .map_or(bot_kind::RULE, Bots::kind)
            .to_owned();
        let bots = (0..players)
            .map(|seat| (seat != human && kind != bot_kind::RANDOM).then(|| kind.clone()))
            .collect();
        let game = Game::with_seeded_random(state, content, table.game_seed())
            .with_sources(POK)
            .with_galaxy(galaxy.clone());
        Ok(Built {
            game,
            galaxy: Some(galaxy),
            seed: setup.seed,
            players,
            humans: 1 << human,
            bots,
            table: false,
            seats: None,
            game_id: setup.game_id.clone(),
            scenario: table.start_from.clone(),
            setup: Some(table),
            request: GameRequest {
                setup: Some(setup),
                ..GameRequest::default()
            },
            opening,
        })
    }
}

/// What the setup form may offer: the options of `ti4_host::setup`, with the maps of every
/// count, the presets and what identifies the engine. The MLP kinds are those of the models
/// that the engine has loaded.
#[must_use]
pub fn describe() -> serde_json::Value {
    let models: Vec<ti4_host::bots::ModelEntry> = ti4_host::bots::models()
        .iter()
        .map(ti4_host::bots::MlpModel::entry)
        .collect();
    let mut options =
        serde_json::to_value(setup::setup_options("The page", 0, &models)).unwrap_or_default();
    let maps: serde_json::Map<String, serde_json::Value> = setup::PLAYER_COUNTS
        .map(|count| {
            (
                count.to_string(),
                serde_json::to_value(catalog::catalog(Some(count)).unwrap_or_default())
                    .unwrap_or_default(),
            )
        })
        .collect();
    if let Some(object) = options.as_object_mut() {
        object.insert("maps".to_owned(), maps.into());
        object.insert("presets".to_owned(), ti4_host::preset::KNOWN.into());
        object.insert(
            "content_digest".to_owned(),
            ti4_content::provenance::embedded_digest().corpus.into(),
        );
        object.insert("engine_build".to_owned(), env!("CARGO_PKG_VERSION").into());
    }
    options
}

/// A pictured map for the setup form: the tiles of the board a table of `player_count` gets for
/// `choice`, with a place for each seat (`/api/maps/{alias}/preview` of the server).
///
/// # Errors
/// A message when the choice does not build for that many players.
pub fn map_preview(
    choice: &MapChoice,
    player_count: usize,
    variant: u32,
) -> Result<serde_json::Value, String> {
    let seed = catalog::variant_seed_salted(0x5eed_0ff1_1e00, choice, player_count, variant);
    catalog::preview(choice, player_count, seed)
        .map(|preview| serde_json::to_value(preview).unwrap_or_default())
}
