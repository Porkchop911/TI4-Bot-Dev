//! Train the invasion predictor straight from the lean ground simulator (ARENA-002, version 3).
//!
//! Nothing is stored. Each batch samples invasions of one planet, simulates every one
//! `--label-seeds` times, and takes one optimiser step. Labels are take/hold rates and each
//! ground-force type's surviving fraction per side. A fixed held-out set, labelled with far more
//! seeds, is scored during training.
//!
//! An invasion is the forces an invader has landed (infantry and mechs of one of the six factions,
//! with or without upgrades; mechs may already be damaged) against the planet's defending forces
//! and PDS. An L1Z1X invader brings Harrow ships unless a planetary shield stands and no war sun
//! does. Bombardment is not sampled: it has already happened when forces are committed, which is
//! the decision this predictor serves.
//!
//! `--space <v2 predictor.json> --export <v3 predictor.json>` writes the version-3 predictor: the
//! version-2 space network unchanged plus this ground network, and checks the plain-Rust forward
//! pass against the trained one.

use std::collections::BTreeMap;
use std::time::Instant;

use rayon::prelude::*;
use tch::nn::{self, Module, OptimizerConfig};
use tch::{Device, Kind, Tensor};
use ti4_content::ContentStore;
use ti4_policy::battle::{
    self as policy_battle, BattlePredictor, GROUND_IDS, GROUND_INPUT_WIDTH, GROUND_OUTPUT_WIDTH,
    GroundBattleSide,
};
use ti4_training::battle_arena::{self as arena, GroundSide, Profile, Rng};

const SLOTS: usize = GROUND_IDS.len();
const LABEL_WIDTH: usize = 3 + 4 * SLOTS;

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

struct Party {
    faction: &'static str,
    forces: Vec<(String, usize)>,
    damaged: Vec<(String, usize)>,
    guns: Vec<(String, usize)>,
    harrow: Vec<(String, usize)>,
}

fn counted(pairs: &[(String, usize)]) -> Vec<(String, usize)> {
    let mut map: BTreeMap<String, usize> = BTreeMap::new();
    for (id, n) in pairs {
        if *n > 0 {
            *map.entry(id.clone()).or_default() += n;
        }
    }
    map.into_iter().collect()
}

fn sample(content: &ContentStore, rng: &mut Rng) -> (Party, Party) {
    let profiles = Profile::all(true);
    let forces = |profile: Profile, rng: &mut Rng, at_least_one: bool| loop {
        let infantry = rng.below(9);
        let mechs = rng.below(5);
        if at_least_one && infantry + mechs == 0 {
            continue;
        }
        let mech = profile.unit_for(content, "mech");
        let hurt = if rng.below(3) == 0 {
            rng.below(mechs + 1)
        } else {
            0
        };
        return (
            counted(&[
                (profile.unit_for(content, "infantry"), infantry),
                (mech.clone(), mechs),
            ]),
            counted(&[(mech, hurt)]),
        );
    };
    let attacker_profile = profiles[rng.below(profiles.len())];
    let defender_profile = profiles[rng.below(profiles.len())];
    let (a_forces, a_damaged) = forces(attacker_profile, rng, true);
    let (d_forces, d_damaged) = forces(defender_profile, rng, false);
    let pds = if rng.below(2) == 0 { rng.below(4) } else { 0 };
    let guns = counted(&[(defender_profile.unit_for(content, "pds"), pds)]);
    let mut harrow = Vec::new();
    if attacker_profile.faction == "l1z1x" {
        let dreads = rng.below(5);
        let war_suns = rng.below(3);
        if pds == 0 || war_suns > 0 {
            harrow = counted(&[
                (attacker_profile.unit_for(content, "dreadnought"), dreads),
                ("warsun".to_owned(), war_suns),
            ]);
        }
    }
    (
        Party {
            faction: attacker_profile.faction,
            forces: a_forces,
            damaged: a_damaged,
            guns: Vec::new(),
            harrow,
        },
        Party {
            faction: defender_profile.faction,
            forces: d_forces,
            damaged: d_damaged,
            guns,
            harrow: Vec::new(),
        },
    )
}

