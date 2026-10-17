//! Agenda voting by valuation, the vote budget and rider cards (`docs/BOT_AGENDA_VOTING.md`).
//!
//! Three knobs, all off in bot versions 1 to 4:
//!
//! - `agenda_value`: an expected value to the seat for every outcome of every agenda in the
//!   content ([`evaluate`]), the outcome with the best (mine - weighted rivals') margin wins; a
//!   small margin means no opinion.
//! - `agenda_budget`: how many votes to cast ([`plan_vote`]): enough to make the outcome likely,
//!   not more, keeping the rest for the second agenda of the phase.
//! - `agenda_riders`: when to play a rider card and which outcome to predict ([`rider_pick`]).
//!
//! Everything reads public information ([`Observed`]) plus the choice in front of the seat; the
//! forecast of a rival's vote is the same valuation computed from the rival's seat.

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::too_many_lines,
    clippy::similar_names,
    clippy::many_single_char_names
)]

use std::cell::RefCell;
use std::collections::BTreeMap;

use ti4_engine::choice::{Choice, ChoiceOption, Observed};
use ti4_model::id::{PlanetId, PlayerId};

use crate::tuning;

/// One victory point in value points.
const VP: f64 = 10.0;
/// One trade good.
const TG: f64 = 1.0;
/// One action card.
const AC: f64 = 1.5;
/// One command token.
const CT: f64 = 2.5;
/// One technology.
const TECH: f64 = 5.0;
/// Rounds a game lasts in total (the remaining rounds scale the lasting effects).
const GAME_ROUNDS: f64 = 7.0;

// -- the table as data ----------------------------------------------------------------------

struct Unit {
    base: String,
    value: f64,
    system: String,
    planet: Option<String>,
    ship: bool,
    fighter: bool,
    ground: bool,
}

struct Planet {
    id: String,
    system: String,
    resources: i64,
    influence: i64,
    cultural: bool,
    specialty: bool,
    home: bool,
    ready: bool,
}

struct Seat {
    id: PlayerId,
    faction: String,
    vp: i32,
    tg: i32,
    ac: i32,
    techs: i32,
    tactic: i32,
    fleet: i32,
    strategy: i32,
    technology_card: bool,
    planets: Vec<Planet>,
    units: Vec<Unit>,
}

impl Seat {
    fn count(&self, base: &str) -> f64 {
        self.units.iter().filter(|u| u.base == base).count() as f64
    }

    fn influence_ready(&self) -> i64 {
        self.planets
            .iter()
            .filter(|p| p.ready)
            .map(|p| p.influence)
            .sum()
    }

    fn is(&self, faction: &str) -> bool {
        self.faction == faction
    }

    fn war_suns(&self) -> f64 {
        self.count("warsun")
    }

    fn owns_war_sun_tech(&self, techs: &[String]) -> bool {
        techs.iter().any(|t| t == "ws" || t == "pws2") || self.war_suns() > 0.0
    }
}

struct Ctx<'a> {
    seen: &'a Observed<'a>,
    me: usize,
    seats: Vec<Seat>,
    /// Technologies owned, per seat (aliases).
    techs: Vec<Vec<String>>,
    rl: f64,
}

fn planet_value(res: i64, inf: i64) -> f64 {
    0.6 * (res + inf) as f64
}

fn vp_gain(vp_now: i32, gain: i32) -> f64 {
    let value = f64::from(gain) * VP;
    if gain > 0 && vp_now + gain >= 10 {
        value * 4.0
    } else {
        value
    }
}

impl<'a> Ctx<'a> {
    fn build(seen: &'a Observed<'a>, me: &PlayerId) -> Option<Self> {
        let players = seen.players();
        let me_idx = players.iter().position(|p| *p == me)?;
        let content = seen.content();
        let sources = seen.sources();
        let mut seats: Vec<Seat> = Vec::new();
        let mut techs: Vec<Vec<String>> = Vec::new();
        let mut cache: BTreeMap<String, (String, bool, bool, bool, bool, f64)> = BTreeMap::new();
        for p in &players {
            let public = seen.seat(p)?;
            let mut planets = Vec::new();
            for (system, planet) in seen.controlled_planets(p) {
                let Some(record) = ti4_content::galaxy::planet(content, planet.as_str(), sources)
                else {
                    continue;
                };
                planets.push(Planet {
                    id: planet.to_string(),
                    system: system.to_string(),
                    resources: record.resources(),
                    influence: record.influence(),
                    cultural: record.has_trait("cultural"),
                    specialty: !record.tech_specialties().is_empty(),
                    home: record.homeworld_of().is_some(),
                    ready: seen.planet_is_ready(planet),
                });
            }
            let technology = ["Technology", "technology"];
            let tech_card = public
                .strategy_cards
                .iter()
                .any(|c| technology.iter().any(|t| c.as_str().contains(t)));
            techs.push(public.technologies.iter().map(ToString::to_string).collect());
            seats.push(Seat {
                id: (*p).clone(),
                faction: public.faction.to_string(),
                vp: public.victory_points,
                tg: public.trade_goods,
                ac: i32::try_from(public.action_cards_held).unwrap_or(0),
                techs: i32::try_from(public.technologies.len()).unwrap_or(0),
                tactic: public.tactic_tokens,
                fleet: public.fleet_tokens,
                strategy: public.strategic_tokens,
                technology_card: tech_card,
                planets,
                units: Vec::new(),
            });
        }
        for (system, here) in seen.board() {
            let mut add = |unit: &ti4_model::units::Unit, planet: Option<&PlanetId>| {
                let Some(idx) = players.iter().position(|p| **p == unit.owner) else {
                    return;
                };
                let info = cache
                    .entry(unit.type_id.to_string())
                    .or_insert_with(|| {
                        let t = ti4_content::units::unit_type(content, unit.type_id.as_str(), sources);
                        (
                            t.as_ref().map_or(String::new(), |t| t.base_type().to_owned()),
                            t.as_ref().is_some_and(|t| t.is_ship()),
                            t.as_ref().is_some_and(|t| t.is_fighter()),
                            t.as_ref().is_some_and(|t| t.is_ground_force()),
                            t.as_ref().is_some_and(|t| t.is_structure()),
                            ti4_engine::valuation::unit_value(content, sources, unit.type_id.as_str()),
                        )
                    })
                    .clone();
                seats[idx].units.push(Unit {
                    base: info.0,
                    value: info.5,
                    system: system.to_string(),
                    planet: planet.map(ToString::to_string),
                    ship: info.1,
                    fighter: info.2,
                    ground: info.3,
                });
            };
            for unit in &here.units {
                add(unit, None);
            }
            for (planet, units) in &here.planet_units {
                for unit in units {
                    add(unit, Some(planet));
                }
            }
        }
        let rl = (GAME_ROUNDS - f64::from(seen.round())).max(1.0);
        Some(Self {
            seen,
            me: me_idx,
            seats,
            techs,
            rl,
        })
    }

    fn n(&self) -> usize {
        self.seats.len()
    }

    fn idx(&self, player: &str) -> Option<usize> {
        self.seats.iter().position(|s| s.id.as_str() == player)
    }

    fn threat(&self, r: usize) -> f64 {
        (1.0 + 0.15 * f64::from(self.seats[r].vp - self.seats[self.me].vp)).clamp(0.5, 2.0)
    }

    /// The value of a change vector to the seat: mine minus the weighted rivals'.
    fn score(&self, delta: &[f64]) -> f64 {
        let n = self.n();
        if n < 2 {
            return delta[self.me];
        }
        let rivals: f64 = (0..n)
            .filter(|&r| r != self.me)
            .map(|r| self.threat(r) * delta[r])
            .sum::<f64>()
            / (n - 1) as f64;
        delta[self.me] - tuning::agenda_rival_weight() * ((n - 1) as f64).sqrt() * rivals
    }

    fn zero(&self) -> Vec<f64> {
        vec![0.0; self.n()]
    }

    fn all(&self, f: impl Fn(&Seat, usize) -> f64) -> Vec<f64> {
        (0..self.n()).map(|i| f(&self.seats[i], i)).collect()
    }

    fn mecatol_system(&self) -> Option<String> {
        crate::mecatol_system(self.seen).map(|s| s.to_string())
    }

    fn adjacent(&self, system: &str) -> Vec<String> {
        self.seen
            .galaxy()
            .map(|g| g.adjacent(system).into_iter().map(str::to_owned).collect())
            .unwrap_or_default()
    }

    fn wormhole_system(&self, system: &str) -> bool {
        self.seen.galaxy().is_some_and(|g| {
            g.wormhole_kinds(system)
                .iter()
                .any(|k| matches!(*k, "alpha" | "beta"))
        })
    }

    fn near_wormhole(&self, system: &str) -> bool {
        self.wormhole_system(system) || self.adjacent(system).iter().any(|s| self.wormhole_system(s))
    }

    fn planet_owner(&self, planet: &str) -> Option<usize> {
        self.seats
            .iter()
            .position(|s| s.planets.iter().any(|p| p.id == planet))
    }

    fn units_on(&self, planet: &str) -> Vec<(usize, &Unit)> {
        self.seats
            .iter()
            .enumerate()
            .flat_map(|(i, s)| {
                s.units
                    .iter()
                    .filter(move |u| u.planet.as_deref() == Some(planet))
                    .map(move |u| (i, u))
            })
            .collect()
    }

