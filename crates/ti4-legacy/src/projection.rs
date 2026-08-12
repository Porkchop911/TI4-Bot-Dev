//! Canonical public-state projection for native differential fixtures.
//!
//! This mirrors the executable source public-state schema.
//! It deliberately exposes only public card counts, never action-card or secret-objective identities.

use std::collections::BTreeMap;

use serde_json::{Value, json};
use ti4_model::id::{PlanetId, PlayerId, RelicId, StrategyCardId, SystemId, TechnologyId};
use ti4_model::state::{GameState, Player, SystemState};
use ti4_model::units::Unit;

/// Project a native game state into the oracle's canonical public-state schema.
///
/// Map keys are held in ordered maps, so the compact JSON encoding is deterministic.
#[must_use]
pub fn state_projection(state: &GameState) -> Value {
    let initiative_order = state.initiative_order();
    json!({
        "type": "state",
        "turn": state.turn_seq,
        "round": state.round,
        "phase": state.phase,
        "active_player": state.active.as_ref().map(PlayerId::as_str),
        "speaker": state.speaker.as_str(),
        "seating_order": ids(&state.seating_order),
        "initiative_order": ids(&initiative_order),
        "players": initiative_order
            .iter()
            .filter_map(|id| state.player(id))
            .map(|player| player_projection(player, state))
            .collect::<Vec<_>>(),
        "systems": state.board.iter()
            .map(|(id, system)| system_projection(id, system))
            .collect::<Vec<_>>(),
        "unclaimed_strategy_cards": state.unclaimed_strategy_cards.iter()
            .map(StrategyCardId::as_str)
            .collect::<Vec<_>>(),
        "strategy_card_goods": integer_map(&state.strategy_card_goods),
        "agenda": {
            "custodians_removed": state.custodians_removed,
            "laws": string_map(&state.laws),
        },
        "game_over": state.finished,
    })
}

/// Encode a projection as compact, deterministic UTF-8 JSON.
///
/// # Panics
///
/// The projection contains only JSON-native values, so serialization cannot fail.
#[must_use]
pub fn canonical_state_bytes(state: &GameState) -> Vec<u8> {
    serde_json::to_vec(&state_projection(state)).expect("native state projection is JSON")
}

/// First deterministic difference between a source public-state projection and a native state.
#[derive(Debug, Clone, PartialEq)]
pub struct StateProjectionDifference {
    /// JSONPath-like location of the first mismatched value.
    pub path: String,
    /// Value emitted by the pinned source projection.
    pub expected: Value,
    /// Value emitted by the native projection.
    pub actual: Value,
}

/// Compare a native state with a checked source public-state projection.
///
/// Collection order is significant because it is part of the source projection contract. Map
/// keys are compared in their deterministic serialized order, so the first reported mismatch is
/// reproducible and suitable for fixture evidence.
#[must_use]
pub fn first_state_projection_difference(
    state: &GameState,
    expected: &Value,
) -> Option<StateProjectionDifference> {
    first_difference(expected, &state_projection(state), "$")
}

fn first_difference(
    expected: &Value,
    actual: &Value,
    path: &str,
) -> Option<StateProjectionDifference> {
    match (expected, actual) {
        (Value::Object(expected_object), Value::Object(actual_object)) => {
            for (key, expected_value) in expected_object {
                let key_path = format!("{path}[{key:?}]");
                let Some(actual_value) = actual_object.get(key) else {
                    return Some(StateProjectionDifference {
                        path: key_path,
                        expected: expected_value.clone(),
                        actual: Value::Null,
                    });
                };
                if let Some(difference) = first_difference(expected_value, actual_value, &key_path)
                {
                    return Some(difference);
                }
            }
            for (key, actual_value) in actual_object {
                if !expected_object.contains_key(key) {
                    return Some(StateProjectionDifference {
                        path: format!("{path}[{key:?}]"),
                        expected: Value::Null,
                        actual: actual_value.clone(),
                    });
                }
            }
            None
        }
        (Value::Array(expected_array), Value::Array(actual_array)) => {
            for (index, expected_value) in expected_array.iter().enumerate() {
                let Some(actual_value) = actual_array.get(index) else {
                    return Some(StateProjectionDifference {
                        path: format!("{path}[{index}]"),
                        expected: expected_value.clone(),
                        actual: Value::Null,
                    });
                };
                if let Some(difference) =
                    first_difference(expected_value, actual_value, &format!("{path}[{index}]"))
                {
                    return Some(difference);
                }
            }
            actual_array
                .iter()
                .enumerate()
                .nth(expected_array.len())
                .map(|(index, actual_value)| StateProjectionDifference {
                    path: format!("{path}[{index}]"),
                    expected: Value::Null,
                    actual: actual_value.clone(),
                })
        }
        _ if expected == actual => None,
        _ => Some(StateProjectionDifference {
            path: path.to_owned(),
            expected: expected.clone(),
            actual: actual.clone(),
        }),
    }
}

