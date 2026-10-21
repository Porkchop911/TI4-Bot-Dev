//! Build value as a change in summed power-map projection, tested on real board states.
//!
//! V = sum over systems sqrt(S_space) + lambda * sum over systems sqrt(S_ground), from
//! `ti4_policy::power_map::power_map`; the build value of a unit is V(after placing it at the
//! seat's dock) - V(before). Measurement only: no reward or trainer code is touched.
//!
//! ```text
//! cargo run --release -p ti4-mlp --example build_value_probe -- \
//!   --bundle <checkpoint> --roster wide --diplomacy --seeds 24 --limit 300 \
//!   --production-tsv <eval_production.tsv> --out <dir> --tag _projection_v1
//! ```
//! Writes `<dir>/build_value_probe<tag>.{rows.tsv,report.txt}`; refuses to overwrite.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use rayon::prelude::*;
use ti4_content::ContentStore;
use ti4_engine::choice::{Choice, ChoiceOption, Decider, IllegalChoice, Observed, SeatObservation};
use ti4_model::content_types::{DEFAULT, SourceSet};
use ti4_model::id::{FactionId, PlanetId, PlayerId, SystemId, TechnologyId, UnitTypeId};
use ti4_model::state::GameState;
use ti4_model::units::Unit;
use ti4_policy::fleet_strength::{self as fs, Descriptors};
use ti4_policy::power_map::{self, Composition, SystemPower};
use ti4_training::battle_arena::{self, Side};

fn argument(name: &str) -> Option<String> {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == name {
            return args.next();
        }
    }
    None
}

fn refuse(reason: &str) -> ! {
    eprintln!("\nREFUSED: {reason}");
    std::process::exit(2);
}

fn num<T: std::str::FromStr>(name: &str, default: T) -> T {
    argument(name).map_or(default, |v| v.parse().unwrap_or_else(|_| refuse(name)))
}

const FACTIONS: [&str; 6] = ["sol", "letnev", "xxcha", "hacan", "jolnar", "l1z1x"];
const FIGHTS: u32 = 300;

type Units = Vec<(String, usize)>;

// ------------------------------------------------------------------ values

struct Cx<'a> {
    content: &'a ContentStore,
    sources: SourceSet,
    /// Reference enemy "+": 2 carrier II + 2 cruisers (xxcha) plus 2 destroyers.
    ref_space: Descriptors,
    ref_units: Units,
    /// Reference ground defender: 4 infantry.
    ref_ground: Descriptors,
    lambda: f64,
}

impl<'a> Cx<'a> {
    fn new(content: &'a ContentStore, sources: SourceSet) -> Self {
        let units: Units = vec![
            ("carrier2".into(), 2),
            ("cruiser".into(), 2),
            ("destroyer".into(), 2),
        ];
        let ref_space = fs::descriptors(content, sources, "xxcha", &units, &[]);
        let ground: Units = vec![("infantry".into(), 4)];
        let ref_ground = fs::descriptors(content, sources, "xxcha", &ground, &[]);
        let mut cx = Self {
            content,
            sources,
            ref_space,
            ref_units: units,
            ref_ground,
            lambda: 1.0,
        };
        // lambda: a fighter pair on a canonical fleet counts like an infantry pair on a canonical
        // ground force.
        let sp = |extra: usize| {
            let u: Units = vec![("cruiser".into(), 2), ("carrier".into(), 1), ("fighter".into(), extra)];
            cx.space_s("hacan", &u, &[]).sqrt()
        };
        let gr = |extra: usize| {
            let u: Units = vec![("infantry".into(), 4 + extra)];
            cx.ground_s("hacan", &u, &[]).sqrt()
        };
        let (ds, dg) = (sp(2) - sp(0), gr(2) - gr(0));
        cx.lambda = if dg > 1e-9 { ds / dg } else { 1.0 };
        cx
    }

    fn unit(&self, id: &str) -> Option<ti4_content::units::UnitType<'a>> {
        ti4_content::units::unit_type(self.content, id, self.sources)
    }

    /// Basic fighters beyond the fleet's carrying capacity are dropped (Fighter II is exempt).
    fn capped(&self, units: &[(String, usize)]) -> Units {
        let capacity: i64 = units
            .iter()
            .filter_map(|(id, n)| Some(self.unit(id)?.capacity() * i64::try_from(*n).ok()?))
            .sum();
        let room = usize::try_from(capacity.max(0)).unwrap_or(0);
        units
            .iter()
            .filter_map(|(id, n)| {
                let k = self.unit(id)?;
                let n = if k.is_fighter() && k.required_technology().is_none() { (*n).min(room) } else { *n };
                (n > 0).then(|| (id.clone(), n))
            })
            .collect()
    }

    fn space_s(&self, faction: &str, units: &[(String, usize)], damaged: &[(String, usize)]) -> f64 {
        let d = fs::descriptors(self.content, self.sources, faction, &self.capped(units), damaged);
        fs::strength_after_opening(&d, &self.ref_space)
    }

    fn ground_s(&self, faction: &str, units: &[(String, usize)], damaged: &[(String, usize)]) -> f64 {
        let d = fs::descriptors(self.content, self.sources, faction, units, damaged);
        fs::strength_after_opening(&d, &self.ref_ground)
    }
}

struct Eval {
    /// system -> (sqrt S_space, sqrt S_ground)
    by_sys: BTreeMap<SystemId, (f64, f64)>,
    pm: BTreeMap<SystemId, SystemPower>,
}

impl Eval {
    fn of(cx: &Cx<'_>, state: &GameState, galaxy: &ti4_content::galaxy::Galaxy, player: &PlayerId, faction: &str) -> Self {
        let seen = Observed::new(state, cx.content, cx.sources, Some(galaxy));
        let pm = power_map::power_map(&seen, player);
        let by_sys = pm
            .iter()
            .map(|(id, p)| {
                let s = cx.space_s(faction, &p.space_projection_units.units, &p.space_projection_units.damaged);
                let g = cx.ground_s(faction, &p.ground_projection_units.units, &p.ground_projection_units.damaged);
                (id.clone(), (s.sqrt(), g.sqrt()))
            })
            .collect();
        Self { by_sys, pm }
    }
    /// (space part, lambda * ground part), optionally restricted to `only`.
    fn v(&self, lambda: f64, only: Option<&BTreeSet<SystemId>>) -> (f64, f64) {
        let (mut s, mut g) = (0.0, 0.0);
        for (id, (a, b)) in &self.by_sys {
            if only.is_none_or(|o| o.contains(id)) {
                s += a;
                g += b;
            }
        }
        (s, lambda * g)
    }
    fn total(&self, lambda: f64) -> f64 {
        let (s, g) = self.v(lambda, None);
        s + g
    }
}

