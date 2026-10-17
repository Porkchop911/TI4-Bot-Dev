//! Backward planning (bot version 4, experimental; every knob `bw_*` in [`crate::tuning`], default off).
//!
//! From round [`tuning::bw_round`] on, the seat looks at the objectives it can still score (the
//! revealed publics and, where cheap, its held secrets) and plans its turn *backwards* from
//! each: what must be true, what is the cheapest route to it this turn (activations, which
//! planets or systems, which fleet), how likely is that route to work, and what does it cost.
//! One [`ObjectivePlanner`] per objective group answers that for the goals it knows, each in its
//! own source file (see [`registry`]):
//!
//! | file | goals |
//! |---|---|
//! | [`territory`] | control N planets, trait / specialty / outside-home / rim / attached planets |
//! | [`fleet`] | ships in N systems, planetless systems, next to Mecatol Rex / an anomaly / a rival home, fleet size |
//! | `payment` (Agent B) | spend resources, influence, trade goods, tokens |
//! | `technology` (Agent B) | techs, colours, unit upgrades |
//! | `combat` (Agent B) | kills, rival systems, everything else |
//! | [`mecatol`] | Mecatol Rex: the custodians' point and Imperial's primary point |
//!
//! [`verdict`] collects the plans, weighs them ([`weigh`]) and returns a [`Verdict`]:
//!
//! - **expected VP** of a plan = probability this turn x marginal points - the tokens and
//!   resources it costs; a public objective beyond the one the seat could already score this
//!   round only counts a quarter unless Imperial's extra scoring is in play (as
//!   [`crate::objectives::gain`]);
//! - **Imperial**: when at least [`tuning::bw_likely`]-probable plans for two public objectives
//!   exist, the Imperial card is wanted (it scores a second public objective) and each such plan
//!   gains [`tuning::bw_imperial_bonus`];
//! - **payment objectives** ("spend N ...") are discounted by [`tuning::bw_payment_discount`]:
//!   they are usually scoreable whatever the board looks like and need no steering;
//! - the Mecatol Rex / Imperial plan competes as one more candidate;
//! - **fallback**: when the best plan's expected VP is below [`tuning::bw_threshold`] the verdict
//!   is empty and every default rule plays unchanged.
//!
//! The chosen plans steer the *activation* (a system in a plan's route gets
//! `bw_steer x expected VP` extra worth, [`Verdict::bonus`]) and the strategy-card choice
//! (Imperial). Nothing here ever picks an option that is not offered.

use std::collections::BTreeMap;

use ti4_engine::choice::{Observed, SeatObservation};
use ti4_engine::fleet_strength::matchup;
use ti4_engine::objectives::Imagined;
use ti4_model::id::{PlanetId, PlayerId, SystemId};

use crate::objectives::{self, Goal};
use crate::tuning;

pub mod abilities;
pub mod agenda;
pub mod combat;
#[cfg(test)]
mod coverage;
pub mod fleet;
pub mod fleet_shape;
pub mod holdings;
pub mod mecatol;
pub mod mecatol_two_step;
pub mod payment;
pub mod prep;
pub mod production;
pub mod structures;
pub mod technology;
pub mod territory;
pub mod ttfd;
pub mod unsteerable;

/// What a plan costs this turn.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Cost {
    /// Command tokens (tactic tokens for activations, others spent).
    pub tokens: i32,
    /// Resources spent (production, payments).
    pub resources: i64,
    /// Tactical activations.
    pub activations: u32,
}

/// What the plan wants the seat to do.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Steer {
    /// Systems to activate (and move or land in).
    pub systems: Vec<SystemId>,
    /// Unit type ids worth producing for it.
    pub builds: Vec<String>,
    /// Strategy card names worth taking or playing (e.g. "Imperial", "Technology").
    pub cards: Vec<String>,
    /// Strategy cards to play at the next opportunity (the prep step of a two-step plan).
    pub play: Vec<String>,
    /// Technologies to pick when a research is offered (the prep of a technology step).
    pub techs: Vec<String>,
    /// Systems whose invasion piles every ground force on one planet.
    pub pile: Vec<SystemId>,
    /// Systems where ships and their cargo gather for the next activation (the movement hook
    /// moves everything there).
    pub gather: Vec<SystemId>,
}

/// One plan: the cheapest route found to score one objective, and how it weighs.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    /// The objective alias (or `"mecatol"` for the Mecatol / Imperial plan).
    pub objective: String,
    /// Victory points it is worth.
    pub points: f64,
    /// Probability that the steer is carried out and the objective is met by the end of this
    /// round's action phase.
    pub probability: f64,
    /// What it costs.
    pub cost: Cost,
    /// What to do.
    pub steer: Steer,
    /// For logs and tests.
    pub reason: String,
    /// A "spend N ..." objective (discounted in [`weigh`]).
    pub payment: bool,
    /// A held secret rather than a revealed public.
    pub secret: bool,
}

/// One activation the seat could make this turn, as far as an objective cares.
#[derive(Debug, Clone, PartialEq)]
pub struct Step {
    pub system: SystemId,
    /// Planets the activation would take (greedy, as the activation value counts them).
    pub planets: Vec<PlanetId>,
    /// A ship of the seat would be in the system afterwards where none is now.
    pub ship: bool,
    /// Whether a ship could come from a system that keeps another ship.
    pub spare_ship: bool,
    /// Probability that the step does what it says (fight won, landing succeeds, ship comes).
    pub success: f64,
    /// The prep a prepared step starts with ([`prep`]); `None` for a plain activation.
    pub prep: Option<prep::PrepInfo>,
}

/// Mecatol Rex as the planners see it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MecatolState {
    pub system: Option<SystemId>,
    pub held: bool,
    pub custodians_removed: bool,
    pub takeable: bool,
    pub in_reach: bool,
    pub rival_holds: bool,
}

/// Everything a planner needs about the position and the seat's means this turn.
pub struct PlanCtx {
    pub me: PlayerId,
    pub round: u32,
    /// The goal being planned (set by the dispatcher before each call of
    /// [`ObjectivePlanner::plan`]; planners plan exactly this goal).
    pub goal: Goal,
    /// Every goal of the seat (publics, secrets), unmet and met.
    pub goals: Vec<Goal>,
    pub tactic_tokens: i32,
    pub strategic_tokens: i32,
    pub fleet_tokens: i32,
    pub resources: i64,
    pub influence: i64,
    pub trade_goods: i64,
    pub mecatol: MecatolState,
    /// Every activation worth considering (own-token systems and unwinnable fights left out).
    pub steps: Vec<Step>,
}

/// The cheapest set of steps found for a goal.
#[derive(Debug, Clone, PartialEq)]
pub struct Route {
    /// Indices into [`PlanCtx::steps`].
    pub steps: Vec<usize>,
    /// Product of the steps' success, with a small discount per extra step.
    pub probability: f64,
}

