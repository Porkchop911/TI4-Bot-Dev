//! M11-001 — the bridge wire envelopes.
//!
//! Six envelopes cross the bridge: an **upload** of the table's telemetry, a **poll** carrying the
//! mod's log back and collecting work, a **queue** submission from another process, the **log**
//! read-back, the **command** itself, and the **outcome** the executor reports for a command it
//! ran. This module is the typed form of all six and nothing else: no transport, no queue policy,
//! no matching of outcomes to commands. Those are M11-002, M11-004 and M11-006.
//!
//! # Which side is authoritative
//!
//! The exit gate for M11 is that *the existing Lua executor communicates with Rust unchanged*, so
//! `tts/bridge_executor.lua` in the historical repository — not the Python server — is the contract
//! these types are cut against. Where the two disagree, the Lua wins, because the Lua is the side
//! that cannot be redeployed to every table already running the mod.
//!
//! Two consequences are load-bearing and easy to lose:
//!
//! - The executor refuses a command whose `action` is not a string, and reports nothing at all for
//!   a command with no `id`. So `action` is mandatory here and `id` is genuinely optional — a
//!   hand-queued command with no id is legal and simply has nobody waiting on it.
//! - An outcome is not JSON. The executor appends a *line* to its log (`bridgeResult`), and that
//!   log rides back inside the next poll body. [`Outcome`] therefore parses a line, not an object,
//!   and [`PollRequest`] is where outcomes actually arrive.
//!
//! # Tolerance is asymmetric, on purpose
//!
//! A malformed poll body must never cost a poll: the poll is also how commands are delivered, so
//! refusing one because an older executor posted `{}` would strand the table with no way to be
//! given orders. [`PollRequest::lenient`] encodes that. A malformed *queue* submission is the
//! opposite case — it comes from another process on this machine, and refusing it surfaces the
//! mistake where it was made instead of in a game's chat.

use serde::{Deserialize, Serialize};

/// Where another process submits a command.
pub const QUEUE_PATH: &str = "/queue";
/// Where the executor asks for work and hands back its log.
pub const POLL_PATH: &str = "/poll";
/// Where the accumulated log is read back.
pub const LOG_PATH: &str = "/log";
/// Where the most recent upload's payload is read back.
pub const LATEST_PATH: &str = "/latest";
/// Where the last turn colour the executor reported is read back.
pub const TURN_PATH: &str = "/turn";

/// The paths the mod posts telemetry to.
///
/// Three rather than one because the mod chooses among them by what it is sending; the bridge
/// treats all three alike and records which one carried a payload.
pub const UPLOAD_PATHS: [&str; 3] = ["/posttimestamp", "/postkey", "/data"];

/// What this bridge can do, announced on the health check.
///
/// A listener that does not find a capability it needs refuses rather than guessing — see M11-003.
/// The list is ordered and stable; it is a contract, not a set.
pub const FEATURES: [&str; 5] = ["latest", "log", "poll", "command_ids", "turn_signal"];

/// A command's number.
///
/// Unsigned and integral, which is narrower than the historical bridge allowed. The reason is not
/// tidiness: the executor renders the id with Lua's `tostring` when it reports the outcome, so a
/// non-integral id would come back as `1.0` and match no command that was ever queued. An id that
/// cannot survive the round trip is not an id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CommandId(pub u64);

impl std::fmt::Display for CommandId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

/// One instruction for the executor.
///
/// The action-specific fields are kept as they arrived rather than modelled per action. That is
/// deliberate at this package: the builders in M11-014 and M11-015 own the per-action shapes, and
/// until they exist, a command that lost a field on the way through would be worse than one this
/// module does not understand. It also keeps the echo in [`QueueResponse`] honest — a caller
/// cannot be shown one command and have another queued.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Command {
    /// Absent when the command was queued by hand; nobody is waiting on its outcome.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<CommandId>,
    /// What to do. The executor refuses a command without it.
    pub action: String,
    /// Everything else the action needs, carried through untouched.
    #[serde(flatten)]
    pub rest: serde_json::Map<String, serde_json::Value>,
}

