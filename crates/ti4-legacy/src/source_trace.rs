//! Checked public-state intake from a bounded source trace.
//!
//! The source state is retained as canonical JSON rather than prematurely coerced into native
//! `GameState`. A native state import needs the still-unimplemented hidden-deck and scenario
//! compatibility policy; this boundary preserves exactly what the source exporter made public.

use serde_json::Value;

use crate::converter::{BoundedTrace, BoundedTraceError, parse_bounded_trace};

/// A bounded trace together with its first and final source public-state snapshots.
#[derive(Debug, Clone, PartialEq)]
pub struct SourceTraceStates {
    /// Checked header, decision sequence, and dice entropy.
    pub trace: BoundedTrace,
    /// The public state observed before the bounded run begins.
    pub initial: Value,
    /// The public state observed after the bounded run ends.
    pub final_state: Value,
}

/// A bounded source trace has no usable public-state intake boundary.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SourceTraceStateError {
    /// The underlying bounded trace did not meet its pinned contract.
    #[error(transparent)]
    Trace(#[from] BoundedTraceError),
    /// A state record does not have the executable source public-state schema's required facts.
    #[error("line {line} is not a valid public state: {reason}")]
    InvalidState { line: usize, reason: String },
    /// The trace had fewer than its required initial/final public-state snapshots.
    #[error("bounded trace has {found} state snapshots, expected exactly two")]
    StateCount { found: usize },
}

/// Extract the two public state snapshots from a validated bounded trace.
///
/// The state records stay as JSON because the source schema intentionally redacts private
/// identities. They are inputs for later comparison and scenario/state translation, not evidence
/// that a complete native `GameState` can already be rebuilt.
///
/// # Errors
///
/// Returns [`SourceTraceStateError`] when the bounded trace itself is invalid, a public-state
/// schema boundary does not match the header, or the trace has anything other than two states.
pub fn parse_source_trace_states(input: &str) -> Result<SourceTraceStates, SourceTraceStateError> {
    let trace = parse_bounded_trace(input)?;
    let mut states = Vec::new();
    for (offset, line) in input.lines().enumerate() {
        let line_number = offset + 1;
        let record = serde_json::from_str::<Value>(line).map_err(|error| {
            SourceTraceStateError::InvalidState {
                line: line_number,
                reason: error.to_string(),
            }
        })?;
        if record["type"] == "state" {
            validate_state(&record, line_number, &trace)?;
            states.push(record);
        }
    }
    if states.len() != 2 {
        return Err(SourceTraceStateError::StateCount {
            found: states.len(),
        });
    }
    let mut states = states.into_iter();
    let initial = states
        .next()
        .ok_or(SourceTraceStateError::StateCount { found: 0 })?;
    let final_state = states
        .next()
        .ok_or(SourceTraceStateError::StateCount { found: 1 })?;
    Ok(SourceTraceStates {
        trace,
        initial,
        final_state,
    })
}

fn validate_state(
    state: &Value,
    line: usize,
    trace: &BoundedTrace,
) -> Result<(), SourceTraceStateError> {
    let object = state
        .as_object()
        .ok_or_else(|| invalid_state(line, "record is not an object"))?;
    for field in [
        "turn",
        "round",
        "phase",
        "speaker",
        "seating_order",
        "initiative_order",
        "players",
        "systems",
        "unclaimed_strategy_cards",
        "strategy_card_goods",
        "agenda",
        "game_over",
    ] {
        if !object.contains_key(field) {
            return Err(invalid_state(line, &format!("missing {field:?}")));
        }
    }
    if state["seating_order"]
        != Value::Array(trace.seats.iter().cloned().map(Value::String).collect())
    {
        return Err(invalid_state(line, "seating order differs from header"));
    }
    let player_ids = state["players"]
        .as_array()
        .ok_or_else(|| invalid_state(line, "players is not an array"))?
        .iter()
        .map(|player| player["id"].as_str().map(str::to_owned))
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| invalid_state(line, "player lacks string id"))?;
    if player_ids != trace.seats {
        return Err(invalid_state(
            line,
            "players are not in the header's seat order",
        ));
    }
    if state["round"].as_u64().is_none()
        || state["turn"].as_u64().is_none()
        || state["phase"].as_str().is_none()
        || state["speaker"].as_str().is_none()
        || state["systems"].as_array().is_none()
        || state["unclaimed_strategy_cards"].as_array().is_none()
        || state["strategy_card_goods"].as_object().is_none()
        || state["agenda"].as_object().is_none()
        || state["game_over"].as_bool().is_none()
    {
        return Err(invalid_state(line, "state field has an invalid JSON type"));
    }
    Ok(())
}

