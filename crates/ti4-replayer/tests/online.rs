//! Online play over loopback, against a real game: seating, redaction, and a remote answer that goes
//! through the gate like a click in the window.
//!
//! The host is driven the way the window drives it — drain the gate's feed, hand the frames to
//! `NetHost::sync` — so what a client receives here is what a remote player would see.

// Every test here drives a live branch, which only the `host` build has.
#![cfg(feature = "host")]

use std::io::ErrorKind;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use ti4_model::id::PlayerId;
use ti4_model::view::{HIDDEN, leaks};
use ti4_replayer::SeatControl;
use ti4_replayer::live::{AdvanceGoal, Gate, LiveBranch};
use ti4_replayer::net::redact::private_ids;
use ti4_replayer::net::{JoinRequest, NetClient, NetHost, Remote};
use ti4_review::{ProfileTable, ReviewFrame, ReviewSession, SimulationConfig};

const WAIT: Duration = Duration::from_secs(180);

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crates/<name> sits directly under the workspace root")
        .to_path_buf()
}

fn config() -> SimulationConfig {
    SimulationConfig {
        checkpoint: workspace_root().join("examples/reviewer/checkpoint-473312/slots.json"),
        map_pool: workspace_root().join("examples/reviewer/full_np8_12_holdout.json"),
        seed: 4_242,
        rotation: 1,
        table: ProfileTable::Learner,
        temperature: 0.5,
        diplomacy: false,
        lineup: None,
    }
}

/// What the window holds for the live branch.
#[derive(Default)]
struct Window {
    header: Option<ReviewSession>,
    frames: Vec<ReviewFrame>,
}

impl Window {
    fn drain(&mut self, gate: &Arc<Gate>, host: &NetHost) {
        let feed = gate.take_feed();
        if self.header.is_none() {
            self.header = feed.header;
        }
        for frame in feed.frames {
            if frame.index == self.frames.len() {
                self.frames.push(frame);
            }
        }
        host.sync(gate, self.header.as_ref(), &self.frames);
    }

