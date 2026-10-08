//! Semantic golden for the reviewer's real learned-policy path.
//!
//! # What this proves, and what it does not
//!
//! `LiveReview::start_with_control` is the seam R02 uses to put a manual-seat decorator underneath
//! the reviewer's trace wrapper. Two separate claims need evidence, and this file covers the second:
//!
//! 1. *Adding the seam did not change the existing path.* Measured by running the pre-seam and
//!    post-seam reviewer over identical inputs and comparing their saved sessions; recorded in
//!    `plans/evidence/R02-002.md`. The golden below is the projection of that pre-seam-equal
//!    session, so the assertion here is not circular.
//! 2. *The seam is transparent and an override under the trace still reports the policy's own
//!    scores.* Asserted below against this golden and against the recorded decision rows.
//!
//! # Inputs
//!
//! The bundle and map pool are the reviewer's committed example inputs, so these tests need nothing
//! outside Git and never skip. `TI4_REVIEW_GOLDEN_CHECKPOINT` / `TI4_REVIEW_GOLDEN_POOL` override
//! them when a newer bundle needs the same comparison.
//!
//! # What the golden leaves out
//!
//! Filesystem paths (machine-specific), `engine_commit`/`engine_dirty` (they change on every commit,
//! so including them would make the golden rot), the policy `source` string, and the per-option
//! feature vectors and outcome previews (large; their presence is recorded as counts). Input identity
//! is carried by `checkpoint_sha256`, `map_pool_sha256` and `content_sha256` instead.

use std::cell::RefCell;
use std::env;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use ti4_engine::choice::{Choice, ChoiceOption, Decider, IllegalChoice, SeatObservation};
use ti4_model::id::PlayerId;
use ti4_review::{
    AdvanceUnit, LiveReview, PolicyHook, ProfileTable, ReviewFrame, SimulationConfig,
};

const CHECKPOINT: &str = "examples/reviewer/checkpoint-473312/slots.json";
const MAP_POOL: &str = "examples/reviewer/full_np8_12_holdout.json";
const GOLDEN: &str = "tests/golden/pre-r02-mlp-seed7777-rotation2.json";

const SEED: u64 = 7_777;
const ROTATION: usize = 2;
const TEMPERATURE: f64 = 0.5;
/// Long enough to cross several action periods and both seat-side decision paths.
const STEPS: usize = 240;
/// The override test only needs the first few decisions of one seat.
const OVERRIDE_STEPS: usize = 80;

/// Cargo runs a test binary with the *package* directory as its working directory, so the
/// workspace-relative defaults have to be anchored rather than trusted from wherever the shell was.
fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crates/<name> sits directly under the workspace root")
        .to_path_buf()
}

fn env_path(name: &str, default: &str) -> PathBuf {
    env::var_os(name).map_or_else(|| workspace_root().join(default), PathBuf::from)
}

fn golden_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(GOLDEN)
}

fn config() -> SimulationConfig {
    SimulationConfig {
        checkpoint: env_path("TI4_REVIEW_GOLDEN_CHECKPOINT", CHECKPOINT),
        map_pool: env_path("TI4_REVIEW_GOLDEN_POOL", MAP_POOL),
        seed: SEED,
        rotation: ROTATION,
        table: ProfileTable::Learner,
        temperature: TEMPERATURE,
        diplomacy: false,
        lineup: None,
    }
}

/// The example inputs are tracked in Git, so a missing one is a broken checkout, not a skip.
///
/// A silently skipped golden proves nothing, which is why there is deliberately no skip path here:
/// the two files are part of the repository, and the check exists only to name them in the failure.
fn assert_inputs_present() {
    let config = config();
    for path in [&config.checkpoint, &config.map_pool] {
        assert!(
            path.is_file(),
            "reviewer golden input {} is missing from the checkout",
            path.display()
        );
    }
}

