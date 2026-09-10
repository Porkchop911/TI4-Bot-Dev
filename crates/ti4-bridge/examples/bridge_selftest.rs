//! End-to-end proof that the bridge is up: Rust against the real vendored endpoint.
//!
//! ```text
//! python bridge/run_bridge.py --port 8080     # in one terminal
//! cargo run -p ti4-bridge --example bridge_selftest
//! ```
//!
//! It impersonates both ends of the wire in turn — posting telemetry the way the mod does, reading
//! the board back the way the bot will, queuing a command the way a decision will, and polling the
//! way the Lua executor does — and then checks that the command came back around with the id it was
//! given, and that the executor's `result` line is read as the outcome of *that* command.
//!
//! This is an example rather than a test on purpose. It needs a Python process listening on a real
//! port, and a `cargo test --workspace` that fails on a machine without Python would break work
//! that has nothing to do with the bridge.
//!
//! Pass an address to point it somewhere else: `... --example bridge_selftest -- 127.0.0.1:9099`.

use ti4_bridge::client::{BridgeClient, ClientError};
use ti4_bridge::wire::{Command, Outcome, OutcomeStatus, PollRequest};

/// A small real board: one system, a Blue carrier in space and Blue infantry on its planet.
const SUMMARY: &str = "18+0+0Bc2f;Bi;Ro";

fn main() -> std::process::ExitCode {
    let address = std::env::args()
        .nth(1)
        .unwrap_or_else(|| ti4_bridge::client::DEFAULT_ADDRESS.to_owned());
    let client = BridgeClient::new(&address);

    match run(&client) {
        Ok(()) => {
            println!("\nbridge self-test passed against {address}");
            std::process::ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("\nbridge self-test FAILED: {error}");
            if matches!(error, ClientError::Unreachable { .. }) {
                eprintln!("start it first:  python bridge/run_bridge.py");
            }
            std::process::ExitCode::FAILURE
        }
    }
}

fn run(client: &BridgeClient) -> Result<(), ClientError> {
    let health = client.health()?;
    println!("bridge is up at {}", client.address());
    println!("  features   {}", health.features.join(", "));
    println!("  uploads    {}", health.uploads);
    println!("  pending    {}", health.pending);

    // 1. Post telemetry the way the mod does.
    let mut payload = serde_json::Map::new();
    payload.insert("hexSummary".to_owned(), SUMMARY.into());
    payload.insert("round".to_owned(), 2.into());
    client.upload(&payload)?;
    println!("\nposted a board as the mod would");

    // 2. Read it back the way the bot will, decoded rather than as a string.
    let board = client.board()?;
    println!("read it back decoded:");
    for system in &board.systems {
        println!(
            "  tile {:<4} at ({:+},{:+})  space: {:?}  planets: {}",
            system.tile,
            system.x,
            system.y,
            system.space.occupiers(),
            system.planets.len()
        );
        for (index, planet) in system.planets.iter().enumerate() {
            println!(
                "    planet {index}  occupiers {:?}  owner token {:?}",
                planet.occupiers(),
                planet.owner_token()
            );
        }
    }
    let tile = board
        .system("18")
        .ok_or_else(|| ClientError::Malformed("tile 18 did not come back".to_owned()))?;
    if tile.space.stacks.len() != 2 || tile.planets.len() != 2 {
        return Err(ClientError::Malformed(format!(
            "tile 18 decoded as {tile:?}, which is not what was posted"
        )));
    }

    // 3. Queue a command the way a decision will.
    let queued = client.queue(&Command::new("ping"))?;
    println!("\nqueued a command, bridge stamped it id {}", queued.id);

    // 4. Collect it the way the Lua executor does.
    let batch = client.poll(&PollRequest::default())?;
    let delivered = batch
        .commands
        .iter()
        .find(|command| command.id == Some(queued.id))
        .ok_or_else(|| {
            ClientError::Malformed(format!(
                "the executor's poll did not carry command {}",
                queued.id
            ))
        })?;
    println!(
        "executor's poll delivered it: action {:?}, id {:?}",
        delivered.action, delivered.id
    );

    // 5. Report the outcome the way the executor does — as a log line, in the next poll.
    let reported = Outcome {
        id: queued.id,
        status: OutcomeStatus::Ok,
        detail: Some("ping".to_owned()),
    };
    client.poll(&PollRequest {
        log: vec![reported.to_line()],
        turn: Some("Blue".to_owned()),
    })?;

    // 6. And read that outcome back off the bridge's log, matched to the command it is about.
    let log = client.log()?;
    let matched = log
        .iter()
        .filter_map(|line| Outcome::parse(line).ok())
        .find(|outcome| outcome.id == queued.id)
        .ok_or_else(|| {
            ClientError::Malformed(format!("no outcome for command {} in the log", queued.id))
        })?;
    if matched != reported {
        return Err(ClientError::Malformed(format!(
            "outcome came back as {matched:?}, not {reported:?}"
        )));
    }
    println!(
        "outcome round-tripped: command {} -> {:?} ({:?})",
        matched.id, matched.status, matched.detail
    );

    let turn = client.turn()?;
    println!(
        "turn signal: {:?}",
        turn.as_ref().map(|report| report.turn.as_str())
    );

    Ok(())
}