    /// Keep relaying until `done` holds.
    fn pump_until(
        &mut self,
        what: &str,
        gate: &Arc<Gate>,
        host: &NetHost,
        mut done: impl FnMut() -> bool,
    ) {
        let started = Instant::now();
        while !done() {
            assert!(
                started.elapsed() < WAIT,
                "timed out waiting for {what}: host has {} frames, gate {:?}, pending {:?}",
                self.frames.len(),
                gate.state(),
                gate.pending().map(|pending| pending.actor)
            );
            self.drain(gate, host);
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

fn join(address: SocketAddr, code: &str, seat: Option<&str>) -> std::io::Result<NetClient> {
    NetClient::join(&JoinRequest {
        address: address.to_string(),
        code: code.to_owned(),
        seat: seat.map(PlayerId::new),
        name: "tester".to_owned(),
        resume: None,
    })
}

#[test]
fn remote_seats_play_through_the_gate_and_see_only_their_own_view() {
    let branch = LiveBranch::start(config(), SeatControl::all_auto()).expect("the branch starts");
    let gate = Arc::clone(branch.gate());
    gate.attach_feed();
    gate.run(AdvanceGoal::Steps(3)).expect("the branch runs");
    assert!(branch.wait_until_idle(WAIT), "the opening steps finish");

    let host = NetHost::start("127.0.0.1:0".parse().unwrap()).expect("the host binds");
    let mut window = Window::default();
    let started = Instant::now();
    while window.header.is_none() || window.frames.is_empty() {
        assert!(started.elapsed() < WAIT, "the branch never sent its header");
        window.drain(&gate, &host);
        std::thread::sleep(Duration::from_millis(20));
    }

    // Strangers stay out.
    let refused = join(host.address(), "not-the-code", None)
        .err()
        .expect("a wrong code is refused");
    assert_eq!(refused.kind(), ErrorKind::PermissionDenied, "{refused}");

    // A named seat is taken, and becomes manual on the host.
    let seat0 = PlayerId::new("seat0");
    let first = join(host.address(), host.code(), Some("seat0")).expect("seat0 is free");
    assert_eq!(first.remote().seat.as_ref(), Some(&seat0));
    assert!(gate.snapshot().seats.is_manual(&seat0));
    assert!(host.is_remote(&seat0));

    // Nobody else gets it, and "any seat" gets a different one.
    let taken = join(host.address(), host.code(), Some("seat0"))
        .err()
        .expect("seat0 is taken");
    assert!(taken.to_string().contains("taken"), "{taken}");
    let second = join(host.address(), host.code(), None).expect("some seat is free");
    let seat_b = second.remote().seat.clone().expect("seated");
    assert_ne!(seat_b, seat0);

    // Both clients catch up with the host's timeline.
    let total = window.frames.len();
    window.pump_until("both clients to catch up", &gate, &host, || {
        let (a, b) = (first.remote(), second.remote());
        assert!(
            a.closed.is_none() && b.closed.is_none(),
            "closed: {:?} / {:?}",
            a.closed,
            b.closed
        );
        a.frames.len() >= total && b.frames.len() >= total
    });

    // What seat0 was sent hides everything the full state holds privately from it.
    assert_redacted_for(&seat0, &window.frames, &first.remote());
    run_until_a_remote_seat_answers(&mut window, &gate, &host, [&first, &second]);

    gate.stop();
    drop(first);
    drop(second);
    drop(host);
}

/// Every frame `remote` holds hides from `seat` what the matching full frame holds privately.
fn assert_redacted_for(seat: &PlayerId, full_frames: &[ReviewFrame], remote: &Remote) {
    let header = remote.header.as_ref().expect("a header arrived");
    assert_eq!(
        header.manifest.seed, 0,
        "the seed would let a client replay the game"
    );
    assert!(header.frames.is_empty());
    let full = full_frames.last().expect("the host has frames");
    let seen = &remote.frames[full_frames.len() - 1];
    assert!(
        leaks(&seen.state, seat).is_empty(),
        "{:?}",
        leaks(&seen.state, seat)
    );
    assert_eq!(seen.state.rng_seed, 0);
    assert!(
        seen.state
            .action_card_deck
            .iter()
            .all(|card| card.as_str() == HIDDEN)
    );
    assert!(
        seen.state
            .secret_deck
            .iter()
            .all(|card| card.as_str() == HIDDEN)
    );
    assert!(seen.state.agenda_deck.iter().all(|card| card == HIDDEN));
    assert_eq!(
        seen.state.action_card_deck.len(),
        full.state.action_card_deck.len()
    );
    assert!(
        !private_ids(&full.state, seat).is_empty(),
        "the opening state has decks and secrets to hide"
    );
    for (note, holder) in &seen.state.promissory_notes {
        assert!(
            holder == seat
                || seen.state.promissory_faceup.contains(note)
                || note.starts_with(HIDDEN),
            "{note} held face down by {holder} is visible to {seat}"
        );
    }
    for (full, seen) in full_frames.iter().zip(remote.frames.iter()) {
        assert!(
            seen.decisions
                .iter()
                .all(|decision| decision.player == seat.as_str())
        );
        // String values only: a JSON key can be a struct field that shares a card's name. The
        // viewer's own decisions are theirs to see, and their policy metadata can coincide with a
        // card id (the policy is "mlp", and so is a secret objective), so they are left out.
        let mut strings = Vec::new();
        let mut without_own = serde_json::to_value(seen).unwrap();
        without_own["decisions"] = serde_json::Value::Array(Vec::new());
        collect_strings(&without_own, &mut strings);
        for id in private_ids(&full.state, seat) {
            assert!(
                !strings.iter().any(|text| {
                    text == &id
                        || text
                            .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
                            .any(|token| token == id)
                }),
                "frame {} leaks {id} to {seat}",
                seen.index
            );
        }
    }
}

/// Run until one of the remote seats is asked something, check only its owner is told, then answer
/// it once with an invented option (refused, nothing consumed) and once for real (played).
fn run_until_a_remote_seat_answers(
    window: &mut Window,
    gate: &Arc<Gate>,
    host: &NetHost,
    [first, second]: [&NetClient; 2],
) {
    gate.run(AdvanceGoal::EndOfGame)
        .expect("the branch runs on");
    window.pump_until("a remote seat to be asked", gate, host, || {
        first.remote().pending.is_some() || second.remote().pending.is_some()
    });
    let (asked, other) = if first.remote().pending.is_some() {
        (first, second)
    } else {
        (second, first)
    };
    let pending = asked.remote().pending.clone().expect("checked above");
    assert_eq!(Some(&pending.actor), asked.remote().seat.as_ref());
    assert!(
        other.remote().pending.is_none(),
        "a choice goes to its own seat only"
    );
    assert_eq!(
        gate.pending().map(|parked| parked.fingerprint),
        Some(pending.fingerprint.clone()),
        "the client sees exactly the offer the engine is parked on"
    );
    // The occurrence has to survive the wire, because the client's click carries it back and the gate
    // accepts nothing else. A client that lost or rewrote it could never answer anything.
    assert_eq!(
        gate.pending().map(|parked| parked.offer),
        Some(pending.offer),
        "the client holds the very occurrence the engine is parked on, not a lookalike"
    );

    asked.submit(&pending, "no-such-option".to_owned()).unwrap();
    window.pump_until("the refusal", gate, host, || {
        asked.remote().answer.is_some()
    });
    assert_eq!(
        asked.remote().answer.as_ref().map(|answer| answer.0),
        Some(false)
    );
    assert!(
        gate.pending().is_some(),
        "a refused answer consumes nothing"
    );
    asked.remote().answer = None;
    asked
        .submit(&pending, pending.options[0].id.clone())
        .unwrap();
    window.pump_until("the acceptance", gate, host, || {
        asked.remote().answer.is_some()
    });
    let answer = asked.remote().answer.clone().unwrap();
    assert!(answer.0, "{}", answer.1);
}

fn collect_strings(value: &serde_json::Value, out: &mut Vec<String>) {
    match value {
        serde_json::Value::String(text) => out.push(text.clone()),
        serde_json::Value::Array(items) => items.iter().for_each(|item| collect_strings(item, out)),
        serde_json::Value::Object(map) => map.values().for_each(|item| collect_strings(item, out)),
        _ => {}
    }
}
