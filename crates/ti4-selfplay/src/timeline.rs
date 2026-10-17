//! Per-round game timelines, recorded while a game is played (replaces replaying games to analyse
//! them: `docs/RULE_BOT_OPTIMISATION.md`, "Stored games").
//!
//! A [`Recorder`] watches the game state after every engine step and writes, per game: map facts
//! per seat, a snapshot per seat at the end of every action phase (planets, resources, influence,
//! technologies, units and where they stand, tokens, strategy cards), victory point events with
//! round and source, activations (what was attacked and how it ended), attacks on home systems,
//! objectives that could have been scored, losses per round. It reads the state only, so a game
//! is the same with or without it. The shape is that of the former `analyze` tool's output.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Value, json};
use ti4_content::ContentStore;
use ti4_content::galaxy::{Galaxy, planet as planet_rec, system as system_rec};
use ti4_host::map::create_game_with_template_seated;
use ti4_host::setup::card_set;
use ti4_model::content_types::POK;
use ti4_model::id::PlayerId;
use ti4_model::state::{GameState, Phase};

use crate::spec::{GameSpec, SEATS};

fn kind_of(t: &str) -> &'static str {
    if t.contains("infantry") {
        "infantry"
    } else if t.contains("mech") {
        "mech"
    } else if t.contains("fighter") {
        "fighter"
    } else if t.contains("pds") {
        "pds"
    } else if t.contains("dock") {
        "dock"
    } else if t.contains("flagship") {
        "flagship"
    } else if t.contains("warsun") || t.contains("war_sun") {
        "warsun"
    } else if t.contains("dread") {
        "dreadnought"
    } else if t.contains("cruiser") {
        "cruiser"
    } else if t.contains("destroyer") {
        "destroyer"
    } else if t.contains("carrier") {
        "carrier"
    } else {
        "other"
    }
}

fn is_ship(k: &str) -> bool {
    matches!(
        k,
        "flagship" | "warsun" | "dreadnought" | "cruiser" | "destroyer" | "carrier" | "fighter"
    )
}
fn is_warship(k: &str) -> bool {
    matches!(
        k,
        "flagship" | "warsun" | "dreadnought" | "cruiser" | "destroyer" | "carrier"
    )
}

/// Unit counts of one seat: (ships by raw type id, ground by raw id, structures), plus where.
#[derive(Default, Clone)]
struct Fleet {
    ships: i64,    // non-fighter ships
    fighters: i64, // fighters
    ground: i64,   // infantry + mech
    home_ships: i64,
    home_ground: i64,
    systems: BTreeSet<String>,
}

fn tally(
    state: &GameState,
    who: &PlayerId,
    home: Option<&String>,
) -> (Fleet, BTreeMap<String, i64>) {
    let mut f = Fleet::default();
    let mut by_type: BTreeMap<String, i64> = BTreeMap::new();
    for (id, sys) in &state.board {
        let is_home = home.is_some_and(|h| h.as_str() == id.as_str());
        for (units, on_planet) in
            std::iter::once((&sys.units, false)).chain(sys.planet_units.values().map(|v| (v, true)))
        {
            let _ = on_planet;
            for u in units {
                if &u.owner != who {
                    continue;
                }
                let t = u.type_id.as_str();
                let k = kind_of(t);
                *by_type.entry(t.to_owned()).or_default() += 1;
                match k {
                    "infantry" | "mech" => {
                        f.ground += 1;
                        if is_home {
                            f.home_ground += 1;
                        }
                    }
                    "fighter" => f.fighters += 1,
                    "pds" | "dock" => {}
                    "other" => {}
                    _ => {
                        f.ships += 1;
                        if is_home {
                            f.home_ships += 1;
                        }
                        f.systems.insert(id.as_str().to_owned());
                    }
                }
            }
        }
    }
    (f, by_type)
}

struct Ctx<'a> {
    content: &'a ContentStore,
    galaxy: &'a Galaxy,
}

fn planet_rf(c: &Ctx, p: &str) -> (i64, i64) {
    planet_rec(c.content, p, POK).map_or((0, 0), |r| (r.resources(), r.influence()))
}

