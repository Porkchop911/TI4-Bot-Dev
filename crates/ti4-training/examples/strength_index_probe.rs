//! Can one number per fleet stand in for the battle arena?
//!
//! Scores random fleet pairs from the arena's own catalogue (six factions, with and without
//! upgrades, up to 8 ships and 16 fighters) with candidate strength indices, fits
//! `P(attacker wins) = sigmoid(k * ln(S_a / S_d) + c)` on half the pairs, and reports how well
//! each index predicts the simulated win rate on the other half.
//!
//! ```text
//! cargo run --release -p ti4-training --example strength_index_probe -- [--pairs N] [--fights N]
//! ```

use rayon::prelude::*;

use ti4_content::ContentStore;
use ti4_model::POK;
use ti4_training::battle_arena::{self, Composition, Profile, Rng, Side};

/// What the indices read from a fleet.
#[derive(Debug, Clone, Copy, Default)]
struct Parts {
    cost: f64,
    /// Expected hits per combat round.
    firepower: f64,
    /// Hits the fleet can absorb, sustain included.
    durability: f64,
    /// Expected space-cannon hits before combat.
    cannon: f64,
    /// Expected anti-fighter-barrage hits in round 1.
    barrage: f64,
    fighters: f64,
}

fn hit_chance(hits_on: i64, modifier: i64) -> f64 {
    let needed = hits_on - modifier;
    (f64::from(i32::try_from(11 - needed).unwrap_or(0)) / 10.0).clamp(0.0, 1.0)
}

fn parts(content: &ContentStore, profile: Profile, fleet: &[(String, usize)]) -> Parts {
    let modifier = battle_arena::modifier(profile.faction);
    let mut out = Parts::default();
    for (id, count) in fleet {
        let unit = ti4_content::units::unit_type(content, id, POK).expect("unit exists");
        #[expect(clippy::cast_precision_loss, reason = "small counts")]
        let n = *count as f64;
        #[expect(clippy::cast_precision_loss, reason = "small counts")]
        let dice = unit.combat_dice() as f64;
        let p = unit
            .combat_hits_on()
            .map_or(0.0, |on| hit_chance(on, modifier));
        let mut per_ship = dice * p;
        if id == "jolnar_flagship" {
            // Each 9 or 10 before modifiers adds 2 hits.
            per_ship += dice * 0.2 * 2.0;
        }
        out.firepower += n * per_ship;
        let mut hp = 1.0 + f64::from(u8::from(unit.sustain_damage()));
        if id == "letnev_flagship" {
            hp += 1.0; // repairs every round: roughly one more hit over a fight
        }
        out.durability += n * hp;
        out.cost += n * unit.cost();
        #[expect(clippy::cast_precision_loss, reason = "small counts")]
        {
            out.cannon += n
                * unit.space_cannon_dice() as f64
                * unit
                    .space_cannon_hits_on()
                    .map_or(0.0, |on| hit_chance(on, modifier));
            out.barrage += n
                * unit.afb_dice() as f64
                * unit
                    .afb_hits_on()
                    .map_or(0.0, |on| hit_chance(on, modifier));
        }
        if unit.is_fighter() {
            out.fighters += n;
        }
    }
    out
}

/// Lanchester square law: a fleet's fighting strength is firepower times durability.
fn square(own: Parts) -> f64 {
    own.firepower * own.durability
}

/// The square law after the opening volleys: each side's space cannon, and its barrage up to the
/// other side's fighters, come off the other side's durability first, and firepower falls in the
/// same proportion.
fn square_after_opening(own: Parts, enemy: Parts) -> f64 {
    let lost = enemy.cannon + enemy.barrage.min(own.fighters);
    let left = (own.durability - lost).max(0.0);
    let scale = if own.durability > 0.0 {
        left / own.durability
    } else {
        0.0
    };
    own.firepower * scale * left
}

#[derive(Clone, Copy)]
struct Pair {
    cost: f64,
    square: f64,
    opening: f64,
    /// `opening`, scaled by the square root of the smaller side's durability: large fleets roll
    /// more dice, so luck matters less and the same ratio is more decisive.
    scaled: f64,
    /// Simulated attacker win rate (mutual destruction counts as not winning).
    win: f64,
}

fn ratio(a: f64, b: f64) -> f64 {
    ((a + 1e-3) / (b + 1e-3)).ln()
}

fn sigmoid(x: f64) -> f64 {
    1.0 / (1.0 + (-x).exp())
}

/// Least-squares fit of `sigmoid(k x + c)` to win rates by gradient descent.
fn fit(xs: &[f64], ys: &[f64]) -> (f64, f64) {
    let (mut k, mut c) = (1.0, 0.0);
    #[expect(clippy::cast_precision_loss, reason = "sample count")]
    let n = xs.len() as f64;
    for _ in 0..4000 {
        let (mut gk, mut gc) = (0.0, 0.0);
        for (x, y) in xs.iter().zip(ys) {
            let p = sigmoid(k * x + c);
            let g = (p - y) * p * (1.0 - p);
            gk += g * x;
            gc += g;
        }
        k -= 2.0 * gk / n;
        c -= 2.0 * gc / n;
    }
    (k, c)
}

