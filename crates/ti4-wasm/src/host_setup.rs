//! How a hosted game is set up: the table the person asks for, the game it makes, and what the
//! setup form of `web/` may offer (card sets, factions, maps, presets).
//!
//! The game is made by the same functions as a lobby game of the server
//! (`ti4_host::map::create_game_with_factions`), so a seed gives the same board and the same
//! opening state in both.

use serde::{Deserialize, Serialize};
use serde_json::json;
use ti4_content::ContentStore;
use ti4_content::galaxy::Galaxy;
use ti4_engine::game::Game;
use ti4_engine::seating;
use ti4_host::card_set;
use ti4_host::maps::catalog::{self, MapChoice};
use ti4_model::content_types::POK;
use ti4_model::id::PlayerId;
use ti4_view::map::SpeakerOrder;

use crate::SEATS;

/// The tables the offline game offers.
pub const MIN_PLAYERS: usize = 3;
pub const MAX_PLAYERS: usize = 6;

fn local_game_id() -> String {
    "local".to_owned()
}

/// The table of a game. Everything the game's opening state is made from; a save carries it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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
    /// A start preset of the server (a test table). Not offered by the form.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset: Option<String>,
    /// `"seat"` (the first seat opens) or `"random"`. Left out: the lobby's default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speaker: Option<String>,
    /// A prepared board of `ti4_view::scenario` (its seed and players win).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scenario: Option<String>,
    /// `"rule"` (the default) or `"random"`: who plays the seats of the others.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bots: Option<String>,
}

/// What a save brings to a new run of the same game.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Resume {
    /// The ids of the person's answers.
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
    /// How many decisions the saved game had, and their hash: checked when the replay ends.
    #[serde(default)]
    pub decisions: Option<usize>,
    #[serde(default)]
    pub decisions_hash: Option<String>,
}

/// The request of `ti4_host_start`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Start {
    pub setup: Setup,
    #[serde(default)]
    pub resume: Option<Resume>,
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
    pub seed: u64,
    pub players: usize,
    pub human: usize,
    /// The seats the rule bot plays (bit 0 is seat `a`).
    pub rule_seats: u32,
    /// The setup with the choices that were left open filled in.
    pub setup: Setup,
}

