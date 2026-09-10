//! M11-010 — the table's telemetry, read into engine state.
//!
//! The mod uploads far more than the board. Alongside `hexSummary` it sends, per player: faction,
//! colour, command tokens, trade goods, commodities, technologies, strategy cards, score and laws —
//! plus the round, the speaker, whose turn it is, and the revealed objectives. Most of the
//! observation problem is already solved by the mod; this is the translation.
//!
//! # Nothing hidden is invented
//!
//! The telemetry reports `handSummary` as **counts** — `{"Actions": 2, "Secret Objectives": 1}` —
//! and never the cards themselves. So the imported state's hands are empty, and that fact is
//! reported in [`Imported::gaps`] rather than papered over. Everything this module could not
//! observe appears there, because a state that silently differs from the table is worse than one
//! that says where it is blind: reconciliation would compare against it and report plausible
//! differences that are really import gaps.
//!
//! # Built on a real setup, not assembled field by field
//!
//! `GameState` has thirty-four fields and most of them are decks whose contents the table cannot
//! show. Rather than construct one by hand and quietly leave two-thirds at their defaults, this
//! starts from [`ti4_engine::setup::start_game`] — a genuine, internally consistent game — and
//! overwrites exactly what the telemetry observed. The decks stay coherent because nothing here
//! touches them.
//!
//! # Three name schemes that do not match
//!
//! - The mod drops the leading article: it says `Xxcha Kingdom`, the corpus says
//!   `The Xxcha Kingdom`. Matching normalises both.
//! - The mod writes tile `1`, the corpus writes `01` — handled in [`crate::mapping`].
//! - The summary's unit codes are their own alphabet, mapped explicitly in
//!   [`crate::hexsummary`] and translated to engine unit ids here.

use std::collections::{BTreeMap, BTreeSet};

use serde::Deserialize;
use ti4_content::ContentStore;
use ti4_content::galaxy::Galaxy;
use ti4_model::content_types::SourceSet;
use ti4_model::id::{PlayerId, SystemId, UnitTypeId};
use ti4_model::state::GameState;
use ti4_model::units::Unit;

use crate::hexsummary::{self, Board, Colour, Piece};
use crate::mapping;

/// Read a Lua table that may arrive as a map or, when empty, as `[]`.
///
/// Lua has one table type and no way to tell an empty dictionary from an empty list, so the mod's
/// JSON encoder writes `[]` for both. `handSummary` is a map the moment a seat holds a card and an
/// array the moment it holds none — which means a strict map deserialiser works on every fixture
/// until somebody plays their last action card.
fn lua_table<'de, D, T>(deserializer: D) -> Result<BTreeMap<String, T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::de::DeserializeOwned,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    match value {
        serde_json::Value::Object(map) => {
            serde_json::from_value(serde_json::Value::Object(map)).map_err(serde::de::Error::custom)
        }
        // Only an *empty* array is the ambiguous case. A populated one is a real disagreement
        // about the shape and is worth refusing.
        serde_json::Value::Array(items) if items.is_empty() => Ok(BTreeMap::new()),
        other => Err(serde::de::Error::custom(format!(
            "expected a table, got {other}"
        ))),
    }
}

/// What the mod uploads, as far as this module reads it.
///
/// Unknown fields are ignored rather than refused: the mod is developed independently and will add
/// keys, and a bridge that stops working when it does is a bridge nobody can keep running.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Telemetry {
    /// The board, in the mod's sticky encoding.
    #[serde(rename = "hexSummary")]
    pub hex_summary: String,
    /// The game round, one-based.
    pub round: u32,
    /// The colour holding the speaker token.
    pub speaker: String,
    /// The colour whose turn it is.
    pub turn: String,
    /// Points needed to win — 10 or 14.
    pub scoreboard: i32,
    /// Prophecy of Kings content is in play.
    #[serde(rename = "isPoK")]
    pub is_pok: bool,
    /// Laws in play, by the mod's naming.
    pub laws: Vec<serde_json::Value>,
    /// Revealed objectives, grouped by kind.
    #[serde(deserialize_with = "lua_table")]
    pub objectives: BTreeMap<String, serde_json::Value>,
    /// One record per seat, in table order.
    pub players: Vec<TelemetryPlayer>,
}

