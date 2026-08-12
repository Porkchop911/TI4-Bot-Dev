//! The TI4 rules engine.
//!
//! # State of this crate
//!
//! Small, and honest about it. It holds setup, the phase state machine, and turn order,
//! ported from the oracle's `engine/game.py`. It does **not** yet hold movement, combat,
//! production, full status scoring/token choices, or agenda voting/effects. Its structural
//! game driver stops with typed metadata at those remaining decision boundaries.
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

pub mod action_cards;
pub mod agenda;
pub mod choice;
pub mod combat;
pub mod deck;
pub mod dice;
pub mod draft;
#[cfg(test)]
pub mod exploration;
#[cfg(test)]
pub mod fixtures;
pub mod fleet;
pub mod game;
pub mod invasion;
pub mod leaders;
pub mod movement;
pub mod objectives;
pub mod payment;
pub mod phase;
pub mod production;
pub mod rng;
pub mod seating;
pub mod secrets;
pub mod setup;
pub mod status;
pub mod strategy;
pub mod tactical;
pub mod technology;
pub mod tokens;
pub mod transactions;
pub mod transit;
pub mod vote;

pub use agenda::{
    AGENDAS_PER_PHASE, AgendaPhaseError, AgendaPhaseReport, AgendaResolution, RevealedAgenda,
    resolve_agenda_phase,
};
pub use choice::{
    AlwaysDecline, Choice, ChoiceOption, Decider, DecisionLog, DecisionRecord, FirstOption,
    IllegalChoice, Scripted, SeededRandom, Table, distinct_units, first_of_each, options_from,
    unit_label, validate,
};
pub use combat::{
    CombatError, CombatOutcome, absorb_hits, anti_fighter_barrage, combatants, resolve,
    space_cannon_offense,
};
pub use deck::{EXPLORATION_TRAITS, StartingDecks, build_starting_decks};
pub use dice::{Dice, Roll};
pub use draft::{DraftError, STRATEGY_CARD_KIND, strategy_options, take_strategy_card};
pub use game::{Game, GameError, RunError, StepResult};
pub use phase::{
    PhaseOutcome, advance_phase, advance_turn, begin_action_turn, begin_next_round,
    next_strategy_picker, stock_unclaimed_cards, strategy_pick_order,
};
pub use rng::GameRng;
pub use seating::{SeatingError, build_board, deploy, home_systems, neutral_systems};
pub use setup::{SetupError, cards_per_player, start_game, start_game_seeded, strategy_card_setup};
pub use status::{
    StatusPhaseError, StatusPhaseReport, resolve_after_token_gain, resolve_before_token_gain,
    resolve_status_phase,
};
pub use strategy::{
    ACTION_KIND, FOLLOW_SECONDARY_ID, STRATEGIC_ACTION_ID, STRATEGY_KIND, SecondaryResolution,
    StrategyActionError, StrategySecondaryError, StrategySecondaryWindow, begin_strategic_action,
    strategic_action_options, take_strategic_action,
};
pub use tactical::{TacticalError, activatable, activate, movable, movement_options};
pub use tokens::{POOL_KIND, STATUS_TOKENS, TokenGain, TokenGainError, TokenPlacement};
pub use transit::{Cargo, CargoSource, CargoWindow, MoveOutcome, apply_move};
pub use vote::{AGAINST, Ballot, FOR, VoteError, VoteWindow, outcomes, votable_planets};
