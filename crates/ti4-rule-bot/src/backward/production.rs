//! Backward planning for the secrets that ask for a production feat: five dreadnoughts (Gather a
//! Mighty Fleet), mechs on four distinct planets (Mechanize the Military) and a system with a
//! production capacity of eight (Produce en Masse).
//!
//! The feats are reached by one production at a space dock (activate the dock's system, build) or,
//! for the capacity, by more docks from Construction. The planner counts what the seat's docks can
//! produce in one activation (capacity, resources, a dock system without its own command token),
//! and for Produce en Masse the docks Construction could add to the system it already holds the
//! most production in.
//!
//! # Objectives (exactly; checked by `covers_exactly_the_documented_objectives`)
//!
//! | alias | name | kind | points | family |
//! |---|---|---|---|---|
//! | `gamf` | Gather a Mighty Fleet | secret | 1 | `units_dreadnought` |
//! | `mtm` | Mechanize the Military | secret | 1 | `mechs_on_distinct_planets` |
//! | `pem` | Produce en Masse | secret | 1 | `production_in_one_system` |

use ti4_engine::choice::{Observed, SeatObservation};
use ti4_model::id::{PlayerId, SystemId};

use super::{Cost, ObjectivePlanner, Plan, PlanCtx, Steer, structures};

/// The family tokens this group plans.
pub const IDS: &[&str] = &[
    "units_dreadnought",
    "mechs_on_distinct_planets",
    "production_in_one_system",
];

/// The aliases this group plans, as documented at the top of the file.
pub const COVERS: &[&str] = &["gamf", "mtm", "pem"];

/// Probability that the production at the dock builds what it was activated for.
pub const BUILD_P: f64 = 0.8;
/// The same for mechs, which the production places on planets the seat does not choose.
pub const MECH_P: f64 = 0.6;
/// Probability that a Construction placement lands where the capacity is wanted.
pub const DOCK_P: f64 = 0.7;

/// One system where the seat produces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dock {
    pub system: SystemId,
    /// Production capacity of the system.
    pub capacity: i64,
    /// Controlled planets of the system without a mech of the seat.
    pub mechless: usize,
    /// Resources of the controlled planets without a dock, best first.
    pub dockless: Vec<i64>,
    /// The system holds the seat's command token (it cannot be activated).
    pub tokened: bool,
}

/// How many units a production can make and what they cost: `None` when `count` does not fit.
#[must_use]
pub fn can_build(dock: &Dock, count: usize, unit_cost: i64, resources: i64) -> bool {
    let n = i64::try_from(count).unwrap_or(i64::MAX);
    !dock.tokened && n <= dock.capacity && n.saturating_mul(unit_cost) <= resources
}

/// The best capacity reachable by putting docks on `dock`'s dockless planets: `place` of them.
#[must_use]
pub fn capacity_with(dock: &Dock, place: usize, dock_production: impl Fn(i64) -> i64) -> i64 {
    dock.capacity
        + dock
            .dockless
            .iter()
            .take(place)
            .map(|&r| dock_production(r))
            .sum::<i64>()
}

fn docks_of(seen: &Observed<'_>, me: &PlayerId) -> Vec<Dock> {
    let tokened = seen.systems_with_token(me);
    let mut out = Vec::new();
    for (system, record) in seen.board() {
        let mut capacity = 0i64;
        let mut has_production = false;
        let mut dockless = Vec::new();
        let mut mechless = 0usize;
        for (planet, owner) in &record.planet_control {
            if owner != me {
                continue;
            }
            let resources =
                ti4_content::galaxy::planet(seen.content(), planet.as_str(), seen.sources())
                    .map_or(0, |p| p.resources());
            let units = record
                .planet_units
                .get(planet)
                .map_or(&[][..], Vec::as_slice);
            let mut dock_here = false;
            let mut mech_here = false;
            for unit in units.iter().filter(|u| &u.owner == me) {
                let Some(kind) = crate::unit_type(seen, unit.type_id.as_str()) else {
                    continue;
                };
                if kind.has_production() {
                    capacity += kind.production(resources);
                    has_production = true;
                    dock_here = true;
                }
                mech_here |= kind.base_type() == "mech";
            }
            if !dock_here {
                dockless.push(resources);
            }
            if !mech_here {
                mechless += 1;
            }
        }
        if !has_production {
            continue;
        }
        dockless.sort_by(|a, b| b.cmp(a));
        out.push(Dock {
            system: system.clone(),
            capacity,
            mechless,
            dockless,
            tokened: tokened.contains(system),
        });
    }
    out.sort_by(|a, b| b.capacity.cmp(&a.capacity).then(a.system.cmp(&b.system)));
    out
}