/// One seat, as the mod reports it.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct TelemetryPlayer {
    /// The mod's colour name.
    pub color: String,
    /// The full faction name, without the corpus's leading article.
    #[serde(rename = "factionName")]
    pub faction_name: String,
    /// The short name, which the corpus sometimes carries and sometimes does not.
    #[serde(rename = "factionShort")]
    pub faction_short: String,
    /// Whether this seat is still acting this round. A seat that has passed reports `false`.
    ///
    /// Absent in older uploads, where `true` is the safe reading: treating an unknown seat as
    /// passed would tell the policy its opponents are done when they are not.
    #[serde(default = "yes")]
    pub active: bool,
    /// Victory points.
    pub score: i32,
    /// Points from the custodians token specifically.
    #[serde(rename = "custodiansPoints")]
    pub custodians_points: i32,
    pub commodities: i32,
    #[serde(rename = "tradeGoods")]
    pub trade_goods: i32,
    /// Tokens in the three pools.
    #[serde(rename = "commandTokens")]
    pub command_tokens: CommandTokens,
    /// Technology names, in the mod's spelling.
    pub technologies: Vec<String>,
    /// Strategy cards held this round.
    #[serde(rename = "strategyCards")]
    pub strategy_cards: Vec<String>,
    /// Those already used.
    #[serde(rename = "strategyCardsFaceDown")]
    pub strategy_cards_face_down: Vec<String>,
    /// Counts only — never the cards. This is why hands cannot be imported.
    #[serde(rename = "handSummary", deserialize_with = "lua_table")]
    pub hand_summary: BTreeMap<String, i32>,
}

/// Serde default for [`TelemetryPlayer::active`].
const fn yes() -> bool {
    true
}

/// The three command token pools.
#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(default)]
pub struct CommandTokens {
    pub fleet: i32,
    pub strategy: i32,
    pub tactics: i32,
}

/// Something the table could not tell us.
///
/// Kept as data rather than a log line so a caller can decide what to do about each one — refuse to
/// act, ask the human, or proceed knowing the shape of its own ignorance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Gap {
    /// A seat's action cards are known only as a count.
    HandUnknown { seat: PlayerId, cards: i32 },
    /// A seat's secret objectives are known only as a count.
    SecretsUnknown { seat: PlayerId, secrets: i32 },
    /// A tile on the table that the content corpus does not carry.
    UnknownTile(String),
    /// A faction name the corpus could not resolve; the seat was skipped.
    UnknownFaction { colour: String, name: String },
    /// A unit code with no engine equivalent at this seat.
    UntranslatedUnit { tile: String, piece: Piece },
    /// Which planets are exhausted is not reported, so every planet is imported ready.
    ExhaustionUnknown,
    /// Deck orders, discards and the objective deck are not observable.
    DecksUnknown,
    /// The phase was inferred from whether strategy cards are dealt, not reported.
    PhaseInferred(ti4_model::state::Phase),
    /// A strategy card the mod named that is not in the set this game is playing with.
    StrategyCardNotResolved { seat: PlayerId, name: String },
    /// A technology the mod named that the corpus does not carry under that name.
    TechnologyNotResolved { seat: PlayerId, name: String },
    /// The table reports laws in play that this importer does not translate.
    LawsNotTranslated(Vec<String>),
    /// The table reports revealed objectives that this importer does not translate.
    ///
    /// The setup-dealt ones are cleared regardless, so an untranslated objective is *absent*
    /// rather than replaced by whatever `start_game` happened to reveal.
    ObjectivesNotTranslated(Vec<String>),
}

impl std::fmt::Display for Gap {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::HandUnknown { seat, cards } => {
                write!(
                    formatter,
                    "{seat}'s {cards} action card(s) are not observable"
                )
            }
            Self::SecretsUnknown { seat, secrets } => write!(
                formatter,
                "{seat}'s {secrets} secret objective(s) are not observable"
            ),
            Self::UnknownTile(tile) => write!(formatter, "tile {tile} is not in the corpus"),
            Self::UnknownFaction { colour, name } => {
                write!(
                    formatter,
                    "{colour} plays {name:?}, which the corpus does not name"
                )
            }
            Self::UntranslatedUnit { tile, piece } => {
                write!(formatter, "{piece:?} on tile {tile} has no engine unit id")
            }
            Self::ExhaustionUnknown => {
                formatter.write_str("planet exhaustion is not reported; all imported ready")
            }
            Self::DecksUnknown => formatter.write_str("deck order and discards are not observable"),
            Self::StrategyCardNotResolved { seat, name } => write!(
                formatter,
                "{seat} holds strategy card {name:?}, which is not in this game's card set"
            ),
            Self::TechnologyNotResolved { seat, name } => write!(
                formatter,
                "{seat} holds {name:?}, which resolves to no corpus technology id"
            ),
            Self::LawsNotTranslated(laws) => write!(
                formatter,
                "{} law(s) are in play and were not translated: {laws:?}",
                laws.len()
            ),
            Self::ObjectivesNotTranslated(names) => write!(
                formatter,
                "{} revealed objective(s) were not translated (setup's were cleared): {names:?}",
                names.len()
            ),
            Self::PhaseInferred(phase) => write!(
                formatter,
                "the phase is not reported; inferred {phase:?} from the dealt strategy cards"
            ),
        }
    }
}

