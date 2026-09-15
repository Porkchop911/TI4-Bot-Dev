//! Deterministic structured-diplomacy rules.

pub mod candidates;
pub mod log;
pub mod promises;
pub mod relations;
pub mod signals;
mod transfers;
pub mod window;

pub use log::{DiplomacyLogError, export_log_records};
pub use promises::{
    DiplomacyEventContext, PromiseError, evaluate_event, fulfill_payment, settle_deadlines,
    settle_game_end,
};
pub use relations::{
    RelationshipEvent, apply_relationship_event, decay_relationships, recent_attack, recent_breach,
};