/// A planner for one objective group.
pub trait ObjectivePlanner {
    /// The objective aliases or family tokens (see `ti4_engine::objectives::family_token`) it
    /// plans. The dispatcher calls [`Self::plan`] once for each of the seat's goals that matches.
    fn objective_ids(&self) -> &[&str];
    /// The plan for `ctx.goal`, or `None` when the group has nothing to propose (met already,
    /// out of reach, no way to steer).
    fn plan(&self, obs: &SeatObservation<'_>, ctx: &PlanCtx) -> Option<Plan>;
}

/// Every planner. Agent B registers its groups here, one line each.
#[must_use]
pub fn registry() -> Vec<Box<dyn ObjectivePlanner>> {
    let mut out: Vec<Box<dyn ObjectivePlanner>> = Vec::new();
    if tuning::bw_territory() {
        out.push(Box::new(territory::Territory));
    }
    if tuning::bw_fleet() {
        out.push(Box::new(fleet::Fleet));
    }
    if tuning::bw_fleet_shape() {
        out.push(Box::new(fleet_shape::FleetShape));
    }
    if tuning::bw_payment() {
        out.push(Box::new(payment::Payment));
    }
    if tuning::bw_technology() {
        out.push(Box::new(technology::Technology));
    }
    if tuning::bw_combat() {
        out.push(Box::new(combat::Combat));
        out.push(Box::new(structures::Structures));
    }
    if tuning::bw_combat_abilities() {
        out.push(Box::new(abilities::Abilities));
    }
    if tuning::bw_agenda() {
        out.push(Box::new(agenda::Agenda));
    }
    if tuning::bw_holdings() {
        out.push(Box::new(holdings::Holdings));
    }
    if tuning::bw_ttfd() {
        out.push(Box::new(ttfd::Ttfd));
    }
    if tuning::bw_production() {
        out.push(Box::new(production::Production));
    }
    out
}

impl Goal {
    /// Whether `ids` names this goal by alias or by family token.
    #[must_use]
    pub fn matches_any(&self, ids: &[&str]) -> bool {
        ids.iter().any(|id| *id == self.alias || *id == self.family)
    }
}

impl PlanCtx {
    /// Build the context for the seat.
    #[must_use]
    pub fn new(seen: &SeatObservation<'_>, me: &PlayerId) -> Self {
        let observed = seen.observed();
        let seat = observed.seat(me);
        let mecatol = mecatol::state(observed, me);
        let mut ctx = Self {
            goals: {
                let mut goals = objectives::goals(seen, me, &Imagined::NONE);
                // Event secrets have no progress in the engine; the combat group lists them.
                if tuning::bw_combat() {
                    goals.extend(combat::event_goals(seen));
                }
                if tuning::bw_combat_abilities() {
                    goals.extend(abilities::event_goals(seen));
                }
                if tuning::bw_agenda() {
                    goals.extend(agenda::event_goals(seen));
                }
                if tuning::bw_ttfd() {
                    goals.extend(ttfd::event_goals(seen));
                }
                goals
            },
            tactic_tokens: seat.as_ref().map_or(0, |s| s.tactic_tokens),
            strategic_tokens: seat.as_ref().map_or(0, |s| s.strategic_tokens),
            fleet_tokens: seat.as_ref().map_or(0, |s| s.fleet_tokens),
            resources: observed.available_spend(me, ti4_engine::production::Spend::Resources),
            influence: observed.available_spend(me, ti4_engine::production::Spend::Influence),
            trade_goods: objectives::trade_goods(observed, me),
            mecatol,
            steps: steps(seen, me),
            round: observed.round(),
            ..Self::blank(me)
        };
        let extra = prep::prepared(observed, me, &ctx);
        ctx.steps.extend(extra);
        ctx
    }

    /// A context with nothing in it (tests, and the base of [`Self::new`]).
    #[must_use]
    pub fn blank(me: &PlayerId) -> Self {
        Self {
            me: me.clone(),
            round: 3,
            goal: Goal {
                alias: String::new(),
                secret: false,
                points: 0.0,
                bought: false,
                have: 0.0,
                threshold: 1.0,
                satisfied: false,
                family: String::new(),
            },
            goals: Vec::new(),
            tactic_tokens: 3,
            strategic_tokens: 2,
            fleet_tokens: 3,
            resources: 0,
            influence: 0,
            trade_goods: 0,
            mecatol: MecatolState::default(),
            steps: Vec::new(),
        }
    }

    /// Whether the seat still holds a public objective it could score this round already.
    #[must_use]
    pub fn public_ready(&self) -> f64 {
        self.goals
            .iter()
            .filter(|g| !g.secret && g.satisfied)
            .map(|g| g.points)
            .fold(0.0, f64::max)
    }

    /// The cheapest route (fewest steps, then best probability) of at most `max_steps` of the
    /// steps `useful` accepts that makes the goal `alias` met, judged by the engine's own
    /// requirement functions on the imagined position.
    pub fn route(
        &self,
        seen: &SeatObservation<'_>,
        alias: &str,
        max_steps: usize,
        useful: impl Fn(&Step) -> bool,
    ) -> Option<Route> {
        let max_steps = max_steps
            .min(usize::try_from(self.tactic_tokens).unwrap_or(0))
            .min(3);
        if max_steps == 0 {
            return None;
        }
        let mut plain: Vec<usize> = (0..self.steps.len())
            .filter(|&i| self.steps[i].prep.is_none() && useful(&self.steps[i]))
            .collect();
        let mut prepared: Vec<usize> = (0..self.steps.len())
            .filter(|&i| self.steps[i].prep.is_some() && useful(&self.steps[i]))
            .collect();
        for list in [&mut plain, &mut prepared] {
            list.sort_by(|&a, &b| {
                self.steps[b]
                    .success
                    .total_cmp(&self.steps[a].success)
                    .then(a.cmp(&b))
            });
        }
        plain.truncate(6);
        prepared.truncate(3);
        let pool: Vec<usize> = plain.into_iter().chain(prepared).collect();
        for size in 1..=max_steps.min(pool.len()) {
            let mut best: Option<Route> = None;
            for combo in combinations(&pool, size) {
                if !self.affordable(&combo) {
                    continue;
                }
                if self.meets(seen, alias, &combo) {
                    let p = combo
                        .iter()
                        .map(|&i| self.steps[i].success)
                        .product::<f64>()
                        * 0.9_f64.powi(i32::try_from(size).unwrap_or(1) - 1);
                    if best.as_ref().is_none_or(|b| p > b.probability) {
                        best = Some(Route {
                            steps: combo,
                            probability: p,
                        });
                    }
                }
            }
            if best.is_some() {
                return best;
            }
        }
        None
    }

    /// Whether the steps `combo` fit together: one prepared step at most, no system twice, and
    /// all activations (preps included) within the tactic tokens, strategy tokens and resources.
    #[must_use]
    pub fn affordable(&self, combo: &[usize]) -> bool {
        let steps: Vec<&Step> = combo.iter().map(|&i| &self.steps[i]).collect();
        if steps.iter().filter(|s| s.prep.is_some()).count() > 1 {
            return false;
        }
        let systems: std::collections::BTreeSet<&SystemId> =
            steps.iter().map(|s| &s.system).collect();
        if systems.len() != steps.len() {
            return false;
        }
        let prep = |f: fn(&prep::PrepInfo) -> i64| -> i64 {
            steps.iter().filter_map(|s| s.prep.as_ref()).map(f).sum()
        };
        let tokens = i64::try_from(steps.len()).unwrap_or(0) + prep(|p| i64::from(p.tokens));
        tokens <= i64::from(self.tactic_tokens)
            && tokens <= 3
            && prep(|p| i64::from(p.strategic)) <= i64::from(self.strategic_tokens)
            && prep(|p| p.resources) <= self.resources
    }