// ------------------------------------------------------------------ board edits

fn placed(state: &GameState, dock: &SystemId, planet: Option<&PlanetId>, id: &str, n: usize, owner: &PlayerId, ground: bool) -> Option<GameState> {
    let mut s = state.clone();
    for _ in 0..n {
        let unit = Unit::new(UnitTypeId::new(id), owner.clone());
        if ground {
            s.system_mut(dock).planet_units.entry(planet?.clone()).or_default().push(unit);
        } else {
            s.system_mut(dock).units.push(unit);
        }
    }
    Some(s)
}

fn with_gd(state: &GameState, player: &PlayerId, on: bool) -> GameState {
    let mut s = state.clone();
    if let Some(p) = s.player_mut(player) {
        let gd = TechnologyId::new("gd");
        if on {
            p.technologies.insert(gd);
            p.gravity_drive_used_activation = None;
        } else {
            p.technologies.remove(&gd);
        }
    }
    s
}

/// The seat's production system with the most producing units, and a planet there it controls.
fn find_dock(state: &GameState, cx: &Cx<'_>, player: &PlayerId) -> Option<(SystemId, Option<PlanetId>)> {
    let mut best: Option<(usize, SystemId, Option<PlanetId>)> = None;
    for (id, sys) in &state.board {
        let prod = ti4_engine::production::producers(state, cx.content, cx.sources, player, id);
        if prod.is_empty() {
            continue;
        }
        let planet = prod
            .iter()
            .find_map(|(_, p)| p.clone())
            .or_else(|| sys.planet_control.iter().find(|(_, o)| *o == player).map(|(p, _)| p.clone()));
        if best.as_ref().is_none_or(|b| prod.len() > b.0) {
            best = Some((prod.len(), id.clone(), planet));
        }
    }
    best.map(|(_, s, p)| (s, p))
}

// ------------------------------------------------------------------ records

#[derive(Clone, Debug, Default)]
struct Cell {
    key: String,
    cost: f64,
    slots: f64,
    d_space: f64,
    d_ground: f64,
    d_targets: f64,
    d_target_sys: f64,
    d_truth: Option<f64>,
}

#[derive(Clone, Debug, Default)]
struct StateRow {
    id: String,
    faction: String,
    round: u32,
    supply_full: bool,
    cap_full: bool,
    no_capacity: bool,
    dock_fighters: usize,
    v_space: f64,
    v_ground: f64,
    positive_systems: usize,
    empty_share: f64,
    space_empty_share: f64,
    target_systems: usize,
    truth_mode: &'static str,
    p0: f64,
    cells: Vec<Cell>,
    ctl: BTreeMap<String, f64>,
}

fn comp_has_enemy_or_takeable(state: &GameState, cx: &Cx<'_>, sys: &SystemId, player: &PlayerId) -> bool {
    let s = state.system_state(sys);
    if s.units.iter().any(|u| &u.owner != player) || s.planet_units.values().flatten().any(|u| &u.owner != player) {
        return true;
    }
    ti4_content::galaxy::system(cx.content, sys.as_str(), cx.sources)
        .map(|x| x.planets())
        .unwrap_or_default()
        .into_iter()
        .any(|p| s.planet_control.get(&PlanetId::new(p)) != Some(player))
}

fn enemy_planet_value(state: &GameState, cx: &Cx<'_>, sys: &SystemId, player: &PlayerId) -> f64 {
    let s = state.system_state(sys);
    ti4_content::galaxy::system(cx.content, sys.as_str(), cx.sources)
        .map(|x| x.planets())
        .unwrap_or_default()
        .into_iter()
        .filter(|p| s.planet_control.get(&PlanetId::new(*p)).is_some_and(|o| o != player))
        .filter_map(|p| ti4_content::galaxy::planet(cx.content, p, cx.sources))
        .map(|p| (p.resources() + p.influence()) as f64)
        .sum()
}

fn enemy_fleet(state: &GameState, cx: &Cx<'_>, sys: &SystemId, player: &PlayerId) -> Option<(PlayerId, Units, Units, f64)> {
    let s = state.system_state(sys);
    let mut best: Option<(PlayerId, Units, Units, f64)> = None;
    for owner in s.owners() {
        if owner == player {
            continue;
        }
        let ships: Vec<&Unit> = s.units_of(owner).into_iter().filter(|u| cx.unit(u.type_id.as_str()).is_some_and(|k| k.is_ship())).collect();
        if ships.is_empty() {
            continue;
        }
        let (mut all, mut hurt) = (BTreeMap::new(), BTreeMap::new());
        for u in &ships {
            *all.entry(u.type_id.to_string()).or_insert(0usize) += 1;
            if u.sustained_damage {
                *hurt.entry(u.type_id.to_string()).or_insert(0usize) += 1;
            }
        }
        let (units, damaged): (Units, Units) = (all.into_iter().collect(), hurt.into_iter().collect());
        let cost = fs::descriptors(cx.content, cx.sources, "hacan", &units, &[]).cost;
        if best.as_ref().is_none_or(|b| cost > b.3) {
            best = Some((owner.clone(), units, damaged, cost));
        }
    }
    best
}

fn win_p(cx: &Cx<'_>, att: &Composition, att_faction: &str, def: &Side) -> f64 {
    let units = cx.capped(&att.units);
    if units.is_empty() {
        return 0.0;
    }
    let dmg: Units = att
        .damaged
        .iter()
        .filter_map(|(id, n)| units.iter().find(|(u, _)| u == id).map(|(_, k)| (id.clone(), (*n).min(*k))))
        .collect();
    let sa = Side::of(cx.content, &units, &dmg, att_faction, true);
    let w: f64 = (0..FIGHTS)
        .map(|k| match battle_arena::fight_outcome(&sa, def, battle_arena::fnv(&format!("bv:{k}")), true).winner {
            Some("a") => 1.0,
            None => 0.5,
            _ => 0.0,
        })
        .sum();
    w / f64::from(FIGHTS)
}

