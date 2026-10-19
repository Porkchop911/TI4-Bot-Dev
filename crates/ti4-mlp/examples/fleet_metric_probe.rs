//! Which fleet-strength metric best predicts the lean arena's space-combat result?
//!
//! Samples fleet pairs (fleets from policy games plus synthetic mixes), prices each pair in
//! `ti4-calc`'s arena (the ground truth), and fits `sigma(a * feature + b)` per candidate metric.
//! Measurement only: touches no reward or trainer code.
//!
//! ```text
//! cargo run --release -p ti4-mlp --example fleet_metric_probe -- \
//!   --bundle <checkpoint> --map-pool <pool.json> --seeds 8 --pairs 2400 --fights 400 --out <dir>
//! ```
//! Writes `<dir>/fleet_metric_probe.{pairs.tsv,report.txt}`; refuses to overwrite.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;
use std::sync::Arc;

use rayon::prelude::*;
use ti4_content::ContentStore;
use ti4_engine::choice::{Choice, ChoiceOption, Decider, IllegalChoice, SeatObservation};
use ti4_model::content_types::DEFAULT;
use ti4_model::id::{FactionId, PlayerId, SystemId};
use ti4_policy::fleet_strength::{self as fs, Descriptors};
use ti4_training::battle_arena::{self, Side};

const FACTIONS: [&str; 6] = ["sol", "letnev", "xxcha", "hacan", "jolnar", "l1z1x"];

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

type Units = Vec<(String, usize)>;

#[derive(Clone, Debug)]
struct Fleet {
    faction: String,
    units: Units,
    damaged: Units,
}

impl Fleet {
    fn key(&self) -> String {
        format!("{}|{:?}|{:?}", self.faction, self.units, self.damaged)
    }
    fn ships(&self) -> usize {
        self.units.iter().map(|(_, n)| n).sum()
    }
    fn label(&self) -> String {
        self.units
            .iter()
            .map(|(id, n)| format!("{n}{id}"))
            .collect::<Vec<_>>()
            .join("+")
    }
}

// ---------------------------------------------------------------- policy fleets

struct Watch {
    inner: Box<dyn Decider>,
    log: Rc<RefCell<(BTreeSet<u32>, Vec<Fleet>)>>,
}

impl Decider for Watch {
    fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
        self.inner.choose(choice)
    }
    fn choose_seeing(
        &mut self,
        choice: &Choice,
        seen: &SeatObservation<'_>,
    ) -> Result<ChoiceOption, IllegalChoice> {
        if seen.phase() == ti4_model::state::Phase::Action
            && self.log.borrow_mut().0.insert(seen.round())
        {
            let o = seen.observed();
            let (content, sources) = (o.content(), o.sources());
            if let Some(galaxy) = o.galaxy() {
                let mut found = Vec::new();
                for id in galaxy.system_ids() {
                    let state = o.system(&SystemId::new(id));
                    for p in o.players() {
                        let ships: Vec<_> = state
                            .units_of(p)
                            .into_iter()
                            .filter(|u| {
                                ti4_content::units::unit_type(content, u.type_id.as_str(), sources)
                                    .is_some_and(|k| k.is_ship())
                            })
                            .collect();
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
                        found.push(Fleet {
                            faction: o.seat(p).map(|s| s.faction.to_string()).unwrap_or_default(),
                            units: all.into_iter().collect(),
                            damaged: hurt.into_iter().collect(),
                        });
                    }
                }
                self.log.borrow_mut().1.extend(found);
            }
        }
        self.inner.choose_seeing(choice, seen)
    }
    fn stage_scores(&mut self, scores: Vec<(Option<f64>, Option<f64>)>) {
        self.inner.stage_scores(scores);
    }
}

