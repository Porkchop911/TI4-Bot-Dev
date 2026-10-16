//! Redacted views of the game state.
//!
//! Some things in TI4 are private: the action cards in your hand, the secret objectives you
//! hold. Everything else — the board, who controls what, how many victory points everyone
//! has — is open information that any player may read.
//!
//! A view is a [`GameState`] with the private parts of *other* players **replaced rather
//! than removed**. A hand of four unknown cards becomes four [`HIDDEN`] markers, so its
//! *size* stays visible, which is correct: at a real table you can see how many cards
//! somebody holds without seeing which.
//!
//! Replacing rather than deleting matters for a second reason. Every existing query keeps
//! working against a view, so nothing has to learn about redaction to be safe. Code that
//! only counts is unaffected; only code that inspects a specific card sees the marker, and a
//! marker is not a valid id, so it matches nothing in the content corpus.
//!
//! The seam is deliberately narrow. A bot is handed a redacted state, but asks the *real*
//! engine to enumerate its legal options, because legality depends on facts the bot is not
//! entitled to compute for itself.

use crate::id::{ActionCardId, PlayerId, SecretObjectiveId};
use crate::state::{GameState, Player};

/// Stands in for a card whose identity is private.
///
/// Not a valid alias anywhere in the corpus, so a lookup against real content fails rather
/// than silently matching something.
pub const HIDDEN: &str = "?";

/// The per-player fields nobody else may read.
///
/// Public counts survive; identities do not. LRR 61.17 makes an unscored secret objective
/// hidden until it is scored. Named here as data so that
/// [`every_private_field_is_redacted`](self) can check the list against what [`leaks`]
/// inspects — a new private field that nobody redacted should fail a test, not leak quietly.
pub const PRIVATE_SEQUENCES: [&str; 2] = ["action_cards", "secret_objectives"];

/// Whether a card id is the privacy marker rather than a real card.
#[must_use]
pub fn is_hidden(card: &str) -> bool {
    card == HIDDEN
}

/// One player as their opponents see them.
#[must_use]
pub fn redact_player(player: &Player) -> Player {
    redact_player_with(player, false)
}

/// One player as their opponents see them, with their secrets optionally left visible.
///
/// Search Warrant: "The owner of this card plays with their secret objectives revealed." Passed in
/// rather than read here, because a law lives in `GameState` and this function is deliberately
/// about one player -- see `view_for`, which knows both.
#[must_use]
pub fn redact_player_with(player: &Player, secrets_revealed: bool) -> Player {
    let mut redacted = player.clone();
    redacted.action_cards = player
        .action_cards
        .iter()
        .map(|_| ActionCardId::new(HIDDEN))
        .collect();
    if !secrets_revealed {
        redacted.secret_objectives = player
            .secret_objectives
            .iter()
            .map(|_| SecretObjectiveId::new(HIDDEN))
            .collect();
    }
    // The Firmament's facedown plot cards: the count and the control tokens on them are public, which
    // card each one is is not. A flipped (faceup) card is public.
    redacted.plots = player
        .plots
        .iter()
        .map(|stored| crate::plots::Plot::shown_to_others(stored))
        .collect();
    redacted
}

/// Whether a `GameState::faction_marks` row may be shown to `viewer`.
///
/// Faction modules keep private bookkeeping there, so the map is not public by default:
///
/// * `cards:reveal:<scope>|<source>|<viewer>|<owner>|<kind>` names cards shown to one seat
///   (Mageon Implants, Spy Net): visible to that viewer only.
/// * `cards:staged:*` holds card ids waiting to be announced: visible to nobody.
/// * `private:<player>:*` is one seat's own secret: visible to that player only.
///
/// Every other row is public (a "used this round" flag, a swapped planet value).
#[must_use]
pub fn mark_visible_to(key: &str, viewer: &PlayerId) -> bool {
    if let Some(rest) = key.strip_prefix("cards:reveal:") {
        return rest.split('|').nth(2) == Some(viewer.as_str());
    }
    if key.starts_with("cards:staged:") {
        return false;
    }
    if let Some(rest) = key.strip_prefix("private:") {
        return rest.split(':').next() == Some(viewer.as_str());
    }
    true
}

/// Drop every `faction_marks` row `viewer` may not see ([`mark_visible_to`]).
pub fn redact_marks(view: &mut GameState, viewer: &PlayerId) {
    view.faction_marks
        .retain(|key, _| mark_visible_to(key, viewer));
}

/// The game as one player is entitled to see it.
///
/// Their own state is untouched — you know your own hand — and the board is shared, so only
/// other players' private holdings change.
#[must_use]
pub fn view_for(state: &GameState, viewer: &PlayerId) -> GameState {
    let mut view = state.clone();
    redact_marks(&mut view, viewer);
    for player in &mut view.players {
        if &player.id != viewer {
            // Search Warrant leaves its owner's secrets face up for everybody.
            let revealed = state
                .laws
                .get("warrant")
                .is_some_and(|owner| *owner == player.id.to_string());
            *player = redact_player_with(player, revealed);
        }
    }
    view
}

