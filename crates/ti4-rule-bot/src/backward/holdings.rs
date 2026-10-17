//! Backward planning for the secrets that ask the seat to *hold* something at the status phase:
//! Form a Spy Network (discard 5 action cards), Destroy Heretical Works (purge 2 relic fragments)
//! and Strengthen Bonds (have another player's promissory note in your play area).
//!
//! What the seat controls is not wasting the enabling resource, so these are mostly pick-time
//! biases ([`keep`], [`answer`]) behind `bw_holdings`:
//!
//! - **fsn**: while the hand is no larger than five, no action card is played (the action menu and
//!   the reaction windows lose their card options). The plan adds the Politics card (two cards from
//!   its primary, two from a rival's secondary for a token) when the cards missing could come from
//!   them; otherwise nothing is steered.
//! - **dhw**: while the seat holds two to four fragments, it does not purge three for a relic (it
//!   would fall below two). Getting fragments is luck of the exploration of the planets it takes
//!   (the territory planners already steer those), so no plan is made.
//! - **sb**: a rival's Support for the Throne offered to the seat is accepted (the only note that
//!   goes faceup into a play area, and a point of its own). Whether a rival offers it is not the
//!   seat's to decide, so no plan is made.
//!
//! # Objectives (exactly; checked by `covers_exactly_the_documented_objectives`)
//!
//! | alias | name | kind | points | family |
//! |---|---|---|---|---|
//! | `fsn` | Form a Spy Network | secret | 1 | `action_cards` |
//! | `dhw` | Destroy Heretical Works | secret | 1 | `relic_fragments` |
//! | `sb` | Strengthen Bonds | secret | 1 | `rival_note_issuers` |

use ti4_engine::choice::{Choice, ChoiceOption, SeatObservation};
use ti4_engine::objectives::Imagined;
use ti4_model::id::PlayerId;

use super::{Cost, ObjectivePlanner, Plan, PlanCtx, Steer};
use crate::objectives::{self, Goal};
use crate::tuning;

/// The secrets this group plans.
pub const IDS: &[&str] = &["fsn", "dhw", "sb"];
/// The aliases this group plans, as documented at the top of the file.
pub const COVERS: &[&str] = &["fsn", "dhw", "sb"];

/// Action cards a Politics primary draws, and a secondary.
pub const POLITICS_CARDS: u32 = 2;
/// Probability that the seat's own Politics primary is played this round.
pub const OWN_CARD_P: f64 = 0.95;
/// Probability that a rival plays Politics and the secondary is open.
pub const SECONDARY_P: f64 = 0.7;

/// The held secret `alias` as a goal, met or not.
fn held_goal(seen: &SeatObservation<'_>, me: &PlayerId, alias: &str) -> Option<Goal> {
    objectives::goals(seen, me, &Imagined::NONE)
        .into_iter()
        .find(|g| g.alias == alias && g.secret)
}

fn is_card_play(option: &ChoiceOption) -> bool {
    option.kind == "action_card" || option.id.starts_with("action_card|")
}

fn is_purge(option: &ChoiceOption) -> bool {
    option.id.contains("purge|")
}

/// Options to keep (indices), when some of the choice's options would spend what a held secret
/// needs: action cards while Form a Spy Network is five or fewer cards from done, a relic purge
/// while Destroy Heretical Works holds two to four fragments. `None`: nothing is held back.
#[must_use]
pub fn keep(choice: &Choice, seen: &SeatObservation<'_>, me: &PlayerId) -> Option<Vec<usize>> {
    if !tuning::bw_enabled() || !tuning::bw_holdings() {
        return None;
    }
    let holds_cards = held_goal(seen, me, "fsn").is_some_and(|g| g.have <= g.threshold);
    let holds_fragments = held_goal(seen, me, "dhw")
        .is_some_and(|g| g.have >= g.threshold && g.have - 3.0 < g.threshold);
    if !holds_cards && !holds_fragments {
        return None;
    }
    let keep: Vec<usize> = choice
        .options
        .iter()
        .enumerate()
        .filter(|(_, o)| !(holds_cards && is_card_play(o)) && !(holds_fragments && is_purge(o)))
        .map(|(i, _)| i)
        .collect();
    (keep.len() < choice.options.len() && !keep.is_empty()).then_some(keep)
}

