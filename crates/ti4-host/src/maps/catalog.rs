//! What the host can choose, and what each choice looks like: the buildable template list, a
//! pictured preview of any choice, and the public description of a lobby's current choice.
//!
//! Previews are built with the very function a started game uses
//! ([`crate::map::create_game_with_template_seated`]), so a lobby's preview is the board its game
//! gets. No seed ever leaves this module.
//!
//! Both hosts use it: the server for the tables of its lobbies, the wasm host for the table of
//! the page ([`crate::setup`]).

use std::collections::BTreeMap;
use std::sync::{Mutex, OnceLock};

use serde::{Deserialize, Serialize};
use ti4_content::ContentStore;
use ti4_engine::seating;
use ti4_model::content_types::POK;
use ti4_model::id::PlayerId;

use super::template_loader::{MapTemplateSummary, TemplateLoader};
use crate::protocol::view::BoardTileView;

/// The host's choice of map for a lobby.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum MapChoice {
    /// A predefined layout; open slots are filled from the lobby's seeded filler.
    Template { alias: String },
    /// The seeded random board.
    Random,
}

impl MapChoice {
    /// The choice a persisted lobby's optional template alias stands for; `None` is the random board.
    #[must_use]
    pub fn from_stored(template: Option<&str>) -> Self {
        template.map_or(Self::Random, |alias| Self::Template {
            alias: alias.to_owned(),
        })
    }

    /// The optional alias a lobby record stores for this choice.
    #[must_use]
    pub fn stored(&self) -> Option<String> {
        match self {
            Self::Template { alias } => Some(alias.clone()),
            Self::Random => None,
        }
    }
}

/// Public description of a lobby's map choice. Deliberately has no seed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct MapChoiceView {
    pub kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub alias: Option<String>,
    /// Display name of a template that has one (community layouts).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub author: Option<String>,
    pub systems: usize,
    pub hyperlanes: bool,
    pub recommended: bool,
}

/// The place of one seat on the previewed map. It names no faction: nobody knows before Start
/// who sits where, and which faction a seat without a choice gets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct SeatPreview {
    /// 1-based number of the place, in the seat order of the map.
    pub seat: usize,
    /// The id of the tile of [`MapPreview::tiles`] that stands for the home of this seat
    /// (`seat_1`, …). It is not a system of the game.
    pub home_system_id: String,
}

/// A pictured map: the tiles a game with this choice has, and where each seat sits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct MapPreview {
    pub choice: MapChoice,
    pub player_count: usize,
    pub tiles: Vec<BoardTileView>,
    pub seats: Vec<SeatPreview>,
}

/// Seat order stand-ins; the map depends on seat order only, never on who sits where.
fn placeholder_players(count: usize) -> Vec<PlayerId> {
    (1..=count)
        .map(|i| PlayerId::new(format!("p{i}")))
        .collect()
}

/// The id of the tile that stands for the home of the seat at `seat` (1-based) in a preview.
#[must_use]
pub fn seat_place_id(seat: usize) -> String {
    format!("seat_{seat}")
}

/// The map a game of `player_count` seats gets for `choice` and `seed`, with a place for each
/// seat where a home system will be.
///
/// The board is built with stand-in factions (the fixed table: the in-scope factions in order,
/// then the two extra seats, `seating::seat_faction`), and each home is then sent as a place: a
/// tile with no planets, at the position of the home. Every tile that is not a home is the tile
/// of the started game, whichever factions sit (`a_preview_is_the_board_of_the_game_but_for_its_homes`).
/// A faction that brings a tile of its own (the Ghosts of Creuss) adds it at Start.
///
/// # Errors
/// A message when the template is unknown, seats another number of players, or does not build.
pub fn preview(choice: &MapChoice, player_count: usize, seed: u64) -> Result<MapPreview, String> {
    let content = ContentStore::embedded();
    let factions: Vec<ti4_model::id::FactionId> = (0..player_count)
        .map(|seat| ti4_model::id::FactionId::new(seating::seat_faction(seat)))
        .collect();
    let players = placeholder_players(player_count);
    let assignments: std::collections::BTreeMap<PlayerId, ti4_model::id::FactionId> = players
        .iter()
        .cloned()
        .zip(factions.iter().cloned())
        .collect();
    let (_, galaxy) = crate::map::create_game_with_template_seated(
        content,
        &players,
        seed,
        match choice {
            MapChoice::Template { alias } => Some(alias.as_str()),
            MapChoice::Random => None,
        },
        &assignments,
        None,
    )?;
    Ok(pictured(
        choice.clone(),
        crate::map::build_board_tiles(content, &galaxy),
        &factions,
    ))
}

/// The board of the game that the scenario `id` of `crate::view::scenario` starts, with the homes
/// of its seats as places, for a table of the seed `seed`. `choice` is what the table says of its
/// map.
///
/// # Errors
/// A message for an unknown scenario, or when its map does not build.
pub fn scenario_preview(choice: &MapChoice, id: &str, seed: u64) -> Result<MapPreview, String> {
    let content = ContentStore::embedded();
    let players = crate::view::scenario::find(id)
        .ok_or_else(|| format!("no scenario '{id}'"))?
        .players;
    let (state, galaxy) =
        crate::view::scenario::start(content, id, &placeholder_players(players), seed)?;
    let factions: Vec<ti4_model::id::FactionId> = state
        .players
        .iter()
        .map(|seat| seat.faction.clone())
        .collect();
    Ok(pictured(
        choice.clone(),
        crate::map::build_board_tiles(content, &galaxy),
        &factions,
    ))
}