fn policy_fleets(content: &'static ContentStore, seeds: u64) -> Vec<Fleet> {
    let bundle_path = argument("--bundle").unwrap_or_else(|| refuse("--bundle is required"));
    ti4_tensor::configure_deterministic(20_260_826)
        .unwrap_or_else(|e| refuse(&format!("backend: {e}")));
    let loaded = ti4_mlp::bundle::read(std::path::Path::new(&bundle_path))
        .unwrap_or_else(|e| refuse(&format!("reading {bundle_path}: {e}")));
    let vocabulary = loaded.vocabulary;
    let actor = loaded.actor;
    let pool_path = argument("--map-pool").unwrap_or_else(|| refuse("--map-pool is required"));
    let pool_bytes = ti4_sim::artifacts::read_and_verify_pool_role(
        std::path::Path::new(&pool_path),
        &[ti4_sim::artifacts::ArtifactRole::Validation],
    )
    .unwrap_or_else(|e| refuse(&format!("{pool_path}: {e}")));
    let pool = Arc::new(
        ti4_sim::MapPool::from_reader(std::io::Cursor::new(&pool_bytes))
            .unwrap_or_else(|e| refuse(&format!("pool: {e}"))),
    );
    let players: Vec<PlayerId> = (0..FACTIONS.len())
        .map(|i| PlayerId::new(format!("seat{i}")))
        .collect();
    let seed_base: u64 = 910_000_000;
    let jobs: Vec<(u64, usize)> = (seed_base..seed_base + seeds)
        .flat_map(|s| (0..FACTIONS.len()).map(move |r| (s, r)))
        .collect();
    let workers = rayon::current_num_threads().max(1);
    let chunks: Vec<_> = jobs
        .chunks(jobs.len().div_ceil(workers))
        .map(|c| (actor.inference_copy(), c.to_vec()))
        .collect();
    let harvest: Vec<Vec<Fleet>> = chunks
        .into_par_iter()
        .map(|(local, chunk)| {
            let local = Rc::new(local);
            let mut out = Vec::new();
            for (seed, rotation) in chunk {
                let seated: BTreeMap<PlayerId, FactionId> = players
                    .iter()
                    .enumerate()
                    .map(|(i, p)| {
                        (
                            p.clone(),
                            ti4_training::rollout::seated_faction(
                                &FACTIONS.map(FactionId::new),
                                seed,
                                rotation,
                                i,
                            ),
                        )
                    })
                    .collect();
                let log = Rc::new(RefCell::new((BTreeSet::new(), Vec::new())));
                let (rollout, _) =
                    ti4_training::rollout::play_with_capabilities_and_decider_factory_digest(
                        content,
                        &players,
                        &seated,
                        DEFAULT,
                        seed,
                        ti4_training::rollout::Horizon {
                            rounds: 4,
                            steps: 10_000,
                        },
                        ti4_engine::opening::DEFAULT_REQUIREMENT,
                        &ti4_training::rollout::OpeningMap::PythonPool {
                            pool: Arc::clone(&pool),
                            tile_seed_offset: 0,
                        },
                        ti4_training::rollout::SimulationCapabilities { diplomacy: true },
                        false,
                        |baselines| {
                            let mut deciders: BTreeMap<PlayerId, Box<dyn Decider>> =
                                BTreeMap::new();
                            for (index, player) in players.iter().enumerate() {
                                let row = ti4_mlp::FactionRow::of(seated[player].as_str())
                                    .map_err(|e| format!("{player}: {e}"))?;
                                let baseline = baselines
                                    .get(player)
                                    .copied()
                                    .ok_or_else(|| format!("{player} has no baseline"))?;
                                let stream = seed
                                    .wrapping_mul(1_000_003)
                                    .wrapping_add(u64::try_from(index).unwrap_or(0));
                                let (decider, _) = ti4_mlp::bot::MlpBot::sharing(
                                    &local,
                                    vocabulary.clone(),
                                    row,
                                    stream,
                                )
                                .at_temperature(0.001)
                                .from_setup(baseline)
                                .seat();
                                deciders.insert(
                                    player.clone(),
                                    Box::new(Watch {
                                        inner: decider,
                                        log: Rc::clone(&log),
                                    }),
                                );
                            }
                            Ok(deciders)
                        },
                    );
                if let Some(e) = &rollout.error {
                    eprintln!("game {seed}/{rotation} failed: {e}");
                    continue;
                }
                out.append(&mut log.borrow_mut().1);
            }
            out
        })
        .collect();
    let mut seen = BTreeSet::new();
    harvest
        .into_iter()
        .flatten()
        .filter(|f| seen.insert(f.key()))
        .collect()
}

// ---------------------------------------------------------------- synthetic fleets

fn synthetic(rng: &mut battle_arena::Rng) -> Fleet {
    let faction = match rng.below(100) {
        0..=14 => "jolnar",
        15..=29 => "sardakk",
        _ => "hacan",
    };
    let mode = rng.below(3); // 0 mixed, 1 warship, 2 fighter
    let upg = |rng: &mut battle_arena::Rng, base: &str| {
        if base != "warsun" && rng.below(100) < 40 {
            format!("{base}2")
        } else {
            base.to_owned()
        }
    };
    // (base, weight, per-type cap)
    let weights: [(&str, usize, usize); 5] = match mode {
        1 => [("destroyer", 3, 5), ("cruiser", 4, 5), ("carrier", 1, 4), ("dreadnought", 5, 4), ("warsun", 2, 2)],
        2 => [("destroyer", 2, 5), ("cruiser", 2, 5), ("carrier", 6, 4), ("dreadnought", 1, 4), ("warsun", 0, 2)],
        _ => [("destroyer", 3, 5), ("cruiser", 3, 5), ("carrier", 3, 4), ("dreadnought", 3, 4), ("warsun", 1, 2)],
    };
    let sum: usize = weights.iter().map(|w| w.1).sum();
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut chosen: BTreeMap<&str, String> = BTreeMap::new();
    for _ in 0..1 + rng.below(10) {
        let mut r = rng.below(sum);
        for (base, w, cap) in weights {
            if r < w {
                let id = chosen.entry(base).or_insert_with(|| upg(rng, base)).clone();
                let n = counts.entry(id).or_insert(0);
                if *n < cap {
                    *n += 1;
                }
                break;
            }
            r -= w;
        }
    }
    let fighters = match mode {
        2 => 4 + rng.below(9),
        1 => rng.below(3),
        _ => rng.below(7),
    };
    if fighters > 0 {
        let id = upg(rng, "fighter");
        counts.insert(id, fighters);
    }
    Fleet {
        faction: faction.to_owned(),
        units: counts.into_iter().collect(),
        damaged: Vec::new(),
    }
}

