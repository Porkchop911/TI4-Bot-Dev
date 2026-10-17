//! Aggregating the results of many games: the summary a run prints and writes.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use serde::{Deserialize, Serialize};

use crate::play::{GameResult, TypeStat};
use crate::spec::SEATS;

/// min / mean / median / p90 / max of a sample.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Dist {
    pub n: usize,
    pub min: f64,
    pub mean: f64,
    pub median: f64,
    pub p90: f64,
    pub max: f64,
}

#[must_use]
pub fn dist(values: &[f64]) -> Dist {
    if values.is_empty() {
        return Dist::default();
    }
    let mut v = values.to_vec();
    v.sort_by(|a, b| a.total_cmp(b));
    let at = |q: f64| {
        let pos = q * (v.len() - 1) as f64;
        let (lo, hi) = (pos.floor() as usize, pos.ceil() as usize);
        v[lo] + (v[hi] - v[lo]) * (pos - lo as f64)
    };
    Dist {
        n: v.len(),
        min: v[0],
        mean: v.iter().sum::<f64>() / v.len() as f64,
        median: at(0.5),
        p90: at(0.9),
        max: v[v.len() - 1],
    }
}

/// Wins of one group (a seat, a faction) over its games.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct WinStat {
    pub games: usize,
    pub wins: usize,
    pub win_rate: f64,
    /// 95% Wilson interval of the win rate.
    pub ci95: [f64; 2],
    pub mean_vp: f64,
}

fn win_stat(games: usize, wins: usize, vp_sum: f64) -> WinStat {
    let (lo, hi) = wilson(wins, games);
    WinStat {
        games,
        wins,
        win_rate: if games == 0 {
            0.0
        } else {
            wins as f64 / games as f64
        },
        ci95: [lo, hi],
        mean_vp: if games == 0 {
            0.0
        } else {
            vp_sum / games as f64
        },
    }
}

#[must_use]
pub fn wilson(wins: usize, n: usize) -> (f64, f64) {
    if n == 0 {
        return (0.0, 1.0);
    }
    let (n, p, z) = (n as f64, wins as f64 / n as f64, 1.96_f64);
    let denom = 1.0 + z * z / n;
    let centre = (p + z * z / (2.0 * n)) / denom;
    let half = z * (p * (1.0 - p) / n + z * z / (4.0 * n * n)).sqrt() / denom;
    ((centre - half).max(0.0), (centre + half).min(1.0))
}

/// The time to completion of a group of games: finished games only, plus how many did not.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Completion {
    pub games: usize,
    pub finished: usize,
    pub unfinished: usize,
    pub unfinished_seeds: Vec<u64>,
    pub wall_s: Dist,
    pub rounds: Dist,
    pub decisions: Dist,
    /// Microseconds of wall time per decision, pooled over the finished games.
    pub us_per_decision: f64,
    /// Per game: wall microseconds per decision.
    pub us_per_decision_dist: Dist,
}

/// A group of games, aggregated.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Agg {
    pub games: usize,
    pub terminations: BTreeMap<String, usize>,
    /// Games that ended by a cutoff, as a share of all.
    pub cutoff_rate: f64,
    pub rounds: Dist,
    pub decisions: Dist,
    pub wall_s: Dist,
    pub completion: Completion,
    pub win_by_seat: Vec<WinStat>,
    pub win_by_faction: BTreeMap<String, WinStat>,
    /// Victory points of the winner of finished games, histogram.
    pub winner_vp: BTreeMap<String, usize>,
    /// All final points of all seats, histogram.
    pub final_vp: BTreeMap<String, usize>,
    /// Games where the best seat's lead over the second best was zero (tie on points).
    pub tied_at_top: usize,
    /// Seats that finished with 0 points, as a share of seat-games.
    pub zero_vp_seat_rate: f64,
    pub by_map: BTreeMap<String, Completion>,
    pub by_players: BTreeMap<String, Completion>,
    pub phase_ms_mean: BTreeMap<String, f64>,
    pub bot_share: f64,
    pub by_subtype: BTreeMap<String, TypeStat>,
    /// Per game and seat: how often each objective was scored (share of seat-games).
    pub objective_rate: BTreeMap<String, f64>,
    /// Mean points per game from each source.
    pub vp_source_mean: BTreeMap<String, f64>,
    /// Units at the end per seat-game (`GameResult::end_counts`).
    #[serde(default)]
    pub end_count_mean: BTreeMap<String, f64>,
    /// Options taken (`subtype=option`), mean per game.
    pub usage_mean: BTreeMap<String, f64>,
}

