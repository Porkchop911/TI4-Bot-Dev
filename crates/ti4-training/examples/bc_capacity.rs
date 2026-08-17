#![expect(
    clippy::cast_precision_loss,
    reason = "decision counts, option counts and victory points are small integers"
)]

//! Can the learned policy's representation express good play at all?
//!
//! The Stage-2 plateau has two candidate causes and they call for opposite work:
//!
//! - **The optimiser** cannot find the weights. Then PPO, a value baseline and GAE are the
//!   investment, and the policy class is fine.
//! - **The policy class** cannot express them. Then no optimiser helps, and the features have to
//!   change.
//!
//! Training experiments cannot separate these: a run that plateaus looks identical either way.
//! This one can, and without training anything.
//!
//! # The test
//!
//! [`crate::bot::ScoredBot`] is a hand-written, non-linear, *relational* policy — it knows that a
//! ground force lands to take a planet rather than to reinforce a garrison that is already
//! superior. Play games with it, log every decision it makes, and then ask the learned
//! representation to **imitate** it: fit the same linear-softmax-over-options form the policy
//! uses, by supervised cross-entropy, on those decisions.
//!
//! What matters is the **training** agreement, not the held-out figure. This is a capacity
//! question, not a generalisation question: if a linear function of these features cannot
//! reproduce the teacher's choices on the very data it is fitted to, then no amount of policy
//! gradient will find a better policy in that class, because it is not in the class. Held-out
//! agreement is reported alongside because a large gap says something different again — that the
//! features memorise rather than generalise.
//!
//! # Reading the result
//!
//! Compared against chance, which is `1 / options` for each decision:
//!
//! - **train agreement near chance** — the representation cannot express the teacher. The
//!   plateau is the policy class. Changing the optimiser cannot help.
//! - **train agreement high, and RL never found it** — the class contains good play and the
//!   optimiser is failing to reach it.
//!
//! The fit is the same functional form as inference — one weight per feature, scores dotted,
//! softmax over the option set — so the gradient of the cross-entropy is exactly
//! `φ_chosen − Σₒ pₒ φₒ`, the same expression the policy-gradient trainer uses. Nothing here is a
//! different model being asked an easier question.
//!
//! ```text
//! cargo run -p ti4-training --example bc_capacity --release -- --games 60 --epochs 12
//! ```

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use ti4_content::ContentStore;
use ti4_engine::choice::{Choice, ChoiceOption, Decider, IllegalChoice, Observed};
use ti4_engine::opening::DEFAULT_REQUIREMENT;
use ti4_model::content_types::FULL;
use ti4_model::id::{FactionId, PlayerId};
use ti4_policy::bot::ScoredBot;
use ti4_policy::features::{FeatureVector, explicit_choice_features};
use ti4_policy::intern::FeatureKey;
use ti4_policy::learned::decision_head;
use ti4_training::rollout::{Horizon, OpeningMap, play_with_deciders};

const FACTIONS: [&str; 6] = ["sol", "letnev", "xxcha", "hacan", "jolnar", "l1z1x"];

fn decimal(name: &str, fallback: f64) -> f64 {
    let args: Vec<String> = std::env::args().collect();
    args.iter()
        .position(|argument| argument == name)
        .and_then(|index| args.get(index + 1))
        .and_then(|value| value.parse().ok())
        .unwrap_or(fallback)
}

fn number(name: &str, fallback: usize) -> usize {
    let args: Vec<String> = std::env::args().collect();
    args.iter()
        .position(|argument| argument == name)
        .and_then(|index| args.get(index + 1))
        .and_then(|value| value.parse().ok())
        .unwrap_or(fallback)
}

/// One decision the teacher made: what it was shown, and which option it took.
struct Sample {
    head: String,
    options: Vec<FeatureVector>,
    chosen: usize,
}

/// A `ScoredBot` that records the representation's view of every decision it takes.
struct Teacher {
    inner: ScoredBot,
    log: Rc<RefCell<Vec<Sample>>>,
}

