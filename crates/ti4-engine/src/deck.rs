//! Setup deck construction.
//!
//! Ported from the catalogue and `build_deck` functions in the oracle's
//! `objectives.py`, `action_cards.py`, `agenda.py`, `exploration.py`, `relics.py`, and
//! `secrets.py`.  Decks keep ids rather than records: the immutable content store remains
//! the card authority, and game state only needs to know draw order.

use std::collections::BTreeMap;

use ti4_content::ContentStore;
use ti4_model::content_types::{ContentType, SourceSet};
use ti4_model::id::{ActionCardId, ObjectiveId, RelicId, SecretObjectiveId};

use crate::rng::{GameRng, domain};

/// The four exploration decks in the oracle's construction order.
pub const EXPLORATION_TRAITS: [&str; 4] = ["CULTURAL", "HAZARDOUS", "INDUSTRIAL", "FRONTIER"];

/// Ordered card ids for every deck created during game setup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartingDecks {
    pub objectives: Vec<ObjectiveId>,
    pub exploration: BTreeMap<String, Vec<String>>,
    pub relics: Vec<RelicId>,
    pub agendas: Vec<String>,
    pub action_cards: Vec<ActionCardId>,
    pub secrets: Vec<SecretObjectiveId>,
}

/// Build every setup deck from `sources` with the native, domain-separated RNG.
///
/// Public objectives are special: LRR 61.13 requires five shuffled stage-I cards followed
/// by five shuffled stage-II cards.  Every other deck is the complete sorted catalogue in
/// scope, shuffled once.  Relics omit the corpus's fake placeholder record.
#[must_use]
pub fn build_starting_decks(
    content: &ContentStore,
    sources: SourceSet,
    seed: u64,
) -> StartingDecks {
    let mut rng = GameRng::new(seed);

    let mut stage_one = ids(content, ContentType::PublicObjectives, sources)
        .into_iter()
        .filter(|id| {
            content
                .get(ContentType::PublicObjectives, id)
                .and_then(|r| r.int("points"))
                == Some(1)
        })
        .map(ObjectiveId::new)
        .collect::<Vec<_>>();
    let mut stage_two = ids(content, ContentType::PublicObjectives, sources)
        .into_iter()
        .filter(|id| {
            content
                .get(ContentType::PublicObjectives, id)
                .and_then(|r| r.int("points"))
                == Some(2)
        })
        .map(ObjectiveId::new)
        .collect::<Vec<_>>();
    rng.shuffle(domain::OBJECTIVES, &mut stage_one);
    rng.shuffle(domain::OBJECTIVES, &mut stage_two);
    let objectives = stage_one
        .into_iter()
        .take(5)
        .chain(stage_two.into_iter().take(5))
        .collect();

    let mut exploration = BTreeMap::new();
    for trait_name in EXPLORATION_TRAITS {
        let mut cards = ids(content, ContentType::Explores, sources)
            .into_iter()
            .filter(|id| {
                content
                    .get(ContentType::Explores, id)
                    .and_then(|record| record.text("type"))
                    .is_some_and(|kind| kind.eq_ignore_ascii_case(trait_name))
            })
            .collect::<Vec<_>>();
        rng.shuffle(domain::EXPLORATION, &mut cards);
        exploration.insert(trait_name.to_owned(), cards);
    }

    let mut relics = ids(content, ContentType::Relics, sources)
        .into_iter()
        .filter(|id| {
            !content
                .get(ContentType::Relics, id)
                .is_some_and(|record| record.flag("isFakeRelic"))
        })
        .map(RelicId::new)
        .collect::<Vec<_>>();
    rng.shuffle(domain::RELICS, &mut relics);

    let mut agendas = agenda_ids(content, sources);
    rng.shuffle(domain::AGENDAS, &mut agendas);

    let mut action_cards = ids(content, ContentType::ActionCards, sources)
        .into_iter()
        .map(ActionCardId::new)
        .collect::<Vec<_>>();
    rng.shuffle(domain::ACTION_CARDS, &mut action_cards);

    let mut secrets = ids(content, ContentType::SecretObjectives, sources)
        .into_iter()
        .map(SecretObjectiveId::new)
        .collect::<Vec<_>>();
    rng.shuffle(domain::SECRETS, &mut secrets);

    StartingDecks {
        objectives,
        exploration,
        relics,
        agendas,
        action_cards,
        secrets,
    }
}

