//! A long-lived decision service: the trained policy, answering choices from the Python driver.
//!
//! ```text
//! cargo run --release -p ti4-mlp --example mlp_serve -- \
//!     --bundle out/checkpoints/stage2-mlp-shaped-resumed/checkpoint-241428 \
//!     --captures out/bridge-captures
//! ```
//!
//! Speaks JSON lines on stdin and stdout, one request per line:
//!
//! ```json
//! {"choice": {"player": "hacan", "prompt": "...", "options": [...]}}
//! {"id": "tactical", "label": "take a tactical action", "assigned": 116, "oov": 28}
//! ```
//!
//! # Why a service rather than a command
//!
//! `mlp_decide` loads libtorch and the checkpoint on every invocation — seconds each, and a single
//! turn asks dozens of questions. Everything expensive happens once here: the backend, the bundle,
//! the vocabulary, and one seat-bound bot per faction, created on first use and kept.
//!
//! # Where the state comes from, and the one thing that is stale
//!
//! Python owns the table and the legality; this owns the judgement. The **choice** arrives per
//! request. The **state** is imported from the newest capture in `--captures`, re-read each request
//! so that a fresh upload is picked up without a restart.
//!
//! That means the board is current between turns and **stale within one**: the mod uploads when the
//! table changes, and a bot's own queued commands have not reached the table while it is still
//! deciding. The option scoring is unaffected — the options themselves arrive fresh from Python and
//! are what the policy actually ranks — but the board context behind them can lag by up to a turn.
//! Stated rather than hidden, because it is the first thing to suspect if advice looks stale.
//!
//! Every answer is checked against the offered options by [`ask_private`], so this can never reply
//! with something the driver did not present.

use std::collections::BTreeMap;
use std::io::{BufRead, Write};

use ti4_engine::choice::{Choice, ask_private};
use ti4_model::content_types::FULL;

fn argument(name: &str) -> Option<String> {
    let mut arguments = std::env::args().skip(1);
    while let Some(found) = arguments.next() {
        if found == name {
            return arguments.next();
        }
    }
    None
}

fn refuse(reason: &str) -> ! {
    eprintln!("REFUSED: {reason}");
    std::process::exit(1)
}

/// One request line.
///
/// Parsed by hand rather than derived: `serde`'s derive is not a dependency of this crate, and a
/// two-field request does not justify making it one.
struct Request {
    choice: Choice,
    /// An explicit capture to read instead of the newest one.
    capture: Option<String>,
}

impl Request {
    fn parse(line: &str) -> Result<Self, String> {
        let value: serde_json::Value =
            serde_json::from_str(line).map_err(|error| format!("unreadable request: {error}"))?;
        let choice = value
            .get("choice")
            .ok_or_else(|| "a request needs a \"choice\"".to_owned())?;
        let choice: Choice = serde_json::from_value(choice.clone())
            .map_err(|error| format!("the choice does not parse: {error}"))?;
        let capture = value
            .get("capture")
            .and_then(serde_json::Value::as_str)
            .map(ToOwned::to_owned);
        Ok(Self { choice, capture })
    }
}

fn main() {
    let bundle_path = argument("--bundle").unwrap_or_else(|| refuse("--bundle is required"));
    let captures = argument("--captures").unwrap_or_else(|| "out/bridge-captures".to_owned());
    let temperature: f64 = argument("--temperature").map_or(0.001, |value| {
        value.parse().unwrap_or_else(|_| refuse("--temperature"))
    });

    ti4_tensor::configure_deterministic(20_260_826)
        .unwrap_or_else(|error| refuse(&format!("configuring the backend: {error}")));
    let store = ti4_content::ContentStore::embedded();
    let bundle = ti4_mlp::bundle::read(std::path::Path::new(&bundle_path))
        .unwrap_or_else(|error| refuse(&format!("reading {bundle_path}: {error}")));
    let actor = std::rc::Rc::new(bundle.actor);
    let vocabulary = bundle.vocabulary;

    // One bot per seat, kept. Rebuilding per decision would throw away the memoised vocabulary
    // lookups that make the hot path cheap, and would reseed the sampler every question.
    let mut seats: BTreeMap<
        String,
        (
            Box<dyn ti4_engine::choice::Decider>,
            ti4_mlp::bot::InferenceStatus,
        ),
    > = BTreeMap::new();

    eprintln!(
        "mlp_serve ready: {bundle_path} at temperature {temperature}, captures from {captures}"
    );
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(line) => line,
            Err(error) => {
                eprintln!("stdin: {error}");
                break;
            }
        };
        if line.trim().is_empty() {
            continue;
        }

        let reply = answer(
            &line,
            store,
            &captures,
            &actor,
            &vocabulary,
            temperature,
            &mut seats,
        );
        let text = serde_json::to_string(&reply).unwrap_or_else(|error| {
            format!(r#"{{"error":"the reply would not serialise: {error}"}}"#)
        });
        if writeln!(stdout, "{text}").is_err() || stdout.flush().is_err() {
            break; // The driver went away.
        }
    }
}