fn battle_side(party: &Party) -> GroundBattleSide {
    GroundBattleSide {
        forces: party.forces.clone(),
        damaged: party.damaged.clone(),
        guns: party.guns.clone(),
        harrow: party.harrow.clone(),
        modifier: arena::modifier(party.faction),
    }
}

fn lean(content: &ContentStore, party: &Party) -> GroundSide {
    GroundSide::of(content, &party.forces, &party.damaged, party.faction)
        .with_defense_guns(content, &party.guns)
        .with_bombardment(content, &party.harrow, false, true)
}

fn key(party: &Party) -> String {
    format!(
        "{}#{:?}#{:?}#{:?}#{:?}",
        party.faction, party.forces, party.damaged, party.guns, party.harrow
    )
}

fn held_out(a: &Party, d: &Party) -> bool {
    arena::fnv(&format!("{}~{}", key(a), key(d))) % 10 == 0
}

fn label(content: &ContentStore, a: &Party, d: &Party, seeds: u64, seed: u64) -> Vec<f32> {
    let (sa, sd) = (lean(content, a), lean(content, d));
    let slots = |side: &GroundSide| -> Vec<usize> {
        side.names()
            .iter()
            .map(|id| {
                GROUND_IDS
                    .iter()
                    .position(|known| known == id)
                    .expect("slot")
            })
            .collect()
    };
    let (a_slots, d_slots) = (slots(&sa), slots(&sd));
    let (a_start, d_start) = (sa.fielded(), sd.fielded());
    let mut rng = Rng::new(seed);
    let mut takes = 0u32;
    let mut a_left = vec![0usize; a_start.len()];
    let mut d_left = vec![0usize; d_start.len()];
    for _ in 0..seeds {
        let outcome = arena::ground_fight(&sa, &sd, rng.next_u64());
        if outcome.winner == Some("a") {
            takes += 1;
        }
        for (total, left) in a_left.iter_mut().zip(&outcome.attacker_left) {
            *total += left;
        }
        for (total, left) in d_left.iter_mut().zip(&outcome.defender_left) {
            *total += left;
        }
    }
    #[expect(clippy::cast_precision_loss, reason = "counts are small")]
    let (runs, take) = (seeds as f32, takes as f32);
    let mut out = vec![0.0f32; LABEL_WIDTH];
    out[0] = take / runs;
    out[1] = 1.0 - take / runs;
    for (offset, slots, start, left) in [
        (0, &a_slots, &a_start, &a_left),
        (SLOTS, &d_slots, &d_start, &d_left),
    ] {
        for ((slot, start), left) in slots.iter().zip(start).zip(left) {
            #[expect(clippy::cast_precision_loss, reason = "counts are small")]
            let fraction = *left as f32 / (*start as f32 * runs);
            out[3 + offset + slot] = fraction;
            out[3 + 2 * SLOTS + offset + slot] = 1.0;
        }
    }
    out
}

fn batch(
    content: &ContentStore,
    size: usize,
    seeds: u64,
    held: bool,
    base: u64,
) -> (Vec<f32>, Vec<Vec<f32>>) {
    let rows: Vec<(Vec<f32>, Vec<f32>)> = (0..size)
        .into_par_iter()
        .map(|index| {
            let mut rng = Rng::new(base ^ (index as u64).wrapping_mul(0x2545_F491_4F6C_DD1D));
            let (a, d) = loop {
                let (a, d) = sample(content, &mut rng);
                if held_out(&a, &d) == held {
                    break (a, d);
                }
            };
            let x = policy_battle::encode_ground(&battle_side(&a), &battle_side(&d))
                .expect("sampled invasions are in the encoding");
            (x, label(content, &a, &d, seeds, rng.next_u64()))
        })
        .collect();
    let mut xs = Vec::with_capacity(size * GROUND_INPUT_WIDTH);
    let mut ys = Vec::with_capacity(size);
    for (x, y) in rows {
        xs.extend(x);
        ys.push(y);
    }
    (xs, ys)
}