    /// Whether goal `alias` is met after the steps `combo` all happened.
    #[must_use]
    pub fn meets(&self, seen: &SeatObservation<'_>, alias: &str, combo: &[usize]) -> bool {
        let planets: Vec<PlanetId> = combo
            .iter()
            .flat_map(|&i| self.steps[i].planets.iter().cloned())
            .collect();
        let systems: Vec<String> = combo
            .iter()
            .filter(|&&i| self.steps[i].ship)
            .map(|&i| self.steps[i].system.to_string())
            .collect();
        objectives::goals(
            seen,
            &self.me,
            &Imagined {
                planets: &planets,
                systems: &systems,
                ..Imagined::default()
            },
        )
        .iter()
        .any(|g| g.alias == alias && g.satisfied)
    }

    /// The route as a plan skeleton: cost, steer and the steps' probability.
    #[must_use]
    pub fn plan_of(&self, route: &Route, reason: String) -> Plan {
        let mut steer = Steer::default();
        let mut cost = Cost::default();
        for &i in &route.steps {
            let step = &self.steps[i];
            if let Some(p) = &step.prep {
                steer.systems.extend(p.systems.iter().cloned());
                steer.builds.extend(p.builds.iter().cloned());
                steer.cards.extend(p.cards.iter().cloned());
                steer.play.extend(p.play.iter().cloned());
                steer.gather.extend(p.gather.iter().cloned());
                steer.techs.extend(p.techs.iter().cloned());
                cost.tokens += p.tokens + p.strategic;
                cost.resources += p.resources;
                cost.activations += u32::try_from(p.tokens).unwrap_or(0);
            }
            steer.systems.push(step.system.clone());
            cost.tokens += 1;
            cost.activations += 1;
        }
        let prepared = route
            .steps
            .iter()
            .find_map(|&i| self.steps[i].prep.as_ref().map(|p| p.label.clone()));
        let reason = prepared.map_or(reason.clone(), |label| format!("{label}: {reason}"));
        Plan {
            objective: self.goal.alias.clone(),
            points: self.goal.points,
            probability: route.probability,
            cost,
            steer,
            reason,
            payment: self.goal.bought,
            secret: self.goal.secret,
        }
    }
}

/// All `size`-subsets of `pool`, in index order.
fn combinations(pool: &[usize], size: usize) -> Vec<Vec<usize>> {
    fn go(
        pool: &[usize],
        size: usize,
        from: usize,
        cur: &mut Vec<usize>,
        out: &mut Vec<Vec<usize>>,
    ) {
        if cur.len() == size {
            out.push(cur.clone());
            return;
        }
        for i in from..pool.len() {
            cur.push(pool[i]);
            go(pool, size, i + 1, cur, out);
            cur.pop();
        }
    }
    let mut out = Vec::new();
    go(pool, size, 0, &mut Vec::new(), &mut out);
    out
}

/// Probability that a fleet wins the fight in `system` (1 when nobody is there to fight).
fn fight_probability(seen: &Observed<'_>, me: &PlayerId, system: &SystemId) -> f64 {
    if !crate::has_hostile_ships(seen, me, system) {
        return 1.0;
    }
    let movable = crate::movable_for(seen, me, system);
    let board = seen.system(system);
    let own = crate::fleet(
        seen,
        me,
        board
            .units
            .iter()
            .filter(|u| &u.owner == me && crate::is_ship(seen, u))
            .chain(movable.iter().map(|m| &m.unit)),
    );
    let enemy = crate::hostile_space(seen, me, system);
    1.0 / (1.0 + (-2.0 * matchup(&own, &enemy)).exp())
}

/// Every activation the seat could make this turn that does something, with its odds.
fn steps(seen: &SeatObservation<'_>, me: &PlayerId) -> Vec<Step> {
    let observed = seen.observed();
    let Some(galaxy) = observed.galaxy() else {
        return Vec::new();
    };
    let tokened = observed.systems_with_token(me);
    let mut out = Vec::new();
    for id in galaxy.system_ids() {
        let system = SystemId::new(id);
        if tokened.contains(&system) {
            continue;
        }
        let base = crate::activation_value(observed, me, &system);
        if base < 0.0 {
            continue;
        }
        let (ships, ground) = objectives::reach(observed, me, &system);
        let present = observed
            .system(&system)
            .units
            .iter()
            .any(|u| &u.owner == me && crate::is_ship(observed, u));
        let planets = if ground > 0 {
            objectives::taken_planets(observed, me, &system, ground)
        } else {
            Vec::new()
        };
        let ship = ships && !present;
        if planets.is_empty() && !ship {
            continue;
        }
        let spare_ship = crate::movable_for(observed, me, &system).iter().any(|m| {
            crate::is_ship(observed, &m.unit)
                && observed
                    .system(&m.origin)
                    .units
                    .iter()
                    .filter(|u| &u.owner == me && crate::is_ship(observed, u))
                    .count()
                    >= 2
        });
        let mut success = fight_probability(observed, me, &system);
        if !planets.is_empty() {
            success *= 0.9;
        }
        if ship && !spare_ship {
            success *= 0.6;
        }
        out.push(Step {
            system,
            planets,
            ship,
            spare_ship,
            success,
            prep: None,
        });
    }
    out
}

/// A plan with its weighed expected victory points.
#[derive(Debug, Clone, PartialEq)]
pub struct Weighed {
    pub plan: Plan,
    pub expected: f64,
}

/// The outcome of planning: weighed plans, the ones chosen and what they steer.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Verdict {
    /// Every plan found, best first.
    pub plans: Vec<Weighed>,
    /// Indices into [`Self::plans`] of the plans followed this turn.
    pub chosen: Vec<usize>,
    /// Imperial is wanted (two public objectives look likely, or Mecatol Rex is in play).
    pub imperial: bool,
    /// Nothing reaches the threshold: every default rule plays as before.
    pub fallback: bool,
}

impl Verdict {
    /// Extra activation worth per system from the chosen plans.
    #[must_use]
    pub fn bonus(&self) -> BTreeMap<SystemId, f64> {
        let mut out: BTreeMap<SystemId, f64> = BTreeMap::new();
        if self.fallback {
            return out;
        }
        for &i in &self.chosen {
            let w = &self.plans[i];
            for system in &w.plan.steer.systems {
                *out.entry(system.clone()).or_default() += tuning::bw_steer() * w.expected;
            }
        }
        out
    }
}

impl Verdict {
    /// Whether a chosen plan wants `card` played now.
    #[must_use]
    pub fn plays(&self, card: &str) -> bool {
        !self.fallback
            && self
                .chosen
                .iter()
                .any(|&i| self.plans[i].plan.steer.play.iter().any(|c| c == card))
    }

    /// Unit types the chosen plans want produced.
    #[must_use]
    pub fn builds(&self) -> Vec<String> {
        if self.fallback {
            return Vec::new();
        }
        self.chosen
            .iter()
            .flat_map(|&i| self.plans[i].plan.steer.builds.iter().cloned())
            .collect()
    }

    /// Technologies the chosen plans want researched.
    #[must_use]
    pub fn techs(&self) -> Vec<String> {
        if self.fallback {
            return Vec::new();
        }
        self.chosen
            .iter()
            .flat_map(|&i| self.plans[i].plan.steer.techs.iter().cloned())
            .collect()
    }

    /// Strategy cards the chosen plans want, with the expected VP behind each.
    #[must_use]
    pub fn cards(&self) -> BTreeMap<String, f64> {
        let mut out: BTreeMap<String, f64> = BTreeMap::new();
        if self.fallback {
            return out;
        }
        for &i in &self.chosen {
            for card in &self.plans[i].plan.steer.cards {
                *out.entry(card.clone()).or_default() += self.plans[i].expected;
            }
        }
        out
    }
}