/// Sorted catalogue ids, matching the oracle's `sorted(catalogue(sources))` before shuffle.
/// Base-game agendas Prophecy of Kings takes out of the deck.
///
/// `PoK` adds thirteen agendas and removes thirteen, so the deck stays at fifty. Most of the
/// removed ones come back as exploration cards or relics -- The Crown of Emphidia is a relic in
/// `PoK`, and the corpus carries both it and the base agenda, so without this a `PoK` game could
/// deal the agenda *and* hand out the relic version of the same card.
///
/// `representative_government` is the base printing, named "Representative Government (Base
/// Game)" in the corpus; `PoK`'s replacement of the same name is `rep_govt` and stays. Removing
/// by name rather than alias would have taken both.
const SUPERSEDED_BY_POK: [&str; 13] = [
    "core_mining",
    "crown_of_emphidia",
    "crown_of_thalnos",
    "demilitarized_zone",
    "holy_planet_of_ixth",
    "representative_government",
    "rt_biotic",
    "rt_cybernetic",
    "rt_propulsion",
    "rt_warfare",
    "senate_sanctuary",
    "shard_of_the_throne",
    "terraforming_initiative",
];

/// The agenda deck for a source scope, with the `PoK` removals applied.
fn agenda_ids(content: &ContentStore, sources: SourceSet) -> Vec<String> {
    let mut ids = ids(content, ContentType::Agendas, sources);
    if sources.contains(ti4_model::content_types::Source::Pok) {
        ids.retain(|id| !SUPERSEDED_BY_POK.contains(&id.as_str()));
    }
    ids
}

fn ids(content: &ContentStore, category: ContentType, sources: SourceSet) -> Vec<String> {
    let mut ids = content
        .from_sources(category, sources)
        .filter_map(|record| record.id().map(ToOwned::to_owned))
        .collect::<Vec<_>>();
    ids.sort_unstable();
    ids
}

#[cfg(test)]
mod tests {
    use super::*;
    use ti4_model::content_types::{BASE, FULL, POK};

    fn content() -> &'static ContentStore {
        ContentStore::embedded()
    }

    #[test]
    fn objectives_keep_their_stage_boundary() {
        let decks = build_starting_decks(content(), FULL, 7);
        assert_eq!(decks.objectives.len(), 10);
        assert!(decks.objectives[..5].iter().all(|id| {
            content()
                .get(ContentType::PublicObjectives, id.as_str())
                .and_then(|record| record.int("points"))
                == Some(1)
        }));
        assert!(decks.objectives[5..].iter().all(|id| {
            content()
                .get(ContentType::PublicObjectives, id.as_str())
                .and_then(|record| record.int("points"))
                == Some(2)
        }));
    }


    #[test]
    fn pok_deals_fifty_agendas_without_the_cards_it_supersedes() {
        // PoK adds thirteen agendas and removes thirteen; the deck stays at fifty. Dealing all
        // sixty-three let a PoK game draw The Crown of Emphidia as an agenda while the same card
        // was also available as a relic.
        let decks = build_starting_decks(content(), POK, 11);
        assert_eq!(decks.agendas.len(), 50, "a PoK agenda deck is fifty cards");
        for superseded in SUPERSEDED_BY_POK {
            assert!(
                !decks.agendas.iter().any(|id| id == superseded),
                "{superseded} is superseded in PoK and must not be dealt"
            );
        }
        // The replacement of the same name stays; removing by name would have taken both.
        assert!(
            decks.agendas.iter().any(|id| id == "rep_govt"),
            "PoK's own Representative Government stays in the deck"
        );
    }

    #[test]
    fn a_base_only_game_keeps_every_base_agenda() {
        // The removals are PoK's doing. Without it the base deck is untouched, so a base game
        // still deals Core Mining and the rest.
        let decks = build_starting_decks(content(), BASE, 11);
        assert!(
            decks.agendas.iter().any(|id| id == "crown_of_emphidia"),
            "a base game still has The Crown of Emphidia"
        );
    }

    #[test]
    fn every_built_deck_is_a_source_scoped_permutation() {
        let decks = build_starting_decks(content(), POK, 7);
        assert_eq!(
            decks.action_cards.len(),
            ids(content(), ContentType::ActionCards, POK).len()
        );
        assert_eq!(decks.agendas.len(), agenda_ids(content(), POK).len());
        assert_eq!(
            decks.secrets.len(),
            ids(content(), ContentType::SecretObjectives, POK).len()
        );
        assert_eq!(
            decks.relics.len(),
            ids(content(), ContentType::Relics, POK).len() - 1
        );
        for trait_name in EXPLORATION_TRAITS {
            assert_eq!(decks.exploration[trait_name].len(), 20, "{trait_name}");
        }
    }

    #[test]
    fn the_fake_relic_is_never_in_the_deck() {
        let decks = build_starting_decks(content(), FULL, 7);
        assert!(decks.relics.iter().all(|id| {
            !content()
                .get(ContentType::Relics, id.as_str())
                .is_some_and(|record| record.flag("isFakeRelic"))
        }));
    }

    #[test]
    fn deck_orders_are_seeded_and_repeatable() {
        assert_eq!(
            build_starting_decks(content(), POK, 7),
            build_starting_decks(content(), POK, 7)
        );
        assert_ne!(
            build_starting_decks(content(), POK, 7).agendas,
            build_starting_decks(content(), POK, 8).agendas
        );
    }
}
