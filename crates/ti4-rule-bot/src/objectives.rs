//! The rule bot's objective layer: from round three ([`OBJECTIVE_ROUND`]) it plays for victory
//! points and for keeping them.
//!
//! A child module of [`super`] (the rule bot), consulted by [`super::pick`] before its own
//! opinions through one hook, [`pick`]. It returns `Some(index)` only where an objective changes
//! the answer and `None` otherwise, so everything else stays exactly as the rule bot plays it.
//! It reads only the seat's own observation: the revealed public objectives, its own secrets
//! (through the engine's bound-seat progress, [`SeatObservation::held_secret_progress`]) and the
//! public board. It is deterministic.
//!
//! # What it does
//!
//! - **Scoring**: of the objectives offered, the most points first; between equal points a
//!   requirement it might lose before a bought one it can buy again, and trade goods or command
//!   tokens last (planets exhausted in the status phase are readied right after it, so a planet
//!   spend is nearly free then). Imperial's extra objective the same way.
//! - **Holding back** ([`Reserve`]): when its best public objective this round is a bought one it
//!   can afford now (Erect a Monument, Sway the Council, Amass Wealth, Lead from the Front, ...)
//!   and no requirement it already meets is worth as much, it keeps what that costs until the
//!   status phase: production, research and Leadership's influence stay within what is spare, a
//!   payment uses the planets the objective does not need, and command tokens are not spent on
//!   secondaries or minor activations.
//! - **Activation**: each system's smart-bot value gains what taking it would do for its
//!   objectives, computed by the engine's own requirement functions on the imagined position
//!   (the planets its carriers can take there, and a ship in the system): completing a public or
//!   secret objective it can score this round is worth [`COMPLETE_VALUE`] per point, progress
//!   toward one within two steps a share of [`PROGRESS_VALUE`]. A fight that wins nothing (no
//!   planet, no objective, nothing of its own threatened) is not taken any more; a planet of a
//!   rival about to win is worth more ([`DENY_VALUE`]). With a ship to send and nothing else to
//!   move, a presence objective's system gets the cheapest ship that leaves company behind.
//! - **Holding on**: a lone ship does not leave a system while a just-met placement objective it
//!   would score this round could depend on it (Intimidate Council's and the rim's systems
//!   exactly, other ship-placement families conservatively).
//! - **Imperial**: played as soon as a public objective is scoreable (one point now and the
//!   status phase's still to come); drafted when a public objective is or can be met, besides
//!   when Mecatol Rex is held or takeable.
//! - **Strategy cards** from round three: the card that serves an objective within reach
//!   (Technology for technology counts, Construction for structures, Leadership for token costs,
//!   Trade for trade goods), else as before (trade goods on the card).
//! - **Research** and **structure placement**: the option that moves an objective most.
//! - **Secret objectives**: over the hand limit, the secret furthest from done goes back.

use std::collections::BTreeSet;

use rand::Rng;
use rand_chacha::ChaCha8Rng;
use ti4_engine::choice::{Choice, ChoiceOption, Observed, SeatObservation};
use ti4_engine::objectives::{CardProgress, Imagined};
use ti4_engine::production::Spend;
use ti4_model::id::{ObjectiveId, PlanetId, PlayerId, SystemId};

use super::{
    ACTIVATION_THRESHOLD, EAGER_WORTH, activation_value, best_above, card_is, has_hostile_ships,
    holds_mecatol, home_system, is_ground, is_ship, mecatol_in_reach, mecatol_takeable,
    payload_f64, payload_str, ready_card, strategic_option, targets, unit_type,
};

/// The first round the objective layer plays: rounds one and two are the expansion.
pub const OBJECTIVE_ROUND: u32 = 3;
/// Activation value of completing an objective it can score this round, per victory point.
pub const COMPLETE_VALUE: f64 = 6.0;
/// Activation value of progress toward an objective within two steps, per victory point, scaled
/// by the share of the requirement gained.
pub const PROGRESS_VALUE: f64 = 3.0;
/// Extra value of each planet taken from a rival this close to winning.
pub const DENY_VALUE: f64 = 3.0;
/// A rival this many points (or fewer) from ten is about to win.
pub const DENY_WITHIN: i32 = 3;
/// Victory points that win the game.
const WIN: i32 = 10;

/// Which seats play the layer: all (`None`, the default) or only those named. For before/after
/// surveys (`ti4-server/tests/it/smart_bot_objectives_survey.rs`), like the random bots' delay override.
static ENABLED: std::sync::Mutex<Option<BTreeSet<String>>> = std::sync::Mutex::new(None);

/// Switch the objective layer on or off for every seat, process-wide (surveys only).
pub fn set_enabled(enabled: bool) {
    *ENABLED.lock().expect("objective layer switch") =
        if enabled { None } else { Some(BTreeSet::new()) };
}

/// Switch the objective layer on for the seats named `seats` only, process-wide (surveys only):
/// the others play the rule bot without it.
pub fn set_enabled_for(seats: &[&str]) {
    *ENABLED.lock().expect("objective layer switch") =
        Some(seats.iter().map(|s| (*s).to_owned()).collect());
}

fn enabled_for(seat: &PlayerId) -> bool {
    ENABLED
        .lock()
        .expect("objective layer switch")
        .as_ref()
        .is_none_or(|seats| seats.contains(seat.as_str()))
}

/// Whether the objective layer plays this decision.
fn active(seen: &Observed<'_>) -> bool {
    seen.round() >= OBJECTIVE_ROUND
}

/// The objective layer's answer to `choice`, or `None` to leave it to the rule bot.
pub fn pick(
    choice: &Choice,
    seen: &SeatObservation<'_>,
    me: &PlayerId,
    rng: &mut ChaCha8Rng,
) -> Option<usize> {
    if !enabled_for(me) {
        return None;
    }
    let observed = seen.observed();
    let subtype = choice.context.as_ref().map(|c| c.subtype.as_str());
    // The scoring order matters in every round.
    if choice.options.iter().any(|o| o.kind == "score") {
        return pick_score(choice, observed);
    }
    if subtype == Some("imperial_score_objective") {
        return pick_score(choice, observed);
    }
    if subtype == Some("return_over_secret_hand_limit") {
        return pick_secret_return(choice, seen);
    }
    if !active(observed) {
        return None;
    }
    match subtype {
        Some("activate_system") => return pick_activation(choice, seen, me),
        Some("movement_step") => return pick_move(choice, seen, me),
        Some("draft_strategy_card") => return pick_strategy_card(choice, seen, me),
        Some("research_technology") => return pick_research(choice, seen, me),
        Some("place_structure") => return pick_structure(choice, seen),
        _ => {}
    }
    if choice
        .options
        .iter()
        .any(|o| o.kind == "action" && o.id == "tactical")
    {
        return pick_action(choice, seen, me, rng);
    }
    let reserve = Reserve::of(seen, me);
    if reserve.is_empty() {
        return None;
    }
    if let Some(index) = pick_secondary(choice, observed, me, &reserve) {
        return Some(index);
    }
    if choice.options.iter().any(|o| o.kind == "produce") {
        return pick_produce(choice, observed, me, &reserve);
    }
    if !choice.options.is_empty() && choice.options.iter().all(|o| o.kind == "pay") {
        return pick_pay(choice, observed, &reserve);
    }
    None
}

// -- objectives in view ---------------------------------------------------------------------------

/// One objective the seat can still score: a revealed public it has not scored, or a secret it
/// holds.
#[derive(Debug, Clone, PartialEq)]
pub struct Goal {
    pub alias: String,
    pub secret: bool,
    pub points: f64,
    /// Scored by spending (61.10) rather than by meeting a requirement.
    pub bought: bool,
    pub have: f64,
    pub threshold: f64,
    pub satisfied: bool,
    pub family: String,
}

