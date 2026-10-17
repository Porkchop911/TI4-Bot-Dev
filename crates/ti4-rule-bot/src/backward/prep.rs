//! The prep layer: *prepared* steps for every planner.
//!
//! A plain [`Step`] is one activation judged on the position as it is. A prepared step is an
//! activation preceded by a prep that changes what the activation can do, judged on the
//! position *after* the prep:
//!
//! | kind | knob | prep | what it adds to the act step |
//! |---|---|---|---|
//! | [`PrepKind::Build`] | [`tuning::bw_prep_build`] | a production: a home dock activation, Construction's production (Thunder's Edge), Warfare's secondary | ground forces to land, a carrier to bring them, cruisers to win a fight, a ship to send |
//! | [`PrepKind::Warfare`] | [`tuning::bw_prep_warfare`] | an activation next to the target, paid for by Warfare's extra activation when tokens are short | the ships (and cargo) that could reach the neighbour but not the target |
//! | [`PrepKind::TechRange`] | [`tuning::bw_prep_tech_range`] | a movement technology | ships one hex short |
//! | [`PrepKind::TechStrength`] | [`tuning::bw_prep_tech_strength`] | a combat or upgrade technology | the fleet's strength, judged with `fleet_strength` |
//!
//! [`prepared`] appends the prepared steps to [`super::PlanCtx::steps`], so
//! [`super::PlanCtx::route`] and every planner built on it see them without change. A prepared
//! step carries its prep ([`PrepInfo`]: tokens, strategic tokens, resources, the card to pick or
//! play, units to build, systems to activate first) and its probability is prep feasibility x act
//! success x 0.9 for the extra step. A route holds at most one prepared step, and all steps and
//! preps together stay within the seat's tactic tokens and three activations.

//! # Which objectives get which prep
//!
//! Every objective of the route-shaped planners: [`super::territory::ALIASES`] and
//! [`super::fleet::ALIASES`] (all four kinds, [`APPLIES_TO`]), and the `mecatol` plan
//! ([`super::mecatol_two_step`]). Planners with their own logic (payment, technology, combat,
//! structures) read `ctx.steps` themselves if they want the prepared steps.

use std::collections::BTreeMap;

use ti4_engine::choice::Observed;
use ti4_engine::fleet_strength::{Descriptors, descriptors, matchup};
use ti4_model::id::{PlanetId, PlayerId, SystemId};

use super::{PlanCtx, Step};
use crate::tuning;

/// Which prep a prepared step uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrepKind {
    Build,
    Warfare,
    TechRange,
    TechStrength,
}

/// What a prep costs and asks for.
#[derive(Debug, Clone, PartialEq)]
pub struct PrepInfo {
    pub kind: PrepKind,
    /// "build + act", "warfare move + act", ... (the start of a plan's reason).
    pub label: String,
    /// Tactic tokens the prep itself uses (a gather activation or a dock activation).
    pub tokens: i32,
    /// Strategy tokens it uses (a secondary).
    pub strategic: i32,
    pub resources: i64,
    /// Systems activated first.
    pub systems: Vec<SystemId>,
    /// The subset of `systems` where ships and cargo gather (the movement hook moves everything).
    pub gather: Vec<SystemId>,
    /// Unit types to produce.
    pub builds: Vec<String>,
    /// Strategy card to draft.
    pub cards: Vec<String>,
    /// Strategy card to play now.
    pub play: Vec<String>,
    /// Technologies to pick when a research is offered.
    pub techs: Vec<String>,
    /// Probability that the prep itself works.
    pub odds: f64,
}

/// The fight and landing in one system as the seat can bring them.
#[derive(Debug, Clone)]
pub struct SysCase {
    pub system: SystemId,
    /// Planets worth taking: (planet, worth, ground forces needed), best per troop first.
    pub targets: Vec<(PlanetId, f64, usize)>,
    /// Ground forces that reach it now (plus those there).
    pub ground: usize,
    /// Whether a ship can reach it now.
    pub ships: bool,
    /// An own ship is there already.
    pub present: bool,
    pub hostile: bool,
    /// The fleet that could fight there now, as unit counts.
    pub own: Vec<(String, usize)>,
    pub enemy: Descriptors,
    pub margin: f64,
    pub faction: String,
}

/// What an activation does: planets taken, a ship arriving, and the odds.
#[derive(Debug, Clone, PartialEq)]
pub struct Outcome {
    pub planets: Vec<PlanetId>,
    pub ship: bool,
    pub success: f64,
}

