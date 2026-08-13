//! Translation of bounded Python-oracle traces into explicit native replay inputs.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

/// Schema emitted by the pinned bounded-game oracle exporter.
pub const BOUNDED_TRACE_SCHEMA_VERSION: &str = "1.0.0";
/// Commit accepted by this translator's compatibility contract.
pub const PINNED_ORACLE_COMMIT: &str = "37061c511a4780d4c0719e0342533a498cd4b457";

/// Explicit replay inputs extracted from a bounded oracle trace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoundedTrace {
    /// Oracle scenario used to construct the source game.
    pub scenario: String,
    /// Shared source seed for the game, selected decisions, and dice stream.
    pub seed: i64,
    /// Requested bounded game length.
    pub rounds: u64,
    /// Seat order used by the oracle scenario.
    pub seats: Vec<String>,
    /// Oracle source-set label.
    pub sources: String,
    /// Selected legal options in original decision order.
    pub decisions: Vec<String>,
    /// Captured dice history and its source seed.
    pub dice: DiceEntropy,
}

/// Explicit dice entropy captured from the source trace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiceEntropy {
    /// The source game's dice seed.
    pub seed: i64,
    /// Rolls in the order they occurred.
    pub rolls: Vec<DiceRoll>,
}

/// One observed dice roll from the oracle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiceRoll {
    /// Rule-level reason supplied by the source game.
    pub reason: String,
    /// Rolled faces in source order.
    pub faces: Vec<u8>,
    /// Minimum successful face, if the roll has a hit threshold.
    pub hits_on: Option<u8>,
    /// Source-indexed dice that were rerolled.
    pub rerolled: Vec<u64>,
}

/// A bounded oracle trace is malformed, incomplete, or outside the accepted source contract.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum BoundedTraceError {
    /// A non-empty line was not a JSON record.
    #[error("line {line} is not valid JSON: {message}")]
    InvalidJson { line: usize, message: String },
    /// A record has an absent or invalid required field.
    #[error("line {line} is invalid: {reason}")]
    InvalidRecord { line: usize, reason: String },
    /// The first record was not a conforming bounded-game header.
    #[error("invalid bounded-game header: {reason}")]
    InvalidHeader { reason: String },
    /// The trace had no records.
    #[error("bounded trace is empty")]
    EmptyTrace,
    /// The trace did not contain its required final dice entropy record.
    #[error("bounded trace has no final dice entropy record")]
    MissingEntropy,
    /// More than one dice entropy record was present.
    #[error("bounded trace contains more than one dice entropy record")]
    DuplicateEntropy,
    /// A dice record did not use the header's source seed.
    #[error("dice entropy seed {entropy} does not match header seed {header}")]
    EntropySeedMismatch { header: i64, entropy: i64 },
}

#[derive(Debug, Deserialize)]
struct HeaderRecord {
    #[serde(rename = "type")]
    record_type: String,
    schema_version: String,
    oracle_commit: String,
    scenario: String,
    seed: i64,
    rounds: u64,
    seats: Vec<String>,
    sources: String,
    export_scope: String,
}

#[derive(Debug, Deserialize)]
struct EntropyRecord {
    #[serde(rename = "type")]
    record_type: String,
    stream: String,
    seed: i64,
    rolls: Vec<DiceRoll>,
}

