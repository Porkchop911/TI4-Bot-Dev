//! Running many games on worker threads, and the A/B comparison.

use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use serde::{Deserialize, Serialize};
use ti4_rule_bot::tuning::Tuning;

use crate::play::{GameResult, PlayOptions, TraceLine, play_guarded};
use crate::spec::{GameSpec, Limits};

/// One game to play.
pub struct Job {
    pub spec: GameSpec,
    pub options: PlayOptions,
}

/// Called on the worker thread when a game is done (to write a trace, for instance).
pub type OnDone<'a> = &'a (dyn Fn(&GameResult, Option<&[TraceLine]>) + Sync);

/// [`run_jobs`], reading the stored run when it exists and storing the result otherwise
/// ([`crate::store`]). `on_done` is called for stored games too (without a trace).
pub fn run_jobs_cached(
    jobs: &[Job],
    workers: usize,
    progress: bool,
    on_done: Option<OnDone<'_>>,
) -> Vec<GameResult> {
    if let Some(results) = crate::store::load(jobs) {
        eprintln!("stored run found: {} games read, not played", results.len());
        if let Some(f) = on_done {
            for r in &results {
                f(r, None);
            }
        }
        return results;
    }
    let started = Instant::now();
    let results = run_jobs(jobs, workers, progress, on_done);
    crate::store::save(jobs, &results, started.elapsed().as_secs_f64());
    results
}

/// Play `jobs` on `workers` threads. Results come back in job order, whatever the order of
/// completion, so a run is the same list on any number of workers.
pub fn run_jobs(
    jobs: &[Job],
    workers: usize,
    progress: bool,
    on_done: Option<OnDone<'_>>,
) -> Vec<GameResult> {
    crate::play::install_panic_capture();
    let next = AtomicUsize::new(0);
    let done = AtomicUsize::new(0);
    let slots: Mutex<Vec<Option<GameResult>>> = Mutex::new((0..jobs.len()).map(|_| None).collect());
    let started = Instant::now();
    let workers = workers.clamp(1, jobs.len().max(1));
    std::thread::scope(|scope| {
        for _ in 0..workers {
            std::thread::Builder::new()
                .stack_size(128 << 20)
                .spawn_scoped(scope, || {
                    loop {
                        let i = next.fetch_add(1, Ordering::SeqCst);
                        let Some(job) = jobs.get(i) else { break };
                        let (result, trace) = play_guarded(&job.spec, &job.options);
                        if let Some(f) = on_done {
                            f(&result, trace.as_deref());
                        }
                        slots.lock().expect("slots")[i] = Some(result);
                        let n = done.fetch_add(1, Ordering::SeqCst) + 1;
                        if progress && (n % 10 == 0 || n == jobs.len()) {
                            let secs = started.elapsed().as_secs_f64();
                            eprintln!(
                                "  {n}/{} games, {:.0} s, eta {:.0} s",
                                jobs.len(),
                                secs,
                                secs / n as f64 * (jobs.len() - n) as f64
                            );
                        }
                    }
                })
                .expect("a worker thread starts");
        }
    });
    slots
        .into_inner()
        .expect("slots")
        .into_iter()
        .map(|r| r.expect("every job ran"))
        .collect()
}

// -- A/B -------------------------------------------------------------------------------------

/// Mean and 95% interval of a paired difference.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Estimate {
    pub n: usize,
    pub mean: f64,
    pub se: f64,
    pub ci95: [f64; 2],
}

#[must_use]
pub fn estimate(values: &[f64]) -> Estimate {
    let n = values.len();
    if n == 0 {
        return Estimate::default();
    }
    let mean = values.iter().sum::<f64>() / n as f64;
    let var = if n > 1 {
        values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / (n - 1) as f64
    } else {
        0.0
    };
    let se = (var / n as f64).sqrt();
    Estimate {
        n,
        mean,
        se,
        ci95: [mean - 1.96 * se, mean + 1.96 * se],
    }
}

/// The paired effect of the candidate on one slice (a faction, a map).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SliceEffect {
    /// Win share: faction slices: win of the seat as candidate minus as baseline; map slices:
    /// share of the pair's games won by candidate seats minus 50%.
    pub win: Estimate,
    pub vp: Estimate,
}