// ------------------------------------------------------------------ one state

fn analyse(seen: &SeatObservation<'_>, player: &PlayerId, cx: &Cx<'_>, id: String) -> Option<StateRow> {
    let o = seen.observed();
    let (state, galaxy) = (o.probe_state(), o.galaxy()?);
    let faction = o.seat(player)?.faction.to_string();
    let (dock, planet) = find_dock(state, cx, player)?;
    let mut row = StateRow { id, faction: faction.clone(), round: seen.round(), ..StateRow::default() };

    // --- pre-state flags
    let supply = usize::try_from(o.fleet_supply_limit(player)).unwrap_or(0);
    let is_fighter = |u: &Unit| cx.unit(u.type_id.as_str()).is_some_and(|k| k.is_fighter());
    let is_ship = |u: &Unit| cx.unit(u.type_id.as_str()).is_some_and(|k| k.is_ship());
    let (mut reach, mut full) = (0usize, 0usize);
    for sid in galaxy.system_ids() {
        let sid = SystemId::new(sid);
        let movers = o.movable_into(player, &sid);
        if !movers.iter().any(|m| m.origin == dock && m.origin != sid && !is_fighter(&m.unit)) {
            continue;
        }
        let total = movers.iter().filter(|m| m.origin != sid && !is_fighter(&m.unit)).count();
        let present = o.system(&sid).units_of(player).into_iter().filter(|u| is_ship(u) && !is_fighter(u)).count();
        reach += 1;
        full += usize::from(total >= supply.saturating_sub(present));
    }
    row.supply_full = reach > 0 && full * 2 >= reach;
    let dock_state = o.system(&dock);
    let cap: i64 = dock_state
        .units_of(player)
        .into_iter()
        .filter(|u| !is_fighter(u))
        .filter_map(|u| cx.unit(u.type_id.as_str()).map(|k| k.capacity()))
        .sum();
    let cargo = o.loadable(player, &dock);
    row.dock_fighters = cargo.iter().filter(|c| is_fighter(&c.unit)).count();
    row.no_capacity = cap == 0;
    row.cap_full = cap == 0 || i64::try_from(cargo.len()).unwrap_or(0) >= cap;

    // --- baseline
    let base = Eval::of(cx, state, galaxy, player, &faction);
    let lam = cx.lambda;
    let targets: BTreeSet<SystemId> = galaxy
        .system_ids()
        .into_iter()
        .map(SystemId::new)
        .filter(|s| comp_has_enemy_or_takeable(state, cx, s, player))
        .collect();
    row.target_systems = targets.len();
    let (bs, bg) = base.v(lam, None);
    row.v_space = bs;
    row.v_ground = bg;
    row.positive_systems = base.by_sys.values().filter(|(a, b)| a + lam * b > 1e-9).count();
    let v_all = bs + bg;
    let (ts, tg) = base.v(lam, Some(&targets));
    row.empty_share = if v_all > 0.0 { 1.0 - (ts + tg) / v_all } else { 0.0 };
    row.space_empty_share = if bs > 0.0 { 1.0 - ts / bs } else { 0.0 };
    let (b_t, b_all) = (ts + tg, v_all);

    // --- target of the ground-truth fight
    let mut pick: Option<(f64, f64, SystemId)> = None;
    let mut pick_ref: Option<(f64, SystemId)> = None;
    for (sid, p) in &base.pm {
        if p.space_projection_units.count() == 0 {
            continue;
        }
        let val = enemy_planet_value(state, cx, sid, player);
        if let Some((_, _, _, cost)) = enemy_fleet(state, cx, sid, player) {
            if pick.as_ref().is_none_or(|b| (val, cost) > (b.0, b.1)) {
                pick = Some((val, cost, sid.clone()));
            }
        }
        if val > 0.0 && pick_ref.as_ref().is_none_or(|b| val > b.0) {
            pick_ref = Some((val, sid.clone()));
        }
    }
    let (target, def_side, mode) = match (pick, pick_ref) {
        (Some((_, _, t)), _) => {
            let (owner, units, dmg, _) = enemy_fleet(state, cx, &t, player).expect("fleet");
            let of = o.seat(&owner).map(|s| s.faction.to_string()).unwrap_or_default();
            let u = cx.capped(&units);
            (Some(t), Some(Side::of(cx.content, &u, &dmg.iter().filter(|(i, _)| u.iter().any(|(x, _)| x == i)).cloned().collect::<Vec<_>>(), &of, true)), "real")
        }
        (None, Some((_, t))) => (Some(t), Some(Side::of(cx.content, &cx.ref_units, &[], "xxcha", true)), "ref"),
        _ => (None, None, "none"),
    };
    row.truth_mode = mode;
    let base_truth = match (&target, &def_side) {
        (Some(t), Some(d)) => Some(win_p(cx, &base.pm[t].space_projection_units, &faction, d)),
        _ => None,
    };
    row.p0 = base_truth.unwrap_or(f64::NAN);
    let base_tsys = target.as_ref().map_or(0.0, |t| base.by_sys[t].0);

    // --- the adds
    let buildable = ti4_engine::production::buildable_for(state, cx.content, cx.sources, player);
    let owned = |base_type: &str| buildable.iter().find(|b| cx.unit(b).is_some_and(|k| k.base_type() == base_type)).cloned();
    let mut adds: Vec<(String, String, usize, bool)> = Vec::new(); // (key, id, n, ground)
    for (bt, n, ground) in [("fighter", 2, false), ("infantry", 2, true), ("mech", 1, true), ("destroyer", 1, false), ("cruiser", 1, false), ("carrier", 1, false), ("dreadnought", 1, false), ("warsun", 1, false)] {
        let Some(id) = owned(bt) else { continue };
        adds.push((if id == bt { bt.to_owned() } else { format!("{bt}*") }, id.clone(), n, ground));
        if id != bt && cx.unit(bt).is_some() && bt != "mech" {
            adds.push((bt.to_owned(), bt.to_owned(), n, ground)); // the plain version too
        }
    }
    for (key, uid, n, ground) in &adds {
        let Some(s) = placed(state, &dock, planet.as_ref(), uid, *n, player, *ground) else { continue };
        let e = Eval::of(cx, &s, galaxy, player, &faction);
        let (es, eg) = e.v(lam, None);
        let (et, etg) = e.v(lam, Some(&targets));
        let k = cx.unit(uid).expect("unit");
        let (d_truth, d_tsys) = match (&target, &def_side, base_truth) {
            (Some(t), Some(d), Some(p0)) => (Some(win_p(cx, &e.pm[t].space_projection_units, &faction, d) - p0), e.by_sys[t].0 - base_tsys),
            _ => (None, 0.0),
        };
        row.cells.push(Cell {
            key: key.clone(),
            cost: k.cost() * *n as f64,
            slots: *n as f64,
            d_space: es - bs,
            d_ground: eg - bg,
            d_targets: et + etg - b_t,
            d_target_sys: d_tsys,
            d_truth,
        });
        let _ = b_all;
    }

    // --- controls
    let v = |s: &GameState| Eval::of(cx, s, galaxy, player, &faction);
    let add = |s: &GameState, id: &str, n: usize| placed(s, &dock, planet.as_ref(), id, n, player, false);
    // (a) forced supply-full: `supply` copies of the seat's cruiser at the dock.
    if let (Some(cr), Some(de), Some(dr)) = (owned("cruiser"), owned("destroyer"), owned("dreadnought")) {
        if supply >= 2 {
            if let Some(f) = add(state, &cr, supply) {
                let vf = v(&f).total(lam);
                let vd = v(&add(&f, &de, 1).unwrap_or_else(|| f.clone())).total(lam);
                let vn = v(&add(&f, &dr, 1).unwrap_or_else(|| f.clone())).total(lam);
                let vr = add(state, &cr, supply - 1).and_then(|g| add(&g, &dr, 1)).map_or(vf, |g| v(&g).total(lam));
                row.ctl.insert("full.destroyer".into(), vd - vf);
                row.ctl.insert("full.dread".into(), vn - vf);
                row.ctl.insert("full.dread_replacing_cruiser".into(), vr - vf);
                row.ctl.insert("full.base_V".into(), vf);
                // the same, leaving out the dock system itself (ships present there are never truncated)
                let others: BTreeSet<SystemId> = galaxy.system_ids().into_iter().map(SystemId::new).filter(|s| *s != dock).collect();
                let nd = |s: &GameState| v(s).v(lam, Some(&others));
                let tot = |t: (f64, f64)| t.0 + t.1;
                let b = tot(nd(&f));
                row.ctl.insert("full.destroyer.nodock".into(), tot(nd(&add(&f, &de, 1).unwrap_or_else(|| f.clone()))) - b);
                row.ctl.insert("full.dread.nodock".into(), tot(nd(&add(&f, &dr, 1).unwrap_or_else(|| f.clone()))) - b);
                row.ctl.insert("full.dread_replacing_cruiser.nodock".into(), add(state, &cr, supply - 1).and_then(|g| add(&g, &dr, 1)).map_or(0.0, |g| tot(nd(&g)) - b));
            }
        }
    }
    // (b) a carrier with spare fighters at the dock vs without
    if let (Some(ca), Some(fi)) = (owned("carrier"), owned("fighter")) {
        let b0 = base.total(lam);
        if let Some(c0) = add(state, &ca, 1) {
            let e0 = v(&c0);
            row.ctl.insert("carrier.plain.space".into(), e0.v(lam, None).0 - bs);
            row.ctl.insert("carrier.plain".into(), e0.total(lam) - b0);
        }
        if let Some(sp) = add(state, &fi, 4) {
            let e1 = v(&sp);
            let (s1, g1) = e1.v(lam, None);
            if let Some(c1) = add(&sp, &ca, 1) {
                let e2 = v(&c1);
                row.ctl.insert("carrier.fighters.space".into(), e2.v(lam, None).0 - s1);
                row.ctl.insert("carrier.fighters".into(), e2.total(lam) - (s1 + g1));
            }
        }
    }
    // (d) gravity drive: +1 move for every ship vs +0
    for (tag, on) in [("gd1", true), ("gd0", false)] {
        let g = with_gd(state, player, on);
        let e0 = v(&g);
        row.ctl.insert(format!("{tag}.V"), e0.total(lam));
        row.ctl.insert(format!("{tag}.systems"), e0.by_sys.values().filter(|(a, b)| a + lam * b > 1e-9).count() as f64);
        for (nm, base_type) in [("cruiser", "cruiser"), ("dread", "dreadnought")] {
            if let Some(id) = owned(base_type) {
                if let Some(s) = add(&g, &id, 1) {
                    row.ctl.insert(format!("{tag}.{nm}"), v(&s).total(lam) - e0.total(lam));
                }
            }
        }
    }
    Some(row)
}