    fn neighbours(&self) -> f64 {
        ((self.n() - 1) as f64).min(2.0)
    }
}

// -- gains of an elected player ---------------------------------------------------------------

fn elect_gain(ctx: &Ctx<'_>, agenda: &str, i: usize) -> f64 {
    let s = &ctx.seats[i];
    let rl = ctx.rl;
    let mut g = match agenda {
        "secret" => 3.5,
        "committee" => 4.0,
        "arbiter" => 3.0,
        "minister_commerce" => ctx.neighbours() * rl * 0.7 * if s.is("hacan") { 1.5 } else { 1.0 },
        "minister_exploration" => {
            0.8 * rl * if matches!(s.faction.as_str(), "saar" | "arborec" | "sol") { 1.3 } else { 1.0 }
        }
        "minister_industry" => 2.5,
        "minister_peace" => 4.0,
        "minister_policy" => 1.5 * rl * 0.9 * if matches!(s.faction.as_str(), "yssaril" | "nekro") { 1.3 } else { 1.0 },
        "minister_sciences" => {
            2.5 * if s.technology_card { 1.6 } else { 1.0 }
                * if matches!(s.faction.as_str(), "jolnar" | "nekro" | "hacan") { 1.3 } else { 1.0 }
        }
        "minister_war" => 3.0,
        "prophecy" => {
            let fighters = s.units.iter().filter(|u| u.fighter).count();
            if fighters >= 4 { 1.5 } else { 0.3 }
        }
        "shard_of_the_throne" => {
            let mine = fleet_value(s);
            let best = ctx.seats.iter().map(fleet_value).fold(0.0, f64::max);
            let weaker = ctx
                .seats
                .iter()
                .enumerate()
                .filter(|(j, _)| *j != i)
                .all(|(_, o)| fleet_value(o) < mine * 1.2);
            let _ = best;
            let held: f64 = if weaker { 7.0 } else { 5.0 };
            held.max(vp_gain(s.vp, 1) * 0.6)
        }
        "crown_of_emphidia" => 8.0,
        "crown_of_thalnos" => 3.0 + if matches!(s.faction.as_str(), "sardakk" | "jolnar") { 0.5 } else { 0.0 },
        "standardization" => {
            let total = s.tactic + s.fleet + s.strategy;
            CT * 0.7 * f64::from(8 - total)
        }
        "censure" => {
            if s.is("yssaril") { 4.0 } else { 8.5 }
        }
        "grant_reallocation" => {
            if s.fleet >= 2 {
                1.5
            } else {
                0.0
            }
        }
        "warrant" => 4.5,
        "minister_antiquities" => 7.0 * if matches!(s.faction.as_str(), "empyrean" | "naaz") { 1.3 } else { 1.0 },
        _ => 0.0,
    };
    // Cards worth a victory point carry the win pressure of the point.
    if matches!(agenda, "shard_of_the_throne" | "crown_of_emphidia" | "censure") {
        let pressure = vp_gain(s.vp, 1) / VP;
        g *= pressure.max(1.0).min(2.5);
    }
    g
}

fn fleet_value(s: &Seat) -> f64 {
    s.units.iter().filter(|u| u.ship).map(|u| u.value).sum()
}

fn is_elect_player(agenda: &str) -> bool {
    matches!(
        agenda,
        "secret"
            | "committee"
            | "arbiter"
            | "minister_commerce"
            | "minister_exploration"
            | "minister_industry"
            | "minister_peace"
            | "minister_policy"
            | "minister_sciences"
            | "minister_war"
            | "prophecy"
            | "shard_of_the_throne"
            | "crown_of_emphidia"
            | "crown_of_thalnos"
            | "standardization"
            | "censure"
            | "grant_reallocation"
            | "warrant"
            | "minister_antiquities"
            | "execution"
    )
}

fn is_elect_planet(agenda: &str) -> bool {
    matches!(
        agenda,
        "core_mining"
            | "demilitarized_zone"
            | "holy_planet_of_ixth"
            | "rt_biotic"
            | "rt_cybernetic"
            | "rt_propulsion"
            | "rt_warfare"
            | "senate_sanctuary"
            | "terraforming_initiative"
            | "redistribution"
            | "disarmament"
    )
}

// -- ongoing value of a law in play, per seat -------------------------------------------------

fn law_standing(ctx: &Ctx<'_>, law: &str, outcome: &str) -> Vec<f64> {
    let mut d = ctx.zero();
    if is_elect_player(law) {
        if let Some(h) = ctx.idx(outcome) {
            d[h] = elect_gain(ctx, law, h);
        }
        return d;
    }
    if is_elect_planet(law) {
        if let Some(o) = ctx.planet_owner(outcome) {
            d[o] = planet_gain(ctx, law, outcome, o);
        }
        return d;
    }
    if outcome != "for" {
        return d;
    }
    match law {
        "regulations" => d = ctx.all(|s, _| -CT * f64::from((s.fleet - 4).max(0))),
        "sanctions" => d = ctx.all(|s, _| -0.7 * f64::from((s.ac - 3).max(0))),
        "conventions" => {
            d = ctx.all(|s, _| if matches!(s.faction.as_str(), "letnev" | "muaat") { -3.0 } else { 0.0 });
        }
        "travel_ban" => d = ctx.all(|s, _| if s.is("ghost") { -6.0 } else { -0.5 }),
        "defense_act" => d = ctx.all(|s, _| 0.3 * s.count("pds")),
        "schematics" => d = ctx.all(|s, _| -3.5 * s.war_suns()),
        "conscription" => {
            d = ctx.all(|s, _| {
                -0.3 * s
                    .units
                    .iter()
                    .filter(|u| u.fighter || u.base == "infantry")
                    .count() as f64
            });
        }
        "articles_war" => d = ctx.all(|s, _| -1.5 * s.count("mech")),
        "checks" => d = ctx.all(|_, _| -4.0),
        "rep_govt" | "representative_government" => {
            let avg: f64 = ctx.seats.iter().map(|s| s.influence_ready() as f64).sum::<f64>() / ctx.n() as f64;
            d = ctx.all(|s, _| 0.4 * (avg - s.influence_ready() as f64));
        }
        _ => {}
    }
    d
}

// -- planet elections --------------------------------------------------------------------------

fn planet_gain(ctx: &Ctx<'_>, agenda: &str, planet: &str, owner: usize) -> f64 {
    let rl = ctx.rl;
    let s = &ctx.seats[owner];
    let dock = ctx
        .units_on(planet)
        .iter()
        .any(|(_, u)| u.base == "spacedock");
    match agenda {
        "core_mining" => 1.1 * rl - 1.5,
        "terraforming_initiative" => 1.1 * rl,
        "senate_sanctuary" => rl * if matches!(s.faction.as_str(), "hacan" | "xxcha") { 1.3 } else { 1.0 },
        "holy_planet_of_ixth" => vp_gain(s.vp, 1) * 0.9 - if dock { 4.0 } else { 0.0 },
        "rt_biotic" | "rt_cybernetic" | "rt_propulsion" => 2.5,
        "rt_warfare" => 2.5 * if s.is("muaat") { 1.5 } else { 1.0 },
        _ => 0.0,
    }
}

fn planet_deltas(ctx: &Ctx<'_>, agenda: &str, planet: &str) -> Vec<f64> {
    let mut d = ctx.zero();
    let Some(owner) = ctx.planet_owner(planet) else {
        return d;
    };
    match agenda {
        "redistribution" | "demilitarized_zone" => {
            for (i, u) in ctx.units_on(planet) {
                d[i] -= u.value;
            }
            if agenda == "demilitarized_zone" {
                let p = ctx.seats[owner].planets.iter().find(|p| p.id == planet);
                d[owner] -= 1.0 + p.map_or(0.0, |p| 0.3 * p.resources as f64 * ctx.rl);
            }
        }
        "disarmament" => {
            for (i, u) in ctx.units_on(planet) {
                if u.ground {
                    d[i] -= u.value;
                    d[owner] += TG;
                }
            }
        }
        _ => d[owner] = planet_gain(ctx, agenda, planet, owner),
    }
    d
}

// -- for / against -----------------------------------------------------------------------------