/// The result of an A/B run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Comparison {
    pub mode: String,
    pub baseline: std::collections::BTreeMap<String, String>,
    pub candidate: std::collections::BTreeMap<String, String>,
    pub tables: usize,
    /// Tables dropped because a game of the pair did not end in a victory.
    pub excluded: usize,
    /// Mixed mode: share of games won by a candidate seat, per pair of swapped games, minus the
    /// 50% that equal bots would get. Positive: the candidate is stronger.
    pub win_share_advantage: Estimate,
    /// The candidate's wins over its games, pooled, with a Wilson interval.
    pub candidate_win_rate: f64,
    pub candidate_win_rate_ci95: [f64; 2],
    /// Mixed mode: mean final points of the candidate's seats minus the baseline's.
    pub vp_advantage: Estimate,
    /// Both modes: paired differences (candidate minus baseline) of game length.
    pub rounds_diff: Estimate,
    pub decisions_diff: Estimate,
    pub wall_ms_diff: Estimate,
    pub finished_rate_baseline: f64,
    pub finished_rate_candidate: f64,
    /// Mixed mode: per faction, the paired effect of the candidate on the seats of that faction
    /// (wins and points of the faction's seat when it plays the candidate minus when it plays
    /// the baseline; the other seats vary with the mask, so this is noisy).
    #[serde(default)]
    pub by_faction: std::collections::BTreeMap<String, SliceEffect>,
    /// Mixed mode: per map and per table size, the win share advantage of the candidate seats.
    #[serde(default)]
    pub by_map: std::collections::BTreeMap<String, SliceEffect>,
    /// VP per round played (final points over rounds), paired by table: mixed mode, the
    /// candidate seats' mean minus the baseline seats'; separate mode, the all-candidate game's
    /// mean seat minus the all-baseline game's (an absolute strength measure).
    #[serde(default)]
    pub vp_per_round: Estimate,
    /// The same for the points held at the end of round 3, 4 and 5 (the final points when the
    /// game ended sooner).
    #[serde(default)]
    pub vp_r3: Estimate,
    #[serde(default)]
    pub vp_r4: Estimate,
    #[serde(default)]
    pub vp_r5: Estimate,
    /// Controllable victory points ([`crate::controllable`]), paired like the VP metrics above,
    /// from the stored timelines: keys `ctrl_vp_per_round`, `ctrl_r3`, `ctrl_r4`, `ctrl_r5`,
    /// `noisy_final` (the other sources) and `cat:<category>:r3|r5|final`.
    #[serde(default)]
    pub controllable: std::collections::BTreeMap<String, Estimate>,
    /// Tables whose games have no per-source split (no stored timeline).
    #[serde(default)]
    pub no_split: usize,
    /// `candidate` stronger / weaker / not distinguishable at 95%.
    pub verdict: String,
    pub wall_s: f64,
}

/// Points of `seat` at the end of `round` (the final points if the game ended before).
fn vp_at(g: &GameResult, seat: usize, round: usize) -> f64 {
    f64::from(g.vp_by_round.get(round - 1).map_or(g.vp[seat], |v| v[seat]))
}

/// A game's seat metric: `which` 0 = VP per round, 3..=5 = points after that round.
fn seat_metric(g: &GameResult, seat: usize, which: usize) -> f64 {
    if which == 0 {
        f64::from(g.vp[seat]) / f64::from(g.rounds.max(1))
    } else {
        vp_at(g, seat, which)
    }
}

/// Mean over the seats selected by `want` (`None`: all seats).
fn mean_metric(g: &GameResult, want: Option<(&[bool], bool)>, which: usize) -> f64 {
    let seats: Vec<usize> = (0..g.vp.len())
        .filter(|&s| want.is_none_or(|(mask, w)| mask[s] == w))
        .collect();
    seats.iter().map(|&s| seat_metric(g, s, which)).sum::<f64>() / seats.len().max(1) as f64
}

