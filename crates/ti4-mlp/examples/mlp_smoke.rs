//! Can the MLP play a game? — the §7.1 legality smoke, at its smallest.
//!
//! Six MLP bots, one real six-player game, every decision scored by the actor against the
//! M09-024b2 vocabulary. The weights are zero, so the policy is uniform over each legal set; that
//! is the point. This proves the chain — engine choice → bound observation → projected features →
//! dense columns → trunk → readout → softmax → a legal answer — actually connects, before anything
//! is trained and before there is a checkpoint format to load.
//!
//! # What the coverage number is, and is not
//!
//! By default this runs a **discovery-regression** smoke: the seed is inside M09-024b2's own
//! discovery range on the same training pool and extractor, so 100% vocabulary coverage is expected
//! *by construction* and a shortfall would mean discovery or the projection had regressed. It is
//! not independent coverage. `--seed` outside `202_608_210..202_608_338`, or a different pool,
//! measures something else and is labelled as such in the output.
//!
//! ```text
//! cargo run --release -p ti4-mlp --example mlp_smoke -- \
//!     --slots out/vocabulary/slots.json \
//!     --map-pool out/pools/full_np8_12_train.json
//! ```

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use sha2::Digest;
use ti4_content::ContentStore;
use ti4_engine::choice::{SeededRandom, Table};
use ti4_engine::game::Game;
use ti4_engine::setup::start_game_seeded;
use ti4_mlp::bot::MlpBot;
use ti4_mlp::{Actor, FactionRow, Width};
use ti4_model::content_types::DEFAULT;
use ti4_model::id::{FactionId, PlayerId};
use ti4_policy::vocabulary::Vocabulary;
use ti4_sim::artifacts::ArtifactRole;

const FACTIONS: [&str; 6] = ["sol", "letnev", "xxcha", "hacan", "jolnar", "l1z1x"];
const TILE_SEED_OFFSET: u64 = 20_000_000;
/// The accepted vocabulary generation. The smoke runs against that one or none.
///
/// M09-027b replaced M09-024b2's `14c19387…8479`. The registry moved to v3 to give the critic
/// namespace a reserved column, which shifts every ordinary column after the reserved block, and
/// discovery now emits `critic-state:*` names — 10,997 slots became 11,118. Both changes make the
/// previous generation a different artifact rather than a stale one, so the pin moves with it.
///
/// M10-035 replaces `8805cfdd…2b9d` for the same two reasons at once. The registry moved to v4 for
/// the `action-plan` namespace, and discovery now emits ten opening-progress facts and nine
/// action-feasibility ones — 11,118 slots became 11,138.
///
/// M10-036 replaces `4456cf89…1421b` for the first reason only. The registry stays at v4: no new
/// namespace, so no new reserved column. What changed is that an activation option now carries ten
/// further facts — what could reach the tile, what the tile is worth, who already holds it, and
/// whether taking it moves a revealed objective — and 11,137 slots became 11,147.
///
/// A vocabulary change with no registry change still makes earlier bundles unusable, which is
/// worth stating plainly because the failure mode differs: the old bundles remain *readable*, and
/// their weights are simply attached to the wrong columns. Only the pin catches that.
const ACCEPTED_SLOTS_SHA256: &str =
    "e30b9165ab7dffc1d62ae58b1ec8cb5ed97014d4f7ef22ac17931ba8f57d0a2a";

/// The accepted generation's `slots.json`, from `out/vocabulary/current.json`.
fn ti4_training_generation() -> Option<String> {
    ti4_training::vocabulary_corpus::accepted_generation(std::path::Path::new("out/vocabulary"))
        .ok()
        .map(|generation| generation.slots.display().to_string())
}

fn refuse(reason: &str) -> ! {
    eprintln!("REFUSED: {reason}");
    std::process::exit(2);
}

fn argument(name: &str) -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