/// The value change of every seat for `outcome` (`for` / `against`) of a For/Against agenda.
fn for_against(ctx: &Ctx<'_>, agenda: &str, outcome: &str) -> Vec<f64> {
    let f = outcome == "for";
    let rl = ctx.rl;
    let n = ctx.n();
    match agenda {
        "revolution" => {
            if f {
                ctx.all(|s, _| {
                    let cheapest = s
                        .units
                        .iter()
                        .filter(|u| u.ship && !u.fighter)
                        .map(|u| u.value)
                        .fold(f64::INFINITY, f64::min);
                    if cheapest.is_finite() { -cheapest * (rl * 0.8).min(4.0) } else { 0.0 }
                })
            } else {
                ctx.all(|s, _| {
                    let mut vals: Vec<f64> = s
                        .planets
                        .iter()
                        .map(|p| planet_value(p.resources, p.influence))
                        .collect();
                    vals.sort_by(f64::total_cmp);
                    -vals.iter().take(s.techs.max(0) as usize).sum::<f64>()
                })
            }
        }
        "arms_reduction" => {
            if f {
                ctx.all(|s, _| {
                    let dn = (s.count("dreadnought") - 2.0).max(0.0);
                    let cr = (s.count("cruiser") - 4.0).max(0.0);
                    -(dn * 4.0 + cr * 2.0)
                })
            } else {
                ctx.all(|s, _| {
                    -0.6 * s
                        .planets
                        .iter()
                        .filter(|p| p.specialty)
                        .map(|p| (p.resources + p.influence) as f64)
                        .sum::<f64>()
                })
            }
        }
        "artifact" => {
            if f {
                let mr = ctx.mecatol_system();
                let adj = mr.as_deref().map(|m| ctx.adjacent(m)).unwrap_or_default();
                ctx.all(|s, _| {
                    let mut loss = 0.0;
                    if let Some(m) = &mr {
                        loss += s.units.iter().filter(|u| &u.system == m).map(|u| u.value).sum::<f64>();
                    }
                    for a in &adj {
                        let mut vals: Vec<f64> = s
                            .units
                            .iter()
                            .filter(|u| &u.system == a)
                            .map(|u| u.value)
                            .collect();
                        vals.sort_by(f64::total_cmp);
                        loss += vals.iter().take(3).sum::<f64>();
                    }
                    0.5 * (2.0 * TECH * 0.8) - 0.5 * loss
                })
            } else {
                ctx.zero()
            }
        }
        "plowshares" => {
            if f {
                ctx.all(|s, _| {
                    let inf = s.units.iter().filter(|u| u.base == "infantry").count() as f64;
                    let destroyed = (inf / 2.0).ceil();
                    // A trade good each, the infantry only worth 0.8.
                    destroyed * (TG - 0.8)
                })
            } else {
                ctx.all(|s, _| 0.8 * s.planets.len() as f64)
            }
        }
        "rearmament" => {
            if f {
                ctx.all(|s, _| if s.count("mech") < 4.0 { 3.5 } else { 0.0 })
            } else {
                ctx.all(|s, _| -2.5 * s.count("mech"))
            }
        }
        "schematics" => {
            let owner_exists = (0..n).any(|i| ctx.seats[i].owns_war_sun_tech(&ctx.techs[i]));
            if f {
                ctx.all(|s, i| {
                    let mut v = if owner_exists && !s.owns_war_sun_tech(&ctx.techs[i]) { 2.0 } else { 0.0 };
                    v -= 3.5 * s.war_suns();
                    if s.is("muaat") {
                        v -= 15.0;
                    }
                    v
                })
            } else {
                ctx.all(|s, i| {
                    if s.owns_war_sun_tech(&ctx.techs[i]) { -AC * f64::from(s.ac) } else { 0.0 }
                })
            }
        }
        "articles_war" => {
            if f {
                ctx.all(|s, _| -1.5 * s.count("mech"))
            } else {
                ctx.zero()
            }
        }
        "conscription" => {
            if f {
                ctx.all(|s, _| {
                    let cheap = s.units.iter().filter(|u| u.fighter || u.base == "infantry").count() as f64;
                    -(0.3 * cheap).min(5.0)
                })
            } else {
                ctx.zero()
            }
        }
        "regulations" => {
            if f {
                ctx.all(|s, _| -CT * f64::from((s.fleet - 4).max(0)))
            } else {
                ctx.all(|_, _| CT)
            }
        }
        "cladenstine" => {
            if f {
                ctx.all(|s, _| -CT * f64::from((s.tactic + s.fleet + s.strategy).min(2)))
            } else {
                ctx.all(|s, _| if s.fleet > 0 { -CT } else { 0.0 })
            }
        }
        "shared_research" => {
            if f {
                ctx.all(|s, _| if s.is("empyrean") { -3.0 } else { 0.5 })
            } else {
                ctx.all(|_, _| -1.5)
            }
        }
        "checks" => {
            if f {
                ctx.all(|_, _| -4.0)
            } else {
                ctx.all(|s, _| {
                    let mut vals: Vec<f64> = s
                        .planets
                        .iter()
                        .map(|p| planet_value(p.resources, p.influence))
                        .collect();
                    vals.sort_by(|a, b| b.total_cmp(a));
                    -0.5 * vals.iter().skip(3).sum::<f64>()
                })
            }
        }
        "rep_govt" | "representative_government" => {
            if f {
                let avg: f64 = ctx.seats.iter().map(|s| s.influence_ready() as f64).sum::<f64>() / n as f64;
                ctx.all(|s, _| 0.4 * rl * (avg - s.influence_ready() as f64) * 0.5)
            } else {
                ctx.zero()
            }
        }
        "constitution" => {
            if f {
                let mut d = ctx.all(|s, _| {
                    -0.6 * s
                        .planets
                        .iter()
                        .filter(|p| p.home)
                        .map(|p| (p.resources + p.influence) as f64)
                        .sum::<f64>()
                });
                for (law, outcome) in ctx.seen.laws() {
                    let standing = law_standing(ctx, law, outcome);
                    for (x, st) in d.iter_mut().zip(standing) {
                        *x -= st;
                    }
                }
                d
            } else {
                ctx.zero()
            }
        }
        "economic_equality" => {
            if f {
                ctx.all(|s, _| 5.0 * TG - f64::from(s.tg) * TG)
            } else {
                ctx.all(|s, _| -f64::from(s.tg) * TG)
            }
        }
        "sanctions" => {
            if f {
                ctx.all(|s, _| -AC * f64::from((s.ac - 3).max(0)))
            } else {
                ctx.all(|s, _| if s.ac > 0 { -AC } else { 0.0 })
            }
        }
        "unconventional" => {
            // The voters' part is in `conditional`.
            ctx.zero()
        }
        "mutiny" => ctx.zero(),
        "seed_empire" => {
            let top = ctx.seats.iter().map(|s| s.vp).max().unwrap_or(0);
            let low = ctx.seats.iter().map(|s| s.vp).min().unwrap_or(0);
            if f {
                ctx.all(|s, _| if s.vp == top { vp_gain(s.vp, 1) } else { 0.0 })
            } else {
                ctx.all(|s, _| if s.vp == low { vp_gain(s.vp, 1) } else { 0.0 })
            }
        }
        "incentive" => {
            let lead = ctx.seats[ctx.me].vp
                - (ctx.seats.iter().map(|s| s.vp).sum::<i32>() as f64 / n as f64).round() as i32;
            // The old rule (For) unless I clearly lead.
            let mut d = ctx.zero();
            d[ctx.me] = if f { 0.3 } else { 0.4 * f64::from(lead - 1).max(0.0) };
            d
        }
        "travel_ban" => {
            if f {
                ctx.all(|s, _| {
                    if s.is("ghost") {
                        -10.0
                    } else if s
                        .planets
                        .iter()
                        .any(|p| ctx.wormhole_system(&p.system))
                    {
                        -3.0
                    } else {
                        0.0
                    }
                })
            } else {
                ctx.all(|s, _| {
                    -2.0 * s
                        .units
                        .iter()
                        .filter(|u| u.base == "pds" && ctx.near_wormhole(&u.system))
                        .count() as f64
                })
            }
        }
        "defense_act" => {
            if f {
                ctx.all(|s, _| 0.3 * s.count("pds"))
            } else {
                ctx.all(|s, _| if s.count("pds") > 0.0 { -2.0 } else { 0.0 })
            }
        }
        "wormhole_recon" => {
            if f {
                ctx.all(|s, _| if matches!(s.faction.as_str(), "ghost" | "cabal" | "empyrean") { 3.0 } else { 0.0 })
            } else {
                ctx.all(|s, _| {
                    let mut systems: Vec<&str> = s
                        .units
                        .iter()
                        .filter(|u| u.ship && ctx.wormhole_system(&u.system))
                        .map(|u| u.system.as_str())
                        .collect();
                    systems.sort_unstable();
                    systems.dedup();
                    -1.5 * systems.len() as f64
                })
            }
        }
        "nexus" => {
            if f {
                ctx.zero()
            } else {
                let mr = ctx.mecatol_system();
                ctx.all(|s, _| {
                    mr.as_deref()
                        .filter(|m| s.units.iter().any(|u| &u.system == m && u.ground))
                        .map_or(0.0, |_| -1.5)
                })
            }
        }
        "wormhole_research" => {
            if f {
                ctx.all(|s, _| {
                    let lost: f64 = s
                        .units
                        .iter()
                        .filter(|u| u.ship && ctx.wormhole_system(&u.system))
                        .map(|u| u.value)
                        .sum();
                    let has = lost > 0.0;
                    (if has { TECH } else { 0.0 }) - lost
                })
            } else {
                ctx.zero()
            }
        }
        "conventions" => {
            if f {
                ctx.all(|s, _| if matches!(s.faction.as_str(), "letnev" | "muaat") { -3.0 } else { 0.0 })
            } else {
                ctx.zero()
            }
        }
        "covert" => ctx.zero(),
        _ => ctx.zero(),
    }
}