// ------------------------------------------------------------------ games

struct Watch {
    inner: Box<dyn Decider>,
    done: BTreeSet<u32>,
    player: PlayerId,
    seed: u64,
    out: Arc<Mutex<Vec<StateRow>>>,
    count: Arc<[AtomicUsize; 3]>,
    limit: usize,
    lambda_cx: Rc<RefCell<Option<Cx<'static>>>>,
}

impl Decider for Watch {
    fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
        self.inner.choose(choice)
    }
    fn choose_seeing(&mut self, choice: &Choice, seen: &SeatObservation<'_>) -> Result<ChoiceOption, IllegalChoice> {
        let round = seen.round();
        if seen.phase() == ti4_model::state::Phase::Action
            && seen.active_system().is_none()
            && (2..=4).contains(&round)
            && self.count[usize::try_from(round - 2).unwrap_or(0)].load(Ordering::Relaxed) < self.limit
            && self.done.insert(round)
        {
            if self.lambda_cx.borrow().is_none() {
                let content: &'static ContentStore = ContentStore::embedded();
                *self.lambda_cx.borrow_mut() = Some(Cx::new(content, seen.observed().sources()));
            }
            let guard = self.lambda_cx.borrow();
            let cx = guard.as_ref().expect("cx");
            let id = format!("{}/{}/r{}", self.seed, self.player, round);
            if let Some(row) = analyse(seen, &self.player, cx, id) {
                self.count[usize::try_from(round - 2).unwrap_or(0)].fetch_add(1, Ordering::Relaxed);
                self.out.lock().expect("lock").push(row);
            }
        }
        self.inner.choose_seeing(choice, seen)
    }
    fn stage_scores(&mut self, scores: Vec<(Option<f64>, Option<f64>)>) {
        self.inner.stage_scores(scores);
    }
}

