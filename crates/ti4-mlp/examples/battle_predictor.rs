//! Train a space-combat predictor straight from the lean arena simulator (ARENA-002).
//!
//! Nothing is stored. Each batch samples positions, simulates every one `--label-seeds` times on
//! the CPU, and takes one optimiser step. Labels are the attacker/defender/mutual outcome rates
//! and each ship type's surviving fraction per side. A fixed held-out set, labelled with far more
//! seeds, is scored during training.
//!
//! A position is two fleets from the six factions in scope, with or without upgrades, up to
//! `--max-ships` non-fighter ships and `--max-fighters` fighters within capacity and supply.
//! Ships that can sustain damage may start the fight already damaged. Either side may have guns
//! (PDS or PDS II; for Xxcha also mechs and a flagship next door), and sometimes the defender is
//! guns alone. Identical fleets fielded under different faction names are one fleet. The split
//! hashes the position without regard to role, so a matchup and its mirror never straddle train
//! and held-out.
//!
//! Features come from `ti4_policy::battle::encode` at the current feature version, the encoding
//! live play uses. `--export` writes the trained network as a `BattlePredictor` and checks that
//! its plain-Rust forward pass agrees with the trained one.

use std::collections::BTreeMap;
use std::time::Instant;

use rayon::prelude::*;
use tch::nn::{self, Module, OptimizerConfig};
use tch::{Device, Kind, Tensor};
use ti4_content::ContentStore;
use ti4_model::POK;
use ti4_policy::battle::{
    self as policy_battle, BattlePredictor, BattleSide, FEATURE_VERSION, INPUT_WIDTH, OUTPUT_WIDTH,
    UNIT_IDS,
};
use ti4_training::battle_arena::{self as arena, Profile, Rng, Side};

/// Survival slots per side.
const SLOTS: usize = UNIT_IDS.len();

/// One label: three outcome rates, then attacker and defender survival, then their masks.
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
                    UNIT_IDS.contains(&id.as_str()),
                    "{id} is outside the battle encoding"
                );
            }
        }
        Self { fleets }
    }
}

/// One side of a sampled position: a fleet (or none, for a defender that is guns alone), its
/// starting damage, and its guns.
struct Party {
    fleet: Option<usize>,
    faction: &'static str,
    damage: Vec<(String, usize)>,
    guns: Vec<(String, usize)>,
}

struct Position {
    attacker: Party,
    defender: Party,
    /// A fight already under way: no space cannon, no barrage.
    in_progress: bool,
}

fn damage(fleet: &Fleet, rng: &mut Rng) -> Vec<(String, usize)> {
    fleet
        .sustainers
        .iter()
        .map(|(id, n)| (id.clone(), rng.below(n + 1)))
        .collect()
}

/// Guns for a side: PDS or PDS II, and for Xxcha mechs and (when the fleet has none) a flagship
/// firing from next door. Never empty when `at_least_one`.
fn guns(
    space: &Space,
    faction: &str,
    fleet: Option<usize>,
    at_least_one: bool,
    rng: &mut Rng,
) -> Vec<(String, usize)> {
    let mut out = Vec::new();
    loop {
        let pds = rng.below(4);
        if pds > 0 {
            let id = if rng.below(2) == 0 { "pds" } else { "pds2" };
            out.push((id.to_owned(), pds));
        }
        if faction == "xxcha" {
            let mechs = rng.below(3);
            if mechs > 0 {
                out.push(("xxcha_mech".to_owned(), mechs));
            }
            let has_flagship = fleet.is_some_and(|index| {
                space.fleets[index]
                    .units
                    .iter()
                    .any(|(id, _)| id == "xxcha_flagship")
            });
            if !has_flagship && rng.below(4) == 0 {
                out.push(("xxcha_flagship".to_owned(), 1));
            }
        }
        if !out.is_empty() || !at_least_one {
            return out;
        }
    }
}