/// Controllable-VP metrics of one pair of games (`a` before, `b` after): seat metrics, averaged
/// over the candidate seats minus the baseline seats (mixed; over both games), or the mean seat
/// of `b` minus that of `a` (uniform tables).
fn controllable_pair(
    a: &GameResult,
    b: &GameResult,
    ta: &crate::controllable::Table,
    tb: &crate::controllable::Table,
    mixed: bool,
) -> Vec<(String, f64)> {
    use crate::controllable::CATEGORIES;
    type Metric = Box<dyn Fn(&crate::controllable::Table, &GameResult, usize) -> f64>;
    let end = |r: usize| {
        move |t: &crate::controllable::Table, g: &GameResult, s: usize| {
            t.at(s, Some(r.min(g.rounds as usize)), None, false)
        }
    };
    let mut metrics: Vec<(String, Metric)> = vec![
        (
            "ctrl_vp_per_round".to_owned(),
            Box::new(|t, g, s| t.at(s, None, None, false) / f64::from(g.rounds.max(1))),
        ),
        ("ctrl_r3".to_owned(), Box::new(end(3))),
        ("ctrl_r4".to_owned(), Box::new(end(4))),
        ("ctrl_r5".to_owned(), Box::new(end(5))),
        (
            "noisy_final".to_owned(),
            Box::new(|t, _, s| t.at(s, None, None, true)),
        ),
    ];
    for (i, name) in CATEGORIES.iter().enumerate() {
        for (label, round) in [("r3", Some(3usize)), ("r5", Some(5)), ("final", None)] {
            metrics.push((
                format!("cat:{name}:{label}"),
                Box::new(move |t, g, s| {
                    t.at(s, round.map(|r| r.min(g.rounds as usize)), Some(i), false)
                }),
            ));
        }
    }
    let seat_mean = |g: &GameResult,
                     t: &crate::controllable::Table,
                     want: Option<(&[bool], bool)>,
                     m: &Metric| {
        let seats: Vec<usize> = (0..g.vp.len())
            .filter(|&s| want.is_none_or(|(mask, w)| mask[s] == w))
            .collect();
        seats.iter().map(|&s| m(t, g, s)).sum::<f64>() / seats.len().max(1) as f64
    };
    metrics
        .iter()
        .map(|(name, m)| {
            let value = if mixed {
                [(a, ta), (b, tb)]
                    .iter()
                    .map(|(g, t)| {
                        let mask = g.candidate_seats.as_deref().expect("mask");
                        seat_mean(g, t, Some((mask, true)), m)
                            - seat_mean(g, t, Some((mask, false)), m)
                    })
                    .sum::<f64>()
                    / 2.0
            } else {
                seat_mean(b, tb, None, m) - seat_mean(a, ta, None, m)
            };
            (name.clone(), value)
        })
        .collect()
}

fn seats_mask(n: usize, shift: usize) -> Vec<bool> {
    (0..n).map(|i| (i + shift) % 2 == 0).collect()
}

fn tunings_for(mask: &[bool], baseline: Tuning, candidate: Tuning) -> Vec<Tuning> {
    mask.iter()
        .map(|&c| if c { candidate } else { baseline })
        .collect()
}

/// Run the comparison on `specs`.
///
/// `mixed`: each table is played twice with the same seed, map and factions; in the first game
/// the even seats play the candidate and the odd seats the baseline, in the second they swap.
/// A candidate seat wins when the winner is one of its seats. `separate`: the table is played
/// once with the baseline on every seat and once with the candidate on every seat, and the
/// games are compared as to length and how often they end.
pub fn compare(
    specs: &[GameSpec],
    limits: &Limits,
    baseline: Tuning,
    candidate: Tuning,
    mixed: bool,
    workers: usize,
    progress: bool,
) -> (Comparison, Vec<GameResult>) {
    let started = Instant::now();
    let mut jobs = Vec::new();
    for spec in specs {
        let n = spec.players;
        for shift in 0..2 {
            let (mask, tunings) = if mixed {
                let mask = seats_mask(n, shift);
                (Some(mask.clone()), tunings_for(&mask, baseline, candidate))
            } else {
                let all = shift == 1;
                (
                    Some(vec![all; n]),
                    vec![if all { candidate } else { baseline }],
                )
            };
            jobs.push(Job {
                spec: spec.clone(),
                options: PlayOptions {
                    limits: limits.clone(),
                    tunings,
                    candidate_seats: mask,
                    trace: false,
                    timeline: true,
                },
            });
        }
    }
    let results = run_jobs_cached(&jobs, workers, progress, None);
    comparison_from(results, baseline, candidate, mixed, started)
}