impl Command {
    /// A command with no arguments beyond its action and no id.
    pub fn new(action: impl Into<String>) -> Self {
        Self {
            id: None,
            action: action.into(),
            rest: serde_json::Map::new(),
        }
    }

    /// The same command, stamped.
    #[must_use]
    pub fn with_id(mut self, id: CommandId) -> Self {
        self.id = Some(id);
        self
    }
}

/// The executor's poll body: its log since the last poll, and whose turn the table thinks it is.
///
/// The turn is a cheap signal that a hotseat player pressed End Turn. It is deliberately not board
/// state — importing the human's physical changes waits for the next full upload.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PollRequest {
    /// Lines the executor accumulated, including any `result` lines.
    #[serde(default)]
    pub log: Vec<String>,
    /// The colour whose turn it is, when the executor reported one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn: Option<String>,
}

impl PollRequest {
    /// Read a poll body without ever failing.
    ///
    /// Anything unrecognised becomes an empty poll. A poll is how commands reach the table, so a
    /// body this bridge cannot read must still cost the caller nothing but the contents of that
    /// one body — never the delivery.
    ///
    /// Fields are taken individually for the same reason: an executor that sends a good `log` and
    /// a malformed `turn` keeps its log.
    #[must_use]
    pub fn lenient(body: &[u8]) -> Self {
        let Ok(value) = serde_json::from_slice::<serde_json::Value>(body) else {
            return Self::default();
        };
        let Some(object) = value.as_object() else {
            return Self::default();
        };
        let log = object
            .get("log")
            .and_then(serde_json::Value::as_array)
            .map(|lines| {
                lines
                    .iter()
                    .filter_map(|line| line.as_str().map(ToOwned::to_owned))
                    .collect()
            })
            .unwrap_or_default();
        let turn = object
            .get("turn")
            .and_then(serde_json::Value::as_str)
            .map(ToOwned::to_owned);
        Self { log, turn }
    }
}

/// What every poll and every upload is answered with.
///
/// Always an object with a `commands` array, even when the array is empty: the executor reads the
/// reply as JSON and an empty body would be a parse failure it reports into a human's chat once
/// per poll.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandBatch {
    /// In the order the executor must run them.
    pub commands: Vec<Command>,
}

/// One payload the mod posted, with how it was addressed and when it arrived.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Upload {
    /// Which of [`UPLOAD_PATHS`] carried it.
    pub path: String,
    /// The query string, flattened to the first value of each key.
    #[serde(default)]
    pub args: std::collections::BTreeMap<String, String>,
    /// The body, untouched.
    pub payload: serde_json::Map<String, serde_json::Value>,
    /// Unix seconds. A float on the wire because that is what the mod's readers expect.
    pub received_at: f64,
}

impl Upload {
    /// The board string, if this payload carried one.
    #[must_use]
    pub fn hex_summary(&self) -> Option<&str> {
        self.payload.get("hexSummary")?.as_str()
    }

    /// The round number, if this payload carried one.
    ///
    /// Integral only: a round that arrived as `3.0` is a malformed round, not round three.
    #[must_use]
    pub fn round(&self) -> Option<u64> {
        self.payload.get("round")?.as_u64()
    }
}

/// The answer to a `/queue` submission.
///
/// The stamped command comes back whole, not just its number, so a caller echoing the result
/// cannot display one thing and have queued another.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueueResponse {
    /// The command as queued, including the id this bridge gave it.
    pub queued: Command,
    /// That id, lifted out for callers that only need the number.
    pub id: CommandId,
    /// How many commands are waiting, including this one.
    pub pending: usize,
}

/// The answer to `/log`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogResponse {
    /// Oldest first.
    pub log: Vec<String>,
}

/// The answer to `/turn`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TurnResponse {
    /// The colour the executor last reported.
    pub turn: String,
    /// Unix seconds when it was reported.
    pub received_at: f64,
}