fn completion(games: &[&GameResult]) -> Completion {
    let done: Vec<&&GameResult> = games
        .iter()
        .filter(|g| g.termination == "victory")
        .collect();
    let wall: Vec<f64> = done.iter().map(|g| g.wall_ms / 1e3).collect();
    let per: Vec<f64> = done
        .iter()
        .filter(|g| g.decisions > 0)
        .map(|g| g.wall_ms * 1e3 / g.decisions as f64)
        .collect();
    let total_us: f64 = done.iter().map(|g| g.wall_ms * 1e3).sum();
    let total_dec: f64 = done.iter().map(|g| g.decisions as f64).sum();
    Completion {
        games: games.len(),
        finished: done.len(),
        unfinished: games.len() - done.len(),
        unfinished_seeds: games
            .iter()
            .filter(|g| g.termination != "victory")
            .map(|g| g.spec.seed)
            .collect(),
        wall_s: dist(&wall),
        rounds: dist(&done.iter().map(|g| f64::from(g.rounds)).collect::<Vec<_>>()),
        decisions: dist(&done.iter().map(|g| g.decisions as f64).collect::<Vec<_>>()),
        us_per_decision: if total_dec > 0.0 {
            total_us / total_dec
        } else {
            0.0
        },
        us_per_decision_dist: dist(&per),
    }
}

#[must_use]
pub fn aggregate(games: &[&GameResult], detail: bool) -> Agg {
    let n = games.len();
    let mut agg = Agg {
        games: n,
        ..Agg::default()
    };
    if n == 0 {
        return agg;
    }
    for g in games {
        *agg.terminations.entry(g.termination.clone()).or_default() += 1;
    }
    let cut = games
        .iter()
        .filter(|g| g.termination.starts_with("cutoff"))
        .count();
    agg.cutoff_rate = cut as f64 / n as f64;
    agg.rounds = dist(
        &games
            .iter()
            .map(|g| f64::from(g.rounds))
            .collect::<Vec<_>>(),
    );
    agg.decisions = dist(&games.iter().map(|g| g.decisions as f64).collect::<Vec<_>>());
    agg.wall_s = dist(&games.iter().map(|g| g.wall_ms / 1e3).collect::<Vec<_>>());
    agg.completion = completion(games);
    // Wins, only over games that were won.
    let mut seat: BTreeMap<usize, (usize, usize, f64)> = BTreeMap::new();
    let mut faction: BTreeMap<String, (usize, usize, f64)> = BTreeMap::new();
    let mut seat_games = 0usize;
    let mut zero = 0usize;
    for g in games.iter().filter(|g| g.termination == "victory") {
        for (i, vp) in g.vp.iter().enumerate() {
            let e = seat.entry(i).or_default();
            e.0 += 1;
            e.1 += usize::from(g.winner == Some(i));
            e.2 += f64::from(*vp);
            let f = faction.entry(g.spec.factions[i].clone()).or_default();
            f.0 += 1;
            f.1 += usize::from(g.winner == Some(i));
            f.2 += f64::from(*vp);
            *agg.final_vp.entry(format!("{vp:02}")).or_default() += 1;
            seat_games += 1;
            zero += usize::from(*vp == 0);
        }
        if let Some(w) = g.winner {
            *agg.winner_vp.entry(format!("{:02}", g.vp[w])).or_default() += 1;
        }
        let mut sorted = g.vp.clone();
        sorted.sort_unstable_by(|a, b| b.cmp(a));
        if sorted.len() > 1 && sorted[0] == sorted[1] {
            agg.tied_at_top += 1;
        }
    }
    agg.zero_vp_seat_rate = if seat_games == 0 {
        0.0
    } else {
        zero as f64 / seat_games as f64
    };
    agg.win_by_seat = seat.values().map(|&(g, w, v)| win_stat(g, w, v)).collect();
    agg.win_by_faction = faction
        .into_iter()
        .map(|(k, (g, w, v))| (k, win_stat(g, w, v)))
        .collect();
    if detail {
        let mut by_map: BTreeMap<String, Vec<&GameResult>> = BTreeMap::new();
        let mut by_players: BTreeMap<usize, Vec<&GameResult>> = BTreeMap::new();
        for g in games {
            by_map
                .entry(if g.spec.map == "random" {
                    format!("random@{}p", g.spec.players)
                } else {
                    g.spec.map.clone()
                })
                .or_default()
                .push(g);
            by_players.entry(g.spec.players).or_default().push(g);
        }
        agg.by_map = by_map
            .into_iter()
            .map(|(k, v)| (k.to_owned(), completion(&v)))
            .collect();
        agg.by_players = by_players
            .into_iter()
            .map(|(k, v)| (format!("{k}p"), completion(&v)))
            .collect();
        let mut phase: BTreeMap<String, f64> = BTreeMap::new();
        let (mut bot, mut wall) = (0.0, 0.0);
        let mut scored: BTreeMap<String, usize> = BTreeMap::new();
        let mut sources: BTreeMap<String, f64> = BTreeMap::new();
        let mut ends: BTreeMap<String, f64> = BTreeMap::new();
        let mut usage: BTreeMap<String, f64> = BTreeMap::new();
        let mut seatgames = 0usize;
        for g in games {
            for (k, v) in &g.phase_ms {
                *phase.entry(k.clone()).or_default() += v;
            }
            bot += g.bot_ms;
            wall += g.wall_ms;
            for (k, v) in &g.by_subtype {
                let e = agg.by_subtype.entry(k.clone()).or_default();
                e.n += v.n;
                e.smart += v.smart;
                e.ns += v.ns;
            }
            for objectives in &g.scored {
                seatgames += 1;
                for o in objectives {
                    *scored.entry(o.clone()).or_default() += 1;
                }
            }
            for (k, v) in &g.vp_sources {
                *sources.entry(k.clone()).or_default() += f64::from(*v);
            }
            for (k, v) in &g.end_counts {
                *ends.entry(k.clone()).or_default() += f64::from(*v);
            }
            for (k, v) in &g.usage {
                *usage.entry(k.clone()).or_default() += *v as f64;
            }
        }
        agg.phase_ms_mean = phase.into_iter().map(|(k, v)| (k, v / n as f64)).collect();
        agg.bot_share = if wall > 0.0 { bot / wall } else { 0.0 };
        agg.objective_rate = scored
            .into_iter()
            .map(|(k, v)| (k, v as f64 / seatgames.max(1) as f64))
            .collect();
        agg.vp_source_mean = sources
            .into_iter()
            .map(|(k, v)| (k, v / n as f64))
            .collect();
        agg.end_count_mean = ends
            .into_iter()
            .map(|(k, v)| (k, v / seatgames.max(1) as f64))
            .collect();
        agg.usage_mean = usage.into_iter().map(|(k, v)| (k, v / n as f64)).collect();
    }
    agg
}

