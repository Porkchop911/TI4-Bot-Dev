//! Public-only source-state import for the bounded replay boundary.

use std::collections::{BTreeMap, BTreeSet};

use serde::Deserialize;
use serde_json::Value;
use ti4_model::id::{
    ActionCardId, FactionId, PlanetId, PlayerId, RelicId, SecretObjectiveId, StrategyCardId,
    SystemId, TechnologyId, UnitTypeId,
};
use ti4_model::state::{GameState, Phase, SystemState};
use ti4_model::units::Unit;

use crate::projection::first_state_projection_difference;

/// A source public-state snapshot cannot be represented safely by the current native model.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PublicStateImportError {
    #[error("invalid source public state: {0}")]
    Schema(String),
    #[error("source public state is not safely importable: {0}")]
    Unsupported(String),
    #[error("imported public state differs at {path}")]
    ProjectionMismatch { path: String },
}

#[derive(Debug, Deserialize)]
struct Snapshot {
    #[serde(rename = "type")]
    record_type: String,
    turn: u32,
    round: u32,
    phase: Phase,
    active_player: Option<PlayerId>,
    speaker: PlayerId,
    seating_order: Vec<PlayerId>,
    initiative_order: Vec<PlayerId>,
    players: Vec<SnapshotPlayer>,
    systems: Vec<SnapshotSystem>,
    unclaimed_strategy_cards: Vec<StrategyCardId>,
    strategy_card_goods: BTreeMap<StrategyCardId, i32>,
    agenda: SnapshotAgenda,
    game_over: bool,
}

#[derive(Debug, Deserialize)]
struct SnapshotPlayer {
    id: PlayerId,
    faction: FactionId,
    vp: i32,
    techs: BTreeSet<TechnologyId>,
    command_tokens: SnapshotTokens,
    trade_goods: i32,
    commodities: i32,
    home_system: Option<SystemId>,
    home_planets: Vec<PlanetId>,
    controlled_planets: Vec<PlanetId>,
    strategy_cards: Vec<StrategyCardId>,
    exhausted_strategy_cards: BTreeSet<StrategyCardId>,
    passed: bool,
    action_cards_count: usize,
    secret_objectives_count: usize,
    relics: Vec<RelicId>,
}

#[derive(Debug, Deserialize)]
struct SnapshotTokens {
    tactic: i32,
    fleet: i32,
    strategic: i32,
}

#[derive(Debug, Deserialize)]
struct SnapshotSystem {
    id: SystemId,
    units: Vec<SnapshotUnit>,
    command_tokens: BTreeSet<PlayerId>,
    planet_control: BTreeMap<PlanetId, PlayerId>,
    planet_units: BTreeMap<PlanetId, Vec<SnapshotUnit>>,
}

#[derive(Debug, Deserialize)]
struct SnapshotUnit {
    type_id: UnitTypeId,
    owner: PlayerId,
    damage: bool,
}

#[derive(Debug, Deserialize)]
struct SnapshotAgenda {
    custodians_removed: bool,
    laws: BTreeMap<String, String>,
}

/// Import an initial bounded source public-state snapshot into a native state.
///
/// Private card identities become opaque placeholders. The result is suitable only for public
/// projection comparison, never for driving a native game.
///
/// # Errors
///
/// Returns [`PublicStateImportError`] for malformed source JSON, unsupported strategy holdings, or
/// any difference between the reconstructed and source public projections.
pub fn import_initial_public_state(snapshot: &Value) -> Result<GameState, PublicStateImportError> {
    let expected = snapshot.clone();
    let snapshot = serde_json::from_value::<Snapshot>(snapshot.clone())
        .map_err(|error| PublicStateImportError::Schema(error.to_string()))?;
    validate_snapshot(&snapshot)?;

    let mut state = GameState::new(
        &snapshot.seating_order,
        &snapshot.unclaimed_strategy_cards,
        BTreeMap::new(),
        Some(snapshot.speaker.clone()),
        if snapshot.seating_order.len() <= 4 {
            2
        } else {
            1
        },
    );
    state.phase = snapshot.phase;
    state.round = snapshot.round;
    state.turn_seq = snapshot.turn;
    state.active = snapshot.active_player;
    state.strategy_card_goods = snapshot.strategy_card_goods;
    state.custodians_removed = snapshot.agenda.custodians_removed;
    state.laws = snapshot.agenda.laws;
    state.finished = snapshot.game_over;

    for source in snapshot.players {
        populate_player(&mut state, source)?;
    }
    for source in snapshot.systems {
        state.board.insert(
            source.id,
            SystemState {
                units: source.units.into_iter().map(import_unit).collect(),
                command_tokens: source.command_tokens,
                planet_control: source.planet_control,
                planet_units: source
                    .planet_units
                    .into_iter()
                    .map(|(planet, units)| (planet, units.into_iter().map(import_unit).collect()))
                    .collect(),
                // The oracle snapshot format predates Thunder's Edge coexistence and carries no
                // record of it, so an imported position has nobody coexisting anywhere.
                coexisting: std::collections::BTreeMap::new(),
            },
        );
    }
    if let Some(difference) = first_state_projection_difference(&state, &expected) {
        return Err(PublicStateImportError::ProjectionMismatch {
            path: difference.path,
        });
    }
    Ok(state)
}