/// Why telemetry could not become a state at all.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ImportError {
    /// The board did not decode.
    #[error(transparent)]
    Board(#[from] hexsummary::MalformedSummary),
    /// The board could not be placed.
    #[error(transparent)]
    Mapping(#[from] mapping::MappingError),
    /// No seat resolved, so there is no game to import.
    #[error("no seat's faction could be resolved from the telemetry")]
    NoSeats,
    /// Setup refused the resolved seats.
    #[error("setting up the imported game: {0}")]
    Setup(String),
}

/// A state read off the table, with an account of what could not be read.
#[derive(Debug, Clone)]
pub struct Imported {
    /// The engine state.
    pub state: GameState,
    /// The board, for `Game::with_galaxy`.
    pub galaxy: Galaxy,
    /// Which colour holds which seat, for translating decisions back into commands.
    pub seats: BTreeMap<Colour, PlayerId>,
    /// Everything the table did not say.
    pub gaps: Vec<Gap>,
}

/// Normalise a faction name for comparison: drop a leading article, fold case, collapse spaces.
fn normalise(name: &str) -> String {
    let trimmed = name.trim();
    let without_article = trimmed
        .strip_prefix("The ")
        .or_else(|| trimmed.strip_prefix("the "))
        .unwrap_or(trimmed);
    without_article
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Letters and digits only, so `Jol-Nar` and `jolnar` compare equal.
fn squashed(name: &str) -> String {
    name.chars()
        .filter(char::is_ascii_alphanumeric)
        .flat_map(char::to_lowercase)
        .collect()
}

/// The corpus's alias for a faction the mod named.
///
/// Tries the full name, then the short name, then the alias itself, all normalised. The mod drops
/// the leading article — `Xxcha Kingdom` against the corpus's `The Xxcha Kingdom` — which is why
/// none of these is an equality test.
#[must_use]
pub fn resolve_faction(
    store: &ContentStore,
    name: &str,
    short: &str,
    sources: SourceSet,
) -> Option<String> {
    let catalogue = ti4_content::factions::catalogue(store, sources);
    let wanted = normalise(name);
    let wanted_short = normalise(short);
    for (alias, faction) in &catalogue {
        let full = faction.name().map(normalise).unwrap_or_default();
        // The corpus carries no short name accessor, so the alias carries that job: `jolnar`
        // against the mod's `Jol-Nar` needs punctuation folded out as well as case.
        if full == wanted
            || (!wanted_short.is_empty() && squashed(alias) == squashed(&wanted_short))
            || squashed(alias) == squashed(&wanted)
        {
            return Some((*alias).to_owned());
        }
    }
    None
}

/// The corpus's technology ids, keyed by the folded printed name the mod reports.
///
/// The mod sends printed names -- `Sarween Tools`, `Antimass Deflectors` -- and the corpus keys
/// technologies by short alias (`amd`, `gd`, `fl`). Importing the printed name verbatim produced a
/// `TechnologyId` the trained vocabulary has never seen, and **every** out-of-vocabulary feature on
/// a real board came from exactly that: `seat-state:opponent-technology:Plasma Scoring` and its
/// siblings. Coverage on an imported board was 79-88% against 100% on a simulated one, and this was
/// the whole of the difference.
///
/// The seventh naming scheme in this project, and the same shape as the tile padding and the
/// faction article: two sides of the wire spelling the same thing differently.
fn technology_ids(store: &ContentStore, sources: SourceSet) -> BTreeMap<String, String> {
    store
        .from_sources(ti4_model::content_types::ContentType::Technologies, sources)
        .filter_map(|record| {
            let id = record.id()?;
            let name = record.text("name")?;
            Some((squashed(name), id.to_owned()))
        })
        .collect()
}

/// The strategy-card ids in play, keyed by the folded printed name the mod reports.
///
/// Built from the ids `start_game` actually dealt rather than from the whole corpus: the corpus
/// carries both `base2` and `pok2diplomacy` for "Diplomacy", and choosing the wrong one gives the
/// seat a card the rest of the state has never heard of.
///
/// Getting this wrong does not merely lose information. The mod reports `Leadership`; the engine
/// calls it `pok1leadership`; importing the lowercased printed name gave every seat a card no rule
/// could resolve, the strategic action never completed, and the action phase **never ended** -- a
/// simulated run from an imported board spent 399,908 consecutive steps asking "action phase".
fn strategy_card_ids(
    state: &GameState,
    store: &ContentStore,
    sources: SourceSet,
) -> BTreeMap<String, String> {
    let in_play: BTreeSet<&str> = state
        .card_initiative
        .keys()
        .map(ti4_model::id::StrategyCardId::as_str)
        .collect();
    store
        .from_sources(
            ti4_model::content_types::ContentType::StrategyCards,
            sources,
        )
        .filter_map(|record| {
            let id = record.id()?;
            if !in_play.contains(id) {
                return None;
            }
            let name = record.text("name")?;
            Some((squashed(name), id.to_owned()))
        })
        .collect()
}

/// The engine unit id for a summary piece, or `None` for the two tokens, which are not units.
#[must_use]
pub const fn unit_id(piece: Piece) -> Option<&'static str> {
    Some(match piece {
        Piece::Carrier => "carrier",
        Piece::Cruiser => "cruiser",
        Piece::Destroyer => "destroyer",
        Piece::Dreadnought => "dreadnought",
        Piece::Fighter => "fighter",
        Piece::Flagship => "flagship",
        Piece::Infantry => "infantry",
        Piece::Mech => "mech",
        Piece::Pds => "pds",
        Piece::SpaceDock => "spacedock",
        Piece::WarSun => "warsun",
        Piece::OwnerToken | Piece::CommandToken => return None,
    })
}

/// Read a table's telemetry into an engine state.
///
/// # Errors
/// [`ImportError`] when the board does not decode or place, when no seat resolves, or when setup
/// refuses the resolved seats.
// One line over the limit after the review fixes. Splitting further would separate the seating
// from the position-preserving vector it exists to protect, which is the bug this function was
// corrected for.
#[expect(
    clippy::too_many_lines,
    reason = "seating and its position vector belong together"
)]
pub fn import(
    store: &'static ContentStore,
    telemetry: &Telemetry,
    sources: SourceSet,
) -> Result<Imported, ImportError> {
    let board = hexsummary::decode(&telemetry.hex_summary)?;
    let mapped = mapping::galaxy_from_board(store, &board, sources)?;

    let mut gaps: Vec<Gap> = mapped
        .unknown_tiles
        .iter()
        .map(|tile| Gap::UnknownTile(tile.clone()))
        .collect();

    // -- seats ------------------------------------------------------------------------------
    //
    // `by_position` is parallel to `telemetry.players` and holds `None` where a faction did not
    // resolve. An earlier version pushed only the resolved seats into a compacted vector and then
    // indexed *that* with the original telemetry position: one unresolved faction shifted every
    // later seat, so scores, tokens, technologies, the speaker and the active player were all
    // silently attached to the wrong player. Positions are kept for exactly that reason.
    let mut seats: BTreeMap<Colour, PlayerId> = BTreeMap::new();
    let mut by_position: Vec<Option<PlayerId>> = Vec::with_capacity(telemetry.players.len());
    let mut order: Vec<PlayerId> = Vec::new();
    for player in &telemetry.players {
        let Some(alias) =
            resolve_faction(store, &player.faction_name, &player.faction_short, sources)
        else {
            gaps.push(Gap::UnknownFaction {
                colour: player.color.clone(),
                name: player.faction_name.clone(),
            });
            by_position.push(None);
            continue;
        };
        let seat = PlayerId::new(&alias);
        if let Some(colour) = colour_of(&player.color) {
            seats.insert(colour, seat.clone());
        }
        by_position.push(Some(seat.clone()));
        order.push(seat);
    }
    if order.is_empty() {
        return Err(ImportError::NoSeats);
    }

    // A real setup, so every deck this module cannot observe is at least internally coherent.
    let speaker = telemetry
        .players
        .iter()
        .position(|player| player.color == telemetry.speaker)
        .and_then(|index| by_position.get(index))
        .and_then(Clone::clone);
    let mut state = ti4_engine::setup::start_game(store, &order, sources, speaker)
        .map_err(|error| ImportError::Setup(error.to_string()))?;

    // `start_game` deals a real setup, which includes hands. Those cards are *invented* here --
    // the table showed counts and nothing else -- so they go, and the counts become gaps below.
    // Leaving them would let the bot play an action card it does not hold.
    for player in &mut state.players {
        // `start_game` creates generic seats; the resolved telemetry alias is also this bridge's
        // player id. Rules code reads `Player::faction` for transaction partners and faction
        // abilities, so retaining the placeholder collapses every imported seat into "generic".
        player.faction = ti4_model::id::FactionId::new(player.id.as_str());
        player.action_cards.clear();
        player.secret_objectives.clear();
    }

    state.round = telemetry.round.max(1);
    state.active = telemetry
        .players
        .iter()
        .position(|player| player.color == telemetry.turn)
        .and_then(|index| by_position.get(index))
        .and_then(Clone::clone);

    // -- per seat ---------------------------------------------------------------------------
    let technologies = technology_ids(store, sources);
    let cards = strategy_card_ids(&state, store, sources);
    for (index, reported) in telemetry.players.iter().enumerate() {
        let Some(Some(seat)) = by_position.get(index) else {
            continue; // This record's faction did not resolve; it belongs to nobody.
        };
        let Some(player) = state.players.iter_mut().find(|p| &p.id == seat) else {
            continue;
        };
        apply_seat(player, reported, &technologies, &cards, &mut gaps);

        // Hands are counts, never cards. Recorded, never guessed.
        let cards = reported.hand_summary.get("Actions").copied().unwrap_or(0);
        if cards > 0 {
            gaps.push(Gap::HandUnknown {
                seat: seat.clone(),
                cards,
            });
        }
        let secrets = reported
            .hand_summary
            .get("Secret Objectives")
            .copied()
            .unwrap_or(0);
        if secrets > 0 {
            gaps.push(Gap::SecretsUnknown {
                seat: seat.clone(),
                secrets,
            });
        }
    }

    // The table never says which phase it is in, and the engine offers entirely different
    // decisions in each. The one observable signal is whether the strategy cards have been dealt:
    // before they are, the table is still choosing them.
    //
    // This cannot separate Action from Status or Agenda -- all three have cards dealt -- so it
    // names Action, which is where all but a handful of a round's decisions live, and records the
    // inference. A caller that knows better should overwrite `state.phase` and ignore the gap.
    // A fresh live draft uploads between picks, so one dealt card does not mean the strategy
    // phase is complete. Three- and four-player games draft two; larger games draft one.
    let cards_per_seat = if telemetry.players.len() <= 4 { 2 } else { 1 };
    let dealt = !telemetry.players.is_empty()
        && telemetry
            .players
            .iter()
            .all(|player| player.strategy_cards.len() >= cards_per_seat);
    state.phase = if dealt {
        ti4_model::state::Phase::Action
    } else {
        ti4_model::state::Phase::Strategy
    };
    gaps.push(Gap::PhaseInferred(state.phase));

    apply_public_state(&mut state, telemetry, &mut gaps);

    // `start_game` begins with every strategy card unclaimed. Telemetry overwrites the players'
    // holdings, so remove those same ids from the draft pool as one atomic import invariant.
    // Otherwise a live import between picks can legally offer a card already on the table.
    let held: BTreeSet<_> = state
        .players
        .iter()
        .flat_map(|player| player.strategy_cards.iter().cloned())
        .collect();
    state
        .unclaimed_strategy_cards
        .retain(|card| !held.contains(card));

    // LRR 27.4: once the custodians token is lifted, every round has an agenda phase.
    state.custodians_removed = telemetry
        .players
        .iter()
        .any(|player| player.custodians_points > 0);

    // -- board ------------------------------------------------------------------------------
    state.board = board_state(store, &board, &seats, sources, &mut gaps);

    gaps.push(Gap::ExhaustionUnknown);
    gaps.push(Gap::DecksUnknown);

    Ok(Imported {
        state,
        galaxy: mapped.galaxy,
        seats,
        gaps,
    })
}

