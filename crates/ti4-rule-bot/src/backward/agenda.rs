//! Backward planning for the agenda secrets: Drive the Debate (you, or a planet you control, are
//! elected by an agenda) and Dictate Policy (three or more laws in play). Both are scored in the
//! agenda phase, which only happens once the custodians token is gone, and both depend on what
//! the agenda deck turns up and on the vote.
//!
//! Backwards: Drive the Debate needs an election agenda (about 45% of the deck) to reveal and the
//! seat's own name or planet to win the vote; Dictate Policy needs agendas that become laws (a
//! For/Against law that passes, or any election law) until the count is three. The seat steers
//! the one part it controls, its votes. For Drive the Debate nothing is left to add: the existing
//! vote rule (`answers.rs`) already votes for the seat or its planet in every election that does
//! not harm the elected, so only the odds are reported. For Dictate Policy the seat votes For on a
//! law that is not an election while it has fewer than three laws ([`AgendaBias`], used by
//! `answers::pick_with`). The plans carry no board steer; the weighing may drop them below the
//! threshold.
//!
//! # Objectives (exactly; checked by `covers_exactly_the_documented_objectives`)
//!
//! | alias | name | kind | points | family |
//! |---|---|---|---|---|
//! | `dtd` | Drive the Debate | secret | 1 | none (event, agenda phase) |
//! | `dp` | Dictate Policy | secret | 1 | `laws` |

use ti4_engine::choice::SeatObservation;
use ti4_model::content_types::ContentType;
use ti4_model::id::PlayerId;

use super::{Cost, ObjectivePlanner, Plan, PlanCtx, Steer};
use crate::objectives::Goal;
use crate::tuning;

/// The secrets this group plans.
pub const IDS: &[&str] = &["dtd", "dp"];
/// The aliases this group plans, as documented at the top of the file.
pub const COVERS: &[&str] = &["dtd", "dp"];

/// Share of the agenda deck that elects a player or a planet.
pub const ELECTION_SHARE: f64 = 0.45;
/// Share of the deck that becomes a law when it passes (laws, of which elections always pass).
pub const LAW_SHARE: f64 = 0.38;
/// Agendas revealed in one agenda phase.
pub const AGENDAS: u32 = 2;

/// Which secrets bias the seat's votes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AgendaBias {
    pub dp: bool,
}

impl AgendaBias {
    /// The bias of the seat: nothing with the knob off, For on laws while it holds Dictate
    /// Policy and has fewer than three laws.
    #[must_use]
    pub fn of(seen: &SeatObservation<'_>) -> Self {
        if !tuning::bw_enabled() || !tuning::bw_agenda() {
            return Self::default();
        }
        let held = seen.held_secrets();
        Self {
            dp: held.iter().any(|s| s.as_str() == "dp")
                && seen
                    .held_secret_progress()
                    .iter()
                    .any(|p| p.alias == "dp" && !p.satisfied),
        }
    }
}

/// The type (`Law` or `Directive`) and target text of an agenda.
#[must_use]
pub fn agenda_kind(
    seen: &ti4_engine::choice::Observed<'_>,
    agenda: &str,
) -> Option<(String, String)> {
    let record = seen.content().get(ContentType::Agendas, agenda)?;
    Some((
        record.text("type")?.to_owned(),
        record.text("target")?.to_owned(),
    ))
}

/// The chance that at least `k` of `n` independent agendas each do it with probability `p`.
#[must_use]
pub fn at_least_of(n: u32, p: f64, k: u32) -> f64 {
    super::abilities::at_least(n as usize, p, k as usize)
}

fn influence(seen: &ti4_engine::choice::Observed<'_>, player: &PlayerId) -> f64 {
    #[expect(clippy::cast_precision_loss, reason = "single digits")]
    let total = seen
        .controlled_planets(player)
        .into_iter()
        .filter_map(|(_, planet)| {
            ti4_content::galaxy::planet(seen.content(), planet.as_str(), seen.sources())
        })
        .map(|record| record.influence())
        .sum::<i64>() as f64;
    total
}

/// The seat's share of the table's influence (what its votes weigh).
fn vote_share(seen: &ti4_engine::choice::Observed<'_>, me: &PlayerId) -> f64 {
    let all: f64 = seen.players().into_iter().map(|p| influence(seen, p)).sum();
    if all <= 0.0 {
        return 0.0;
    }
    influence(seen, me) / all
}

/// The agenda group.
pub struct Agenda;

impl ObjectivePlanner for Agenda {
    fn objective_ids(&self) -> &[&str] {
        IDS
    }

    fn plan(&self, obs: &SeatObservation<'_>, ctx: &PlanCtx) -> Option<Plan> {
        let observed = obs.observed();
        // No agenda phase before the custodians token is gone.
        if !observed.custodians_removed() {
            return None;
        }
        let probability = match ctx.goal.alias.as_str() {
            "dtd" => {
                let win = (1.6 * vote_share(observed, &ctx.me)).clamp(0.1, 0.9);
                at_least_of(AGENDAS, ELECTION_SHARE, 1) * win * 0.9
            }
            "dp" => {
                #[expect(
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    reason = "a small count"
                )]
                let k = ctx.goal.gap().ceil().max(0.0) as u32;
                if k == 0 || k > AGENDAS {
                    return None;
                }
                at_least_of(AGENDAS, LAW_SHARE, k)
            }
            _ => return None,
        };
        Some(Plan {
            objective: ctx.goal.alias.clone(),
            points: ctx.goal.points,
            probability,
            cost: Cost::default(),
            steer: Steer::default(),
            reason: format!("{}: votes in the agenda phase", ctx.goal.alias),
            payment: false,
            secret: true,
        })
    }
}

/// The held agenda secrets as goals when the engine reports no progress (Drive the Debate).
#[must_use]
pub fn event_goals(seen: &SeatObservation<'_>) -> Vec<Goal> {
    super::combat::event_goals_of(seen, &["dtd"])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn odds_of_agendas() {
        // At least one of two elections at 45%: 1 - 0.55^2.
        assert!((at_least_of(2, 0.45, 1) - (1.0 - 0.55 * 0.55)).abs() < 1e-9);
        assert!((at_least_of(2, 0.38, 2) - 0.38 * 0.38).abs() < 1e-9);
        assert!(at_least_of(2, 0.38, 3).abs() < 1e-12);
    }

    #[test]
    fn covers_exactly_the_documented_objectives() {
        assert_eq!(
            crate::backward::coverage::matched(&Agenda),
            crate::backward::coverage::sorted(COVERS)
        );
    }
}
