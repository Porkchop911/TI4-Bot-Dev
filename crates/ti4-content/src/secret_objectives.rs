//! What a secret objective asks for, in groups.
//!
//! The corpus prints the phase in which a secret objective is scored and nothing else. A reader of
//! the whole deck wants the objectives in groups by what they ask of a player, and no field of the
//! corpus has one. The groups are written by hand here, beside the corpus and not in it: the files
//! of `content/` are copies of the oracle.
//!
//! The key is the alias of the objective. An alias that is not listed has no group; the test of
//! this module fails for it.

/// The group of what a secret objective asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SecretObjectiveGroup {
    /// Control planets, or lose one.
    Planets,
    /// Have ships in certain systems.
    Ships,
    /// Have units or structures of a kind, wherever they are.
    Units,
    /// Own technologies, hold or spend cards.
    TechnologyAndCards,
    /// Win a combat.
    Combat,
    /// Destroy units of another player.
    Destroy,
    /// The agenda phase, the neighbors and the order of the round.
    Table,
}

impl SecretObjectiveGroup {
    /// Every group, in the order in which a reader is shown them.
    pub const ALL: [Self; 7] = [
        Self::Planets,
        Self::Ships,
        Self::Units,
        Self::TechnologyAndCards,
        Self::Combat,
        Self::Destroy,
        Self::Table,
    ];

    /// The name of the group for a reader.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Planets => "Planets you control",
            Self::Ships => "Ships on the board",
            Self::Units => "Units and structures",
            Self::TechnologyAndCards => "Technology and cards",
            Self::Combat => "Win a combat",
            Self::Destroy => "Destroy units",
            Self::Table => "Agenda and table",
        }
    }
}

/// The group of the secret objective with this `alias`. `None` for an alias that this module does
/// not know.
#[must_use]
pub fn group_of(alias: &str) -> Option<SecretObjectiveGroup> {
    use SecretObjectiveGroup as G;
    Some(match alias {
        // Forge an Alliance, Mine Rare Metals, Monopolize Production, Establish Hegemony, Hoard
        // Raw Materials, Seize an Icon, Stake Your Claim, Occupy the Seat of the Empire, Become a
        // Martyr.
        "faa" | "mrm" | "mp" | "eh" | "hrm" | "sai" | "syc" | "ose" | "bam" => G::Planets,
        // Become the Gatekeeper, Control the Region, Cut Supply Lines, Learn the Secrets of the
        // Cosmos, Threaten Enemies, Defy Space and Time.
        "btgk" | "ctr" | "csl" | "lsc" | "te" | "dfat" => G::Ships,
        // Establish a Perimeter, Fuel the War Machine, Gather a Mighty Fleet, Mechanize the
        // Military, Occupy the Fringe, Produce en Masse.
        "eap" | "fwm" | "gamf" | "mtm" | "otf" | "pem" => G::Units,
        // Adapt New Strategies, Master the Laws of Physics, Form a Spy Network, Destroy Heretical
        // Works, Strengthen Bonds.
        "ans" | "mlp" | "fsn" | "dhw" | "sb" => G::TechnologyAndCards,
        // Spark a Rebellion, Unveil Flagship, Betray a Friend, Brave the Void, Darken the Skies,
        // Demonstrate Your Power.
        "sar" | "uf" | "baf" | "btv" | "dts" | "dyp" => G::Combat,
        // Destroy Their Greatest Ship, Make an Example of Their World, Turn Their Fleets to Dust,
        // Fight with Precision.
        "dtgs" | "mew" | "ttfd" | "fwp" => G::Destroy,
        // Dictate Policy, Drive the Debate, Foster Cohesion, Prove Endurance.
        "dp" | "dtd" | "fc" | "pe" => G::Table,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use ti4_model::content_types::ContentType;

    use super::*;
    use crate::loader::ContentStore;

    #[test]
    fn every_secret_objective_of_the_corpus_has_a_group() {
        let mut counts: BTreeMap<SecretObjectiveGroup, usize> = BTreeMap::new();
        for record in ContentStore::embedded().records(ContentType::SecretObjectives) {
            let alias = record.id().expect("an alias");
            let group = group_of(alias).unwrap_or_else(|| panic!("no group for {alias:?}"));
            *counts.entry(group).or_default() += 1;
        }
        let count = |group| counts.get(&group).copied().unwrap_or(0);

        assert_eq!(count(SecretObjectiveGroup::Planets), 9);
        assert_eq!(count(SecretObjectiveGroup::Ships), 6);
        assert_eq!(count(SecretObjectiveGroup::Units), 6);
        assert_eq!(count(SecretObjectiveGroup::TechnologyAndCards), 5);
        assert_eq!(count(SecretObjectiveGroup::Combat), 6);
        assert_eq!(count(SecretObjectiveGroup::Destroy), 4);
        assert_eq!(count(SecretObjectiveGroup::Table), 4);
    }
}
