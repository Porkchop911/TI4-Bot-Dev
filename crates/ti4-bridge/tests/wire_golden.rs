//! M11-001 — the golden corpus for the bridge wire envelopes.
//!
//! Each file in `tests/golden/` is one envelope as it appears on the wire. The property every one
//! of them holds is that parsing it and serialising the result reproduces the same JSON *value* —
//! not the same bytes, since the files are indented for reading and the wire is not.
//!
//! Value equality rather than byte equality is the point: it proves nothing was dropped and
//! nothing was invented, which is what a caller echoing a queued command or an importer reading an
//! upload actually depends on. A field this crate does not model still has to survive.
//!
//! These files are the artifact to diff against when the Lua executor changes. They are checked in
//! deliberately, because the historical Python bridge's own fixtures live in a repository that is
//! read-only and pinned, and cannot be regenerated here.

use std::path::PathBuf;

use ti4_bridge::wire::{
    CommandBatch, CommandId, Health, LogResponse, Outcome, OutcomeStatus, PollRequest,
    QueueResponse, Upload,
};

fn golden(name: &str) -> String {
    let path: PathBuf = [env!("CARGO_MANIFEST_DIR"), "tests", "golden", name]
        .iter()
        .collect();
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("reading {}: {error}", path.display()))
}

/// Parse `name` as `T`, serialise it back, and require the same JSON value.
fn round_trips<T>(name: &str) -> T
where
    T: serde::de::DeserializeOwned + serde::Serialize,
{
    let text = golden(name);
    let parsed: T =
        serde_json::from_str(&text).unwrap_or_else(|error| panic!("parsing {name}: {error}"));
    let expected: serde_json::Value =
        serde_json::from_str(&text).expect("the golden file is valid JSON");
    let actual = serde_json::to_value(&parsed).expect("serialisable");
    assert_eq!(actual, expected, "{name} did not survive the round trip");
    parsed
}

#[test]
fn every_envelope_survives_the_round_trip() {
    let batch: CommandBatch = round_trips("command.json");
    let poll: PollRequest = round_trips("poll.json");
    let queued: QueueResponse = round_trips("queue.json");
    let log: LogResponse = round_trips("log.json");
    let upload: Upload = round_trips("upload.json");
    let health: Health = round_trips("health.json");

    // Spot-check that the parse was real rather than a structurally empty success.
    assert_eq!(batch.commands.len(), 11);
    assert_eq!(poll.turn.as_deref(), Some("Blue"));
    assert_eq!(queued.id, CommandId(12));
    assert_eq!(log.log.len(), 2);
    assert_eq!(upload.hex_summary(), Some("18:1;19:2"));
    assert_eq!(upload.round(), Some(3));
    assert!(health.ok);
}

#[test]
fn the_command_corpus_covers_the_actions_the_executor_dispatches() {
    let batch: CommandBatch = round_trips("command.json");
    let actions: Vec<&str> = batch
        .commands
        .iter()
        .map(|command| command.action.as_str())
        .collect();
    for expected in [
        "ping", "activate", "move", "land", "claim", "control", "card", "token", "score",
        "end_turn",
    ] {
        assert!(
            actions.contains(&expected),
            "no {expected} command in the corpus"
        );
    }

    // The last one is deliberately unstamped: the executor reports nothing for a command with no
    // id, and a corpus of only stamped commands would not exercise that.
    let unstamped = batch
        .commands
        .iter()
        .filter(|command| command.id.is_none())
        .count();
    assert_eq!(unstamped, 1);

    // Action-specific fields are carried, not dropped. `move` is the one that would silently
    // become a different move.
    let mover = batch
        .commands
        .iter()
        .find(|command| command.action == "move")
        .expect("a move command");
    assert_eq!(
        mover
            .rest
            .get("units")
            .and_then(|units| units.as_array())
            .map(Vec::len),
        Some(2)
    );
}

#[test]
fn the_outcomes_in_the_golden_poll_are_read_exactly_as_the_executor_wrote_them() {
    let poll: PollRequest = round_trips("poll.json");
    let outcomes: Vec<Outcome> = poll
        .log
        .iter()
        .filter_map(|line| Outcome::parse(line).ok())
        .collect();

    assert_eq!(
        outcomes.len(),
        3,
        "two of the five lines are ordinary log output and must not become outcomes"
    );
    assert_eq!(outcomes[0].id, CommandId(1));
    assert_eq!(outcomes[0].status, OutcomeStatus::Ok);
    assert_eq!(outcomes[1].status, OutcomeStatus::Refused);
    assert_eq!(
        outcomes[1].detail.as_deref(),
        Some("19 is not adjacent to 18")
    );
    assert_eq!(outcomes[2].status, OutcomeStatus::Error);
    assert_eq!(
        outcomes[2].detail.as_deref(),
        Some("attempt to index a nil value: field 'units'"),
        "a Lua error message contains colons and must be kept whole"
    );

    for outcome in &outcomes {
        assert!(
            poll.log.contains(&outcome.to_line()),
            "{outcome:?} does not render back to the line it came from"
        );
    }
}