impl Setup {
    /// Checks the table and fills in what was left open.
    ///
    /// # Errors
    /// A message for the person when the table is not one the host offers.
    pub fn resolved(&self) -> Result<Self, String> {
        let mut setup = self.clone();
        if let Some(id) = &setup.scenario {
            let scenario =
                ti4_view::scenario::find(id).ok_or_else(|| format!("no scenario '{id}'"))?;
            setup.seed = scenario.seed;
            setup.players = scenario.players;
        } else if !(MIN_PLAYERS..=MAX_PLAYERS).contains(&setup.players) {
            return Err(format!(
                "{} players: an offline game has {MIN_PLAYERS} to {MAX_PLAYERS} seats",
                setup.players
            ));
        }
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
        if let Some(name) = &setup.card_set {
            card_set::validate(name).map_err(|error| error.to_string())?;
        }
        if let Some(faction) = &setup.human_faction
            && !seating::LOBBY_FACTIONS.contains(&faction.as_str())
        {
            return Err(format!("faction '{faction}' is not played here"));
        }
        if let Some(preset) = &setup.preset
            && !ti4_host::preset::is_known(preset)
        {
            return Err(format!("no preset '{preset}'"));
        }
        match setup.speaker.as_deref() {
            None | Some("seat" | "random") => {}
            Some(other) => return Err(format!("speaker '{other}': use 'seat' or 'random'")),
        }
        match setup.bots.as_deref() {
            None | Some("rule" | "random") => {}
            Some(other) => return Err(format!("bots '{other}': use 'rule' or 'random'")),
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
        let ids: Vec<PlayerId> = SEATS[..players].iter().map(|s| PlayerId::new(*s)).collect();
        let (state, galaxy) = if let Some(scenario) = &setup.scenario {
            ti4_view::scenario::start(content, scenario, &ids)?
        } else {
            let template = match setup.map.as_deref() {
                None => catalog::recommended_for(players),
                Some("random") => None,
                Some(alias) => Some(alias.to_owned()),
            };
            let chosen: Vec<Option<String>> = (0..players)
                .map(|seat| {
                    (seat == human)
                        .then(|| setup.human_faction.clone())
                        .flatten()
                })
                .collect();
            ti4_host::map::create_game_with_factions(
                content,
                &ids,
                setup.seed,
                template.as_deref(),
                setup.preset.as_deref(),
                Some(setup.card_set.as_deref().unwrap_or(card_set::DEFAULT)),
                SpeakerOrder::from_stored(setup.speaker.as_deref()),
                Some(&chosen),
            )?
        };
        let rule_seats = if setup.bots.as_deref() == Some("random") {
            0
        } else {
            ((1_u32 << players) - 1) & !(1 << human)
        };
        let game = Game::with_seeded_random(state, content, setup.seed)
            .with_sources(POK)
            .with_galaxy(galaxy.clone());
        Ok(Built {
            game,
            galaxy: Some(galaxy),
            seed: setup.seed,
            players,
            human,
            rule_seats,
            setup,
        })
    }
}

/// What the setup form may offer, for tables of 3 to 6.
#[must_use]
pub fn describe() -> serde_json::Value {
    let content = ContentStore::embedded();
    let card_sets: Vec<_> = card_set::OFFERED
        .iter()
        .map(|set| {
            let (ids, initiative) =
                ti4_engine::setup::strategy_card_setup_for_set(content, set).unwrap_or_default();
            let mut cards: Vec<_> = ids
                .iter()
                .map(|id| {
                    json!({
                        "id": id.as_str(),
                        "name": content
                            .get(ti4_model::content_types::ContentType::StrategyCards, id.as_str())
                            .and_then(|card| card.text("name"))
                            .unwrap_or(id.as_str()),
                        "initiative": initiative.get(id).copied().unwrap_or(99),
                    })
                })
                .collect();
            cards.sort_by_key(|card| card["initiative"].as_i64());
            json!({
                "id": set,
                "name": match *set {
                    card_set::TE => "Thunder's Edge",
                    card_set::POK => "Prophecy of Kings",
                    card_set::BASE_GAME_CODEX1 => "Base game with Codex I",
                    _ => "Strategy cards",
                },
                "default": *set == card_set::DEFAULT,
                "cards": cards,
            })
        })
        .collect();
    let faction = |id: &&str| {
        json!({
            "id": id,
            "name": ti4_content::factions::get(content, id)
                .and_then(|record| record.name().map(str::to_owned))
                .unwrap_or_else(|| (*id).to_owned()),
        })
    };
    let (base, pok) = seating::LOBBY_FACTIONS.split_at(seating::LOBBY_BASE_FACTIONS);
    let maps: serde_json::Map<String, serde_json::Value> = (MIN_PLAYERS..=MAX_PLAYERS)
        .map(|count| {
            (
                count.to_string(),
                serde_json::to_value(catalog::catalog(Some(count)).unwrap_or_default())
                    .unwrap_or_default(),
            )
        })
        .collect();
    json!({
        "min_players": MIN_PLAYERS,
        "max_players": MAX_PLAYERS,
        "strategy_card_sets": card_sets,
        "faction_groups": [
            { "name": "Base game", "factions": base.iter().map(faction).collect::<Vec<_>>() },
            { "name": "Prophecy of Kings", "factions": pok.iter().map(faction).collect::<Vec<_>>() },
        ],
        "bot_kinds": [
            { "id": "rule", "name": "Rule bot",
              "description": "Plays the other seats by rules: it builds, moves and scores." },
            { "id": "random", "name": "Random",
              "description": "Plays the other seats at random, among what the game allows." },
        ],
        "maps": maps,
        "presets": ti4_host::preset::KNOWN,
        "scenarios": ti4_view::scenario::SCENARIOS,
        "content_digest": ti4_content::provenance::embedded_digest().corpus,
        "engine_build": env!("CARGO_PKG_VERSION"),
    })
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