fn readout(logits: &Tensor) -> Tensor {
    let outcome = logits.narrow(1, 0, 3).softmax(-1, Kind::Float);
    #[expect(clippy::cast_possible_wrap, reason = "sizes are small")]
    let survival = logits.narrow(1, 3, 2 * SLOTS as i64).sigmoid();
    Tensor::cat(&[outcome, survival], 1)
}

/// Take-rate MAE (all, contested), survival MAE over fielded slots.
fn score(predicted: &[f32], labels: &[Vec<f32>]) -> (f64, f64, f64) {
    let (mut mae, mut contested, mut n_contested, mut survival, mut slots) =
        (0.0, 0.0, 0usize, 0.0, 0usize);
    for (row, label) in predicted.chunks(3 + 2 * SLOTS).zip(labels) {
        let gap = f64::from((row[0] - label[0]).abs());
        mae += gap;
        if (0.1..=0.9).contains(&label[0]) {
            contested += gap;
            n_contested += 1;
        }
        for slot in 0..2 * SLOTS {
            if label[3 + 2 * SLOTS + slot] > 0.0 {
                survival += f64::from((row[3 + slot] - label[3 + slot]).abs());
                slots += 1;
            }
        }
    }
    #[expect(clippy::cast_precision_loss, reason = "counts are small")]
    let (n, c, s) = (labels.len() as f64, n_contested as f64, slots as f64);
    (mae / n, contested / c.max(1.0), survival / s.max(1.0))
}

