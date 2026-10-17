//! Backward planning for Turn Their Fleets to Dust: space cannon offense destroys the last of a
//! player's non-fighter ships in the active system.
//!
//! The seat's space cannon fires at ships that move into a system it covers, so the feat needs
//! (1) a PDS covering a system a rival will activate and (2) a rival fleet small enough to die to
//! it. Rivals go to Mecatol Rex (the custodians token, Imperial's point), so the seat puts a PDS
//! on Mecatol Rex when it controls it, or, with PDS II (Deep Space Cannon: space cannon against
//! ships in adjacent systems; Hel-Titan II for the Titans), on a planet in a system next to it.
//! Both parts are partly the rivals' doing, so the plan's odds are low and are only the product
//! of the structure being placed (Construction: own primary, a rival's secondary) and the
//! chance that a rival activates Mecatol Rex with a fleet the cannon wipes out. The effective
//! steer is the placement hook ([`placement`], `objectives::pick_structure`): whenever a
//! structure is placed, a PDS goes to Mecatol Rex (or beside it) first.
//!
//! # Objectives (exactly; checked by `covers_exactly_the_documented_objectives`)
//!
//! | alias | name | kind | points | family |
//! |---|---|---|---|---|
//! | `ttfd` | Turn Their Fleets to Dust | secret | 1 | none (event) |

use ti4_engine::choice::{Choice, Observed, SeatObservation};
use ti4_model::id::{PlayerId, SystemId};

use super::{Cost, ObjectivePlanner, Plan, PlanCtx, Steer, structures};
use crate::tuning;

/// The secrets this group plans.
pub const IDS: &[&str] = &["ttfd"];
/// The aliases this group plans, as documented at the top of the file.
pub const COVERS: &[&str] = &["ttfd"];

/// A rival activates Mecatol Rex (or the system the cannon reaches) this round.
pub const RIVAL_COMES: f64 = 0.3;
/// The visiting fleet is small enough for one or two cannon dice to destroy it.
pub const FLEET_DIES: f64 = 0.15;

/// The held event secret as a goal.
#[must_use]
pub fn event_goals(seen: &SeatObservation<'_>) -> Vec<crate::objectives::Goal> {
    super::combat::event_goals_of(seen, IDS)
}

/// Whether the seat owns the technology that gives its PDS the Deep Space Cannon.
fn deep_cannon(seen: &Observed<'_>, me: &PlayerId) -> bool {
    seen.seat(me).is_some_and(|seat| {
        seat.technologies
            .iter()
            .any(|t| matches!(t.as_str(), "pds2" | "ht2"))
    })
}

/// Worth of putting a PDS on `planet` of `system`: 3 on Mecatol Rex, 2 beside it with the Deep
/// Space Cannon, 0 elsewhere.
#[must_use]
pub fn placement_score(
    system: &str,
    planet: &str,
    mecatol: &str,
    beside: &[String],
    deep: bool,
) -> f64 {
    if system == mecatol && planet == crate::MECATOL {
        3.0
    } else if deep && beside.iter().any(|s| s == system) {
        2.0
    } else {
        0.0
    }
}

fn beside_mecatol(seen: &Observed<'_>, mecatol: &SystemId) -> Vec<String> {
    seen.galaxy().map_or_else(Vec::new, |galaxy| {
        galaxy
            .adjacent(mecatol.as_str())
            .into_iter()
            .map(ToOwned::to_owned)
            .collect()
    })
}

/// Whether a PDS of the seat already covers Mecatol Rex.
fn covered(seen: &Observed<'_>, me: &PlayerId, mecatol: &SystemId) -> bool {
    let deep = deep_cannon(seen, me);
    let beside = beside_mecatol(seen, mecatol);
    seen.board().iter().any(|(system, record)| {
        (system == mecatol || (deep && beside.iter().any(|s| s == system.as_str())))
            && record.planet_units.values().flatten().any(|u| {
                &u.owner == me
                    && crate::unit_type(seen, u.type_id.as_str())
                        .is_some_and(|k| k.has_space_cannon())
            })
    })
}

/// A structure placement (`kind|system|planet`) for Turn Their Fleets to Dust: the PDS that
/// covers Mecatol Rex, when the seat holds the secret and nothing covers it yet.
#[must_use]
pub fn placement(choice: &Choice, seen: &SeatObservation<'_>, me: &PlayerId) -> Option<usize> {
    if !tuning::bw_enabled() || !tuning::bw_ttfd() {
        return None;
    }
    if !seen.held_secrets().iter().any(|s| s.as_str() == "ttfd") {
        return None;
    }
    let observed = seen.observed();
    let mecatol = crate::mecatol_system(observed)?;
    if covered(observed, me, &mecatol) {
        return None;
    }
    let beside = beside_mecatol(observed, &mecatol);
    let deep = deep_cannon(observed, me);
    choice
        .options
        .iter()
        .enumerate()
        .filter_map(|(i, o)| {
            let mut parts = o.id.split('|');
            let (kind, system, planet) = (parts.next()?, parts.next()?, parts.next()?);
            (kind == "pds").then_some(())?;
            let score = placement_score(system, planet, mecatol.as_str(), &beside, deep);
            (score > 0.0).then_some((i, score))
        })
        .max_by(|a, b| a.1.total_cmp(&b.1).then(b.0.cmp(&a.0)))
        .map(|(i, _)| i)
}