impl Goal {
    /// Steps still missing.
    #[must_use]
    pub fn gap(&self) -> f64 {
        (self.threshold - self.have).max(0.0)
    }
}

fn points_of(seen: &Observed<'_>, alias: &str) -> f64 {
    f64::from(
        ti4_engine::objectives::points_for(seen.content(), &ObjectiveId::new(alias)).unwrap_or(1),
    )
}

fn goal_of(seen: &Observed<'_>, card: CardProgress, secret: bool) -> Goal {
    let bought = ti4_engine::objectives::cost_of(&ObjectiveId::new(&card.alias)).is_some();
    Goal {
        points: if secret {
            1.0
        } else {
            points_of(seen, &card.alias)
        },
        secret,
        bought,
        have: card.have,
        threshold: card.threshold,
        satisfied: card.satisfied,
        family: card.family_token,
        alias: card.alias,
    }
}

/// Every objective the seat can still score, as it would stand after `imagined`.
#[must_use]
pub fn goals(seen: &SeatObservation<'_>, me: &PlayerId, imagined: &Imagined<'_>) -> Vec<Goal> {
    let observed = seen.observed();
    let scored = observed.scored_by(me);
    let mut out: Vec<Goal> = observed
        .revealed_objective_progress_imagining(me, imagined)
        .into_iter()
        .filter(|card| !scored.contains(&ObjectiveId::new(&card.alias)))
        .map(|card| goal_of(observed, card, false))
        .collect();
    out.extend(
        seen.held_secret_progress_imagining(imagined)
            .into_iter()
            .filter(|card| !scored.contains(&ObjectiveId::new(&card.alias)))
            .map(|card| goal_of(observed, card, true)),
    );
    out
}

/// Points of the best objective of each kind (public, secret) the seat could score this round
/// without anything more: a met requirement, or a bought objective it can afford.
fn ready_points(goals: &[Goal]) -> (f64, f64) {
    let best = |secret: bool| {
        goals
            .iter()
            .filter(|g| g.secret == secret && g.satisfied)
            .map(|g| g.points)
            .fold(0.0, f64::max)
    };
    (best(false), best(true))
}

/// What reaching `after` from `before` is worth, in activation units.
///
/// Completing an objective is worth [`COMPLETE_VALUE`] per point it adds to what the seat could
/// already score in that category this round (one public and one secret per status phase), and a
/// quarter of that when the slot is already filled (it waits for the next round). Progress toward
/// an objective at most two steps short is worth [`PROGRESS_VALUE`] per point times the share
/// gained. Bought objectives are left out: a planet taken this turn comes in exhausted.
#[must_use]
pub fn gain(before: &[Goal], after: &[Goal]) -> f64 {
    let (public_ready, secret_ready) = ready_points(before);
    let mut value = 0.0;
    for old in before.iter().filter(|g| !g.bought) {
        let Some(new) = after
            .iter()
            .find(|g| g.alias == old.alias && g.secret == old.secret)
        else {
            continue;
        };
        if old.satisfied {
            continue;
        }
        if new.satisfied {
            let ready = if old.secret {
                secret_ready
            } else {
                public_ready
            };
            let extra = (old.points - ready).max(0.0);
            value += COMPLETE_VALUE * extra.max(0.25 * old.points);
        } else if new.have > old.have && new.gap() <= 2.0 && old.threshold > 0.0 {
            value += PROGRESS_VALUE * old.points * (new.have - old.have) / old.threshold;
        }
    }
    value
}

// -- holding back what a bought objective costs ---------------------------------------------------

/// What the seat keeps unspent this action phase for the bought objective it means to score in
/// the status phase.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Reserve {
    pub resources: i64,
    pub influence: i64,
    pub trade_goods: i64,
    pub tokens: i32,
}

impl Reserve {
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.resources == 0 && self.influence == 0 && self.trade_goods == 0 && self.tokens == 0
    }

    /// The reserve for one bought objective's price.
    #[must_use]
    pub fn for_cost(cost: ti4_engine::objectives::Cost) -> Self {
        use ti4_engine::objectives::Cost;
        match cost {
            Cost::Spend {
                amount,
                kind: Spend::Resources,
            } => Self {
                resources: amount,
                ..Self::default()
            },
            Cost::Spend {
                amount,
                kind: Spend::Influence,
            } => Self {
                influence: amount,
                ..Self::default()
            },
            Cost::TradeGoods(amount) => Self {
                trade_goods: i64::from(amount),
                ..Self::default()
            },
            Cost::Tokens(amount) => Self {
                tokens: amount,
                ..Self::default()
            },
            Cost::AllThree(amount) => Self {
                resources: amount,
                influence: amount,
                trade_goods: amount,
                tokens: 0,
            },
        }
    }

    /// The seat's reserve now: in the action phase from round three, for the bought public
    /// objective it can afford with the most points, when no met requirement is worth as much.
    #[must_use]
    pub fn of(seen: &SeatObservation<'_>, me: &PlayerId) -> Self {
        let observed = seen.observed();
        if !active(observed) || observed.phase() != ti4_model::state::Phase::Action {
            return Self::default();
        }
        Self::choose(&goals(seen, me, &Imagined::NONE))
    }

    /// [`Self::of`] for a given list of goals.
    #[must_use]
    pub fn choose(goals: &[Goal]) -> Self {
        let free = goals
            .iter()
            .filter(|g| !g.secret && !g.bought && g.satisfied)
            .map(|g| g.points)
            .fold(0.0, f64::max);
        goals
            .iter()
            .filter(|g| !g.secret && g.bought && g.satisfied && g.points > free)
            .filter_map(|g| {
                let cost = ti4_engine::objectives::cost_of(&ObjectiveId::new(&g.alias))?;
                Some((g.points, cost_rank(cost), cost))
            })
            .max_by(|a, b| a.0.total_cmp(&b.0).then(b.1.cmp(&a.1)))
            .map_or_else(Self::default, |(_, _, cost)| Self::for_cost(cost))
    }
}

/// How dear a bought objective's price is beyond the status phase: a planet spend costs next to
/// nothing then (the planets are readied right after), trade goods and tokens are gone for good.
fn cost_rank(cost: ti4_engine::objectives::Cost) -> u8 {
    use ti4_engine::objectives::Cost;
    match cost {
        Cost::Spend { .. } => 0,
        Cost::AllThree(_) => 1,
        Cost::TradeGoods(_) | Cost::Tokens(_) => 2,
    }
}

/// A ready planet's (resources, influence).
fn ready_planets(seen: &Observed<'_>, me: &PlayerId) -> Vec<(i64, i64)> {
    seen.controlled_planets(me)
        .into_iter()
        .filter(|(_, planet)| seen.planet_is_ready(planet))
        .filter_map(|(_, planet)| {
            ti4_content::galaxy::planet(seen.content(), planet.as_str(), seen.sources())
        })
        .map(|record| (record.resources(), record.influence()))
        .collect()
}

/// Resources the seat can spend without breaking `reserve`: the planets the influence part does
/// not need (fewest resources lost first), and trade goods beyond what the reserve keeps.
#[must_use]
pub fn spare_resources(planets: &[(i64, i64)], trade_goods: i64, reserve: &Reserve) -> i64 {
    let (spare_planets, goods) = spare_after_influence(planets, trade_goods, reserve);
    spare_planets.iter().map(|(r, _)| r).sum::<i64>() + goods - reserve.resources
}

