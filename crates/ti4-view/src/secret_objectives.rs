//! What a client is shown of the secret objectives beside its own hand: how many cards the deck
//! still holds, and which objectives each seat has scored. The printed objectives and the deck
//! that a game begins with are in the catalogue ([`crate::components`]).
//!
//! Nothing here decides a rule. A scored secret objective is public (LRR 61.17 hides the unscored
//! ones only), so the facts are the same for every viewer.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use ti4_content::ContentStore;
use ti4_model::content_types::ContentType;
use ti4_model::id::{PlayerId, SecretObjectiveId};
use ti4_model::state::GameState;

/// The secret objectives as every player at the table sees them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SecretObjectiveFacts {
    /// How many cards the deck still holds. Which cards is hidden.
    pub deck: usize,
    /// The secret objectives that each seat has scored, sorted by id. A seat that has scored none
    /// is left out.
    pub scored: BTreeMap<PlayerId, Vec<SecretObjectiveId>>,
    /// How many secret objectives a seat may have, scored and unscored together.
    pub hand_limit: usize,
}

/// Projects the secret objectives of `state`.
#[must_use]
pub fn project_secret_objective_facts(
    state: &GameState,
    content: &ContentStore,
) -> SecretObjectiveFacts {
    let scored = state
        .players
        .iter()
        .filter_map(|player| {
            // The same test as `ti4_engine::secrets::scored_count`, and the secrets that were
            // scored as plots.
            let scored: BTreeSet<SecretObjectiveId> = state
                .scored_by(&player.id)
                .iter()
                .filter(|alias| {
                    content
                        .get(ContentType::SecretObjectives, alias.as_str())
                        .is_some()
                })
                .map(|alias| SecretObjectiveId::new(alias.as_str()))
                .chain(player.plot_objectives.iter().cloned())
                .collect();
            (!scored.is_empty()).then(|| (player.id.clone(), scored.into_iter().collect()))
        })
        .collect();
    SecretObjectiveFacts {
        deck: state.secret_deck.len(),
        scored,
        hand_limit: ti4_engine::secrets::HAND_LIMIT,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ti4_model::content_types::POK;
    use ti4_model::id::ObjectiveId;

    fn game() -> (GameState, ti4_content::galaxy::Galaxy) {
        let content = ContentStore::embedded();
        let ids: Vec<PlayerId> = ["a", "b", "c", "d", "e", "f"]
            .iter()
            .map(|name| PlayerId::new(*name))
            .collect();
        let loader = crate::maps::TemplateLoader::load().expect("the templates load");
        let template =
            crate::maps::default_template_for(content, &loader, 6, POK).expect("a template");
        crate::map::create_game_with_template(content, &ids, 42, Some(&template)).expect("a game")
    }

    #[test]
    fn every_objective_of_the_catalogue_is_in_the_deck_or_in_a_hand() {
        let content = ContentStore::embedded();
        let (state, galaxy) = game();
        let facts = project_secret_objective_facts(&state, content);
        // Setup deals out what this map can never satisfy (the board's census).
        let homes: Vec<ti4_model::id::SystemId> = state
            .players
            .iter()
            .filter_map(|player| player.home_system.clone())
            .collect();
        let census = ti4_engine::map_feasibility::census(content, POK, &galaxy, &homes);
        let dealt_out = ti4_engine::map_feasibility::unsatisfiable_secret(&census).len();
        let held: usize = state
            .players
            .iter()
            .map(|player| player.secret_objectives.len())
            .sum();
        let printed = crate::components::component_catalog(content, POK).secret_objectives;

        assert_eq!(facts.deck + held + dealt_out, printed.len());
        assert!(facts.scored.is_empty());
        assert_eq!(facts.hand_limit, 3);
    }

    #[test]
    fn a_scored_secret_is_listed_for_its_seat_and_a_public_objective_is_not() {
        let content = ContentStore::embedded();
        let mut state = game().0;
        let seat = PlayerId::new("b");
        let secret = state
            .players
            .iter_mut()
            .find(|player| player.id == seat)
            .expect("the seat")
            .secret_objectives
            .remove(0);
        state.record_score(&seat, ObjectiveId::new(secret.as_str()));
        let public = state
            .objective_deck
            .first()
            .cloned()
            .expect("a public objective");
        state.record_score(&seat, public);
        let facts = project_secret_objective_facts(&state, content);

        assert_eq!(facts.scored, BTreeMap::from([(seat, vec![secret])]));
    }
}