/// The A/B metrics of a played (or stored) run: `results` are the games of [`compare`] in job
/// order, two per table.
pub fn comparison_from(
    results: Vec<GameResult>,
    baseline: Tuning,
    candidate: Tuning,
    mixed: bool,
    started: Instant,
) -> (Comparison, Vec<GameResult>) {
    let (mut win_share, mut vp_adv, mut rounds, mut decisions, mut wall) =
        (vec![], vec![], vec![], vec![], vec![]);
    let mut metric: [Vec<f64>; 4] = Default::default();
    let mut ctrl: std::collections::BTreeMap<String, Vec<f64>> = std::collections::BTreeMap::new();
    let mut no_split = 0usize;
    let (mut excluded, mut cand_games, mut cand_wins) = (0usize, 0usize, 0usize);
    let (mut base_done, mut cand_done) = (0usize, 0usize);
    type Series = std::collections::BTreeMap<String, (Vec<f64>, Vec<f64>)>;
    let (mut fac, mut maps): (Series, Series) = (Series::new(), Series::new());
    for pair in results.chunks(2) {
        let (a, b) = (&pair[0], &pair[1]);
        let ok = |g: &GameResult| g.termination == "victory";
        if !mixed {
            base_done += usize::from(ok(a));
            cand_done += usize::from(ok(b));
        }
        if !(ok(a) && ok(b)) {
            excluded += 1;
            continue;
        }
        for (slot, which) in [0usize, 3, 4, 5].into_iter().enumerate() {
            let diff = if mixed {
                [a, b]
                    .iter()
                    .map(|g| {
                        let mask = g.candidate_seats.as_deref().expect("mask");
                        mean_metric(g, Some((mask, true)), which)
                            - mean_metric(g, Some((mask, false)), which)
                    })
                    .sum::<f64>()
                    / 2.0
            } else {
                mean_metric(b, None, which) - mean_metric(a, None, which)
            };
            metric[slot].push(diff);
        }
        match (
            crate::controllable::Table::of(a),
            crate::controllable::Table::of(b),
        ) {
            (Some(ta), Some(tb)) => {
                for (key, value) in controllable_pair(a, b, &ta, &tb, mixed) {
                    ctrl.entry(key).or_default().push(value);
                }
            }
            _ => no_split += 1,
        }
        rounds.push(f64::from(b.rounds) - f64::from(a.rounds));
        decisions.push(b.decisions as f64 - a.decisions as f64);
        wall.push(b.wall_ms - a.wall_ms);
        if mixed {
            let mut wins = 0.0;
            let mut vp = 0.0;
            for g in [a, b] {
                let mask = g
                    .candidate_seats
                    .as_ref()
                    .expect("mixed games carry the mask");
                let winner_is_cand = g.winner.is_some_and(|w| mask[w]);
                wins += f64::from(u8::from(winner_is_cand));
                cand_games += 1;
                cand_wins += usize::from(winner_is_cand);
                let mean = |want: bool| {
                    let v: Vec<f64> =
                        g.vp.iter()
                            .zip(mask)
                            .filter(|(_, c)| **c == want)
                            .map(|(v, _)| f64::from(*v))
                            .collect();
                    v.iter().sum::<f64>() / v.len().max(1) as f64
                };
                vp += mean(true) - mean(false);
            }
            win_share.push(wins / 2.0 - 0.5);
            vp_adv.push(vp / 2.0);
            let entry = maps
                .entry(format!("{}-{}", a.spec.players, a.spec.map))
                .or_default();
            entry.0.push(wins / 2.0 - 0.5);
            entry.1.push(vp / 2.0);
            let (ma, mb) = (a.candidate_seats.as_ref(), b.candidate_seats.as_ref());
            if let (Some(ma), Some(_mb)) = (ma, mb) {
                for (s, f) in a.spec.factions.iter().enumerate() {
                    let (cand, base) = if ma[s] { (a, b) } else { (b, a) };
                    let won = |g: &GameResult| f64::from(u8::from(g.winner == Some(s)));
                    let e = fac.entry(f.clone()).or_default();
                    e.0.push(won(cand) - won(base));
                    e.1.push(f64::from(cand.vp[s]) - f64::from(base.vp[s]));
                }
            }
        }
    }
    let n = results.len() / 2;
    let (lo, hi) = crate::stats::wilson(cand_wins, cand_games);
    let adv = estimate(&win_share);
    let verdict = if !mixed {
        "separate mode: compare the length and finish figures; strength needs mixed mode".to_owned()
    } else if adv.n == 0 {
        "no finished pairs".to_owned()
    } else if adv.ci95[0] > 0.0 {
        "candidate stronger (95%)".to_owned()
    } else if adv.ci95[1] < 0.0 {
        "candidate weaker (95%)".to_owned()
    } else {
        "no significant difference at 95%".to_owned()
    };
    let comparison = Comparison {
        mode: if mixed { "mixed" } else { "separate" }.to_owned(),
        baseline: baseline_diff(baseline),
        candidate: baseline_diff(candidate),
        tables: n,
        excluded,
        win_share_advantage: adv,
        candidate_win_rate: if cand_games == 0 {
            0.0
        } else {
            cand_wins as f64 / cand_games as f64
        },
        candidate_win_rate_ci95: [lo, hi],
        vp_advantage: estimate(&vp_adv),
        rounds_diff: estimate(&rounds),
        decisions_diff: estimate(&decisions),
        wall_ms_diff: estimate(&wall),
        finished_rate_baseline: if mixed {
            0.0
        } else {
            base_done as f64 / n.max(1) as f64
        },
        finished_rate_candidate: if mixed {
            0.0
        } else {
            cand_done as f64 / n.max(1) as f64
        },
        by_faction: fac
            .into_iter()
            .map(|(k, (w, v))| {
                (
                    k,
                    SliceEffect {
                        win: estimate(&w),
                        vp: estimate(&v),
                    },
                )
            })
            .collect(),
        by_map: maps
            .into_iter()
            .map(|(k, (w, v))| {
                (
                    k,
                    SliceEffect {
                        win: estimate(&w),
                        vp: estimate(&v),
                    },
                )
            })
            .collect(),
        controllable: ctrl.into_iter().map(|(k, v)| (k, estimate(&v))).collect(),
        no_split,
        vp_per_round: estimate(&metric[0]),
        vp_r3: estimate(&metric[1]),
        vp_r4: estimate(&metric[2]),
        vp_r5: estimate(&metric[3]),
        verdict,
        wall_s: started.elapsed().as_secs_f64(),
    };
    (comparison, results)
}

