//! What a client is shown of the exploration: the cards that each deck still holds, the cards
//! that were drawn, and the planets that are explored when a player takes them. The printed
//! names and texts are a catalogue of their own, which a client asks for once.
//!
//! Nothing here decides a rule. A planet is explored when a player takes control of it and no
//! player had it (35.1), into the decks that [`ti4_engine::planets::traits_now`] names: this is
//! what the invasion asks when control changes.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use ti4_content::ContentStore;
use ti4_content::galaxy::{self, Galaxy};
use ti4_engine::deck::{EXPLORATION_TRAITS, build_starting_decks};
use ti4_model::content_types::{ContentType, SourceSet};
use ti4_model::id::{PlanetId, SystemId};
use ti4_model::state::{ExplorationRecord, GameState};

/// The exploration as every player at the table sees it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExplorationFacts {
    /// The cards that each deck still holds, sorted by id. The order of a deck is hidden; what
    /// left it was shown to the table.
    pub decks: BTreeMap<String, Vec<String>>,
    /// Every card that was drawn, in order.
    pub drawn: Vec<ExplorationRecord>,
    /// The planets that no player controls and that have a deck: a player who takes control of
    /// one explores it. More than one deck is the choice of that player.
    pub explores: BTreeMap<PlanetId, Vec<String>>,
}

/// One exploration card as printed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExplorationCard {
    pub id: String,
    pub name: String,
    /// The deck of the card, as [`EXPLORATION_TRAITS`] names it.
    pub deck: String,
    /// `Instant`, `Fragment`, `Attach` or `Token`.
    pub resolution: String,
    pub text: String,
}

/// One attachment of a planet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttachmentCard {
    pub id: String,
    pub name: String,
    pub resources: i64,
    pub influence: i64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tech_specialties: Vec<String>,
    /// The text of the exploration card that attaches it. Empty for an attachment of another
    /// origin.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub text: String,
}

/// The printed side of the exploration. It does not change during a game.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExplorationCatalog {
    pub cards: Vec<ExplorationCard>,
    /// The cards that each deck begins a game with, sorted by id.
    pub decks: BTreeMap<String, Vec<String>>,
    pub attachments: Vec<AttachmentCard>,
}

/// Projects the exploration of `state` on the map `galaxy`.
#[must_use]
pub fn project_exploration_facts(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: &Galaxy,
) -> ExplorationFacts {
    let decks = EXPLORATION_TRAITS
        .iter()
        .map(|deck| {
            let mut cards = state
                .exploration_decks
                .get(*deck)
                .cloned()
                .unwrap_or_default();
            cards.sort();
            ((*deck).to_owned(), cards)
        })
        .collect();
    let mut explores = BTreeMap::new();
    for system_id in galaxy.system_ids() {
        let Some(system) = galaxy::system(content, system_id, sources) else {
            continue;
        };
        let held = state.board.get(&SystemId::new(system_id));
        for planet in system.planets() {
            let planet = PlanetId::new(planet);
            if held.is_some_and(|held| {
                held.planet_control.contains_key(&planet) || held.purged_planets.contains(&planet)
            }) {
                continue;
            }
            let into = ti4_engine::planets::traits_now(state, content, sources, &planet);
            if !into.is_empty() {
                explores.insert(planet, into);
            }
        }
    }
    ExplorationFacts {
        decks,
        drawn: state.exploration_log.clone(),
        explores,
    }
}

