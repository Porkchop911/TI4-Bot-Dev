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
}