impl SysCase {
    #[must_use]
    pub fn new(seen: &Observed<'_>, me: &PlayerId, system: &SystemId) -> Self {
        let (ships, ground) = crate::objectives::reach(seen, me, system);
        let movable = seen.movable_into(me, system);
        let board = seen.system(system);
        let here = board
            .units
            .iter()
            .filter(|u| &u.owner == me && crate::is_ship(seen, u));
        let present = here.clone().next().is_some();
        let (own, _) = crate::tally(here.chain(movable.iter().map(|m| &m.unit)));
        Self {
            targets: crate::targets(seen, me, system),
            hostile: crate::has_hostile_ships(seen, me, system),
            enemy: crate::hostile_space(seen, me, system),
            margin: crate::fight_margin(seen, system),
            faction: crate::faction(seen, me),
            system: system.clone(),
            ground,
            ships,
            present,
            own,
        }
    }

    /// The strength matchup of the fleet `own` against the defence (infinite without one).
    #[must_use]
    pub fn matchup_of(&self, seen: &Observed<'_>, own: &[(String, usize)]) -> f64 {
        if !self.hostile {
            return f64::INFINITY;
        }
        let mine = descriptors(seen.content(), seen.sources(), &self.faction, own, &[]);
        matchup(&mine, &self.enemy)
    }

    /// The planets `ground` forces take, greedily in target order.
    #[must_use]
    pub fn taken(&self, ground: usize) -> Vec<PlanetId> {
        let mut left = ground;
        let mut out = Vec::new();
        for (planet, _, need) in &self.targets {
            if *need <= left {
                left -= need;
                out.push(planet.clone());
            }
        }
        out
    }

    /// What the activation does with `ground` forces and fleet `own`, if it does anything. With
    /// `reachable` false nothing arrives.
    #[must_use]
    pub fn outcome(
        &self,
        seen: &Observed<'_>,
        ground: usize,
        own: &[(String, usize)],
        reachable: bool,
    ) -> Option<Outcome> {
        if !reachable || own.is_empty() {
            return None;
        }
        let m = self.matchup_of(seen, own);
        if m < self.margin {
            return None;
        }
        let planets = self.taken(ground);
        let ship = !self.present;
        if planets.is_empty() && !ship {
            return None;
        }
        let fight = if self.hostile {
            1.0 / (1.0 + (-2.0 * m).exp())
        } else {
            1.0
        };
        Some(Outcome {
            success: fight * if planets.is_empty() { 1.0 } else { 0.9 },
            planets,
            ship,
        })
    }

    /// The activation as it stands.
    #[must_use]
    pub fn plain(&self, seen: &Observed<'_>) -> Option<Outcome> {
        self.outcome(seen, self.ground, &self.own, self.ships)
    }
}

pub(super) fn with_added(fleet: &[(String, usize)], id: &str, n: usize) -> Vec<(String, usize)> {
    let mut out = fleet.to_vec();
    match out.iter_mut().find(|(k, _)| k == id) {
        Some((_, c)) => *c += n,
        None => out.push((id.to_owned(), n)),
    }
    out
}

/// Production slots of the home system (docks' production values, as the engine counts them).
pub(super) fn home_slots(seen: &Observed<'_>, me: &PlayerId, home: &SystemId) -> i64 {
    seen.system(home)
        .planet_units
        .iter()
        .flat_map(|(planet, units)| {
            let res = ti4_content::galaxy::planet(seen.content(), planet.as_str(), seen.sources())
                .map_or(0, |p| p.resources());
            units
                .iter()
                .filter(|u| &u.owner == me)
                .filter_map(move |u| crate::unit_type(seen, u.type_id.as_str()))
                .map(move |t| t.production(res))
        })
        .sum::<i64>()
        .max(0)
}

/// How the seat gets a production before the act step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// Activate the home dock (one more command token).
    Dock,
    /// The seat's own Construction card, whose primary offers a production (Thunder's Edge).
    Construction,
    /// The seat's own Warfare primary (Thunder's Edge): a free tactical action into the home
    /// dock, which produces without a token.
    WarfarePrimary,
    /// The printed Warfare primary frees a token: a home dock activation with it.
    DockWithWarfare,
    /// Another seat's Warfare secondary, which produces at home for a strategy token.
    WarfareSecondary,
}