/// What the seat itself gets or loses for having voted `side` when `outcome` wins.
fn conditional(ctx: &Ctx<'_>, agenda: &str, side: &str, outcome: &str) -> f64 {
    let me = &ctx.seats[ctx.me];
    match (agenda, side, outcome) {
        ("mutiny", "for", "for") => vp_gain(me.vp, 1),
        ("mutiny", "for", "against") => vp_gain(me.vp, -1),
        ("unconventional", "for", "for") => 2.0 * AC,
        ("unconventional", "for", "against") => -AC * f64::from(me.ac),
        ("articles_war", "for", "against") => 3.0 * TG,
        ("conventions", "against", "against") => -AC * f64::from(me.ac),
        ("rep_govt" | "representative_government", "against", "against") => {
            -0.6 * me
                .planets
                .iter()
                .filter(|p| p.cultural)
                .map(|p| (p.resources + p.influence) as f64)
                .sum::<f64>()
        }
        ("wormhole_research", "against", "against") => -CT,
        _ => 0.0,
    }
}

/// The rivals' part of a voter-conditional agenda: every rival that voted the side when it wins.
/// Treated as an even split of the rivals.
fn rival_conditional(ctx: &Ctx<'_>, agenda: &str, outcome: &str) -> Vec<f64> {
    let mut d = ctx.zero();
    let share = 0.5;
    for r in (0..ctx.n()).filter(|&r| r != ctx.me) {
        let s = &ctx.seats[r];
        d[r] = share
            * match (agenda, outcome) {
                ("mutiny", "for") => vp_gain(s.vp, 1),
                ("mutiny", "against") => vp_gain(s.vp, -1),
                ("unconventional", "for") => 2.0 * AC,
                ("unconventional", "against") => -AC * f64::from(s.ac),
                ("articles_war", "against") => 3.0 * TG,
                ("conventions", "against") => -AC * f64::from(s.ac),
                _ => 0.0,
            };
    }
    d
}

fn is_voter_conditional(agenda: &str) -> bool {
    matches!(
        agenda,
        "mutiny" | "unconventional" | "articles_war" | "conventions" | "rep_govt" | "representative_government" | "wormhole_research"
    )
}

// -- the score of every outcome ---------------------------------------------------------------

/// The value deltas of `outcome` for all seats.
fn outcome_deltas(ctx: &Ctx<'_>, agenda: &str, outcome: &str) -> Vec<f64> {
    if agenda == "execution" {
        let mut d = ctx.zero();
        if let Some(x) = ctx.idx(outcome) {
            let s = &ctx.seats[x];
            let speaker = if ctx.seen.speaker().as_str() == outcome { 2.0 } else { 0.0 };
            d[x] = -(AC * f64::from(s.ac) + 0.6 * s.influence_ready() as f64 + speaker);
        }
        return d;
    }
    if is_elect_player(agenda) {
        let mut d = ctx.zero();
        if let Some(x) = ctx.idx(outcome) {
            d[x] = elect_gain(ctx, agenda, x);
            // Standardization: nobody gains by being elected above 8 tokens.
            if agenda == "standardization" {
                d[x] = d[x].max(-6.0);
            }
        }
        return d;
    }
    if is_elect_planet(agenda) {
        return planet_deltas(ctx, agenda, outcome);
    }
    match agenda {
        "abolishment" | "miscount" => {
            let k = if agenda == "abolishment" { 1.0 } else { 0.5 };
            let law_outcome = ctx.seen.law_outcome(outcome).unwrap_or("for").to_owned();
            law_standing(ctx, outcome, &law_outcome).into_iter().map(|v| -k * v).collect()
        }
        _ => {
            let mut d = for_against(ctx, agenda, outcome);
            if is_voter_conditional(agenda) {
                for (x, c) in d.iter_mut().zip(rival_conditional(ctx, agenda, outcome)) {
                    *x += c;
                }
            }
            d
        }
    }
}

/// Scores of every outcome, best first.
#[derive(Debug, Clone)]
pub struct Verdict {
    /// `(outcome, score)`, best first.
    pub scores: Vec<(String, f64)>,
}

impl Verdict {
    /// The best outcome and its margin to the second best.
    #[must_use]
    pub fn best(&self) -> Option<(&str, f64)> {
        let first = self.scores.first()?;
        let second = self.scores.get(1).map_or(first.1 - 0.0, |s| s.1);
        Some((first.0.as_str(), first.1 - second))
    }

    /// The score of one outcome.
    #[must_use]
    pub fn score_of(&self, outcome: &str) -> f64 {
        self.scores
            .iter()
            .find(|(o, _)| o == outcome)
            .map_or(0.0, |(_, s)| *s)
    }
}

/// Value every outcome for `me`. `None` when the position is unreadable.
#[must_use]
pub fn evaluate(
    agenda: &str,
    outcomes: &[&str],
    seen: &Observed<'_>,
    me: &PlayerId,
) -> Option<Verdict> {
    let ctx = Ctx::build(seen, me)?;
    evaluate_in(&ctx, agenda, outcomes)
}

fn evaluate_in(ctx: &Ctx<'_>, agenda: &str, outcomes: &[&str]) -> Option<Verdict> {
    if outcomes.is_empty() {
        return None;
    }
    let mut scores: Vec<(String, f64)> = outcomes
        .iter()
        .map(|o| {
            let d = outcome_deltas(ctx, agenda, o);
            ((*o).to_owned(), ctx.score(&d))
        })
        .collect();
    scores.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    Some(Verdict { scores })
}

/// The outcome the seat wants, when its margin clears the threshold. Voter-conditional agendas
/// are decided by [`plan_vote`], this gives their unconditional preference.
#[must_use]
pub fn preferred(
    agenda: &str,
    outcomes: &[&str],
    seen: &Observed<'_>,
    me: &PlayerId,
) -> Option<String> {
    let verdict = evaluate(agenda, outcomes, seen, me)?;
    let (best, margin) = verdict.best()?;
    (margin >= tuning::agenda_min_margin()).then(|| best.to_owned())
}

// -- the vote model ----------------------------------------------------------------------------

fn phi(x: f64) -> f64 {
    // Logistic approximation of the normal CDF.
    1.0 / (1.0 + (-1.702 * x).exp())
}

/// What is known about the vote at the moment of the seat's own cast.
struct Ballot {
    outcomes: Vec<String>,
    tally: Vec<f64>,
    /// Forecast votes still to come, per outcome.
    forecast: Vec<f64>,
    /// Variance of those forecasts.
    var: f64,
}

impl Ballot {
    /// Probability of every outcome winning when the seat adds `v` votes to `x`.
    fn probabilities(&self, x: Option<usize>, v: f64) -> Vec<f64> {
        let totals: Vec<f64> = (0..self.outcomes.len())
            .map(|o| self.tally[o] + self.forecast[o] + if Some(o) == x { v } else { 0.0 })
            .collect();
        let sd = (self.var + 1.0).sqrt();
        let mut p: Vec<f64> = (0..totals.len())
            .map(|o| {
                let rival = totals
                    .iter()
                    .enumerate()
                    .filter(|(j, _)| *j != o)
                    .map(|(_, t)| *t)
                    .fold(f64::NEG_INFINITY, f64::max);
                if rival.is_finite() { phi((totals[o] - rival) / sd) } else { 1.0 }
            })
            .collect();
        let sum: f64 = p.iter().sum();
        if sum > 0.0 {
            for q in &mut p {
                *q /= sum;
            }
        }
        p
    }
}

fn voting_order_after_me(seen: &Observed<'_>, me: &PlayerId) -> Vec<PlayerId> {
    let players = seen.players();
    let speaker = seen.speaker();
    let Some(s) = players.iter().position(|p| *p == speaker) else {
        return Vec::new();
    };
    let order: Vec<PlayerId> = players[s + 1..]
        .iter()
        .chain(players[..=s].iter())
        .map(|p| (*p).clone())
        .collect();
    match order.iter().position(|p| p == me) {
        Some(i) => order[i + 1..].to_vec(),
        None => Vec::new(),
    }
}

fn tally_of(choice: &Choice) -> BTreeMap<String, f64> {
    choice
        .options
        .iter()
        .filter(|o| !o.is_decline())
        .map(|o| {
            let v = o
                .payload
                .get("current_votes")
                .and_then(serde_json::Value::as_f64)
                .unwrap_or(0.0);
            (o.id.clone(), v)
        })
        .collect()
}

fn ballot_for(
    agenda: &str,
    outcomes: &[String],
    tally: &BTreeMap<String, f64>,
    after: &[PlayerId],
    seen: &Observed<'_>,
    me: &PlayerId,
) -> Ballot {
    let refs: Vec<&str> = outcomes.iter().map(String::as_str).collect();
    let mut forecast = vec![0.0; outcomes.len()];
    let mut var = 0.0;
    let share = tuning::agenda_rival_share();
    for r in after {
        let infl = influence_ready_of(seen, r);
        if infl <= 0.0 {
            continue;
        }
        // The rival's own valuation, else what the old fixed table would vote (the other bots).
        // On the agendas that reward the voters the valuation of a bystander is misleading (it
        // sees only the rivals' gain), so the other bots are forecast by the old table.
        let pref = if is_voter_conditional(agenda) {
            crate::answers::old_preference(agenda, &refs, seen, r)
        } else {
            preferred(agenda, &refs, seen, r)
                .or_else(|| crate::answers::old_preference(agenda, &refs, seen, r))
        }
        .and_then(|o| outcomes.iter().position(|x| *x == o));
        match pref {
            Some(o) => {
                forecast[o] += 0.8 * infl;
                var += (0.4 * infl).powi(2);
            }
            None => {
                // Unknown: against the seat's choice with the rival share, spread over the others.
                let _ = (me, share);
                var += (0.3 * infl).powi(2);
            }
        }
    }
    Ballot {
        outcomes: outcomes.to_vec(),
        tally: outcomes.iter().map(|o| tally.get(o).copied().unwrap_or(0.0)).collect(),
        forecast,
        var,
    }
}