/// The numbers of `t` that differ from the shipped bot.
#[must_use]
pub fn baseline_diff(t: Tuning) -> std::collections::BTreeMap<String, String> {
    let default = Tuning::default().entries();
    t.entries()
        .into_iter()
        .zip(default)
        .filter(|(a, b)| a.1 != b.1)
        .map(|(a, _)| (a.0.to_owned(), a.1))
        .collect()
}

/// Text of a comparison.
#[must_use]
pub fn render_comparison(c: &Comparison) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    let _ = writeln!(
        out,
        "== A/B ({}) : baseline {:?} vs candidate {:?} ==",
        c.mode, c.baseline, c.candidate
    );
    let _ = writeln!(
        out,
        "tables {} (pairs excluded: {}), {:.1} s",
        c.tables, c.excluded, c.wall_s
    );
    let row = |out: &mut String, name: &str, e: &Estimate, scale: f64, unit: &str| {
        let _ = writeln!(
            out,
            "  {:<34} {:+.3}{unit}  95% CI [{:+.3}, {:+.3}]  (n={})",
            name,
            e.mean * scale,
            e.ci95[0] * scale,
            e.ci95[1] * scale,
            e.n
        );
    };
    if c.mode == "mixed" {
        row(
            &mut out,
            "win share vs 50% (candidate)",
            &c.win_share_advantage,
            100.0,
            " pp",
        );
        let _ = writeln!(
            out,
            "  candidate seats won {:.1}% of games [{:.1}, {:.1}] (equal bots: ~50%)",
            100.0 * c.candidate_win_rate,
            100.0 * c.candidate_win_rate_ci95[0],
            100.0 * c.candidate_win_rate_ci95[1]
        );
        row(
            &mut out,
            "VP per seat, candidate - baseline",
            &c.vp_advantage,
            1.0,
            " VP",
        );
    } else {
        let _ = writeln!(
            out,
            "  games finished: baseline {:.1}%  candidate {:.1}%",
            100.0 * c.finished_rate_baseline,
            100.0 * c.finished_rate_candidate
        );
    }
    row(&mut out, "VP per round (paired)", &c.vp_per_round, 1.0, "");
    if !c.controllable.is_empty() {
        let _ = writeln!(
            out,
            "controllable VP (objectives, custodians, Imperial's Mecatol point; not agenda, Support for the Throne, riders, relics){}:",
            if c.no_split > 0 {
                format!(", {} tables without a split skipped", c.no_split)
            } else {
                String::new()
            }
        );
        for key in [
            "ctrl_vp_per_round",
            "ctrl_r3",
            "ctrl_r4",
            "ctrl_r5",
            "noisy_final",
        ] {
            if let Some(e) = c.controllable.get(key) {
                row(&mut out, &format!("  {key}"), e, 1.0, "");
            }
        }
        for (key, e) in c
            .controllable
            .iter()
            .filter(|(k, _)| k.starts_with("cat:") && k.ends_with(":final"))
        {
            row(&mut out, &format!("  {key}"), e, 1.0, "");
        }
    } else if c.no_split > 0 {
        let _ = writeln!(
            out,
            "controllable VP: no stored per-source split in these games ({} tables)",
            c.no_split
        );
    }
    row(&mut out, "VP after round 3", &c.vp_r3, 1.0, "");
    row(&mut out, "VP after round 4", &c.vp_r4, 1.0, "");
    row(&mut out, "VP after round 5", &c.vp_r5, 1.0, "");
    row(
        &mut out,
        "rounds, candidate - baseline",
        &c.rounds_diff,
        1.0,
        "",
    );
    row(
        &mut out,
        "decisions, candidate - baseline",
        &c.decisions_diff,
        1.0,
        "",
    );
    row(
        &mut out,
        "wall ms, candidate - baseline",
        &c.wall_ms_diff,
        1.0,
        " ms",
    );
    let _ = writeln!(out, "verdict: {}", c.verdict);
    if !c.by_faction.is_empty() {
        let _ = writeln!(
            out,
            "per faction (seat as candidate minus as baseline; win pp, VP):"
        );
        for (name, e) in &c.by_faction {
            let _ = writeln!(
                out,
                "  {:<12} n={:<4} win {:+6.1} [{:+6.1}, {:+6.1}]  vp {:+.2} [{:+.2}, {:+.2}]",
                name,
                e.win.n,
                100.0 * e.win.mean,
                100.0 * e.win.ci95[0],
                100.0 * e.win.ci95[1],
                e.vp.mean,
                e.vp.ci95[0],
                e.vp.ci95[1]
            );
        }
    }
    out
}