impl Decider for Teacher {
    fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
        // No position in hand means no features; such decisions are not part of the question.
        self.inner.choose(choice)
    }

    fn choose_seeing(
        &mut self,
        choice: &Choice,
        seen: &Observed<'_>,
    ) -> Result<ChoiceOption, IllegalChoice> {
        let picked = self.inner.choose_seeing(choice, seen)?;
        // A single-option decision decides nothing and would count as a free hit.
        if choice.options.len() > 1
            && let Some(chosen) = choice.options.iter().position(|o| o.id == picked.id)
        {
            self.log.borrow_mut().push(Sample {
                head: decision_head(choice).to_owned(),
                options: explicit_choice_features(seen, choice, &choice.player),
                chosen,
            });
        }
        Ok(picked)
    }
}

/// Softmax over the option scores, shifted by the best so a large score cannot overflow.
fn probabilities(scores: &[f64]) -> Vec<f64> {
    let best = scores.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let weights: Vec<f64> = scores.iter().map(|s| (s - best).exp()).collect();
    let total: f64 = weights.iter().sum();
    if total <= 0.0 || !total.is_finite() {
        return vec![1.0 / scores.len() as f64; scores.len()];
    }
    weights.into_iter().map(|w| w / total).collect()
}

fn score(weights: &BTreeMap<FeatureKey, f64>, vector: &FeatureVector) -> f64 {
    vector
        .iter()
        .map(|(key, value)| weights.get(key).copied().unwrap_or(0.0) * value)
        .sum()
}

/// Whether the model's best-scoring option is the one the teacher took.
fn agrees(weights: &BTreeMap<FeatureKey, f64>, sample: &Sample) -> bool {
    let mut best = (f64::NEG_INFINITY, usize::MAX);
    for (index, vector) in sample.options.iter().enumerate() {
        let value = score(weights, vector);
        if value > best.0 {
            best = (value, index);
        }
    }
    best.1 == sample.chosen
}

