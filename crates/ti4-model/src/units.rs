//! Units on the board.
//!
//! A unit is a *value*, not an entity: two of a player's fighters in the same system are
//! interchangeable and carry no identity of their own. That is why removing a unit means
//! "remove one like this" rather than "remove the one with this handle", and why a damaged
//! dreadnought is represented by replacing the unit rather than mutating it in place.
//!
//! Unit *statistics* are not here. They live in the content corpus and are read through
//! `ti4_content::units::UnitType`, so a unit upgrade is a different record with the same
//! `baseType` and upgrading is a lookup rather than a branch.

use serde::{Deserialize, Serialize};

use crate::id::{PlayerId, UnitTypeId};

/// One unit on the board.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Unit {
    /// The content id of this unit's type, e.g. `carrier` or `sol_flagship`.
    pub type_id: UnitTypeId,
    pub owner: PlayerId,
    /// Whether this unit has used Sustain Damage and not yet been repaired.
    pub sustained_damage: bool,
    /// Thunder's Edge galvanize token. It changes combat modifiers and is referenced by
    /// Proxima Targeting VI; keeping it on the unit preserves replay state.
    pub galvanized: bool,
}

impl Unit {
    #[must_use]
    pub const fn new(type_id: UnitTypeId, owner: PlayerId) -> Self {
        Self {
            type_id,
            owner,
            sustained_damage: false,
            galvanized: false,
        }
    }

    /// The same unit, marked as having sustained damage.
    #[must_use]
    pub fn sustained(&self) -> Self {
        Self {
            sustained_damage: true,
            ..self.clone()
        }
    }

    /// The same unit, repaired.
    #[must_use]
    pub fn repaired(&self) -> Self {
        Self {
            sustained_damage: false,
            ..self.clone()
        }
    }

    /// The same unit, galvanized.
    #[must_use]
    pub fn galvanized(&self) -> Self {
        Self {
            galvanized: true,
            ..self.clone()
        }
    }
}

/// Which side of a combat a player is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CombatSide {
    Attacker,
    Defender,
}

impl CombatSide {
    #[must_use]
    pub const fn other(self) -> Self {
        match self {
            Self::Attacker => Self::Defender,
            Self::Defender => Self::Attacker,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fighter() -> Unit {
        Unit::new(UnitTypeId::new("fighter"), PlayerId::new("a"))
    }

    #[test]
    fn two_units_of_the_same_type_and_owner_are_interchangeable() {
        assert_eq!(fighter(), fighter());
    }

    #[test]
    fn a_damaged_unit_is_a_different_value() {
        let dread = Unit::new(UnitTypeId::new("dreadnought"), PlayerId::new("a"));
        let hurt = dread.sustained();
        assert_ne!(dread, hurt);
        assert!(hurt.sustained_damage);
        assert_eq!(hurt.repaired(), dread);
    }

    #[test]
    fn ownership_distinguishes_otherwise_identical_units() {
        let mine = fighter();
        let theirs = Unit::new(UnitTypeId::new("fighter"), PlayerId::new("b"));
        assert_ne!(mine, theirs);
    }

    #[test]
    fn galvanizing_is_recorded_on_the_unit_so_it_survives_a_replay() {
        let unit = fighter().galvanized();
        assert!(unit.galvanized);
        let json = serde_json::to_string(&unit).unwrap();
        assert_eq!(serde_json::from_str::<Unit>(&json).unwrap(), unit);
    }

    #[test]
    fn combat_sides_are_opposites() {
        assert_eq!(CombatSide::Attacker.other(), CombatSide::Defender);
        assert_eq!(CombatSide::Defender.other().other(), CombatSide::Defender);
    }
}