fn influence_ready_of(seen: &Observed<'_>, player: &PlayerId) -> f64 {
    seen.controlled_planets(player)
        .into_iter()
        .filter(|(_, p)| seen.planet_is_ready(p))
        .filter_map(|(_, p)| {
            ti4_content::galaxy::planet(seen.content(), p.as_str(), seen.sources())
        })
        .map(|r| r.influence())
        .sum::<i64>() as f64
}

fn my_planet_influences(seen: &Observed<'_>, me: &PlayerId) -> Vec<i64> {
    seen.controlled_planets(me)
        .into_iter()
        .filter(|(_, p)| seen.planet_is_ready(p))
        .filter_map(|(_, p)| ti4_content::galaxy::planet(seen.content(), p.as_str(), seen.sources()))
        .map(|r| r.influence())
        .filter(|i| *i > 0)
        .collect()
}

// -- memory across the decisions of one agenda phase ------------------------------------------

#[derive(Default)]
struct Memory {
    /// Agendas this seat voted on (or was asked about) in the round: `(seat, round, alias)`.
    asked: Vec<(String, u32, String)>,
    /// Politics placements by this seat: `(seat, round, alias, on top)`.
    placed: Vec<(String, u32, String, bool)>,
    /// The vote plan under way for the seat.
    plan: Option<Plan>,
    /// Rider played: `(seat, round, agenda)`.
    rider: Option<(String, u32, String)>,
}

#[derive(Clone)]
struct Plan {
    seat: String,
    outcome: String,
    target: f64,
    cast: f64,
}

thread_local! {
    static MEMORY: RefCell<Memory> = RefCell::new(Memory::default());
}

/// Forget everything (tests).
pub fn reset() {
    MEMORY.with(|m| *m.borrow_mut() = Memory::default());
}

impl Memory {
    /// A round lower than one remembered means a new game started on this thread: forget.
    fn sync(&mut self, round: u32) {
        let newest = self
            .asked
            .iter()
            .map(|e| e.1)
            .chain(self.placed.iter().map(|e| e.1))
            .chain(self.rider.iter().map(|e| e.1))
            .max()
            .unwrap_or(0);
        if newest > round {
            self.asked.clear();
            self.placed.clear();
            self.rider = None;
        }
    }
}

/// Record a Politics placement the seat made.
pub fn note_placement(seat: &PlayerId, round: u32, agenda: &str, top: bool) {
    MEMORY.with(|m| {
        let mut m = m.borrow_mut();
        m.sync(round);
        m.placed.retain(|(_, r, _, _)| *r + 1 >= round);
        m.placed
            .push((seat.to_string(), round, agenda.to_owned(), top));
    });
}

/// The agendas the seat expects on top of the deck, from its own Politics placements this round
/// (the last card placed on top is revealed first).
fn expected_top(seat: &PlayerId, round: u32) -> Vec<String> {
    MEMORY.with(|m| {
        let m = m.borrow();
        let mut deck: Vec<String> = Vec::new();
        for (s, r, alias, top) in &m.placed {
            if s == seat.as_str() && *r == round {
                if *top {
                    deck.insert(0, alias.clone());
                }
            }
        }
        deck
    })
}

/// Whether the seat was already asked about another agenda this round (so this is the second).
fn is_second(seat: &PlayerId, round: u32, agenda: &str) -> bool {
    MEMORY.with(|m| {
        let mut m = m.borrow_mut();
        m.sync(round);
        m.asked.retain(|(_, r, _)| *r + 1 >= round);
        let second = m
            .asked
            .iter()
            .any(|(s, r, a)| s == seat.as_str() && *r == round && a != agenda);
        if !m
            .asked
            .iter()
            .any(|(s, r, a)| s == seat.as_str() && *r == round && a == agenda)
        {
            m.asked.push((seat.to_string(), round, agenda.to_owned()));
        }
        second
    })
}

// -- target agenda targets ---------------------------------------------------------------------

/// The outcomes an agenda would offer, from its printed target (as the engine reads it).
pub fn outcomes_for(seen: &Observed<'_>, agenda: &str) -> Vec<String> {
    outcomes_of(seen, agenda)
}

fn outcomes_of(seen: &Observed<'_>, agenda: &str) -> Vec<String> {
    let Some((_, target)) = crate::backward::agenda::agenda_kind(seen, agenda) else {
        return vec!["for".to_owned(), "against".to_owned()];
    };
    let head = target.split('(').next().unwrap_or(&target).trim().to_owned();
    if !head.starts_with("Elect") {
        return vec!["for".to_owned(), "against".to_owned()];
    }
    if head.contains("Player") {
        return seen.players().iter().map(|p| p.to_string()).collect();
    }
    if head.contains("Planet") {
        let mut planets: Vec<String> = seen
            .board()
            .values()
            .flat_map(|s| s.planet_control.keys())
            .map(ToString::to_string)
            .collect();
        planets.sort();
        planets.dedup();
        if head.contains("Non-Home") || head.contains("Other Than Mecatol") {
            planets.retain(|p| {
                p != "mr"
                    && ti4_content::galaxy::planet(seen.content(), p, seen.sources())
                        .is_none_or(|r| r.homeworld_of().is_none())
            });
        }
        return planets;
    }
    if head.contains("Law") {
        return seen.laws().keys().cloned().collect();
    }
    Vec::new()
}

/// The margin of the next agenda for the seat (its stake), when that agenda is known.
fn known_stake(seen: &Observed<'_>, me: &PlayerId, agenda: &str) -> f64 {
    let outcomes = outcomes_of(seen, agenda);
    let refs: Vec<&str> = outcomes.iter().map(String::as_str).collect();
    evaluate(agenda, &refs, seen, me)
        .and_then(|v| v.best().map(|(_, m)| m))
        .unwrap_or(0.0)
}

// -- the vote itself ---------------------------------------------------------------------------

/// What `cast_vote` decided.
pub struct Cast {
    /// The option to pick (an outcome or the abstention).
    pub index: usize,
}