fn digest(value: &str) -> String {
    Sha256::digest(value.as_bytes())
        .iter()
        .fold(String::new(), |mut out, byte| {
            use std::fmt::Write as _;
            let _ = write!(out, "{byte:02x}");
            out
        })
}

/// Six decimals as text: catches a changed score, stable across runs.
fn number(value: Option<f64>) -> String {
    value.map_or_else(|| "-".to_owned(), |value| format!("{value:.6}"))
}

fn frame_projection(frame: &ReviewFrame) -> Value {
    json!({
        "index": frame.index,
        "engine_step": frame.engine_step,
        "decision_count": frame.decision_count,
        "action_count": frame.action_count,
        "round": frame.round,
        "phase": frame.phase,
        "active": frame.active,
        "resolved_choice": frame.resolved_choice,
        "action_completed": frame.action_completed,
        "finished": frame.finished,
        "error": frame.error,
        "new_events": [
            frame.new_events.len(),
            digest(&frame.new_events.join("\n")),
        ],
        "structured_events": [
            frame.structured_events.len(),
            digest(
                &serde_json::to_string(&frame.structured_events)
                    .expect("structured events serialize"),
            ),
        ],
        "action_summary": frame.action_summary.as_ref().map(|summary| {
            let text = serde_json::to_string(summary).expect("action summary serializes");
            [summary.actor.clone(), digest(&text)]
        }),
        "decisions": frame
            .decisions
            .iter()
            .map(|decision| {
                json!({
                    "sequence": decision.sequence,
                    "player": decision.player,
                    "faction": decision.faction,
                    "prompt": decision.prompt,
                    "path": decision.path,
                    "requested_head": decision.requested_head,
                    "resolved_head": decision.resolved_head,
                    "temperature": number(decision.temperature),
                    "chosen": decision.chosen,
                    "context": decision.context,
                    "options": decision
                        .options
                        .iter()
                        .map(|option| {
                            json!({
                                "id": option.id,
                                "kind": option.kind,
                                "label": option.label,
                                "payload": digest(&serde_json::to_string(&option.payload)
                                    .expect("payload serializes")),
                                "preview": option.preview.is_some(),
                                "score": number(option.score),
                                "probability": number(option.probability),
                                "features": option.features.len(),
                            })
                        })
                        .collect::<Vec<_>>(),
                })
            })
            .collect::<Vec<_>>(),
    })
}

/// The canonical session projection the golden stores.
fn projection(review: &LiveReview) -> Value {
    let manifest = &review.session.manifest;
    json!({
        "checkpoint_sha256": manifest.checkpoint_sha256,
        "map_pool_sha256": manifest.map_pool_sha256,
        "content_sha256": manifest.content_sha256,
        "seed": manifest.seed,
        "tile_seed": manifest.tile_seed,
        "rotation": manifest.rotation,
        "profile_table": manifest.profile_table,
        "temperature": manifest.temperature,
        "diplomacy": manifest.diplomacy,
        "factions": manifest.factions,
        "initial_speaker": manifest.initial_speaker,
        "map_arrangement_index": manifest.map_arrangement_index,
        "map_arrangement_sha256": manifest.map_arrangement_sha256,
        "policy": {
            "format": manifest.policy.format,
            "schema": manifest.policy.schema,
            "name": manifest.policy.name,
            "git_commit": manifest.policy.git_commit,
            "update": manifest.policy.update,
            "dimensions": manifest.policy.dimensions,
            "heads": manifest.policy.heads,
            "factions": manifest.policy.factions,
            "profiles": manifest.policy.profiles,
            "projection_abi": manifest.policy.projection_abi,
            "oov_registry_version": manifest.policy.oov_registry_version,
            "critic_mode": manifest.policy.critic_mode,
            "trained_temperature": manifest.policy.trained_temperature,
        },
        "outcome": review.session.outcome,
        "frames": review
            .session
            .frames
            .iter()
            .map(frame_projection)
            .collect::<Vec<_>>(),
    })
}

