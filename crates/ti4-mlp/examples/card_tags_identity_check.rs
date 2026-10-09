//! Prove that migrating a checkpoint to the card-text families changes nothing until it is trained.
//!
//! The migrated bundle's new rows are zero, so `card-tag:*` and `card-tag-opt:*` contribute
//! `0 * value` and every logit must equal the source bundle's. Two checks, nothing written:
//!
//! 1. **Logits.** Six-seat seeded games are played by the *source* bundle. At each decision the
//!    probe projects the legal set twice -- card tags off for the source, on for the migrated
//!    bundle (the hands taken from the bound seat, as the bot does) -- scores both, and records
//!    `max |Δlogit|` over the options. It also counts how many extra card-tag features the migrated
//!    projection carried, so a pass cannot come from the tags being absent.
//! 2. **Play.** The same seeded games are played again by the migrated bundle's own bot (which
//!    enables the families itself, from its vocabulary). Every offered option set and every choice
//!    must match the source's.
//!
//! ```text
//! cargo run --release -p ti4-mlp --example card_tags_identity_check -- \
//!   --source out/trade-teacher-wide-v8-20261008/checkpoint-20 \
//!   --migrated out/card-tags-migrated-20261009/checkpoint-20 \
//!   --seeds 3 --rounds 4 --stride 3
//! ```
//!
//! CPU only. Exits non-zero if any logit differs by more than `--tolerance` (default 1e-5), the
//! source bundle already enables the families, the migrated bundle does not, or the plays differ.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::Path;
use std::rc::Rc;

use ti4_content::ContentStore;
use ti4_engine::choice::{Choice, ChoiceOption, Decider, IllegalChoice, SeatObservation};
use ti4_mlp::{Actor, FactionRow, SparseOption};
use ti4_model::content_types::DEFAULT;
use ti4_model::id::{FactionId, PlayerId};
use ti4_policy::vocabulary::Vocabulary;
use ti4_training::rollout::{
    OpeningMap, SimulationCapabilities, seated_faction,
    setup_game_with_capabilities_and_decider_factory,
};

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

fn number<T: std::str::FromStr>(name: &str, default: T) -> T {
    argument(name).map_or(default, |value| {
        value
            .parse()
            .unwrap_or_else(|_| refuse(&format!("{name} expects a number, got {value}")))
    })
}

fn refuse(message: &str) -> ! {
    eprintln!("REFUSED: {message}");
    std::process::exit(2)
}

#[derive(Default)]
struct Stats {
    decisions: usize,
    options: usize,
    /// Decisions whose migrated projection carried at least one card-tag feature.
    with_tags: usize,
    /// `card-tag:*` features across all options (the seat family, repeated on every option).
    seat_features: usize,
    /// `card-tag-opt:*` features across all options, and the options that carried any.
    option_features: usize,
    options_with_card_tags: usize,
    /// Score one decision in this many (the others are only played).
    stride: usize,
    seen: usize,
    max_dlogit: f64,
    max_dprob: f64,
    limit: usize,
}

struct Models {
    source: (Rc<Actor>, Vocabulary),
    migrated: (Rc<Actor>, Vocabulary),
}

fn sparse(
    vector: &ti4_policy::features::FeatureVector,
    vocabulary: &Vocabulary,
) -> SparseOption {
    let mut columns = Vec::with_capacity(vector.len());
    let mut values = Vec::with_capacity(vector.len());
    for (key, value) in vector {
        columns.push(i64::try_from(vocabulary.column_of_key(*key)).unwrap_or(0));
        #[expect(clippy::cast_possible_truncation, reason = "features are f32-scale")]
        values.push(*value as f32);
    }
    SparseOption { columns, values }
}

fn logits(actor: &Actor, options: &[SparseOption], head: &str, row: FactionRow) -> Vec<f64> {
    let tensor = actor
        .logits(options, head, row)
        .unwrap_or_else(|error| refuse(&format!("logits: {error}")));
    let flat = tensor.reshape(&[-1]);
    (0..flat.size()[0])
        .map(|index| f64::from(flat.get(index).double_value(&[]) as f32))
        .collect()
}

/// The source bot decides; the probe compares what each bundle would have scored.
struct Probe {
    inner: Box<dyn Decider>,
    models: Rc<Models>,
    row: FactionRow,
    stats: Rc<RefCell<Stats>>,
}