fn sys_info(c: &Ctx, s: &str) -> Value {
    let Some(sys) = system_rec(c.content, s, POK) else {
        return json!({"id": s});
    };
    let mut res = 0;
    let mut inf = 0;
    let planets = sys.planets();
    for p in &planets {
        let (r, i) = planet_rf(c, p);
        res += r;
        inf += i;
    }
    let mut kind = vec![];
    if sys.is_nebula() {
        kind.push("nebula");
    }
    if sys.is_supernova() {
        kind.push("supernova");
    }
    if sys.is_asteroid_field() {
        kind.push("asteroids");
    }
    if sys.is_gravity_rift() {
        kind.push("rift");
    }
    if sys.is_hyperlane() {
        kind.push("hyperlane");
    }
    json!({"id": s, "planets": planets.len(), "res": res, "inf": inf, "kind": kind,
           "wormholes": sys.wormholes().into_iter().collect::<Vec<_>>()})
}

fn map_facts(c: &Ctx, state: &GameState) -> Value {
    let homes: Vec<(String, String)> = state
        .players
        .iter()
        .filter_map(|p| {
            p.home_system
                .as_ref()
                .map(|h| (p.id.as_str().to_owned(), h.as_str().to_owned()))
        })
        .collect();
    let mecatol = "18";
    let mut seats = vec![];
    for (pid, h) in &homes {
        let adj: Vec<String> = c
            .galaxy
            .adjacent(h)
            .into_iter()
            .map(str::to_owned)
            .collect();
        let mut near1 = vec![];
        for a in &adj {
            near1.push(sys_info(c, a));
        }
        // systems within distance 2 (including 1)
        let mut r2 = 0;
        let mut i2 = 0;
        let mut empty2 = 0;
        let mut anom2 = 0;
        let mut n2 = 0;
        for s in c.galaxy.system_ids() {
            if s == h {
                continue;
            }
            if let Some(d) = c.galaxy.distance(h, s) {
                if d <= 2 && d >= 1 {
                    n2 += 1;
                    let info = sys_info(c, s);
                    if homes.iter().any(|(_, hh)| hh == s) {
                        continue;
                    }
                    r2 += info["res"].as_i64().unwrap_or(0);
                    i2 += info["inf"].as_i64().unwrap_or(0);
                    if info["planets"].as_i64().unwrap_or(0) == 0 {
                        empty2 += 1;
                    }
                    if info["kind"].as_array().is_some_and(|k| !k.is_empty()) {
                        anom2 += 1;
                    }
                }
            }
        }
        let others: Vec<i32> = homes
            .iter()
            .filter(|(o, _)| o != pid)
            .filter_map(|(_, oh)| c.galaxy.distance(h, oh))
            .collect();
        seats.push(json!({
            "seat": pid, "home": h,
            "dist_mecatol": c.galaxy.distance(h, mecatol),
            "nearest_home": others.iter().min(), "homes_within_2": others.iter().filter(|d| **d <= 2).count(),
            "homes_within_3": others.iter().filter(|d| **d <= 3).count(),
            "adj": near1, "n_adj": adj.len(),
            "res2": r2, "inf2": i2, "empty2": empty2, "anomaly2": anom2, "n2": n2,
        }));
    }
    json!(seats)
}

fn planets_of(state: &GameState, c: &Ctx, who: &PlayerId) -> (usize, i64, i64, usize, Vec<String>) {
    let (mut n, mut r, mut i, mut nonhome) = (0, 0, 0, 0);
    let mut ids = vec![];
    let home: BTreeSet<String> = state
        .players
        .iter()
        .find(|p| &p.id == who)
        .map(|p| {
            p.home_planets
                .iter()
                .map(|x| x.as_str().to_owned())
                .collect()
        })
        .unwrap_or_default();
    for sys in state.board.values() {
        for (p, owner) in &sys.planet_control {
            if owner == who {
                n += 1;
                let (rr, ii) = planet_rf(c, p.as_str());
                r += rr;
                i += ii;
                if !home.contains(p.as_str()) {
                    nonhome += 1;
                }
                ids.push(p.as_str().to_owned());
            }
        }
    }
    (n, r, i, nonhome, ids)
}

struct Snap<'a> {
    state: &'a GameState,
    c: &'a Ctx<'a>,
}