#[cfg(test)]
mod controllable_tests {
    use super::*;
    use serde_json::json;

    fn game(mask: [bool; 2], events: serde_json::Value) -> GameResult {
        GameResult {
            vp: vec![2, 1],
            rounds: 4,
            candidate_seats: Some(mask.to_vec()),
            timeline: Some(json!({ "vp_events": events })),
            ..crate::play::empty_for_tests()
        }
    }

    #[test]
    fn controllable_pair_ignores_agenda_and_swaps() {
        // Game one: seat a is the candidate and scores an objective and an agenda point; game
        // two: the seats swap and the candidate (seat b) scores nothing.
        let one = game(
            [true, false],
            json!([
                {"round": 1, "seat": "a", "d": 1, "src": "objective"},
                {"round": 2, "seat": "a", "d": 1, "src": "agenda:mutiny"},
            ]),
        );
        let two = game([false, true], json!([]));
        let (ta, tb) = (
            crate::controllable::Table::of(&one).unwrap(),
            crate::controllable::Table::of(&two).unwrap(),
        );
        let m: std::collections::BTreeMap<_, _> = controllable_pair(&one, &two, &ta, &tb, true)
            .into_iter()
            .collect();
        // (1 - 0 + 0 - 0) / 2 controllable points, the agenda point is noisy.
        assert!((m["ctrl_r5"] - 0.5).abs() < 1e-9);
        assert!((m["noisy_final"] - 0.5).abs() < 1e-9);
        assert!((m["ctrl_vp_per_round"] - 0.125).abs() < 1e-9);
    }
}