#[expect(
    clippy::too_many_lines,
    reason = "a linear smoke script: it reads in the order the game is set up and played"
)]
fn main() {
    let content = ContentStore::embedded();
    // Default to the accepted generation the pointer names, rather than a fixed path that a
    // republish moves out from under.
    let slots = argument("--slots").unwrap_or_else(|| {
        ti4_training_generation()
            .unwrap_or_else(|| refuse("out/vocabulary/current.json names no valid generation"))
    });
    let pool_path =
        argument("--map-pool").unwrap_or_else(|| "out/pools/full_np8_12_train.json".to_owned());
    let rounds: u32 = argument("--rounds")
        .and_then(|v| v.parse().ok())
        .unwrap_or(4);
    // Proves the fail-closed path is real rather than asserted: one bot is given a temperature the
    // actor must refuse, so every one of its decisions becomes a counted fallback and the run must
    // exit non-zero (F-M09-026-3).
    let force_failure = std::env::args().any(|a| a == "--force-inference-failure");
    let seed: u64 = argument("--seed")
        .and_then(|v| v.parse().ok())
        .unwrap_or(202_608_210);

    ti4_tensor::configure_deterministic(i64::try_from(seed).unwrap_or(i64::MAX))
        .expect("deterministic configuration");

    // F-M09-026-8: both inputs are verified before use and every consumer parses from the verified
    // bytes. Reading a path twice, or reading an unverified one, lets an arbitrary vocabulary or an
    // unknown pool produce a successful-looking coverage report.
    let slot_bytes =
        std::fs::read(&slots).unwrap_or_else(|e| refuse(&format!("reading {slots}: {e}")));
    let slots_sha = format!("{:x}", sha2::Sha256::digest(&slot_bytes));
    if slots_sha != ACCEPTED_SLOTS_SHA256 {
        refuse(&format!(
            "{slots} is {slots_sha}, not the accepted vocabulary generation {ACCEPTED_SLOTS_SHA256}"
        ));
    }
    let text = String::from_utf8(slot_bytes)
        .unwrap_or_else(|e| refuse(&format!("slots.json is not UTF-8: {e}")));
    let vocabulary = Vocabulary::from_json(&text)
        .unwrap_or_else(|e| refuse(&format!("slots.json does not load: {e}")));
    println!(
        "vocabulary: {} slots, V_cap {}, registry v{}",
        vocabulary.slot_count(),
        vocabulary.capacity(),
        vocabulary.oov_registry_version()
    );

    let capacity = i64::try_from(vocabulary.capacity()).expect("capacity fits");
    let backend = ti4_tensor::backend();
    println!(
        "backend: cuda {} · intra-op {} · width 256 · {} heads · 33 seats",
        backend.cuda,
        backend.intra_op_threads,
        ti4_mlp::heads().len()
    );

    let players: Vec<PlayerId> = (0..FACTIONS.len())
        .map(|index| PlayerId::new(format!("seat{index}")))
        .collect();
    let factions: BTreeMap<PlayerId, FactionId> = players
        .iter()
        .enumerate()
        .map(|(index, player)| (player.clone(), FactionId::new(FACTIONS[index])))
        .collect();

    let mut state = start_game_seeded(content, &players, DEFAULT, None, seed).expect("setup");
    for (player, faction) in &factions {
        if let Some(seat) = state.player_mut(player) {
            seat.faction = faction.clone();
        }
    }
    ti4_engine::promissory::deal(&mut state, content, DEFAULT);
    let pool_bytes = ti4_sim::artifacts::read_and_verify_pool_role(
        std::path::Path::new(&pool_path),
        &[ArtifactRole::Train, ArtifactRole::Validation],
    )
    .unwrap_or_else(|e| refuse(&format!("{pool_path} is not an allowed pool: {e}")));
    let pool = Arc::new(
        ti4_sim::MapPool::from_reader(std::io::Cursor::new(&pool_bytes))
            .unwrap_or_else(|e| refuse(&format!("parsing the verified pool bytes: {e}"))),
    );
    let homes: Vec<String> = players
        .iter()
        .map(|player| {
            ti4_content::factions::get(content, factions[player].as_str())
                .and_then(|f| f.home_system())
                .expect("home")
                .to_owned()
        })
        .collect();
    let borrowed: Vec<&str> = homes.iter().map(String::as_str).collect();
    let galaxy = pool
        .galaxy(
            content,
            DEFAULT,
            seed.wrapping_add(TILE_SEED_OFFSET),
            &borrowed,
        )
        .expect("galaxy");
    for (player, faction) in &factions {
        ti4_engine::seating::deploy(&mut state, content, player, faction, DEFAULT).expect("deploy");
    }

    let mut table = Table::with_default(Box::new(SeededRandom::new(seed)));
    let mut statuses: Vec<ti4_mlp::bot::InferenceStatus> = Vec::new();
    for (index, player) in players.iter().enumerate() {
        // The conditioning key is the **faction identity**, resolved through the pinned roster —
        // never the physical seat index, which changes with rotation (F-M09-026-2).
        let identity = FactionRow::of(factions[player].as_str())
            .expect("every seated faction is in the roster");
        let mut bot = MlpBot::new(
            Actor::zeros(Width::W256, capacity),
            Vocabulary::from_json(&text).expect("vocabulary"),
            identity,
            seed.wrapping_mul(1_000_003)
                .wrapping_add(u64::try_from(index).unwrap_or(0)),
        );
        if force_failure && index == 0 {
            bot = bot.at_temperature(0.0);
        }
        let (decider, status) = bot.seat();
        statuses.push(status);
        table.seat(player.clone(), decider);
    }

    let mut game = Game::with_table(state, content, table)
        .with_sources(DEFAULT)
        .with_galaxy(galaxy);

    let started = std::time::Instant::now();
    let start_round = game.state.round;
    let target = start_round.saturating_add(rounds);
    let mut steps = 0usize;
    let mut resolved = 0usize;
    while !game.state.finished && game.state.round < target {
        let result = game.step();
        if let Some(error) = &result.error {
            eprintln!("game died at step {steps}: {error}");
            std::process::exit(if force_failure { 4 } else { 2 });
        }
        if result.resolved_choice {
            resolved += 1;
        }
        steps += 1;
        if steps > 500_000 {
            eprintln!("step bound hit");
            std::process::exit(3);
        }
    }

    // The round *state* is not the number of rounds played: the game starts at round 1, so a
    // four-round horizon ends with the counter reading 5. Reporting the counter as "played 5
    // rounds" overstated it by one (F-M09-026-6).
    let completed = game.state.round.saturating_sub(start_round);
    println!(
        "
completed {completed} of {rounds} rounds (round state {start_round} -> {}): {steps} steps, {resolved} resolved choices, {:.1?}",
        game.state.round,
        started.elapsed()
    );
    println!("finished: {}", game.state.finished);

    // --- The value path, against the generation this run actually loaded. ---
    //
    // F-M09-027-3. The unit tests build their own vocabulary, so they cannot see a *published*
    // artifact that has no room for the critic — which is exactly the state the previous generation
    // was in: every `critic-state:*` fact resolved to one column and `V` was a rank-1 sum. This
    // runs the real extractor over the position the game just reached, through the engine's own
    // ask path, and refuses if the critic collapses.
    {
        struct Probe {
            actor: Actor,
            vocabulary: Vocabulary,
            row: FactionRow,
            columns: usize,
            names: usize,
            value: f64,
        }
        impl ti4_engine::choice::Decider for Probe {
            fn choose(
                &mut self,
                choice: &ti4_engine::choice::Choice,
            ) -> Result<ti4_engine::choice::ChoiceOption, ti4_engine::choice::IllegalChoice>
            {
                Ok(choice.options[0].clone())
            }
            fn choose_seeing(
                &mut self,
                choice: &ti4_engine::choice::Choice,
                seen: &ti4_engine::choice::SeatObservation<'_>,
            ) -> Result<ti4_engine::choice::ChoiceOption, ti4_engine::choice::IllegalChoice>
            {
                let vector = ti4_policy::critic::critic_vector(
                    seen,
                    ti4_policy::critic::CriticFeatures::full(),
                );
                self.names = vector.facts().len();
                let input = ti4_mlp::CriticInput::new(&vector, &self.vocabulary);
                self.columns = input.distinct_columns();
                self.value = self.actor.value(&input, self.row).unwrap_or(f64::NAN);
                Ok(choice.options[0].clone())
            }
        }

        let mut probe = Probe {
            actor: Actor::zeros(Width::W256, capacity),
            vocabulary: Vocabulary::from_json(&text).expect("vocabulary"),
            row: FactionRow::of(factions[&players[0]].as_str()).expect("roster"),
            columns: 0,
            names: 0,
            value: f64::NAN,
        };
        let choice = ti4_engine::choice::Choice::new(
            players[0].clone(),
            "value probe",
            vec![ti4_engine::choice::ChoiceOption::new("noop", "noop")],
        );
        if let Err(error) = ti4_engine::choice::ask_private(
            &choice,
            &game.state,
            content,
            DEFAULT,
            game.galaxy(),
            &mut probe,
        ) {
            refuse(&format!("the value probe could not be answered: {error}"));
        }
        // Plumbing only, and worth saying so: this actor is zero-initialised like the ones the
        // seats use, so `V` is 0 by construction and its value carries no information. What is
        // being checked is that the gather, the trunk and the readout complete over real columns
        // without producing a non-finite number.
        if !probe.value.is_finite() {
            refuse("V is not finite against the accepted vocabulary");
        }
        // The load-bearing number. One column means every critic fact landed on the same row.
        if probe.columns <= 1 {
            refuse(&format!(
                "the critic collapsed onto {} column(s): {} facts, so V is a rank-1 projection",
                probe.columns, probe.names
            ));
        }
        println!(
            "critic: {} facts over {} distinct columns; V = {:.6} (zero actor, plumbing only)",
            probe.names, probe.columns, probe.value
        );
    }
    let assigned: usize = statuses
        .iter()
        .map(|s| s.counters().assigned.load(Ordering::Relaxed))
        .sum();
    let oov: usize = statuses
        .iter()
        .map(|s| s.counters().oov.load(Ordering::Relaxed))
        .sum();
    // The status cannot be discarded to reach a success: this is the only accessor, and it returns
    // a Result that carries the fallback count.
    let mut decisions = 0usize;
    let mut inference_failures = Vec::new();
    for status in statuses {
        match status.into_result() {
            Ok(answered) => decisions += answered,
            Err(failure) => inference_failures.push(failure),
        }
    }
    let looked_up = assigned + oov;
    #[expect(clippy::cast_precision_loss, reason = "reporting only")]
    let coverage = if looked_up == 0 {
        0.0
    } else {
        100.0 * (assigned as f64) / (looked_up as f64)
    };
    let fallbacks: usize = inference_failures.iter().map(|f| f.fallbacks).sum();
    let inside_discovery = (202_608_210..202_608_338).contains(&seed);
    println!(
        "model answered {decisions} decisions, {fallbacks} fallbacks; {looked_up} feature lookups, {coverage:.2}% assigned, {oov} OOV"
    );
    println!(
        "coverage reading: {}",
        if inside_discovery {
            "discovery-regression (seed inside M09-024b2's discovery range: 100% is expected by construction; a shortfall means discovery or the projection regressed)"
        } else {
            "independent (this seed is outside the discovery range)"
        }
    );

    // Fail closed. A run in which the model never answered, or fell back even once, is not a
    // successful smoke — it is a failure that happens to have produced legal moves.
    let mut refusals: Vec<String> = Vec::new();
    for failure in &inference_failures {
        refusals.push(failure.to_string());
    }
    if looked_up == 0 {
        refusals.push("no feature lookups were made".to_owned());
    }
    if completed != rounds && !game.state.finished {
        refusals.push(format!("completed {completed} of {rounds} rounds"));
    }
    if !refusals.is_empty() {
        eprintln!("SMOKE FAILED: {}", refusals.join("; "));
        std::process::exit(4);
    }

    for player in &players {
        let seat = game.state.player(player).expect("seated");
        println!(
            "  {:<6} {:<8} {:>2} VP",
            player.as_str(),
            seat.faction.as_str(),
            seat.victory_points
        );
    }
}