fn snapshot(sn: &Snap, round: u32, label: &str) -> Value {
    let state = sn.state;
    let mut seats = vec![];
    for p in &state.players {
        let home = p.home_system.as_ref().map(|h| h.as_str().to_owned());
        let (f, by_type) = tally(state, &p.id, home.as_ref());
        let (n, r, i, nonhome, _) = planets_of(state, sn.c, &p.id);
        let scored = state.scored_by(&p.id);
        let structures: i64 = by_type
            .iter()
            .filter(|(t, _)| t.contains("pds") || t.contains("dock"))
            .map(|(_, v)| *v)
            .sum();
        seats.push(json!({
            "seat": p.id.as_str(), "vp": p.victory_points,
            "planets": n, "res": r, "inf": i, "nonhome": nonhome,
            "techs": p.technologies.iter().map(|t| t.as_str().to_owned()).collect::<Vec<_>>(),
            "ships": f.ships, "fighters": f.fighters, "ground": f.ground,
            "home_ships": f.home_ships, "home_ground": f.home_ground,
            "ship_systems": f.systems.len(), "structures": structures,
            "tac": p.tactic_tokens, "fleet": p.fleet_tokens, "strat": p.strategic_tokens,
            "tg": p.trade_goods, "comm": p.commodities, "ac": p.action_cards.len(),
            "secrets_held": p.secret_objectives.len(),
            "secret_ids": p.secret_objectives.iter().map(|s| s.as_str().to_owned()).collect::<Vec<_>>(),
            "home_ctrl": p.home_planets.iter().filter(|hp| state.board.values().any(|sy| sy.planet_control.get(*hp).is_some_and(|o| *o == p.id))).count(),
            "home_total": p.home_planets.len(),
            "scored": scored.iter().map(|o| o.as_str().to_owned()).collect::<Vec<_>>(),
            "cards": p.strategy_cards.iter().map(|c| c.as_str().to_owned()).collect::<Vec<_>>(),
        }));
    }
    json!({"round": round, "at": label, "seats": seats,
           "revealed": state.revealed_objectives.iter().map(|o| o.as_str().to_owned()).collect::<Vec<_>>(),
           "custodians_removed": state.custodians_removed})
}

fn fleet_value(state: &GameState, who: &PlayerId) -> (i64, i64) {
    let (f, _) = tally(state, who, None);
    (f.ships + f.fighters / 2, f.ground)
}

/// Foreign forces in the seat's home system right now.
fn home_threat(state: &GameState, p: &ti4_model::state::Player) -> (i64, i64) {
    let Some(h) = &p.home_system else {
        return (0, 0);
    };
    let Some(sys) = state.board.get(h) else {
        return (0, 0);
    };
    let ships = sys
        .units
        .iter()
        .filter(|u| u.owner != p.id && is_warship(kind_of(u.type_id.as_str())))
        .count() as i64;
    let ground = sys
        .planet_units
        .values()
        .flatten()
        .filter(|u| u.owner != p.id && matches!(kind_of(u.type_id.as_str()), "infantry" | "mech"))
        .count() as i64;
    (ships, ground)
}

/// Watches one game (see the module docs).
pub struct Recorder {
    content: &'static ContentStore,
    galaxy: Galaxy,
    players: usize,
    facts: Value,
    snaps: Vec<Value>,
    vp_events: Vec<Value>,
    acts: Vec<Value>,
    home_hits: Vec<Value>,
    missed: Vec<Value>,
    cards: Vec<Value>,
    losses: Vec<Value>,
    ledger_seen: usize,
    prev_phase: Phase,
    prev_active_sys: Option<String>,
    open_act: Option<Value>,
    open_act_pre: Option<(i64, i64, i64, i64)>,
    prev_fleet: Vec<(i64, i64)>,
    loss_acc: Vec<(i64, i64)>,
    hit_flag: BTreeSet<(u32, usize, &'static str)>,
}

impl Recorder {
    /// A recorder for the game `spec` builds, starting from `state`; `None` when the map cannot be
    /// rebuilt (the game itself would not have built either).
    #[must_use]
    pub fn new(spec: &GameSpec, state: &GameState) -> Option<Self> {
        let content = ContentStore::embedded();
        let ids: Vec<PlayerId> = SEATS[..spec.players]
            .iter()
            .map(|s| PlayerId::new(*s))
            .collect();
        let chosen: Vec<Option<String>> = spec.factions.iter().cloned().map(Some).collect();
        let template = (spec.map != "random").then_some(spec.map.as_str());
        let assignments: BTreeMap<_, _> = ids
            .iter()
            .cloned()
            .zip(crate::spec::lobby_seat_factions(&chosen, spec.seed).ok()?)
            .collect();
        let (_, galaxy) = create_game_with_template_seated(
            content,
            &ids,
            spec.seed,
            template,
            &assignments,
            Some(spec.card_set.as_deref().unwrap_or(card_set::DEFAULT)),
        )
        .ok()?;
        let facts = {
            let c = Ctx {
                content,
                galaxy: &galaxy,
            };
            map_facts(&c, state)
        };
        Some(Self {
            content,
            galaxy,
            players: spec.players,
            facts,
            snaps: vec![],
            vp_events: vec![],
            acts: vec![],
            home_hits: vec![],
            missed: vec![],
            cards: vec![],
            losses: vec![],
            ledger_seen: 0,
            prev_phase: state.phase,
            prev_active_sys: None,
            open_act: None,
            open_act_pre: None,
            prev_fleet: state
                .players
                .iter()
                .map(|p| fleet_value(state, &p.id))
                .collect(),
            loss_acc: vec![(0, 0); spec.players],
            hit_flag: BTreeSet::new(),
        })
    }