#[expect(clippy::too_many_lines, reason = "a linear training script")]
fn main() {
    let content = ContentStore::embedded();
    let steps = parse("--steps", 10_000usize);
    let batch_size = parse("--batch", 4096usize);
    let seeds = parse("--label-seeds", 32u64);
    let held_size = parse("--held", 20_000usize);
    let held_seeds = parse("--held-seeds", 4096u64);
    let hidden = parse("--hidden", 256i64);
    let lr = parse("--lr", 1e-3f64);
    let lr_final = parse("--lr-final", lr * 0.02);
    let eval_every = parse("--eval-every", 2000usize);
    let seed = parse("--seed", 20_260_917u64);
    let space = argument("--space");
    let export = argument("--export");

    tch::manual_seed(i64::try_from(seed).unwrap_or(0));
    let device = ti4_tensor::OptimizerDevice::Cuda
        .resolve()
        .unwrap_or(Device::Cpu);
    let started = Instant::now();
    println!("ground predictor (ARENA-002 v3)   device {device:?}");
    println!(
        "  input {GROUND_INPUT_WIDTH}, output {GROUND_OUTPUT_WIDTH}; batch {batch_size} x {seeds}; held-out {held_size} x {held_seeds}"
    );
    let (held_x, held_y) = batch(
        content,
        held_size,
        held_seeds,
        true,
        arena::fnv("ground-held"),
    );
    println!(
        "  held-out labelled in {:.1?}; {} contested",
        started.elapsed(),
        held_y
            .iter()
            .filter(|y| (0.1..=0.9).contains(&y[0]))
            .count()
    );
    #[expect(clippy::cast_possible_wrap, reason = "sizes are small")]
    let held_tensor = Tensor::from_slice(&held_x)
        .view([held_size as i64, GROUND_INPUT_WIDTH as i64])
        .to_device(device);

    let vs = nn::VarStore::new(device);
    let root = vs.root();
    #[expect(clippy::cast_possible_wrap, reason = "sizes are small")]
    let (input, output, slots) = (
        GROUND_INPUT_WIDTH as i64,
        GROUND_OUTPUT_WIDTH as i64,
        SLOTS as i64,
    );
    let net = nn::seq()
        .add(nn::linear(&root / "l1", input, hidden, Default::default()))
        .add_fn(|x| x.relu())
        .add(nn::linear(&root / "l2", hidden, hidden, Default::default()))
        .add_fn(|x| x.relu())
        .add(nn::linear(&root / "l3", hidden, output, Default::default()));
    let mut optimiser = nn::Adam::default()
        .build(&vs, lr)
        .expect("optimiser builds");

    println!("  step    MAE all  contested  survival  elapsed");
    for step in 1..=steps {
        let (x, y) = batch(
            content,
            batch_size,
            seeds,
            false,
            arena::fnv(&format!("ground-train-{seed}-{step}")),
        );
        #[expect(clippy::cast_possible_wrap, reason = "sizes are small")]
        let rows = batch_size as i64;
        let xs = Tensor::from_slice(&x).view([rows, input]).to_device(device);
        let flat: Vec<f32> = y.iter().flatten().copied().collect();
        #[expect(clippy::cast_possible_wrap, reason = "sizes are small")]
        let ys = Tensor::from_slice(&flat)
            .view([rows, LABEL_WIDTH as i64])
            .to_device(device);
        #[expect(clippy::cast_precision_loss, reason = "step counts are small")]
        let progress = (step - 1) as f64 / steps as f64;
        optimiser.set_lr(
            lr_final + (lr - lr_final) * 0.5 * (1.0 + (std::f64::consts::PI * progress).cos()),
        );
        let logits = net.forward(&xs);
        let log_p = logits.narrow(1, 0, 3).log_softmax(-1, Kind::Float);
        #[expect(clippy::cast_precision_loss, reason = "sizes are small")]
        let outcome_loss = -(ys.narrow(1, 0, 3) * log_p).sum(Kind::Float) / batch_size as f64;
        let survival = logits.narrow(1, 3, 2 * slots).sigmoid();
        let mask = ys.narrow(1, 3 + 2 * slots, 2 * slots);
        let survival_loss = ((survival - ys.narrow(1, 3, 2 * slots)).square() * &mask)
            .sum(Kind::Float)
            / mask.sum(Kind::Float).clamp_min(1.0);
        optimiser.backward_step(&(outcome_loss + survival_loss));
        if step % eval_every == 0 || step == steps {
            let predicted = tch::no_grad(|| readout(&net.forward(&held_tensor)));
            let predicted = ti4_tensor::to_vec(&predicted).expect("predictions read back");
            let (mae, contested, survival) = score(&predicted, &held_y);
            println!(
                "  {step:>5}  {mae:>8.4}  {contested:>9.4}  {survival:>8.4}  {:>7.1?}",
                started.elapsed()
            );
        }
    }

    if let (Some(space), Some(path)) = (space, export) {
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
        let base = BattlePredictor::from_json(
            &std::fs::read_to_string(&space).expect("space predictor reads"),
        )
        .expect("space predictor loads");
        let predictor = base
            .with_ground(
                format!("arena-lean-ground-v1, seed {seed}, {steps} steps"),
                vec![layer("l1"), layer("l2"), layer("l3")],
            )
            .expect("ground layers chain");
        std::fs::write(&path, predictor.to_json().expect("serialises")).expect("export written");
        let predicted = tch::no_grad(|| readout(&net.forward(&held_tensor)));
        let predicted = ti4_tensor::to_vec(&predicted).expect("predictions read back");
        let mut worst = 0.0f32;
        for (row, expected) in held_x
            .chunks(GROUND_INPUT_WIDTH)
            .zip(predicted.chunks(3 + 2 * SLOTS))
        {
            let p = predictor.predict_ground(row).expect("version 3");
            let got = [p.attacker_wins, p.defender_wins, p.mutual_destruction]
                .into_iter()
                .chain(p.attacker_survival)
                .chain(p.defender_survival);
            for (got, want) in got.zip(expected) {
                worst = worst.max((got - want).abs());
            }
        }
        println!("  exported {path} (version 3; CPU vs trained max |diff| {worst:.2e})");
        assert!(worst < 1e-4, "exported ground network disagrees");
    }
}