fn validate_snapshot(snapshot: &Snapshot) -> Result<(), PublicStateImportError> {
    if snapshot.record_type != "state" {
        return Err(PublicStateImportError::Schema(
            "record type is not state".to_owned(),
        ));
    }
    let player_ids = snapshot
        .players
        .iter()
        .map(|player| &player.id)
        .collect::<Vec<_>>();
    if snapshot.seating_order.len() < 2
        || snapshot.seating_order.iter().collect::<BTreeSet<_>>().len()
            != snapshot.seating_order.len()
        || player_ids != snapshot.seating_order.iter().collect::<Vec<_>>()
    {
        return Err(PublicStateImportError::Schema(
            "seating order and players disagree".to_owned(),
        ));
    }
    if snapshot
        .players
        .iter()
        .any(|player| !player.strategy_cards.is_empty())
        || snapshot.initiative_order != snapshot.seating_order
    {
        return Err(PublicStateImportError::Unsupported(
            "held strategy cards need omitted initiative metadata".to_owned(),
        ));
    }

    Ok(())
}

fn populate_player(
    state: &mut GameState,
    source: SnapshotPlayer,
) -> Result<(), PublicStateImportError> {
    let player = state
        .player_mut(&source.id)
        .ok_or_else(|| PublicStateImportError::Schema("unknown player".to_owned()))?;
    player.faction = source.faction;
    player.victory_points = source.vp;
    player.technologies = source.techs;
    player.tactic_tokens = source.command_tokens.tactic;
    player.fleet_tokens = source.command_tokens.fleet;
    player.strategic_tokens = source.command_tokens.strategic;
    player.trade_goods = source.trade_goods;
    player.commodities = source.commodities;
    player.home_system = source.home_system;
    player.home_planets = source.home_planets;
    player.exhausted_strategy_cards = source.exhausted_strategy_cards;
    player.passed = source.passed;
    player.relics = source.relics;
    player.action_cards = opaque_ids(
        "legacy-action",
        source.action_cards_count,
        ActionCardId::new,
    );
    player.secret_objectives = opaque_ids(
        "legacy-secret",
        source.secret_objectives_count,
        SecretObjectiveId::new,
    );
    let _ = source.controlled_planets;
    Ok(())
}

fn opaque_ids<T>(prefix: &str, count: usize, make: impl Fn(String) -> T) -> Vec<T> {
    (0..count)
        .map(|index| make(format!("{prefix}-{index}")))
        .collect()
}

fn import_unit(unit: SnapshotUnit) -> Unit {
    let damage = unit.damage;
    let native = Unit::new(unit.type_id, unit.owner);
    if damage { native.sustained() } else { native }
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};

    use crate::source_trace::parse_source_trace_states;
    use serde_json::json;
    use ti4_content::ContentStore;
    use ti4_engine::{Game, GameError, IllegalChoice, Scripted, Table};

    use super::*;

    #[test]
    fn imports_the_retained_initial_snapshot_without_public_difference() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/legacy_entropy/bounded-v1/trace-001.ndjson");
        let trace = parse_source_trace_states(&std::fs::read_to_string(path).unwrap()).unwrap();

        assert!(import_initial_public_state(&trace.initial).is_ok());
    }

    #[test]
    fn imports_every_retained_initial_snapshot_without_public_difference() {
        let root =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/legacy_entropy/bounded-v1");
        let mut paths = fs::read_dir(root)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| {
                path.extension()
                    .is_some_and(|extension| extension == "ndjson")
            })
            .collect::<Vec<_>>();
        paths.sort();

        assert_eq!(paths.len(), 100);
        for path in paths {
            let trace = parse_source_trace_states(&fs::read_to_string(&path).unwrap()).unwrap();
            assert!(
                import_initial_public_state(&trace.initial).is_ok(),
                "failed to import {}",
                path.display()
            );
        }
    }

    #[test]
    fn rejects_held_strategy_cards_without_their_omitted_initiative_metadata() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/legacy_entropy/bounded-v1/trace-001.ndjson");
        let trace = parse_source_trace_states(&fs::read_to_string(path).unwrap()).unwrap();
        let mut snapshot = trace.initial;
        snapshot["players"][0]["strategy_cards"] = json!(["leadership"]);

        assert_eq!(
            import_initial_public_state(&snapshot),
            Err(PublicStateImportError::Unsupported(
                "held strategy cards need omitted initiative metadata".to_owned()
            ))
        );
    }

    #[test]
    fn retained_source_script_stops_at_the_first_unimplemented_action_option() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/legacy_entropy/bounded-v1/trace-001.ndjson");
        let trace = parse_source_trace_states(&fs::read_to_string(path).unwrap()).unwrap();
        let state = import_initial_public_state(&trace.initial).unwrap();
        let mut game = Game::with_table(
            state,
            ContentStore::embedded(),
            Table::with_default(Box::new(Scripted::new(trace.trace.decisions))),
        );

        let error = (0..10)
            .find_map(|_| game.step().error)
            .expect("source script must not be silently accepted");
        assert!(matches!(
            error,
            GameError::IllegalChoice(IllegalChoice::ScriptDiverged { ref wanted, .. })
                if wanted == "component|expedition|secret"
        ));
        assert_eq!(
            game.table.log.len(),
            6,
            "only the shared strategy picks applied"
        );
    }
}