/// Parse one bounded-game NDJSON export into explicit native replay inputs.
///
/// The parser intentionally accepts only the version and oracle commit pinned by M00. It ignores
/// state, event, and outcome projections because later migration packages own native state replay;
/// choice order and dice entropy are preserved exactly.
///
/// # Errors
/// Returns [`BoundedTraceError`] for malformed NDJSON, incompatible headers, omitted selected
/// decisions, or anything other than exactly one final dice entropy record.
pub fn parse_bounded_trace(input: &str) -> Result<BoundedTrace, BoundedTraceError> {
    let lines = input.lines().collect::<Vec<_>>();
    let Some((first, rest)) = lines.split_first() else {
        return Err(BoundedTraceError::EmptyTrace);
    };
    if first.is_empty() {
        return Err(BoundedTraceError::InvalidRecord {
            line: 1,
            reason: "first record is empty".to_owned(),
        });
    }
    let header = parse_header(first)?;
    validate_header(&header)?;

    let mut decisions = Vec::new();
    let mut entropy = None;
    for (offset, line) in rest.iter().enumerate() {
        let line_number = offset + 2;
        if line.is_empty() {
            return Err(BoundedTraceError::InvalidRecord {
                line: line_number,
                reason: "empty records are not allowed".to_owned(),
            });
        }
        let record = serde_json::from_str::<serde_json::Value>(line).map_err(|error| {
            BoundedTraceError::InvalidJson {
                line: line_number,
                message: error.to_string(),
            }
        })?;
        let record_type = record_type(&record, line_number)?;
        match record_type {
            "choice" => decisions.push(selected_choice(&record, line_number)?),
            "entropy" => {
                if offset + 1 != rest.len() {
                    return Err(BoundedTraceError::InvalidRecord {
                        line: line_number,
                        reason: "dice entropy must be the final record".to_owned(),
                    });
                }
                if entropy.is_some() {
                    return Err(BoundedTraceError::DuplicateEntropy);
                }
                let parsed = serde_json::from_value::<EntropyRecord>(record).map_err(|error| {
                    BoundedTraceError::InvalidRecord {
                        line: line_number,
                        reason: error.to_string(),
                    }
                })?;
                if parsed.record_type != "entropy" || parsed.stream != "dice" {
                    return Err(BoundedTraceError::InvalidRecord {
                        line: line_number,
                        reason: "expected dice entropy".to_owned(),
                    });
                }
                entropy = Some(DiceEntropy {
                    seed: parsed.seed,
                    rolls: parsed.rolls,
                });
            }
            // The board is carried alongside the states and read by `state_import::import_map`
            // rather than here: this validator's job is the decision sequence and the dice, and a
            // map record is neither. Accepted rather than rejected, because a trace that carries
            // one is a *newer* corpus, not a malformed one.
            "state" | "event" | "outcome" | "map" => {}
            other => {
                return Err(BoundedTraceError::InvalidRecord {
                    line: line_number,
                    reason: format!("unsupported record type {other:?}"),
                });
            }
        }
    }

    let dice = entropy.ok_or(BoundedTraceError::MissingEntropy)?;
    if dice.seed != header.seed {
        return Err(BoundedTraceError::EntropySeedMismatch {
            header: header.seed,
            entropy: dice.seed,
        });
    }
    Ok(BoundedTrace {
        scenario: header.scenario,
        seed: header.seed,
        rounds: header.rounds,
        seats: header.seats,
        sources: header.sources,
        decisions,
        dice,
    })
}

fn parse_header(line: &str) -> Result<HeaderRecord, BoundedTraceError> {
    serde_json::from_str(line).map_err(|error| BoundedTraceError::InvalidHeader {
        reason: error.to_string(),
    })
}

fn validate_header(header: &HeaderRecord) -> Result<(), BoundedTraceError> {
    if header.record_type != "header" {
        return Err(BoundedTraceError::InvalidHeader {
            reason: "first record is not a header".to_owned(),
        });
    }
    if header.schema_version != BOUNDED_TRACE_SCHEMA_VERSION {
        return Err(BoundedTraceError::InvalidHeader {
            reason: format!("unsupported schema version {:?}", header.schema_version),
        });
    }
    if header.oracle_commit != PINNED_ORACLE_COMMIT {
        return Err(BoundedTraceError::InvalidHeader {
            reason: format!("unexpected oracle commit {:?}", header.oracle_commit),
        });
    }
    if header.export_scope != "bounded_game" {
        return Err(BoundedTraceError::InvalidHeader {
            reason: format!("unexpected export scope {:?}", header.export_scope),
        });
    }
    if header.scenario.is_empty() || header.sources.is_empty() || header.rounds == 0 {
        return Err(BoundedTraceError::InvalidHeader {
            reason: "scenario, sources, and positive rounds are required".to_owned(),
        });
    }
    let unique_seats = header.seats.iter().collect::<BTreeSet<_>>();
    if header.seats.len() < 2
        || header.seats.iter().any(String::is_empty)
        || unique_seats.len() != header.seats.len()
    {
        return Err(BoundedTraceError::InvalidHeader {
            reason: "at least two distinct non-empty seats are required".to_owned(),
        });
    }
    Ok(())
}

