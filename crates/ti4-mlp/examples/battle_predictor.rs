//! Train a space-combat predictor straight from the lean arena simulator (ARENA-002 pilot).
//!
//! Nothing is stored. Each batch samples positions, simulates every one `--label-seeds` times on
//! the CPU for a soft attacker/defender/mutual label, and takes one optimiser step. A fixed
//! held-out set, labelled with far more seeds, is scored during training.
//!
//! A position is two fleets from the six factions in scope, with or without upgrades, up to
//! `--max-ships` non-fighter ships and `--max-fighters` fighters within capacity and supply.
//! Ships that can sustain damage may start the fight already damaged. Identical fleets fielded
//! under different faction names are one fleet. The split hashes the position without regard to
//! role, so a matchup and its mirror never straddle train and held-out.
//!
//! Features come from `ti4_policy::battle::encode`, the encoding live play uses. `--export`
//! writes the trained network as a `ti4_policy::battle::BattlePredictor` and checks that its
//! plain-Rust forward pass agrees with the trained one.

use std::collections::BTreeMap;
use std::time::Instant;

use rayon::prelude::*;
use tch::nn::{self, Module, OptimizerConfig};
use tch::{Device, Kind, Tensor};
use ti4_content::ContentStore;
use ti4_model::POK;
use ti4_policy::battle::{self as policy_battle, BattlePredictor, BattleSide, INPUT_WIDTH};
use ti4_training::battle_arena::{self as arena, Profile, Rng, Side};

fn argument(name: &str) -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    args.iter()
        .position(|arg| arg == name)
        .and_then(|at| args.get(at + 1))
        .cloned()
}

fn parse<T: std::str::FromStr>(name: &str, default: T) -> T {
    argument(name).map_or(default, |value| {
        value
            .parse()
            .unwrap_or_else(|_| panic!("{name} expects a number"))
    })
}

/// A distinct fleet: what it resolves to, not whose name is on it.
struct Fleet {
    faction: &'static str,
    units: Vec<(String, usize)>,
    /// Ids that can sustain damage, with their counts.
    sustainers: Vec<(String, usize)>,
    key: String,
}

struct Space {
    fleets: Vec<Fleet>,
}

impl Space {
    fn build(content: &ContentStore, max_ships: usize, max_fighters: usize) -> Self {
        let mut distinct: BTreeMap<String, Fleet> = BTreeMap::new();
        for profile in Profile::all(true) {
            for composition in arena::compositions(content, profile, max_ships, max_fighters) {
                let units = profile.resolve(content, &composition);
                let key = format!("{}|{units:?}", arena::modifier(profile.faction));
                distinct.entry(key.clone()).or_insert_with(|| {
                    let sustainers = units
                        .iter()
                        .filter(|(id, _)| {
                            ti4_content::units::unit_type(content, id, POK)
                                .is_some_and(|unit| unit.sustain_damage())
                        })
                        .cloned()
                        .collect();
                    Fleet {
                        faction: profile.faction,
                        units,
                        sustainers,
                        key,
                    }
                });
            }
        }
        let fleets: Vec<Fleet> = distinct.into_values().collect();
        for fleet in &fleets {
            for (id, _) in &fleet.units {
                assert!(
                    policy_battle::UNIT_IDS.contains(&id.as_str()),
                    "{id} is outside the battle encoding"
                );
            }
        }
        Self { fleets }
    }
}

struct Position {
    attacker: usize,
    defender: usize,
    attacker_damage: Vec<(String, usize)>,
    defender_damage: Vec<(String, usize)>,
}

fn damage(fleet: &Fleet, rng: &mut Rng) -> Vec<(String, usize)> {
    fleet
        .sustainers
        .iter()
        .map(|(id, n)| (id.clone(), rng.below(n + 1)))
        .collect()
}

impl Position {
    fn sample(space: &Space, rng: &mut Rng) -> Self {
        let attacker = rng.below(space.fleets.len());
        let defender = rng.below(space.fleets.len());
        Self {
            attacker,
            defender,
            attacker_damage: damage(&space.fleets[attacker], rng),
            defender_damage: damage(&space.fleets[defender], rng),
        }
    }