/// Weigh `plans` (see the module docs) and choose.
#[must_use]
pub fn weigh(ctx: &PlanCtx, plans: Vec<Plan>) -> Verdict {
    let likely = tuning::bw_likely();
    let ready = ctx.public_ready();
    // Two public objectives look likely: Imperial scores the second.
    let likely_public = plans
        .iter()
        .filter(|p| !p.secret && p.objective != "mecatol" && p.probability >= likely)
        .count()
        + usize::from(ready > 0.0);
    let imperial = likely_public >= 2;
    let mut weighed: Vec<Weighed> = plans
        .into_iter()
        .map(|plan| {
            let mut value = if plan.objective == "mecatol" {
                plan.points
            } else if !plan.secret && !imperial {
                // One public scoring per round: only what beats the one already in hand.
                (plan.points - ready).max(0.25 * plan.points)
            } else {
                plan.points
            };
            if plan.payment {
                value *= 1.0 - tuning::bw_payment_discount();
            }
            let mut expected = plan.probability * value
                - tuning::bw_token_cost() * f64::from(plan.cost.tokens)
                - tuning::bw_resource_cost() * plan.cost.resources as f64;
            if imperial && !plan.secret && plan.probability >= likely && plan.objective != "mecatol"
            {
                expected += tuning::bw_imperial_bonus();
            }
            Weighed { plan, expected }
        })
        .collect();
    weighed.sort_by(|a, b| {
        b.expected
            .total_cmp(&a.expected)
            .then_with(|| a.plan.objective.cmp(&b.plan.objective))
    });
    let fallback = weighed
        .first()
        .is_none_or(|w| w.expected < tuning::bw_threshold());
    // Follow the best plan and every other that fits the tactic tokens left.
    let mut chosen = Vec::new();
    let mut tokens = ctx.tactic_tokens;
    if !fallback {
        for (i, w) in weighed.iter().enumerate() {
            if w.expected < tuning::bw_threshold() {
                break;
            }
            if w.plan.cost.tokens <= tokens || chosen.is_empty() {
                tokens -= w.plan.cost.tokens;
                chosen.push(i);
            }
        }
    }
    let imperial = !fallback
        && (imperial
            || chosen
                .iter()
                .any(|&i| weighed[i].plan.steer.cards.iter().any(|c| c == "Imperial")));
    Verdict {
        plans: weighed,
        chosen,
        imperial,
        fallback,
    }
}