/// Strengthen Bonds: accept an offer that hands the seat a rival's Support for the Throne.
#[must_use]
pub fn answer(choice: &Choice, seen: &SeatObservation<'_>, me: &PlayerId) -> Option<usize> {
    if !tuning::bw_enabled() || !tuning::bw_holdings() {
        return None;
    }
    let subtype = choice.context.as_ref().map(|c| c.subtype.as_str())?;
    if subtype != "answer_transaction" {
        return None;
    }
    held_goal(seen, me, "sb").filter(|g| !g.satisfied)?;
    let gives_support = choice
        .prompt
        .split(" for ")
        .next()
        .is_some_and(|part| part.contains(ti4_engine::promissory::SUPPORT_PREFIX));
    if !gives_support {
        return None;
    }
    choice.options.iter().position(|o| o.id == "accept")
}

/// Turns of Politics (own primary, rival secondaries) that could draw `missing` cards, and their
/// probability; `None` when out of reach.
#[must_use]
pub fn politics_route(
    missing: u32,
    own_open: bool,
    rivals_open: u32,
    tokens: i32,
) -> Option<(f64, i32, bool)> {
    let mut sources: Vec<(f64, i32, bool)> = Vec::new();
    if own_open {
        sources.push((OWN_CARD_P, 0, true));
    }
    let mut left = tokens;
    for _ in 0..rivals_open {
        if left >= 1 {
            left -= 1;
            sources.push((SECONDARY_P, 1, false));
        }
    }
    sources.sort_by(|a, b| b.0.total_cmp(&a.0));
    let needed = usize::try_from(missing.div_ceil(POLITICS_CARDS)).ok()?;
    if needed == 0 || needed > sources.len() {
        return None;
    }
    let used = &sources[..needed];
    Some((
        used.iter().map(|s| s.0).product(),
        used.iter().map(|s| s.1).sum(),
        used.iter().any(|s| s.2),
    ))
}

fn politics_open(
    seen: &ti4_engine::choice::Observed<'_>,
    seat: &ti4_engine::choice::PublicSeat<'_>,
) -> bool {
    seat.strategy_cards.iter().any(|c| {
        !seat.exhausted_strategy_cards.contains(c)
            && ti4_engine::strategy_cards::card_name(seen.content(), c.as_str()).as_deref()
                == Some("Politics")
    })
}

/// The holdings group.
pub struct Holdings;

impl ObjectivePlanner for Holdings {
    fn objective_ids(&self) -> &[&str] {
        IDS
    }