/// The answer to a bare `GET /`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Health {
    /// Always true; its presence is the health signal.
    pub ok: bool,
    /// How many uploads have arrived this run.
    pub uploads: usize,
    /// How many commands are waiting.
    pub pending: usize,
    /// [`FEATURES`], so a listener can negotiate rather than guess.
    pub features: Vec<String>,
}

impl Health {
    /// A health report for a bridge in the given state.
    #[must_use]
    pub fn new(uploads: usize, pending: usize) -> Self {
        Self {
            ok: true,
            uploads,
            pending,
            features: FEATURES.iter().map(|name| (*name).to_owned()).collect(),
        }
    }
}

/// A refusal, in the shape every error response takes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorResponse {
    /// Why. Shown to whoever asked, so it names the mistake rather than the internals.
    pub error: String,
}

impl ErrorResponse {
    /// A refusal carrying this reason.
    pub fn new(reason: impl Into<String>) -> Self {
        Self {
            error: reason.into(),
        }
    }
}

/// How a command ended, as the executor classifies it.
///
/// Three outcomes and no fourth. In particular there is no "probably fine": a line this module
/// cannot classify is [`OutcomeError`], never [`OutcomeStatus::Ok`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OutcomeStatus {
    /// The executor ran it and nothing objected.
    Ok,
    /// The table refused it — the move was illegal, the piece was not there.
    Refused,
    /// The executor itself failed, which is a bug on one side or the other.
    Error,
}

impl OutcomeStatus {
    /// The word the executor writes for this status.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Refused => "refused",
            Self::Error => "error",
        }
    }
}

/// What became of one command.
///
/// Reported by the executor as a log line, not as JSON, and carried back inside the next
/// [`PollRequest`]. Matching these to the commands they are about is M11-006.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Outcome {
    /// The command this is about.
    pub id: CommandId,
    /// How it ended.
    pub status: OutcomeStatus,
    /// The executor's own words, when it had any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// Why a log line is not an outcome.
///
/// Distinguished rather than collapsed because the caller's response differs: a line that is not a
/// result at all is the normal case and is simply not an outcome, while a result line this module
/// cannot read is a contract break worth surfacing.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OutcomeError {
    /// The line is ordinary log output, not a result. Expected, and not a problem.
    #[error("not a result line")]
    NotAResult,
    /// A result line whose id is missing or is not a number.
    #[error("result line has no readable command id: {0:?}")]
    UnreadableId(String),
    /// A result line whose status word is none of the three.
    #[error("result line has an unknown status {0:?}")]
    UnknownStatus(String),
}

impl Outcome {
    /// Read one executor log line.
    ///
    /// The shape the executor writes is `result <id> <status>` with an optional `: <detail>`. The
    /// detail may itself contain colons — a refusal reason is free text — so the split is at the
    /// *first* colon after the status and the remainder is kept whole.
    ///
    /// # Errors
    /// [`OutcomeError::NotAResult`] for ordinary log output, and the other variants for a result
    /// line that cannot be read. Nothing here ever produces a successful outcome by default.
    pub fn parse(line: &str) -> Result<Self, OutcomeError> {
        let rest = line.trim().strip_prefix("result ").ok_or({
            // A bare "result" with nothing after it is also not a result line: there is no command
            // it could be about.
            OutcomeError::NotAResult
        })?;
        let mut parts = rest.trim_start().splitn(2, ' ');
        let id = parts.next().unwrap_or_default();
        let id: u64 = id
            .parse()
            .map_err(|_| OutcomeError::UnreadableId(id.to_owned()))?;
        let remainder = parts.next().unwrap_or_default().trim_start();
        let (status, detail) = match remainder.split_once(':') {
            Some((status, detail)) => {
                let detail = detail.trim();
                let detail = (!detail.is_empty()).then(|| detail.to_owned());
                (status.trim(), detail)
            }
            None => (remainder.trim(), None),
        };
        let status = match status {
            "ok" => OutcomeStatus::Ok,
            "refused" => OutcomeStatus::Refused,
            "error" => OutcomeStatus::Error,
            other => return Err(OutcomeError::UnknownStatus(other.to_owned())),
        };
        Ok(Self {
            id: CommandId(id),
            status,
            detail,
        })
    }