/// Influence the seat can spend without breaking `reserve`.
#[must_use]
pub fn spare_influence(planets: &[(i64, i64)], trade_goods: i64, reserve: &Reserve) -> i64 {
    // The resource part keeps the planets best at resources (least influence lost first).
    let mut sorted: Vec<(i64, i64)> = planets.to_vec();
    sorted.sort_by_key(|&(r, i)| (std::cmp::Reverse(r - i), i));
    let mut need = reserve.resources;
    let mut rest = Vec::new();
    for (r, i) in sorted {
        if need > 0 && r > 0 {
            need -= r;
        } else {
            rest.push((r, i));
        }
    }
    let goods = trade_goods - reserve.trade_goods - need.max(0);
    rest.iter().map(|(_, i)| i).sum::<i64>() + goods - reserve.influence
}

/// The planets left after setting aside those that pay the reserve's influence (most influence
/// for least resources first), and the trade goods left after the reserve's own and any influence
/// the planets cannot cover.
fn spare_after_influence(
    planets: &[(i64, i64)],
    trade_goods: i64,
    reserve: &Reserve,
) -> (Vec<(i64, i64)>, i64) {
    let mut sorted: Vec<(i64, i64)> = planets.to_vec();
    sorted.sort_by_key(|&(r, i)| (std::cmp::Reverse(i - r), r));
    let mut need = reserve.influence;
    let mut rest = Vec::new();
    for (r, i) in sorted {
        if need > 0 && i > 0 {
            need -= i;
        } else {
            rest.push((r, i));
        }
    }
    (rest, trade_goods - reserve.trade_goods - need.max(0))
}

fn trade_goods(seen: &Observed<'_>, me: &PlayerId) -> i64 {
    seen.seat(me).map_or(0, |seat| i64::from(seat.trade_goods))
}

fn pool_tokens(seen: &Observed<'_>, me: &PlayerId) -> i32 {
    seen.seat(me)
        .map_or(0, |seat| seat.tactic_tokens + seat.strategic_tokens)
}

/// Production within the spare resources: the rule bot's pick among what stays affordable, or
/// stop.
fn pick_produce(
    choice: &Choice,
    seen: &Observed<'_>,
    me: &PlayerId,
    reserve: &Reserve,
) -> Option<usize> {
    if reserve.resources == 0 && reserve.influence == 0 && reserve.trade_goods == 0 {
        return None;
    }
    #[expect(clippy::cast_precision_loss, reason = "a handful of resources")]
    let spare = spare_resources(&ready_planets(seen, me), trade_goods(seen, me), reserve) as f64;
    let keep: Vec<usize> = choice
        .options
        .iter()
        .enumerate()
        .filter(|(_, o)| {
            o.kind != "produce"
                || payload_f64(o, "owed").is_none_or(|owed| owed <= 0.0 || owed <= spare)
        })
        .map(|(i, _)| i)
        .collect();
    if keep.len() == choice.options.len() {
        return None;
    }
    let mut narrowed = choice.clone();
    narrowed.options = keep.iter().map(|&i| choice.options[i].clone()).collect();
    if !narrowed.options.iter().any(|o| o.kind == "produce") {
        return choice.options.iter().position(ChoiceOption::is_decline);
    }
    super::pick_produce(&narrowed, seen, me).and_then(|i| keep.get(i).copied())
}

/// Payment keeping the reserve: a planet's value of the other kind counts in full while the
/// reserve needs that kind, and trade goods are spent last while it needs them.
fn pick_pay(choice: &Choice, seen: &Observed<'_>, reserve: &Reserve) -> Option<usize> {
    let resources = choice
        .options
        .iter()
        .find_map(|o| payload_str(o, "kind"))
        .is_none_or(|kind| kind == "resources");
    let keeps_other = if resources {
        reserve.influence > 0
    } else {
        reserve.resources > 0
    };
    let weight = if keeps_other { 1.5 } else { 0.2 };
    let goods_penalty = if reserve.trade_goods > 0 { 10.0 } else { 0.0 };
    choice
        .options
        .iter()
        .enumerate()
        .filter_map(|(i, o)| {
            let worth = payload_f64(o, "worth")?;
            let owed = payload_f64(o, "owed")?;
            let other = match o.id.strip_prefix("exhaust|") {
                Some(rest) => {
                    let planet = rest.split('|').next().unwrap_or(rest);
                    ti4_content::galaxy::planet(seen.content(), planet, seen.sources()).map_or(
                        0.0,
                        |record| {
                            #[expect(clippy::cast_precision_loss, reason = "single digits")]
                            let value = if resources {
                                record.influence()
                            } else {
                                record.resources()
                            } as f64;
                            weight * value
                        },
                    )
                }
                None => worth + goods_penalty,
            };
            Some((i, (worth - owed).max(0.0) + other, worth))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1).then(b.2.total_cmp(&a.2)))
        .map(|(i, _, _)| i)
}

/// Secondaries that would spend what the reserve keeps: declined.
fn pick_secondary(
    choice: &Choice,
    seen: &Observed<'_>,
    me: &PlayerId,
    reserve: &Reserve,
) -> Option<usize> {
    let no = choice.options.iter().position(|o| o.id == "no")?;
    choice.options.iter().position(|o| o.id == "yes")?;
    let prompt = choice.prompt.as_str();
    let planets = ready_planets(seen, me);
    let goods = trade_goods(seen, me);
    if reserve.tokens > 0
        && prompt.starts_with("spend a strategy token")
        && pool_tokens(seen, me) - 1 < reserve.tokens
    {
        return Some(no);
    }
    if prompt.starts_with("spend a strategy token and 4 resources to research")
        && spare_resources(&planets, goods, reserve) < 4
    {
        return Some(no);
    }
    if prompt.contains("influence for a command token")
        && reserve.tokens == 0
        && spare_influence(&planets, goods, reserve)
            < ti4_engine::strategy_cards::INFLUENCE_PER_TOKEN
    {
        return Some(no);
    }
    None
}

// -- scoring -------------------------------------------------------------------------------------

/// The order objectives are scored in: points, then a requirement before a bought objective,
/// then the cheapest price.
fn score_rank(seen: &Observed<'_>, alias: &str) -> (i64, u8) {
    let id = ObjectiveId::new(alias);
    #[expect(clippy::cast_possible_truncation, reason = "printed points")]
    let points = points_of(seen, alias) as i64;
    let price = ti4_engine::objectives::cost_of(&id).map_or(0, |cost| 1 + cost_rank(cost));
    (points, price)
}

fn pick_score(choice: &Choice, seen: &Observed<'_>) -> Option<usize> {
    choice
        .options
        .iter()
        .enumerate()
        .filter(|(_, o)| !o.is_decline())
        .max_by(|(ia, a), (ib, b)| {
            let (pa, ca) = score_rank(seen, &a.id);
            let (pb, cb) = score_rank(seen, &b.id);
            pa.cmp(&pb).then(cb.cmp(&ca)).then(ib.cmp(ia))
        })
        .map(|(i, _)| i)
}

/// Over the secret hand limit: the secret furthest from done goes back (one whose progress the
/// engine cannot count, an event secret, counts as a quarter done).
fn pick_secret_return(choice: &Choice, seen: &SeatObservation<'_>) -> Option<usize> {
    let progress = seen.held_secret_progress();
    choice
        .options
        .iter()
        .enumerate()
        .map(|(i, o)| {
            let done = progress
                .iter()
                .find(|p| p.alias == o.id)
                .map_or(0.25, |p| (p.have / p.threshold.max(1.0)).min(1.0));
            (i, done)
        })
        .min_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)))
        .map(|(i, _)| i)
}

// -- the action phase ----------------------------------------------------------------------------

