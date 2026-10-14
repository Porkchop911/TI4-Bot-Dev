//! What a public objective asks for, in groups.
//!
//! The corpus prints the stage of a public objective and nothing else. A reader of the objectives
//! that a game can still reveal wants them in groups by what they ask of a player, as the secret
//! objectives are ([`crate::secret_objectives`]). The groups are written by hand here, beside the
//! corpus and not in it.
//!
//! The key is the alias of the objective. An alias that is not listed has no group; the test of
//! this module fails for it.

/// The group of what a public objective asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PublicObjectiveGroup {
    /// Spend resources, influence, trade goods or command tokens.
    Spend,
    /// Control planets of a kind or in a place.
    Planets,
    /// Have ships or units in certain systems.
    Units,
    /// Have structures.
    Structures,
    /// Own technologies.
    Technology,
}

impl PublicObjectiveGroup {
    /// Every group, in the order in which a reader is shown them.
    pub const ALL: [Self; 5] = [
        Self::Spend,
        Self::Planets,
        Self::Units,
        Self::Structures,
        Self::Technology,
    ];

    /// The name of the group for a reader.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Spend => "Spend",
            Self::Planets => "Planets you control",
            Self::Units => "Units on the board",
            Self::Structures => "Structures",
            Self::Technology => "Technology",
        }
    }
}

/// The group of the public objective with this `alias`. `None` for an alias that this module does
/// not know.
#[must_use]
pub fn group_of(alias: &str) -> Option<PublicObjectiveGroup> {
    use PublicObjectiveGroup as G;
    Some(match alias {
        // Stage I: Amass Wealth, Lead From the Front, Erect a Monument, Sway the Council,
        // Negotiate Trade Routes. Stage II: Centralize Galactic Trade, Galvanize the People,
        // Found a Golden Age, Manipulate Galactic Law, Hold Vast Reserves.
        "amass_wealth" | "lead" | "monument" | "sway_council" | "trade_routes"
        | "centralize_trade" | "galvanize" | "golden_age" | "manipulate_law" | "vast_reserves" => {
            G::Spend
        }
        // Stage I: Corner the Market, Expand Borders, Discover Lost Outposts, Push Boundaries,
        // Found Research Outposts. Stage II: Reclaim Ancient Monuments, Form Galactic Brain
        // Trust, Conquer the Weak, Rule Distant Lands, Subdue the Galaxy, Unify the Colonies.
        "corner" | "expand_borders" | "lost_outposts" | "push_boundaries" | "research_outposts"
        | "ancient_monuments" | "brain_trust" | "conquer" | "distant_lands" | "subdue"
        | "unify_colonies" => G::Planets,
        // Stage I: Explore Deep Space, Engineer a Marvel, Intimidate Council, Make History,
        // Populate the Outer Rim, Raise a Fleet. Stage II: Become a Legend, Command an Armada,
        // Control the Borderlands, Achieve Supremacy, Patrol Vast Territories.
        "deep_space"
        | "engineer_marvel"
        | "intimidate"
        | "make_history"
        | "outer_rim"
        | "raise_fleet"
        | "become_legend"
        | "command_armada"
        | "control_borderlands"
        | "supremacy"
        | "vast_territories" => G::Units,
        // Stage I: Build Defenses, Improve Infrastructure. Stage II: Construct Massive Cities,
        // Protect the Border.
        "build_defenses" | "infrastructure" | "massive_cities" | "protect_border" => G::Structures,
        // Stage I: Develop Weaponry, Diversify Research. Stage II: Master the Sciences,
        // Revolutionize Warfare.
        "develop" | "diversify" | "master_science" | "revolutionize" => G::Technology,
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
    fn every_public_objective_of_the_corpus_has_a_group() {
        let mut counts: BTreeMap<PublicObjectiveGroup, usize> = BTreeMap::new();
        for record in ContentStore::embedded().records(ContentType::PublicObjectives) {
            let alias = record.id().expect("an alias");
            let group = group_of(alias).unwrap_or_else(|| panic!("no group for {alias:?}"));
            *counts.entry(group).or_default() += 1;
        }
        let count = |group| counts.get(&group).copied().unwrap_or(0);

        assert_eq!(count(PublicObjectiveGroup::Spend), 10);
        assert_eq!(count(PublicObjectiveGroup::Planets), 11);
        assert_eq!(count(PublicObjectiveGroup::Units), 11);
        assert_eq!(count(PublicObjectiveGroup::Structures), 4);
        assert_eq!(count(PublicObjectiveGroup::Technology), 4);
    }
}