impl Source {
    fn label(self) -> &'static str {
        match self {
            Self::Dock => "home dock activation",
            Self::Construction => "Construction production",
            Self::WarfarePrimary => "Warfare primary, free activation of the dock",
            Self::DockWithWarfare => "Warfare primary token, home dock activation",
            Self::WarfareSecondary => "Warfare secondary production",
        }
    }
}

/// The ways to get a production, as (source, tactic tokens, strategy tokens, odds).
pub(super) fn sources(
    seen: &Observed<'_>,
    me: &PlayerId,
    ctx: &PlanCtx,
) -> Vec<(Source, i32, i32, f64)> {
    let mut out = Vec::new();
    if let Some(home) = crate::home_system(seen, me)
        && ctx.tactic_tokens >= 2
        && !seen.systems_with_token(me).contains(&home)
        && crate::has_own_production(seen, me, &home)
    {
        out.push((Source::Dock, 1, 0, 0.95));
    }
    if crate::ready_card(seen, me, "Construction").is_some_and(|c| c.starts_with("te")) {
        out.push((Source::Construction, 0, 0, 0.95));
    }
    let access = warfare(seen, me);
    if let Some(card) = &access.own
        && let Some(home) = crate::home_system(seen, me)
        && crate::has_own_production(seen, me, &home)
    {
        if card.starts_with("te") {
            out.push((Source::WarfarePrimary, 0, 0, 0.95));
        } else if access.extra_activation
            && ctx.tactic_tokens == 1
            && !seen.systems_with_token(me).contains(&home)
        {
            out.push((Source::DockWithWarfare, 1, 0, 0.95));
        }
    }
    if ctx.strategic_tokens >= 1 && crate::secondary_to_come(seen, me, "Warfare") {
        out.push((Source::WarfareSecondary, 0, 1, 0.6));
    }
    out
}