/// A game that did not end in a victory (or a panic), with what is needed to play it again.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Problem {
    pub termination: String,
    pub detail: Option<String>,
    pub seed: u64,
    pub setup: String,
    pub reproduce: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Meta {
    pub schema: u32,
    pub tool: String,
    pub git_rev: String,
    pub cores: usize,
    pub workers: usize,
    pub profile: String,
    pub wall_s: f64,
    pub games_per_hour: f64,
    pub started: String,
}

/// What `selfplay run` writes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Summary {
    pub meta: Meta,
    pub limits: crate::spec::Limits,
    /// The bot's numbers that differ from the defaults (empty: the shipped bot).
    pub bot: BTreeMap<String, String>,
    pub skipped: Vec<String>,
    pub combined: Agg,
    pub setups: BTreeMap<String, Agg>,
    pub problems: Vec<Problem>,
    /// Compact record of every game, enough to compare runs: seed, setup, outcome, hash.
    pub games: Vec<GameLine>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameLine {
    pub spec: crate::spec::GameSpec,
    pub termination: String,
    pub winner: Option<usize>,
    pub vp: Vec<i32>,
    pub rounds: u32,
    pub decisions: usize,
    pub wall_ms: f64,
    pub hash: String,
}

impl From<&GameResult> for GameLine {
    fn from(g: &GameResult) -> Self {
        Self {
            spec: g.spec.clone(),
            termination: g.termination.clone(),
            winner: g.winner,
            vp: g.vp.clone(),
            rounds: g.rounds,
            decisions: g.decisions,
            wall_ms: g.wall_ms,
            hash: g.hash.clone(),
        }
    }
}

pub fn problems(results: &[GameResult]) -> Vec<Problem> {
    results
        .iter()
        .filter(|g| !matches!(g.termination.as_str(), "victory" | "cutoff_rounds"))
        .map(|g| Problem {
            termination: g.termination.clone(),
            detail: g.detail.clone(),
            seed: g.spec.seed,
            setup: g.spec.setup.clone(),
            reproduce: format!("selfplay replay {}", g.spec.command_line()),
        })
        .collect()
}

