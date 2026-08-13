//! Redacted views of the game state (M08-001).
//!
//! Ported from the oracle's `engine/views.py`.
//!
//! Some things in TI4 are private: the action cards in your hand, the secret objectives you
//! hold. Everything else — the board, who controls what, how many victory points everyone has —
//! is open information any player may read.
//!
//! A view replaces the private parts of *other* players rather than removing them. A hand of four
//! unknown cards becomes four [`HIDDEN`] markers, so its **size** stays visible, which is correct:
//! at a real table you can see how many cards somebody holds without seeing which.
//!
//! Replacing rather than deleting matters for a second reason. Every existing query keeps working
//! against a view, so nothing has to learn about redaction to be safe. Code that only counts is
//! unaffected; only code that inspects a specific card sees the marker, and a marker is not a card
//! id, so it matches nothing.

use ti4_model::id::{ActionCardId, PlayerId, SecretObjectiveId};
use ti4_model::state::{GameState, Player};

/// Stands in for a card whose identity is private.
///
/// Not a valid alias anywhere, so a lookup against real content fails rather than quietly matching
/// something.
pub const HIDDEN: &str = "?";

/// Per-player holdings nobody else may read. Public counts survive; identities do not.
///
/// 61.17 makes an unscored secret objective hidden until it is scored.
pub const PRIVATE_SEQUENCES: [&str; 2] = ["action_cards", "secret_objectives"];

/// Private holdings this engine keeps in shared state, and so cannot redact per player.
///
/// Named rather than left implicit. `GameState::promissory_notes` maps a note to its holder for
/// the whole table, so a view shows which notes everybody holds — which 69.6 makes hidden until
/// they are played. The oracle has the same exposure and its bots were tuned against it, so
/// closing it here would change how bots choose and break parity before it improved anything.
/// It is recorded as a gap with a test naming it, so redacting it later is a deliberate act
/// rather than an accident.
pub const UNREDACTED: [&str; 1] = ["promissory_notes"];

/// Whether a card id is the private marker rather than a real card.
#[must_use]
pub fn is_hidden(card: &str) -> bool {
    card == HIDDEN
}

/// One player as their opponents see them.
#[must_use]
pub fn redact_player(player: &Player) -> Player {
    let mut seen = player.clone();
    seen.action_cards = player
        .action_cards
        .iter()
        .map(|_| ActionCardId::new(HIDDEN))
        .collect();
    seen.secret_objectives = player
        .secret_objectives
        .iter()
        .map(|_| SecretObjectiveId::new(HIDDEN))
        .collect();
    seen
}

/// The game as one player is entitled to see it.
///
/// Their own state is untouched — you know your own hand — and the board is shared, so only other
/// players' private holdings change.
#[must_use]
pub fn view_for(state: &GameState, viewer: &PlayerId) -> GameState {
    let mut view = state.clone();
    for player in &mut view.players {
        if &player.id != viewer {
            *player = redact_player(player);
        }
    }
    view
}

/// Private identities this view still exposes. Empty means the view is clean.
///
/// Written as a check rather than a comment so a newly added private field that nobody redacted
/// shows up as a failing test instead of a quiet leak.
#[must_use]
pub fn leaks(state: &GameState, viewer: &PlayerId) -> Vec<String> {
    let mut found = Vec::new();
    for player in &state.players {
        if &player.id == viewer {
            continue;
        }
        for card in &player.action_cards {
            if !is_hidden(card.as_str()) {
                found.push(format!("{}.action_cards={card}", player.id));
            }
        }
        for secret in &player.secret_objectives {
            if !is_hidden(secret.as_str()) {
                found.push(format!("{}.secret_objectives={secret}", player.id));
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table() -> GameState {
        let mut state = ti4_engine::fixtures::game(&["a", "b"]);
        let seat = state.player_mut(&PlayerId::new("b")).unwrap();
        seat.action_cards = vec![
            ActionCardId::new("sabotage"),
            ActionCardId::new("direct_hit"),
        ];
        seat.secret_objectives = vec![SecretObjectiveId::new("become_a_legend")];
        state
    }

    #[test]
    fn a_rivals_hand_keeps_its_size_and_loses_its_names() {
        // The distinction the whole module rests on. At a table you can count somebody's cards.
        let view = view_for(&table(), &PlayerId::new("a"));
        let rival = view.player(&PlayerId::new("b")).unwrap();

        assert_eq!(rival.action_cards.len(), 2, "the count is public");
        assert!(
            rival
                .action_cards
                .iter()
                .all(|card| is_hidden(card.as_str())),
            "and the names are not: {:?}",
            rival.action_cards
        );
        assert_eq!(rival.secret_objectives.len(), 1);
        assert!(is_hidden(rival.secret_objectives[0].as_str()));
    }

    #[test]
    fn you_can_read_your_own_hand() {
        let view = view_for(&table(), &PlayerId::new("b"));
        let own = view.player(&PlayerId::new("b")).unwrap();

        assert_eq!(own.action_cards[0].as_str(), "sabotage");
        assert_eq!(own.secret_objectives[0].as_str(), "become_a_legend");
    }

    #[test]
    fn the_marker_matches_no_real_card() {
        // A redacted hand must not resolve against content, or a bot reading it would find a
        // card that nobody holds rather than failing.
        let content = ti4_content::ContentStore::embedded();
        assert!(
            content
                .get(ti4_model::content_types::ContentType::ActionCards, HIDDEN)
                .is_none(),
            "the marker is not an alias"
        );
    }

    #[test]
    fn a_view_reports_no_leaks_and_an_unredacted_state_does() {
        let state = table();
        let viewer = PlayerId::new("a");

        let raw = leaks(&state, &viewer);
        assert_eq!(raw.len(), 3, "the real state exposes all three: {raw:?}");

        assert!(
            leaks(&view_for(&state, &viewer), &viewer).is_empty(),
            "the view is clean"
        );
    }

    #[test]
    fn the_board_survives_redaction() {
        // Redaction that dropped public facts would make a bot play worse for a reason nothing
        // reported. Only the two private sequences may differ.
        let mut state = table();
        state
            .player_mut(&PlayerId::new("b"))
            .unwrap()
            .victory_points = 4;
        state.player_mut(&PlayerId::new("b")).unwrap().trade_goods = 7;

        let view = view_for(&state, &PlayerId::new("a"));
        let rival = view.player(&PlayerId::new("b")).unwrap();
        assert_eq!(rival.victory_points, 4);
        assert_eq!(rival.trade_goods, 7);
        assert_eq!(view.round, state.round);
        assert_eq!(view.board, state.board);
    }

    #[test]
    fn the_gaps_this_view_still_has_are_named() {
        // Not a behavioural assertion: a ledger. Redacting promissory notes would change what
        // bots see, so it has to be a decision somebody makes, not a diff nobody noticed.
        assert_eq!(UNREDACTED, ["promissory_notes"]);
        assert_eq!(PRIVATE_SEQUENCES, ["action_cards", "secret_objectives"]);
    }
}