/// Build prep for `case`: what a production at home adds, if it makes the activation do more.
fn build_prep(
    seen: &Observed<'_>,
    me: &PlayerId,
    ctx: &PlanCtx,
    case: &SysCase,
    plain: Option<&Outcome>,
) -> Option<(Outcome, PrepInfo)> {
    let (source, tokens, strategic, odds) = sources(seen, me, ctx)
        .into_iter()
        .max_by(|a, b| a.3.total_cmp(&b.3).then(b.1.cmp(&a.1)))?;
    let home = crate::home_system(seen, me)?;
    let slots = home_slots(seen, me, &home);
    if slots <= 0 {
        return None;
    }
    let carrier = crate::unit_type(seen, "carrier")?;
    let cruiser = crate::unit_type(seen, "cruiser")?;
    let infantry = crate::unit_type(seen, "infantry")?;
    let carrier_range = crate::carrier_reach(seen, me);
    let cruiser_range = i32::try_from(cruiser.move_value()).unwrap_or(1);
    let have_taken = plain.map_or(0, |o| o.planets.len());
    let mut units: Vec<(String, usize)> = Vec::new();
    let mut slots_left = slots;
    let mut budget = ctx.resources as f64;
    let mut fleet = case.own.clone();
    let mut ground = case.ground;
    if case.targets.is_empty() {
        // A ship goal: a new cruiser that can get there.
        if case.present
            || plain.is_some_and(|o| o.ship)
            || cruiser.cost() > budget
            || (home != case.system && !seen.can_reach(me, &home, &case.system, cruiser_range))
        {
            return None;
        }
        units.push((cruiser.id().to_owned(), 1));
        fleet = with_added(&fleet, cruiser.id(), 1);
        budget -= cruiser.cost();
        slots_left -= 1;
    } else {
        if home != case.system && !seen.can_reach(me, &home, &case.system, carrier_range) {
            return None;
        }
        // Ground forces needed for one planet more.
        let extra = (1..=usize::try_from(slots_left).unwrap_or(0))
            .find(|e| case.taken(case.ground + e).len() > have_taken)?;
        let extra_i = i64::try_from(extra).unwrap_or(0);
        let movable = seen.movable_into(me, &case.system);
        let cap_home: i64 = movable
            .iter()
            .filter(|m| m.origin == home && m.capacity > 0 && crate::is_ship(seen, &m.unit))
            .map(|m| m.capacity)
            .sum();
        let waiting = i64::try_from(
            seen.loadable(me, &home)
                .iter()
                .filter(|c| crate::is_ground(seen, &c.unit))
                .count(),
        )
        .unwrap_or(0);
        let spare = (cap_home - waiting).max(0);
        let cap = carrier.capacity().max(1);
        let carriers = ((extra_i - spare).max(0) + cap - 1) / cap;
        #[expect(clippy::cast_precision_loss, reason = "small counts")]
        let cost = extra_i as f64 * infantry.cost() + carriers as f64 * carrier.cost();
        if extra_i + carriers > slots_left || cost > budget {
            return None;
        }
        slots_left -= extra_i + carriers;
        budget -= cost;
        units.push((infantry.id().to_owned(), extra));
        ground += extra;
        if carriers > 0 {
            let n = usize::try_from(carriers).unwrap_or(0);
            units.push((carrier.id().to_owned(), n));
            fleet = with_added(&fleet, carrier.id(), n);
        }
    }
    // Cruisers until the fight is won.
    let mut added = 0usize;
    while case.hostile
        && case.matchup_of(seen, &fleet) < case.margin
        && slots_left > 0
        && budget >= cruiser.cost()
        && added < 4
    {
        fleet = with_added(&fleet, cruiser.id(), 1);
        slots_left -= 1;
        budget -= cruiser.cost();
        added += 1;
    }
    if added > 0 {
        units.push((cruiser.id().to_owned(), added));
    }
    let out = case.outcome(seen, ground, &fleet, true)?;
    if out.planets.len() <= have_taken && !(out.ship && !plain.is_some_and(|o| o.ship)) {
        return None;
    }
    #[expect(clippy::cast_possible_truncation, reason = "a handful of resources")]
    let resources = (ctx.resources as f64 - budget).ceil() as i64;
    let mut systems = Vec::new();
    if matches!(
        source,
        Source::Dock | Source::DockWithWarfare | Source::WarfarePrimary
    ) {
        systems.push(home);
    }
    let card = match source {
        Source::Construction => Some("Construction"),
        Source::WarfarePrimary | Source::DockWithWarfare => Some("Warfare"),
        _ => None,
    };
    Some((
        out,
        PrepInfo {
            kind: PrepKind::Build,
            label: "build + act".to_owned(),
            tokens,
            strategic,
            resources,
            gather: Vec::new(),
            systems,
            builds: units.iter().map(|(u, _)| u.clone()).collect(),
            cards: card.map(str::to_owned).into_iter().collect(),
            play: card.map(str::to_owned).into_iter().collect(),
            techs: Vec::new(),
            odds,
        },
    ))
    .map(|(o, mut p)| {
        p.label = format!("build + act ({})", source.label());
        (o, p)
    })
}

/// What the seat's Warfare card gives this turn (shared by the build and move preps).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WarfareAccess {
    /// The seat's own ready Warfare card.
    pub own: Option<String>,
    /// Playing it gives one more activation: Thunder's Edge's free tactical action, or the
    /// printed card's recalled token (needs a token on the board).
    pub extra_activation: bool,
    /// Another seat's Warfare is still to come: its secondary produces at home.
    pub secondary: bool,
}

/// The seat's Warfare options.
#[must_use]
pub fn warfare(seen: &Observed<'_>, me: &PlayerId) -> WarfareAccess {
    let own = crate::ready_card(seen, me, "Warfare");
    let extra_activation = own
        .as_ref()
        .is_some_and(|c| c.starts_with("te") || !seen.systems_with_token(me).is_empty());
    WarfareAccess {
        secondary: crate::secondary_to_come(seen, me, "Warfare"),
        extra_activation,
        own,
    }
}