fn start(hook: Option<PolicyHook<'_>>, steps: usize) -> LiveReview {
    let mut review = match hook {
        None => LiveReview::start(&config()).expect("reviewer starts on the golden inputs"),
        Some(hook) => LiveReview::start_with_control(&config(), hook)
            .expect("reviewer starts on the golden inputs with a hook"),
    };
    review.advance(AdvanceUnit::Step, steps);
    review
}

fn golden_text() -> String {
    let path = golden_path();
    std::fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "read {}: {error}; regenerate with TI4_REVIEW_GOLDEN_UPDATE=1 only after \
             explaining why intended behaviour changed",
            path.display()
        )
    })
}

fn compare_with_golden(actual: &Value) {
    let text = serde_json::to_string_pretty(actual).expect("projection serializes");
    if env::var_os("TI4_REVIEW_GOLDEN_UPDATE").is_some() {
        let path = golden_path();
        std::fs::create_dir_all(path.parent().expect("golden has a parent"))
            .expect("golden directory exists or is created");
        std::fs::write(&path, format!("{text}\n")).expect("golden writes");
        eprintln!("wrote {} ({} bytes)", path.display(), text.len());
        return;
    }
    // Compared as parsed JSON rather than as text: the repository normalises end-of-line endings on
    // checkout for most file types, and a fixture that depends on byte-exact newlines has already
    // bitten this project once.
    let want: Value = serde_json::from_str(&golden_text()).expect("golden parses");
    if &want == actual {
        return;
    }
    let mut first_difference =
        "the session projection differs before any frame: the manifest or outcome changed"
            .to_owned();
    if let (Some(want), Some(got)) = (want["frames"].as_array(), actual["frames"].as_array()) {
        for (index, (want, got)) in want.iter().zip(got.iter()).enumerate() {
            if want != got {
                first_difference = format!("frame {index}\n  golden:  {want}\n  current: {got}");
                break;
            }
        }
    }
    panic!(
        "reviewer semantic golden changed; regenerate only after explaining why\n\
         {first_difference}"
    );
}

#[test]
fn the_reviewers_default_path_matches_the_frozen_pre_r02_golden() {
    assert_inputs_present();
    compare_with_golden(&projection(&start(None, STEPS)));
}

#[test]
fn an_identity_hook_reproduces_the_default_path_exactly() {
    assert_inputs_present();
    let seats = Rc::new(RefCell::new(Vec::<String>::new()));
    let recorded = Rc::clone(&seats);
    let hooked = projection(&start(
        Some(&|player: &PlayerId, policy: Box<dyn Decider>| {
            recorded.borrow_mut().push(player.to_string());
            policy
        }),
        STEPS,
    ));
    let default = projection(&start(None, STEPS));
    assert_eq!(
        hooked, default,
        "an identity hook changed the session, so the hook is not transparent"
    );
    assert_eq!(
        seats
            .borrow_mut()
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        vec!["seat0", "seat1", "seat2", "seat3", "seat4", "seat5"],
        "the hook must be handed each physical seat exactly once, in setup order"
    );
}

/// Counts how often the policy underneath an override is actually consulted.
struct Counting {
    inner: Box<dyn Decider>,
    calls: Rc<RefCell<usize>>,
}

impl Decider for Counting {
    fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
        *self.calls.borrow_mut() += 1;
        self.inner.choose(choice)
    }

    fn choose_seeing(
        &mut self,
        choice: &Choice,
        seen: &SeatObservation<'_>,
    ) -> Result<ChoiceOption, IllegalChoice> {
        *self.calls.borrow_mut() += 1;
        self.inner.choose_seeing(choice, seen)
    }
}