// ---------------------------------------------------------------- fleet maths

struct Ctx {
    content: &'static ContentStore,
}

impl Ctx {
    fn unit(&self, id: &str) -> ti4_content::units::UnitType<'static> {
        ti4_content::units::unit_type(self.content, id, ti4_model::POK).expect("unit")
    }
    /// Basic fighters (Fighter II is exempt, as the engine charges it to the fleet pool) are
    /// dropped beyond the fleet's capacity: the legal fleet.
    fn capped(&self, f: &Fleet) -> Fleet {
        let capacity: i64 = f
            .units
            .iter()
            .map(|(id, n)| self.unit(id).capacity() * i64::try_from(*n).unwrap_or(0))
            .sum();
        let mut out = f.clone();
        for (id, n) in &mut out.units {
            let u = self.unit(id);
            if u.is_fighter() && u.required_technology().is_none() {
                *n = (*n).min(usize::try_from(capacity.max(0)).unwrap_or(0));
            }
        }
        out.units.retain(|(_, n)| *n > 0);
        out
    }
    fn desc(&self, f: &Fleet) -> Descriptors {
        fs::descriptors(self.content, ti4_model::POK, &f.faction, &f.units, &f.damaged)
    }
    fn side(&self, f: &Fleet) -> Side {
        Side::of(self.content, &f.units, &f.damaged, &f.faction, true)
    }
    fn value(&self, left: &[usize], names: &[String]) -> f64 {
        names
            .iter()
            .zip(left)
            .map(|(id, n)| self.unit(id).cost() * *n as f64)
            .sum()
    }
}

/// Everything a metric needs about one side: descriptors of the fleet as built, and of the legal
/// fleet (fighters beyond capacity dropped).
struct View {
    du: Descriptors,
    dc: Descriptors,
}

fn view(cx: &Ctx, f: &Fleet) -> View {
    View {
        du: cx.desc(f),
        dc: cx.desc(&cx.capped(f)),
    }
}

/// (hits, hp) after the opponent's opening fire.
fn after(own: &Descriptors, enemy: &Descriptors) -> (f64, f64) {
    let lost = enemy.cannon + enemy.barrage.min(own.fighters);
    let left = (own.durability - lost).max(0.0);
    let scale = if own.durability > 0.0 { left / own.durability } else { 0.0 };
    (own.firepower * scale, left)
}

fn lnr(a: f64, b: f64) -> f64 {
    ((a + 1e-3) / (b + 1e-3)).ln()
}

const METRICS: [&str; 12] = [
    "A hits+HP",
    "B sqrt(hits*HP)",
    "C pair (Lanchester)",
    "D pair, fighters capped",
    "C ref-opp",
    "D ref-opp",
    "E0 hits^a*HP^b raw, capped",
    "E1 after-opening fitted exps, pair",
    "E2 after-opening fitted exps, ref-opp",
    "C ref-opp+ (ref + 2 destroyers)",
    "D ref-opp+ (ref + 2 destroyers)",
    "E2+ fitted exps, ref-opp+",
];