impl Position {
    fn sample(space: &Space, rng: &mut Rng) -> Self {
        let party = |rng: &mut Rng, may_be_bare: bool| {
            let bare = may_be_bare && rng.below(12) == 0;
            let fleet = (!bare).then(|| rng.below(space.fleets.len()));
            let faction = fleet.map_or_else(
                || Profile::CATALOGUE[rng.below(Profile::CATALOGUE.len())].0,
                |index| space.fleets[index].faction,
            );
            let damage = fleet.map_or_else(Vec::new, |index| damage(&space.fleets[index], rng));
            // Guns are common on defence, rare on attack; guns alone always have some.
            let armed = bare || rng.below(if may_be_bare { 3 } else { 12 }) == 0;
            let guns = if armed {
                guns(space, faction, fleet, bare, rng)
            } else {
                Vec::new()
            };
            Party {
                fleet,
                faction,
                damage,
                guns,
            }
        };
        let attacker = party(rng, false);
        let mut defender = party(rng, true);
        // Two in five positions are fights under way, as a retreat announcement sees them. Guns
        // have already fired by then, and a fight needs defending ships.
        let in_progress = rng.below(5) < 2;
        let mut attacker = attacker;
        if in_progress {
            attacker.guns.clear();
            defender.guns.clear();
            if defender.fleet.is_none() {
                let index = rng.below(space.fleets.len());
                defender.faction = space.fleets[index].faction;
                defender.damage = damage(&space.fleets[index], rng);
                defender.fleet = Some(index);
            }
        }
        Self {
            attacker,
            defender,
            in_progress,
        }
    }

    fn key(space: &Space, party: &Party) -> String {
        let fleet = party
            .fleet
            .map_or("-", |index| space.fleets[index].key.as_str());
        format!(
            "{fleet}#{:?}#{:?}#{}",
            party.damage, party.guns, party.faction
        )
    }

    /// Held out one position in ten, keyed without regard to role.
    fn held_out(&self, space: &Space) -> bool {
        let a = format!("{}@{}", Self::key(space, &self.attacker), self.in_progress);
        let d = format!("{}@{}", Self::key(space, &self.defender), self.in_progress);
        let pair = if a <= d {
            format!("{a}~{d}")
        } else {
            format!("{d}~{a}")
        };
        arena::fnv(&pair) % 10 == 0
    }

    fn battle_side(space: &Space, party: &Party) -> BattleSide {
        BattleSide {
            units: party
                .fleet
                .map_or_else(Vec::new, |index| space.fleets[index].units.clone()),
            damaged: party.damage.clone(),
            guns: party.guns.clone(),
            modifier: arena::modifier(party.faction),
        }
    }

    fn features(&self, space: &Space, out: &mut Vec<f32>) {
        let attacker = Self::battle_side(space, &self.attacker);
        let defender = Self::battle_side(space, &self.defender);
        let input =
            policy_battle::encode_at(FEATURE_VERSION, &attacker, &defender, self.in_progress)
                .expect("the sampled space is in the encoding");
        out.extend_from_slice(&input);
    }

    fn side(space: &Space, content: &ContentStore, party: &Party) -> Side {
        let units = party
            .fleet
            .map_or_else(Vec::new, |index| space.fleets[index].units.clone());
        Side::of(content, &units, &party.damage, party.faction, true)
            .with_guns(content, &party.guns)
    }