fn answer(
    line: &str,
    store: &'static ti4_content::ContentStore,
    captures: &str,
    actor: &std::rc::Rc<ti4_mlp::Actor>,
    vocabulary: &ti4_policy::vocabulary::Vocabulary,
    temperature: f64,
    seats: &mut BTreeMap<
        String,
        (
            Box<dyn ti4_engine::choice::Decider>,
            ti4_mlp::bot::InferenceStatus,
        ),
    >,
) -> serde_json::Value {
    let request = match Request::parse(line) {
        Ok(request) => request,
        Err(error) => return serde_json::json!({ "error": error }),
    };

    let capture = match request.capture.clone().map_or_else(|| newest(captures), Ok) {
        Ok(path) => path,
        Err(error) => return serde_json::json!({ "error": error }),
    };
    let imported = match std::fs::read_to_string(&capture)
        .map_err(|e| format!("reading {capture}: {e}"))
        .and_then(|text| {
            serde_json::from_str::<ti4_bridge::import::Telemetry>(&text)
                .map_err(|e| format!("{capture} is not telemetry: {e}"))
        })
        .and_then(|telemetry| {
            ti4_bridge::import::import(store, &telemetry, FULL)
                .map_err(|e| format!("importing {capture}: {e}"))
        }) {
        Ok(imported) => imported,
        Err(error) => return serde_json::json!({ "error": error }),
    };

    let seat = request.choice.player.to_string();
    if !imported.state.players.iter().any(|p| p.id.as_str() == seat) {
        return serde_json::json!({
            "error": format!("{seat} is not seated at the imported table")
        });
    }

    // A faction the checkpoint has no row for is a *per-request* refusal, never an exit. This
    // process is shared by every seat: exiting here turned one unsupported faction into a
    // table-wide policy outage, because the triggering decision fell back and then so did every
    // later decision for every supported seat, against a dead process.
    if !seats.contains_key(&seat) {
        match ti4_mlp::FactionRow::of(&seat) {
            Ok(row) => {
                let bot = ti4_mlp::bot::MlpBot::sharing(actor, vocabulary.clone(), row, 0)
                    .at_temperature(temperature)
                    .seat();
                seats.insert(seat.clone(), bot);
            }
            Err(error) => {
                return serde_json::json!({
                    "error": format!("{seat} has no faction row in this checkpoint: {error}")
                });
            }
        }
    }
    let entry = seats
        .get_mut(&seat)
        .expect("just inserted or already present");

    let before_assigned = entry
        .1
        .counters()
        .assigned
        .load(std::sync::atomic::Ordering::Relaxed);
    let before_oov = entry
        .1
        .counters()
        .oov
        .load(std::sync::atomic::Ordering::Relaxed);

    match ask_private(
        &request.choice,
        &imported.state,
        store,
        FULL,
        Some(&imported.galaxy),
        &mut *entry.0,
    ) {
        Ok(option) => {
            // Counters are cumulative across the run, so this decision is the difference.
            let assigned = entry
                .1
                .counters()
                .assigned
                .load(std::sync::atomic::Ordering::Relaxed)
                - before_assigned;
            let oov = entry
                .1
                .counters()
                .oov
                .load(std::sync::atomic::Ordering::Relaxed)
                - before_oov;
            serde_json::json!({
                "id": option.id,
                "label": option.label,
                "assigned": assigned,
                "oov": oov,
            })
        }
        // A refusal is data, not a crash: the driver decides whether to fall back to its own bot
        // or stop. Answering with a guess here would be the failure this whole design avoids.
        Err(error) => serde_json::json!({ "error": error.to_string() }),
    }
}

/// The most recent capture in `directory`.
fn newest(directory: &str) -> Result<String, String> {
    let mut captures: Vec<std::path::PathBuf> = std::fs::read_dir(directory)
        .map_err(|error| format!("reading {directory}: {error}"))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|e| e.to_str()) == Some("json"))
        .collect();
    captures.sort();
    captures
        .last()
        .map(|path| path.display().to_string())
        .ok_or_else(|| format!("no captures in {directory}"))
}