fn d(x: f64) -> String {
    if x >= 100.0 {
        format!("{x:.0}")
    } else if x >= 10.0 {
        format!("{x:.1}")
    } else {
        format!("{x:.2}")
    }
}

fn completion_rows(out: &mut String, label: &str, rows: &BTreeMap<String, Completion>) {
    let _ = writeln!(
        out,
        "{label:<22} {:>5} {:>5} {:>8} {:>8} {:>8} {:>8} {:>8} {:>8} {:>8} {:>9}",
        "games",
        "done",
        "wall mean",
        "median",
        "p90",
        "max s",
        "rounds",
        "decis.",
        "us/dec",
        "(rate)"
    );
    for (k, c) in rows {
        let _ = writeln!(
            out,
            "{:<22} {:>5} {:>5} {:>8} {:>8} {:>8} {:>8} {:>8} {:>8} {:>8} {:>9}",
            k,
            c.games,
            c.finished,
            d(c.wall_s.mean),
            d(c.wall_s.median),
            d(c.wall_s.p90),
            d(c.wall_s.max),
            d(c.rounds.mean),
            d(c.decisions.mean),
            d(c.us_per_decision),
            if c.games > 0 {
                format!("{:.0}%", 100.0 * c.finished as f64 / c.games as f64)
            } else {
                String::new()
            },
        );
    }
}