    /// Outcome rates, survival fractions per slot and side, and the masks saying which slots the
    /// side fielded.
    fn label(&self, space: &Space, content: &ContentStore, seeds: u64, seed: u64) -> Vec<f32> {
        let a = Self::side(space, content, &self.attacker);
        let d = Self::side(space, content, &self.defender);
        let slots = |side: &Side| -> Vec<usize> {
            side.names()
                .iter()
                .map(|id| UNIT_IDS.iter().position(|known| known == id).expect("slot"))
                .collect()
        };
        let (a_slots, d_slots) = (slots(&a), slots(&d));
        let (a_start, d_start) = (a.fielded(), d.fielded());
        let mut rng = Rng::new(seed);
        let mut counts = [0u32; 3];
        let mut a_left = vec![0usize; a_start.len()];
        let mut d_left = vec![0usize; d_start.len()];
        for _ in 0..seeds {
            let outcome = if self.in_progress {
                arena::fight_in_progress(&a, &d, rng.next_u64())
            } else {
                arena::fight_outcome(&a, &d, rng.next_u64(), true)
            };
            let slot = match outcome.winner {
                Some("a") => 0,
                Some("b") => 1,
                _ => 2,
            };
            counts[slot] += 1;
            for (total, left) in a_left.iter_mut().zip(&outcome.attacker_left) {
                *total += left;
            }
            for (total, left) in d_left.iter_mut().zip(&outcome.defender_left) {
                *total += left;
            }
        }
        let mut label = vec![0.0f32; LABEL_WIDTH];
        #[expect(clippy::cast_precision_loss, reason = "counts are small")]
        let runs = seeds as f32;
        for (index, count) in counts.iter().enumerate() {
            #[expect(clippy::cast_precision_loss, reason = "counts are small")]
            let count = *count as f32;
            label[index] = count / runs;
        }
        for (offset, slots, start, left) in [
            (0, &a_slots, &a_start, &a_left),
            (SLOTS, &d_slots, &d_start, &d_left),
        ] {
            for ((slot, start), left) in slots.iter().zip(start).zip(left) {
                #[expect(clippy::cast_precision_loss, reason = "counts are small")]
                let fraction = *left as f32 / (*start as f32 * runs);
                label[3 + offset + slot] = fraction;
                label[3 + 2 * SLOTS + offset + slot] = 1.0;
            }
        }
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
) -> (Vec<f32>, Vec<Vec<f32>>) {
    let rows: Vec<(Vec<f32>, Vec<f32>)> = (0..size)
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

/// Mean outcome-label entropy: the part of the cross-entropy no model can remove.
fn entropy(labels: &[Vec<f32>]) -> f64 {
    let total: f64 = labels
        .iter()
        .flat_map(|row| row[..3].iter())
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
    guns_only_mae: f64,
    survival_mae: f64,
}

/// `predicted` rows: three outcome probabilities, then survival fractions for both sides.
fn score(predicted: &[f32], labels: &[Vec<f32>], x: &[f32]) -> Scores {
    let (mut kl, mut mae, mut mae_c, mut mae_f) = (0.0, 0.0, 0.0, 0.0);
    let (mut contested, mut bare, mut mae_bare) = (0usize, 0usize, 0.0);
    let (mut survival, mut slots) = (0.0, 0usize);
    let width = policy_battle::side_width(FEATURE_VERSION);
    let rows = predicted
        .chunks(3 + 2 * SLOTS)
        .zip(labels)
        .zip(x.chunks(INPUT_WIDTH));
    for ((row, label), input) in rows {
        for (p, y) in row[..3].iter().zip(&label[..3]) {
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
        if input[width..width + 2 * SLOTS].iter().all(|v| *v == 0.0) {
            mae_bare += gap;
            bare += 1;
        }
        for slot in 0..2 * SLOTS {
            if label[3 + 2 * SLOTS + slot] > 0.0 {
                survival += f64::from((row[3 + slot] - label[3 + slot]).abs());
                slots += 1;
            }
        }
    }
    #[expect(clippy::cast_precision_loss, reason = "counts are small")]
    let (n, c, b, s) = (
        labels.len() as f64,
        contested as f64,
        bare as f64,
        slots as f64,
    );
    Scores {
        kl: kl / n,
        mae: mae / n,
        mae_contested: mae_c / c.max(1.0),
        mae_foregone: mae_f / (n - c).max(1.0),
        guns_only_mae: mae_bare / b.max(1.0),
        survival_mae: survival / s.max(1.0),
    }
}

/// Outcome probabilities and survival fractions from raw outputs.
fn readout(logits: &Tensor) -> Tensor {
    let outcome = logits.narrow(1, 0, 3).softmax(-1, Kind::Float);
    #[expect(clippy::cast_possible_wrap, reason = "sizes are small")]
    let survival = logits.narrow(1, 3, 2 * SLOTS as i64).sigmoid();
    Tensor::cat(&[outcome, survival], 1)
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
    let survival_weight = parse("--survival-weight", 1.0f64);
    let seed = parse("--seed", 20_260_917u64);
    let out = argument("--out");
    let export = argument("--export");
    // Version 3 on carries a ground network; the space trainer copies it from this predictor.
    let ground = argument("--ground");

    tch::manual_seed(i64::try_from(seed).unwrap_or(0));
    let device = ti4_tensor::OptimizerDevice::Cuda
        .resolve()
        .unwrap_or(Device::Cpu);

    let started = Instant::now();
    let space = Space::build(content, max_ships, max_fighters);
    println!("battle predictor (ARENA-002)");
    println!(
        "  device        {device:?}   workers {}",
        rayon::current_num_threads()
    );
    println!(
        "  fleets        {} distinct  (max {max_ships} ships, {max_fighters} fighters, six factions +/- upgrades)",
        space.fleets.len()
    );
    println!(
        "  encoding      battle feature v{FEATURE_VERSION}: input {INPUT_WIDTH}, output {OUTPUT_WIDTH}"
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
    let bare = held_x
        .chunks(INPUT_WIDTH)
        .filter(|row| {
            let width = policy_battle::side_width(FEATURE_VERSION);
            row[width..width + 2 * SLOTS].iter().all(|v| *v == 0.0)
        })
        .count();
    println!(
        "  held-out      labelled in {:.1?}; {} contested; {bare} guns-only defenders; label entropy {:.4}",
        clock.elapsed(),
        held_y
            .iter()
            .filter(|y| (0.1..=0.9).contains(&y[0]))
            .count(),
        entropy(&held_y)
    );
    #[expect(clippy::cast_possible_wrap, reason = "sizes are small")]
    let held_tensor = Tensor::from_slice(&held_x)
        .view([held_size as i64, INPUT_WIDTH as i64])
        .to_device(device);
    println!();

    let vs = nn::VarStore::new(device);
    let root = vs.root();
    #[expect(clippy::cast_possible_wrap, reason = "sizes are small")]
    let (input, output) = (INPUT_WIDTH as i64, OUTPUT_WIDTH as i64);
    let net = nn::seq()
        .add(nn::linear(&root / "l1", input, hidden, Default::default()))
        .add_fn(|x| x.relu())
        .add(nn::linear(&root / "l2", hidden, hidden, Default::default()))
        .add_fn(|x| x.relu())
        .add(nn::linear(&root / "l3", hidden, output, Default::default()));
    let mut optimiser = nn::Adam::default()
        .build(&vs, lr)
        .expect("optimiser builds");

    println!(
        "  step   train KL   held KL   MAE all  contested  foregone  guns-only  survival   fights/s   elapsed"
    );
    let mut simulated = 0u64;
    let mut sim_time = 0.0f64;
    let (mut loss_sum, mut entropy_sum, mut loss_steps) = (0.0f64, 0.0f64, 0usize);
    #[expect(clippy::cast_possible_wrap, reason = "sizes are small")]
    let slots = SLOTS as i64;
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
        #[expect(clippy::cast_possible_wrap, reason = "sizes are small")]
        let ys = Tensor::from_slice(&flat)
            .view([rows, LABEL_WIDTH as i64])
            .to_device(device);
        // Cosine decay to `--lr-final`: a constant rate left late-run evaluations noisy.
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
        let target = ys.narrow(1, 3, 2 * slots);
        let mask = ys.narrow(1, 3 + 2 * slots, 2 * slots);
        let survival_loss = ((survival - target).square() * &mask).sum(Kind::Float)
            / mask.sum(Kind::Float).clamp_min(1.0);
        let loss = &outcome_loss + survival_loss * survival_weight;
        optimiser.backward_step(&loss);
        loss_sum += outcome_loss.double_value(&[]);
        entropy_sum += entropy(&y);
        loss_steps += 1;

        if step % eval_every == 0 || step == steps {
            let predicted = tch::no_grad(|| readout(&net.forward(&held_tensor)));
            let predicted = ti4_tensor::to_vec(&predicted).expect("predictions read back");
            let scores = score(&predicted, &held_y, &held_x);
            #[expect(clippy::cast_precision_loss, reason = "counts are small")]
            let train_kl = (loss_sum - entropy_sum) / loss_steps as f64;
            #[expect(clippy::cast_precision_loss, reason = "counts are small")]
            let rate = simulated as f64 / sim_time.max(1e-9);
            println!(
                "  {step:>5}  {train_kl:>8.4}  {:>8.4}  {:>8.4}  {:>9.4}  {:>8.4}  {:>9.4}  {:>8.4}  {rate:>9.0}  {:>8.1?}",
                scores.kl,
                scores.mae,
                scores.mae_contested,
                scores.mae_foregone,
                scores.guns_only_mae,
                scores.survival_mae,
                started.elapsed()
            );
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
        let ground_layers = ground.as_ref().map_or_else(Vec::new, |path| {
            BattlePredictor::from_json(&std::fs::read_to_string(path).expect("ground predictor"))
                .expect("ground predictor loads")
                .ground_layer_parts()
        });
        let predictor = BattlePredictor::assemble(
            FEATURE_VERSION,
            format!(
                "arena-lean-v4 sustain-first cheapest-fresh, both sides space cannon and guns, \
                 flagship effects, fights under way; max {max_ships} ships {max_fighters} \
                 fighters; seed {seed}, {steps} steps"
            ),
            vec![layer("l1"), layer("l2"), layer("l3")],
            ground_layers,
        )
        .expect("exported layers chain (version 3 on needs --ground)");
        std::fs::write(&path, predictor.to_json().expect("serialises")).expect("export written");

        // The plain-Rust forward pass must reproduce the trained network.
        let predicted = tch::no_grad(|| readout(&net.forward(&held_tensor)));
        let predicted = ti4_tensor::to_vec(&predicted).expect("predictions read back");
        let mut worst = 0.0f32;
        for (row, expected) in held_x
            .chunks(INPUT_WIDTH)
            .zip(predicted.chunks(3 + 2 * SLOTS))
        {
            let p = predictor.predict(row);
            let got = [p.attacker_wins, p.defender_wins, p.mutual_destruction]
                .into_iter()
                .chain(p.attacker_survival)
                .chain(p.defender_survival);
            for (got, want) in got.zip(expected) {
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