/// Answers with the last option on offer and never asks the policy below it — the shape of a human
/// clicking the bottom row of the manual panel.
struct LastOption {
    /// Kept because a decorator that answers for a policy still has to be constructible from one;
    /// this test is precisely the case where it must not be consulted.
    _inner: Box<dyn Decider>,
    answered: Rc<RefCell<usize>>,
}

impl Decider for LastOption {
    fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
        *self.answered.borrow_mut() += 1;
        choice
            .options
            .last()
            .cloned()
            .ok_or_else(|| IllegalChoice::NoOptions {
                player: choice.player.clone(),
                prompt: choice.prompt.clone(),
            })
    }

    fn choose_seeing(
        &mut self,
        choice: &Choice,
        _seen: &SeatObservation<'_>,
    ) -> Result<ChoiceOption, IllegalChoice> {
        *self.answered.borrow_mut() += 1;
        choice
            .options
            .last()
            .cloned()
            .ok_or_else(|| IllegalChoice::NoOptions {
                player: choice.player.clone(),
                prompt: choice.prompt.clone(),
            })
    }
}

/// Whether a traced row is a real engine decision.
///
/// The reviewer's trace also carries rows the engine never asked — the fleet package a plan sampled
/// and the notice that a plan stopped — and their option ids (`package|N`) belong to the plan, not to
/// a decision. Replaying or asserting over them as if they were answers is the mistake R02's own
/// replay record exists to avoid, so it is filtered here too.
fn real_decision(path: &str) -> bool {
    path != "fleet decision" && path != "plan stopped"
}

#[test]
fn an_override_under_the_trace_still_reports_the_policys_own_scores() {
    assert_inputs_present();
    let inner_calls = Rc::new(RefCell::new(0_usize));
    let counting = Rc::clone(&inner_calls);
    let answered = Rc::new(RefCell::new(0_usize));
    let answer_count = Rc::clone(&answered);
    let wrapped = Rc::new(RefCell::new(0_usize));
    let wrap_count = Rc::clone(&wrapped);

    let review = start(
        Some(
            &move |player: &PlayerId, policy: Box<dyn Decider>| -> Box<dyn Decider> {
                if player.as_str() != "seat0" {
                    return policy;
                }
                *wrap_count.borrow_mut() += 1;
                Box::new(LastOption {
                    _inner: Box::new(Counting {
                        inner: policy,
                        calls: Rc::clone(&counting),
                    }),
                    answered: Rc::clone(&answer_count),
                })
            },
        ),
        OVERRIDE_STEPS,
    );

    assert_eq!(
        *wrapped.borrow(),
        1,
        "the hook wraps each seat once, at setup"
    );
    let overridden: Vec<_> = review
        .session
        .frames
        .iter()
        .flat_map(|frame| frame.decisions.iter())
        .filter(|decision| decision.player == "seat0" && real_decision(&decision.path))
        .collect();
    assert!(
        overridden.len() >= 3,
        "seat0 was asked too few real decisions in {OVERRIDE_STEPS} steps to prove anything"
    );
    for decision in &overridden {
        let last = decision.options.last().unwrap_or_else(|| {
            panic!(
                "seat0 traced a choice with no options: {:?}",
                decision.prompt
            )
        });
        assert_eq!(
            decision.chosen.as_deref(),
            Some(last.id.as_str()),
            "the override did not answer for seat0 at sequence {:?}",
            decision.sequence
        );
        assert!(
            decision
                .options
                .iter()
                .all(|option| option.score.is_some() && option.probability.is_some()),
            "an overridden choice lost the policy's own scores: {:?}",
            decision.prompt
        );
    }
    assert_eq!(
        *answered.borrow(),
        overridden.len(),
        "every real seat0 decision should have been answered by the override"
    );
    assert_eq!(
        *inner_calls.borrow(),
        0,
        "the policy under the override must not be consulted when a person answers"
    );
    assert!(
        !review
            .session
            .frames
            .iter()
            .any(|frame| frame.error.is_some()),
        "overriding one seat must not put the engine into an error state"
    );
}