fn invalid_state(line: usize, reason: &str) -> SourceTraceStateError {
    SourceTraceStateError::InvalidState {
        line,
        reason: reason.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    const TRACE: &str = concat!(
        r#"{"export_scope":"bounded_game","oracle_commit":"37061c511a4780d4c0719e0342533a498cd4b457","rounds":1,"scenario":"save54_base","schema_version":"1.0.0","seats":["sol","letnev"],"seed":7,"sources":"base","type":"header"}"#,
        "\n",
        r#"{"active_player":null,"agenda":{"custodians_removed":false,"laws":{}},"game_over":false,"initiative_order":["sol","letnev"],"phase":"strategy","players":[{"id":"sol"},{"id":"letnev"}],"round":1,"seating_order":["sol","letnev"],"speaker":"sol","strategy_card_goods":{},"systems":[],"turn":0,"type":"state","unclaimed_strategy_cards":[]}"#,
        "\n",
        r#"{"options":[],"player":"sol","selected":"pass","type":"choice"}"#,
        "\n",
        r#"{"active_player":null,"agenda":{"custodians_removed":false,"laws":{}},"game_over":false,"initiative_order":["sol","letnev"],"phase":"strategy","players":[{"id":"sol"},{"id":"letnev"}],"round":2,"seating_order":["sol","letnev"],"speaker":"sol","strategy_card_goods":{},"systems":[],"turn":1,"type":"state","unclaimed_strategy_cards":[]}"#,
        "\n",
        r#"{"rolls":[],"seed":7,"stream":"dice","type":"entropy"}"#,
        "\n"
    );

    #[test]
    fn captures_checked_initial_and_final_public_snapshots() {
        let states = parse_source_trace_states(TRACE).unwrap();

        assert_eq!(states.trace.decisions, ["pass"]);
        assert_eq!(states.initial["round"], 1);
        assert_eq!(states.final_state["round"], 2);
        assert_eq!(states.initial["players"][0]["id"], "sol");
    }

    #[test]
    fn rejects_state_that_disagrees_with_header_seats() {
        let mismatched = TRACE.replace(
            r#""seating_order":["sol","letnev"]"#,
            r#""seating_order":["letnev","sol"]"#,
        );

        assert!(matches!(
            parse_source_trace_states(&mismatched),
            Err(SourceTraceStateError::InvalidState { line: 2, .. })
        ));
    }

    #[test]
    fn rejects_missing_or_extra_state_snapshots() {
        let missing = TRACE.replace(
            r#"{"active_player":null,"agenda":{"custodians_removed":false,"laws":{}},"game_over":false,"initiative_order":["sol","letnev"],"phase":"strategy","players":[{"id":"sol"},{"id":"letnev"}],"round":2,"seating_order":["sol","letnev"],"speaker":"sol","strategy_card_goods":{},"systems":[],"turn":1,"type":"state","unclaimed_strategy_cards":[]}"#,
            r#"{"cancelled":false,"event_type":"X","id":1,"payload":{},"phase":"strategy","round":1,"turn":0,"type":"event"}"#,
        );
        assert!(matches!(
            parse_source_trace_states(&missing),
            Err(SourceTraceStateError::StateCount { found: 1 })
        ));

        let extra = TRACE.replace(
            r#"{"options":[],"player":"sol","selected":"pass","type":"choice"}"#,
            r#"{"active_player":null,"agenda":{"custodians_removed":false,"laws":{}},"game_over":false,"initiative_order":["sol","letnev"],"phase":"strategy","players":[{"id":"sol"},{"id":"letnev"}],"round":1,"seating_order":["sol","letnev"],"speaker":"sol","strategy_card_goods":{},"systems":[],"turn":0,"type":"state","unclaimed_strategy_cards":[]}"#,
        );
        assert!(matches!(
            parse_source_trace_states(&extra),
            Err(SourceTraceStateError::StateCount { found: 3 })
        ));
    }

    #[test]
    fn every_retained_trace_has_checked_public_state_boundaries() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/legacy_entropy/bounded-v1");
        let mut paths = fs::read_dir(root)
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                path.extension()
                    .is_some_and(|extension| extension == "ndjson")
            })
            .collect::<Vec<_>>();
        paths.sort();

        assert_eq!(paths.len(), 100);
        for path in paths {
            let input = fs::read_to_string(&path).unwrap();
            assert!(parse_source_trace_states(&input).is_ok(), "{path:?}");
        }
    }
}