/// Ground forces the seat could have in `system` after activating it, and whether a ship could
/// get there (the same reach as [`activation_value`]).
fn reach(seen: &Observed<'_>, me: &PlayerId, system: &SystemId) -> (bool, usize) {
    let movable = seen.movable_into(me, system);
    let ships = movable.iter().any(|m| is_ship(seen, &m.unit));
    let capacity: i64 = movable
        .iter()
        .filter(|m| m.capacity > 0 && is_ship(seen, &m.unit))
        .map(|m| m.capacity)
        .sum();
    let origins: BTreeSet<&SystemId> = movable
        .iter()
        .filter(|m| m.capacity > 0 && is_ship(seen, &m.unit))
        .map(|m| &m.origin)
        .collect();
    let loadable: usize = origins
        .into_iter()
        .map(|origin| {
            seen.loadable(me, origin)
                .iter()
                .filter(|cargo| is_ground(seen, &cargo.unit))
                .count()
        })
        .sum();
    let there = seen
        .system(system)
        .units
        .iter()
        .filter(|u| &u.owner == me && is_ground(seen, u))
        .count();
    (
        ships,
        loadable.min(usize::try_from(capacity).unwrap_or(0)) + there,
    )
}

/// The planets an activation of `system` would take, greedily as [`super::takeable_value`] does.
fn taken_planets(
    seen: &Observed<'_>,
    me: &PlayerId,
    system: &SystemId,
    ground: usize,
) -> Vec<PlanetId> {
    let mut left = ground;
    let mut out = Vec::new();
    for (planet, _, need) in targets(seen, me, system) {
        if need <= left {
            left -= need;
            out.push(planet);
        }
    }
    out
}

/// A rival this close to winning, with its points.
fn rivals_about_to_win(seen: &Observed<'_>, me: &PlayerId) -> Vec<PlayerId> {
    seen.players()
        .into_iter()
        .filter(|p| *p != me)
        .filter(|p| {
            seen.seat(p)
                .is_some_and(|seat| seat.victory_points >= WIN - DENY_WITHIN)
        })
        .cloned()
        .collect()
}

/// Whether the seat has planets in `system` or it neighbours its home: a hostile fleet there is a
/// threat worth fighting.
fn threatens_own(seen: &Observed<'_>, me: &PlayerId, system: &SystemId) -> bool {
    if seen
        .system(system)
        .planet_control
        .values()
        .any(|owner| owner == me)
    {
        return true;
    }
    let (Some(galaxy), Some(home)) = (seen.galaxy(), home_system(seen, me)) else {
        return false;
    };
    &home == system
        || galaxy
            .adjacent(home.as_str())
            .iter()
            .any(|s| *s == system.as_str())
}

/// One system's worth to activate from round three: the rule bot's value, plus what it does for
/// the objectives, plus planets taken from a rival about to win; minus a fight that wins nothing.
fn system_value(
    seen: &SeatObservation<'_>,
    me: &PlayerId,
    system: &SystemId,
    before: &[Goal],
    leaders: &[PlayerId],
) -> f64 {
    let observed = seen.observed();
    let base = activation_value(observed, me, system);
    if base < 0.0 {
        return base;
    }
    let (ships, ground) = reach(observed, me, system);
    let present = observed
        .system(system)
        .units
        .iter()
        .any(|u| &u.owner == me && is_ship(observed, u));
    if !ships && !present && ground == 0 {
        return base;
    }
    let planets = taken_planets(observed, me, system, ground);
    let systems = if ships && !present {
        vec![system.to_string()]
    } else {
        Vec::new()
    };
    let objective = if planets.is_empty() && systems.is_empty() {
        0.0
    } else {
        let after = goals(
            seen,
            me,
            &Imagined {
                planets: &planets,
                systems: &systems,
                ..Imagined::default()
            },
        );
        gain(before, &after)
    };
    let board = observed.system(system);
    let denied = planets
        .iter()
        .filter(|planet| {
            board
                .planet_control
                .get(*planet)
                .is_some_and(|owner| leaders.contains(owner))
        })
        .count();
    #[expect(clippy::cast_precision_loss, reason = "a handful of planets")]
    let deny = DENY_VALUE * denied as f64;
    let mut value = base + objective + deny;
    // A fight that takes nothing and protects nothing is not worth the ships any more.
    if has_hostile_ships(observed, me, system)
        && planets.is_empty()
        && objective <= 0.0
        && !threatens_own(observed, me, system)
    {
        value = 0.0;
    }
    value
}

/// Every system the seat could activate (no token of its own), valued by [`system_value`].
fn values<'s>(
    seen: &SeatObservation<'_>,
    me: &PlayerId,
    systems: impl IntoIterator<Item = &'s SystemId>,
) -> Vec<(SystemId, f64)> {
    let before = goals(seen, me, &Imagined::NONE);
    let leaders = rivals_about_to_win(seen.observed(), me);
    systems
        .into_iter()
        .map(|system| {
            (
                system.clone(),
                system_value(seen, me, system, &before, &leaders),
            )
        })
        .collect()
}

fn pick_activation(choice: &Choice, seen: &SeatObservation<'_>, me: &PlayerId) -> Option<usize> {
    seen.galaxy()?;
    let ids: Vec<SystemId> = choice
        .options
        .iter()
        .filter(|o| o.kind == "activate")
        .map(|o| SystemId::new(&o.id))
        .collect();
    let valued = values(seen, me, &ids);
    let scored: Vec<(usize, f64)> = choice
        .options
        .iter()
        .enumerate()
        .filter(|(_, o)| o.kind == "activate")
        .filter_map(|(i, o)| {
            valued
                .iter()
                .find(|(s, _)| s.as_str() == o.id)
                .map(|(_, v)| (i, *v))
        })
        .collect();
    // Nothing worth a token (an empty fight scores zero now): the least harmful system rather
    // than the rule bot's own pick, which would take that fight.
    best_above(&scored, ACTIVATION_THRESHOLD).or_else(|| best_above(&scored, 0.0))
}

/// Action phase from round three: Imperial at once when it scores a public objective; a tactical
/// action when a system is worth one (objectives counted, empty fights not); with the reserve's
/// tokens at stake only for a system worth [`EAGER_WORTH`]; otherwise the card or a pass.
fn pick_action(
    choice: &Choice,
    seen: &SeatObservation<'_>,
    me: &PlayerId,
    rng: &mut ChaCha8Rng,
) -> Option<usize> {
    let observed = seen.observed();
    let galaxy = observed.galaxy()?;
    let tactical = choice.options.iter().position(|o| o.id == "tactical")?;
    let play = |name: &str| {
        ready_card(observed, me, name).and_then(|card| strategic_option(choice, observed, &card))
    };
    if observed.scoreable_public(me) > 0
        && let Some(index) = play("Imperial")
    {
        return Some(index);
    }
    let out_of_tokens = observed
        .seat(me)
        .is_some_and(|seat| seat.tactic_tokens <= 0);
    if out_of_tokens {
        return None;
    }
    let tokens = observed.systems_with_token(me);
    let systems: Vec<SystemId> = galaxy
        .system_ids()
        .into_iter()
        .map(SystemId::new)
        .filter(|system| !tokens.contains(system))
        .collect();
    let worth = values(seen, me, &systems)
        .into_iter()
        .map(|(_, v)| v)
        .fold(f64::NEG_INFINITY, f64::max);
    let hold_imperial = !holds_mecatol(observed, me) && mecatol_in_reach(observed, me);
    let strategic = choice
        .options
        .iter()
        .enumerate()
        .filter(|(_, o)| o.kind == "action" && o.id.starts_with("strategic"))
        .min_by_key(|(i, o)| {
            let imperial =
                o.id.strip_prefix("strategic|")
                    .is_some_and(|card| card_is(observed, card, "Imperial"));
            (hold_imperial && imperial, *i)
        })
        .map(|(i, _)| i);
    let reserve = Reserve::of(seen, me);
    let keep_tokens = reserve.tokens > 0 && pool_tokens(observed, me) - 1 < reserve.tokens;
    let needed = if keep_tokens {
        EAGER_WORTH * 2.0
    } else {
        ACTIVATION_THRESHOLD
    };
    if worth < needed {
        return strategic.or_else(|| choice.options.iter().position(|o| o.id == "pass"));
    }
    match strategic {
        Some(index) if worth < EAGER_WORTH && rng.random_bool(0.25) => Some(index),
        _ => Some(tactical),
    }
}