fn player_projection(player: &Player, state: &GameState) -> Value {
    json!({
        "id": player.id.as_str(),
        "faction": player.faction.as_str(),
        "vp": player.victory_points,
        "techs": player.technologies.iter().map(TechnologyId::as_str).collect::<Vec<_>>(),
        "command_tokens": {
            "tactic": player.tactic_tokens,
            "fleet": player.fleet_tokens,
            "strategic": player.strategic_tokens,
        },
        "trade_goods": player.trade_goods,
        "commodities": player.commodities,
        "home_system": player.home_system.as_ref().map(SystemId::as_str),
        "home_planets": player.home_planets.iter().map(PlanetId::as_str).collect::<Vec<_>>(),
        "controlled_planets": controlled_planets(state, &player.id),
        "strategy_cards": player.strategy_cards.iter().map(StrategyCardId::as_str).collect::<Vec<_>>(),
        "exhausted_strategy_cards": player.exhausted_strategy_cards.iter()
            .map(StrategyCardId::as_str)
            .collect::<Vec<_>>(),
        "passed": player.passed,
        "action_cards_count": player.action_cards.len(),
        "secret_objectives_count": player.secret_objectives.len(),
        "relics": player.relics.iter().map(RelicId::as_str).collect::<Vec<_>>(),
    })
}

fn controlled_planets(state: &GameState, player: &PlayerId) -> Vec<String> {
    state
        .board
        .values()
        .flat_map(|system| system.planet_control.iter())
        .filter(|(_, owner)| *owner == player)
        .map(|(planet, _)| planet.as_str().to_owned())
        .collect()
}

fn system_projection(id: &SystemId, system: &SystemState) -> Value {
    json!({
        "id": id.as_str(),
        "units": units_projection(&system.units),
        "command_tokens": ids(&system.command_tokens.iter().cloned().collect::<Vec<_>>()),
        "planet_control": player_map(&system.planet_control),
        "planet_units": system.planet_units.iter()
            .map(|(planet, units)| (planet.as_str().to_owned(), units_projection(units)))
            .collect::<BTreeMap<_, _>>(),
    })
}

fn units_projection(units: &[Unit]) -> Vec<Value> {
    let mut projected = units
        .iter()
        .map(|unit| {
            json!({
                "type_id": unit.type_id.as_str(),
                "owner": unit.owner.as_str(),
                "damage": unit.sustained_damage,
            })
        })
        .collect::<Vec<_>>();
    projected.sort_by_key(|value| {
        let object = value.as_object().expect("unit projection is an object");
        (
            object["owner"]
                .as_str()
                .expect("owner is a string")
                .to_owned(),
            object["type_id"]
                .as_str()
                .expect("type is a string")
                .to_owned(),
            object["damage"].as_bool().expect("damage is a boolean"),
        )
    });
    projected
}

fn ids(ids: &[PlayerId]) -> Vec<&str> {
    ids.iter().map(PlayerId::as_str).collect()
}

fn integer_map<K: ToString>(map: &BTreeMap<K, i32>) -> BTreeMap<String, i32> {
    map.iter()
        .map(|(key, value)| (key.to_string(), *value))
        .collect()
}

fn string_map(map: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    map.clone()
}

