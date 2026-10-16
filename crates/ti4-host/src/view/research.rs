//! A research as a client shows it: the technology tree of the seat that is asked.
//!
//! The engine offers what the seat may research now as the options of the question (each of kind
//! [`ti4_engine::strategy_cards::RESEARCH_KIND`], with its id the technology). The tree around
//! them is printed: the colour and the prerequisites of each technology, what the seat owns, and
//! the ready planets whose technology specialty stands in for a prerequisite (90.8). The printed
//! name and text are in the catalogue ([`crate::view::components`]).
//!
//! Nothing here decides a rule: which technology can be researched is the engine's answer.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use ti4_content::ContentStore;
use ti4_engine::choice::Choice;
use ti4_engine::strategy_cards::RESEARCH_KIND;
use ti4_engine::technology;
use ti4_model::content_types::{ContentType, SourceSet};
use ti4_model::id::TechnologyId;
use ti4_model::state::GameState;

/// A colour of technology, by the letter that the corpus writes it with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum TechColour {
    /// Propulsion.
    B,
    /// Biotic.
    G,
    /// Cybernetic.
    Y,
    /// Warfare.
    R,
}

impl TechColour {
    fn of_track(track: &str) -> Option<Self> {
        match track {
            "PROPULSION" => Some(Self::B),
            "BIOTIC" => Some(Self::G),
            "CYBERNETIC" => Some(Self::Y),
            "WARFARE" => Some(Self::R),
            _ => None,
        }
    }
}

/// One technology of the tree, as printed, with whether the seat owns it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ResearchTechnology {
    /// The id of the technology: the id of its option when it can be researched now.
    pub id: String,
    /// The colour of the card. A unit upgrade has none (90.7b).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub colour: Option<TechColour>,
    /// The prerequisites, one for each printed symbol, in the printed order.
    pub prerequisites: Vec<TechColour>,
    pub unit_upgrade: bool,
    /// A technology of a faction (90.11).
    pub faction: bool,
    pub owned: bool,
}

/// A ready planet of the seat with a technology specialty.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ResearchSpecialty {
    pub planet: String,
    pub colour: TechColour,
}

/// The technology tree of the seat that is asked to research.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ResearchFacts {
    /// Every technology of the game that the seat could own: the generic ones that its faction
    /// has not replaced, its faction technologies, and what it owns besides.
    pub technologies: Vec<ResearchTechnology>,
    pub specialties: Vec<ResearchSpecialty>,
}

/// The tree for `choice`, when it asks its seat to research a technology: one of its options is
/// a research.
#[must_use]
pub fn project_research_facts(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    choice: &Choice,
) -> Option<ResearchFacts> {
    if !choice
        .options
        .iter()
        .any(|option| option.kind == RESEARCH_KIND)
    {
        return None;
    }
    let seat = state.player(&choice.player)?;
    let faction = seat.faction.as_str();
    let active = technology::active_aliases(content, sources);
    let mut ids: BTreeSet<TechnologyId> = active
        .into_iter()
        .filter(|id| {
            technology::faction_of(content, id).is_none_or(|owner| owner == faction)
                && !technology::replaced_for_faction(content, faction, id)
        })
        .collect();
    ids.extend(seat.technologies.iter().cloned());
    let technologies = ids
        .into_iter()
        .filter(|id| {
            content
                .get(ContentType::Technologies, id.as_str())
                .is_some()
        })
        .map(|id| {
            let printed = content
                .get(ContentType::Technologies, id.as_str())
                .and_then(|record| record.text("requirements"))
                .unwrap_or("");
            ResearchTechnology {
                colour: technology::colour_type(content, &id).and_then(TechColour::of_track),
                // The corpus writes a prerequisite as a letter of its colour (`RRRY`).
                prerequisites: printed
                    .chars()
                    .filter_map(technology::colour_of)
                    .filter_map(TechColour::of_track)
                    .collect(),
                unit_upgrade: technology::is_unit_upgrade(content, &id),
                faction: technology::faction_of(content, &id).is_some(),
                owned: seat.technologies.contains(&id),
                id: id.to_string(),
            }
        })
        .collect();
    let mut specialties: Vec<ResearchSpecialty> = state
        .controlled_planets(&choice.player)
        .into_iter()
        .filter(|(_, planet)| !state.exhausted_planets.contains(*planet))
        .flat_map(|(_, planet)| {
            ti4_engine::planets::tech_specialties_now(state, content, sources, planet)
                .into_iter()
                .filter_map(|specialty| TechColour::of_track(&specialty.to_ascii_uppercase()))
                .map(|colour| ResearchSpecialty {
                    planet: planet.to_string(),
                    colour,
                })
        })
        .collect();
    specialties.sort_by(|a, b| (a.colour, &a.planet).cmp(&(b.colour, &b.planet)));
    Some(ResearchFacts {
        technologies,
        specialties,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ti4_engine::choice::ChoiceOption;
    use ti4_model::content_types::POK;
    use ti4_model::id::PlayerId;

    fn research(ids: &[&str]) -> Choice {
        Choice::new(
            PlayerId::new("a"),
            "research a technology",
            ids.iter()
                .map(|id| ChoiceOption::labelled(*id, RESEARCH_KIND, *id))
                .collect(),
        )
    }

    #[test]
    fn the_tree_of_sol_has_its_faction_upgrades_in_place_of_the_generic_ones() {
        let state = ti4_engine::fixtures::seated_game(&[("a", "sol"), ("b", "xxcha")], POK);
        let content = ContentStore::embedded();
        let facts =
            project_research_facts(&state, content, POK, &research(&["sdn"])).expect("a research");
        let tech = |id: &str| facts.technologies.iter().find(|tech| tech.id == id);
        // Sol begins with Antimass Deflectors and Neural Motivator.
        let amd = tech("amd").expect("Antimass Deflectors");
        assert!(amd.owned && amd.colour == Some(TechColour::B) && amd.prerequisites.is_empty());
        let ws = tech("ws").expect("War Sun");
        assert!(ws.unit_upgrade && ws.colour.is_none() && !ws.owned);
        assert_eq!(
            ws.prerequisites,
            [TechColour::R, TechColour::R, TechColour::R, TechColour::Y]
        );
        assert!(tech("ac2").is_some_and(|tech| tech.faction && tech.unit_upgrade));
        assert!(
            tech("cv2").is_none(),
            "Sol's Advanced Carrier II replaces it"
        );
        for tech in facts.technologies.iter().filter(|tech| tech.faction) {
            let id = TechnologyId::new(tech.id.as_str());
            assert_eq!(
                technology::faction_of(content, &id),
                Some("sol"),
                "{}",
                tech.id
            );
        }
    }

    #[test]
    fn a_question_that_researches_nothing_has_no_tree() {
        let state = ti4_engine::fixtures::seated_game(&[("a", "sol"), ("b", "xxcha")], POK);
        let choice = Choice::new(
            PlayerId::new("a"),
            "follow",
            vec![ChoiceOption::labelled("yes", "follow", "Follow")],
        );
        assert!(project_research_facts(&state, ContentStore::embedded(), POK, &choice).is_none());
    }
}