fn record_type(record: &serde_json::Value, line: usize) -> Result<&str, BoundedTraceError> {
    record
        .as_object()
        .and_then(|record| record.get("type"))
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| BoundedTraceError::InvalidRecord {
            line,
            reason: "record type is required".to_owned(),
        })
}

fn selected_choice(record: &serde_json::Value, line: usize) -> Result<String, BoundedTraceError> {
    record
        .as_object()
        .and_then(|record| record.get("selected"))
        .and_then(serde_json::Value::as_str)
        .filter(|selected| !selected.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| BoundedTraceError::InvalidRecord {
            line,
            reason: "choice selected option is required".to_owned(),
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID_TRACE: &str = concat!(
        r#"{"export_scope":"bounded_game","oracle_commit":"37061c511a4780d4c0719e0342533a498cd4b457","rounds":1,"scenario":"save54_base","schema_version":"1.0.0","seats":["sol","letnev"],"seed":7,"sources":"base","type":"header"}"#,
        "\n",
        r#"{"options":["a","b"],"player":"sol","selected":"b","type":"choice"}"#,
        "\n",
        r#"{"event_type":"ACTION","type":"event"}"#,
        "\n",
        r#"{"rolls":[{"faces":[8,4],"hits_on":6,"reason":"combat","rerolled":[1]}],"seed":7,"stream":"dice","type":"entropy"}"#,
        "\n"
    );

    #[test]
    fn bounded_trace_exposes_ordered_decisions_and_dice_entropy() {
        let trace = parse_bounded_trace(VALID_TRACE).unwrap();

        assert_eq!(trace.scenario, "save54_base");
        assert_eq!(trace.seed, 7);
        assert_eq!(trace.seats, ["sol", "letnev"]);
        assert_eq!(trace.decisions, ["b"]);
        assert_eq!(trace.dice.seed, 7);
        assert_eq!(trace.dice.rolls[0].faces, [8, 4]);
    }

    #[test]
    fn bounded_trace_rejects_a_moved_oracle_header() {
        let trace = VALID_TRACE.replace("37061c511a4780d4c0719e0342533a498cd4b457", "moved");

        assert!(matches!(
            parse_bounded_trace(&trace),
            Err(BoundedTraceError::InvalidHeader { .. })
        ));
    }

    #[test]
    fn bounded_trace_rejects_nonadjacent_duplicate_seats() {
        let trace = VALID_TRACE.replace(r#"["sol","letnev"]"#, r#"["sol","letnev","sol"]"#);

        assert!(matches!(
            parse_bounded_trace(&trace),
            Err(BoundedTraceError::InvalidHeader { .. })
        ));
    }

    #[test]
    fn bounded_trace_preserves_a_valid_negative_python_seed() {
        let trace = VALID_TRACE.replace(r#""seed":7"#, r#""seed":-7"#);

        let parsed = parse_bounded_trace(&trace).unwrap();
        assert_eq!(parsed.seed, -7);
        assert_eq!(parsed.dice.seed, -7);
    }

    #[test]
    fn bounded_trace_rejects_a_choice_without_a_selected_option() {
        let trace = VALID_TRACE.replace(r#""selected":"b","#, "");

        assert!(matches!(
            parse_bounded_trace(&trace),
            Err(BoundedTraceError::InvalidRecord { line: 2, .. })
        ));
    }

    #[test]
    fn bounded_trace_rejects_missing_or_mismatched_final_entropy() {
        let missing = VALID_TRACE.lines().take(3).collect::<Vec<_>>().join("\n");
        assert!(matches!(
            parse_bounded_trace(&missing),
            Err(BoundedTraceError::MissingEntropy)
        ));

        let mismatched =
            VALID_TRACE.replace(r#""seed":7,"stream":"dice"#, r#""seed":8,"stream":"dice"#);
        assert!(matches!(
            parse_bounded_trace(&mismatched),
            Err(BoundedTraceError::EntropySeedMismatch {
                header: 7,
                entropy: 8
            })
        ));
    }
}