/// Ground and fleet the seat has at `case.system` after gathering at `gather` (an activation next
/// to it): the ships that could reach `gather` join those that reach the system.
pub(super) fn gathered(
    seen: &Observed<'_>,
    me: &PlayerId,
    case: &SysCase,
    gather: &SystemId,
) -> (usize, Vec<(String, usize)>) {
    let mut set: BTreeMap<(SystemId, usize), ti4_engine::tactical::Movable> = BTreeMap::new();
    for m in seen
        .movable_into(me, gather)
        .into_iter()
        .chain(seen.movable_into(me, &case.system))
    {
        set.entry((m.origin.clone(), m.index)).or_insert(m);
    }
    let carriers: Vec<_> = set
        .values()
        .filter(|m| m.capacity > 0 && crate::is_ship(seen, &m.unit))
        .collect();
    let capacity: i64 = carriers.iter().map(|m| m.capacity).sum();
    let origins: std::collections::BTreeSet<&SystemId> =
        carriers.iter().map(|m| &m.origin).collect();
    let loadable: usize = origins
        .into_iter()
        .map(|o| {
            seen.loadable(me, o)
                .iter()
                .filter(|c| crate::is_ground(seen, &c.unit))
                .count()
        })
        .sum();
    let board = seen.system(&case.system);
    let there = board
        .units
        .iter()
        .filter(|u| &u.owner == me && crate::is_ground(seen, u))
        .count();
    let ground = loadable.min(usize::try_from(capacity).unwrap_or(0)) + there;
    let present = board
        .units
        .iter()
        .filter(|u| &u.owner == me && crate::is_ship(seen, u));
    let (own, _) = crate::tally(present.chain(set.values().map(|m| &m.unit)));
    (ground, own)
}

/// Warfare prep: gather beside the target (Warfare's extra activation if tokens are short).
fn warfare_prep(
    seen: &Observed<'_>,
    me: &PlayerId,
    ctx: &PlanCtx,
    case: &SysCase,
    plain: Option<&Outcome>,
) -> Option<(Outcome, PrepInfo)> {
    let access = warfare(seen, me);
    let use_warfare = ctx.tactic_tokens < 2;
    if ctx.tactic_tokens < 1 || (use_warfare && !access.extra_activation) {
        return None;
    }
    let galaxy = seen.galaxy()?;
    let tokened = seen.systems_with_token(me);
    let have_taken = plain.map_or(0, |o| o.planets.len());
    let (gather, out) = galaxy
        .adjacent(case.system.as_str())
        .into_iter()
        .map(SystemId::new)
        .filter(|x| !tokened.contains(x) && !crate::has_hostile_ships(seen, me, x))
        .filter_map(|x| {
            let (ground, own) = gathered(seen, me, case, &x);
            case.outcome(seen, ground, &own, true).map(|o| (x, o))
        })
        .filter(|(_, o)| o.planets.len() > have_taken || (o.ship && !plain.is_some_and(|p| p.ship)))
        .max_by(|a, b| {
            a.1.planets
                .len()
                .cmp(&b.1.planets.len())
                .then(a.1.success.total_cmp(&b.1.success))
                .then_with(|| b.0.cmp(&a.0))
        })?;
    Some((
        out,
        PrepInfo {
            kind: PrepKind::Warfare,
            label: if use_warfare {
                "warfare move + act (Warfare gives the activation)".to_owned()
            } else {
                "warfare move + act (tokens enough, no Warfare)".to_owned()
            },
            tokens: i32::from(!use_warfare),
            strategic: 0,
            resources: 0,
            systems: vec![gather.clone()],
            gather: vec![gather],
            builds: Vec::new(),
            cards: if use_warfare {
                vec!["Warfare".to_owned()]
            } else {
                Vec::new()
            },
            play: if use_warfare {
                vec!["Warfare".to_owned()]
            } else {
                Vec::new()
            },
            techs: Vec::new(),
            odds: if use_warfare { 0.95 * 0.85 } else { 0.85 },
        },
    ))
}

/// The prepared steps for every system the seat could activate (own-token systems left out), for
/// the kinds switched on. A prepared step is kept only where it does more than the plain step.
#[must_use]
pub fn prepared(seen: &Observed<'_>, me: &PlayerId, ctx: &PlanCtx) -> Vec<Step> {
    let any = tuning::bw_prep_build()
        || tuning::bw_prep_warfare()
        || tuning::bw_prep_tech_range()
        || tuning::bw_prep_tech_strength();
    let Some(galaxy) = seen.galaxy().filter(|_| any) else {
        return Vec::new();
    };
    let tokened = seen.systems_with_token(me);
    let mut out = Vec::new();
    for id in galaxy.system_ids() {
        let system = SystemId::new(id);
        if tokened.contains(&system)
            || ctx.mecatol.held && ctx.mecatol.system.as_ref() == Some(&system)
        {
            continue;
        }
        let case = SysCase::new(seen, me, &system);
        let plain = case.plain(seen);
        let complete = plain.as_ref().is_some_and(|o| {
            o.success >= tuning::bw_likely()
                && if case.targets.is_empty() {
                    o.ship
                } else {
                    o.planets.len() == case.targets.len()
                }
        });
        if complete {
            continue;
        }
        let mut options: Vec<(Outcome, PrepInfo)> = Vec::new();
        if tuning::bw_prep_build() {
            options.extend(build_prep(seen, me, ctx, &case, plain.as_ref()));
        }
        if tuning::bw_prep_warfare() {
            options.extend(warfare_prep(seen, me, ctx, &case, plain.as_ref()));
        }
        if tuning::bw_prep_tech_range() || tuning::bw_prep_tech_strength() {
            options.extend(tech_preps(seen, me, ctx, &case, plain.as_ref()));
        }
        let best = options.into_iter().max_by(|a, b| {
            let key = |o: &(Outcome, PrepInfo)| o.0.success * o.1.odds;
            key(a).total_cmp(&key(b))
        });
        if let Some((o, prep)) = best {
            out.push(Step {
                system,
                planets: o.planets,
                ship: o.ship,
                spare_ship: true,
                success: prep.odds * o.success * 0.9,
                prep: Some(prep),
            });
        }
    }
    out
}