#[expect(
    clippy::too_many_lines,
    reason = "one diagnostic reads as one linear procedure: collect, split, fit, report"
)]
fn main() {
    let games = number("--games", 60);
    let epochs = number("--epochs", 12);
    // Averaged over the head's decisions and decayed, because a raw per-decision step at this
    // scale diverges: a first pilot produced *below-chance* training accuracy on some heads,
    // which is the signature of an optimiser blowing up rather than of a model that cannot fit.
    let learning_rate = decimal("--learning-rate", 0.05);
    let content = ContentStore::embedded();

    let players: Vec<PlayerId> = (0..6).map(|i| PlayerId::new(format!("seat{i}"))).collect();
    let factions: BTreeMap<PlayerId, FactionId> = players
        .iter()
        .cloned()
        .zip(FACTIONS.iter().map(|f| FactionId::new(*f)))
        .collect();

    println!("collecting decisions from the authored bot over {games} games...");
    let mut samples: Vec<Sample> = Vec::new();
    // The teacher's own strength, because "can the class fit it" only matters if it is worth
    // fitting. Compared against the learned champion's 13.71 table total on its panel.
    let mut teacher_vp: BTreeMap<FactionId, (i64, usize)> = BTreeMap::new();
    let first_seed = number("--first-seed", 0) as u64;
    for seed in first_seed..first_seed + games as u64 {
        let log = Rc::new(RefCell::new(Vec::new()));
        let deciders: BTreeMap<PlayerId, Box<dyn Decider>> = players
            .iter()
            .enumerate()
            .map(|(index, player)| {
                let bot = Teacher {
                    inner: ScoredBot::new(seed.wrapping_mul(1_000_003) + index as u64),
                    log: Rc::clone(&log),
                };
                (player.clone(), Box::new(bot) as Box<dyn Decider>)
            })
            .collect();
        let rollout = play_with_deciders(
            content,
            &players,
            &factions,
            FULL,
            seed,
            Horizon::rounds(4),
            DEFAULT_REQUIREMENT,
            &OpeningMap::RustVaried,
            deciders,
        );
        if let Some(error) = &rollout.error {
            eprintln!("  seed {seed}: {error}");
            continue;
        }
        for seat in &rollout.seats {
            let entry = teacher_vp.entry(seat.faction.clone()).or_default();
            entry.0 += seat.episode.final_progress.victory_points;
            entry.1 += 1;
        }
        samples.append(&mut log.borrow_mut());
    }

    let table: f64 = teacher_vp
        .values()
        .map(|(points, games)| *points as f64 / *games as f64)
        .sum();
    println!(
        "
teacher strength: table total {table:.2} VP over {games} games"
    );
    for (faction, (points, played)) in &teacher_vp {
        print!("  {faction} {:.2}", *points as f64 / *played as f64);
    }
    println!();

    // A fixed split: every fifth decision is held out, so train and test come from the same games
    // and the same positions rather than from different parts of a game.
    let mut by_head: BTreeMap<String, (Vec<usize>, Vec<usize>)> = BTreeMap::new();
    for (index, sample) in samples.iter().enumerate() {
        let entry = by_head.entry(sample.head.clone()).or_default();
        if index % 5 == 0 {
            entry.1.push(index);
        } else {
            entry.0.push(index);
        }
    }

    println!(
        "\n{} decisions over {} heads\n",
        samples.len(),
        by_head.len()
    );
    println!(
        "{:<14} {:>7} {:>7} {:>8} {:>9} {:>9} {:>9}   train curve by epoch",
        "head", "train", "test", "opts/dec", "chance", "TRAIN", "test"
    );

    let mut pooled = (0.0, 0.0, 0.0, 0usize);
    for (head, (train, test)) in &by_head {
        if train.len() < 50 {
            continue;
        }
        let mut weights: BTreeMap<FeatureKey, f64> = BTreeMap::new();
        let mut trace: Vec<f64> = Vec::new();
        for epoch in 0..epochs {
            // Decayed, so late epochs settle rather than oscillate.
            let step = learning_rate / (1.0 + epoch as f64).sqrt();
            for index in train {
                let sample = &samples[*index];
                let scores: Vec<f64> = sample
                    .options
                    .iter()
                    .map(|vector| score(&weights, vector))
                    .collect();
                let chances = probabilities(&scores);
                // The cross-entropy gradient is the policy gradient's own expression:
                // phi(chosen) - sum_o p_o phi_o.
                for (option, vector) in sample.options.iter().enumerate() {
                    let coefficient =
                        f64::from(u8::from(option == sample.chosen)) - chances[option];
                    if coefficient == 0.0 {
                        continue;
                    }
                    for (key, value) in vector {
                        *weights.entry(*key).or_insert(0.0) += step * coefficient * value;
                    }
                }
            }
            if epoch % 4 == 0 || epoch + 1 == epochs {
                let hit = train
                    .iter()
                    .filter(|index| agrees(&weights, &samples[**index]))
                    .count() as f64
                    / train.len() as f64;
                trace.push(hit);
            }
        }
        let hit = |set: &Vec<usize>| {
            set.iter()
                .filter(|index| agrees(&weights, &samples[**index]))
                .count() as f64
                / set.len().max(1) as f64
        };
        let chance: f64 = train
            .iter()
            .map(|index| 1.0 / samples[*index].options.len() as f64)
            .sum::<f64>()
            / train.len() as f64;
        let options: f64 = train
            .iter()
            .map(|index| samples[*index].options.len() as f64)
            .sum::<f64>()
            / train.len() as f64;
        let (train_hit, test_hit) = (hit(train), hit(test));
        let curve: Vec<String> = trace.iter().map(|value| format!("{value:.2}")).collect();
        println!(
            "{head:<14} {:>7} {:>7} {options:>8.1} {chance:>9.3} {train_hit:>9.3} {test_hit:>9.3}   {}",
            train.len(),
            test.len(),
            curve.join(" ")
        );
        pooled.0 += chance * train.len() as f64;
        pooled.1 += train_hit * train.len() as f64;
        pooled.2 += test_hit * test.len() as f64;
        pooled.3 += train.len();
    }
    let n = pooled.3 as f64;
    let tested: usize = by_head
        .values()
        .filter(|(train, _)| train.len() >= 50)
        .map(|(_, test)| test.len())
        .sum();
    println!(
        "\npooled: chance {:.3}  TRAIN {:.3}  test {:.3}",
        pooled.0 / n,
        pooled.1 / n,
        pooled.2 / tested.max(1) as f64
    );
    println!(
        "\nTRAIN is the answer. Near chance => the representation cannot express the teacher and\n\
         the plateau is the policy class. Well above chance => the class contains good play and\n\
         the optimiser is what failed to reach it."
    );
}