thread_local! {
    static GATHER: std::cell::RefCell<Vec<SystemId>> = const { std::cell::RefCell::new(Vec::new()) };
    static PILE: std::cell::RefCell<Vec<SystemId>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Whether `system` is a gather step of the plans chosen by the last [`verdict`] (the movement
/// hook then moves every ship and its cargo that can go on).
#[must_use]
pub fn gathering(system: &SystemId) -> bool {
    tuning::bw_enabled() && GATHER.with(|g| g.borrow().contains(system))
}

/// Whether `system` is a system whose invasion the last [`verdict`] wants piled on one planet.
#[must_use]
pub fn piling(system: &SystemId) -> bool {
    tuning::bw_enabled() && PILE.with(|g| g.borrow().contains(system))
}

/// Plan the seat's turn. `None` when the layer is off or it is too early.
#[must_use]
pub fn verdict(seen: &SeatObservation<'_>, me: &PlayerId) -> Option<Verdict> {
    if !tuning::bw_enabled() || seen.observed().round() < tuning::bw_round() {
        return None;
    }
    let mut ctx = PlanCtx::new(seen, me);
    let planners = registry();
    let mut plans = Vec::new();
    for goal in ctx.goals.clone() {
        if goal.satisfied {
            continue;
        }
        ctx.goal = goal.clone();
        for planner in &planners {
            if goal.matches_any(planner.objective_ids())
                && let Some(plan) = planner.plan(seen, &ctx)
            {
                plans.push(plan);
                break;
            }
        }
    }
    if tuning::bw_mecatol() {
        plans.extend(mecatol::plan(seen, &ctx));
        plans.extend(mecatol_two_step::plan(seen, &ctx));
    }
    let verdict = weigh(&ctx, plans);
    GATHER.with(|g| {
        *g.borrow_mut() = if verdict.fallback {
            Vec::new()
        } else {
            verdict
                .chosen
                .iter()
                .flat_map(|&i| verdict.plans[i].plan.steer.gather.iter().cloned())
                .collect()
        };
    });
    PILE.with(|g| {
        *g.borrow_mut() = if verdict.fallback {
            Vec::new()
        } else {
            verdict
                .chosen
                .iter()
                .flat_map(|&i| verdict.plans[i].plan.steer.pile.iter().cloned())
                .collect()
        };
    });
    Some(verdict)
}

impl PlanCtx {
    /// The usual plan of a route-shaped goal: the cheapest route of at most `max_steps` of the
    /// steps `useful` accepts, or `None` when there is none (nothing reachable this turn).
    pub fn route_plan(
        &self,
        seen: &SeatObservation<'_>,
        max_steps: usize,
        useful: impl Fn(&Step) -> bool,
        what: &str,
    ) -> Option<Plan> {
        let route = self.route(seen, &self.goal.alias, max_steps, useful)?;
        let names: Vec<&str> = route
            .steps
            .iter()
            .map(|&i| self.steps[i].system.as_str())
            .collect();
        Some(self.plan_of(
            &route,
            format!("{}: {what} via {}", self.goal.alias, names.join(", ")),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;
    use ti4_content::ContentStore;
    use ti4_engine::choice::{Choice, ChoiceOption, Decider, IllegalChoice};
    use ti4_engine::fixtures::{game, hub_from, hub_with_centre, plain_systems, put};
    use ti4_model::POK;
    use ti4_model::state::{GameState, Phase};

    fn me() -> PlayerId {
        PlayerId::new("a")
    }

    /// The family token of every public and secret objective that has a running count (or a
    /// cost), by alias; event objectives are absent.
    pub(crate) fn alias_families() -> BTreeMap<String, String> {
        let content = ContentStore::embedded();
        let state = game(&["a", "b"]);
        let hub = hub_with_centre("18");
        let player = me();
        let mut position = ti4_engine::objectives::Position::new(&state, content, POK, &player);
        position.galaxy = Some(&hub.galaxy);
        let secret_position = ti4_engine::secrets::Position {
            state: &state,
            content,
            sources: POK,
            player: &player,
            galaxy: Some(&hub.galaxy),
            imagined: ti4_engine::objectives::Imagined::NONE,
        };
        let mut out = BTreeMap::new();
        for file in ["public_objectives", "secret_objectives"] {
            let secret = file.starts_with("secret");
            for record in content.records(if secret {
                ti4_model::content_types::ContentType::SecretObjectives
            } else {
                ti4_model::content_types::ContentType::PublicObjectives
            }) {
                let Some(alias) = record.text("alias") else {
                    continue;
                };
                let family = if secret {
                    ti4_engine::secrets::progress(
                        &ti4_model::id::SecretObjectiveId::new(alias),
                        &secret_position,
                    )
                    .map(|p| ti4_engine::objectives::family_token(&p.family))
                } else {
                    let id = ti4_model::id::ObjectiveId::new(alias);
                    ti4_engine::objectives::counting_progress(&id, &position)
                        .or_else(|| {
                            ti4_engine::objectives::remaining_position_progress(&id, &position)
                        })
                        .map(|p| ti4_engine::objectives::family_token(&p.family))
                        .or_else(|| {
                            ti4_engine::objectives::bought_progress_at(&position, &id)
                                .map(|p| ti4_engine::objectives::cost_family_token(&p.family))
                        })
                };
                out.insert(alias.to_owned(), family.unwrap_or_default());
            }
        }
        out
    }

    /// The aliases (public and secret) a planner would take, by alias or by family token.
    pub(crate) fn matched_aliases(
        planner: &dyn ObjectivePlanner,
    ) -> std::collections::BTreeSet<String> {
        let ids = planner.objective_ids();
        alias_families()
            .into_iter()
            .filter(|(alias, family)| {
                ids.iter()
                    .any(|id| id == alias || (!family.is_empty() && id == family))
            })
            .map(|(alias, _)| alias)
            .collect()
    }

    fn plan(
        objective: &str,
        points: f64,
        probability: f64,
        tokens: i32,
        secret: bool,
        payment: bool,
    ) -> Plan {
        Plan {
            objective: objective.to_owned(),
            points,
            probability,
            cost: Cost {
                tokens,
                resources: 0,
                activations: 0,
            },
            steer: Steer {
                systems: vec![SystemId::new(objective)],
                ..Steer::default()
            },
            reason: String::new(),
            payment,
            secret,
        }
    }

    fn on() -> tuning::Tuning {
        tuning::Tuning {
            bw_enabled: true,
            ..tuning::for_version(3)
        }
    }

    #[test]
    fn nothing_reachable_falls_back() {
        tuning::with(on(), || {
            let v = weigh(
                &PlanCtx::blank(&me()),
                vec![plan("x", 1.0, 0.1, 1, false, false)],
            );
            assert!(v.fallback && v.chosen.is_empty() && v.bonus().is_empty());
            assert!(weigh(&PlanCtx::blank(&me()), Vec::new()).fallback);
        });
    }

    #[test]
    fn a_likely_plan_is_chosen_and_steers_its_systems() {
        tuning::with(on(), || {
            let v = weigh(
                &PlanCtx::blank(&me()),
                vec![plan("x", 1.0, 0.9, 1, false, false)],
            );
            assert!(!v.fallback && v.chosen == vec![0] && v.bonus().len() == 1);
        });
    }

    #[test]
    fn two_likely_publics_want_imperial_and_a_payment_goal_is_discounted() {
        tuning::with(on(), || {
            let one = weigh(
                &PlanCtx::blank(&me()),
                vec![plan("x", 1.0, 0.9, 1, false, false)],
            );
            assert!(!one.imperial);
            let two = weigh(
                &PlanCtx::blank(&me()),
                vec![
                    plan("x", 1.0, 0.9, 1, false, false),
                    plan("y", 1.0, 0.8, 1, false, false),
                ],
            );
            assert!(two.imperial);
            // The second public only counts a quarter without Imperial; with it, full.
            assert!(two.plans[1].expected > one.plans[0].expected - 0.2);
            let paid = weigh(
                &PlanCtx::blank(&me()),
                vec![plan("p", 1.0, 0.9, 0, false, true)],
            );
            let free = weigh(
                &PlanCtx::blank(&me()),
                vec![plan("p", 1.0, 0.9, 0, false, false)],
            );
            assert!(paid.plans[0].expected < free.plans[0].expected);
        });
    }

    #[test]
    fn the_mecatol_plan_competes_and_asks_for_imperial() {
        tuning::with(on(), || {
            let mut mecatol = plan("mecatol", 1.6, 0.8, 1, false, false);
            mecatol.steer.cards = vec!["Imperial".to_owned()];
            let v = weigh(
                &PlanCtx::blank(&me()),
                vec![plan("x", 1.0, 0.5, 1, false, false), mecatol],
            );
            assert_eq!(v.plans[v.chosen[0]].plan.objective, "mecatol");
            assert!(v.imperial);
        });
    }

    // -- positions --

    fn planets(system: &str) -> Vec<ti4_content::galaxy::Planet<'static>> {
        ti4_content::galaxy::planets_in(ContentStore::embedded(), system, POK)
    }

    fn cultural() -> Vec<String> {
        plain_systems(200)
            .into_iter()
            .filter(|id| {
                let p = planets(id);
                p.len() == 1 && p[0].has_trait("cultural")
            })
            .collect()
    }

    fn empty_systems(count: usize) -> Vec<String> {
        plain_systems(200)
            .into_iter()
            .filter(|id| planets(id).is_empty())
            .take(count)
            .collect()
    }

    struct Probe(Option<Verdict>);
    impl Decider for Probe {
        fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
            Ok(choice.options[0].clone())
        }
        fn choose_seeing(
            &mut self,
            choice: &Choice,
            seen: &SeatObservation<'_>,
        ) -> Result<ChoiceOption, IllegalChoice> {
            self.0 = verdict(seen, &me());
            let _ = ChaCha8Rng::seed_from_u64(1);
            Ok(choice.options[0].clone())
        }
    }

    fn verdict_of(state: &GameState, hub: &ti4_engine::fixtures::Hub) -> Option<Verdict> {
        let options = vec![ChoiceOption::labelled("x", "activate", "x")];
        let mut probe = Probe(None);
        ti4_engine::choice::ask_private(
            &Choice::new(me(), "activate a system", options),
            state,
            ContentStore::embedded(),
            POK,
            Some(&hub.galaxy),
            &mut probe,
        )
        .expect("legal");
        probe.0
    }

    /// Round three; the seat holds `held` cultural planets off the map, has carrier and troops at
    /// the first outer system, and `targets` cultural systems are around; Form an Alliance
    /// (`faa`, four cultural planets) is its secret.
    fn position(held: usize, targets: usize) -> (GameState, ti4_engine::fixtures::Hub) {
        let cul = cultural();
        let empty = empty_systems(7 - targets);
        let mut ids = vec![empty[0].clone()];
        ids.extend(cul[..targets].iter().cloned());
        ids.extend(empty[1..].iter().cloned());
        let hub = hub_from(&ids);
        let mut state = game(&["a", "b"]);
        state.round = 3;
        state.phase = Phase::Action;
        state.revealed_objectives.clear();
        for seat in &mut state.players {
            seat.secret_objectives.clear();
        }
        let start = SystemId::new(hub.outer.last().expect("outer").clone());
        put(&mut state, &start, "carrier", &me(), 1);
        put(&mut state, &start, "infantry", &me(), 2);
        for system in &cul[targets..targets + held] {
            let planet = PlanetId::new(planets(system)[0].id());
            state
                .system_mut(&SystemId::new(system.clone()))
                .planet_control
                .insert(planet, me());
        }
        state
            .player_mut(&me())
            .expect("seat")
            .secret_objectives
            .push(ti4_model::id::SecretObjectiveId::new("faa"));
        (state, hub)
    }

    #[test]
    fn territory_plans_the_planet_that_completes_the_secret() {
        tuning::with(on(), || {
            let (state, hub) = position(3, 2);
            let v = verdict_of(&state, &hub).expect("on");
            let best = &v.plans.first().expect("a plan");
            assert_eq!(best.plan.objective, "faa");
            assert!(best.plan.probability > 0.5 && !v.fallback, "{v:?}");
            assert_eq!(best.plan.steer.systems.len(), 1);
        });
    }

    #[test]
    fn territory_is_not_reachable_two_planets_short_with_one_token() {
        tuning::with(
            tuning::Tuning {
                bw_enabled: true,
                ..tuning::for_version(3)
            },
            || {
                let (mut state, hub) = position(1, 2);
                state.player_mut(&me()).expect("seat").tactic_tokens = 1;
                let v = verdict_of(&state, &hub).expect("on");
                assert!(v.plans.iter().all(|w| w.plan.objective != "faa"), "{v:?}");
                assert!(v.fallback);
            },
        );
    }

    /// Seven empty systems, Deep Space (ships in three planetless systems) revealed, the seat's
    /// cruisers placed as `(index of the outer system, count)`.
    fn deep_space(ships: &[(usize, usize)]) -> (GameState, ti4_engine::fixtures::Hub) {
        let hub = hub_from(&empty_systems(7));
        let mut state = game(&["a", "b"]);
        state.round = 3;
        state.phase = Phase::Action;
        state.revealed_objectives = vec![ti4_model::id::ObjectiveId::new("deep_space")];
        for seat in &mut state.players {
            seat.secret_objectives.clear();
        }
        for (at, count) in ships {
            put(
                &mut state,
                &SystemId::new(hub.outer[*at].clone()),
                "cruiser",
                &me(),
                *count,
            );
        }
        (state, hub)
    }

    #[test]
    fn fleet_plans_the_third_planetless_system_from_a_spare_ship() {
        tuning::with(on(), || {
            let (state, hub) = deep_space(&[(0, 1), (2, 3)]);
            let v = verdict_of(&state, &hub).expect("on");
            let best = v.plans.first().expect("a plan");
            assert_eq!(best.plan.objective, "deep_space");
            assert_eq!(best.plan.cost.activations, 1);
            assert!(!v.fallback, "{v:?}");
        });
    }

    #[test]
    fn fleet_has_no_plan_when_two_systems_are_missing_and_one_token_is_left() {
        tuning::with(on(), || {
            let (mut state, hub) = deep_space(&[(2, 3)]);
            state.player_mut(&me()).expect("seat").tactic_tokens = 1;
            let v = verdict_of(&state, &hub).expect("on");
            assert!(v.plans.iter().all(|w| w.plan.objective != "deep_space"));
            assert!(v.fallback);
        });
    }

    // -- Mecatol two-step plans --

    /// Mecatol Rex in the centre, the Sol home (`1`, with its dock) on the ring holding a
    /// carrier with `troops` infantry, `defenders` rival infantry on Mecatol Rex, custodians gone.
    fn mecatol_position(troops: usize, defenders: usize) -> (GameState, ti4_engine::fixtures::Hub) {
        let home_id = ti4_content::galaxy::planet(ContentStore::embedded(), "jord", POK)
            .and_then(|p| p.system_id())
            .expect("sol home")
            .to_owned();
        let mut ids = vec!["18".to_owned(), home_id.clone()];
        ids.extend(empty_systems(5));
        let hub = hub_from(&ids);
        let mut state = game(&["a", "b"]);
        state.round = 3;
        state.phase = Phase::Action;
        state.custodians_removed = true;
        state.revealed_objectives.clear();
        for seat in &mut state.players {
            seat.secret_objectives.clear();
        }
        let home = SystemId::new(home_id);
        let jord = PlanetId::new("jord");
        state
            .system_mut(&home)
            .planet_control
            .insert(jord.clone(), me());
        ti4_engine::fixtures::put_on_planet(&mut state, &home, &jord, "spacedock", &me(), 1);
        put(&mut state, &home, "carrier", &me(), 1);
        ti4_engine::fixtures::put_on_planet(&mut state, &home, &jord, "infantry", &me(), troops);
        let mr = SystemId::new("18");
        ti4_engine::fixtures::put_on_planet(
            &mut state,
            &mr,
            &PlanetId::new("mr"),
            "infantry",
            &PlayerId::new("b"),
            defenders,
        );
        state
            .system_mut(&mr)
            .planet_control
            .insert(PlanetId::new("mr"), PlayerId::new("b"));
        (state, hub)
    }

    fn build_plan(v: &Verdict) -> Option<&Weighed> {
        v.plans
            .iter()
            .find(|w| w.plan.reason.starts_with("build + act"))
    }

    fn build_on() -> tuning::Tuning {
        tuning::Tuning {
            bw_prep_build: true,
            ..on()
        }
    }

    #[test]
    fn build_and_act_adds_the_troops_a_home_dock_can_make() {
        tuning::with(build_on(), || {
            let (state, hub) = mecatol_position(2, 3);
            let v = verdict_of(&state, &hub).expect("on");
            let plan = &build_plan(&v).expect("a two-step plan").plan;
            assert_eq!(plan.steer.systems.len(), 2);
            assert!(
                plan.steer.builds.iter().any(|b| b == "infantry"),
                "{plan:?}"
            );
            assert!(plan.cost.tokens == 2 && plan.probability > 0.3, "{plan:?}");
        });
    }

    #[test]
    fn build_and_act_is_not_offered_when_no_production_can_close_the_gap() {
        tuning::with(build_on(), || {
            let (state, hub) = mecatol_position(2, 12);
            let v = verdict_of(&state, &hub).expect("on");
            assert!(build_plan(&v).is_none(), "{v:?}");
        });
    }

    #[test]
    fn build_and_act_leaves_a_one_step_take_alone() {
        tuning::with(build_on(), || {
            let (state, hub) = mecatol_position(4, 3);
            let v = verdict_of(&state, &hub).expect("on");
            assert!(build_plan(&v).is_none(), "{v:?}");
        });
    }

    /// Empty centre, Mecatol Rex on the ring at 0 (`defenders` rival infantry, custodians gone),
    /// the Sol home opposite (3) with a carrier and `troops` infantry: two hexes from Mecatol Rex
    /// through the centre, one move short for the carrier.
    fn far_position(
        troops: usize,
        defenders: usize,
        at: usize,
    ) -> (GameState, ti4_engine::fixtures::Hub) {
        let home_id = ti4_content::galaxy::planet(ContentStore::embedded(), "jord", POK)
            .and_then(|p| p.system_id())
            .expect("sol home")
            .to_owned();
        let mut empties = empty_systems(6).into_iter();
        let mut ids = vec![empties.next().expect("centre"), "18".to_owned()];
        for ring in 1..6 {
            ids.push(if ring == 3 {
                home_id.clone()
            } else {
                empties.next().expect("ring")
            });
        }
        let hub = hub_from(&ids);
        let mut state = game(&["a", "b"]);
        state.round = 3;
        state.phase = Phase::Action;
        state.custodians_removed = true;
        state.revealed_objectives.clear();
        for seat in &mut state.players {
            seat.secret_objectives.clear();
        }
        let home = SystemId::new(home_id);
        let jord = PlanetId::new("jord");
        state
            .system_mut(&home)
            .planet_control
            .insert(jord.clone(), me());
        let from = SystemId::new(hub.outer[at].clone());
        put(&mut state, &from, "carrier", &me(), 1);
        ti4_engine::fixtures::put_on_planet(&mut state, &home, &jord, "infantry", &me(), troops);
        let mr = SystemId::new("18");
        ti4_engine::fixtures::put_on_planet(
            &mut state,
            &mr,
            &PlanetId::new("mr"),
            "infantry",
            &PlayerId::new("b"),
            defenders,
        );
        state
            .system_mut(&mr)
            .planet_control
            .insert(PlanetId::new("mr"), PlayerId::new("b"));
        (state, hub)
    }

    fn warfare_plan(v: &Verdict) -> Option<&Weighed> {
        v.plans
            .iter()
            .find(|w| w.plan.reason.starts_with("warfare move + act"))
    }

    fn warfare_on() -> tuning::Tuning {
        tuning::Tuning {
            bw_prep_warfare: true,
            ..on()
        }
    }

    #[test]
    fn warfare_variant_gathers_beside_mecatol_when_tokens_suffice() {
        tuning::with(warfare_on(), || {
            // The carrier sits on the home ring slot 3; troops are on the home planet there.
            let (state, hub) = far_position(2, 1, 3);
            let v = verdict_of(&state, &hub).expect("on");
            let w = warfare_plan(&v).expect("a two-step plan");
            assert_eq!(w.plan.steer.systems.len(), 2);
            assert!(w.plan.steer.play.is_empty(), "{:?}", w.plan);
            assert_eq!(w.plan.steer.systems[0].as_str(), hub.centre);
        });
    }

    #[test]
    fn warfare_variant_plays_the_card_when_one_token_is_left() {
        tuning::with(warfare_on(), || {
            let (mut state, hub) = far_position(2, 1, 3);
            let seat = state.player_mut(&me()).expect("seat");
            seat.tactic_tokens = 1;
            seat.strategy_cards
                .push(ti4_model::id::StrategyCardId::new("te6warfare"));
            let v = verdict_of(&state, &hub).expect("on");
            let w = warfare_plan(&v).expect("a two-step plan");
            assert_eq!(w.plan.steer.play, vec!["Warfare".to_owned()]);
            // Without the card the single token cannot do both steps.
            state
                .player_mut(&me())
                .expect("seat")
                .strategy_cards
                .clear();
            let v = verdict_of(&state, &hub).expect("on");
            assert!(warfare_plan(&v).is_none());
        });
    }

    #[test]
    fn warfare_variant_is_not_offered_without_enough_troops_or_when_one_step_suffices() {
        tuning::with(warfare_on(), || {
            let (state, hub) = far_position(1, 3, 3);
            assert!(warfare_plan(&verdict_of(&state, &hub).expect("on")).is_none());
            // Carrier next to Mecatol Rex already: the one-step plan is enough.
            let (state, hub) = far_position(2, 1, 1);
            let seat_home_troops = state.clone();
            let _ = seat_home_troops;
            assert!(warfare_plan(&verdict_of(&state, &hub).expect("on")).is_none());
        });
    }

    /// The steps of the context built at a decision.
    fn steps_of(state: &GameState, hub: &ti4_engine::fixtures::Hub) -> Vec<Step> {
        struct Grab(Vec<Step>);
        impl Decider for Grab {
            fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
                Ok(choice.options[0].clone())
            }
            fn choose_seeing(
                &mut self,
                choice: &Choice,
                seen: &SeatObservation<'_>,
            ) -> Result<ChoiceOption, IllegalChoice> {
                self.0 = PlanCtx::new(seen, &me()).steps;
                Ok(choice.options[0].clone())
            }
        }
        let options = vec![ChoiceOption::labelled("x", "activate", "x")];
        let mut grab = Grab(Vec::new());
        ti4_engine::choice::ask_private(
            &Choice::new(me(), "activate a system", options),
            state,
            ContentStore::embedded(),
            POK,
            Some(&hub.galaxy),
            &mut grab,
        )
        .expect("legal");
        grab.0
    }

    #[test]
    fn every_planner_sees_the_prepared_steps_of_the_layer() {
        let (state, hub) = mecatol_position(2, 3);
        // Layer off: Mecatol Rex has no step (two troops against three defenders).
        tuning::with(on(), || {
            assert!(steps_of(&state, &hub).iter().all(|s| s.prep.is_none()));
        });
        tuning::with(build_on(), || {
            let steps = steps_of(&state, &hub);
            let prepared = steps
                .iter()
                .find(|s| s.prep.is_some() && s.system.as_str() == "18")
                .expect("a prepared step");
            let prep = prepared.prep.as_ref().expect("prep");
            assert_eq!(prep.kind, prep::PrepKind::Build);
            assert!(prepared.success < 1.0 && prepared.success > 0.3);
        });
    }

    #[test]
    fn a_route_holds_one_prepared_step_and_stays_within_the_tokens() {
        let mut ctx = PlanCtx::blank(&me());
        let prep = |tokens| prep::PrepInfo {
            kind: prep::PrepKind::Build,
            label: String::new(),
            tokens,
            strategic: 0,
            resources: 0,
            systems: Vec::new(),
            gather: Vec::new(),
            builds: Vec::new(),
            cards: Vec::new(),
            play: Vec::new(),
            techs: Vec::new(),
            odds: 0.9,
        };
        let step = |name: &str, prep: Option<prep::PrepInfo>| Step {
            system: SystemId::new(name),
            planets: Vec::new(),
            ship: true,
            spare_ship: true,
            success: 0.8,
            prep,
        };
        ctx.steps = vec![
            step("a", Some(prep(1))),
            step("b", Some(prep(0))),
            step("c", None),
        ];
        ctx.tactic_tokens = 3;
        assert!(ctx.affordable(&[0, 2]));
        assert!(!ctx.affordable(&[0, 1]), "two prepared steps");
        ctx.tactic_tokens = 2;
        assert!(!ctx.affordable(&[0, 2]), "the prep's token counts");
        assert!(ctx.affordable(&[1, 2]));
    }

    fn tech_plan<'v>(v: &'v Verdict, label: &str) -> Option<&'v Weighed> {
        v.plans.iter().find(|w| w.plan.reason.starts_with(label))
    }

    fn give(state: &mut GameState, techs: &[&str], card: Option<&str>) {
        let seat = state.player_mut(&me()).expect("seat");
        for t in techs {
            seat.technologies
                .insert(ti4_model::id::TechnologyId::new(*t));
        }
        if let Some(card) = card {
            seat.strategy_cards
                .push(ti4_model::id::StrategyCardId::new(card));
        }
    }

    #[test]
    fn tech_range_brings_a_far_carrier_into_reach_with_gravity_drive() {
        let tuned = tuning::Tuning {
            bw_prep_tech_range: true,
            ..on()
        };
        tuning::with(tuned, || {
            let (mut state, hub) = far_position(2, 1, 3);
            // Without a technology to research nothing changes.
            assert!(
                tech_plan(&verdict_of(&state, &hub).expect("on"), "technology range").is_none()
            );
            give(&mut state, &["amd"], Some("pok7technology"));
            let v = verdict_of(&state, &hub).expect("on");
            let w = tech_plan(&v, "technology range + act (Gravity Drive)").expect("a plan");
            assert_eq!(w.plan.steer.techs, vec!["gd".to_owned()]);
            assert!(w.plan.steer.cards.iter().any(|c| c == "Technology"));
            assert_eq!(w.plan.steer.play, vec!["Technology".to_owned()]);
            // A carrier already next to Mecatol Rex needs no range.
            let (mut near, hub) = far_position(2, 1, 1);
            give(&mut near, &["amd"], Some("pok7technology"));
            assert!(tech_plan(&verdict_of(&near, &hub).expect("on"), "technology range").is_none());
        });
    }

    #[test]
    fn tech_strength_wins_the_fight_a_better_cruiser_decides() {
        let tuned = tuning::Tuning {
            bw_prep_tech_strength: true,
            ..on()
        };
        tuning::with(tuned, || {
            let find = |techs: &[&str]| {
                (1..12).find_map(|k| {
                    let (mut state, hub) = mecatol_position(2, 1);
                    let mr = SystemId::new("18");
                    put(&mut state, &mr, "cruiser", &PlayerId::new("b"), k);
                    let home = state
                        .board
                        .iter()
                        .find(|(s, r)| {
                            s.as_str() != "18" && r.planet_control.values().any(|o| o == &me())
                        })
                        .map(|(s, _)| s.clone())
                        .expect("home");
                    put(&mut state, &home, "cruiser", &me(), 4);
                    give(&mut state, techs, Some("pok7technology"));
                    let v = verdict_of(&state, &hub).expect("on");
                    tech_plan(&v, "technology strength").map(|w| (k, w.plan.steer.techs.clone()))
                })
            };
            let (_, techs) =
                find(&["nm", "st", "ps"]).expect("some fleet is decided by cruiser II");
            assert_eq!(techs, vec!["cr2".to_owned()]);
            assert!(find(&[]).is_none(), "no technology to research, no plan");
        });
    }

    // -- fleet shape --

    fn shape_on() -> tuning::Tuning {
        tuning::Tuning {
            bw_fleet_shape: true,
            bw_prep_build: true,
            ..on()
        }
    }

    /// Mecatol Rex in the centre, the Sol home with its dock and `home_ships` cruisers on the
    /// ring, `other` cruisers on the next ring system, `alias` revealed.
    fn shape_position(
        alias: &str,
        home_ships: usize,
        other: usize,
        trade_goods: i32,
    ) -> (GameState, ti4_engine::fixtures::Hub) {
        let (mut state, hub) = mecatol_position(0, 0);
        state.revealed_objectives = vec![ti4_model::id::ObjectiveId::new(alias)];
        let mr = SystemId::new("18");
        state.system_mut(&mr).planet_control.clear();
        state.system_mut(&mr).planet_units.clear();
        let home = state
            .board
            .iter()
            .find(|(s, r)| s.as_str() != "18" && r.planet_control.values().any(|o| o == &me()))
            .map(|(s, _)| s.clone())
            .expect("home");
        state.system_mut(&home).units.clear();
        put(&mut state, &home, "cruiser", &me(), home_ships);
        let next = SystemId::new(hub.outer[2].clone());
        put(&mut state, &next, "cruiser", &me(), other);
        let seat = state.player_mut(&me()).expect("seat");
        seat.fleet_tokens = 9;
        seat.trade_goods = trade_goods;
        seat.faction = ti4_model::id::FactionId::new("sol");
        (state, hub)
    }

    fn plan_for<'v>(v: &'v Verdict, alias: &str) -> Option<&'v Weighed> {
        v.plans.iter().find(|w| w.plan.objective == alias)
    }

    #[test]
    fn raise_a_fleet_gathers_the_ships_in_one_system() {
        tuning::with(shape_on(), || {
            let (state, hub) = shape_position("raise_fleet", 3, 2, 0);
            let v = verdict_of(&state, &hub).expect("on");
            let w = plan_for(&v, "raise_fleet").expect("a plan");
            assert_eq!(w.plan.steer.gather.len(), 1);
            assert!(w.plan.reason.starts_with("gather"), "{:?}", w.plan);
        });
    }

    #[test]
    fn raise_a_fleet_builds_the_missing_ships_or_has_no_plan() {
        tuning::with(shape_on(), || {
            // Four ships all together: one more is built at the dock.
            let (state, hub) = shape_position("raise_fleet", 4, 0, 0);
            let v = verdict_of(&state, &hub).expect("on");
            let w = plan_for(&v, "raise_fleet").expect("a plan");
            assert!(!w.plan.steer.builds.is_empty(), "{:?}", w.plan);
            // Nothing within reach, nothing to build with.
            let off = tuning::Tuning {
                bw_prep_build: false,
                ..shape_on()
            };
            tuning::with(off, || {
                let (state, hub) = shape_position("raise_fleet", 2, 1, 0);
                assert!(plan_for(&verdict_of(&state, &hub).expect("on"), "raise_fleet").is_none());
            });
        });
    }

    #[test]
    fn a_flagship_is_built_when_it_can_be_paid() {
        tuning::with(shape_on(), || {
            let (state, hub) = shape_position("engineer_marvel", 1, 0, 8);
            let v = verdict_of(&state, &hub).expect("on");
            let w = plan_for(&v, "engineer_marvel").expect("a plan");
            assert!(
                w.plan
                    .steer
                    .builds
                    .iter()
                    .any(|b| b.contains("flagship") || !b.is_empty())
            );
            let (poor, hub) = shape_position("engineer_marvel", 1, 0, 0);
            assert!(plan_for(&verdict_of(&poor, &hub).expect("on"), "engineer_marvel").is_none());
        });
    }

    #[test]
    fn supremacy_moves_an_existing_flagship_to_mecatol_rex() {
        tuning::with(shape_on(), || {
            let (mut state, hub) = shape_position("supremacy", 1, 0, 0);
            let flagship = ti4_content::units::catalogue(ContentStore::embedded(), POK)
                .values()
                .find(|t| t.base_type() == "flagship" && t.faction() == Some("sol"))
                .map(|t| t.id().to_owned())
                .expect("sol flagship");
            put(
                &mut state,
                &SystemId::new(hub.outer[2].clone()),
                &flagship,
                &me(),
                1,
            );
            let v = verdict_of(&state, &hub).expect("on");
            let w = plan_for(&v, "supremacy").expect("a plan");
            assert!(
                w.plan.steer.systems.iter().any(|s| s.as_str() == "18"),
                "{:?}",
                w.plan
            );
            // The same position without the flagship and without the means to build one.
            let (poor, hub) = shape_position("supremacy", 1, 0, 0);
            assert!(plan_for(&verdict_of(&poor, &hub).expect("on"), "supremacy").is_none());
        });
    }

    #[test]
    fn a_ground_pile_is_planned_from_the_forces_the_carriers_bring() {
        tuning::with(shape_on(), || {
            let (mut state, hub) = mecatol_position(0, 0);
            let home = state
                .board
                .iter()
                .find(|(s, r)| s.as_str() != "18" && r.planet_control.values().any(|o| o == &me()))
                .map(|(s, _)| s.clone())
                .expect("home");
            put(&mut state, &home, "carrier", &me(), 2);
            put(&mut state, &home, "infantry", &me(), 9);
            state
                .player_mut(&me())
                .expect("seat")
                .secret_objectives
                .push(ti4_model::id::SecretObjectiveId::new("otf"));
            let v = verdict_of(&state, &hub).expect("on");
            let w = plan_for(&v, "otf").expect("a plan");
            assert_eq!(w.plan.steer.pile.len(), 1);
            let (few, hub) = mecatol_position(3, 0);
            let mut few = few;
            few.player_mut(&me())
                .expect("seat")
                .secret_objectives
                .push(ti4_model::id::SecretObjectiveId::new("otf"));
            assert!(plan_for(&verdict_of(&few, &hub).expect("on"), "otf").is_none());
        });
    }

    #[test]
    fn off_before_version_4() {
        let (state, hub) = position(3, 2);
        tuning::with(tuning::for_version(3), || {
            assert!(verdict_of(&state, &hub).is_none());
        });
        tuning::with(tuning::for_version(4), || {
            assert!(verdict_of(&state, &hub).is_some());
        });
    }
}