// -- the board a replayed game was played on ---------------------------------------------------

/// A tile placement read from a bounded trace's map record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlacedTile {
    /// The system's id.
    pub system: String,
    /// Where it sat.
    pub position: ti4_model::Hex,
}

/// Why a map record could not be read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MapImportError {
    /// The trace carries no map record at all.
    ///
    /// Distinct from a map that says it is absent: an older corpus predates the record entirely,
    /// and reporting that as "the game had no board" would hide a stale fixture.
    #[error("bounded trace carries no map record; it predates the map schema")]
    Missing,
    /// The record is present and says the game had no board.
    #[error("the exported game was played without a board")]
    NoBoard,
    /// A tile entry is not in the map schema's shape.
    #[error("map tile {index} is malformed: {reason}")]
    MalformedTile {
        /// Which entry.
        index: usize,
        /// What was wrong with it.
        reason: String,
    },
    /// The placements do not form a legal galaxy.
    #[error("the placements are not a legal board: {0}")]
    Illegal(String),
}

/// Read the tile placements out of a bounded trace's records.
///
/// # Errors
/// [`MapImportError`] naming what was wrong.
pub fn read_map(records: &[Value]) -> Result<Vec<PlacedTile>, MapImportError> {
    let record = records
        .iter()
        .find(|record| record.get("type").and_then(Value::as_str) == Some("map"))
        .ok_or(MapImportError::Missing)?;
    if record.get("present").and_then(Value::as_bool) != Some(true) {
        return Err(MapImportError::NoBoard);
    }
    let tiles =
        record
            .get("tiles")
            .and_then(Value::as_array)
            .ok_or(MapImportError::MalformedTile {
                index: 0,
                reason: "the record carries no tile list".to_owned(),
            })?;

    tiles
        .iter()
        .enumerate()
        .map(|(index, tile)| {
            let read = |name: &str| -> Result<i32, MapImportError> {
                tile.get(name)
                    .and_then(Value::as_i64)
                    .and_then(|value| i32::try_from(value).ok())
                    .ok_or_else(|| MapImportError::MalformedTile {
                        index,
                        reason: format!("{name} is missing or not a coordinate"),
                    })
            };
            let system = tile.get("system").and_then(Value::as_str).ok_or_else(|| {
                MapImportError::MalformedTile {
                    index,
                    reason: "system is missing".to_owned(),
                }
            })?;
            Ok(PlacedTile {
                system: system.to_owned(),
                position: ti4_model::Hex {
                    q: read("q")?,
                    r: read("r")?,
                },
            })
        })
        .collect()
}

/// Rebuild the galaxy a replayed game was played on.
///
/// This is what lets a replay reach a tactical action at all. Without it the native engine is
/// offered no board, declines every activation as impossible, and diverges from the oracle's
/// script the first time it took one — which in a game of this is almost immediately.
///
/// `sources` is deliberately not the game's declared scope. A tile that the oracle placed is a
/// fact about the board it played on, and filtering the reconstruction by the scenario's declared
/// sources rejects it: half this corpus declares `base` and places a Thunder's Edge tile. Scoping
/// belongs to what a game may *offer*, not to what a replay may *observe*.
///
/// # Errors
/// [`MapImportError`] when the record is absent, malformed, or does not form a legal board.
pub fn import_map(
    records: &[Value],
    content: &ti4_content::ContentStore,
    sources: ti4_model::content_types::SourceSet,
) -> Result<ti4_content::galaxy::Galaxy, MapImportError> {
    let tiles = read_map(records)?;
    let placements: Vec<(&str, ti4_model::Hex)> = tiles
        .iter()
        .map(|tile| (tile.system.as_str(), tile.position))
        .collect();
    ti4_content::galaxy::Galaxy::placed(content, &placements, sources)
        .map_err(|error| MapImportError::Illegal(error.to_string()))
}