fn player_map(map: &BTreeMap<PlanetId, PlayerId>) -> BTreeMap<String, String> {
    map.iter()
        .map(|(planet, player)| (planet.as_str().to_owned(), player.as_str().to_owned()))
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use serde_json::json;
    use ti4_model::id::{
        ActionCardId, FactionId, PlanetId, PlayerId, SecretObjectiveId, SystemId, TechnologyId,
        UnitTypeId,
    };
    use ti4_model::state::{GameState, Phase, SystemState};
    use ti4_model::units::Unit;

    use super::*;

    fn state() -> GameState {
        let seats = vec![PlayerId::new("b"), PlayerId::new("a")];
        let mut state = GameState::new(&seats, &[], BTreeMap::new(), None, 1);
        state.phase = Phase::Action;
        state.round = 3;
        state.turn_seq = 7;
        state.active = Some(PlayerId::new("a"));
        state.players[0].faction = FactionId::new("hacan");
        state.players[0].home_system = Some(SystemId::new("30"));
        state.players[0].home_planets = vec![PlanetId::new("arretze")];
        state.players[0].victory_points = 4;
        state.players[0].technologies = BTreeSet::from([TechnologyId::new("antimass")]);
        state.players[0].trade_goods = 3;
        state.players[0].commodities = 2;
        state.players[0].action_cards = vec![ActionCardId::new("private_action")];
        state.players[0].secret_objectives = vec![SecretObjectiveId::new("private_secret")];

        let mut system = SystemState {
            units: vec![
                Unit::new(UnitTypeId::new("cruiser"), PlayerId::new("b")),
                Unit::new(UnitTypeId::new("fighter"), PlayerId::new("a")).sustained(),
            ],
            command_tokens: BTreeSet::from([PlayerId::new("b"), PlayerId::new("a")]),
            ..SystemState::default()
        };
        system
            .planet_control
            .insert(PlanetId::new("arretze"), PlayerId::new("b"));
        system.planet_units.insert(
            PlanetId::new("arretze"),
            vec![Unit::new(UnitTypeId::new("infantry"), PlayerId::new("b"))],
        );
        state.board.insert(SystemId::new("18"), system);
        state
            .laws
            .insert("fleet_regulations".to_owned(), "for".to_owned());
        state
    }

    #[test]
    fn projection_matches_the_oracle_public_schema_and_redacts_private_cards() {
        let state = state();
        let projection = state_projection(&state);

        assert_eq!(projection["type"], "state");
        assert_eq!(projection["turn"], 7);
        assert_eq!(projection["phase"], "action");
        assert_eq!(projection["active_player"], "a");
        assert_eq!(
            projection["agenda"],
            json!({"custodians_removed": false, "laws": {"fleet_regulations": "for"}})
        );
        assert_eq!(
            projection["players"][0],
            json!({
                "id": "b", "faction": "hacan", "vp": 4, "techs": ["antimass"],
                "command_tokens": {"tactic": 3, "fleet": 3, "strategic": 2},
                "trade_goods": 3, "commodities": 2, "home_system": "30",
                "home_planets": ["arretze"], "controlled_planets": ["arretze"],
                "strategy_cards": [], "exhausted_strategy_cards": [], "passed": false,
                "action_cards_count": 1, "secret_objectives_count": 1, "relics": [],
            })
        );
        assert_eq!(
            projection["systems"][0]["units"],
            json!([
                {"type_id": "fighter", "owner": "a", "damage": true},
                {"type_id": "cruiser", "owner": "b", "damage": false},
            ])
        );

        let bytes = String::from_utf8(canonical_state_bytes(&state)).unwrap();
        assert!(!bytes.contains("private_action"));
        assert!(!bytes.contains("private_secret"));
    }

    #[test]
    fn canonical_bytes_are_stable_across_repeated_projections() {
        let state = state();

        assert_eq!(canonical_state_bytes(&state), canonical_state_bytes(&state));
        assert_eq!(
            serde_json::from_slice::<Value>(&canonical_state_bytes(&state)).unwrap(),
            state_projection(&state)
        );
    }

    #[test]
    fn comparison_reports_the_first_nested_difference_with_a_stable_path() {
        let state = state();
        let mut expected = state_projection(&state);
        expected["players"][0]["vp"] = json!(99);

        assert_eq!(
            first_state_projection_difference(&state, &expected),
            Some(StateProjectionDifference {
                path: "$[\"players\"][0][\"vp\"]".to_owned(),
                expected: json!(99),
                actual: json!(4),
            })
        );
    }

    #[test]
    fn comparison_detects_missing_and_extra_fields() {
        let state = state();
        let mut expected = state_projection(&state);
        expected["agenda"]["unmodeled"] = json!(true);

        assert_eq!(
            first_state_projection_difference(&state, &expected),
            Some(StateProjectionDifference {
                path: "$[\"agenda\"][\"unmodeled\"]".to_owned(),
                expected: json!(true),
                actual: Value::Null,
            })
        );
    }
}
