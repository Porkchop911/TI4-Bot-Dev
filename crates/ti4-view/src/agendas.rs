//! What a client is shown of the agenda deck beside the laws in play: how many cards it still
//! holds. The laws are in the table of the view ([`crate::view::TableView::laws`]), and the
//! printed agendas and the deck that a game begins with are in the catalogue
//! ([`crate::components`]).
//!
//! Nothing here decides a rule. The count of a deck is public, so the facts are the same for every
//! viewer.

use serde::{Deserialize, Serialize};
use ti4_model::state::GameState;

/// The agenda deck as every player at the table sees it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgendaFacts {
    /// How many cards the deck still holds. Which cards is hidden.
    pub deck: usize,
}

/// Projects the agenda deck of `state`.
#[must_use]
pub fn project_agenda_facts(state: &GameState) -> AgendaFacts {
    AgendaFacts {
        deck: state.agenda_deck.len(),
    }
}