#[cfg(test)]
mod map_tests {
    use super::*;
    use std::fs;
    use std::path::Path;

    fn every_source() -> ti4_model::content_types::SourceSet {
        use ti4_model::content_types::Source;
        Source::Base
            | Source::Codex1
            | Source::Codex2
            | Source::Codex3
            | Source::Codex4
            | Source::Pok
            | Source::ThundersEdge
    }

    fn records(name: &str) -> Vec<Value> {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/legacy_entropy/bounded-v1")
            .join(name);
        fs::read_to_string(path)
            .expect("trace readable")
            .lines()
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect()
    }

    #[test]
    fn the_corpus_carries_the_board_each_game_was_played_on() {
        // Until it did, differential replay could never reach a tactical action: the native engine
        // offers none without a galaxy, so every trace diverged from the oracle's script the first
        // time it took one. `tactical` was the third commonest stop across the corpus and vanished
        // from the tally entirely once the board was carried.
        let tiles = read_map(&records("trace-001.ndjson")).expect("the trace carries a map");
        assert!(tiles.len() > 20, "a board, not a handful of tiles");
        assert!(
            tiles
                .iter()
                .any(|tile| tile.position == ti4_model::Hex::ORIGIN),
            "something sits at the centre"
        );
    }

    #[test]
    fn a_reconstructed_board_places_every_tile_where_the_oracle_had_it() {
        let content = ti4_content::ContentStore::embedded();
        let read = records("trace-001.ndjson");
        let tiles = read_map(&read).expect("map");
        let galaxy = import_map(&read, content, every_source()).expect("a legal board");

        for tile in &tiles {
            assert_eq!(
                galaxy.coord_of(&tile.system),
                Some(tile.position),
                "{} was rebuilt somewhere else",
                tile.system
            );
        }
    }

    #[test]
    fn every_trace_in_the_corpus_rebuilds_a_board() {
        // Half of them failed at first: the reconstruction was filtered by the scenario's declared
        // sources, and half this corpus declares `base` while placing a Thunder's Edge tile. A
        // tile the oracle placed is a fact about the board, not a scoping decision.
        let content = ti4_content::ContentStore::embedded();
        let dir =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/legacy_entropy/bounded-v1");
        let mut checked = 0;
        for entry in fs::read_dir(&dir).expect("corpus") {
            let path = entry.expect("entry").path();
            if path.extension().is_none_or(|ext| ext != "ndjson") {
                continue;
            }
            let read: Vec<Value> = fs::read_to_string(&path)
                .expect("readable")
                .lines()
                .filter_map(|line| serde_json::from_str(line).ok())
                .collect();
            import_map(&read, content, every_source()).unwrap_or_else(|error| {
                panic!("{} has no rebuildable board: {error}", path.display())
            });
            checked += 1;
        }
        assert_eq!(checked, 100, "the whole corpus was checked");
    }

    #[test]
    fn a_trace_without_a_map_record_says_so_rather_than_reporting_an_empty_board() {
        // An older corpus predates the record. Reporting that as "this game had no board" would
        // hide a stale fixture behind a plausible-looking answer.
        let nothing: Vec<Value> = vec![serde_json::json!({"type": "state"})];
        assert_eq!(read_map(&nothing), Err(MapImportError::Missing));

        let boardless: Vec<Value> =
            vec![serde_json::json!({"type": "map", "present": false, "tiles": []})];
        assert_eq!(read_map(&boardless), Err(MapImportError::NoBoard));
    }

    #[test]
    fn a_malformed_tile_is_refused_rather_than_placed_at_the_origin() {
        // A missing coordinate defaulting to zero would stack tiles on the centre and rebuild a
        // board that is legal, connected, and not the one the game was played on.
        let broken: Vec<Value> = vec![serde_json::json!({
            "type": "map",
            "present": true,
            "tiles": [{"system": "18", "q": 0}],
        })];
        assert!(matches!(
            read_map(&broken),
            Err(MapImportError::MalformedTile { index: 0, .. })
        ));
    }
}