/// Presence objectives whose fixed board shape is known ([`Observed::objectives_implicating_system`]):
/// with units in exactly as many of those systems as needed, the last one in each stays.
const SHAPED: [(&str, bool); 3] = [
    ("intimidate", true),
    ("outer_rim", false),
    ("control_borderlands", false),
];

/// Whether moving one ship out of `origin` would break a met, unscored objective: the last ship
/// in a system of a just-met shaped objective (or the last unit of any kind, for the rim), or
/// the last ship anywhere while Command the Seas style "ships in N systems" is just met and the
/// active system already has a ship of its own (so the count would drop).
fn breaks_a_met_objective(
    seen: &Observed<'_>,
    me: &PlayerId,
    origin: &SystemId,
    active: &SystemId,
    goals: &[Goal],
) -> bool {
    let board = seen.system(origin);
    let ships = board
        .units
        .iter()
        .filter(|u| &u.owner == me && is_ship(seen, u))
        .count();
    if ships > 1 {
        return false;
    }
    let ground = board
        .planet_units
        .values()
        .flatten()
        .any(|u| &u.owner == me);
    let fragile = |alias: &str| {
        goals
            .iter()
            .any(|g| g.alias == alias && g.satisfied && g.have <= g.threshold)
    };
    let implicated = seen.objectives_implicating_system(me, origin);
    for (alias, ships_only) in SHAPED {
        if fragile(alias)
            && implicated.iter().any(|o| o.as_str() == alias)
            && (ships_only || !ground)
        {
            return true;
        }
    }
    // Other ship-placement objectives (ships in N systems, beside a rival's home or an anomaly,
    // neighbours, ...): the board shape is not known here, so while one is just met and is what
    // the seat would score in its category, no lone ship leaves -- unless it only moves into a
    // system it has no ship in yet, which keeps a count of systems whole.
    let active_has_ship = seen
        .system(active)
        .units
        .iter()
        .any(|u| &u.owner == me && is_ship(seen, u));
    let held = protected(goals);
    held.iter().any(|g| {
        PRESENCE_FAMILIES.contains(&g.family.as_str())
            && (active_has_ship || g.family != "ships_in_systems")
    })
}

/// Requirement families met by where ships stand.
const PRESENCE_FAMILIES: [&str; 8] = [
    "ships_in_systems",
    "ship_systems_beside_anomaly",
    "ship_systems_beside_rival_home",
    "ships_adjacent_to_mecatol",
    "rival_dock_systems_with_ships",
    "neighbours",
    "planetless_systems",
    "capital_ship_systems",
];

/// The met, unscored, just-met objectives that are what the seat would score in their category
/// this round: no other met objective of the same category (public or secret) is worth as much.
fn protected(goals: &[Goal]) -> Vec<&Goal> {
    goals
        .iter()
        .filter(|g| g.satisfied && !g.bought && g.have <= g.threshold)
        .filter(|g| {
            !goals.iter().any(|other| {
                other.secret == g.secret
                    && other.alias != g.alias
                    && other.satisfied
                    && other.points >= g.points
                    && (other.bought || other.have > other.threshold)
            })
        })
        .collect()
}

/// Movement from round three: never the ship that holds a met objective
/// ([`breaks_a_met_objective`]); into a system activated for a presence objective, the cheapest
/// ship ([`pick_presence_move`]); otherwise the rule bot's movement among the moves left.
fn pick_move(choice: &Choice, seen: &SeatObservation<'_>, me: &PlayerId) -> Option<usize> {
    let observed = seen.observed();
    let active = observed.active_system()?.clone();
    let goals = goals(seen, me, &Imagined::NONE);
    let keep: Vec<usize> = choice
        .options
        .iter()
        .enumerate()
        .filter(|(_, o)| {
            o.kind != "move"
                || !payload_str(o, "origin").is_some_and(|origin| {
                    payload_str(o, "unit")
                        .and_then(|unit| unit_type(observed, unit))
                        .is_some_and(|t| t.is_ship())
                        && breaks_a_met_objective(
                            observed,
                            me,
                            &SystemId::new(origin),
                            &active,
                            &goals,
                        )
                })
        })
        .map(|(i, _)| i)
        .collect();
    if keep.len() == choice.options.len() {
        return pick_presence_move(choice, seen, me);
    }
    let mut narrowed = choice.clone();
    narrowed.options = keep.iter().map(|&i| choice.options[i].clone()).collect();
    let inner = pick_presence_move(&narrowed, seen, me)
        .or_else(|| super::pick_move(&narrowed, observed, me));
    inner
        .and_then(|i| keep.get(i).copied())
        .or_else(|| choice.options.iter().position(ChoiceOption::is_decline))
}

/// Movement into a system activated for a presence objective (no planet to take, no fight, no
/// dock, no ship of its own there yet): the cheapest ship whose origin keeps another ship.
fn pick_presence_move(choice: &Choice, seen: &SeatObservation<'_>, me: &PlayerId) -> Option<usize> {
    let observed = seen.observed();
    let active = observed.active_system()?.clone();
    let board = observed.system(&active);
    if has_hostile_ships(observed, me, &active)
        || !targets(observed, me, &active).is_empty()
        || board
            .units
            .iter()
            .any(|u| &u.owner == me && is_ship(observed, u))
    {
        return None;
    }
    let before = goals(seen, me, &Imagined::NONE);
    let systems = vec![active.to_string()];
    let after = goals(
        seen,
        me,
        &Imagined {
            systems: &systems,
            ..Imagined::default()
        },
    );
    if gain(&before, &after) <= 0.0 {
        return None;
    }
    let ships_at = |origin: &str| {
        observed
            .system(&SystemId::new(origin))
            .units
            .iter()
            .filter(|u| &u.owner == me && is_ship(observed, u))
            .count()
    };
    choice
        .options
        .iter()
        .enumerate()
        .filter(|(_, o)| o.kind == "move")
        .filter_map(|(i, o)| {
            let kind = unit_type(observed, payload_str(o, "unit")?)?;
            if !kind.is_ship() || kind.is_fighter() {
                return None;
            }
            let alone = payload_str(o, "origin").is_some_and(|origin| ships_at(origin) <= 1);
            #[expect(clippy::cast_possible_truncation, reason = "printed costs, tenths")]
            let cost = (kind.cost() * 10.0).round() as i64;
            Some((i, (alone, cost)))
        })
        .min_by_key(|&(i, key)| (key, i))
        .map(|(i, _)| i)
}

// -- strategy, research, structures ---------------------------------------------------------------

/// Families a strategy card moves: Technology researches, Construction places structures,
/// Leadership gains tokens, Trade gains trade goods.
fn card_families(name: &str) -> &'static [&'static str] {
    match name {
        "Technology" => &[
            "unit_upgrades",
            "colours_2",
            "same_colour_technologies",
            "faction_technologies",
        ],
        "Construction" => &[
            "structures",
            "structures_away",
            "units_pds",
            "units_spacedock",
        ],
        "Leadership" => &["cost_tokens"],
        "Trade" => &["cost_trade_goods", "cost_all_three"],
        _ => &[],
    }
}