// -- technology preps ----------------------------------------------------------------------------

/// A researchable unit upgrade: the unit type it replaces, the one it makes, and the technology.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Upgrade {
    alias: String,
    from: String,
    to: String,
}

/// Colours of the seat's owned technologies (printed prerequisites only).
fn owned_colours(seen: &Observed<'_>, me: &PlayerId) -> [usize; 4] {
    let mut have = [0usize; 4];
    let Some(seat) = seen.seat(me) else {
        return have;
    };
    for t in seat.technologies {
        if let Some(name) = ti4_engine::technology::colour_type(seen.content(), t)
            && let Some(c) = ["BIOTIC", "CYBERNETIC", "PROPULSION", "WARFARE"]
                .iter()
                .position(|n| *n == name)
        {
            have[c] += 1;
        }
    }
    have
}

/// Whether the seat can research `alias` now: not owned, its faction's, prerequisites met.
fn researchable(seen: &Observed<'_>, me: &PlayerId, alias: &str) -> bool {
    let Some(seat) = seen.seat(me) else {
        return false;
    };
    let id = ti4_model::id::TechnologyId::new(alias);
    let content = seen.content();
    if seat.technologies.contains(&id)
        || !ti4_engine::technology::active_aliases(content, seen.sources()).contains(&id)
        || ti4_engine::technology::faction_of(content, &id)
            .is_some_and(|f| f != seat.faction.as_str())
    {
        return false;
    }
    let have = owned_colours(seen, me);
    let needs = ti4_engine::technology::prerequisites(content, &id);
    ["BIOTIC", "CYBERNETIC", "PROPULSION", "WARFARE"]
        .iter()
        .enumerate()
        .all(|(c, name)| have[c] >= needs.get(*name).copied().unwrap_or(0))
}

/// The unit upgrades the seat could research, for unit types it owns.
fn upgrades(seen: &Observed<'_>, me: &PlayerId) -> Vec<Upgrade> {
    let mut kinds: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for board in seen.board().values() {
        for u in board.units.iter().filter(|u| &u.owner == me) {
            kinds.insert(u.type_id.to_string());
        }
    }
    kinds
        .into_iter()
        .filter_map(|from| {
            let to = crate::unit_type(seen, &from)?.upgrades_to()?.to_owned();
            let alias = crate::unit_type(seen, &to)?
                .required_technology()?
                .to_owned();
            researchable(seen, me, &alias).then_some(Upgrade { alias, from, to })
        })
        .collect()
}

/// The best research open to the seat before the status phase (B's technology model: own card
/// 95 percent, a rival's secondary 70 percent for a strategy token and four resources).
fn research_source(seen: &Observed<'_>, me: &PlayerId) -> Option<super::technology::Source> {
    let seat = seen.seat(me)?;
    let is_tech = |c: &ti4_model::id::StrategyCardId| {
        ti4_engine::strategy_cards::card_name(seen.content(), c.as_str()).as_deref()
            == Some("Technology")
    };
    let own_open = seat
        .strategy_cards
        .iter()
        .any(|c| is_tech(c) && !seat.exhausted_strategy_cards.contains(c));
    let rivals = seen
        .players()
        .into_iter()
        .filter(|p| *p != me)
        .filter_map(|p| seen.seat(p))
        .filter(|s| !s.passed)
        .filter(|s| {
            s.strategy_cards
                .iter()
                .any(|c| is_tech(c) && !s.exhausted_strategy_cards.contains(c))
        })
        .count();
    super::technology::sources(
        own_open,
        rivals,
        seen.available_spend(me, ti4_engine::production::Spend::Resources),
        seat.strategic_tokens,
    )
    .into_iter()
    .next()
}

