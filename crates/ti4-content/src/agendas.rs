//! What an agenda affects, in groups.
//!
//! The corpus prints whether an agenda is a law or a directive and what is elected, and nothing
//! else. A reader of the whole deck wants the agendas in groups by what they change, and no field
//! of the corpus has one. The groups are written by hand here, beside the corpus and not in it:
//! the files of `content/` are copies of the oracle.
//!
//! The key is the alias of the agenda. An alias that is not listed has no group; the test of this
//! module fails for it.

/// The group of what an agenda affects.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AgendaGroup {
    /// Gives or takes a victory point, or brings an objective.
    Points,
    /// A card that the elected player holds and uses.
    Offices,
    /// Changes one elected planet.
    Planets,
    /// Destroys, places or limits units, or changes a combat.
    Units,
    /// Wormholes and anomalies.
    Map,
    /// Research, and the prerequisites of a technology.
    Technology,
    /// Command tokens, trade goods and action cards.
    Supplies,
    /// The laws in play, who votes, and the strategy cards.
    Votes,
}

impl AgendaGroup {
    /// Every group, in the order in which a reader is shown them.
    pub const ALL: [Self; 8] = [
        Self::Points,
        Self::Offices,
        Self::Planets,
        Self::Units,
        Self::Map,
        Self::Technology,
        Self::Supplies,
        Self::Votes,
    ];

    /// The name of the group for a reader.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Points => "Victory points and objectives",
            Self::Offices => "Ministers and offices",
            Self::Planets => "Planets",
            Self::Units => "Units and combat",
            Self::Map => "Map and movement",
            Self::Technology => "Technology",
            Self::Supplies => "Command tokens, trade goods and cards",
            Self::Votes => "Laws and votes",
        }
    }
}

/// The group of the agenda with this `alias`. `None` for an alias that this module does not know.
#[must_use]
pub fn group_of(alias: &str) -> Option<AgendaGroup> {
    use AgendaGroup as G;
    Some(match alias {
        // Mutiny, Seed of an Empire, Political Censure, Shard of the Throne, The Crown of
        // Emphidia, Holy Planet of Ixth, Classified Document Leaks, Incentive Program, Archived
        // Secret, Search Warrant.
        "mutiny"
        | "seed_empire"
        | "censure"
        | "shard_of_the_throne"
        | "crown_of_emphidia"
        | "holy_planet_of_ixth"
        | "classified"
        | "incentive"
        | "secret"
        | "warrant" => G::Points,
        // The eight ministers, Imperial Arbiter, Committee Formation, Prophecy of Ixth, The Crown
        // of Thalnos.
        "minister_commerce"
        | "minister_exploration"
        | "minister_industry"
        | "minister_peace"
        | "minister_policy"
        | "minister_sciences"
        | "minister_war"
        | "minister_antiquities"
        | "arbiter"
        | "committee"
        | "prophecy"
        | "crown_of_thalnos" => G::Offices,
        // Core Mining, Demilitarized Zone, Senate Sanctuary, Terraforming Initiative, Colonial
        // Redistribution, Compensated Disarmament.
        "core_mining"
        | "demilitarized_zone"
        | "senate_sanctuary"
        | "terraforming_initiative"
        | "redistribution"
        | "disarmament" => G::Planets,
        // Arms Reduction, Conventions of War, Homeland Defense Act, Publicize Weapon Schematics,
        // Regulated Conscription, Swords to Plowshares, Articles of War, Rearmament Agreement.
        "arms_reduction" | "conventions" | "defense_act" | "schematics" | "conscription"
        | "plowshares" | "articles_war" | "rearmament" => G::Units,
        // Enforced Travel Ban, Wormhole Reconstruction, Nexus Sovereignty, Shared Research.
        "travel_ban" | "wormhole_recon" | "nexus" | "shared_research" => G::Map,
        // Anti-Intellectual Revolution, Ixthian Artifact, Wormhole Research, Research Grant
        // Reallocation, the four Research Teams.
        "revolution" | "artifact" | "wormhole_research" | "grant_reallocation" | "rt_biotic"
        | "rt_cybernetic" | "rt_propulsion" | "rt_warfare" => G::Technology,
        // Economic Equality, Executive Sanctions, Unconventional Measures, Clandestine
        // Operations, Fleet Regulations, Armed Forces Standardization, Galactic Crisis Pact.
        "economic_equality" | "sanctions" | "unconventional" | "cladenstine" | "regulations"
        | "standardization" | "crisis" => G::Supplies,
        // Judicial Abolishment, Miscount Disclosed, New Constitution, Representative Government
        // (both printings), Covert Legislation, Public Execution, Checks and Balances.
        "abolishment"
        | "miscount"
        | "constitution"
        | "representative_government"
        | "rep_govt"
        | "covert"
        | "execution"
        | "checks" => G::Votes,
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
    fn every_agenda_of_the_corpus_has_a_group() {
        let mut counts: BTreeMap<AgendaGroup, usize> = BTreeMap::new();
        for record in ContentStore::embedded().records(ContentType::Agendas) {
            let alias = record.id().expect("an alias");
            let group = group_of(alias).unwrap_or_else(|| panic!("no group for {alias:?}"));
            *counts.entry(group).or_default() += 1;
        }
        let count = |group| counts.get(&group).copied().unwrap_or(0);

        assert_eq!(count(AgendaGroup::Points), 10);
        assert_eq!(count(AgendaGroup::Offices), 12);
        assert_eq!(count(AgendaGroup::Planets), 6);
        assert_eq!(count(AgendaGroup::Units), 8);
        assert_eq!(count(AgendaGroup::Map), 4);
        assert_eq!(count(AgendaGroup::Technology), 8);
        assert_eq!(count(AgendaGroup::Supplies), 7);
        assert_eq!(count(AgendaGroup::Votes), 8);
    }
}