/// The exploration cards and the attachments of `sources`, as printed.
#[must_use]
pub fn exploration_catalog(content: &ContentStore, sources: SourceSet) -> ExplorationCatalog {
    let explores = || {
        content
            .records(ContentType::Explores)
            .iter()
            .filter(move |record| record.in_sources(sources))
    };
    let text = |record: &ti4_content::Record, key: &str| {
        record.text(key).map(ToOwned::to_owned).unwrap_or_default()
    };
    let cards = explores()
        .filter_map(|record| {
            let id = record.id()?;
            Some(ExplorationCard {
                id: id.to_owned(),
                name: record.text("name").unwrap_or(id).to_owned(),
                deck: text(record, "type").to_ascii_uppercase(),
                resolution: text(record, "resolution"),
                text: text(record, "text"),
            })
        })
        .collect();
    let mut decks = build_starting_decks(content, sources, 0).exploration;
    for deck in decks.values_mut() {
        deck.sort();
    }
    // A research facility is two attachments with one card: the `...stat` one has no card of
    // its own name.
    let printed = |attachment: &str| {
        explores()
            .find(|record| {
                record.text("resolution") == Some("Attach")
                    && record.text("attachmentId").or(record.id()) == Some(attachment)
            })
            .map(|record| text(record, "text"))
    };
    let attachments = content
        .records(ContentType::Attachments)
        .iter()
        .filter(|record| record.in_sources(sources) && !record.flag("isFakeAttachment"))
        .filter_map(|record| {
            let id = record.id()?;
            Some(AttachmentCard {
                id: id.to_owned(),
                name: record.text("name").unwrap_or(id).to_owned(),
                resources: record.int("resourcesModifier").unwrap_or(0),
                influence: record.int("influenceModifier").unwrap_or(0),
                tech_specialties: record
                    .strings("techSpeciality")
                    .into_iter()
                    .map(str::to_ascii_uppercase)
                    .collect(),
                text: printed(id)
                    .or_else(|| id.strip_suffix("stat").and_then(printed))
                    .unwrap_or_default(),
            })
        })
        .collect();
    ExplorationCatalog {
        cards,
        decks,
        attachments,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ti4_model::content_types::POK;
    use ti4_model::id::PlayerId;

    fn game() -> (GameState, Galaxy) {
        let content = ContentStore::embedded();
        let ids: Vec<PlayerId> = ["a", "b", "c", "d", "e", "f", "g", "h"]
            .iter()
            .map(|name| PlayerId::new(*name))
            .collect();
        let loader = crate::maps::TemplateLoader::load().expect("the templates load");
        let template =
            crate::maps::default_template_for(content, &loader, 8, POK).expect("a template");
        crate::map::create_game_with_template(content, &ids, 42, Some(&template)).expect("a game")
    }

    #[test]
    fn a_new_game_has_full_decks_and_every_free_planet_is_explored() {
        let content = ContentStore::embedded();
        let (state, galaxy) = game();
        let facts = project_exploration_facts(&state, content, POK, &galaxy);
        let catalog = exploration_catalog(content, POK);

        assert_eq!(facts.decks, catalog.decks);
        assert_eq!(facts.decks.len(), 4);
        assert!(facts.drawn.is_empty());
        for (deck, cards) in &catalog.decks {
            assert!(!cards.is_empty(), "{deck} has cards");
            for card in cards {
                let printed = catalog
                    .cards
                    .iter()
                    .find(|printed| printed.id == *card)
                    .unwrap_or_else(|| panic!("{card} is in the catalogue"));
                assert_eq!(printed.deck, *deck);
                assert!(!printed.text.is_empty(), "{card} has its text");
            }
        }
        // A home planet has no deck, and it is controlled.
        assert!(!facts.explores.contains_key(&PlanetId::new("jord")));
        assert_eq!(
            facts.explores.get(&PlanetId::new("abyz")),
            Some(&vec!["HAZARDOUS".to_owned()])
        );
        assert_eq!(
            facts.explores.get(&PlanetId::new("mecatol")),
            None,
            "Mecatol Rex has no deck"
        );
    }

    #[test]
    fn a_planet_that_is_taken_is_explored_once() {
        let content = ContentStore::embedded();
        let (mut state, galaxy) = game();
        let seat = PlayerId::new("a");
        let planet = PlanetId::new("abyz");
        let top = state.exploration_decks["HAZARDOUS"][0].clone();
        let before = project_exploration_facts(&state, content, POK, &galaxy);

        ti4_engine::exploration::explore(&mut state, content, &seat, "HAZARDOUS", Some(&planet))
            .expect("a card is drawn");
        state
            .system_mut(&SystemId::new("38"))
            .set_control(planet.clone(), seat.clone());
        let after = project_exploration_facts(&state, content, POK, &galaxy);

        assert!(!after.explores.contains_key(&planet));
        assert!(after.explores.contains_key(&PlanetId::new("fria")));
        assert_eq!(after.drawn.len(), 1);
        assert_eq!(after.drawn[0].card, top);
        assert_eq!(after.drawn[0].planet, Some(planet));
        assert_eq!(
            after.decks["HAZARDOUS"].len() + 1,
            before.decks["HAZARDOUS"].len()
        );
    }

    #[test]
    fn an_attachment_has_the_text_of_its_card() {
        let catalog = exploration_catalog(ContentStore::embedded(), POK);
        let attachment = |id: &str| {
            catalog
                .attachments
                .iter()
                .find(|attachment| attachment.id == id)
                .unwrap_or_else(|| panic!("no attachment {id}"))
        };
        let sphere = attachment("dysonsphere");
        assert_eq!(sphere.name, "Dyson Sphere");
        assert_eq!((sphere.resources, sphere.influence), (2, 1));
        assert!(sphere.text.contains("increased by 2"));
        assert_eq!(attachment("warfare").tech_specialties, ["WARFARE"]);
        // Both sides of a research facility have the text of the one card.
        assert_eq!(attachment("warfarestat").text, attachment("warfare").text);
        assert!(attachment("nanoforge").text.is_empty());
    }
}