// ------------------------------------------------------------------ statistics

fn mean(x: &[f64]) -> f64 {
    if x.is_empty() { f64::NAN } else { x.iter().sum::<f64>() / x.len() as f64 }
}

fn ranks(x: &[f64]) -> Vec<f64> {
    let mut idx: Vec<usize> = (0..x.len()).collect();
    idx.sort_by(|&a, &b| x[a].total_cmp(&x[b]));
    let mut r = vec![0.0; x.len()];
    let mut i = 0;
    while i < idx.len() {
        let mut j = i;
        while j + 1 < idx.len() && (x[idx[j + 1]] - x[idx[i]]).abs() < 1e-9 {
            j += 1;
        }
        for &k in &idx[i..=j] {
            r[k] = (i + j) as f64 / 2.0;
        }
        i = j + 1;
    }
    r
}

fn pearson(x: &[f64], y: &[f64]) -> f64 {
    let (mx, my) = (mean(x), mean(y));
    let (mut sxy, mut sxx, mut syy) = (0.0, 0.0, 0.0);
    for (a, b) in x.iter().zip(y) {
        sxy += (a - mx) * (b - my);
        sxx += (a - mx).powi(2);
        syy += (b - my).powi(2);
    }
    sxy / (sxx * syy).sqrt().max(1e-12)
}

fn spearman(x: &[f64], y: &[f64]) -> f64 {
    pearson(&ranks(x), &ranks(y))
}