/// Public state the table reports but this importer does not yet translate.
///
/// Removed rather than left as whatever `start_game` dealt. An invented revealed objective changes
/// scoring legality and the policy's ranking of every option; an absent one is merely absent, and
/// the gap says which.
fn apply_public_state(state: &mut GameState, telemetry: &Telemetry, gaps: &mut Vec<Gap>) {
    // Observable public state that this importer does not yet translate is *removed*, not left as
    // whatever `start_game` happened to deal. An invented revealed objective changes scoring
    // legality and the policy's ranking; an absent one is merely absent, and the gap says so.
    let revealed: Vec<String> = telemetry
        .objectives
        .iter()
        .filter(|(kind, _)| kind.starts_with("Public Objectives"))
        .filter_map(|(_, value)| value.as_array())
        .flatten()
        .filter_map(|name| name.as_str().map(ToOwned::to_owned))
        .collect();
    if !state.revealed_objectives.is_empty() || !revealed.is_empty() {
        state.revealed_objectives.clear();
        gaps.push(Gap::ObjectivesNotTranslated(revealed));
    }
    let laws: Vec<String> = telemetry
        .laws
        .iter()
        .map(|law| {
            law.as_str().map_or_else(
                || {
                    law.get("name")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("?")
                        .to_owned()
                },
                ToOwned::to_owned,
            )
        })
        .collect();
    if !laws.is_empty() {
        state.laws.clear();
        gaps.push(Gap::LawsNotTranslated(laws));
    }
}

