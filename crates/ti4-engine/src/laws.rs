//! Laws in play (LRR 8.20, and the standing rules they impose).
//!
//! Ported from the oracle's `engine/laws.py`: `in_play`, `active`, `elected`, `repeal`,
//! `fleet_pool_cap`, `action_card_limit`, `nebulae_passable`, and `unimplemented`.
//!
//! A law is *enacted* by a vote and *enforced* by whatever rule it changes. Those are separate:
//! this engine could already enact every law, and enforced none of them, so `state.laws` was a
//! list nothing read. These are the first that bite.

use ti4_content::ContentStore;
use ti4_model::content_types::{ContentType, SourceSet};
use ti4_model::id::PlayerId;
use ti4_model::state::GameState;

use crate::objectives::VICTORY_TARGET;

/// Laws currently in play.
#[must_use]
pub fn in_play(state: &GameState) -> Vec<String> {
    state.laws.keys().cloned().collect()
}

/// Whether a law is in play.
#[must_use]
pub fn active(state: &GameState, alias: &str) -> bool {
    state.laws.contains_key(alias)
}

/// What this law was elected onto — a planet or a player (8.9 to 8.11).
///
/// For a For/Against law the value is the outcome itself, which is why a caller meaning "the
/// elected planet" must check it against the board rather than trusting it blindly.
#[must_use]
pub fn elected<'a>(state: &'a GameState, alias: &str) -> Option<&'a String> {
    state.laws.get(alias)
}

/// Remove a law, resolving any printed consequence of losing its card.
///
/// Public Censure is the one with a consequence: its holder loses the victory point it gave
/// them. A repeal that only deleted the entry would leave that point behind for good.
pub fn repeal(state: &mut GameState, alias: &str) -> bool {
    let Some(owner) = state.laws.get(alias).cloned() else {
        return false;
    };
    state.laws.remove(alias);
    if alias == "censure" {
        let holder = PlayerId::new(owner);
        if let Some(seat) = state.player_mut(&holder) {
            seat.victory_points = (seat.victory_points - 1).clamp(0, VICTORY_TARGET);
        }
    }
    true
}

/// Fleet Regulations caps the fleet pool at four.
#[must_use]
pub fn fleet_pool_cap(state: &GameState, base: i32) -> i32 {
    if active(state, "regulations") {
        base.min(4)
    } else {
        base
    }
}

/// Sanctions caps the action-card hand limit at three.
#[must_use]
pub fn action_card_limit(state: &GameState, base: usize) -> usize {
    if active(state, "sanctions") {
        base.min(3)
    } else {
        base
    }
}

/// Political Censure: its elected owner cannot play action cards.
#[must_use]
pub fn action_cards_forbidden(state: &GameState, player: &PlayerId) -> bool {
    active(state, "censure") && elected(state, "censure").is_some_and(|who| who == player.as_str())
}

/// Shared Research makes nebulae passable.
#[must_use]
pub fn nebulae_passable(state: &GameState) -> bool {
    active(state, "shared_research")
}

/// Laws this engine can enact but not enforce — the honest coverage gap.
#[must_use]
pub fn enforced_aliases() -> Vec<&'static str> {
    vec!["censure", "regulations", "sanctions", "shared_research"]
}

/// Laws in the corpus that nothing here consults.
#[must_use]
pub fn unimplemented(content: &ContentStore, sources: SourceSet) -> Vec<String> {
    let enforced = enforced_aliases();
    content
        .from_sources(ContentType::Agendas, sources)
        .filter(|record| record.text("type") == Some("Law"))
        .filter_map(|record| record.text("alias"))
        .filter(|alias| !enforced.contains(alias))
        .map(ToOwned::to_owned)
        .collect()
}

#[cfg(test)]
mod tests {
    use ti4_model::content_types::POK;

    use super::*;
    use crate::fixtures::game;

    fn player() -> PlayerId {
        PlayerId::new("a")
    }

    #[test]
    fn a_law_is_in_play_once_enacted() {
        let mut state = game(&["a"]);
        assert!(!active(&state, "regulations"));

        state.enact_law("regulations", "for");

        assert!(active(&state, "regulations"));
        assert_eq!(in_play(&state), vec!["regulations".to_owned()]);
        assert_eq!(elected(&state, "regulations"), Some(&"for".to_owned()));
    }

    #[test]
    fn fleet_regulations_caps_the_pool_at_four() {
        let mut state = game(&["a"]);
        assert_eq!(fleet_pool_cap(&state, 6), 6, "uncapped without the law");

        state.enact_law("regulations", "for");
        assert_eq!(fleet_pool_cap(&state, 6), 4);
        assert_eq!(fleet_pool_cap(&state, 3), 3, "a cap never raises anything");
    }

    #[test]
    fn sanctions_caps_the_action_card_hand() {
        let mut state = game(&["a"]);
        assert_eq!(action_card_limit(&state, 7), 7);

        state.enact_law("sanctions", "for");
        assert_eq!(action_card_limit(&state, 7), 3);
    }

    #[test]
    fn shared_research_opens_the_nebulae() {
        let mut state = game(&["a"]);
        assert!(!nebulae_passable(&state));
        state.enact_law("shared_research", "for");
        assert!(nebulae_passable(&state));
    }

    #[test]
    fn repealing_public_censure_takes_its_victory_point_back() {
        // A repeal that only deleted the entry would leave the point behind for good.
        let mut state = game(&["a"]);
        state.enact_law("censure", "a");
        state.player_mut(&player()).unwrap().victory_points = 1;

        assert!(repeal(&mut state, "censure"));

        assert!(!active(&state, "censure"));
        assert_eq!(state.player(&player()).unwrap().victory_points, 0);
    }

    #[test]
    fn a_censure_repeal_cannot_take_a_player_below_zero() {
        let mut state = game(&["a"]);
        state.enact_law("censure", "a");
        repeal(&mut state, "censure");
        assert_eq!(state.player(&player()).unwrap().victory_points, 0);
    }

    #[test]
    fn repealing_a_law_that_is_not_in_play_does_nothing() {
        let mut state = game(&["a"]);
        let before = state.clone();
        assert!(!repeal(&mut state, "regulations"));
        assert!(state.identical(&before));
    }

    #[test]
    fn the_unenforced_laws_are_reported() {
        // Enacting a law nothing reads is the failure this module exists to make visible.
        let unenforced = unimplemented(ContentStore::embedded(), POK);
        assert!(!unenforced.is_empty(), "most laws are still unenforced");
        for alias in enforced_aliases() {
            assert!(
                !unenforced.contains(&alias.to_owned()),
                "{alias} is enforced and should not be listed"
            );
        }
    }

    #[test]
    fn every_enforced_alias_is_a_real_law() {
        for alias in enforced_aliases() {
            let record = ContentStore::embedded().get(ContentType::Agendas, alias);
            assert!(
                record.is_some(),
                "{alias} is not an agenda the corpus knows"
            );
            assert_eq!(
                record.and_then(|r| r.text("type")),
                Some("Law"),
                "{alias} is not a law"
            );
        }
    }
}