impl Decider for Probe {
    fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
        self.inner.choose(choice)
    }

    fn choose_seeing(
        &mut self,
        choice: &Choice,
        seen: &SeatObservation<'_>,
    ) -> Result<ChoiceOption, IllegalChoice> {
        let (limit, scored, take) = {
            let mut stats = self.stats.borrow_mut();
            let take = choice.options.len() >= 2 && stats.seen % stats.stride.max(1) == 0;
            stats.seen += usize::from(choice.options.len() >= 2);
            (stats.limit, stats.decisions, take)
        };
        if scored < limit && take {
            let held = seen.held_secret_progress();
            let baseline = ti4_policy::progress::Baseline::default();
            let old = ti4_policy::projection::mlp_choice_features(
                seen.observed(),
                choice,
                &choice.player,
                &held,
                baseline,
            );
            let cards = seen.held_action_cards();
            let notes = seen.held_promissory_notes();
            let input = ti4_policy::projection::CardTagInput {
                held_action_cards: &cards,
                held_promissory_notes: &notes,
            };
            let new = ti4_policy::projection::mlp_choice_features_with(
                seen.observed(),
                choice,
                &choice.player,
                &held,
                baseline,
                Some(&input),
            );
            let old_sparse: Vec<SparseOption> = old
                .iter()
                .map(|vector| sparse(vector, &self.models.source.1))
                .collect();
            let new_sparse: Vec<SparseOption> = new
                .iter()
                .map(|vector| sparse(vector, &self.models.migrated.1))
                .collect();
            let requested = ti4_policy::learned::decision_head(choice);
            let head_old = self.models.source.0.resolve_layout_head(requested);
            let head_new = self.models.migrated.0.resolve_layout_head(requested);
            let a = logits(&self.models.source.0, &old_sparse, head_old, self.row);
            let b = logits(&self.models.migrated.0, &new_sparse, head_new, self.row);
            let pa = softmax(&a);
            let pb = softmax(&b);
            let (mut seat_features, mut option_features, mut with_opt) = (0, 0, 0);
            for vector in &new {
                let names = ti4_policy::features::names_of(vector);
                let opt = names.iter().filter(|n| n.starts_with("card-tag-opt:")).count();
                let seat = names.iter().filter(|n| n.starts_with("card-tag:")).count();
                seat_features += seat;
                option_features += opt;
                with_opt += usize::from(opt > 0);
            }
            let mut stats = self.stats.borrow_mut();
            stats.decisions += 1;
            stats.options += a.len();
            stats.seat_features += seat_features;
            stats.option_features += option_features;
            stats.options_with_card_tags += with_opt;
            stats.with_tags += usize::from(seat_features + option_features > 0);
            for ((x, y), (px, py)) in a.iter().zip(&b).zip(pa.iter().zip(&pb)) {
                stats.max_dlogit = stats.max_dlogit.max((x - y).abs());
                stats.max_dprob = stats.max_dprob.max((px - py).abs());
            }
        }
        self.inner.choose_seeing(choice, seen)
    }
}

fn softmax(logits: &[f64]) -> Vec<f64> {
    let top = logits.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let exp: Vec<f64> = logits.iter().map(|x| (x - top).exp()).collect();
    let sum: f64 = exp.iter().sum();
    exp.iter().map(|x| x / sum).collect()
}

type Trace = Vec<(String, Vec<String>, String)>;

#[expect(clippy::too_many_arguments, reason = "one call site, all required")]
fn play(
    content: &'static ContentStore,
    models: &Rc<Models>,
    use_migrated_bot: bool,
    stats: &Rc<RefCell<Stats>>,
    seed: u64,
    rounds: u32,
    max_steps: usize,
) -> Result<Trace, String> {
    let factions = FACTIONS.map(FactionId::new);
    let players: Vec<PlayerId> = (0..FACTIONS.len())
        .map(|index| PlayerId::new(format!("seat{index}")))
        .collect();
    let seated: BTreeMap<PlayerId, FactionId> = players
        .iter()
        .enumerate()
        .map(|(index, player)| (player.clone(), seated_faction(&factions, seed, 0, index)))
        .collect();
    let mut game = setup_game_with_capabilities_and_decider_factory(
        content,
        &players,
        &seated,
        DEFAULT,
        seed,
        &OpeningMap::RustVaried,
        SimulationCapabilities { diplomacy: false },
        |baselines| {
            let mut deciders: BTreeMap<PlayerId, Box<dyn Decider>> = BTreeMap::new();
            for (index, player) in players.iter().enumerate() {
                let row = FactionRow::of(seated[player].as_str())
                    .map_err(|error| format!("{player}: {error}"))?;
                let baseline = baselines
                    .get(player)
                    .copied()
                    .ok_or_else(|| format!("{player} has no baseline"))?;
                let stream = seed
                    .wrapping_mul(1_000_003)
                    .wrapping_add(u64::try_from(index).unwrap_or(0));
                let (actor, vocabulary) = if use_migrated_bot {
                    &models.migrated
                } else {
                    &models.source
                };
                let (bot, _status) =
                    ti4_mlp::bot::MlpBot::sharing(actor, vocabulary.clone(), row, stream)
                        .at_temperature(0.001)
                        .from_setup(baseline)
                        .seat();
                let decider: Box<dyn Decider> = if use_migrated_bot {
                    bot
                } else {
                    Box::new(Probe {
                        inner: bot,
                        models: Rc::clone(models),
                        row,
                        stats: Rc::clone(stats),
                    })
                };
                deciders.insert(player.clone(), decider);
            }
            Ok(deciders)
        },
    )?;
    let target = game.state.round + rounds;
    let mut steps = 0_usize;
    while game.state.round < target && !game.state.finished {
        let result = game.step();
        if let Some(error) = result.error {
            return Err(format!("seed {seed}, step {steps}: {error:?}"));
        }
        steps += 1;
        if steps >= max_steps {
            return Err(format!("seed {seed} did not finish within {max_steps} steps"));
        }
    }
    Ok(game
        .table
        .log
        .records
        .iter()
        .map(|record| {
            (
                record.player.to_string(),
                record.offered.clone(),
                record.chosen.clone(),
            )
        })
        .collect())
}

