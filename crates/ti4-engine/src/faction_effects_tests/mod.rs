//! One test per faction effect (faction ability, faction technology, agent, commander, hero,
//! promissory note, Thunder's Edge breakthrough), so "the card was in the game and did nothing"
//! cannot happen unnoticed. The coverage table is `docs/FACTION_EFFECT_COVERAGE.md`.
//!
//! Test names: `<card id>_<effect no.>_<what>` (the id with `-` written `_`). An effect the engine
//! does not implement (or implements wrongly) keeps its test and is marked
//! `#[ignore = "MISSING: <card id> ..."]`; `cargo test -p ti4-engine --lib faction_effects_tests
//! -- --ignored` lists them. Faction technologies already covered by
//! `technology_effects_tests` are only pointed to in the table.
//!
//! Group files: `f1` base game factions (Arborec..Naalu), `f2` base game factions (Nekro..Keleres),
//! `f3` Prophecy of Kings factions, `f4` Thunder's Edge factions and generic promissory notes.
//! Shared helpers live here; the technology tests' helpers (`give`, `seated`, `emit_event`, ...)
//! are reused.

#![allow(dead_code, unused_imports)]

use std::collections::BTreeMap;

use ti4_model::content_types::{DEFAULT, SourceSet};
use ti4_model::state::GameState;

pub(crate) use crate::technology_effects_tests::{content, emit_event, give, owns, pid, seated};

mod f1;
mod f2;
mod f3;
mod f4;
mod v16;

/// A seated, deployed game of `(seat, faction)` pairs with every content source (Thunder's Edge,
/// Codex and Prophecy of Kings factions included).
pub(crate) fn seated_all(seats: &[(&str, &str)]) -> GameState {
    crate::fixtures::seated_game(seats, DEFAULT)
}

/// The sources the full-content tests run under.
pub(crate) const ALL: SourceSet = DEFAULT;

/// A table answering every question from `answers`, in order.
pub(crate) fn scripted(answers: &[&str]) -> crate::choice::Table {
    crate::choice::Table::with_default(Box::new(crate::choice::Scripted::new(
        answers.iter().map(|answer| (*answer).to_owned()),
    )))
}

/// Emit one typed event (JSON payload) through a resolver armed exactly as the game arms it.
pub(crate) fn emit_full(
    state: &mut GameState,
    table: &mut crate::choice::Table,
    sources: SourceSet,
    event_type: &str,
    payload: BTreeMap<String, serde_json::Value>,
) {
    let mut resolver = crate::fixtures::armed_resolver(state);
    let mut dice = crate::dice::Dice::new();
    let mut rng = crate::rng::GameRng::new(0);
    let mut sequence = crate::event::EventSequence::new();
    let mut context = crate::timing::TimingContext {
        state,
        content: content(),
        sources,
        table,
        dice: &mut dice,
        rng: &mut rng,
        event_sequence: &mut sequence,
        galaxy: None,
    };
    let event = context
        .event_sequence
        .next(event_type, payload)
        .expect("an event id");
    resolver
        .emit_with_context(&mut context, event, |_, _| {})
        .expect("the window resolves");
}

/// Payload from string pairs.
pub(crate) fn strs(pairs: &[(&str, &str)]) -> BTreeMap<String, serde_json::Value> {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_owned(), serde_json::Value::from(*v)))
        .collect()
}
