//! When an action card is played, in groups.
//!
//! The corpus prints the timing of a card as free text (`window`), with some fifty different
//! texts. A reader of the whole deck wants them in a few groups, and no field of the corpus has
//! one. The groups are written by hand here, beside the corpus and not in it: the files of
//! `content/` are copies of the oracle.
//!
//! The key is the printed window and not the card, so the copies of a card and the cards of one
//! timing cannot disagree. A window that is not listed has no group; the test of this module
//! fails for it.

/// The group of the timing of an action card.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ActionCardTiming {
    /// "ACTION": the card is a component action.
    Action,
    /// After the player of the card activates a system.
    OwnActivation,
    /// After another player activates a system or moves ships into it.
    OtherActivation,
    /// A combat, and the rolls of the unit abilities around it.
    Combat,
    /// An invasion: the landing, the ground combat and the control of a planet.
    Invasion,
    /// The turn or the action of a player, outside of a tactical action.
    Turn,
    StrategyPhase,
    StatusPhase,
    AgendaPhase,
    /// In every phase.
    AnyTime,
}

impl ActionCardTiming {
    /// Every group, in the order in which a reader is shown them.
    pub const ALL: [Self; 10] = [
        Self::Action,
        Self::OwnActivation,
        Self::OtherActivation,
        Self::Combat,
        Self::Invasion,
        Self::Turn,
        Self::StrategyPhase,
        Self::StatusPhase,
        Self::AgendaPhase,
        Self::AnyTime,
    ];

    /// The name of the group for a reader.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Action => "Action",
            Self::OwnActivation => "Your activation",
            Self::OtherActivation => "Activation of another player",
            Self::Combat => "Combat",
            Self::Invasion => "Invasion",
            Self::Turn => "Turns and actions",
            Self::StrategyPhase => "Strategy phase",
            Self::StatusPhase => "Status phase",
            Self::AgendaPhase => "Agenda phase",
            Self::AnyTime => "Any time",
        }
    }
}

/// The group of a card with this `phase` and `window`, as the corpus prints them. `None` for a
/// window that this module does not know.
#[must_use]
pub fn timing_of(phase: &str, window: &str) -> Option<ActionCardTiming> {
    use ActionCardTiming as T;
    match phase {
        "Agenda" => return Some(T::AgendaPhase),
        "Strategy" => return Some(T::StrategyPhase),
        "Status" => return Some(T::StatusPhase),
        "Any" => return Some(T::AnyTime),
        _ => {}
    }
    Some(match window {
        "Action" => T::Action,

        "After you activate a system"
        | "After you activate a system that contains 1 or more of your ships"
        | "After you activate a system that contains another player's ships"
        | "After you activate an anomaly" => T::OwnActivation,

        "After another player activates a system that contains 1 of your command tokens"
        | "After another player activates a system that contains 1 or more of your structures"
        | "After another player activates a system that contains your units"
        | "After a player moves ships into a system that contains your ships"
        | "After the active player moves ships into the active system during a tactical action" => {
            T::OtherActivation
        }

        "At the start of a combat"
        | "At the start of a combat round"
        | "At the start or end of a combat round"
        | "At the start of the first round of a space combat"
        | "At the start of the 'Announce Retreats' step of space combat, if you are the defender"
        | "After your opponent declares a retreat during a space combat"
        | "Before you assign hits to your ships during a space combat"
        | "When one of your ships uses SUSTAIN DAMAGE during combat"
        | "After another player's ship uses SUSTAIN DAMAGE to cancel a hit produced by your units or abilities"
        | "After 1 of your ships is destroyed during a space combat"
        | "When your last ship in the active system is destroyed"
        | "After you win a space combat"
        | "Before you assign hits produced by another player's SPACE CANNON roll"
        | "Before you roll dice for ANTI-FIGHTER BARRAGE"
        | "After another player makes a BOMBARDMENT, SPACE CANNON, or ANTI-FIGHTER BARRAGE roll" => {
            T::Combat
        }

        "At the start of an invasion"
        | "At the start of an invasion in a system that contains 1 or more of your opponents' PDS units"
        | "After another player commits units to land on a planet you control"
        | "After your ground forces make combat rolls during a round of ground combat"
        | "When you gain control of a planet"
        | "After another player gains control of a planet you control" => T::Invasion,

        "After you perform an action"
        | "When another player would perform a strategic action"
        | "When 1 or more of your units use PRODUCTION"
        | "At the start of another player's turn, if they have a readied strategy card"
        | "At the end of a player's turn, if you have passed"
        | "At the end of any players turn, if there are at least 2 players who have not passed" => {
            T::Turn
        }

        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use ti4_model::content_types::{ContentType, POK};

    use super::*;
    use crate::loader::ContentStore;

    #[test]
    fn every_action_card_of_the_corpus_has_a_group() {
        for record in ContentStore::embedded().records(ContentType::ActionCards) {
            let phase = record.text("phase").unwrap_or_default();
            let window = record.text("window").unwrap_or_default();
            assert!(
                timing_of(phase, window).is_some(),
                "no group for the window {window:?} ({phase})"
            );
        }
    }

    #[test]
    fn the_groups_of_a_game_have_the_cards_that_a_reader_expects() {
        let mut names: BTreeMap<ActionCardTiming, BTreeSet<&str>> = BTreeMap::new();
        for record in ContentStore::embedded().records(ContentType::ActionCards) {
            if !record.in_sources(POK) {
                continue;
            }
            let timing = timing_of(
                record.text("phase").unwrap_or_default(),
                record.text("window").unwrap_or_default(),
            )
            .expect("a group");
            names
                .entry(timing)
                .or_default()
                .insert(record.text("name").unwrap_or_default());
        }
        let count = |timing| names.get(&timing).map_or(0, BTreeSet::len);

        assert_eq!(count(ActionCardTiming::Action), 29);
        assert_eq!(count(ActionCardTiming::OwnActivation), 8);
        assert_eq!(count(ActionCardTiming::OtherActivation), 4);
        assert_eq!(count(ActionCardTiming::Combat), 15);
        assert_eq!(count(ActionCardTiming::Invasion), 8);
        assert_eq!(count(ActionCardTiming::AgendaPhase), 20);
        assert!(names[&ActionCardTiming::AnyTime].contains("Sabotage"));
        assert!(names[&ActionCardTiming::Combat].contains("Morale Boost"));
    }

    #[test]
    fn the_labels_are_all_different() {
        let labels: BTreeSet<_> = ActionCardTiming::ALL.iter().map(|t| t.label()).collect();
        assert_eq!(labels.len(), ActionCardTiming::ALL.len());
    }
}