fn load(path: &Path) -> (Rc<Actor>, Vocabulary) {
    let loaded = ti4_mlp::bundle::read(path)
        .unwrap_or_else(|error| refuse(&format!("{}: {error}", path.display())));
    (Rc::new(loaded.actor), loaded.vocabulary)
}

fn main() {
    let source = argument("--source").unwrap_or_else(|| refuse("--source is required"));
    let migrated = argument("--migrated").unwrap_or_else(|| refuse("--migrated is required"));
    let seeds: u64 = number("--seeds", 2);
    let seed_base: u64 = number("--seed-base", 920_000_000);
    let rounds: u32 = number("--rounds", 4);
    let max_steps: usize = number("--max-steps", 200_000);
    let limit: usize = number("--score-limit", 100_000);
    let stride: usize = number("--stride", 3);
    let tolerance: f64 = number("--tolerance", 1e-5);
    ti4_tensor::configure_deterministic(20_261_009)
        .unwrap_or_else(|error| refuse(&format!("backend: {error}")));
    let content = ContentStore::embedded();
    let models = Rc::new(Models {
        source: load(Path::new(&source)),
        migrated: load(Path::new(&migrated)),
    });
    let source_on = ti4_policy::projection::card_tags_enabled_for(&models.source.1);
    let migrated_on = ti4_policy::projection::card_tags_enabled_for(&models.migrated.1);
    println!(
        "source: slots {} capacity {}, card tags enabled: {source_on}\nmigrated: slots {} capacity {}, card tags enabled: {migrated_on}",
        models.source.1.slot_count(),
        models.source.1.capacity(),
        models.migrated.1.slot_count(),
        models.migrated.1.capacity(),
    );
    let mut ok = !source_on && migrated_on;

    let mut identical = true;
    let mut total = Stats::default();
    for seed in seed_base..seed_base + seeds {
        let stats = Rc::new(RefCell::new(Stats {
            limit,
            stride,
            ..Stats::default()
        }));
        let before = play(content, &models, false, &stats, seed, rounds, max_steps)
            .unwrap_or_else(|error| refuse(&error));
        let after = play(
            content,
            &models,
            true,
            &Rc::new(RefCell::new(Stats::default())),
            seed,
            rounds,
            max_steps,
        )
        .unwrap_or_else(|error| refuse(&error));
        let stats = stats.borrow();
        let same = before == after;
        identical &= same;
        println!(
            "seed {seed}: {} decisions played, {} scored ({} options; {} decisions with card tags; {} seat + {} option tag features; {} options carry an option tag), max |dlogit| {:.3e}, max |dprob| {:.3e}, play {}",
            before.len(),
            stats.decisions,
            stats.options,
            stats.with_tags,
            stats.seat_features,
            stats.option_features,
            stats.options_with_card_tags,
            stats.max_dlogit,
            stats.max_dprob,
            if same { "IDENTICAL" } else { "DIFFERS" },
        );
        if !same {
            let at = before.iter().zip(&after).position(|(a, b)| a != b);
            println!("  first difference at decision {at:?}; lengths {} vs {}", before.len(), after.len());
        }
        total.decisions += stats.decisions;
        total.options += stats.options;
        total.with_tags += stats.with_tags;
        total.seat_features += stats.seat_features;
        total.option_features += stats.option_features;
        total.options_with_card_tags += stats.options_with_card_tags;
        total.max_dlogit = total.max_dlogit.max(stats.max_dlogit);
        total.max_dprob = total.max_dprob.max(stats.max_dprob);
    }
    println!(
        "\nTOTAL: {} decisions scored, {} options, {} decisions carrying card tags ({} seat + {} option tag features; {} options carry an option tag)\nmax |dlogit| = {:.3e} (tolerance {tolerance:e}), max |dprob| = {:.3e}, play {}",
        total.decisions,
        total.options,
        total.with_tags,
        total.seat_features,
        total.option_features,
        total.options_with_card_tags,
        total.max_dlogit,
        total.max_dprob,
        if identical { "IDENTICAL" } else { "DIFFERS" },
    );
    ok &= identical && total.max_dlogit <= tolerance && total.with_tags > 0 && total.decisions > 0;
    // Non-vacuity of both families.
    ok &= total.seat_features > 0 && total.option_features > 0;
    println!("RESULT: {}", if ok { "PASS" } else { "FAIL" });
    if !ok {
        std::process::exit(1);
    }
}