/// Feature vector of `att` against `def` for metric `m`; `r` is the reference opponent.
fn features(m: usize, att: &View, def: &View, rs: [&View; 2]) -> Vec<f64> {
    let r = rs[0];
    let rp = rs[1];
    let s = |o: &Descriptors, e: &Descriptors| {
        let (h, hp) = after(o, e);
        h * hp
    };
    let l = |x: f64| (x + 0.01).ln();
    match m {
        0 => vec![lnr(
            att.du.firepower + att.du.durability,
            def.du.firepower + def.du.durability,
        )],
        1 => vec![0.5 * lnr(
            att.du.firepower * att.du.durability,
            def.du.firepower * def.du.durability,
        )],
        2 => vec![lnr(s(&att.du, &def.du), s(&def.du, &att.du))],
        3 => vec![lnr(s(&att.dc, &def.dc), s(&def.dc, &att.dc))],
        4 => vec![lnr(s(&att.du, &r.du), s(&def.du, &r.du))],
        5 => vec![lnr(s(&att.dc, &r.dc), s(&def.dc, &r.dc))],
        6 => vec![
            l(att.dc.firepower) - l(def.dc.firepower),
            l(att.dc.durability) - l(def.dc.durability),
        ],
        7 => {
            let (ah, ahp) = after(&att.dc, &def.dc);
            let (dh, dhp) = after(&def.dc, &att.dc);
            vec![l(ah) - l(dh), l(ahp) - l(dhp)]
        }
        8 => {
            let (ah, ahp) = after(&att.dc, &r.dc);
            let (dh, dhp) = after(&def.dc, &r.dc);
            vec![l(ah) - l(dh), l(ahp) - l(dhp)]
        }
        9 => vec![lnr(s(&att.du, &rp.du), s(&def.du, &rp.du))],
        10 => vec![lnr(s(&att.dc, &rp.dc), s(&def.dc, &rp.dc))],
        _ => {
            let (ah, ahp) = after(&att.dc, &rp.dc);
            let (dh, dhp) = after(&def.dc, &rp.dc);
            vec![l(ah) - l(dh), l(ahp) - l(dhp)]
        }
    }
}

// ---------------------------------------------------------------- logistic fit

fn sigmoid(z: f64) -> f64 {
    1.0 / (1.0 + (-z).exp())
}

/// Newton fit of sigma(w . x + b) to soft targets `t`; returns weights then b.
fn fit(x: &[Vec<f64>], t: &[f64]) -> Vec<f64> {
    let k = x[0].len() + 1;
    let mut w = vec![0.0; k];
    for _ in 0..60 {
        let mut g = vec![0.0; k];
        let mut h = vec![vec![0.0; k]; k];
        for (xi, ti) in x.iter().zip(t) {
            let mut v = xi.clone();
            v.push(1.0);
            let p = sigmoid(v.iter().zip(&w).map(|(a, b)| a * b).sum());
            for i in 0..k {
                g[i] += (p - ti) * v[i];
                for j in 0..k {
                    h[i][j] += p * (1.0 - p) * v[i] * v[j];
                }
            }
        }
        for (i, row) in h.iter_mut().enumerate() {
            row[i] += 1e-6;
        }
        let step = solve(h, g);
        let mut moved = 0.0;
        for i in 0..k {
            w[i] -= step[i];
            moved += step[i].abs();
        }
        if moved < 1e-9 {
            break;
        }
    }
    w
}

fn solve(mut a: Vec<Vec<f64>>, mut b: Vec<f64>) -> Vec<f64> {
    let n = b.len();
    for i in 0..n {
        let piv = (i..n)
            .max_by(|&p, &q| a[p][i].abs().total_cmp(&a[q][i].abs()))
            .unwrap_or(i);
        a.swap(i, piv);
        b.swap(i, piv);
        for r in i + 1..n {
            let f = a[r][i] / a[i][i];
            for c in i..n {
                a[r][c] -= f * a[i][c];
            }
            b[r] -= f * b[i];
        }
    }
    let mut x = vec![0.0; n];
    for i in (0..n).rev() {
        let s: f64 = (i + 1..n).map(|j| a[i][j] * x[j]).sum();
        x[i] = (b[i] - s) / a[i][i];
    }
    x
}

fn lin(w: &[f64], x: &[f64]) -> f64 {
    x.iter().zip(w).map(|(a, b)| a * b).sum::<f64>() + w[x.len()]
}

fn predict(w: &[f64], x: &[f64]) -> f64 {
    sigmoid(lin(w, x))
}

// ---------------------------------------------------------------- scoring

struct Row {
    att: Fleet,
    def: Fleet,
    win: f64,
    left_att: f64,
    left_def: f64,
    cost_att: f64,
    cost_def: f64,
    flags: [bool; 6], // fighter-heavy, warship-heavy, mixed, faction-mod, over-capacity, policy-sourced
}