/// Copy one seat's reported holdings onto its engine player.
///
/// Technologies and strategy cards are only overwritten when the table reported any: an empty list
/// from the mod is ambiguous between "none" and "not reported by this build of the mod", and
/// wiping a seat's starting technologies on that basis would be a silent rules change.
fn apply_seat(
    player: &mut ti4_model::state::Player,
    reported: &TelemetryPlayer,
    technologies: &BTreeMap<String, String>,
    cards: &BTreeMap<String, String>,
    gaps: &mut Vec<Gap>,
) {
    player.victory_points = reported.score;
    player.trade_goods = reported.trade_goods;
    player.commodities = reported.commodities;
    // The mod reports whether a seat is still acting; the engine stores the complement. Left at
    // `start_game`'s `false`, every passed opponent looks active to the policy in later rounds.
    player.passed = !reported.active;
    player.tactic_tokens = reported.command_tokens.tactics;
    player.fleet_tokens = reported.command_tokens.fleet;
    player.strategic_tokens = reported.command_tokens.strategy;

    if !reported.technologies.is_empty() {
        player.technologies = reported
            .technologies
            .iter()
            .map(|name| {
                technologies.get(&squashed(name)).map_or_else(
                    || {
                        // Kept as reported rather than dropped -- the seat does hold it -- but
                        // named, because an unresolved technology is one the policy cannot see.
                        gaps.push(Gap::TechnologyNotResolved {
                            seat: player.id.clone(),
                            name: name.clone(),
                        });
                        ti4_model::id::TechnologyId::new(name)
                    },
                    ti4_model::id::TechnologyId::new,
                )
            })
            .collect();
    }
    if !reported.strategy_cards.is_empty() {
        // Written as a loop rather than two closures over `gaps`: a closure that pushes needs
        // `FnMut`, and `Iterator::map` wants `FnOnce` per call from a shared reference.
        let mut resolve = |names: &[String]| -> Vec<ti4_model::id::StrategyCardId> {
            let mut out = Vec::with_capacity(names.len());
            for name in names {
                match cards.get(&squashed(name)) {
                    Some(id) => out.push(ti4_model::id::StrategyCardId::new(id)),
                    None => {
                        gaps.push(Gap::StrategyCardNotResolved {
                            seat: player.id.clone(),
                            name: name.clone(),
                        });
                        out.push(ti4_model::id::StrategyCardId::new(name.to_lowercase()));
                    }
                }
            }
            out
        };
        player.strategy_cards = resolve(&reported.strategy_cards);
        player.exhausted_strategy_cards = resolve(&reported.strategy_cards_face_down)
            .into_iter()
            .collect();
    }
}