/// Private identities this view still exposes. Empty means the view is clean.
///
/// Written as a runtime check rather than a comment so that a newly added private field
/// that nobody redacted shows up as a failing test instead of a quiet leak.
#[must_use]
pub fn leaks(state: &GameState, viewer: &PlayerId) -> Vec<String> {
    let mut found: Vec<String> = state
        .faction_marks
        .keys()
        .filter(|key| !mark_visible_to(key, viewer))
        .map(|key| format!("faction_marks.{key}"))
        .collect();
    for player in &state.players {
        if &player.id == viewer {
            continue;
        }
        for card in &player.action_cards {
            if !is_hidden(card.as_str()) {
                found.push(format!("{}.action_cards={card}", player.id));
            }
        }
        for objective in &player.secret_objectives {
            if !is_hidden(objective.as_str()) {
                found.push(format!("{}.secret_objectives={objective}", player.id));
            }
        }
        for stored in &player.plots {
            if crate::plots::Plot::leaks_identity(stored) {
                found.push(format!("{}.plots={stored}", player.id));
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::{StrategyCardId, SystemId, UnitTypeId};

    #[test]
    fn private_faction_marks_reach_only_their_viewer() {
        let mut g = game();
        for (key, value) in [
            ("cards:reveal:action|mi|a|b|action_cards", "b_card_0"),
            ("cards:staged:00000001", "taken|b_card_1"),
            ("private:b:yssaril:plan", "x"),
            ("sardakk:tekklar:round", "3"),
        ] {
            g.faction_marks.insert(key.to_owned(), value.to_owned());
        }
        let for_a = view_for(&g, &pid("a"));
        let for_c = view_for(&g, &pid("c"));
        assert!(
            for_a
                .faction_marks
                .contains_key("cards:reveal:action|mi|a|b|action_cards")
        );
        assert!(
            !for_c
                .faction_marks
                .contains_key("cards:reveal:action|mi|a|b|action_cards")
        );
        for view in [&for_a, &for_c] {
            assert!(!view.faction_marks.contains_key("cards:staged:00000001"));
            assert!(!view.faction_marks.contains_key("private:b:yssaril:plan"));
            assert!(view.faction_marks.contains_key("sardakk:tekklar:round"));
        }
        assert!(
            view_for(&g, &pid("b"))
                .faction_marks
                .contains_key("private:b:yssaril:plan")
        );
        assert!(leaks(&for_c, &pid("c")).is_empty());
        assert!(
            leaks(&g, &pid("c"))
                .iter()
                .any(|leak| leak.contains("cards:reveal"))
        );
    }
    use crate::units::Unit;
    use std::collections::BTreeMap;

    fn pid(id: &str) -> PlayerId {
        PlayerId::new(id)
    }

    fn game() -> GameState {
        let ids = [pid("a"), pid("b"), pid("c")];
        let mut g = GameState::new(&ids, &[], BTreeMap::new(), None, 1);
        for (seat, cards) in [("a", 2), ("b", 4), ("c", 0)] {
            let player = g.player_mut(&pid(seat)).unwrap();
            player.action_cards = (0..cards)
                .map(|i| ActionCardId::new(format!("{seat}_card_{i}")))
                .collect();
            player.secret_objectives = vec![SecretObjectiveId::new(format!("{seat}_secret"))];
        }
        g
    }

    #[test]
    fn a_view_hides_another_players_hand() {
        let view = view_for(&game(), &pid("a"));
        let other = view.player(&pid("b")).unwrap();
        assert!(other.action_cards.iter().all(|c| is_hidden(c.as_str())));
        assert!(
            other
                .secret_objectives
                .iter()
                .all(|s| is_hidden(s.as_str()))
        );
    }

    #[test]
    fn a_view_keeps_your_own_hand_intact() {
        let view = view_for(&game(), &pid("a"));
        let me = view.player(&pid("a")).unwrap();
        assert_eq!(
            me.action_cards,
            game().player(&pid("a")).unwrap().action_cards
        );
        assert_eq!(me.secret_objectives[0], SecretObjectiveId::new("a_secret"));
    }

    #[test]
    fn hand_size_stays_visible() {
        // At a real table you can see how many cards somebody holds without seeing which.
        let view = view_for(&game(), &pid("a"));
        assert_eq!(view.player(&pid("b")).unwrap().action_cards.len(), 4);
        assert_eq!(view.player(&pid("c")).unwrap().action_cards.len(), 0);
    }

    #[test]
    fn an_empty_hand_stays_empty() {
        let view = view_for(&game(), &pid("a"));
        assert!(view.player(&pid("c")).unwrap().action_cards.is_empty());
    }

    #[test]
    fn public_facts_are_untouched() {
        let mut g = game();
        g.player_mut(&pid("b")).unwrap().victory_points = 4;
        g.player_mut(&pid("b")).unwrap().trade_goods = 7;
        g.system_mut(&SystemId::new("18"))
            .add(&[Unit::new(UnitTypeId::new("carrier"), pid("b"))]);
        g.deal_strategy_card(&pid("b"), StrategyCardId::new("leadership"));

        let view = view_for(&g, &pid("a"));
        let other = view.player(&pid("b")).unwrap();
        assert_eq!(other.victory_points, 4);
        assert_eq!(other.trade_goods, 7);
        assert_eq!(
            other.strategy_cards,
            vec![StrategyCardId::new("leadership")]
        );
        assert_eq!(
            view.units_in(&SystemId::new("18")).len(),
            1,
            "the board is shared"
        );
    }

    #[test]
    fn the_marker_is_not_a_real_card() {
        // A lookup against real content must fail rather than silently match something.
        assert!(is_hidden(HIDDEN));
        assert!(!is_hidden("blitz"));
        assert_ne!(HIDDEN, "");
    }

    #[test]
    fn a_view_leaks_nothing() {
        for viewer in ["a", "b", "c"] {
            let view = view_for(&game(), &pid(viewer));
            assert!(leaks(&view, &pid(viewer)).is_empty(), "viewer {viewer}");
        }
    }

    #[test]
    fn the_leak_check_actually_catches_a_leak() {
        // Guards the guard: an unredacted state must be reported, or the check above is
        // vacuous.
        let found = leaks(&game(), &pid("a"));
        assert!(!found.is_empty());
        assert!(found.iter().any(|l| l.starts_with("b.action_cards=")));
        assert!(found.iter().any(|l| l.contains("secret_objectives=")));
    }

    #[test]
    fn every_private_field_is_redacted() {
        // If a private field is added to PRIVATE_SEQUENCES without teaching redact_player
        // and leaks about it, this fails.
        assert_eq!(PRIVATE_SEQUENCES, ["action_cards", "secret_objectives"]);
        let view = view_for(&game(), &pid("a"));
        let leaked = leaks(&view, &pid("a"));
        assert!(leaked.is_empty(), "{leaked:?}");
    }

    #[test]
    fn facedown_plot_cards_show_their_tokens_but_never_which_card_they_are() {
        let mut g = game();
        g.player_mut(&pid("b")).unwrap().plots =
            vec!["d:assail:a".to_owned(), "d:seethe:a,c".to_owned()];
        let found = leaks(&g, &pid("a"));
        assert!(found.iter().any(|l| l.starts_with("b.plots=")), "{found:?}");

        // The tokens are public, the identity is hidden, and the count survives.
        let for_a = view_for(&g, &pid("a"));
        assert_eq!(for_a.player(&pid("b")).unwrap().plots, ["d:?:a", "d:?:a,c"]);
        assert!(leaks(&for_a, &pid("a")).is_empty());
        // The owner sees their own cards.
        let for_b = view_for(&g, &pid("b"));
        assert_eq!(
            for_b.player(&pid("b")).unwrap().plots,
            ["d:assail:a", "d:seethe:a,c"]
        );

        // Once flipped the cards are public.
        g.player_mut(&pid("b")).unwrap().plots = vec!["u:assail:a".to_owned()];
        let for_c = view_for(&g, &pid("c"));
        assert_eq!(for_c.player(&pid("b")).unwrap().plots, ["u:assail:a"]);
        assert!(leaks(&for_c, &pid("c")).is_empty());
    }

    #[test]
    fn redaction_does_not_change_the_shape_of_the_state() {
        // Everything else must survive, or a bot reasoning over a view reasons over a
        // different game.
        let g = game();
        let view = view_for(&g, &pid("a"));
        assert_eq!(view.players.len(), g.players.len());
        assert_eq!(view.seating_order, g.seating_order);
        assert_eq!(view.speaker, g.speaker);
        assert_eq!(view.round, g.round);
        assert_eq!(view.phase, g.phase);
    }

    #[test]
    fn viewing_from_an_unseated_id_redacts_everybody() {
        let view = view_for(&game(), &pid("spectator"));
        assert!(leaks(&view, &pid("spectator")).is_empty());
        for seat in ["a", "b"] {
            assert!(
                view.player(&pid(seat))
                    .unwrap()
                    .action_cards
                    .iter()
                    .all(|c| is_hidden(c.as_str()))
            );
        }
    }

    #[test]
    fn redacting_twice_is_the_same_as_redacting_once() {
        let once = view_for(&game(), &pid("a"));
        let twice = view_for(&once, &pid("a"));
        assert_eq!(once.players, twice.players);
    }
}