fn report(name: &str, train: &[(f64, f64)], test: &[(f64, f64)]) {
    let (xs, ys): (Vec<f64>, Vec<f64>) = train.iter().copied().unzip();
    let (k, c) = fit(&xs, &ys);
    let mut all = (0.0, 0usize);
    let mut contested = (0.0, 0usize);
    let mut calls = (0usize, 0usize);
    for (x, y) in test {
        let p = sigmoid(k * x + c);
        let error = (p - y).abs();
        all.0 += error;
        all.1 += 1;
        if (0.2..=0.8).contains(y) {
            contested.0 += error;
            contested.1 += 1;
        }
        if (y - 0.5).abs() > 0.1 {
            calls.1 += 1;
            if (p > 0.5) == (*y > 0.5) {
                calls.0 += 1;
            }
        }
    }
    #[expect(clippy::cast_precision_loss, reason = "counts")]
    let mean = |(sum, n): (f64, usize)| 100.0 * sum / n.max(1) as f64;
    #[expect(clippy::cast_precision_loss, reason = "counts")]
    let share = 100.0 * calls.0 as f64 / calls.1.max(1) as f64;
    println!(
        "  {name:<24} k {k:6.2}  c {c:6.2}   MAE {:5.2}pp   contested MAE {:5.2}pp (n {})   right side of 50% {share:5.1}% (n {})",
        mean(all),
        mean(contested),
        contested.1,
        calls.1
    );
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let flag = |name: &str, default: usize| {
        args.iter()
            .position(|arg| arg == name)
            .and_then(|at| args.get(at + 1))
            .and_then(|value| value.parse().ok())
            .unwrap_or(default)
    };
    let pairs = flag("--pairs", 6000);
    let fights = flag("--fights", 400);
    let content = ContentStore::embedded();
    let profiles = Profile::all(true);
    let catalogue: Vec<(Profile, Vec<Composition>)> = profiles
        .iter()
        .map(|profile| {
            (
                *profile,
                battle_arena::compositions(content, *profile, 8, 16),
            )
        })
        .collect();

    let started = std::time::Instant::now();
    let samples: Vec<Pair> = (0..pairs)
        .into_par_iter()
        .map(|index| {
            let mut rng = Rng::new(0x5EED_0000 + index as u64);
            let (pa, fa) = &catalogue[rng.below(catalogue.len())];
            let (pd, fd) = &catalogue[rng.below(catalogue.len())];
            // Half the pairs are drawn near each other in size, so contested fights are common.
            let a = fa[rng.below(fa.len())];
            let d = if index % 2 == 0 {
                fd[rng.below(fd.len())]
            } else {
                let hulls: usize = a[1..].iter().sum();
                let near: Vec<&Composition> = fd
                    .iter()
                    .filter(|c| c[1..].iter().sum::<usize>().abs_diff(hulls) <= 1)
                    .collect();
                *near[rng.below(near.len())]
            };
            let fleet_a = pa.resolve(content, &a);
            let fleet_d = pd.resolve(content, &d);
            let side_a = Side::of(content, &fleet_a, &[], pa.faction, true);
            let side_d = Side::of(content, &fleet_d, &[], pd.faction, true);
            let wins = (0..fights)
                .filter(|seed| {
                    battle_arena::fight(&side_a, &side_d, (index as u64) << 20 | *seed as u64).0
                        == Some("a")
                })
                .count();
            let (qa, qd) = (parts(content, *pa, &fleet_a), parts(content, *pd, &fleet_d));
            #[expect(clippy::cast_precision_loss, reason = "counts")]
            let win = wins as f64 / fights as f64;
            Pair {
                cost: ratio(qa.cost, qd.cost),
                square: ratio(square(qa), square(qd)),
                opening: ratio(square_after_opening(qa, qd), square_after_opening(qd, qa)),
                scaled: ratio(square_after_opening(qa, qd), square_after_opening(qd, qa))
                    * qa.durability.min(qd.durability).max(1.0).sqrt(),
                win,
            }
        })
        .collect();
    #[expect(clippy::cast_precision_loss, reason = "fight count")]
    let noise = 50.0 / (fights as f64).sqrt();
    println!(
        "strength index probe: {pairs} pairs x {fights} fights in {:.1}s (sampling noise ~{noise:.1}pp at 50%)",
        started.elapsed().as_secs_f64(),
    );
    let (train, test) = samples.split_at(samples.len() / 2);
    let column = |set: &[Pair], pick: fn(&Pair) -> f64| -> Vec<(f64, f64)> {
        set.iter().map(|pair| (pick(pair), pair.win)).collect()
    };
    report(
        "fleet cost",
        &column(train, |p| p.cost),
        &column(test, |p| p.cost),
    );
    report(
        "firepower x durability",
        &column(train, |p| p.square),
        &column(test, |p| p.square),
    );
    report(
        "  ... after opening fire",
        &column(train, |p| p.opening),
        &column(test, |p| p.opening),
    );
    report(
        "  ... scaled by size",
        &column(train, |p| p.scaled),
        &column(test, |p| p.scaled),
    );
}