    /// Look at the state after an engine step.
    #[allow(
        clippy::too_many_lines,
        reason = "one pass over the state, as the analysis needs"
    )]
    pub fn observe(&mut self, st: &GameState) {
        let content = self.content;
        let c = Ctx {
            content,
            galaxy: &self.galaxy,
        };
        let round = st.round;
        while self.ledger_seen < st.vp_ledger.len() {
            let (pl, d, why) = &st.vp_ledger[self.ledger_seen];
            let head: String = why
                .split([':', '(', '#'])
                .next()
                .unwrap_or(why)
                .trim()
                .chars()
                .take(40)
                .collect();
            self.vp_events.push(json!({"round": round, "seat": pl.as_str(), "d": d, "src": head, "why": why.chars().take(80).collect::<String>(), "phase": format!("{:?}", st.phase)}));
            self.ledger_seen += 1;
        }
        for (i, p) in st.players.iter().enumerate() {
            let now = fleet_value(st, &p.id);
            if now.0 < self.prev_fleet[i].0 {
                self.loss_acc[i].0 += self.prev_fleet[i].0 - now.0;
            }
            if now.1 < self.prev_fleet[i].1 {
                self.loss_acc[i].1 += self.prev_fleet[i].1 - now.1;
            }
            self.prev_fleet[i] = now;
        }
        for (i, p) in st.players.iter().enumerate() {
            let (s, g) = home_threat(st, p);
            if s > 0 && self.hit_flag.insert((round, i, "ships")) {
                self.home_hits
                    .push(json!({"round": round, "seat": p.id.as_str(), "kind": "ships", "n": s}));
            }
            if g > 0 && self.hit_flag.insert((round, i, "ground")) {
                self.home_hits
                    .push(json!({"round": round, "seat": p.id.as_str(), "kind": "ground", "n": g}));
            }
        }
        let cur = st.active_system.as_ref().map(|s| s.as_str().to_owned());
        if cur != self.prev_active_sys {
            if let Some(mut a) = self.open_act.take() {
                let who = a["seat"].as_str().unwrap_or("a").to_owned();
                let pid = PlayerId::new(who.as_str());
                let sysid = a["system"].as_str().unwrap_or("").to_owned();
                let sys = st.board.get(&ti4_model::id::SystemId::new(sysid.as_str()));
                let mine_planets: i64 = sys.map_or(0, |s| {
                    s.planet_control.values().filter(|o| **o == pid).count() as i64
                });
                let foreign_ships = sys.map_or(0, |s| {
                    s.units
                        .iter()
                        .filter(|u| u.owner != pid && is_ship(kind_of(u.type_id.as_str())))
                        .count() as i64
                });
                let foreign_ground = sys.map_or(0, |s| {
                    s.planet_units
                        .values()
                        .flatten()
                        .filter(|u| {
                            u.owner != pid
                                && matches!(kind_of(u.type_id.as_str()), "infantry" | "mech")
                        })
                        .count() as i64
                });
                let my_ships = sys.map_or(0, |s| {
                    s.units
                        .iter()
                        .filter(|u| u.owner == pid && is_ship(kind_of(u.type_id.as_str())))
                        .count() as i64
                });
                let pre = self.open_act_pre.take().unwrap_or((0, 0, 0, 0));
                a["after"] = json!({"planets_mine": mine_planets, "foreign_ships": foreign_ships, "foreign_ground": foreign_ground, "my_ships": my_ships,
                    "gained_planets": mine_planets - pre.0});
                self.acts.push(a);
            }
            if let Some(sysid) = &cur
                && let Some(actor) = st.active.clone()
            {
                let sys = st.board.get(&ti4_model::id::SystemId::new(sysid.as_str()));
                let galaxy_planets =
                    system_rec(content, sysid, POK).map_or(0, |s| s.planets().len());
                let foreign_ships = sys.map_or(0, |s| {
                    s.units
                        .iter()
                        .filter(|u| u.owner != actor && is_ship(kind_of(u.type_id.as_str())))
                        .count() as i64
                });
                let foreign_ground = sys.map_or(0, |s| {
                    s.planet_units
                        .values()
                        .flatten()
                        .filter(|u| {
                            u.owner != actor
                                && matches!(kind_of(u.type_id.as_str()), "infantry" | "mech")
                        })
                        .count() as i64
                });
                let enemy_planets = sys.map_or(0, |s| {
                    s.planet_control.values().filter(|o| **o != actor).count() as i64
                });
                let mine_planets = sys.map_or(0, |s| {
                    s.planet_control.values().filter(|o| **o == actor).count() as i64
                });
                let home_of: Vec<String> = st
                    .players
                    .iter()
                    .filter(|p| p.home_system.as_ref().map(|h| h.as_str()) == Some(sysid.as_str()))
                    .map(|p| p.id.as_str().to_owned())
                    .collect();
                let (res, inf) = system_rec(content, sysid, POK).map_or((0, 0), |s| {
                    s.planets().iter().fold((0, 0), |a, p| {
                        let (r, i) = planet_rf(&c, p);
                        (a.0 + r, a.1 + i)
                    })
                });
                self.open_act = Some(
                    json!({"round": round, "seat": actor.as_str(), "system": sysid, "planets_in_sys": galaxy_planets,
                        "res": res, "inf": inf, "foreign_ships": foreign_ships, "foreign_ground": foreign_ground,
                        "enemy_planets": enemy_planets, "mine_planets": mine_planets, "home_of": home_of,
                        "dist_own_home": st.players.iter().find(|p| p.id == actor).and_then(|p| p.home_system.as_ref()).and_then(|h| self.galaxy.distance(h.as_str(), sysid)),
                    }),
                );
                self.open_act_pre = Some((mine_planets, 0, 0, 0));
            }
            self.prev_active_sys = cur;
        }
        if st.phase != self.prev_phase {
            if self.prev_phase == Phase::Strategy && st.phase == Phase::Action {
                self.cards.push(json!({"round": round, "seats": st.players.iter().map(|p| json!({"seat": p.id.as_str(), "cards": p.strategy_cards.iter().map(|c| c.as_str().to_owned()).collect::<Vec<_>>()})).collect::<Vec<_>>()}));
            }
            if st.phase == Phase::Status && self.prev_phase == Phase::Action {
                self.snaps
                    .push(snapshot(&Snap { state: st, c: &c }, round, "end_action"));
                let eligible: Vec<Vec<String>> = st
                    .players
                    .iter()
                    .map(|p| {
                        ti4_engine::objectives::scoreable_on(
                            st,
                            content,
                            POK,
                            &p.id,
                            Some(&self.galaxy),
                        )
                        .iter()
                        .map(|o| o.as_str().to_owned())
                        .collect()
                    })
                    .collect();
                self.losses.push(json!({"round": round, "seats": self.loss_acc.iter().map(|l| json!({"ships": l.0, "ground": l.1})).collect::<Vec<_>>()}));
                self.loss_acc = vec![(0, 0); self.players];
                self.missed.push(json!({"round": round, "eligible": eligible, "scored_before": st.players.iter().map(|p| st.scored_by(&p.id).len()).collect::<Vec<_>>()}));
            }
            if self.prev_phase == Phase::Status
                && st.phase != Phase::Status
                && let Some(last) = self.missed.last_mut()
            {
                last["scored_after"] = json!(
                    st.players
                        .iter()
                        .map(|p| st.scored_by(&p.id).len())
                        .collect::<Vec<_>>()
                );
            }
            self.prev_phase = st.phase;
        }
    }

    /// The finished record (call once the game has ended or been cut off).
    #[must_use]
    pub fn finish(mut self, st: &GameState) -> Value {
        if let Some(last) = self.missed.last_mut()
            && last.get("scored_after").is_none()
        {
            last["scored_after"] = json!(
                st.players
                    .iter()
                    .map(|p| st.scored_by(&p.id).len())
                    .collect::<Vec<_>>()
            );
        }
        let c = Ctx {
            content: self.content,
            galaxy: &self.galaxy,
        };
        self.snaps
            .push(snapshot(&Snap { state: st, c: &c }, st.round, "final"));
        json!({
            "map": self.facts, "snaps": self.snaps, "vp_events": self.vp_events,
            "acts": self.acts, "home_hits": self.home_hits, "eligible": self.missed,
            "cards": self.cards, "losses": self.losses,
            "agendas": serde_json::to_value(&st.agenda_log).unwrap_or_default(),
            "secrets_unscored": st.players.iter().map(|p| p.secret_objectives.len()).collect::<Vec<_>>(),
        })
    }
}