/// The mod's colour name to the summary's colour.
fn colour_of(name: &str) -> Option<Colour> {
    match name {
        "White" => Some(Colour::White),
        "Blue" => Some(Colour::Blue),
        "Purple" => Some(Colour::Purple),
        "Yellow" => Some(Colour::Yellow),
        "Red" => Some(Colour::Red),
        "Green" => Some(Colour::Green),
        "Orange" => Some(Colour::Orange),
        "Pink" => Some(Colour::Pink),
        "Brown" => Some(Colour::Brown),
        _ => None,
    }
}

/// Everything standing on the board, by system.
fn board_state(
    store: &ContentStore,
    board: &Board,
    seats: &BTreeMap<Colour, PlayerId>,
    sources: SourceSet,
    gaps: &mut Vec<Gap>,
) -> BTreeMap<SystemId, ti4_model::state::SystemState> {
    let catalogue = ti4_content::galaxy::all_systems(store, sources);
    let mut out: BTreeMap<SystemId, ti4_model::state::SystemState> = BTreeMap::new();

    for system in &board.systems {
        let Some(tile) = mapping::corpus_id(&catalogue, &system.tile) else {
            continue;
        };
        let mut entry = ti4_model::state::SystemState::default();

        for stack in &system.space.stacks {
            let Some(colour) = stack.colour else { continue };
            let Some(seat) = seats.get(&colour) else {
                continue;
            };
            if stack.piece == Piece::CommandToken {
                entry.command_tokens.insert(seat.clone());
                continue;
            }
            if stack.piece == Piece::OwnerToken {
                continue; // Owner tokens sit on planets, not in space.
            }
            let Some(unit) = unit_id(stack.piece) else {
                gaps.push(Gap::UntranslatedUnit {
                    tile: system.tile.clone(),
                    piece: stack.piece,
                });
                continue;
            };
            for _ in 0..stack.count {
                entry
                    .units
                    .push(Unit::new(UnitTypeId::new(unit), seat.clone()));
            }
        }

        // Planets come in the mod's order, which is the corpus's order for that tile.
        let planet_ids: Vec<String> = catalogue
            .get(tile)
            .map(|record| {
                record
                    .planets()
                    .into_iter()
                    .map(ToOwned::to_owned)
                    .collect()
            })
            .unwrap_or_default();
        for (index, region) in system.planets.iter().enumerate() {
            let Some(planet_id) = planet_ids.get(index) else {
                continue;
            };
            let planet = ti4_model::id::PlanetId::new(planet_id);

            // Control, in the order the rules give it: a unit on the planet controls it, and an
            // owner token marks control by a player with nothing standing there (LRR 25.4).
            let occupiers = region.occupiers();
            let controller = occupiers
                .first()
                .and_then(|colour| seats.get(colour))
                .or_else(|| region.owner_token().and_then(|colour| seats.get(&colour)));
            if let Some(seat) = controller {
                entry.planet_control.insert(planet.clone(), seat.clone());
            }

            let mut ground: Vec<Unit> = Vec::new();
            for stack in &region.stacks {
                let Some(colour) = stack.colour else { continue };
                let Some(seat) = seats.get(&colour) else {
                    continue;
                };
                let Some(unit) = unit_id(stack.piece) else {
                    continue; // Tokens are not units; owner tokens were read above.
                };
                for _ in 0..stack.count {
                    ground.push(Unit::new(UnitTypeId::new(unit), seat.clone()));
                }
            }
            if !ground.is_empty() {
                entry.planet_units.insert(planet, ground);
            }
        }

        if !entry.units.is_empty()
            || !entry.command_tokens.is_empty()
            || !entry.planet_control.is_empty()
            || !entry.planet_units.is_empty()
        {
            out.insert(SystemId::new(tile), entry);
        }
    }
    out
}

/// Seats that hold no units anywhere, which usually means an import went wrong.
#[must_use]
pub fn seats_with_nothing(state: &GameState) -> BTreeSet<PlayerId> {
    let placed: BTreeSet<PlayerId> = state
        .board
        .values()
        .flat_map(|system| {
            system
                .units
                .iter()
                .chain(system.planet_units.values().flatten())
                .map(|unit| unit.owner.clone())
        })
        .collect();
    state
        .players
        .iter()
        .map(|player| player.id.clone())
        .filter(|seat| !placed.contains(seat))
        .collect()
}