    /// Held out one position in ten, keyed without regard to role.
    fn held_out(&self, space: &Space) -> bool {
        let a = format!(
            "{}#{:?}",
            space.fleets[self.attacker].key, self.attacker_damage
        );
        let d = format!(
            "{}#{:?}",
            space.fleets[self.defender].key, self.defender_damage
        );
        let pair = if a <= d {
            format!("{a}~{d}")
        } else {
            format!("{d}~{a}")
        };
        arena::fnv(&pair) % 10 == 0
    }

    fn battle_side(fleet: &Fleet, hurt: &[(String, usize)]) -> BattleSide {
        BattleSide {
            units: fleet.units.clone(),
            damaged: hurt.to_vec(),
            modifier: arena::modifier(fleet.faction),
        }
    }

    fn features(&self, space: &Space, out: &mut Vec<f32>) {
        let attacker = Self::battle_side(&space.fleets[self.attacker], &self.attacker_damage);
        let defender = Self::battle_side(&space.fleets[self.defender], &self.defender_damage);
        let input = policy_battle::encode(&attacker, &defender).expect("space is in the encoding");
        out.extend_from_slice(&input);
    }

    fn label(&self, space: &Space, content: &ContentStore, seeds: u64, seed: u64) -> [f32; 3] {
        let (fa, fd) = (&space.fleets[self.attacker], &space.fleets[self.defender]);
        let a = Side::of(content, &fa.units, &self.attacker_damage, fa.faction, true);
        let d = Side::of(content, &fd.units, &self.defender_damage, fd.faction, true);
        let mut rng = Rng::new(seed);
        let mut counts = [0u32; 3];
        for _ in 0..seeds {
            let slot = match arena::fight(&a, &d, rng.next_u64()).0 {
                Some("a") => 0,
                Some("b") => 1,
                _ => 2,
            };
            counts[slot] += 1;
        }
        #[expect(clippy::cast_precision_loss, reason = "counts are small")]
        let label = counts.map(|count| count as f32 / seeds as f32);
        label
    }
}

/// Features, flattened, and labels for `size` fresh positions from one side of the split.
fn batch(
    space: &Space,
    content: &ContentStore,
    size: usize,
    seeds: u64,
    held_out: bool,
    base: u64,
) -> (Vec<f32>, Vec<[f32; 3]>) {
    let rows: Vec<(Vec<f32>, [f32; 3])> = (0..size)
        .into_par_iter()
        .map(|index| {
            let mut rng = Rng::new(base ^ (index as u64).wrapping_mul(0x2545_F491_4F6C_DD1D));
            let position = loop {
                let candidate = Position::sample(space, &mut rng);
                if candidate.held_out(space) == held_out {
                    break candidate;
                }
            };
            let mut features = Vec::with_capacity(INPUT_WIDTH);
            position.features(space, &mut features);
            let label = position.label(space, content, seeds, rng.next_u64());
            (features, label)
        })
        .collect();
    let mut features = Vec::with_capacity(size * INPUT_WIDTH);
    let mut labels = Vec::with_capacity(size);
    for (x, y) in rows {
        features.extend(x);
        labels.push(y);
    }
    (features, labels)
}

/// Mean label entropy: the part of the cross-entropy no model can remove.
fn entropy(labels: &[[f32; 3]]) -> f64 {
    let total: f64 = labels
        .iter()
        .flat_map(|row| row.iter())
        .filter(|p| **p > 0.0)
        .map(|p| -f64::from(*p) * f64::from(*p).ln())
        .sum();
    #[expect(clippy::cast_precision_loss, reason = "counts are small")]
    let mean = total / labels.len() as f64;
    mean
}

struct Scores {
    kl: f64,
    mae: f64,
    mae_contested: f64,
    mae_foregone: f64,
    contested: usize,
}

fn score(predicted: &[f32], labels: &[[f32; 3]]) -> Scores {
    let (mut kl, mut mae, mut mae_c, mut mae_f, mut contested) = (0.0, 0.0, 0.0, 0.0, 0usize);
    for (row, label) in predicted.chunks(3).zip(labels) {
        for (p, y) in row.iter().zip(label) {
            if *y > 0.0 {
                kl += f64::from(*y) * (f64::from(*y) / f64::from(p.max(1e-7))).ln();
            }
        }
        let gap = f64::from((row[0] - label[0]).abs());
        mae += gap;
        if (0.1..=0.9).contains(&label[0]) {
            mae_c += gap;
            contested += 1;
        } else {
            mae_f += gap;
        }
    }
    #[expect(clippy::cast_precision_loss, reason = "counts are small")]
    let (n, c) = (labels.len() as f64, contested as f64);
    Scores {
        kl: kl / n,
        mae: mae / n,
        mae_contested: mae_c / c.max(1.0),
        mae_foregone: mae_f / (n - c).max(1.0),
        contested,
    }
}