fn main() {
    let seeds: u64 = num("--seeds", 24);
    let limit: usize = num::<usize>("--limit", 300).div_ceil(3); // per round
    let threads: usize = num("--threads", 28);
    let out = argument("--out").unwrap_or_else(|| refuse("--out <existing dir> is required"));
    let tag = argument("--tag").unwrap_or_else(|| refuse("--tag is required"));
    let rows_path = format!("{out}/build_value_probe{tag}.rows.tsv");
    let report_path = format!("{out}/build_value_probe{tag}.report.txt");
    for p in [&rows_path, &report_path] {
        if std::path::Path::new(p).exists() {
            refuse(&format!("{p} exists; not overwriting"));
        }
    }
    rayon::ThreadPoolBuilder::new().num_threads(threads).build_global().ok();
    let diplomacy = std::env::args().any(|a| a == "--diplomacy");
    let roster = argument("--roster").map_or(ti4_engine::seating::FactionRoster::InScope, |v| {
        ti4_training::rollout::parse_roster(&v).unwrap_or_else(|e| refuse(&e))
    });
    let seed_base: u64 = num("--seed-base", 930_000_000);
    let content: &'static ContentStore = ContentStore::embedded();
    ti4_tensor::configure_deterministic(20_260_826).unwrap_or_else(|e| refuse(&format!("backend: {e}")));
    let bundle_path = argument("--bundle").unwrap_or_else(|| refuse("--bundle is required"));
    let loaded = ti4_mlp::bundle::read(std::path::Path::new(&bundle_path)).unwrap_or_else(|e| refuse(&format!("{bundle_path}: {e}")));
    let (vocabulary, actor) = (loaded.vocabulary, loaded.actor);
    let separate: Option<BTreeMap<String, Arc<ti4_mlp::infer::InferModel>>> =
        ti4_mlp::bundle::separate_model_bundles(std::path::Path::new(&bundle_path))
            .unwrap_or_else(|e| refuse(&e))
            .map(|bundles| {
                let slots = std::fs::read_to_string(std::path::Path::new(&bundle_path).join("slots.json")).unwrap_or_else(|e| refuse(&format!("slots.json: {e}")));
                bundles
                    .into_iter()
                    .map(|(faction, path)| {
                        let model = ti4_mlp::bundle::read(&path).unwrap_or_else(|e| refuse(&format!("{}: {e}", path.display())));
                        let packed = ti4_mlp::infer::InferModel::from_actor(&model.actor, &slots, ti4_infer::Backend::fastest())
                            .unwrap_or_else(|e| refuse(&format!("packing {faction}: {e}")));
                        (faction, Arc::new(packed))
                    })
                    .collect()
            });
    let pool_path = argument("--map-pool").unwrap_or_else(|| "out/pools/full_np8_12_holdout.json".to_owned());
    let pool_bytes = ti4_sim::artifacts::read_and_verify_pool_role(std::path::Path::new(&pool_path), &[ti4_sim::artifacts::ArtifactRole::Validation])
        .unwrap_or_else(|e| refuse(&format!("{pool_path}: {e}")));
    let pool = Arc::new(ti4_sim::MapPool::from_reader(std::io::Cursor::new(&pool_bytes)).unwrap_or_else(|e| refuse(&format!("pool: {e}"))));
    let players: Vec<PlayerId> = (0..FACTIONS.len()).map(|i| PlayerId::new(format!("seat{i}"))).collect();

    let sink: Arc<Mutex<Vec<StateRow>>> = Arc::new(Mutex::new(Vec::new()));
    let count = Arc::new([AtomicUsize::new(0), AtomicUsize::new(0), AtomicUsize::new(0)]);
    let jobs: Vec<u64> = (seed_base..seed_base + seeds).collect();
    let workers = rayon::current_num_threads().max(1);
    let chunks: Vec<_> = jobs.chunks(jobs.len().div_ceil(workers)).map(|c| (actor.inference_copy(), c.to_vec())).collect();
    let clock = std::time::Instant::now();
    let errors: Vec<String> = chunks
        .into_par_iter()
        .flat_map(|(local, chunk)| {
            let local = Rc::new(local);
            let lambda_cx: Rc<RefCell<Option<Cx<'static>>>> = Rc::new(RefCell::new(None));
            let mut errs = Vec::new();
            for seed in chunk {
                let factions = match ti4_training::rollout::game_factions(content, roster, &FACTIONS, &players, DEFAULT, seed) {
                    Ok(f) => f,
                    Err(e) => {
                        errs.push(e);
                        continue;
                    }
                };
                let seated: BTreeMap<PlayerId, FactionId> = players
                    .iter()
                    .enumerate()
                    .map(|(i, p)| (p.clone(), ti4_training::rollout::seated_faction(&factions, seed, 0, i)))
                    .collect();
                let (rollout, _) = ti4_training::rollout::play_with_capabilities_and_decider_factory_digest(
                    content,
                    &players,
                    &seated,
                    DEFAULT,
                    seed,
                    ti4_training::rollout::Horizon { rounds: 4, steps: 10_000 },
                    ti4_engine::opening::DEFAULT_REQUIREMENT,
                    &ti4_training::rollout::OpeningMap::PythonPool { pool: Arc::clone(&pool), tile_seed_offset: 0 },
                    ti4_training::rollout::SimulationCapabilities { diplomacy },
                    false,
                    |baselines| {
                        let mut deciders: BTreeMap<PlayerId, Box<dyn Decider>> = BTreeMap::new();
                        for (index, player) in players.iter().enumerate() {
                            let row = ti4_mlp::FactionRow::of(seated[player].as_str()).map_err(|e| format!("{player}: {e}"))?;
                            let baseline = baselines.get(player).copied().ok_or_else(|| format!("{player} has no baseline"))?;
                            let stream = seed.wrapping_mul(1_000_003).wrapping_add(u64::try_from(index).unwrap_or(0));
                            let bot = match &separate {
                                Some(models) => ti4_mlp::bot::MlpBot::on_infer(
                                    models.get(seated[player].as_str()).ok_or_else(|| format!("no model for {}", seated[player]))?,
                                    vocabulary.clone(),
                                    row,
                                    stream,
                                ),
                                None => ti4_mlp::bot::MlpBot::sharing(&local, vocabulary.clone(), row, stream),
                            };
                            let (decider, _) = bot.at_temperature(0.001).from_setup(baseline).seat();
                            deciders.insert(
                                player.clone(),
                                Box::new(Watch {
                                    inner: decider,
                                    done: BTreeSet::new(),
                                    player: player.clone(),
                                    seed,
                                    out: Arc::clone(&sink),
                                    count: Arc::clone(&count),
                                    limit,
                                    lambda_cx: Rc::clone(&lambda_cx),
                                }),
                            );
                        }
                        Ok(deciders)
                    },
                );
                if let Some(e) = &rollout.error {
                    errs.push(format!("game {seed}: {e}"));
                }
            }
            errs
        })
        .collect();
    for e in &errors {
        eprintln!("{e}");
    }
    let mut rows = std::mem::take(&mut *sink.lock().expect("lock"));
    rows.sort_by(|a, b| a.id.cmp(&b.id));
    eprintln!("{} states ({:.0}s)", rows.len(), clock.elapsed().as_secs_f64());

    let cx = Cx::new(content, ti4_model::POK);
    let mut report = String::new();
    let mut say = |s: String| {
        println!("{s}");
        report.push_str(&s);
        report.push('\n');
    };
    say(format!(
        "states {} (seeds {seeds}, games failed {}), lambda = {:.3} (fighter pair on 2 cruiser+carrier == infantry pair on 4 infantry); reference enemy \"+\" = 2 carrier2 + 2 cruiser + 2 destroyer (xxcha); ground ref = 4 infantry; {FIGHTS} arena fights",
        rows.len(), errors.len(), cx.lambda
    ));
    let by_round: Vec<usize> = (2..=4).map(|r| rows.iter().filter(|x| x.round == r).count()).collect();
    say(format!("rounds 2/3/4: {by_round:?}; factions: {}", rows.iter().map(|r| r.faction.clone()).collect::<BTreeSet<_>>().len()));
    let subsets: [(&str, Box<dyn Fn(&StateRow) -> bool>); 4] = [
        ("all", Box::new(|_| true)),
        ("supply-full", Box::new(|r| r.supply_full)),
        ("capacity-full", Box::new(|r| r.cap_full)),
        ("neither", Box::new(|r| !r.supply_full && !r.cap_full)),
    ];
    let keys: BTreeSet<String> = rows.iter().flat_map(|r| r.cells.iter().map(|c| c.key.clone())).collect();
    say("\n== 1. build value per unit (V = space + lambda*ground). mean dV; per resource; per slot. n = states where the seat can build it ==".into());
    for (name, f) in &subsets {
        let sel: Vec<&StateRow> = rows.iter().filter(|r| f(r)).collect();
        say(format!("-- subset {name} (states {})", sel.len()));
        say(format!("{:<14}{:>5}{:>8}{:>9}{:>9}{:>9}{:>9}{:>9}", "unit", "n", "dV", "space", "ground", "per res", "per slot", "res"));
        for k in &keys {
            let cs: Vec<&Cell> = sel.iter().flat_map(|r| r.cells.iter().filter(|c| &c.key == k)).collect();
            if cs.is_empty() {
                continue;
            }
            let m = |g: &dyn Fn(&Cell) -> f64| mean(&cs.iter().map(|c| g(c)).collect::<Vec<_>>());
            say(format!(
                "{:<14}{:>5}{:>8.3}{:>9.3}{:>9.3}{:>9.3}{:>9.3}{:>9.1}",
                k, cs.len(),
                m(&|c| c.d_space + c.d_ground), m(&|c| c.d_space), m(&|c| c.d_ground),
                m(&|c| (c.d_space + c.d_ground) / c.cost.max(1e-9)), m(&|c| (c.d_space + c.d_ground) / c.slots), m(&|c| c.cost)
            ));
        }
    }

    // --- 2. ground truth
    say("\n== 2. ground truth: dV_space (sum over systems) vs change in simulated win odds of the best reachable fight ==".into());
    for mode in ["real", "ref"] {
        let sel: Vec<&StateRow> = rows.iter().filter(|r| r.truth_mode == mode).collect();
        say(format!(
            "mode {mode}: states {} (target = reachable system with the highest enemy-planet value among those with {}; none: {})",
            sel.len(),
            if mode == "real" { "enemy ships, defender = strongest enemy fleet there" } else { "enemy planets but no enemy ships, defender = ref+" },
            rows.iter().filter(|r| r.truth_mode == "none").count()
        ));
        let (mut rho_sum, mut rho_t, mut used) = (0.0, 0.0, 0usize);
        let (mut pv, mut pt, mut pts) = (vec![], vec![], vec![]);
        let mut dis: BTreeMap<(String, String), (usize, usize)> = BTreeMap::new();
        let mut per_unit: BTreeMap<String, (Vec<f64>, Vec<f64>)> = BTreeMap::new();
        for r in &sel {
            let cs: Vec<&Cell> = r.cells.iter().filter(|c| c.d_truth.is_some()).collect();
            let t: Vec<f64> = cs.iter().map(|c| c.d_truth.unwrap_or(0.0)).collect();
            let v: Vec<f64> = cs.iter().map(|c| c.d_space).collect();
            let vt: Vec<f64> = cs.iter().map(|c| c.d_target_sys).collect();
            for (c, (tt, vv)) in cs.iter().zip(t.iter().zip(&v)) {
                let e = per_unit.entry(c.key.clone()).or_default();
                e.0.push(*vv);
                e.1.push(*tt);
            }
            pv.extend(&v);
            pt.extend(&t);
            pts.extend(&vt);
            if cs.len() >= 3 && t.iter().any(|x| (x - t[0]).abs() > 1e-9) && v.iter().any(|x| (x - v[0]).abs() > 1e-9) {
                rho_sum += spearman(&v, &t);
                rho_t += spearman(&vt, &t);
                used += 1;
            }
            for i in 0..cs.len() {
                for j in 0..i {
                    if (t[i] - t[j]).abs() < 0.03 {
                        continue;
                    }
                    let (a, b) = if cs[i].key < cs[j].key { (i, j) } else { (j, i) };
                    let e = dis.entry((cs[a].key.clone(), cs[b].key.clone())).or_default();
                    e.1 += 1;
                    e.0 += usize::from((v[a] - v[b]).signum() * (t[a] - t[b]).signum() < 0.0 || (v[a] - v[b]).abs() < 1e-6);
                }
            }
        }
        if used == 0 || pv.is_empty() {
            say("  no usable states".into());
            continue;
        }
        say(format!(
            "  per-state Spearman (states with variation in both, n={used}): sum-over-systems dV_space {:.3}; target-system-only d sqrtS {:.3}",
            rho_sum / used as f64, rho_t / used as f64
        ));
        say(format!(
            "  pooled over {} (state,unit) pairs: Spearman sum {:.3} / target-only {:.3}; Pearson sum {:.3}",
            pv.len(), spearman(&pv, &pt), spearman(&pts, &pt), pearson(&pv, &pt)
        ));
        let mu: Vec<(String, f64, f64)> = per_unit.iter().map(|(k, (a, b))| (k.clone(), mean(a), mean(b))).collect();
        say(format!(
            "  Spearman of per-unit MEANS (dV_space vs d win prob): {:.3}",
            spearman(&mu.iter().map(|x| x.1).collect::<Vec<_>>(), &mu.iter().map(|x| x.2).collect::<Vec<_>>())
        ));
        for (k, a, b) in &mu {
            say(format!("    {k:<14} mean dV_space {a:>7.3}   mean d P(win) {:>7.2} pp", b * 100.0));
        }
        let mut d: Vec<_> = dis.into_iter().filter(|(_, (_, n))| *n >= 10).collect();
        d.sort_by(|a, b| (b.1.0 as f64 / b.1.1 as f64).total_cmp(&(a.1.0 as f64 / a.1.1 as f64)));
        say("  most-disagreeing unit pairs (|d truth| >= 3pp; 'disagree' = opposite order or V-tie), rate over pairs:".into());
        for ((a, b), (x, n)) in d.iter().take(8) {
            say(format!("    {a} vs {b}: {x}/{n} = {:.0}%", 100.0 * *x as f64 / *n as f64));
        }
    }

    // --- 3. sanity checks
    say("\n== 3. sanity checks (mean over states) ==".into());
    let c = |k: &str, f: &dyn Fn(&StateRow) -> bool| mean(&rows.iter().filter(|r| f(r)).filter_map(|r| r.ctl.get(k).copied()).collect::<Vec<_>>());
    let nn = |k: &str| rows.iter().filter(|r| r.ctl.contains_key(k)).count();
    say(format!(
        "(a) forced supply-full (supply copies of the seat's cruiser added at the dock; n={}): +destroyer {:.3}; +dreadnought {:.3}; expected for dreadnought replacing a cruiser {:.3}; mean V of the saturated state {:.1}",
        nn("full.destroyer"), c("full.destroyer", &|_| true), c("full.dread", &|_| true), c("full.dread_replacing_cruiser", &|_| true), c("full.base_V", &|_| true)
    ));
    say(format!(
        "    excluding the dock system (ships present in a system are never truncated): +destroyer {:.3}; +dreadnought {:.3}; dreadnought replacing a cruiser {:.3}",
        c("full.destroyer.nodock", &|_| true), c("full.dread.nodock", &|_| true), c("full.dread_replacing_cruiser.nodock", &|_| true)
    ));
    let sf = |r: &StateRow| r.supply_full;
    let cell_mean = |key: &str, f: &dyn Fn(&StateRow) -> bool| {
        mean(&rows.iter().filter(|r| f(r)).flat_map(|r| r.cells.iter().filter(|x| x.key == key).map(|x| x.d_space + x.d_ground)).collect::<Vec<_>>())
    };
    say(format!(
        "    natural supply-full states (n={}): destroyer {:.3} vs not-full {:.3}; dreadnought {:.3} vs not-full {:.3}",
        rows.iter().filter(|r| sf(r)).count(), cell_mean("destroyer", &sf), cell_mean("destroyer", &|r| !sf(r)),
        cell_mean("dreadnought", &sf), cell_mean("dreadnought", &|r| !sf(r))
    ));
    say(format!(
        "(b) carrier dV: plain state {:.3} (space part {:.3}); with 4 spare fighters at the dock {:.3} (space part {:.3}); natural: dock has >=2 uncarried fighters {:.3}, none {:.3}",
        c("carrier.plain", &|_| true), c("carrier.plain.space", &|_| true), c("carrier.fighters", &|_| true), c("carrier.fighters.space", &|_| true),
        cell_mean("carrier", &|r| r.dock_fighters >= 2), cell_mean("carrier", &|r| r.dock_fighters == 0)
    ));
    let fp = |f: &dyn Fn(&StateRow) -> bool| cell_mean("fighter", f);
    say(format!(
        "(c) fighter pair dV: states with zero ship capacity at the dock (n={}) {:.3}; with capacity {:.3}; capacity-full {:.3}",
        rows.iter().filter(|r| r.no_capacity).count(), fp(&|r| r.no_capacity), fp(&|r| !r.no_capacity), fp(&|r| r.cap_full)
    ));
    let g = |k: &str| c(k, &|_| true);
    say(format!(
        "(d) Gravity Drive +1 on every ship vs +0: mean V {:.1} vs {:.1} (x{:.2}); systems with projection {:.1} vs {:.1}; dV cruiser {:.3} vs {:.3} (x{:.2}); dV dreadnought {:.3} vs {:.3} (x{:.2})",
        g("gd1.V"), g("gd0.V"), g("gd1.V") / g("gd0.V"), g("gd1.systems"), g("gd0.systems"),
        g("gd1.cruiser"), g("gd0.cruiser"), g("gd1.cruiser") / g("gd0.cruiser"),
        g("gd1.dread"), g("gd0.dread"), g("gd1.dread") / g("gd0.dread")
    ));
    say(format!(
        "    (power_map applies +1 to all ships at once; the real card moves one ship per activation, so this is an upper bound)\n    share of V from systems with nothing to fight or take: V overall {:.1}%, space part {:.1}%; mean systems with positive projection {:.1} of which targeted {:.1}",
        100.0 * mean(&rows.iter().map(|r| r.empty_share).collect::<Vec<_>>()),
        100.0 * mean(&rows.iter().map(|r| r.space_empty_share).collect::<Vec<_>>()),
        mean(&rows.iter().map(|r| r.positive_systems as f64).collect::<Vec<_>>()),
        mean(&rows.iter().map(|r| r.target_systems as f64).collect::<Vec<_>>())
    ));
    // targets-only variant
    say("    build value restricted to systems with targets (enemy units or a planet not yours), mean dV all-systems vs targets-only, and their correlation over (state,unit):".into());
    for k in &keys {
        let cs: Vec<&Cell> = rows.iter().flat_map(|r| r.cells.iter().filter(|c| &c.key == k)).collect();
        let a: Vec<f64> = cs.iter().map(|c| c.d_space + c.d_ground).collect();
        let b: Vec<f64> = cs.iter().map(|c| c.d_targets).collect();
        say(format!("      {k:<14} all {:.3}  targets {:.3}  ({:.0}% of all)", mean(&a), mean(&b), 100.0 * mean(&b) / mean(&a)));
    }
    let a: Vec<f64> = rows.iter().flat_map(|r| r.cells.iter().map(|c| c.d_space + c.d_ground)).collect();
    let b: Vec<f64> = rows.iter().flat_map(|r| r.cells.iter().map(|c| c.d_targets)).collect();
    say(format!("      Pearson r(all, targets-only) over {} pairs: {:.3}", a.len(), pearson(&a, &b)));

    // --- weight
    if let Some(tsv) = argument("--production-tsv") {
        let text = std::fs::read_to_string(&tsv).unwrap_or_else(|e| refuse(&format!("{tsv}: {e}")));
        let mut lines = text.lines();
        let head: Vec<&str> = lines.next().unwrap_or("").split('\t').collect();
        let data: Vec<Vec<f64>> = lines.map(|l| l.split('\t').map(|x| x.parse().unwrap_or(0.0)).collect()).collect();
        let col = |n: &str| head.iter().position(|h| *h == n).map_or(0.0, |i| mean(&data.iter().map(|r| r[i]).collect::<Vec<_>>()));
        say(format!("\n== weight: units built per seat-game from {tsv} ({} seat-games) x mean dV from section 1 (all states) ==", data.len()));
        let mut total = 0.0;
        for (col_name, key, per) in [("carrier", "carrier", 1.0), ("cruiser", "cruiser", 1.0), ("destroyer", "destroyer", 1.0), ("dreadnought", "dreadnought", 1.0), ("fighter", "fighter", 0.5), ("infantry", "infantry", 0.5), ("mech", "mech", 1.0), ("warsun", "warsun", 1.0)] {
            let built = col(col_name);
            let dv = {
                let pool: Vec<f64> = rows.iter().flat_map(|r| r.cells.iter().filter(|c| c.key.trim_end_matches('*') == key).map(|c| (c.d_space + c.d_ground) / c.slots)).collect();
                mean(&pool)
            };
            let _ = per;
            let part = if dv.is_nan() { 0.0 } else { built * dv };
            say(format!("  {col_name:<12} built {built:>6.2}  mean dV per unit {dv:>7.3}  -> {part:>7.2}"));
            total += part;
        }
        say(format!("  typical-game build reward at weight 1 = {total:.2}; weight for ~1.5 = {:.4} (flagships not measured; round 2-4 states stand in for all builds)", 1.5 / total));
    }

    let mut tsv = String::from("state\tfaction\tround\tsupply_full\tcap_full\tdock_fighters\ttruth_mode\tp0\tunit\tcost\tslots\td_space\td_ground\td_targets\td_target_sys\td_truth\n");
    for r in &rows {
        for c in &r.cells {
            tsv.push_str(&format!(
                "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{:.3}\t{}\t{:.2}\t{}\t{:.4}\t{:.4}\t{:.4}\t{:.4}\t{}\n",
                r.id, r.faction, r.round, u8::from(r.supply_full), u8::from(r.cap_full), r.dock_fighters, r.truth_mode, r.p0,
                c.key, c.cost, c.slots, c.d_space, c.d_ground, c.d_targets, c.d_target_sys, c.d_truth.map_or(String::from("NA"), |x| format!("{x:.4}"))
            ));
        }
    }
    std::fs::write(&rows_path, tsv).unwrap_or_else(|e| refuse(&format!("{rows_path}: {e}")));
    std::fs::write(&report_path, report).unwrap_or_else(|e| refuse(&format!("{report_path}: {e}")));
    println!("wrote {rows_path} and {report_path} ({:.0}s)", clock.elapsed().as_secs_f64());
}