/// Turns the home of each seated faction into the place of a seat.
fn pictured(
    choice: MapChoice,
    mut tiles: Vec<BoardTileView>,
    factions: &[ti4_model::id::FactionId],
) -> MapPreview {
    let content = ContentStore::embedded();
    let mut seats = Vec::with_capacity(factions.len());
    for (index, faction) in factions.iter().enumerate() {
        let home = ti4_content::factions::get(content, faction.as_str())
            .and_then(|f| f.home_system().map(str::to_owned))
            .unwrap_or_default();
        let place = seat_place_id(index + 1);
        if let Some(tile) = tiles.iter_mut().find(|tile| tile.system_id == home) {
            *tile = BoardTileView {
                system_id: place.clone(),
                label: format!("Seat {}", index + 1),
                q: tile.q,
                r: tile.r,
                hyperlane: false,
                special_area: tile.special_area.clone(),
                anomalies: Vec::new(),
                wormholes: Vec::new(),
                egress: false,
                planets: Vec::new(),
            };
        }
        seats.push(SeatPreview {
            seat: index + 1,
            home_system_id: place,
        });
    }
    MapPreview {
        player_count: factions.len(),
        choice,
        tiles,
        seats,
    }
}

/// Checks a choice for a lobby of `player_count` seats by building it once.
///
/// # Errors
/// The same message [`preview`] reports.
pub fn validate(choice: &MapChoice, player_count: usize) -> Result<(), String> {
    preview(choice, player_count, 0).map(|_| ())
}

/// A repeatable seed for the picture of variant `variant` of a card, mixed with `salt`. A server
/// uses a per-process secret so the numbers cannot be walked to learn anything about a real
/// game's seed; a browser host has no secret to keep and passes any number.
#[must_use]
pub fn variant_seed_salted(
    salt: u64,
    choice: &MapChoice,
    player_count: usize,
    variant: u32,
) -> u64 {
    let mut x = salt ^ u64::from(variant).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    x ^= player_count as u64;
    if let MapChoice::Template { alias } = choice {
        for b in alias.bytes() {
            x = (x ^ u64::from(b)).wrapping_mul(0x0100_0000_01B3);
        }
    }
    // splitmix64 finaliser
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

type ShapeKey = (Option<String>, usize);
type ShapeCache = Mutex<BTreeMap<ShapeKey, Option<(usize, bool)>>>;

/// Cached `(systems, hyperlanes)` of a template or random board at a size, or `None` if it does
/// not build. The shape does not depend on the seed.
fn shape(choice: &MapChoice, player_count: usize) -> Option<(usize, bool)> {
    static CACHE: OnceLock<ShapeCache> = OnceLock::new();
    let key = (choice.stored(), player_count);
    let cache = CACHE.get_or_init(Mutex::default);
    if let Some(hit) = cache.lock().expect("shape cache").get(&key) {
        return *hit;
    }
    let computed = preview(choice, player_count, 0).ok().map(|p| {
        let main = p.tiles.iter().filter(|t| t.special_area.is_none());
        let systems = main.clone().filter(|t| !t.hyperlane).count();
        (systems, main.clone().any(|t| t.hyperlane))
    });
    cache.lock().expect("shape cache").insert(key, computed);
    computed
}

/// The template a lobby of `player_count` seats starts with when nobody chooses (cached).
#[must_use]
pub fn recommended_for(player_count: usize) -> Option<String> {
    static CACHE: OnceLock<Mutex<BTreeMap<usize, Option<String>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Mutex::default);
    if let Some(hit) = cache.lock().expect("default cache").get(&player_count) {
        return hit.clone();
    }
    let found = TemplateLoader::load().ok().and_then(|loader| {
        super::default_template_for(ContentStore::embedded(), &loader, player_count, POK)
    });
    cache
        .lock()
        .expect("default cache")
        .insert(player_count, found.clone());
    found
}

/// The templates for `player_count` that build, or every template (with `buildable` telling
/// them apart) when no count is given. The recommended one is flagged.
///
/// # Errors
/// A message when the template data cannot be loaded.
pub fn catalog(player_count: Option<usize>) -> Result<Vec<MapTemplateSummary>, String> {
    let loader = TemplateLoader::load()?;
    let mut out = Vec::new();
    for mut summary in loader.list_summaries() {
        if player_count.is_some_and(|n| n != summary.player_count) {
            continue;
        }
        let choice = MapChoice::Template {
            alias: summary.alias.clone(),
        };
        if let Some((systems, hyperlanes)) = shape(&choice, summary.player_count) {
            summary.buildable = true;
            summary.systems = systems;
            summary.hyperlanes = hyperlanes;
        }
        if player_count.is_some() && !summary.buildable {
            continue;
        }
        out.push(summary);
    }
    if let Some(n) = player_count
        && let Some(default) = recommended_for(n)
        && let Some(entry) = out.iter_mut().find(|s| s.alias == default)
    {
        entry.recommended = true;
    }
    Ok(out)
}