fn agg_text(out: &mut String, agg: &Agg, top: usize) {
    let _ = writeln!(
        out,
        "games {}  terminations {:?}  cutoff rate {:.1}%",
        agg.games,
        agg.terminations,
        100.0 * agg.cutoff_rate
    );
    let _ = writeln!(
        out,
        "rounds mean {} median {} p90 {} max {}   decisions mean {} median {} max {}   wall/game mean {} s",
        d(agg.rounds.mean),
        d(agg.rounds.median),
        d(agg.rounds.p90),
        d(agg.rounds.max),
        d(agg.decisions.mean),
        d(agg.decisions.median),
        d(agg.decisions.max),
        d(agg.wall_s.mean)
    );
    let _ = writeln!(
        out,
        "win rate by seat (expected {:.1}%):",
        100.0 / agg.win_by_seat.len().max(1) as f64
    );
    for (i, w) in agg.win_by_seat.iter().enumerate() {
        let _ = writeln!(
            out,
            "  seat {} {:>5.1}% [{:.1}-{:.1}]  wins {:>4}/{:<4} mean VP {:.2}",
            SEATS[i],
            100.0 * w.win_rate,
            100.0 * w.ci95[0],
            100.0 * w.ci95[1],
            w.wins,
            w.games,
            w.mean_vp
        );
    }
    if !agg.win_by_faction.is_empty() {
        let mut f: Vec<_> = agg.win_by_faction.iter().collect();
        f.sort_by(|a, b| b.1.win_rate.total_cmp(&a.1.win_rate));
        let _ = writeln!(out, "win rate by faction (wins/games, mean VP):");
        for (k, w) in f {
            let _ = writeln!(
                out,
                "  {:<9} {:>5.1}%  {:>3}/{:<3} VP {:.2}",
                k,
                100.0 * w.win_rate,
                w.wins,
                w.games,
                w.mean_vp
            );
        }
    }
    let _ = writeln!(out, "winner VP histogram {:?}", agg.winner_vp);
    let _ = writeln!(
        out,
        "seats ending on 0 VP {:.1}%   ties at the top {}",
        100.0 * agg.zero_vp_seat_rate,
        agg.tied_at_top
    );
    if !agg.phase_ms_mean.is_empty() {
        let _ = writeln!(
            out,
            "mean ms per game by phase {:?}   bot share of wall {:.0}%",
            agg.phase_ms_mean
                .iter()
                .map(|(k, v)| (k.as_str(), v.round()))
                .collect::<Vec<_>>(),
            100.0 * agg.bot_share
        );
    }
    if !agg.by_subtype.is_empty() {
        let mut t: Vec<_> = agg.by_subtype.iter().collect();
        let total_ns: u64 = t.iter().map(|x| x.1.ns).sum();
        t.sort_by_key(|x| std::cmp::Reverse(x.1.ns));
        let _ = writeln!(
            out,
            "decisions by question (top {top} by bot time; smart = rule bot had an opinion):"
        );
        let _ = writeln!(
            out,
            "  {:<42} {:>9} {:>6} {:>9} {:>6}",
            "subtype", "n", "smart", "us/dec", "time%"
        );
        for (k, s) in t.into_iter().take(top) {
            let _ = writeln!(
                out,
                "  {:<42} {:>9} {:>5.0}% {:>9.1} {:>5.1}%",
                k,
                s.n,
                100.0 * s.smart as f64 / s.n as f64,
                s.ns as f64 / 1e3 / s.n as f64,
                100.0 * s.ns as f64 / total_ns.max(1) as f64
            );
        }
    }
    if !agg.objective_rate.is_empty() {
        let mut o: Vec<_> = agg.objective_rate.iter().collect();
        o.sort_by(|a, b| b.1.total_cmp(a.1));
        let _ = writeln!(
            out,
            "objectives scored (share of seat-games), top {top}: {}",
            o.iter()
                .take(top)
                .map(|(k, v)| format!("{k} {:.0}%", 100.0 * **v))
                .collect::<Vec<_>>()
                .join(", ")
        );
        let _ = writeln!(
            out,
            "bottom: {}",
            o.iter()
                .rev()
                .take(top.min(8))
                .map(|(k, v)| format!("{k} {:.1}%", 100.0 * **v))
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    if !agg.end_count_mean.is_empty() {
        let _ = writeln!(
            out,
            "units at the end per seat: {:?}",
            agg.end_count_mean
                .iter()
                .map(|(k, v)| (k.as_str(), (v * 100.0).round() / 100.0))
                .collect::<Vec<_>>()
        );
    }
    if !agg.vp_source_mean.is_empty() {
        let _ = writeln!(
            out,
            "mean VP per game by source: {:?}",
            agg.vp_source_mean
                .iter()
                .map(|(k, v)| (k.as_str(), (v * 100.0).round() / 100.0))
                .collect::<Vec<_>>()
        );
    }
}

/// The readable report.
#[must_use]
pub fn render(summary: &Summary, top: usize) -> String {
    let mut out = String::new();
    let m = &summary.meta;
    let _ = writeln!(
        out,
        "== selfplay: {} games in {:.1} s = {:.0} games/hour ({} workers, {} cores, {}, git {}) ==",
        summary.combined.games,
        m.wall_s,
        m.games_per_hour,
        m.workers,
        m.cores,
        m.profile,
        m.git_rev
    );
    if !summary.bot.is_empty() {
        let _ = writeln!(out, "bot overrides: {:?}", summary.bot);
    }
    for s in &summary.skipped {
        let _ = writeln!(out, "skipped: {s}");
    }
    for (name, agg) in &summary.setups {
        let _ = writeln!(out, "\n-- setup {name} --");
        agg_text(&mut out, agg, top);
        if agg.by_map.len() > 1 {
            let _ = writeln!(out);
            completion_rows(&mut out, "by map", &agg.by_map);
        }
    }
    if summary.setups.len() > 1 {
        let _ = writeln!(out, "\n-- combined --");
        agg_text(&mut out, &summary.combined, top);
    }
    let _ = writeln!(
        out,
        "\n-- time to completion (finished games; us/dec = wall time per decision) --"
    );
    completion_rows(&mut out, "by player count", &summary.combined.by_players);
    let mut maps = summary.combined.by_map.clone();
    maps.retain(|_, _| true);
    if maps.len() > 1 || summary.combined.by_players.len() > 1 {
        let _ = writeln!(out);
        completion_rows(&mut out, "by map", &maps);
    }
    if !summary.combined.completion.unfinished_seeds.is_empty() {
        let seeds = &summary.combined.completion.unfinished_seeds;
        let _ = writeln!(
            out,
            "unfinished seeds ({}): {:?}",
            seeds.len(),
            &seeds[..seeds.len().min(30)]
        );
    }
    if summary.problems.is_empty() {
        let _ = writeln!(out, "\nproblems: none (no stuck, engine error or panic)");
    } else {
        let _ = writeln!(out, "\nproblems ({}):", summary.problems.len());
        for p in summary.problems.iter().take(40) {
            let _ = writeln!(
                out,
                "  {} seed {} [{}]: {}\n      {}",
                p.termination,
                p.seed,
                p.setup,
                p.detail.as_deref().unwrap_or(""),
                p.reproduce
            );
        }
    }
    out
}