/// Decide the outcome and the number of votes of a cast-vote decision. Stores the plan the
/// planet decisions follow. `None` when the knobs do not apply (the old table decides).
pub fn plan_vote(
    choice: &Choice,
    seen: &Observed<'_>,
    me: &PlayerId,
    agenda: &str,
    old: Option<String>,
) -> Option<Cast> {
    let value = tuning::agenda_value();
    let budget = tuning::agenda_budget();
    if !value && !budget {
        return None;
    }
    clear_plan();
    let round = choice.context.as_ref().map_or(seen.round(), |c| c.round);
    let second = is_second(me, round, agenda);
    let outcomes: Vec<String> = choice
        .options
        .iter()
        .filter(|o| !o.is_decline())
        .map(|o| o.id.clone())
        .collect();
    let refs: Vec<&str> = outcomes.iter().map(String::as_str).collect();
    let abstain = choice.options.iter().position(ChoiceOption::is_decline);
    let influences = my_planet_influences(seen, me);
    let available: i64 = influences.iter().sum();
    let tally = tally_of(choice);
    let after = voting_order_after_me(seen, me);

    // Which outcomes to consider, with their scores.
    let verdict = if value { evaluate(agenda, &refs, seen, me)? } else { Verdict { scores: Vec::new() } };
    let ctx = Ctx::build(seen, me);
    let conditional_agenda = value && is_voter_conditional(agenda);

    // Without the valuation the old table names the outcome; the stake is a default.
    let candidates: Vec<String> = if value {
        verdict.scores.iter().take(3).map(|(o, _)| o.clone()).collect()
    } else {
        old.iter().cloned().collect()
    };
    if candidates.is_empty() {
        return None;
    }
    let score_of = |o: &str| -> f64 {
        if value {
            verdict.score_of(o)
        } else if Some(o) == old.as_deref() {
            3.0
        } else {
            0.0
        }
    };
    // Unconditional preference: margin of the best over the rest.
    let margin = if value { verdict.best()?.1 } else { old.as_ref().map(|_| 3.0)? };
    let ballot = ballot_for(agenda, &outcomes, &tally, &after, seen, me);
    // The chance the old table's outcome would win with every planet (for the experiment log).
    let p_old = old
        .as_ref()
        .and_then(|o| outcomes.iter().position(|x| x == o))
        .map(|k| ballot.probabilities(Some(k), available as f64)[k]);
    if !conditional_agenda && margin < tuning::agenda_min_margin() {
        // No opinion: abstain and keep every planet.
        log_vote(seen, me, agenda, round, second, &old, None, 0, available, margin, None, p_old);
        return abstain.map(|index| Cast { index });
    }

    let lambda = if budget && !second {
        let stake2 = {
            let next = expected_top(me, round);
            match next.get(1) {
                Some(a) if next.first().map(String::as_str) == Some(agenda) => known_stake(seen, me, a).abs(),
                _ => tuning::agenda_second_prior(),
            }
        };
        let rivals: f64 = seen
            .players()
            .iter()
            .filter(|p| **p != me)
            .map(|p| (0.35 * influence_ready_of(seen, p)).powi(2))
            .sum();
        stake2 * 0.4 / rivals.sqrt().max(2.0)
    } else {
        0.0
    };

    // Subset sums reachable with my planets.
    let mut levels: Vec<i64> = vec![0];
    if budget {
        let mut reach = vec![false; (available + 1) as usize];
        reach[0] = true;
        for &i in &influences {
            for s in (0..=(available - i).max(-1)).rev() {
                if s >= 0 && reach[s as usize] && (s + i) <= available {
                    reach[(s + i) as usize] = true;
                }
            }
        }
        levels = (0..=available).filter(|&s| reach[s as usize]).collect();
    } else if available > 0 {
        levels.push(available);
    }

    // Abstaining: nobody adds anything.
    let mut best_choice: (f64, Option<usize>, i64) = {
        let p = ballot.probabilities(None, 0.0);
        let ev: f64 = (0..outcomes.len())
            .map(|o| p[o] * score_of(&outcomes[o]))
            .sum();
        (ev, None, 0)
    };
    for cand in &candidates {
        let Some(x) = outcomes.iter().position(|o| o == cand) else { continue };
        for &v in &levels {
            if v == 0 {
                continue;
            }
            let p = ballot.probabilities(Some(x), v as f64);
            let mut ev: f64 = (0..outcomes.len())
                .map(|o| p[o] * score_of(&outcomes[o]))
                .sum();
            if let Some(ctx) = &ctx {
                if conditional_agenda {
                    let side = cand.as_str();
                    ev += (0..outcomes.len())
                        .map(|o| p[o] * conditional(ctx, agenda, side, &outcomes[o]))
                        .sum::<f64>();
                }
            }
            ev -= lambda * v as f64;
            if ev > best_choice.0 + 1e-9 {
                best_choice = (ev, Some(x), v);
            }
        }
    }
    let Some(x) = best_choice.1 else {
        log_vote(seen, me, agenda, round, second, &old, None, 0, available, margin, None, p_old);
        return abstain.map(|index| Cast { index });
    };
    // Never cast more than needed for the same probability: the smallest level within 1% of EV.
    let mut v = best_choice.2;
    if budget {
        let top = best_choice.0;
        let cand = &outcomes[x];
        let _ = cand;
        for &level in &levels {
            if level == 0 || level >= v {
                continue;
            }
            let p = ballot.probabilities(Some(x), level as f64);
            let mut ev: f64 = (0..outcomes.len())
                .map(|o| p[o] * score_of(&outcomes[o]))
                .sum();
            if let Some(ctx) = &ctx {
                if conditional_agenda {
                    ev += (0..outcomes.len())
                        .map(|o| p[o] * conditional(ctx, agenda, &outcomes[x], &outcomes[o]))
                        .sum::<f64>();
                }
            }
            ev -= lambda * level as f64;
            if ev >= top - 0.01 * top.abs().max(0.5) {
                v = level;
                break;
            }
        }
    } else {
        v = available;
    }
    store_plan(me, &outcomes[x], v as f64);
    let p_new = ballot.probabilities(Some(x), v as f64)[x];
    log_vote(seen, me, agenda, round, second, &old, Some(&outcomes[x]), v, available, margin, Some(p_new), p_old);
    choice
        .options
        .iter()
        .position(|o| o.id == outcomes[x])
        .map(|index| Cast { index })
}

/// Whether a vote plan of this seat is in force.
#[must_use]
pub fn has_plan(me: &PlayerId) -> bool {
    MEMORY.with(|m| m.borrow().plan.as_ref().is_some_and(|p| p.seat == me.as_str()))
}

fn clear_plan() {
    MEMORY.with(|m| m.borrow_mut().plan = None);
}

/// Append a line to the file named by `AGENDA_VOTE_LOG` (experiments only).
#[allow(clippy::too_many_arguments)]
fn log_vote(
    seen: &Observed<'_>,
    me: &PlayerId,
    agenda: &str,
    round: u32,
    second: bool,
    old: &Option<String>,
    chosen: Option<&str>,
    votes: i64,
    available: i64,
    margin: f64,
    p_win: Option<f64>,
    p_old: Option<f64>,
) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        use std::io::Write;
        let Some(path) = std::env::var_os("AGENDA_VOTE_LOG") else {
            return;
        };
        let faction = seen.seat(me).map(|s| s.faction.to_string()).unwrap_or_default();
        let line = serde_json::json!({
            "k": "vote", "agenda": agenda, "round": round, "seat": me.as_str(), "faction": faction,
            "second": second, "old": old, "new": chosen, "votes": votes, "available": available,
            "margin": margin, "p_win": p_win, "p_old": p_old,
            "value": tuning::agenda_value(), "budget": tuning::agenda_budget(),
        });
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
            let _ = f.write_all(format!("{line}\n").as_bytes());
        }
    }
    #[cfg(target_arch = "wasm32")]
    let _ = (seen, me, agenda, round, second, old, chosen, votes, available, margin, p_win, p_old);
}

fn store_plan(me: &PlayerId, outcome: &str, target: f64) {
    MEMORY.with(|m| {
        m.borrow_mut().plan = Some(Plan {
            seat: me.to_string(),
            outcome: outcome.to_owned(),
            target,
            cast: 0.0,
        });
    });
}

fn votes_of(option: &ChoiceOption) -> i64 {
    option
        .label
        .split(" for ")
        .nth(1)
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|n| n.parse().ok())
        .unwrap_or(0)
}

/// The planet decision that follows the plan: the cheapest subset of planets reaching the target,
/// one planet at a time; decline once the target is reached. `None`: no plan for this decision.
pub fn pick_planet(choice: &Choice, me: &PlayerId, outcome: &str) -> Option<usize> {
    let plan = MEMORY.with(|m| m.borrow().plan.clone())?;
    if plan.seat != me.as_str() || plan.outcome != outcome {
        return None;
    }
    let decline = choice.options.iter().position(ChoiceOption::is_decline);
    let need = plan.target - plan.cast;
    if need <= 0.0 {
        return decline;
    }
    let planets: Vec<(usize, i64)> = choice
        .options
        .iter()
        .enumerate()
        .filter(|(_, o)| !o.is_decline())
        .map(|(i, o)| (i, votes_of(o)))
        .filter(|(_, v)| *v > 0)
        .collect();
    if planets.is_empty() {
        return decline;
    }
    // Smallest total >= need (then fewest planets); everything when out of reach.
    let n = planets.len().min(16);
    let mut best: Option<(i64, usize, u32)> = None;
    for mask in 1u32..(1u32 << n) {
        let total: i64 = (0..n).filter(|b| mask >> b & 1 == 1).map(|b| planets[b].1).sum();
        if (total as f64) < need {
            continue;
        }
        let count = mask.count_ones() as usize;
        if best.is_none_or(|(t, c, _)| (total, count) < (t, c)) {
            best = Some((total, count, mask));
        }
    }
    let chosen = match best {
        Some((_, _, mask)) => (0..n)
            .filter(|b| mask >> b & 1 == 1)
            .map(|b| planets[b])
            .max_by_key(|(i, v)| (*v, std::cmp::Reverse(*i))),
        None => planets.iter().copied().max_by_key(|(i, v)| (*v, std::cmp::Reverse(*i))),
    }?;
    MEMORY.with(|m| {
        if let Some(p) = m.borrow_mut().plan.as_mut() {
            p.cast += chosen.1 as f64;
        }
    });
    Some(chosen.0)
}

// -- riders --------------------------------------------------------------------------------------

/// The benefit of a rider card in value points, by alias, for the seat.
fn rider_benefit(ctx: &Ctx<'_>, alias: &str) -> f64 {
    let me = &ctx.seats[ctx.me];
    match alias {
        "imp_rider" => vp_gain(me.vp, 1),
        "lead_rider" => 3.0 * CT,
        "politic_rider" => 3.0 * AC + 2.0,
        // The engine records a correct Technology Rider but does not perform its research.
        "tech_rider" => 0.0,
        "trade_rider" => 5.0 * TG,
        "war_rider" => {
            if me.units.iter().any(|u| u.ship) { 4.0 } else { 0.0 }
        }
        "const_rider" => {
            let docks: Vec<&str> = me
                .units
                .iter()
                .filter(|u| u.base == "spacedock")
                .filter_map(|u| u.planet.as_deref())
                .collect();
            if me.planets.iter().any(|p| !docks.contains(&p.id.as_str())) { 4.0 } else { 0.0 }
        }
        "diplo_rider" => 1.5,
        _ => 0.0,
    }
}

fn rider_aliases(option: &ChoiceOption) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(card) = option.payload.get("card").and_then(serde_json::Value::as_str) {
        out.push(card.to_owned());
    }
    out
}

fn is_rider_alias(alias: &str) -> bool {
    matches!(
        alias,
        "imp_rider" | "lead_rider" | "politic_rider" | "tech_rider" | "trade_rider" | "war_rider" | "const_rider" | "diplo_rider"
    )
}

/// The riders' outcome probabilities and the value of the seat's own votes on this agenda.
struct RiderView {
    outcomes: Vec<String>,
    /// Probability of each outcome winning without the seat's votes.
    p_without: Vec<f64>,
    /// Cost of giving up the votes.
    vote_cost: f64,
    /// Votes freed for the next agenda (value).
    freed: f64,
}