fn subset_flags(cx: &Ctx, a: &Fleet, d: &Fleet, policy: bool) -> [bool; 6] {
    let fighter_share = |f: &Fleet| {
        let fighters: usize = f
            .units
            .iter()
            .filter(|(id, _)| cx.unit(id).is_fighter())
            .map(|(_, n)| n)
            .sum();
        fighters as f64 / f.ships().max(1) as f64
    };
    let war_share = |f: &Fleet| {
        let (mut war, mut hulls) = (0usize, 0usize);
        for (id, n) in &f.units {
            let u = cx.unit(id);
            if u.is_fighter() {
                continue;
            }
            hulls += n;
            if matches!(
                u.base_type(),
                "dreadnought" | "warsun" | "cruiser" | "destroyer" | "flagship"
            ) {
                war += n;
            }
        }
        war as f64 / hulls.max(1) as f64
    };
    let fh = fighter_share(a) >= 0.5 || fighter_share(d) >= 0.5;
    let wh = !fh && war_share(a) >= 0.75 && war_share(d) >= 0.75;
    let fm = |f: &Fleet| matches!(f.faction.as_str(), "jolnar" | "sardakk");
    let over = |f: &Fleet| cx.capped(f).ships() < f.ships();
    [fh, wh, !fh && !wh, fm(a) || fm(d), over(a) || over(d), policy]
}

const SUBSETS: [&str; 7] = [
    "all",
    "fighter-heavy",
    "warship-heavy",
    "mixed",
    "faction-mod",
    "over-capacity",
    "policy-sourced",
];

fn in_subset(row: &Row, s: usize) -> bool {
    s == 0 || row.flags[s - 1]
}

struct Score {
    excess: f64,
    brier: f64,
    acc: f64,
    n_acc: usize,
    ece: f64,
    xent: f64,
}

fn entropy(q: f64) -> f64 {
    if q <= 0.0 || q >= 1.0 {
        0.0
    } else {
        -(q * q.ln() + (1.0 - q) * (1.0 - q).ln())
    }
}

fn score(p: &[f64], t: &[f64]) -> Score {
    let n = p.len() as f64;
    let (mut xe, mut kl, mut br, mut ok, mut m) = (0.0, 0.0, 0.0, 0usize, 0usize);
    for (&pi, &ti) in p.iter().zip(t) {
        let pc = pi.clamp(1e-6, 1.0 - 1e-6);
        let ce = -(ti * pc.ln() + (1.0 - ti) * (1.0 - pc).ln());
        xe += ce;
        kl += ce - entropy(ti);
        br += (pi - ti).powi(2);
        if !(0.4..=0.6).contains(&ti) {
            m += 1;
            ok += usize::from((pi > 0.5) == (ti > 0.5));
        }
    }
    let mut ece = 0.0;
    for b in 0..10 {
        let idx: Vec<usize> = (0..p.len())
            .filter(|&i| ((p[i] * 10.0) as usize).min(9) == b)
            .collect();
        if idx.is_empty() {
            continue;
        }
        let mp = idx.iter().map(|&i| p[i]).sum::<f64>() / idx.len() as f64;
        let mt = idx.iter().map(|&i| t[i]).sum::<f64>() / idx.len() as f64;
        ece += idx.len() as f64 / n * (mp - mt).abs();
    }
    Score {
        excess: kl / n,
        brier: br / n,
        acc: ok as f64 / m.max(1) as f64,
        n_acc: m,
        ece,
        xent: xe / n,
    }
}

fn pearson(x: &[f64], y: &[f64]) -> f64 {
    let n = x.len() as f64;
    let (mx, my) = (x.iter().sum::<f64>() / n, y.iter().sum::<f64>() / n);
    let (mut sxy, mut sxx, mut syy) = (0.0, 0.0, 0.0);
    for (a, b) in x.iter().zip(y) {
        sxy += (a - mx) * (b - my);
        sxx += (a - mx).powi(2);
        syy += (b - my).powi(2);
    }
    sxy / (sxx * syy).sqrt().max(1e-12)
}

fn fleet_of(faction: &str, units: &[(&str, usize)]) -> Fleet {
    Fleet {
        faction: faction.into(),
        units: units.iter().map(|(i, n)| ((*i).to_owned(), *n)).collect(),
        damaged: vec![],
    }
}

fn num<T: std::str::FromStr>(name: &str, default: T) -> T {
    argument(name).map_or(default, |v| v.parse().unwrap_or_else(|_| refuse(name)))
}