#[expect(clippy::too_many_lines, reason = "a linear training script")]
fn main() {
    let content = ContentStore::embedded();
    let max_ships = parse("--max-ships", 8usize);
    let max_fighters = parse("--max-fighters", 16usize);
    let steps = parse("--steps", 2000usize);
    let batch_size = parse("--batch", 4096usize);
    let seeds = parse("--label-seeds", 32u64);
    let held_size = parse("--held", 20_000usize);
    let held_seeds = parse("--held-seeds", 1024u64);
    let hidden = parse("--hidden", 512i64);
    let lr = parse("--lr", 1e-3f64);
    let lr_final = parse("--lr-final", lr * 0.02);
    let eval_every = parse("--eval-every", 200usize);
    let seed = parse("--seed", 20_260_917u64);
    let out = argument("--out");
    let export = argument("--export");

    tch::manual_seed(i64::try_from(seed).unwrap_or(0));
    let device = ti4_tensor::OptimizerDevice::Cuda
        .resolve()
        .unwrap_or(Device::Cpu);

    let started = Instant::now();
    let space = Space::build(content, max_ships, max_fighters);
    let width = INPUT_WIDTH;
    println!("battle predictor (ARENA-002 pilot)");
    println!(
        "  device        {device:?}   workers {}",
        rayon::current_num_threads()
    );
    println!(
        "  fleets        {} distinct  (max {max_ships} ships, {max_fighters} fighters, six factions +/- upgrades)",
        space.fleets.len()
    );
    println!(
        "  encoding      battle feature v{}, {} unit ids -> input width {width}",
        policy_battle::FEATURE_VERSION,
        policy_battle::UNIT_IDS.len()
    );
    println!(
        "  batch         {batch_size} positions x {seeds} seeds; held-out {held_size} x {held_seeds} seeds"
    );
    println!("  built in      {:.1?}", started.elapsed());

    let clock = Instant::now();
    let (held_x, held_y) = batch(
        &space,
        content,
        held_size,
        held_seeds,
        true,
        arena::fnv("held-out"),
    );
    println!(
        "  held-out      labelled in {:.1?}; {} of {held_size} contested (attacker rate 0.1-0.9); label entropy {:.4}",
        clock.elapsed(),
        held_y
            .iter()
            .filter(|y| (0.1..=0.9).contains(&y[0]))
            .count(),
        entropy(&held_y)
    );
    #[expect(clippy::cast_possible_wrap, reason = "sizes are small")]
    let held_tensor = Tensor::from_slice(&held_x)
        .view([held_size as i64, width as i64])
        .to_device(device);
    #[expect(clippy::cast_precision_loss, reason = "counts are small")]
    let prior = held_y.iter().map(|y| y[0]).sum::<f32>() / held_size as f32;
    let constant: Vec<f32> = held_y.iter().flat_map(|_| [prior, 0.0, 0.0]).collect();
    println!(
        "  baseline      constant attacker rate {prior:.3}: MAE {:.4}",
        score(&constant, &held_y).mae
    );
    println!();

    let vs = nn::VarStore::new(device);
    let root = vs.root();
    #[expect(clippy::cast_possible_wrap, reason = "sizes are small")]
    let input = width as i64;
    let net = nn::seq()
        .add(nn::linear(&root / "l1", input, hidden, Default::default()))
        .add_fn(|x| x.relu())
        .add(nn::linear(&root / "l2", hidden, hidden, Default::default()))
        .add_fn(|x| x.relu())
        .add(nn::linear(&root / "l3", hidden, 3, Default::default()));
    let mut optimiser = nn::Adam::default()
        .build(&vs, lr)
        .expect("optimiser builds");

    println!("  step   train KL   held KL   MAE all  contested  foregone   fights/s   elapsed");
    let mut simulated = 0u64;
    let mut sim_time = 0.0f64;
    let (mut loss_sum, mut entropy_sum, mut loss_steps) = (0.0f64, 0.0f64, 0usize);
    for step in 1..=steps {
        let clock = Instant::now();
        let (x, y) = batch(
            &space,
            content,
            batch_size,
            seeds,
            false,
            arena::fnv(&format!("train-{seed}-{step}")),
        );
        sim_time += clock.elapsed().as_secs_f64();
        simulated += batch_size as u64 * seeds;
        #[expect(clippy::cast_possible_wrap, reason = "sizes are small")]
        let rows = batch_size as i64;
        let xs = Tensor::from_slice(&x).view([rows, input]).to_device(device);
        let flat: Vec<f32> = y.iter().flatten().copied().collect();
        let ys = Tensor::from_slice(&flat).view([rows, 3]).to_device(device);
        // Cosine decay to `--lr-final`: a constant rate left late-run evaluations noisy.
        #[expect(clippy::cast_precision_loss, reason = "step counts are small")]
        let progress = (step - 1) as f64 / steps as f64;
        optimiser.set_lr(
            lr_final + (lr - lr_final) * 0.5 * (1.0 + (std::f64::consts::PI * progress).cos()),
        );
        let log_p = net.forward(&xs).log_softmax(-1, Kind::Float);
        #[expect(clippy::cast_precision_loss, reason = "sizes are small")]
        let loss = -(ys * log_p).sum(Kind::Float) / batch_size as f64;
        optimiser.backward_step(&loss);
        loss_sum += loss.double_value(&[]);
        entropy_sum += entropy(&y);
        loss_steps += 1;

        if step % eval_every == 0 || step == steps {
            let predicted = tch::no_grad(|| net.forward(&held_tensor).softmax(-1, Kind::Float));
            let predicted = ti4_tensor::to_vec(&predicted).expect("predictions read back");
            let scores = score(&predicted, &held_y);
            #[expect(clippy::cast_precision_loss, reason = "counts are small")]
            let train_kl = (loss_sum - entropy_sum) / loss_steps as f64;
            #[expect(clippy::cast_precision_loss, reason = "counts are small")]
            let rate = simulated as f64 / sim_time.max(1e-9);
            println!(
                "  {step:>5}  {train_kl:>8.4}  {:>8.4}  {:>8.4}  {:>9.4}  {:>8.4}  {rate:>9.0}  {:>8.1?}",
                scores.kl,
                scores.mae,
                scores.mae_contested,
                scores.mae_foregone,
                started.elapsed()
            );
            let _ = scores.contested;
            (loss_sum, entropy_sum, loss_steps) = (0.0, 0.0, 0);
        }
    }

    if let Some(path) = out {
        vs.save(&path).expect("weights saved");
        println!("  weights       {path}");
    }

    if let Some(path) = export {
        let variables = vs.variables();
        let layer = |name: &str| {
            let weight = &variables[&format!("{name}.weight")];
            let size = weight.size();
            let rows = usize::try_from(size[0]).expect("rows");
            let cols = usize::try_from(size[1]).expect("cols");
            let weight = ti4_tensor::to_vec(&weight.to_device(Device::Cpu).contiguous())
                .expect("weight reads back");
            let bias =
                ti4_tensor::to_vec(&variables[&format!("{name}.bias")].to_device(Device::Cpu))
                    .expect("bias reads back");
            (cols, rows, weight, bias)
        };
        let predictor = BattlePredictor::new(
            format!(
                "arena-lean-v1 sustain-first cheapest-fresh, both sides space cannon, flagship effects;                  max {max_ships} ships {max_fighters} fighters; seed {seed}, {steps} steps"
            ),
            vec![layer("l1"), layer("l2"), layer("l3")],
        )
        .expect("exported layers chain");
        std::fs::write(&path, predictor.to_json().expect("serialises")).expect("export written");

        // The plain-Rust forward pass must reproduce the trained network.
        let predicted = tch::no_grad(|| net.forward(&held_tensor).softmax(-1, Kind::Float));
        let predicted = ti4_tensor::to_vec(&predicted).expect("predictions read back");
        let mut worst = 0.0f32;
        for (row, expected) in held_x.chunks(INPUT_WIDTH).zip(predicted.chunks(3)) {
            let input: [f32; INPUT_WIDTH] = row.try_into().expect("row width");
            let p = predictor.predict(&input);
            for (got, want) in [p.attacker_wins, p.defender_wins, p.mutual_destruction]
                .iter()
                .zip(expected)
            {
                worst = worst.max((got - want).abs());
            }
        }
        println!("  exported      {path}  (CPU forward vs trained: max |diff| {worst:.2e})");
        assert!(
            worst < 1e-4,
            "exported predictor disagrees with the trained network"
        );
    }
}