/// What a strategy card is worth from round three: Imperial when it scores (Mecatol Rex held or
/// takeable, or a public objective met or affordable now, to score one at once and another in the
/// status phase), a card that serves an objective within two steps, and trade goods on the card.
fn card_score(seen: &SeatObservation<'_>, me: &PlayerId, card: &str, goals: &[Goal]) -> f64 {
    let observed = seen.observed();
    let name = ti4_engine::strategy_cards::card_name(observed.content(), card).unwrap_or_default();
    let goods = f64::from(super::card_goods(observed, card));
    let mut score = goods;
    if name == "Imperial" {
        if holds_mecatol(observed, me) || mecatol_takeable(observed, me) {
            score += 30.0;
        }
        let public_ready = goals.iter().any(|g| !g.secret && g.satisfied);
        if public_ready {
            score += 12.0;
        }
    }
    for family in card_families(&name) {
        let helps: f64 = goals
            .iter()
            .filter(|g| g.family == *family && !g.satisfied && g.gap() <= 2.0)
            .map(|g| g.points)
            .sum();
        score += 4.0 * helps;
    }
    score
}

fn pick_strategy_card(choice: &Choice, seen: &SeatObservation<'_>, me: &PlayerId) -> Option<usize> {
    let goals = goals(seen, me, &Imagined::NONE);
    let scored: Vec<(usize, f64)> = choice
        .options
        .iter()
        .enumerate()
        .map(|(i, o)| (i, card_score(seen, me, &o.id, &goals)))
        .collect();
    // Without an objective or trade goods to tip it, the rule bot's own draft decides.
    if scored.iter().all(|(_, s)| *s < 1.0) {
        return None;
    }
    best_above(&scored, 1.0)
}

/// Research: declined when it costs what the reserve keeps (the Technology primary's second
/// research, paid from planets the engine picks); otherwise the technology that moves an
/// objective most; none, no opinion.
fn pick_research(choice: &Choice, seen: &SeatObservation<'_>, me: &PlayerId) -> Option<usize> {
    let observed = seen.observed();
    let reserve = Reserve::of(seen, me);
    if !reserve.is_empty()
        && let Some(decline) = choice.options.iter().position(ChoiceOption::is_decline)
    {
        let cost = choice
            .options
            .iter()
            .filter_map(|o| payload_f64(o, "cost"))
            .fold(0.0, f64::max);
        let spare = spare_resources(
            &ready_planets(observed, me),
            trade_goods(observed, me),
            &reserve,
        );
        #[expect(clippy::cast_precision_loss, reason = "a handful of resources")]
        if cost > 0.0 && cost > spare as f64 {
            return Some(decline);
        }
    }
    let me = me.clone();
    let before = goals(seen, &me, &Imagined::NONE);
    let scored: Vec<(usize, f64)> = choice
        .options
        .iter()
        .enumerate()
        .filter(|(_, o)| !o.is_decline())
        .map(|(i, o)| {
            let technologies = vec![o.id.clone()];
            let after = goals(
                seen,
                &me,
                &Imagined {
                    technologies: &technologies,
                    ..Imagined::default()
                },
            );
            (i, gain(&before, &after))
        })
        .collect();
    best_above(&scored, 0.01)
}