fn unit_cost(seen: &Observed<'_>, base: &str) -> i64 {
    #[expect(clippy::cast_possible_truncation, reason = "printed costs are small")]
    let cost = crate::unit_type(seen, base).map_or(99, |k| k.cost().ceil() as i64);
    cost
}

fn missing(ctx: &PlanCtx) -> usize {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a small count"
    )]
    let g = ctx.goal.gap().ceil().max(0.0) as usize;
    g
}

/// The production group.
pub struct Production;

impl ObjectivePlanner for Production {
    fn objective_ids(&self) -> &[&str] {
        IDS
    }

    fn plan(&self, obs: &SeatObservation<'_>, ctx: &PlanCtx) -> Option<Plan> {
        let observed = obs.observed();
        let g = missing(ctx);
        if g == 0 {
            return None;
        }
        let docks = docks_of(observed, &ctx.me);
        let alias = ctx.goal.alias.clone();
        let one_production = |dock: &Dock, base: &str, builds: Vec<String>, p: f64| {
            let cost = unit_cost(observed, base);
            #[expect(clippy::cast_possible_wrap, reason = "a handful of units")]
            let spend = cost * g as i64;
            Plan {
                objective: alias.clone(),
                points: ctx.goal.points,
                probability: p,
                cost: Cost {
                    tokens: 1,
                    resources: spend,
                    activations: 1,
                },
                steer: Steer {
                    systems: vec![dock.system.clone()],
                    builds,
                    ..Steer::default()
                },
                reason: format!("{alias}: build {g} {base} at {}", dock.system),
                payment: false,
                secret: ctx.goal.secret,
            }
        };
        match ctx.goal.family.as_str() {
            "units_dreadnought" if ctx.tactic_tokens >= 1 => {
                let cost = unit_cost(observed, "dreadnought");
                let dock = docks
                    .iter()
                    .find(|d| can_build(d, g, cost, ctx.resources))?;
                Some(one_production(
                    dock,
                    "dreadnought",
                    vec!["dreadnought".to_owned(); g],
                    BUILD_P,
                ))
            }
            "mechs_on_distinct_planets" if ctx.tactic_tokens >= 1 => {
                let cost = unit_cost(observed, "mech");
                let dock = docks
                    .iter()
                    .find(|d| d.mechless >= g && can_build(d, g, cost, ctx.resources))?;
                Some(one_production(
                    dock,
                    "mech",
                    vec!["mech".to_owned(); g],
                    MECH_P,
                ))
            }
            "production_in_one_system" => {
                let seat = observed.seat(&ctx.me)?;
                let own_open = structures::construction_open(observed, &seat);
                let rivals = observed
                    .players()
                    .into_iter()
                    .filter(|p| **p != ctx.me)
                    .filter_map(|p| observed.seat(p))
                    .filter(|s| !s.passed && structures::construction_open(observed, s))
                    .count();
                let sources = structures::placements(own_open, rivals, seat.strategic_tokens);
                let dock_value =
                    |r: i64| crate::unit_type(observed, "spacedock").map_or(0, |k| k.production(r));
                let (place, best) = docks.iter().find_map(|d| {
                    (1..=sources.len().min(d.dockless.len()))
                        .find(|&k| {
                            capacity_with(d, k, dock_value)
                                >= i64::try_from(ctx.goal.threshold.max(0.0) as u64)
                                    .unwrap_or(i64::MAX)
                        })
                        .map(|k| (k, d))
                })?;
                let (p, tokens) = structures::route_of(&sources, place)?;
                Some(Plan {
                    objective: alias.clone(),
                    points: ctx.goal.points,
                    probability: p * DOCK_P,
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
                    reason: format!("{alias}: {place} more dock(s) in {}", best.system),
                    payment: false,
                    secret: ctx.goal.secret,
                })
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dock(capacity: i64, mechless: usize, dockless: &[i64], tokened: bool) -> Dock {
        Dock {
            system: SystemId::new("1"),
            capacity,
            mechless,
            dockless: dockless.to_vec(),
            tokened,
        }
    }

    #[test]
    fn a_production_builds_within_capacity_resources_and_a_free_token() {
        let d = dock(6, 2, &[], false);
        assert!(can_build(&d, 2, 4, 8));
        assert!(!can_build(&d, 2, 4, 7));
        assert!(!can_build(&d, 7, 1, 20));
        assert!(!can_build(&dock(6, 2, &[], true), 1, 1, 9));
    }

    #[test]
    fn extra_docks_add_their_production_best_planet_first() {
        let d = dock(5, 1, &[3, 1], false);
        let dock_production = |r: i64| r + 2;
        assert_eq!(capacity_with(&d, 0, dock_production), 5);
        assert_eq!(capacity_with(&d, 1, dock_production), 10);
        assert_eq!(capacity_with(&d, 2, dock_production), 13);
    }

    #[test]
    fn covers_exactly_the_documented_objectives() {
        assert_eq!(
            crate::backward::coverage::matched(&Production),
            crate::backward::coverage::sorted(COVERS)
        );
    }

    use ti4_content::ContentStore;
    use ti4_engine::choice::{Choice, ChoiceOption, Decider, IllegalChoice};
    use ti4_model::POK;
    use ti4_model::state::Phase;

    struct Probe(Vec<Plan>);
    impl Decider for Probe {
        fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
            Ok(choice.options[0].clone())
        }
        fn choose_seeing(
            &mut self,
            choice: &Choice,
            seen: &SeatObservation<'_>,
        ) -> Result<ChoiceOption, IllegalChoice> {
            let me = PlayerId::new("a");
            let mut ctx = PlanCtx::new(seen, &me);
            for goal in ctx.goals.clone() {
                ctx.goal = goal;
                if let Some(plan) = Production.plan(seen, &ctx) {
                    self.0.push(plan);
                }
            }
            Ok(choice.options[0].clone())
        }
    }

    fn plans(goods: i32, secret: &str) -> Vec<Plan> {
        let mut state = ti4_engine::fixtures::seated_game(&[("a", "sol"), ("b", "letnev")], POK);
        state.round = 3;
        state.phase = Phase::Action;
        {
            let seat = state.player_mut(&PlayerId::new("a")).expect("seat");
            seat.trade_goods = goods;
            seat.tactic_tokens = 3;
            seat.secret_objectives = vec![ti4_model::id::SecretObjectiveId::new(secret)];
        }
        let mut probe = Probe(Vec::new());
        let choice = Choice::new(
            PlayerId::new("a"),
            "x",
            vec![ChoiceOption::labelled("o", "o", "o")],
        );
        ti4_engine::choice::ask_private(
            &choice,
            &state,
            ContentStore::embedded(),
            POK,
            None,
            &mut probe,
        )
        .expect("legal");
        probe.0
    }

    #[test]
    fn five_dreadnoughts_need_the_resources_and_the_dock() {
        // The Sol home dock produces 6 or more: five dreadnoughts (20) are within it with goods.
        let rich = plans(30, "gamf");
        assert_eq!(rich.len(), 1);
        assert_eq!(rich[0].steer.builds.len(), 5);
        assert!(rich[0].cost.resources >= 20 && rich[0].secret);
        // Too poor for twenty resources: no plan, the default logic plays.
        assert!(plans(0, "gamf").is_empty());
        // A secret of another group is not planned here.
        assert!(plans(30, "pe").is_empty());
    }
}