/// The Turn Their Fleets to Dust group.
pub struct Ttfd;

impl ObjectivePlanner for Ttfd {
    fn objective_ids(&self) -> &[&str] {
        IDS
    }

    fn plan(&self, obs: &SeatObservation<'_>, ctx: &PlanCtx) -> Option<Plan> {
        let observed = obs.observed();
        let mecatol = ctx.mecatol.system.clone()?;
        if covered(observed, &ctx.me, &mecatol) {
            return None;
        }
        // A planet to put the PDS on: Mecatol Rex itself, or beside it with the Deep Space Cannon.
        if !ctx.mecatol.held && !deep_cannon(observed, &ctx.me) {
            return None;
        }
        let seat = observed.seat(&ctx.me)?;
        let own_open = seat.strategy_cards.iter().any(|c| {
            !seat.exhausted_strategy_cards.contains(c)
                && ti4_engine::strategy_cards::card_name(observed.content(), c.as_str()).as_deref()
                    == Some("Construction")
        });
        let sources = structures::placements(own_open, 0, seat.strategic_tokens);
        let (build, tokens) = structures::route_of(&sources, 1)?;
        Some(Plan {
            objective: ctx.goal.alias.clone(),
            points: ctx.goal.points,
            probability: build * RIVAL_COMES * FLEET_DIES,
            cost: Cost {
                tokens,
                resources: 0,
                activations: 0,
            },
            steer: Steer {
                cards: if own_open {
                    vec!["Construction".to_owned()]
                } else {
                    Vec::new()
                },
                ..Steer::default()
            },
            reason: format!("ttfd: a PDS covering {mecatol}"),
            payment: false,
            secret: true,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ti4_engine::choice::ChoiceOption;

    #[test]
    fn a_pds_on_mecatol_beats_one_beside_it_which_needs_the_deep_cannon() {
        let beside = vec!["20".to_owned(), "21".to_owned()];
        assert!((placement_score("18", "mr", "18", &beside, false) - 3.0).abs() < 1e-9);
        assert!((placement_score("20", "x", "18", &beside, true) - 2.0).abs() < 1e-9);
        assert!(placement_score("20", "x", "18", &beside, false).abs() < 1e-9);
        assert!(placement_score("30", "y", "18", &beside, true).abs() < 1e-9);
    }

    #[test]
    fn only_pds_options_are_considered() {
        // `placement` reads kind|system|planet; a dock on Mecatol Rex is no cannon.
        let dock = ChoiceOption::labelled("spacedock|18|mr", "structure", "dock");
        assert_eq!(dock.id.split('|').next(), Some("spacedock"));
    }

    #[test]
    fn covers_exactly_the_documented_objectives() {
        assert_eq!(
            crate::backward::coverage::matched(&Ttfd),
            crate::backward::coverage::sorted(COVERS)
        );
    }

    use ti4_content::ContentStore;
    use ti4_engine::choice::{Decider, IllegalChoice};
    use ti4_engine::fixtures::{game, hub_with_centre, put_on_planet};
    use ti4_model::POK;
    use ti4_model::id::{PlanetId, SecretObjectiveId};

    struct PlaceProbe(Option<usize>);
    impl Decider for PlaceProbe {
        fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
            Ok(choice.options[0].clone())
        }
        fn choose_seeing(
            &mut self,
            choice: &Choice,
            seen: &SeatObservation<'_>,
        ) -> Result<ChoiceOption, IllegalChoice> {
            self.0 = placement(choice, seen, &PlayerId::new("a"));
            Ok(choice.options[0].clone())
        }
    }

    #[test]
    fn a_pds_goes_to_mecatol_rex_until_one_covers_it() {
        let hub = hub_with_centre("18");
        let me = PlayerId::new("a");
        let mut state = game(&["a", "b"]);
        state.round = 3;
        state.player_mut(&me).expect("seat").secret_objectives =
            vec![SecretObjectiveId::new("ttfd")];
        let centre = SystemId::new(hub.centre.clone());
        state
            .system_mut(&centre)
            .planet_control
            .insert(PlanetId::new("mr"), me.clone());
        let outer = hub.outer[0].clone();
        let choice = Choice::new(
            me.clone(),
            "place a structure",
            vec![
                ChoiceOption::labelled(format!("pds|{outer}|x"), "structure", "pds elsewhere"),
                ChoiceOption::labelled("spacedock|18|mr", "structure", "dock on mecatol"),
                ChoiceOption::labelled("pds|18|mr", "structure", "pds on mecatol"),
            ],
        );
        let pick = |state: &ti4_model::state::GameState, on: bool| {
            let mut knobs = crate::tuning::Tuning::default();
            knobs.bw_enabled = on;
            knobs.bw_ttfd = on;
            let mut probe = PlaceProbe(None);
            crate::tuning::with(knobs, || {
                ti4_engine::choice::ask_private(
                    &choice,
                    state,
                    ContentStore::embedded(),
                    POK,
                    Some(&hub.galaxy),
                    &mut probe,
                )
                .expect("legal");
            });
            probe.0
        };
        assert_eq!(pick(&state, true), Some(2));
        assert_eq!(pick(&state, false), None);
        // A PDS already there: nothing more to steer.
        put_on_planet(&mut state, &centre, &PlanetId::new("mr"), "pds", &me, 1);
        assert_eq!(pick(&state, true), None);
    }
}