fn rider_view(seen: &Observed<'_>, me: &PlayerId, agenda: &str) -> Option<RiderView> {
    let outcomes = outcomes_of(seen, agenda);
    if outcomes.len() < 2 {
        return None;
    }
    let refs: Vec<&str> = outcomes.iter().map(String::as_str).collect();
    let verdict = evaluate(agenda, &refs, seen, me)?;
    // Everybody else votes in some order: all rivals are "after" for the forecast.
    let rivals: Vec<PlayerId> = seen.players().iter().filter(|p| **p != me).map(|p| (*p).clone()).collect();
    let tally = BTreeMap::new();
    let ballot = ballot_for(agenda, &outcomes, &tally, &rivals, seen, me);
    let p0 = ballot.probabilities(None, 0.0);
    // The seat's votes would back its best outcome.
    let influence = influence_ready_of(seen, me);
    let best = verdict.best()?;
    let x = outcomes.iter().position(|o| o == best.0)?;
    let p1 = ballot.probabilities(Some(x), influence);
    let ev1: f64 = (0..outcomes.len()).map(|o| p1[o] * verdict.score_of(&outcomes[o])).sum();
    let ev0: f64 = (0..outcomes.len()).map(|o| p0[o] * verdict.score_of(&outcomes[o])).sum();
    let ctx = Ctx::build(seen, me)?;
    let mut vote_cost = (ev1 - ev0).max(0.0);
    if is_voter_conditional(agenda) {
        // The seat gives up the conditional gain (Mutiny's point): at least what it expects.
        let side = best.0;
        let cond: f64 = (0..outcomes.len())
            .map(|o| p1[o] * conditional(&ctx, agenda, side, &outcomes[o]))
            .sum();
        vote_cost += cond.max(0.0);
    }
    Some(RiderView {
        outcomes,
        p_without: p0,
        vote_cost,
        freed: if tuning::agenda_budget() { 0.2 * influence } else { 0.0 },
    })
}

/// The rider decision for a reaction window of `AGENDA_REVEALED` (and the card and prediction
/// questions that follow). `None`: not a rider decision.
pub fn rider_pick(
    choice: &Choice,
    seat: &ti4_engine::choice::SeatObservation<'_>,
) -> Option<usize> {
    if !tuning::agenda_riders() {
        return None;
    }
    let seen = seat.observed();
    let me = seat.bound_seat();
    let held: Vec<String> = seat
        .held_action_cards()
        .iter()
        .map(ToString::to_string)
        .filter(|a| is_rider_alias(a))
        .collect();
    let context = choice.context.as_ref()?;
    let subtype = context.subtype.as_str();
    let agenda = context.trigger.as_ref().and_then(|t| t.agenda.clone());
    let decline = choice.options.iter().position(ChoiceOption::is_decline);
    match subtype {
        "predict_agenda_outcome" => {
            // The outcome with the best forecast (the seat's own preference on a tie).
            let agenda = agenda.or_else(|| last_agenda_of(seen))?;
            let view = rider_view(seen, me, &agenda)?;
            let mut best: Option<(usize, f64)> = None;
            for (i, o) in choice.options.iter().enumerate() {
                if o.is_decline() {
                    continue;
                }
                let p = view
                    .outcomes
                    .iter()
                    .position(|x| *x == o.id)
                    .map_or(0.0, |k| view.p_without[k]);
                if best.is_none_or(|(_, bp)| p > bp) {
                    best = Some((i, p));
                }
            }
            best.map(|(i, _)| i)
        }
        s if s.starts_with("reaction_when_AGENDA_REVEALED")
            || s.starts_with("reaction_after_AGENDA_REVEALED") =>
        {
            let agenda = agenda?;
            let play = rider_decision(seen, me, &agenda, context.round, &held);
            // The slot that holds a rider card, else decline.
            if let Some(alias) = play {
                let slot = choice.options.iter().position(|o| {
                    !o.is_decline()
                        && (rider_aliases(o).iter().any(|a| a == &alias)
                            || o
                                .payload
                                .get("card_names")
                                .and_then(serde_json::Value::as_array)
                                .is_some_and(|names| names.iter().any(|n| n.as_str().is_some_and(|n| n.ends_with("Rider")))))
                });
                if let Some(index) = slot {
                    // One card in the slot: it is played now. Several: the card question follows
                    // and marks the play.
                    if !rider_aliases(&choice.options[index]).is_empty() {
                        mark_rider(me, context.round, &agenda);
                    }
                    return slot;
                }
            }
            decline
        }
        s if s.starts_with("play_reaction_when_AGENDA_REVEALED")
            || s.starts_with("play_reaction_after_AGENDA_REVEALED") =>
        {
            let agenda = agenda?;
            let play = rider_decision(seen, me, &agenda, context.round, &held);
            let card = play?;
            let picked = choice
                .options
                .iter()
                .position(|o| rider_aliases(o).iter().any(|a| *a == card));
            if picked.is_some() {
                mark_rider(me, context.round, &agenda);
                return picked;
            }
            decline
        }
        _ => None,
    }
}

fn last_agenda_of(_seen: &Observed<'_>) -> Option<String> {
    MEMORY.with(|m| m.borrow().rider.as_ref().map(|(_, _, a)| a.clone()))
}

fn mark_rider(me: &PlayerId, round: u32, agenda: &str) {
    MEMORY.with(|m| {
        m.borrow_mut().rider = Some((me.to_string(), round, agenda.to_owned()));
    });
}

/// Which rider card (alias) to play on this agenda, if any: the best expected net value.
fn rider_decision(
    seen: &Observed<'_>,
    me: &PlayerId,
    agenda: &str,
    round: u32,
    held: &[String],
) -> Option<String> {
    // One prediction per agenda: do not play a second rider on the agenda already predicted.
    let already = MEMORY.with(|m| {
        m.borrow()
            .rider
            .as_ref()
            .is_some_and(|(s, r, a)| s == me.as_str() && *r == round && a == agenda)
    });
    if already {
        return None;
    }
    let view = rider_view(seen, me, agenda)?;
    let ctx = Ctx::build(seen, me)?;
    let p = view.p_without.iter().copied().fold(0.0, f64::max);
    if p < tuning::rider_min_p() {
        return None;
    }
    let mut best: Option<(String, f64)> = None;
    for alias in held.iter().cloned() {
        let benefit = rider_benefit(&ctx, &alias);
        let net = p * benefit - view.vote_cost + view.freed;
        if benefit > 0.0 && net > 0.0 && best.as_ref().is_none_or(|(_, b)| net > *b) {
            best = Some((alias, net));
        }
    }
    log_rider(seen, me, agenda, round, p, &view, best.as_ref());
    best.map(|(a, _)| a)
}