fn main() {
    let seeds: u64 = num("--seeds", 8);
    let npairs: usize = num("--pairs", 2400);
    let fights: u32 = num("--fights", 400);
    let threads: usize = num("--threads", 16);
    let out = argument("--out").unwrap_or_else(|| refuse("--out <existing dir> is required"));
    let tag = argument("--tag").unwrap_or_default();
    let pairs_path = format!("{out}/fleet_metric_probe{tag}.pairs.tsv");
    let report_path = format!("{out}/fleet_metric_probe{tag}.report.txt");
    for p in [&pairs_path, &report_path] {
        if std::path::Path::new(p).exists() {
            refuse(&format!("{p} exists; not overwriting"));
        }
    }
    rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .build_global()
        .ok();
    let content: &'static ContentStore = ContentStore::embedded();
    let cx = Ctx { content };

    let clock = std::time::Instant::now();
    let pool = policy_fleets(content, seeds);
    eprintln!("policy fleets: {} unique ({:.0}s)", pool.len(), clock.elapsed().as_secs_f64());
    let mut rng = battle_arena::Rng::new(7);
    // Typical enemy: the median-Lanchester-strength policy fleet of at least two ships.
    let mut multi: Vec<&Fleet> = pool.iter().filter(|f| f.ships() >= 2).collect();
    let strength = |f: &Fleet| {
        let d = cx.desc(f);
        d.firepower * d.durability
    };
    multi.sort_by(|a, b| strength(a).total_cmp(&strength(b)));
    let reference: Fleet = multi.get(multi.len() / 2).map_or_else(
        || fleet_of("hacan", &[("cruiser", 2), ("carrier", 1)]),
        |f| (*f).clone(),
    );

    // Half the pairs from policy fleets, half synthetic; 70% of defenders cost-matched within 2x.
    let synth: Vec<Fleet> = (0..npairs).map(|_| synthetic(&mut rng)).collect();
    let cost = |f: &Fleet| cx.desc(f).cost.max(0.5);
    let mut pairs: Vec<(Fleet, Fleet, bool)> = Vec::new();
    for i in 0..npairs {
        let from_policy = i % 2 == 0 && pool.len() > 20;
        let set: &[Fleet] = if from_policy { &pool } else { &synth };
        let a = set[rng.below(set.len())].clone();
        let want = rng.below(10) < 7;
        let mut d = set[rng.below(set.len())].clone();
        for _ in 0..60 {
            let ratio = cost(&d) / cost(&a);
            if !want || (0.5..2.0).contains(&ratio) {
                break;
            }
            d = set[rng.below(set.len())].clone();
        }
        pairs.push((a, d, from_policy));
    }

    let rows: Vec<Row> = pairs
        .into_par_iter()
        .enumerate()
        .map(|(i, (a, d, policy))| {
            let (ca, cd) = (cx.capped(&a), cx.capped(&d));
            let (sa, sd) = (cx.side(&ca), cx.side(&cd));
            let (mut win, mut la, mut ld) = (0.0, 0.0, 0.0);
            for k in 0..fights {
                let o = battle_arena::fight_outcome(
                    &sa,
                    &sd,
                    battle_arena::fnv(&format!("fmp:{i}:{k}")),
                    true,
                );
                win += match o.winner {
                    Some("a") => 1.0,
                    None => 0.5,
                    _ => 0.0,
                };
                la += cx.value(&o.attacker_left, sa.names());
                ld += cx.value(&o.defender_left, sd.names());
            }
            let n = f64::from(fights);
            Row {
                flags: subset_flags(&cx, &a, &d, policy),
                win: win / n,
                left_att: la / n,
                left_def: ld / n,
                cost_att: cx.desc(&ca).cost,
                cost_def: cx.desc(&cd).cost,
                att: a,
                def: d,
            }
        })
        .collect();
    eprintln!("arena done ({:.0}s)", clock.elapsed().as_secs_f64());

    let vr = view(&cx, &reference);
    let mut reference_plus = reference.clone();
    reference_plus.units.push(("destroyer".into(), 2));
    let vrp = view(&cx, &reference_plus);
    let views: Vec<(View, View)> = rows.iter().map(|r| (view(&cx, &r.att), view(&cx, &r.def))).collect();
    let t: Vec<f64> = rows.iter().map(|r| r.win).collect();
    let mut report = String::new();
    let mut say = |s: String| {
        println!("{s}");
        report.push_str(&s);
        report.push('\n');
    };
    say(format!(
        "pairs {}, unique policy fleets {}, arena fights/pair {fights}; reference opponent: {} ({})",
        rows.len(),
        pool.len(),
        reference.label(),
        reference.faction
    ));
    let sub_n: Vec<usize> = (0..7).map(|s| rows.iter().filter(|r| in_subset(r, s)).count()).collect();
    say(format!(
        "subset sizes: {}",
        SUBSETS.iter().zip(&sub_n).map(|(a, b)| format!("{a}={b}")).collect::<Vec<_>>().join(", ")
    ));
    say(format!(
        "pairs outside 40-60% arena band: {}. excessLL = cross-entropy minus the arena probabilities' own entropy (0 = perfect).\n",
        t.iter().filter(|x| !(0.4..=0.6).contains(*x)).count()
    ));

    let feats: Vec<Vec<Vec<f64>>> = (0..METRICS.len())
        .map(|m| views.iter().map(|(a, d)| features(m, a, d, [&vr, &vrp])).collect())
        .collect();
    let mut weights = Vec::new();
    let mut preds: Vec<Vec<f64>> = Vec::new();
    let mut cv: Vec<f64> = Vec::new();
    for m in 0..METRICS.len() {
        let w = fit(&feats[m], &t);
        let p: Vec<f64> = feats[m].iter().map(|x| predict(&w, x)).collect();
        let mut cvp = vec![0.0; p.len()];
        for fold in 0..2 {
            let tr: Vec<usize> = (0..p.len()).filter(|i| i % 2 != fold).collect();
            let wf = fit(
                &tr.iter().map(|&i| feats[m][i].clone()).collect::<Vec<_>>(),
                &tr.iter().map(|&i| t[i]).collect::<Vec<_>>(),
            );
            for i in (0..p.len()).filter(|i| i % 2 == fold) {
                cvp[i] = predict(&wf, &feats[m][i]);
            }
        }
        cv.push(score(&cvp, &t).excess);
        weights.push(w);
        preds.push(p);
    }
    for s in 0..7 {
        say(format!("== subset: {} (n={}) ==", SUBSETS[s], sub_n[s]));
        say(format!("{:<40} {:>9} {:>8} {:>7} {:>8} {:>13} {:>6}", "metric", "excessLL", "xent", "Brier", "ECE", "acc(out-band)", "n"));
        let idx: Vec<usize> = (0..rows.len()).filter(|&i| in_subset(&rows[i], s)).collect();
        let tt: Vec<f64> = idx.iter().map(|&i| t[i]).collect();
        let mut order: Vec<(usize, Score)> = (0..METRICS.len())
            .map(|m| (m, score(&idx.iter().map(|&i| preds[m][i]).collect::<Vec<_>>(), &tt)))
            .collect();
        order.sort_by(|a, b| a.1.excess.total_cmp(&b.1.excess));
        for (m, sc) in order {
            say(format!(
                "{:<40} {:>9.4} {:>8.4} {:>7.4} {:>8.4} {:>12.1}% {:>6}",
                METRICS[m], sc.excess, sc.xent, sc.brier, sc.ece, sc.acc * 100.0, sc.n_acc
            ));
        }
        say(String::new());
    }
    say("fitted weights on the full sample (feature weights..., then b), and 2-fold CV excessLL:".into());
    for m in 0..METRICS.len() {
        say(format!(
            "{:<40} {:?}  cvExcessLL {:.4}",
            METRICS[m],
            weights[m].iter().map(|w| (w * 1000.0).round() / 1000.0).collect::<Vec<_>>(),
            cv[m]
        ));
    }
    say(String::new());
    say("Pearson r of each fitted logit with the normalised surviving-value margin (att_left-def_left)/(att_cost+def_cost):".into());
    let margin: Vec<f64> = rows.iter().map(|r| (r.left_att - r.left_def) / (r.cost_att + r.cost_def).max(1e-9)).collect();
    for m in 0..METRICS.len() {
        let x: Vec<f64> = feats[m].iter().map(|f| lin(&weights[m], f)).collect();
        say(format!("{:<40} r = {:.3}", METRICS[m], pearson(&x, &margin)));
    }
    say(String::new());
    say("calibration (10 bins of predicted p: [n mean_predicted/mean_arena]):".into());
    for m in 0..METRICS.len() {
        let mut line = format!("{:<40}", METRICS[m]);
        for b in 0..10 {
            let idx: Vec<usize> = (0..rows.len()).filter(|&i| ((preds[m][i] * 10.0) as usize).min(9) == b).collect();
            if idx.is_empty() {
                line.push_str(" -");
                continue;
            }
            let k = idx.len() as f64;
            line.push_str(&format!(
                " [{} {:.2}/{:.2}]",
                idx.len(),
                idx.iter().map(|&i| preds[m][i]).sum::<f64>() / k,
                idx.iter().map(|&i| t[i]).sum::<f64>() / k
            ));
        }
        say(line);
    }

    // Unit table.
    say(String::new());
    say("== unit table: win-probability change (percentage points) vs the base fleet itself (mirror, so ~50% before the add) when units are added to a base fleet ==".into());
    say("columns: arena truth, then fitted metrics A, B, C ref-opp+, D ref-opp, D ref-opp+, E2 ref-opp+ (ref = policy-median fleet, ref+ = ref plus 2 destroyers). 'per slot' = per unit produced (a fighter pair is 2 slots, 1 resource).".into());
    let bases: Vec<(&str, Fleet)> = vec![
        ("policy-median fleet", reference.clone()),
        (
            "2 cruiser+1 dread+1 carrier+2 fighter (spare capacity)",
            fleet_of("hacan", &[("cruiser", 2), ("dreadnought", 1), ("carrier", 1), ("fighter", 2)]),
        ),
        (
            "3 cruiser+1 carrier+4 fighter (capacity full)",
            fleet_of("hacan", &[("cruiser", 3), ("carrier", 1), ("fighter", 4)]),
        ),
    ];
    let adds: [(&str, &str, usize); 11] = [
        ("fighter pair", "fighter", 2),
        ("fighter II pair", "fighter2", 2),
        ("destroyer", "destroyer", 1),
        ("destroyer II", "destroyer2", 1),
        ("cruiser", "cruiser", 1),
        ("cruiser II", "cruiser2", 1),
        ("carrier", "carrier", 1),
        ("carrier II", "carrier2", 1),
        ("dreadnought", "dreadnought", 1),
        ("dreadnought II", "dreadnought2", 1),
        ("war sun", "warsun", 1),
    ];
    let table_metrics = [0usize, 1, 9, 5, 10, 11];
    let truth_p = |a: &Fleet, opp: &Fleet| -> f64 {
        let (ca, cr) = (cx.capped(a), cx.capped(opp));
        let (sa, sr) = (cx.side(&ca), cx.side(&cr));
        let n = 3000u32;
        let w: f64 = (0..n)
            .into_par_iter()
            .map(|k| match battle_arena::fight_outcome(&sa, &sr, battle_arena::fnv(&format!("ut:{k}")), true).winner {
                Some("a") => 1.0,
                None => 0.5,
                _ => 0.0,
            })
            .sum();
        w / f64::from(n)
    };
    let metric_p = |m: usize, a: &Fleet, opp: &Fleet| -> f64 { predict(&weights[m], &features(m, &view(&cx, a), &view(&cx, opp), [&vr, &vrp])) };
    for (name, base) in &bases {
        let p0: Vec<f64> = std::iter::once(truth_p(base, base)).chain(table_metrics.iter().map(|&m| metric_p(m, base, base))).collect();
        say(format!(
            "-- base: {name} [{}] ; P(win vs itself): truth {:.3}, A {:.3}, B {:.3}, C+ {:.3}, D {:.3}, D+ {:.3}, E2+ {:.3}",
            base.label(), p0[0], p0[1], p0[2], p0[3], p0[4], p0[5], p0[6]
        ));
        say(format!("{:<16}{:>5}{:>6} | {:>7} {:>7} {:>7} {:>7} {:>7} {:>7} {:>7}", "add", "res", "slots", "truth", "A", "B", "C-ref+", "D-ref", "D-ref+", "E2-ref+"));
        for (label, id, n) in adds {
            let mut f = base.clone();
            match f.units.iter_mut().find(|(i, _)| i == id) {
                Some((_, k)) => *k += n,
                None => f.units.push((id.to_owned(), n)),
            }
            let res = cx.unit(id).cost() * n as f64;
            let p1: Vec<f64> = std::iter::once(truth_p(&f, base)).chain(table_metrics.iter().map(|&m| metric_p(m, &f, base))).collect();
            let d: Vec<f64> = p1.iter().zip(&p0).map(|(a, b)| 100.0 * (a - b)).collect();
            let row = |scale: f64| d.iter().map(|x| format!("{:>7.2}", x / scale)).collect::<Vec<_>>().join(" ");
            say(format!("{:<16}{:>5.1}{:>6} | {}  per unit group", label, res, n, row(1.0)));
            say(format!("{:<27} | {}  per resource", "", row(res)));
            say(format!("{:<27} | {}  per slot", "", row(n as f64)));
        }
        say(String::new());
    }

    let mut tsv = String::from("att_faction\tatt_fleet\tdef_faction\tdef_fleet\tarena_win\tatt_left_value\tdef_left_value\tatt_cost\tdef_cost\tflags_fh_wh_mixed_fmod_over");
    for m in METRICS {
        tsv.push_str(&format!("\tp[{m}]"));
    }
    tsv.push('\n');
    for (i, r) in rows.iter().enumerate() {
        tsv.push_str(&format!(
            "{}\t{}\t{}\t{}\t{:.4}\t{:.3}\t{:.3}\t{:.1}\t{:.1}\t{}",
            r.att.faction, r.att.label(), r.def.faction, r.def.label(), r.win, r.left_att, r.left_def, r.cost_att, r.cost_def,
            r.flags.iter().map(|b| u8::from(*b).to_string()).collect::<String>()
        ));
        for p in &preds {
            tsv.push_str(&format!("\t{:.4}", p[i]));
        }
        tsv.push('\n');
    }
    std::fs::write(&pairs_path, tsv).unwrap_or_else(|e| refuse(&format!("{pairs_path}: {e}")));
    std::fs::write(&report_path, report).unwrap_or_else(|e| refuse(&format!("{report_path}: {e}")));
    println!("wrote {pairs_path} and {report_path} ({:.0}s)", clock.elapsed().as_secs_f64());
}
