//! The per-game strategy card set.
//!
//! A server game plays one of the catalog's `strategy_card_sets`, chosen when it is created.
//! The choice is stored with the lobby and the init record; the cards themselves are baked into
//! the saved initial state, so a restart or a history replacement replays the same cards.
//!
//! Games saved before the option existed carry no value and keep the Prophecy of Kings cards
//! they were recorded with ([`LEGACY`]); only newly created games default to [`DEFAULT`].
//!
//! The sets and their names are in [`crate::setup::card_set`], where both hosts have them.
//! What is of the records of a server is here.

pub use crate::setup::card_set::{
    BASE_GAME_CODEX1, DEFAULT, OFFERED, POK, TE, UnknownCardSet, validate,
};

/// The set a record with no value was recorded with.
pub const LEGACY: &str = POK;

/// The set a stored value stands for: a missing value is a legacy record.
#[must_use]
pub fn resolve(stored: Option<&str>) -> &str {
    stored.unwrap_or(LEGACY)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_offered_sets_validate_and_missing_means_pok() {
        for set in OFFERED {
            assert_eq!(validate(set), Ok(set));
        }
        assert!(validate("base_game").is_err());
        assert!(validate("TE").is_err());
        assert_eq!(resolve(None), "pok");
        assert_eq!(resolve(Some("te")), "te");
        assert_eq!(DEFAULT, "te");
    }
}
