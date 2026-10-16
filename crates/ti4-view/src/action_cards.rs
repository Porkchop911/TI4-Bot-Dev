//! What a client is shown of the action cards beside its own hand: how many cards the deck still
//! holds, the discard pile, and the hand limit of the viewer. The printed cards and the deck that
//! a game begins with are in the catalogue ([`crate::components`]).
//!
//! Nothing here decides a rule. The hand limit is the one that the engine enforces.

use serde::{Deserialize, Serialize};
use ti4_content::ContentStore;
use ti4_model::id::ActionCardId;
use ti4_model::state::GameState;

use crate::status::ViewerRole;

/// The action cards as every player at the table sees them, and the hand limit of the viewer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionCardFacts {
    /// How many cards the deck still holds. Which cards is hidden.
    pub deck: usize,
    /// The discard pile, sorted by id: every card that was played or discarded.
    pub discard: Vec<ActionCardId>,
    /// The cards that lie face up on the card of Garbozia, sorted by id.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub salvage: Vec<ActionCardId>,
    /// How many action cards the viewer may hold. Absent for a spectator, and for a seat that
    /// has no limit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hand_limit: Option<usize>,
}

/// Projects the action cards of `state` for `viewer`.
#[must_use]
pub fn project_action_card_facts(
    state: &GameState,
    content: &ContentStore,
    viewer: &ViewerRole,
) -> ActionCardFacts {
    let sorted = |cards: &[ActionCardId]| {
        let mut cards = cards.to_vec();
        cards.sort();
        cards
    };
    let hand_limit = match viewer {
        ViewerRole::Player(seat) => {
            Some(ti4_engine::action_cards::hand_limit(state, content, seat))
                .filter(|limit| *limit != usize::MAX)
        }
        ViewerRole::Spectator => None,
    };
    ActionCardFacts {
        deck: state.action_card_deck.len(),
        discard: sorted(&state.discarded_action_cards),
        salvage: sorted(&state.salvage_yard),
        hand_limit,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ti4_model::content_types::POK;
    use ti4_model::id::PlayerId;

    fn game() -> GameState {
        let content = ContentStore::embedded();
        let ids: Vec<PlayerId> = ["a", "b", "c", "d", "e", "f"]
            .iter()
            .map(|name| PlayerId::new(*name))
            .collect();
        let loader = crate::maps::TemplateLoader::load().expect("the templates load");
        let template =
            crate::maps::default_template_for(content, &loader, 6, POK).expect("a template");
        crate::map::create_game_with_template(content, &ids, 42, Some(&template))
            .expect("a game")
            .0
    }

    fn seat(name: &str) -> ViewerRole {
        ViewerRole::Player(PlayerId::new(name))
    }

    #[test]
    fn every_card_of_the_catalogue_is_in_the_deck_or_in_a_hand() {
        let content = ContentStore::embedded();
        let state = game();
        let facts = project_action_card_facts(&state, content, &seat("a"));
        let held: usize = state
            .players
            .iter()
            .map(|player| player.action_cards.len())
            .sum();
        let printed = crate::components::component_catalog(content, POK).action_cards;

        assert_eq!(facts.deck + held, printed.len());
        assert!(facts.discard.is_empty());
        assert!(facts.salvage.is_empty());
        assert_eq!(facts.hand_limit, Some(7));
    }

    #[test]
    fn a_discarded_card_is_in_the_pile() {
        let content = ContentStore::embedded();
        let mut state = game();
        let top = state.action_card_deck.remove(0);
        state.discarded_action_cards.push(top.clone());
        let facts = project_action_card_facts(&state, content, &seat("a"));

        assert_eq!(facts.discard, [top]);
        assert_eq!(facts.deck, state.action_card_deck.len());
    }

    #[test]
    fn the_limit_is_the_one_of_the_viewer_now() {
        let content = ContentStore::embedded();
        let mut state = game();
        state.enact_law("sanctions", "for");

        assert_eq!(
            project_action_card_facts(&state, content, &seat("a")).hand_limit,
            Some(3)
        );
        assert_eq!(
            project_action_card_facts(&state, content, &ViewerRole::Spectator).hand_limit,
            None
        );
    }
}
