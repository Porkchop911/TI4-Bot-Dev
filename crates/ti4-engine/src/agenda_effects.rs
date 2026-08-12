//! Agenda effects (LRR 8, M06-014).
//!
//! Ported from the oracle's `engine/agenda.py` effect handlers.
//!
//! A first tranche. An agenda with no registered handler resolves its vote and announces the
//! effect unresolved — the same design every other registry here uses, and the same one the
//! oracle uses via `AGENDA_EFFECT_UNRESOLVED`.

use ti4_model::id::PlayerId;
use ti4_model::state::GameState;

use crate::objectives::VICTORY_TARGET;
use crate::vote::{AGAINST, Ballot, FOR};

/// What an effect did, for the caller to announce.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    /// The effect ran.
    Resolved { agenda: String },
    /// No handler is registered for this agenda.
    Unresolved { agenda: String },
}

/// Agendas this engine can resolve.
#[must_use]
pub fn registered_aliases() -> Vec<&'static str> {
    vec![
        "abolishment",
        "constitution",
        "economic_equality",
        "incentive",
        "mutiny",
        "seed_empire",
    ]
}

/// 98.4a caps a player at the target; a loss cannot take them below zero.
fn adjust_victory_points(state: &mut GameState, player: &PlayerId, delta: i32) {
    if let Some(seat) = state.player_mut(player) {
        seat.victory_points = (seat.victory_points + delta).clamp(0, VICTORY_TARGET);
    }
}

fn everyone(state: &GameState) -> Vec<PlayerId> {
    state.seating_order.clone()
}