/// The public face of `choice` in a lobby of `player_count` seats.
#[must_use]
pub fn describe(choice: &MapChoice, player_count: usize) -> MapChoiceView {
    let (systems, hyperlanes) = shape(choice, player_count).unwrap_or((0, false));
    match choice {
        MapChoice::Random => MapChoiceView {
            kind: "random",
            alias: None,
            name: None,
            author: None,
            systems,
            hyperlanes,
            recommended: false,
        },
        MapChoice::Template { alias } => {
            let loader = TemplateLoader::load().ok();
            let template = loader.as_ref().and_then(|l| l.get(alias));
            let author = template.map(|t| t.author.clone()).filter(|a| !a.is_empty());
            let name = template.and_then(|t| t.name.clone());
            let recommended = recommended_for(player_count).as_deref() == Some(alias.as_str());
            MapChoiceView {
                kind: "template",
                alias: Some(alias.clone()),
                name,
                author,
                systems,
                hyperlanes,
                recommended,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The tiles of the game that a table of these factions starts on `choice` and `seed`.
    fn started(choice: &MapChoice, seed: u64, factions: &[&str]) -> Vec<BoardTileView> {
        let content = ContentStore::embedded();
        let players = placeholder_players(factions.len());
        let assignments: BTreeMap<PlayerId, ti4_model::id::FactionId> = players
            .iter()
            .cloned()
            .zip(factions.iter().map(|f| ti4_model::id::FactionId::new(*f)))
            .collect();
        let (_, galaxy) = crate::map::create_game_with_template_seated(
            content,
            &players,
            seed,
            choice.stored().as_deref(),
            &assignments,
            None,
        )
        .expect("the game builds");
        crate::map::build_board_tiles(content, &galaxy)
    }

    #[test]
    fn a_preview_names_no_faction_and_its_homes_are_places() {
        let pictured = preview(&MapChoice::Random, 6, 3).unwrap();
        assert_eq!(pictured.seats.len(), 6);
        for seat in &pictured.seats {
            let tile = pictured
                .tiles
                .iter()
                .find(|tile| tile.system_id == seat.home_system_id)
                .expect("the place of the seat is a tile");
            assert!(tile.planets.is_empty());
            assert_eq!(tile.label, format!("Seat {}", seat.seat));
        }
        let json = serde_json::to_string(&pictured).unwrap();
        assert!(!json.contains("faction"), "{json}");
        // No home system of a stand-in faction is left on the board.
        for seat in 0..6 {
            let home =
                ti4_content::factions::get(ContentStore::embedded(), seating::seat_faction(seat))
                    .and_then(|f| f.home_system())
                    .unwrap();
            assert!(pictured.tiles.iter().all(|tile| tile.system_id != home));
        }
    }

    /// Which factions sit where changes the homes and nothing else, so the preview can be made
    /// before the seats are drawn. The Ghosts of Creuss are at every table: their home is off
    /// the map and a gate stands in its place.
    #[test]
    fn a_preview_is_the_board_of_the_game_but_for_its_homes() {
        let on_map = |tile: &&BoardTileView| tile.special_area.is_none();
        for (choice, count) in [
            (
                MapChoice::Template {
                    alias: "4pHyperlanes".to_owned(),
                },
                4,
            ),
            (MapChoice::Random, 4),
            (MapChoice::from_stored(recommended_for(6).as_deref()), 6),
            (MapChoice::Random, 8),
        ] {
            for seed in [2, 17] {
                let pictured = preview(&choice, count, seed).unwrap();
                // Another table each time: the ghosts and a run of the lobby factions.
                #[expect(clippy::cast_possible_truncation, reason = "a small test seed")]
                let offset = seed as usize;
                let mut factions = vec!["ghost"];
                factions.extend(
                    seating::LOBBY_FACTIONS
                        .iter()
                        .copied()
                        .filter(|faction| *faction != "ghost")
                        .cycle()
                        .skip(offset)
                        .take(count - 1),
                );
                factions.rotate_left(offset % count);
                let game = started(&choice, seed, &factions);
                assert_eq!(
                    pictured.tiles.iter().filter(on_map).count(),
                    game.iter().filter(on_map).count(),
                    "{choice:?} {seed}: the main map has the same number of tiles"
                );
                for tile in &pictured.tiles {
                    if pictured
                        .seats
                        .iter()
                        .any(|seat| seat.home_system_id == tile.system_id)
                    {
                        assert!(
                            game.iter()
                                .filter(on_map)
                                .any(|other| (other.q, other.r) == (tile.q, tile.r)),
                            "{choice:?} {seed}: the game has no tile at {}",
                            tile.label
                        );
                    } else {
                        assert!(
                            game.contains(tile),
                            "{choice:?} {seed}: the game has another tile for {}",
                            tile.system_id
                        );
                    }
                }
            }
        }
    }
}