    fn plan(&self, obs: &SeatObservation<'_>, ctx: &PlanCtx) -> Option<Plan> {
        if ctx.goal.alias != "fsn" {
            return None;
        }
        let observed = obs.observed();
        let seat = observed.seat(&ctx.me)?;
        let own_open = politics_open(observed, &seat);
        let rivals = u32::try_from(
            observed
                .players()
                .into_iter()
                .filter(|p| **p != ctx.me)
                .filter_map(|p| observed.seat(p))
                .filter(|s| !s.passed && politics_open(observed, s))
                .count(),
        )
        .unwrap_or(0);
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a small count"
        )]
        let missing = ctx.goal.gap().ceil().max(0.0) as u32;
        let (probability, tokens, uses_own) =
            politics_route(missing, own_open, rivals, seat.strategic_tokens)?;
        Some(Plan {
            objective: ctx.goal.alias.clone(),
            points: ctx.goal.points,
            probability,
            cost: Cost {
                tokens,
                resources: 0,
                activations: 0,
            },
            steer: Steer {
                cards: if uses_own {
                    vec!["Politics".to_owned()]
                } else {
                    Vec::new()
                },
                ..Steer::default()
            },
            reason: format!("fsn: {missing} action card(s) from Politics"),
            payment: false,
            secret: true,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn politics_draws_two_cards_per_turn() {
        // Three missing: two turns of Politics (own + one secondary).
        let (p, tokens, own) = politics_route(3, true, 1, 1).unwrap();
        assert!((p - 0.95 * 0.7).abs() < 1e-9 && tokens == 1 && own);
        // Two missing: the own card alone.
        assert_eq!(politics_route(2, true, 0, 0).map(|r| r.1), Some(0));
        // Out of reach: no card, no token.
        assert!(politics_route(2, false, 1, 0).is_none());
        assert!(politics_route(5, true, 1, 1).is_none());
        assert!(politics_route(0, true, 0, 0).is_none());
    }

    #[test]
    fn card_plays_and_purges_are_recognised() {
        assert!(is_card_play(&ChoiceOption::labelled(
            "action_card|0",
            "component",
            "play x"
        )));
        assert!(is_card_play(&ChoiceOption::labelled(
            "sabotage",
            "action_card",
            "play x"
        )));
        assert!(!is_card_play(&ChoiceOption::labelled(
            "tactical", "action", "t"
        )));
        assert!(is_purge(&ChoiceOption::labelled(
            "purge|CULTURAL",
            "component",
            "purge"
        )));
    }

    #[test]
    fn covers_exactly_the_documented_objectives() {
        assert_eq!(
            crate::backward::coverage::matched(&Holdings),
            crate::backward::coverage::sorted(COVERS)
        );
    }

    use ti4_content::ContentStore;
    use ti4_engine::choice::{Decider, IllegalChoice};
    use ti4_engine::fixtures::game;
    use ti4_model::POK;
    use ti4_model::id::{ActionCardId, SecretObjectiveId};

    struct KeepProbe(Option<Vec<usize>>);
    impl Decider for KeepProbe {
        fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
            Ok(choice.options[0].clone())
        }
        fn choose_seeing(
            &mut self,
            choice: &Choice,
            seen: &SeatObservation<'_>,
        ) -> Result<ChoiceOption, IllegalChoice> {
            self.0 = keep(choice, seen, &PlayerId::new("a"));
            Ok(choice.options[0].clone())
        }
    }

    #[test]
    fn spy_network_holds_back_the_action_cards_until_five_are_spare() {
        let menu = Choice::new(
            PlayerId::new("a"),
            "take an action",
            vec![
                ChoiceOption::labelled("tactical", "action", "tactical"),
                ChoiceOption::labelled("action_card|0", "component", "play x"),
                ChoiceOption::labelled("pass", "action", "pass"),
            ],
        );
        let kept = |cards: usize, on: bool| {
            let mut state = game(&["a", "b"]);
            state.round = 3;
            {
                let seat = state.player_mut(&PlayerId::new("a")).expect("seat");
                seat.secret_objectives = vec![SecretObjectiveId::new("fsn")];
                seat.action_cards = (0..cards)
                    .map(|i| ActionCardId::new(format!("c{i}")))
                    .collect();
            }
            let mut knobs = crate::tuning::Tuning::default();
            knobs.bw_enabled = on;
            knobs.bw_holdings = on;
            let mut probe = KeepProbe(None);
            crate::tuning::with(knobs, || {
                ti4_engine::choice::ask_private(
                    &menu,
                    &state,
                    ContentStore::embedded(),
                    POK,
                    None,
                    &mut probe,
                )
                .expect("legal");
            });
            probe.0
        };
        assert_eq!(kept(3, true), Some(vec![0, 2]));
        assert_eq!(kept(5, true), Some(vec![0, 2]));
        // Six cards: one can be played and five remain.
        assert_eq!(kept(6, true), None);
        // The knob off: nothing held back.
        assert_eq!(kept(3, false), None);
    }
}