    /// The line the executor would have written for this outcome.
    ///
    /// The inverse of [`Outcome::parse`] for every outcome this module can hold, which is what
    /// makes the round-trip property in the tests meaningful.
    #[must_use]
    pub fn to_line(&self) -> String {
        match &self.detail {
            Some(detail) => format!("result {} {}: {detail}", self.id, self.status.as_str()),
            None => format!("result {} {}", self.id, self.status.as_str()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn json(text: &str) -> serde_json::Value {
        serde_json::from_str(text).expect("the test's own literal is valid JSON")
    }

    #[test]
    fn a_command_keeps_the_fields_this_module_does_not_model() {
        // The builders that own per-action shapes do not exist yet (M11-014, M11-015). Until they
        // do, a command that silently lost `from` would be executed as a different move.
        let text = r#"{"id":7,"action":"move","from":"18","to":"19","units":["carrier"]}"#;
        let command: Command = serde_json::from_str(text).expect("a well-formed command");
        assert_eq!(command.id, Some(CommandId(7)));
        assert_eq!(command.action, "move");
        assert_eq!(
            command.rest.get("from").and_then(|v| v.as_str()),
            Some("18")
        );
        assert_eq!(
            serde_json::to_value(&command).expect("serialisable"),
            json(text),
            "a command must round-trip whole, or the queue echo is a lie"
        );
    }

    #[test]
    fn a_command_without_an_id_is_legal_and_stays_without_one() {
        let command: Command =
            serde_json::from_str(r#"{"action":"ping"}"#).expect("a hand-queued command");
        assert_eq!(command.id, None);
        // Not `"id":null` — the executor tests `if command.id then`, and a null would read as
        // absent anyway, but an omitted key is what every existing reader expects.
        assert_eq!(
            serde_json::to_string(&command).expect("serialisable"),
            r#"{"action":"ping"}"#
        );
    }

    #[test]
    fn a_command_without_an_action_is_refused() {
        // Both sides refuse it; refusing here means the mistake surfaces before it reaches a table.
        let refused = serde_json::from_str::<Command>(r#"{"id":1,"peculiar":true}"#);
        assert!(refused.is_err(), "a command needs an action");
    }

    #[test]
    fn a_non_integral_id_is_refused_rather_than_rounded() {
        // `tostring(1.0)` in the executor is "1.0", which would match no command ever queued.
        let refused = serde_json::from_str::<Command>(r#"{"id":1.5,"action":"ping"}"#);
        assert!(refused.is_err(), "an id must survive the round trip");
    }

    #[test]
    fn an_unreadable_poll_body_is_an_empty_poll_not_a_lost_poll() {
        for body in [
            &b""[..],
            b"not json at all",
            b"[]",
            b"null",
            br#"{"log":"not an array"}"#,
            br#"{"turn":42}"#,
        ] {
            let poll = PollRequest::lenient(body);
            assert_eq!(
                poll,
                PollRequest::default(),
                "body {:?} must not cost the table its commands",
                String::from_utf8_lossy(body)
            );
        }
    }

    #[test]
    fn a_poll_keeps_the_half_of_itself_that_is_readable() {
        let poll = PollRequest::lenient(br#"{"log":["a",7,"b"],"turn":null}"#);
        assert_eq!(poll.log, vec!["a".to_owned(), "b".to_owned()]);
        assert_eq!(poll.turn, None);
    }

    #[test]
    fn an_empty_batch_is_still_an_object_with_an_array() {
        // The executor JSON-decodes every reply and complains into chat when it cannot. The normal
        // case — nothing to do — must not be the noisy one.
        assert_eq!(
            serde_json::to_string(&CommandBatch::default()).expect("serialisable"),
            r#"{"commands":[]}"#
        );
    }

    #[test]
    fn health_announces_the_features_in_their_fixed_order() {
        let health = Health::new(3, 1);
        assert_eq!(
            serde_json::to_value(&health).expect("serialisable"),
            json(
                r#"{"ok":true,"uploads":3,"pending":1,
                    "features":["latest","log","poll","command_ids","turn_signal"]}"#
            )
        );
    }

    #[test]
    fn an_upload_reads_only_the_typed_shapes_it_promises() {
        let upload: Upload = serde_json::from_str(
            r#"{"path":"/data","args":{"key":"value"},
                "payload":{"hexSummary":"18:1","round":3},"received_at":1757000000.5}"#,
        )
        .expect("a well-formed upload");
        assert_eq!(upload.hex_summary(), Some("18:1"));
        assert_eq!(upload.round(), Some(3));

        let odd: Upload = serde_json::from_str(
            r#"{"path":"/data","payload":{"hexSummary":7,"round":3.5},"received_at":0.0}"#,
        )
        .expect("a well-formed upload with odd contents");
        assert_eq!(odd.hex_summary(), None, "a number is not a board string");
        assert_eq!(odd.round(), None, "3.5 is not round three");
    }

    #[test]
    fn an_outcome_line_round_trips() {
        for (line, expected) in [
            (
                "result 4 ok",
                Outcome {
                    id: CommandId(4),
                    status: OutcomeStatus::Ok,
                    detail: None,
                },
            ),
            (
                "result 12 refused: no ships in 18",
                Outcome {
                    id: CommandId(12),
                    status: OutcomeStatus::Refused,
                    detail: Some("no ships in 18".to_owned()),
                },
            ),
            (
                "result 3 error: attempt to index a nil value: field 'units'",
                Outcome {
                    id: CommandId(3),
                    status: OutcomeStatus::Error,
                    detail: Some("attempt to index a nil value: field 'units'".to_owned()),
                },
            ),
        ] {
            let parsed = Outcome::parse(line).expect("a result line");
            assert_eq!(parsed, expected, "{line}");
            assert_eq!(
                parsed.to_line(),
                line,
                "the line must survive the round trip"
            );
        }
    }

    #[test]
    fn ordinary_log_output_is_simply_not_an_outcome() {
        for line in [
            "polling every 2s",
            "bridge is back",
            "command failed: something",
            "",
            "result",
        ] {
            assert_eq!(
                Outcome::parse(line),
                Err(OutcomeError::NotAResult),
                "{line:?}"
            );
        }
    }

    #[test]
    fn an_unreadable_result_line_is_never_read_as_success() {
        // The one property this parser exists to hold: silence and confusion are not success.
        assert_eq!(
            Outcome::parse("result seven ok"),
            Err(OutcomeError::UnreadableId("seven".to_owned()))
        );
        assert_eq!(
            Outcome::parse("result 4 done"),
            Err(OutcomeError::UnknownStatus("done".to_owned()))
        );
        assert_eq!(
            Outcome::parse("result 4"),
            Err(OutcomeError::UnknownStatus(String::new()))
        );
        assert_eq!(
            Outcome::parse("result 4 OK"),
            Err(OutcomeError::UnknownStatus("OK".to_owned())),
            "the executor writes lower case; anything else is a different executor"
        );
    }

    #[test]
    fn a_detail_containing_colons_is_kept_whole() {
        let parsed = Outcome::parse("result 9 refused: 18: not adjacent to 19").expect("a result");
        assert_eq!(parsed.detail.as_deref(), Some("18: not adjacent to 19"));
    }

    #[test]
    fn the_queue_response_echoes_the_command_it_queued() {
        let queued = Command::new("ping").with_id(CommandId(1));
        let response = QueueResponse {
            queued: queued.clone(),
            id: CommandId(1),
            pending: 1,
        };
        assert_eq!(
            serde_json::to_value(&response).expect("serialisable"),
            json(r#"{"queued":{"id":1,"action":"ping"},"id":1,"pending":1}"#)
        );
        assert_eq!(response.queued, queued);
    }
}