fn log_rider(
    seen: &Observed<'_>,
    me: &PlayerId,
    agenda: &str,
    round: u32,
    p: f64,
    view: &RiderView,
    chosen: Option<&(String, f64)>,
) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        use std::io::Write;
        let Some(path) = std::env::var_os("AGENDA_VOTE_LOG") else {
            return;
        };
        let faction = seen.seat(me).map(|s| s.faction.to_string()).unwrap_or_default();
        let line = serde_json::json!({
            "k": "rider", "agenda": agenda, "round": round, "seat": me.as_str(), "faction": faction,
            "p": p, "vote_cost": view.vote_cost, "card": chosen.map(|c| c.0.clone()),
            "net": chosen.map(|c| c.1),
        });
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
            let _ = f.write_all(format!("{line}\n").as_bytes());
        }
    }
    #[cfg(target_arch = "wasm32")]
    let _ = (seen, me, agenda, round, p, view, chosen);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ti4_content::ContentStore;
    use ti4_engine::fixtures::{put, put_on_planet, seated_game};
    use ti4_model::POK;
    use ti4_model::id::SystemId;

    fn on() -> tuning::Tuning {
        tuning::Tuning {
            agenda_value: true,
            agenda_budget: true,
            agenda_riders: true,
            ..tuning::Tuning::default()
        }
    }

    fn seen(state: &ti4_model::state::GameState) -> Observed<'_> {
        Observed::new(state, ContentStore::embedded(), POK, None)
    }

    fn table() -> ti4_model::state::GameState {
        seated_game(
            &[("a", "muaat"), ("b", "sol"), ("c", "letnev"), ("d", "hacan")],
            POK,
        )
    }

    fn prefer(state: &ti4_model::state::GameState, who: &str, agenda: &str) -> Option<String> {
        let outcomes = outcomes_of(&seen(state), agenda);
        let refs: Vec<&str> = outcomes.iter().map(String::as_str).collect();
        tuning::with(on(), || {
            preferred(agenda, &refs, &seen(state), &PlayerId::new(who))
        })
    }

    #[test]
    fn muaat_never_wants_the_war_sun_schematics() {
        let state = table();
        assert_eq!(prefer(&state, "a", "schematics").as_deref(), Some("against"));
    }

    #[test]
    fn dreadnought_heavy_seat_votes_against_arms_reduction() {
        let mut state = table();
        let home = SystemId::new("18");
        put(&mut state, &home, "dreadnought", &PlayerId::new("a"), 5);
        put(&mut state, &home, "cruiser", &PlayerId::new("b"), 1);
        assert_eq!(
            prefer(&state, "a", "arms_reduction").as_deref(),
            Some("against")
        );
    }

    #[test]
    fn holder_of_mecatol_does_not_gamble_the_artifact() {
        let mut state = table();
        let mr = SystemId::new("18");
        put(&mut state, &mr, "dreadnought", &PlayerId::new("a"), 3);
        put(&mut state, &mr, "carrier", &PlayerId::new("a"), 2);
        put_on_planet(
            &mut state,
            &mr,
            &PlanetId::new("mr"),
            "infantry",
            &PlayerId::new("a"),
            4,
        );
        assert_eq!(prefer(&state, "a", "artifact").as_deref(), Some("against"));
    }

    #[test]
    fn elects_itself_for_a_card_and_a_rival_for_execution() {
        let state = table();
        assert_eq!(prefer(&state, "b", "minister_policy").as_deref(), Some("b"));
        let picked = prefer(&state, "b", "execution");
        assert_ne!(picked.as_deref(), Some("b"));
    }

    #[test]
    fn small_stakes_are_no_opinion() {
        let state = table();
        assert_eq!(prefer(&state, "a", "covert"), None);
    }

    #[test]
    fn every_agenda_of_the_content_is_valued_and_documented() {
        let state = table();
        let doc = include_str!("../../../docs/BOT_AGENDA_VOTING.md");
        let records =
            ContentStore::embedded().records(ti4_model::content_types::ContentType::Agendas);
        assert!(records.len() >= 63);
        for record in records {
            let alias = record.text("alias").expect("alias");
            assert!(
                doc.contains(&format!("`{alias}`")),
                "{alias} is in the document"
            );
            let outcomes = outcomes_of(&seen(&state), alias);
            if outcomes.is_empty() {
                continue; // abolishment and miscount with no law
            }
            let refs: Vec<&str> = outcomes.iter().map(String::as_str).collect();
            for who in ["a", "b", "c", "d"] {
                let verdict = evaluate(alias, &refs, &seen(&state), &PlayerId::new(who))
                    .unwrap_or_else(|| panic!("{alias} values"));
                assert!(
                    verdict.scores.iter().all(|(_, v)| v.is_finite()),
                    "{alias} finite"
                );
            }
        }
    }

    fn vote(me: &PlayerId, agenda: &str, options: &[(&str, f64)]) -> Choice {
        let mut opts: Vec<ChoiceOption> = options
            .iter()
            .map(|(id, tally)| {
                ChoiceOption::labelled(*id, "vote", *id).with("current_votes", *tally)
            })
            .collect();
        opts.push(ChoiceOption::decline());
        let mut choice = Choice::new(me.clone(), "vote for which outcome", opts)
            .detailed("agenda", serde_json::json!({ "id": agenda }));
        choice.context = Some(ti4_engine::decision_context::DecisionContext::new(
            me.clone(),
            ti4_engine::decision_context::DecisionSource::Rule("8.10".to_owned()),
            "cast_vote",
            ti4_model::state::Phase::Agenda,
            4,
        ));
        choice
    }

    fn stored_target() -> f64 {
        MEMORY
            .with(|m| m.borrow().plan.clone())
            .map_or(0.0, |p| p.target)
    }

    #[test]
    fn budget_casts_nothing_when_the_outcome_is_decided_either_way() {
        reset();
        let state = table();
        let me = PlayerId::new("b");
        let safe = vote(
            &me,
            "minister_policy",
            &[("a", 0.0), ("b", 30.0), ("c", 0.0), ("d", 0.0)],
        );
        let lost = vote(
            &me,
            "minister_policy",
            &[("a", 40.0), ("b", 0.0), ("c", 0.0), ("d", 0.0)],
        );
        tuning::with(on(), || {
            let _ = plan_vote(&safe, &seen(&state), &me, "minister_policy", None);
            assert!(stored_target() <= 2.0, "safe: target {}", stored_target());
            reset();
            let _ = plan_vote(&lost, &seen(&state), &me, "minister_policy", None);
            assert!(stored_target() <= 2.0, "lost: target {}", stored_target());
        });
    }

    #[test]
    fn budget_backs_its_own_name_when_the_vote_is_close() {
        reset();
        let state = table();
        let me = PlayerId::new("b");
        let close = vote(
            &me,
            "minister_policy",
            &[("a", 0.0), ("b", 0.0), ("c", 1.0), ("d", 0.0)],
        );
        tuning::with(on(), || {
            let _ = plan_vote(&close, &seen(&state), &me, "minister_policy", None);
            let plan = MEMORY.with(|m| m.borrow().plan.clone()).expect("plan");
            assert_eq!(plan.outcome, "b");
        });
    }

    #[test]
    fn mutiny_joins_a_for_side_that_passes_with_one_cheap_planet() {
        reset();
        let state = table();
        let me = PlayerId::new("b");
        let choice = vote(&me, "mutiny", &[("for", 12.0), ("against", 0.0)]);
        tuning::with(on(), || {
            let cast = plan_vote(&choice, &seen(&state), &me, "mutiny", Some("for".to_owned()));
            assert!(cast.is_some());
            let plan = MEMORY.with(|m| m.borrow().plan.clone()).expect("plan");
            assert_eq!(plan.outcome, "for");
            assert!(plan.target >= 1.0 && plan.target <= 2.0, "{}", plan.target);
        });
    }

    #[test]
    fn mutiny_stays_out_of_a_for_side_that_fails() {
        reset();
        let state = table();
        let me = PlayerId::new("b");
        let choice = vote(&me, "mutiny", &[("for", 0.0), ("against", 40.0)]);
        tuning::with(on(), || {
            let _ = plan_vote(&choice, &seen(&state), &me, "mutiny", Some("for".to_owned()));
            let plan = MEMORY.with(|m| m.borrow().plan.clone());
            assert!(plan.is_none() || plan.is_some_and(|p| p.outcome != "for"));
        });
    }

    #[test]
    fn the_planets_chosen_waste_least_influence() {
        reset();
        let a = PlayerId::new("a");
        store_plan(&a, "x", 4.0);
        let options: Vec<ChoiceOption> = ["m|3", "n|2", "o|2", "p|4"]
            .iter()
            .map(|id| {
                let votes: i64 = id[2..].parse().unwrap();
                ChoiceOption::labelled(*id, "vote_planet", format!("exhaust {id} for {votes} votes"))
            })
            .chain(std::iter::once(ChoiceOption::decline()))
            .collect();
        let choice = Choice::new(a.clone(), "exhaust a planet to vote x", options);
        let first = pick_planet(&choice, &a, "x").expect("a planet");
        let first_votes = votes_of(&choice.options[first]);
        assert!(first_votes == 4 || first_votes == 2, "{first_votes}");
        let second = pick_planet(&choice, &a, "x").expect("next");
        if first_votes == 4 {
            assert!(choice.options[second].is_decline());
        } else {
            assert_eq!(votes_of(&choice.options[second]), 2);
        }
    }

    #[test]
    fn a_rider_waits_while_the_outcome_is_a_toss_up() {
        reset();
        let state = table();
        tuning::with(on(), || {
            let held = vec!["imp_rider".to_owned()];
            assert!(
                rider_decision(&seen(&state), &PlayerId::new("b"), "prophecy", 4, &held).is_none()
            );
        });
    }

    #[test]
    fn a_rider_is_played_on_a_predictable_agenda_the_seat_does_not_care_about() {
        reset();
        let state = table();
        tuning::with(on(), || {
            let held = vec!["imp_rider".to_owned(), "tech_rider".to_owned()];
            // Incentive Program: every other bot votes For, and the seat has no stake.
            let me = PlayerId::new("b");
            assert_eq!(
                rider_decision(&seen(&state), &me, "incentive", 4, &held).as_deref(),
                Some("imp_rider")
            );
            // The same card is not played twice on one agenda, and the Technology Rider is worth
            // nothing in this engine.
            mark_rider(&me, 4, "incentive");
            assert!(rider_decision(&seen(&state), &me, "incentive", 4, &held).is_none());
            reset();
            assert!(
                rider_decision(&seen(&state), &me, "incentive", 4, &["tech_rider".to_owned()])
                    .is_none()
            );
        });
    }

    #[test]
    fn knobs_off_change_nothing() {
        let state = table();
        let me = PlayerId::new("a");
        let choice = vote(&me, "schematics", &[("for", 0.0), ("against", 0.0)]);
        let off = tuning::for_version(4);
        assert!(!off.agenda_value && !off.agenda_budget && !off.agenda_riders);
        tuning::with(off, || {
            assert!(plan_vote(&choice, &seen(&state), &me, "schematics", None).is_none());
        });
    }

    #[test]
    fn probabilities_sum_to_one_and_follow_votes() {
        let ballot = Ballot {
            outcomes: vec!["for".into(), "against".into()],
            tally: vec![3.0, 0.0],
            forecast: vec![0.0, 4.0],
            var: 4.0,
        };
        let p = ballot.probabilities(None, 0.0);
        assert!((p.iter().sum::<f64>() - 1.0).abs() < 1e-9);
        let with = ballot.probabilities(Some(0), 6.0);
        assert!(with[0] > p[0]);
    }

    #[test]
    fn vp_gain_counts_a_win_more() {
        assert!(vp_gain(9, 1) > 3.0 * vp_gain(5, 1));
        assert!((vp_gain(5, -1) + VP).abs() < 1e-9);
    }
}