fn replaced(fleet: &[(String, usize)], from: &str, to: &str) -> Vec<(String, usize)> {
    let mut out: Vec<(String, usize)> = Vec::new();
    for (k, c) in fleet {
        let key = if k == from { to } else { k.as_str() };
        match out.iter_mut().find(|(o, _)| o == key) {
            Some((_, n)) => *n += c,
            None => out.push((key.to_owned(), *c)),
        }
    }
    out
}

/// Ships that could reach `case.system` if the technology gave them move `mv(type)`: those not
/// reaching it now, as (origin, type id, count).
fn newly_in_range(
    seen: &Observed<'_>,
    me: &PlayerId,
    case: &SysCase,
    mv: impl Fn(&str) -> Option<i32>,
) -> Vec<(SystemId, String, usize)> {
    let movable = seen.movable_into(me, &case.system);
    let mut out = Vec::new();
    for (origin, board) in seen.board() {
        if origin == &case.system {
            continue;
        }
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        for u in board.units.iter().filter(|u| {
            &u.owner == me
                && crate::is_ship(seen, u)
                && !crate::unit_type(seen, u.type_id.as_str()).is_some_and(|t| t.is_fighter())
        }) {
            *counts.entry(u.type_id.to_string()).or_default() += 1;
        }
        for (kind, n) in counts {
            let reaching = movable
                .iter()
                .filter(|m| &m.origin == origin && m.unit.type_id.as_str() == kind)
                .count();
            let missing = n.saturating_sub(reaching);
            if missing > 0
                && let Some(v) = mv(&kind)
                && seen.can_reach(me, origin, &case.system, v)
            {
                out.push((origin.clone(), kind, missing));
            }
        }
    }
    out
}

/// Ground the carriers reaching `case.system` bring once `extra` ships (origin, type, count) join.
fn ground_with(
    seen: &Observed<'_>,
    me: &PlayerId,
    case: &SysCase,
    extra: &[(SystemId, String, usize)],
    capacity_of: impl Fn(&str) -> i64,
) -> usize {
    let movable = seen.movable_into(me, &case.system);
    let mut capacity: i64 = movable
        .iter()
        .filter(|m| m.capacity > 0 && crate::is_ship(seen, &m.unit))
        .map(|m| m.capacity)
        .sum();
    let mut origins: std::collections::BTreeSet<SystemId> = movable
        .iter()
        .filter(|m| m.capacity > 0 && crate::is_ship(seen, &m.unit))
        .map(|m| m.origin.clone())
        .collect();
    for (origin, kind, n) in extra {
        let cap = capacity_of(kind);
        if cap > 0 {
            capacity += cap * i64::try_from(*n).unwrap_or(0);
            origins.insert(origin.clone());
        }
    }
    let loadable: usize = origins
        .iter()
        .map(|o| {
            seen.loadable(me, o)
                .iter()
                .filter(|c| crate::is_ground(seen, &c.unit))
                .count()
        })
        .sum();
    let there = seen
        .system(&case.system)
        .units
        .iter()
        .filter(|u| &u.owner == me && crate::is_ground(seen, u))
        .count();
    loadable.min(usize::try_from(capacity).unwrap_or(0)) + there
}