/// Resolve one agenda's effect.
///
/// `speaker_choice` breaks a tie where the card names one player and several are level — 8.18
/// makes resolving the outcome the speaker's job, which is a decision rather than a guess at an
/// unwritten tie-break. It is passed in so this stays free of the choice machinery.
pub fn resolve(
    state: &mut GameState,
    content: &ti4_content::ContentStore,
    agenda: &str,
    outcome: &str,
    ballot: &Ballot,
    speaker_choice: impl Fn(&[PlayerId]) -> Option<PlayerId>,
) -> Effect {
    match agenda {
        "economic_equality" => {
            // Everyone's trade goods go back to the supply first, then For pays five each.
            // Doing it in that order matters: on Against, the card is purely destructive.
            for player in everyone(state) {
                if let Some(seat) = state.player_mut(&player) {
                    seat.trade_goods = 0;
                }
            }
            if outcome == FOR {
                for player in everyone(state) {
                    if let Some(seat) = state.player_mut(&player) {
                        seat.trade_goods += 5;
                    }
                }
            }
        }
        "mutiny" => {
            // Those who voted For gain a point on For, and lose one on Against. Read from the
            // ballot, not from the outcome: who voted which way is the whole card.
            let delta = if outcome == FOR { 1 } else { -1 };
            for player in ballot.voted_for(FOR) {
                adjust_victory_points(state, &player, delta);
            }
        }
        "seed_empire" => {
            let mut points: Vec<(PlayerId, i32)> = state
                .players
                .iter()
                .map(|seat| (seat.id.clone(), seat.victory_points))
                .collect();
            if points.is_empty() {
                return Effect::Resolved {
                    agenda: agenda.to_owned(),
                };
            }
            points.sort_by(|a, b| a.0.cmp(&b.0));
            let target = if outcome == FOR {
                points.iter().map(|(_, n)| *n).max()
            } else {
                points.iter().map(|(_, n)| *n).min()
            };
            let Some(target) = target else {
                return Effect::Resolved {
                    agenda: agenda.to_owned(),
                };
            };
            let tied: Vec<PlayerId> = points
                .into_iter()
                .filter(|(_, n)| *n == target)
                .map(|(player, _)| player)
                .collect();
            let winner = match tied.as_slice() {
                [only] => Some(only.clone()),
                _ => speaker_choice(&tied),
            };
            if let Some(winner) = winner {
                adjust_victory_points(state, &winner, 1);
            }
        }
        "abolishment" => {
            // Judicial Abolishment discards the *elected law*, so the outcome names a law
            // rather than For/Against. A repeal that ignored the outcome would discard
            // whatever happened to be first.
            crate::laws::repeal(state, outcome);
        }
        "constitution" => {
            // New Constitution discards every law in play, on For only.
            if outcome == FOR {
                for alias in crate::laws::in_play(state) {
                    crate::laws::repeal(state, &alias);
                }
            }
        }
        "incentive" => {
            // Stage I on For, stage II on Against. Not the top card: the deck is stage I then
            // stage II in order, so taking the top reveals the wrong stage while any stage I
            // remains, and the card would quietly do the opposite of what it says.
            let stage = if outcome == FOR { 1 } else { 2 };
            crate::objectives::reveal_stage(state, content, stage);
        }
        _ => {
            return Effect::Unresolved {
                agenda: agenda.to_owned(),
            };
        }
    }
    let _ = AGAINST;
    Effect::Resolved {
        agenda: agenda.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use ti4_content::ContentStore;

    use super::*;
    use crate::fixtures::game;

    fn a() -> PlayerId {
        PlayerId::new("a")
    }
    fn b() -> PlayerId {
        PlayerId::new("b")
    }

    fn no_choice(_: &[PlayerId]) -> Option<PlayerId> {
        None
    }

    fn ballot_for(voters: &[PlayerId]) -> Ballot {
        Ballot {
            votes: voters
                .iter()
                .map(|player| (player.clone(), FOR.to_owned()))
                .collect(),
            counts: BTreeMap::from([(FOR.to_owned(), 1)]),
        }
    }

    #[test]
    fn an_unregistered_agenda_reports_itself_unresolved() {
        // The same design every other registry here uses: the gap is announced, not hidden.
        let mut state = game(&["a"]);
        let effect = resolve(
            &mut state,
            ContentStore::embedded(),
            "not_an_agenda",
            FOR,
            &Ballot::default(),
            no_choice,
        );
        assert!(matches!(effect, Effect::Unresolved { .. }));
    }

    #[test]
    fn economic_equality_empties_the_supply_before_paying() {
        // On Against the card is purely destructive, which only holds if the wipe comes first.
        let mut state = game(&["a", "b"]);
        state.player_mut(&a()).unwrap().trade_goods = 9;
        state.player_mut(&b()).unwrap().trade_goods = 2;

        resolve(
            &mut state,
            ContentStore::embedded(),
            "economic_equality",
            AGAINST,
            &Ballot::default(),
            no_choice,
        );

        assert_eq!(state.player(&a()).unwrap().trade_goods, 0);
        assert_eq!(state.player(&b()).unwrap().trade_goods, 0);
    }

    #[test]
    fn economic_equality_pays_five_each_on_for() {
        let mut state = game(&["a", "b"]);
        state.player_mut(&a()).unwrap().trade_goods = 9;

        resolve(
            &mut state,
            ContentStore::embedded(),
            "economic_equality",
            FOR,
            &Ballot::default(),
            no_choice,
        );

        assert_eq!(state.player(&a()).unwrap().trade_goods, 5, "not 9, not 14");
        assert_eq!(state.player(&b()).unwrap().trade_goods, 5);
    }

    #[test]
    fn mutiny_rewards_or_punishes_the_players_who_voted_for() {
        // Read from the ballot, not the outcome: who voted which way is the whole card.
        let mut state = game(&["a", "b"]);
        let ballot = ballot_for(&[a()]);

        resolve(
            &mut state,
            ContentStore::embedded(),
            "mutiny",
            FOR,
            &ballot,
            no_choice,
        );
        assert_eq!(state.player(&a()).unwrap().victory_points, 1);
        assert_eq!(
            state.player(&b()).unwrap().victory_points,
            0,
            "b did not vote for it"
        );

        resolve(
            &mut state,
            ContentStore::embedded(),
            "mutiny",
            AGAINST,
            &ballot,
            no_choice,
        );
        assert_eq!(
            state.player(&a()).unwrap().victory_points,
            0,
            "the same voters lose one on Against"
        );
    }

    #[test]
    fn victory_points_never_go_below_zero() {
        // 98.4a caps the top; a loss must not take a player under.
        let mut state = game(&["a"]);
        resolve(
            &mut state,
            ContentStore::embedded(),
            "mutiny",
            AGAINST,
            &ballot_for(&[a()]),
            no_choice,
        );
        assert_eq!(state.player(&a()).unwrap().victory_points, 0);
    }

    #[test]
    fn seed_of_an_empire_gives_the_point_to_the_leader_on_for() {
        let mut state = game(&["a", "b"]);
        state.player_mut(&b()).unwrap().victory_points = 4;

        resolve(
            &mut state,
            ContentStore::embedded(),
            "seed_empire",
            FOR,
            &Ballot::default(),
            no_choice,
        );

        assert_eq!(state.player(&b()).unwrap().victory_points, 5);
        assert_eq!(state.player(&a()).unwrap().victory_points, 0);
    }

    #[test]
    fn seed_of_an_empire_gives_it_to_the_trailer_on_against() {
        let mut state = game(&["a", "b"]);
        state.player_mut(&b()).unwrap().victory_points = 4;

        resolve(
            &mut state,
            ContentStore::embedded(),
            "seed_empire",
            AGAINST,
            &Ballot::default(),
            no_choice,
        );

        assert_eq!(state.player(&a()).unwrap().victory_points, 1);
        assert_eq!(state.player(&b()).unwrap().victory_points, 4);
    }

    #[test]
    fn a_tie_is_the_speakers_decision_not_a_guess() {
        // 8.18: resolving the outcome is the speaker's job. With no decider the point simply
        // is not awarded, rather than being handed to whoever sorts first.
        let mut state = game(&["a", "b"]);

        resolve(
            &mut state,
            ContentStore::embedded(),
            "seed_empire",
            FOR,
            &Ballot::default(),
            no_choice,
        );
        assert_eq!(state.player(&a()).unwrap().victory_points, 0);
        assert_eq!(state.player(&b()).unwrap().victory_points, 0);

        resolve(
            &mut state,
            ContentStore::embedded(),
            "seed_empire",
            FOR,
            &Ballot::default(),
            |tied| tied.last().cloned(),
        );
        assert_eq!(state.player(&b()).unwrap().victory_points, 1);
    }

    #[test]
    fn judicial_abolishment_discards_the_law_it_elected() {
        // The outcome names a law, not For or Against. A repeal ignoring it would discard
        // whichever law happened to sort first.
        let mut state = game(&["a"]);
        state.enact_law("regulations", "for");
        state.enact_law("sanctions", "for");

        resolve(
            &mut state,
            ContentStore::embedded(),
            "abolishment",
            "sanctions",
            &Ballot::default(),
            no_choice,
        );

        assert!(crate::laws::active(&state, "regulations"), "untouched");
        assert!(!crate::laws::active(&state, "sanctions"), "discarded");
    }

    #[test]
    fn new_constitution_clears_the_table_only_on_for() {
        let mut state = game(&["a"]);
        state.enact_law("regulations", "for");
        state.enact_law("sanctions", "for");

        resolve(
            &mut state,
            ContentStore::embedded(),
            "constitution",
            AGAINST,
            &Ballot::default(),
            no_choice,
        );
        assert_eq!(
            crate::laws::in_play(&state).len(),
            2,
            "Against changes nothing"
        );

        resolve(
            &mut state,
            ContentStore::embedded(),
            "constitution",
            FOR,
            &Ballot::default(),
            no_choice,
        );
        assert!(crate::laws::in_play(&state).is_empty());
    }

    #[test]
    fn incentive_program_reveals_an_objective() {
        let mut state = game(&["a"]);
        let before = state.revealed_objectives.len();

        resolve(
            &mut state,
            ContentStore::embedded(),
            "incentive",
            FOR,
            &Ballot::default(),
            no_choice,
        );

        assert_eq!(state.revealed_objectives.len(), before + 1);
    }

    #[test]
    fn every_registered_alias_is_a_real_agenda() {
        for alias in registered_aliases() {
            assert!(
                ti4_content::ContentStore::embedded()
                    .get(ti4_model::content_types::ContentType::Agendas, alias)
                    .is_some(),
                "{alias} is not an agenda the corpus knows"
            );
        }
    }
}