/// Structure placement (`kind|system|planet`): the one that moves an objective most, a PDS or a
/// dock counted for secrets that count them; none, no opinion.
fn pick_structure(choice: &Choice, seen: &SeatObservation<'_>) -> Option<usize> {
    let me = seen.bound_seat().clone();
    let before = goals(seen, &me, &Imagined::NONE);
    let scored: Vec<(usize, f64)> = choice
        .options
        .iter()
        .enumerate()
        .filter_map(|(i, o)| {
            let mut parts = o.id.split('|');
            let (kind, system, planet) = (parts.next()?, parts.next()?, parts.next()?);
            let structures = vec![(system.to_owned(), planet.to_owned())];
            let after = goals(
                seen,
                &me,
                &Imagined {
                    structures: &structures,
                    ..Imagined::default()
                },
            );
            let units: f64 = before
                .iter()
                .filter(|g| !g.satisfied && g.family == format!("units_{kind}"))
                .map(|g| {
                    if g.gap() <= 1.0 {
                        COMPLETE_VALUE * g.points
                    } else {
                        PROGRESS_VALUE * g.points / g.threshold.max(1.0)
                    }
                })
                .sum();
            Some((i, gain(&before, &after) + units))
        })
        .collect();
    best_above(&scored, 0.01)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use ti4_content::ContentStore;
    use ti4_engine::choice::{Decider, IllegalChoice};
    use ti4_engine::fixtures::{Hub, game, hub_from, plain_systems, put};
    use ti4_model::POK;
    use ti4_model::state::{GameState, Phase};

    fn me() -> PlayerId {
        PlayerId::new("a")
    }

    fn foe() -> PlayerId {
        PlayerId::new("b")
    }

    fn goal(alias: &str, points: f64, bought: bool, have: f64, threshold: f64) -> Goal {
        Goal {
            alias: alias.to_owned(),
            secret: false,
            points,
            bought,
            have,
            threshold,
            satisfied: have >= threshold,
            family: String::new(),
        }
    }

    fn planets(system: &str) -> Vec<ti4_content::galaxy::Planet<'static>> {
        ti4_content::galaxy::planets_in(ContentStore::embedded(), system, POK)
    }

    /// One-planet systems (no home, no Mecatol) whose planet passes `keep`, in corpus order.
    fn one_planet_systems(keep: impl Fn(&ti4_content::galaxy::Planet<'_>) -> bool) -> Vec<String> {
        plain_systems(200)
            .into_iter()
            .filter(|id| {
                let p = planets(id);
                p.len() == 1
                    && p[0].id() != "mr"
                    && p[0].homeworld_of().is_none()
                    && p[0].traits().len() == 1
                    && keep(&p[0])
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

    /// Round three, action phase, with nothing revealed and nothing held.
    fn round_three(state: &mut GameState) {
        state.round = 3;
        state.phase = Phase::Action;
        state.revealed_objectives.clear();
        for seat in &mut state.players {
            seat.secret_objectives.clear();
        }
    }

    fn contextual(choice: Choice, subtype: &str, round: u32) -> Choice {
        choice.contextualized(ti4_engine::decision_context::DecisionContext::new(
            me(),
            ti4_engine::decision_context::DecisionSource::Rule("test".to_owned()),
            subtype,
            Phase::Action,
            round,
        ))
    }

    /// What the rule bot (objective layer first) answers, through the engine's private ask.
    struct Probe(Option<usize>);
    impl Decider for Probe {
        fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
            Ok(choice.options[0].clone())
        }
        fn choose_seeing(
            &mut self,
            choice: &Choice,
            seen: &SeatObservation<'_>,
        ) -> Result<ChoiceOption, IllegalChoice> {
            self.0 = super::super::pick(choice, seen, &mut ChaCha8Rng::seed_from_u64(1));
            Ok(choice.options[0].clone())
        }
    }

    fn picked(state: &GameState, hub: Option<&Hub>, choice: &Choice) -> Option<usize> {
        let mut probe = Probe(None);
        ti4_engine::choice::ask_private(
            choice,
            state,
            ContentStore::embedded(),
            POK,
            hub.map(|h| &h.galaxy),
            &mut probe,
        )
        .expect("legal");
        probe.0
    }

    fn score_choice(aliases: &[&str]) -> Choice {
        let mut options: Vec<ChoiceOption> = aliases
            .iter()
            .map(|a| ChoiceOption::labelled(*a, "score", *a))
            .collect();
        options.push(ChoiceOption::decline());
        Choice::new(me(), "score an objective", options)
    }

    #[test]
    fn scoring_takes_the_most_points_then_a_requirement_then_the_cheapest_price() {
        let state = game(&["a", "b"]);
        let seen = Observed::new(&state, ContentStore::embedded(), POK, None);
        // Stage II (two points) before stage I.
        assert_eq!(
            pick_score(&score_choice(&["corner", "subdue"]), &seen),
            Some(1)
        );
        // A requirement before a bought objective, a planet spend before trade goods.
        assert_eq!(
            pick_score(&score_choice(&["trade_routes", "corner"]), &seen),
            Some(1)
        );
        assert_eq!(
            pick_score(&score_choice(&["trade_routes", "monument"]), &seen),
            Some(1)
        );
        // Never the decline while something scores.
        assert_eq!(pick_score(&score_choice(&["lead"]), &seen), Some(0));
    }

    #[test]
    fn the_reserve_keeps_the_price_of_the_best_affordable_bought_objective() {
        // Erect a Monument affordable, nothing met: keep eight resources.
        let goals = vec![
            goal("monument", 1.0, true, 8.0, 8.0),
            goal("corner", 1.0, false, 2.0, 4.0),
        ];
        assert_eq!(
            Reserve::choose(&goals),
            Reserve {
                resources: 8,
                ..Reserve::default()
            }
        );
        // A met requirement worth as much: nothing to keep.
        let goals = vec![
            goal("monument", 1.0, true, 8.0, 8.0),
            goal("corner", 1.0, false, 4.0, 4.0),
        ];
        assert!(Reserve::choose(&goals).is_empty());
        // A stage II purchase beats a stage I requirement.
        let goals = vec![
            goal("golden_age", 2.0, true, 16.0, 16.0),
            goal("corner", 1.0, false, 4.0, 4.0),
        ];
        assert_eq!(Reserve::choose(&goals).resources, 16);
        // Not affordable: nothing to keep.
        let goals = vec![goal("sway_council", 1.0, true, 5.0, 8.0)];
        assert!(Reserve::choose(&goals).is_empty());
        // Between equal points the planet spend, whose planets are readied after scoring.
        let goals = vec![
            goal("trade_routes", 1.0, true, 5.0, 5.0),
            goal("sway_council", 1.0, true, 8.0, 8.0),
        ];
        assert_eq!(Reserve::choose(&goals).influence, 8);
        assert_eq!(Reserve::choose(&goals).trade_goods, 0);
    }

    #[test]
    fn spare_resources_and_influence_leave_the_reserve_untouched() {
        // (resources, influence)
        let planets = [(3, 0), (0, 3), (2, 2), (1, 4)];
        let none = Reserve::default();
        assert_eq!(spare_resources(&planets, 1, &none), 7);
        // Six resources kept: one spare.
        let res = Reserve {
            resources: 6,
            ..Reserve::default()
        };
        assert_eq!(spare_resources(&planets, 1, &res), 1);
        // Seven influence kept: the influence planets (0,3) and (1,4) pay it, the rest is spare.
        let inf = Reserve {
            influence: 7,
            ..Reserve::default()
        };
        assert_eq!(spare_resources(&planets, 1, &inf), 3 + 2 + 1);
        // Trade goods kept are not spare.
        let goods = Reserve {
            trade_goods: 1,
            ..Reserve::default()
        };
        assert_eq!(spare_resources(&planets, 1, &goods), 6);
        // Influence with six resources kept: (3,0), (2,2) and (1,4) pay them, (0,3) is spare.
        assert_eq!(spare_influence(&planets, 0, &res), 3);
        assert_eq!(spare_influence(&planets, 2, &none), 9 + 2);
    }

    #[test]
    fn completing_an_objective_is_worth_most_when_its_slot_is_still_empty() {
        let before = vec![goal("corner", 1.0, false, 3.0, 4.0)];
        let after = vec![goal("corner", 1.0, false, 4.0, 4.0)];
        assert!((gain(&before, &after) - COMPLETE_VALUE).abs() < 1e-9);
        // Another public is already met: the new one waits a round.
        let mut before_full = before.clone();
        before_full.push(goal("expand_borders", 1.0, false, 6.0, 6.0));
        let mut after_full = after.clone();
        after_full.push(goal("expand_borders", 1.0, false, 6.0, 6.0));
        assert!((gain(&before_full, &after_full) - 0.25 * COMPLETE_VALUE).abs() < 1e-9);
        // Progress within two steps counts a share; further away it does not.
        let near = gain(
            &[goal("subdue", 2.0, false, 8.0, 11.0)],
            &[goal("subdue", 2.0, false, 9.0, 11.0)],
        );
        assert!(near > 0.0 && near < COMPLETE_VALUE);
        let far = gain(
            &[goal("subdue", 2.0, false, 2.0, 11.0)],
            &[goal("subdue", 2.0, false, 3.0, 11.0)],
        );
        assert!(far.abs() < 1e-9);
        // Bought objectives are not moved by taking planets.
        let bought = gain(
            &[goal("monument", 1.0, true, 7.0, 8.0)],
            &[goal("monument", 1.0, true, 8.0, 8.0)],
        );
        assert!(bought.abs() < 1e-9);
    }

    #[test]
    fn a_just_met_objective_is_held_only_when_it_is_what_the_round_would_score() {
        let mut ships = goal("intimidate", 1.0, false, 2.0, 2.0);
        ships.family = "ships_adjacent_to_mecatol".to_owned();
        // Just met and nothing else: held.
        assert_eq!(protected(&[ships.clone()]).len(), 1);
        // Met with a ship to spare: nothing to hold.
        let mut spare = ships.clone();
        spare.have = 3.0;
        assert!(protected(&[spare]).is_empty());
        // Another public met with room to spare and as many points: that one scores instead.
        let corner = goal("corner", 1.0, false, 5.0, 4.0);
        assert!(protected(&[ships.clone(), corner]).is_empty());
        // A secret does not stand in for a public.
        let mut secret = goal("faa", 1.0, false, 5.0, 4.0);
        secret.secret = true;
        assert_eq!(protected(&[ships, secret]).len(), 1);
    }

    fn produce(unit: &str, count: i64, owed: i64) -> ChoiceOption {
        ChoiceOption::labelled(format!("build|{unit}|{count}"), "produce", unit)
            .with("unit", unit)
            .with("count", count)
            .with("owed", owed)
            .with("cost", owed)
            .with("available_resources", 9i64)
            .with("production_spent", count)
    }

    #[test]
    fn production_keeps_what_an_affordable_monument_costs() {
        let mut state = game(&["a", "b"]);
        round_three(&mut state);
        state.revealed_objectives.push(ObjectiveId::new("monument"));
        state.player_mut(&me()).expect("seat").trade_goods = 9;
        let choice = Choice::new(
            me(),
            "produce",
            vec![
                produce("cruiser", 1, 2),
                produce("infantry", 2, 1),
                ChoiceOption::decline(),
            ],
        );
        // Nine to spend, eight kept: the cruiser is out of reach, the infantry is not.
        assert_eq!(picked(&state, None, &choice), Some(1));
        // One trade good fewer: nothing is spare, production stops.
        state.player_mut(&me()).expect("seat").trade_goods = 8;
        assert_eq!(picked(&state, None, &choice), Some(2));
        // Before round three there is no reserve: the cruiser can be bought.
        state.round = 2;
        assert_ne!(picked(&state, None, &choice), Some(2));
    }

    #[test]
    fn research_secondary_is_declined_when_it_would_spend_the_monument() {
        let mut state = game(&["a", "b"]);
        round_three(&mut state);
        state.revealed_objectives.push(ObjectiveId::new("monument"));
        state.player_mut(&me()).expect("seat").trade_goods = 10;
        let choice = Choice::new(
            me(),
            "spend a strategy token and 4 resources to research a technology?",
            vec![
                ChoiceOption::labelled("yes", "secondary", "yes"),
                ChoiceOption::labelled("no", "secondary", "no"),
            ],
        );
        assert_eq!(picked(&state, None, &choice), Some(1));
        state.player_mut(&me()).expect("seat").trade_goods = 12;
        assert_eq!(picked(&state, None, &choice), Some(0));
    }

    #[test]
    fn a_paid_research_is_declined_when_it_would_spend_the_monument() {
        let mut state = game(&["a", "b"]);
        round_three(&mut state);
        state.revealed_objectives.push(ObjectiveId::new("monument"));
        state.player_mut(&me()).expect("seat").trade_goods = 10;
        let choice = contextual(
            Choice::new(
                me(),
                "research a technology",
                vec![
                    ChoiceOption::labelled("amd", "research", "amd").with("cost", 6i64),
                    ChoiceOption::decline(),
                ],
            ),
            "research_technology",
            3,
        );
        assert_eq!(picked(&state, None, &choice), Some(1));
        state.player_mut(&me()).expect("seat").trade_goods = 14;
        assert_ne!(picked(&state, None, &choice), Some(1));
    }

    #[test]
    fn a_strategy_card_that_serves_an_objective_within_reach_is_drafted() {
        let mut state = game(&["a", "b"]);
        round_three(&mut state);
        state.phase = Phase::Strategy;
        // Five trade goods for Trade Routes, three held: Trade is the card.
        state
            .revealed_objectives
            .push(ObjectiveId::new("trade_routes"));
        state.player_mut(&me()).expect("seat").trade_goods = 3;
        let draft = contextual(
            Choice::new(
                me(),
                "choose a strategy card",
                ["pok1leadership", "pok5trade", "pok6warfare"]
                    .iter()
                    .map(|id| ChoiceOption::labelled(*id, "strategy_card", *id))
                    .collect(),
            ),
            "draft_strategy_card",
            3,
        );
        assert_eq!(picked(&state, None, &draft), Some(1));
    }

    #[test]
    fn imperial_is_drafted_when_a_public_objective_is_already_met() {
        let mut state = game(&["a", "b"]);
        round_three(&mut state);
        state.phase = Phase::Strategy;
        state
            .revealed_objectives
            .push(ObjectiveId::new("trade_routes"));
        state.player_mut(&me()).expect("seat").trade_goods = 6;
        let draft = contextual(
            Choice::new(
                me(),
                "choose a strategy card",
                ["pok1leadership", "pok5trade", "pok8imperial"]
                    .iter()
                    .map(|id| ChoiceOption::labelled(*id, "strategy_card", *id))
                    .collect(),
            ),
            "draft_strategy_card",
            3,
        );
        assert_eq!(picked(&state, None, &draft), Some(2));
    }

    /// A ring map: the centre and `outer[1]` each hold one planet, `outer[0]` is where the bot's
    /// carrier waits with two infantry, the rest are empty.
    fn two_targets(centre: &str, other: &str) -> (GameState, Hub) {
        let mut ids = vec![centre.to_owned()];
        let empty = empty_systems(5);
        ids.push(empty[0].clone());
        ids.push(other.to_owned());
        ids.extend(empty[1..].iter().cloned());
        let hub = hub_from(&ids);
        let mut state = game(&["a", "b"]);
        round_three(&mut state);
        let start = SystemId::new(hub.outer[0].clone());
        put(&mut state, &start, "carrier", &me(), 1);
        put(&mut state, &start, "infantry", &me(), 2);
        (state, hub)
    }

    fn activation(hub: &Hub) -> Choice {
        let options = std::iter::once(&hub.centre)
            .chain(hub.outer.iter())
            .map(|s| ChoiceOption::labelled(s.as_str(), "activate", s.as_str()))
            .collect();
        contextual(
            Choice::new(me(), "activate a system", options),
            "activate_system",
            3,
        )
    }

    #[test]
    fn activation_takes_the_planet_that_completes_a_secret_over_a_richer_one() {
        let cultural = one_planet_systems(|p| p.has_trait("cultural"));
        let worth = |id: &str| {
            let p = &planets(id)[0];
            p.resources() + p.influence()
        };
        let target = cultural[0].clone();
        // A richer planet of another trait, but not by a whole objective.
        let richer = one_planet_systems(|p| !p.has_trait("cultural"))
            .into_iter()
            .find(|id| worth(id) > worth(&target) && worth(id) < worth(&target) + 4)
            .expect("a richer planet");
        let (mut state, hub) = two_targets(&target, &richer);
        let choice = activation(&hub);
        // Without the secret the richer planet is the pick (index 2: outer[1]).
        assert_eq!(picked(&state, Some(&hub), &choice), Some(2));
        // Three cultural planets held off the map and Form an Alliance... "faa" needs four.
        for system in &cultural[1..4] {
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
        assert_eq!(picked(&state, Some(&hub), &choice), Some(0));
    }

    #[test]
    fn from_round_three_a_fight_that_wins_nothing_is_left_alone() {
        let empty = empty_systems(7);
        let hub = hub_from(&empty);
        let mut state = game(&["a", "b"]);
        let start = SystemId::new(hub.outer[0].clone());
        put(&mut state, &start, "dreadnought", &me(), 3);
        let centre = SystemId::new(hub.centre.clone());
        put(&mut state, &centre, "destroyer", &foe(), 1);
        let action = |round: u32| {
            contextual(
                Choice::new(
                    me(),
                    "take an action",
                    vec![
                        ChoiceOption::labelled("tactical", "action", "tactical action"),
                        ChoiceOption::labelled("pass", "action", "pass"),
                    ],
                ),
                "action",
                round,
            )
        };
        // The opening bot goes for the lone destroyer.
        state.round = 1;
        assert_eq!(picked(&state, Some(&hub), &action(1)), Some(0));
        // From round three it is not worth a command token.
        round_three(&mut state);
        assert_eq!(picked(&state, Some(&hub), &action(3)), Some(1));
        // Unless the destroyer sits on its doorstep: a planet of its own in that system.
        let planet_system = one_planet_systems(|_| true)[0].clone();
        let mut ids = vec![planet_system.clone()];
        ids.extend(empty[1..7].iter().cloned());
        let hub = hub_from(&ids);
        let mut state = game(&["a", "b"]);
        round_three(&mut state);
        let start = SystemId::new(hub.outer[0].clone());
        put(&mut state, &start, "dreadnought", &me(), 3);
        let centre = SystemId::new(hub.centre.clone());
        put(&mut state, &centre, "destroyer", &foe(), 1);
        let planet = PlanetId::new(planets(&planet_system)[0].id());
        state
            .system_mut(&centre)
            .planet_control
            .insert(planet, me());
        assert_eq!(picked(&state, Some(&hub), &action(3)), Some(0));
    }

    #[test]
    fn over_the_hand_limit_the_secret_furthest_from_done_goes_back() {
        let mut state = game(&["a", "b"]);
        round_three(&mut state);
        let cultural = one_planet_systems(|p| p.has_trait("cultural"));
        for system in &cultural[..2] {
            let planet = PlanetId::new(planets(system)[0].id());
            state
                .system_mut(&SystemId::new(system.clone()))
                .planet_control
                .insert(planet, me());
        }
        // faa: two of four cultural planets; mp: no industrial planet yet.
        let choice = contextual(
            Choice::new(
                me(),
                "return a secret objective to the deck",
                vec![
                    ChoiceOption::labelled("faa", "return", "return faa"),
                    ChoiceOption::labelled("mp", "return", "return mp"),
                ],
            ),
            "return_over_secret_hand_limit",
            3,
        );
        {
            let seat = state.player_mut(&me()).expect("seat");
            seat.secret_objectives
                .push(ti4_model::id::SecretObjectiveId::new("faa"));
            seat.secret_objectives
                .push(ti4_model::id::SecretObjectiveId::new("mp"));
        }
        assert_eq!(picked(&state, None, &choice), Some(1));
    }
}
