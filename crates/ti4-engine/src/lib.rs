//! The TI4 rules engine.
//!
//! # State of this crate
//!
//! Small, and honest about it. It holds setup, the phase state machine, and turn order,
//! ported from the oracle's `engine/game.py`. It does **not** yet hold movement, combat,
//! production, legality checking, the status phase, or the agenda phase.
//!
//! An earlier version of this crate had modules named for all of those. They were removed
//! rather than adapted: `rules.rs` returned `Ok(true)` from all 23 of its validators,
//! `tactical.rs` reported every system as one step from every other and moved no units, and
//! `effects.rs` gave every unit in the game a combat value of 1. They were written against
//! an invented model of TI4 — at one point a five-card strategy deck — and compiled, passed
//! tests, and read as implemented. See `plans/AUDIT_2026-08-11_PLAN_VS_TREE.md`.
//!
//! What replaces them is ported against a named oracle source with the oracle's own tests
//! mirrored, and what is not ported yet is absent rather than faked.

pub mod phase;
pub mod seating;
pub mod setup;

pub use phase::{
    PhaseOutcome, advance_phase, advance_turn, begin_action_turn, begin_next_round,
    next_strategy_picker, stock_unclaimed_cards, strategy_pick_order,
};
pub use seating::{SeatingError, build_board, deploy, home_systems, neutral_systems};
pub use setup::{SetupError, cards_per_player, start_game, strategy_card_setup};