/// Technology preps for `case`: a movement technology (range) or a unit upgrade (strength).
fn tech_preps(
    seen: &Observed<'_>,
    me: &PlayerId,
    ctx: &PlanCtx,
    case: &SysCase,
    plain: Option<&Outcome>,
) -> Vec<(Outcome, PrepInfo)> {
    let Some(source) = research_source(seen, me) else {
        return Vec::new();
    };
    let have_taken = plain.map_or(0, |o| o.planets.len());
    let better =
        |o: &Outcome| o.planets.len() > have_taken || (o.ship && !plain.is_some_and(|p| p.ship));
    let upgrades = upgrades(seen, me);
    let mut out: Vec<(Outcome, PrepInfo)> = Vec::new();
    let info = |kind: PrepKind, label: String, alias: &str| PrepInfo {
        kind,
        label,
        tokens: 0,
        strategic: source.tokens,
        resources: source.resources,
        systems: Vec::new(),
        gather: Vec::new(),
        builds: Vec::new(),
        cards: vec!["Technology".to_owned()],
        play: if source.own_card {
            vec!["Technology".to_owned()]
        } else {
            Vec::new()
        },
        techs: vec![alias.to_owned()],
        odds: source.probability,
    };
    if tuning::bw_prep_tech_range() {
        // Gravity Drive: one ship of the activation moves one hex further.
        if researchable(seen, me, "gd") {
            let extra = newly_in_range(seen, me, case, |kind| {
                crate::unit_type(seen, kind).map(|t| i32::try_from(t.move_value()).unwrap_or(1) + 1)
            });
            // One ship only, a carrier first.
            let one: Vec<(SystemId, String, usize)> = extra
                .into_iter()
                .max_by_key(|(_, k, _)| crate::unit_type(seen, k).map_or(0, |t| t.capacity()))
                .map(|(o, k, _)| vec![(o, k, 1)])
                .unwrap_or_default();
            if !one.is_empty() {
                let cap = |k: &str| crate::unit_type(seen, k).map_or(0, |t| t.capacity());
                let ground = ground_with(seen, me, case, &one, cap);
                let mut own = case.own.clone();
                for (_, k, n) in &one {
                    own = with_added(&own, k, *n);
                }
                if let Some(o) = case.outcome(seen, ground, &own, true).filter(|o| better(o)) {
                    out.push((
                        o,
                        info(
                            PrepKind::TechRange,
                            "technology range + act (Gravity Drive)".to_owned(),
                            "gd",
                        ),
                    ));
                }
            }
        }
        // A faster unit upgrade moves every ship of that type.
        for up in &upgrades {
            let Some(to) = crate::unit_type(seen, &up.to) else {
                continue;
            };
            let Some(from) = crate::unit_type(seen, &up.from) else {
                continue;
            };
            if to.move_value() <= from.move_value() {
                continue;
            }
            let extra = newly_in_range(seen, me, case, |kind| {
                (kind == up.from).then(|| i32::try_from(to.move_value()).unwrap_or(1))
            });
            if extra.is_empty() {
                continue;
            }
            let cap = |k: &str| {
                let k = if k == up.from { up.to.as_str() } else { k };
                crate::unit_type(seen, k).map_or(0, |t| t.capacity())
            };
            let ground = ground_with(seen, me, case, &extra, cap);
            let mut own = replaced(&case.own, &up.from, &up.to);
            for (_, k, n) in &extra {
                own = with_added(&own, &up.to, *n);
                let _ = k;
            }
            if let Some(o) = case.outcome(seen, ground, &own, true).filter(|o| better(o)) {
                out.push((
                    o,
                    info(
                        PrepKind::TechRange,
                        format!("technology range + act ({})", up.alias),
                        &up.alias,
                    ),
                ));
            }
        }
    }
    if tuning::bw_prep_tech_strength() && case.hostile {
        for up in &upgrades {
            if !case.own.iter().any(|(k, _)| k == &up.from) {
                continue;
            }
            let own = replaced(&case.own, &up.from, &up.to);
            if let Some(o) = case
                .outcome(seen, case.ground, &own, case.ships)
                .filter(|o| better(o))
            {
                out.push((
                    o,
                    info(
                        PrepKind::TechStrength,
                        format!("technology strength + act ({})", up.alias),
                        &up.alias,
                    ),
                ));
            }
        }
    }
    let _ = ctx;
    out
}

/// The planner modules whose goals see the prepared steps through `route_plan`.
pub const APPLIES_TO: &[&str] = &["territory", "fleet", "mecatol_two_step"];

#[cfg(test)]
mod tests {
    use super::super::{ObjectivePlanner, fleet, territory};

    #[test]
    fn the_route_planners_the_docs_name_are_the_ones_without_own_logic() {
        // Territory and Fleet are route_plan users: their aliases are disjoint.
        let t: std::collections::BTreeSet<_> = territory::ALIASES.iter().collect();
        let f: std::collections::BTreeSet<_> = fleet::ALIASES.iter().collect();
        assert!(t.is_disjoint(&f));
        assert_eq!(super::APPLIES_TO.len(), 3);
        assert!(!territory::Territory.objective_ids().is_empty());
        assert!(!fleet::Fleet.objective_ids().is_empty());
    }
}
