//! Trade arena, battle-arena style: a planner labels negotiations, the diplomacy head learns them
//! by supervision.
//!
//! The PPO arena (`trade_arena`) learned by trial and error and kept collapsing: a deal good for
//! both sides takes about seven right steps in a row, so exploration almost never found one. Here
//! nothing is explored. Vehicle games played by the current policy supply real positions; at
//! contact points a negotiation is forced, and a **planner** proposes: it searches the items the
//! builder offers for the bundle (at most one item each way, up to three of a counted item; asks only
//! from what is public about the partner, as the bot's lookahead fact sees it) that
//! scores best for the proposer while scoring above zero for the partner under
//! `ti4_policy::deal_value`, and builds it step by step, or makes no offer when none exists. The
//! partner answers by the same sheet (accept exactly when it scores above zero). Every step is a
//! labelled example with the features the bot really sees. In half the contacts the proposer is
//! instead selfish (best for itself, whatever the partner gets) and unlabelled, so responders see
//! offers they should decline: with only good offers, every response label was "accept".
//!
//! Collection runs once; training then makes many passes, with only the diplomacy head's readout
//! and the deal-value fact rows trainable (the rest of the network is frozen, as in the arena).
//! A tenth of the games (by seed) is held out for accuracy.
//!
//! ```text
//! cargo run --release -p ti4-mlp --example trade_teacher -- --bundle <dir> --out <dir> \
//!     [--games 240] [--epochs 20] [--learning-rate 1e-3] [--minibatch 256] \
//!     [--temperature 1.0] [--seed-base 1265000000] [--device cuda] [--roster six|wide]
//!     [--collect-only]
//! ```

use rand::{Rng, SeedableRng};
use rayon::prelude::*;
use std::{
    cell::RefCell,
    collections::{BTreeMap, VecDeque},
    rc::Rc,
    sync::Arc,
    time::Instant,
};
use ti4_content::ContentStore;
use ti4_engine::choice::{Choice, ChoiceOption, Decider, IllegalChoice, SeatObservation};
use ti4_engine::diplomacy::builder::{CANCEL_ID, DONE_ID, Draft, PROPOSE_ID};
use ti4_engine::diplomacy::candidates::contact_seat_index;
use ti4_engine::diplomacy::window::{ACCEPT_ID, DECLINE_ID};
use ti4_mlp::bundle::CriticMode;
use ti4_model::{
    DealTerm, GameState, PlayerId, TransferAsset, content_types::DEFAULT, id::FactionId,
};
use ti4_policy::deal_value;

const FACTIONS: [&str; 6] = ["sol", "letnev", "xxcha", "hacan", "jolnar", "l1z1x"];
const BUILDING: [&str; 4] = [
    "diplomacy_offer_item",
    "diplomacy_ask_item",
    "diplomacy_amount",
    "diplomacy_review",
];
const RESPONSE: &str = "diplomacy_response";
const TILE_SEED_OFFSET: u64 = 20_000_000;
const ROUNDS: u32 = 4;
/// Share of contacts where the proposer is selfish (see [`Shared::label_proposer`]).
const SELFISH: f64 = 0.5;
/// Largest count of one counted item the planner considers.
const MOST: i32 = 3;

fn argument(name: &str) -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

fn refuse(reason: &str) -> ! {
    eprintln!("\nREFUSED: {reason}");
    std::process::exit(2);
}

fn parsed<T: std::str::FromStr>(name: &str, default: T) -> T {
    argument(name).map_or(default, |value| {
        value
            .parse()
            .unwrap_or_else(|_| refuse(&format!("{name}: '{value}' does not parse")))
    })
}

/// One labelled decision.
struct Example {
    row: ti4_mlp::FactionRow,
    options: Vec<ti4_mlp::SparseOption>,
    chosen: usize,
    kind: &'static str,
    held_out: bool,
}

#[derive(Default)]
struct Shared {
    seating: Vec<PlayerId>,
    /// The negotiation in progress: (proposer, recipient).
    open: Option<(PlayerId, PlayerId)>,
    /// Set by the seat that forced a contact; the game loop plans the deal after that step.
    opened: bool,
    /// The planner's option ids for the proposer, in order.
    plan: VecDeque<String>,
    /// Whether the proposer's steps are labels: not for a selfish offer, which exists only so the
    /// responder sees offers it should decline.
    label_proposer: bool,
    examples: Vec<Example>,
    mismatches: usize,
}

struct TeacherSeat {
    seat: PlayerId,
    vehicle: Box<dyn Decider>,
    /// Records the features the bot sees; its own pick is discarded for the label's.
    learner: Box<dyn Decider>,
    records: Rc<RefCell<Vec<ti4_mlp::bot::PpoRecord>>>,
    shared: Rc<RefCell<Shared>>,
    rng: rand_chacha::ChaCha8Rng,
    force: f64,
    held_out: bool,
}

fn subtype(choice: &Choice) -> &str {
    choice
        .context
        .as_ref()
        .map_or("", |context| context.subtype.as_str())
}

/// Accept exactly when the sheet scores the offer on the table above zero for this seat.
fn rule_response(choice: &Choice, seen: &SeatObservation<'_>) -> &'static str {
    let facts = deal_value::deal_facts(seen.observed(), choice);
    let worth_it = choice
        .options
        .iter()
        .zip(&facts)
        .find(|(option, _)| option.id == ACCEPT_ID)
        .is_some_and(|(_, facts)| {
            facts
                .iter()
                .any(|(name, value)| *name == deal_value::FACT_SCORE && *value > 0.0)
        });
    if worth_it { ACCEPT_ID } else { DECLINE_ID }
}

fn kind_of(id: &str) -> &'static str {
    match id {
        ACCEPT_ID => "accept",
        DECLINE_ID => "decline",
        CANCEL_ID => "no offer",
        DONE_ID => "done",
        PROPOSE_ID => "propose",
        _ if id.starts_with("diplomacy|amount|") => "amount",
        _ => "item",
    }
}

impl TeacherSeat {
    /// Label `wanted` on this choice: record the bot's view of it, then keep the label.
    fn label(
        &mut self,
        choice: &Choice,
        seen: &SeatObservation<'_>,
        wanted: &str,
    ) -> Result<ChoiceOption, IllegalChoice> {
        let before = self.records.borrow().len();
        let _ = self.learner.choose_seeing(choice, seen)?;
        let index = choice.options.iter().position(|option| option.id == wanted);
        let mut shared = self.shared.borrow_mut();
        let index = index.unwrap_or_else(|| {
            shared.mismatches += 1;
            choice
                .options
                .iter()
                .position(|option| option.id == CANCEL_ID || option.id == DECLINE_ID)
                .unwrap_or(0)
        });
        let mut records = self.records.borrow_mut();
        if records.len() == before + 1
            && let Some(record) = records.pop()
            && record.step.options.len() == choice.options.len()
        {
            shared.examples.push(Example {
                row: record.step.row,
                options: record.step.options,
                chosen: index,
                kind: kind_of(&choice.options[index].id),
                held_out: self.held_out,
            });
        }
        records.clear();
        Ok(choice.options[index].clone())
    }
}

impl Decider for TeacherSeat {
    fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
        // Every seat is asked through `choose_seeing`; this path only strips contacts.
        let mut stripped = choice.clone();
        stripped
            .options
            .retain(|option| contact_seat_index(&option.id).is_none());
        self.vehicle.choose(if stripped.options.is_empty() {
            choice
        } else {
            &stripped
        })
    }

    fn choose_seeing(
        &mut self,
        choice: &Choice,
        seen: &SeatObservation<'_>,
    ) -> Result<ChoiceOption, IllegalChoice> {
        let sub = subtype(choice);
        if TRACE.load(std::sync::atomic::Ordering::Relaxed) {
            eprintln!(
                "  trace {} {} | {} | {} options",
                self.seat,
                sub,
                choice.prompt,
                choice.options.len()
            );
        }
        let open = self.shared.borrow().open.clone();
        let negotiating = BUILDING.contains(&sub) || sub == RESPONSE;
        if negotiating && let Some((proposer, recipient)) = &open {
            if sub == RESPONSE && &self.seat == recipient {
                let wanted = rule_response(choice, seen);
                return self.label(choice, seen, wanted);
            }
            if BUILDING.contains(&sub) && &self.seat == proposer {
                let wanted = self
                    .shared
                    .borrow_mut()
                    .plan
                    .pop_front()
                    .unwrap_or_else(|| CANCEL_ID.to_owned());
                if !self.shared.borrow().label_proposer {
                    let option = choice.options.iter().find(|option| option.id == wanted);
                    return Ok(option
                        .or_else(|| choice.options.iter().find(|option| option.id == CANCEL_ID))
                        .unwrap_or(&choice.options[0])
                        .clone());
                }
                return self.label(choice, seen, &wanted);
            }
        }
        if !negotiating {
            self.shared.borrow_mut().open = None;
        }
        let contacts: Vec<&ChoiceOption> = choice
            .options
            .iter()
            .filter(|option| contact_seat_index(&option.id).is_some())
            .collect();
        if !negotiating
            && !contacts.is_empty()
            && self.shared.borrow().open.is_none()
            && self.rng.random::<f64>() < self.force
        {
            let pick = contacts[self.rng.random_range(0..contacts.len())];
            let recipient = contact_seat_index(&pick.id)
                .and_then(|index| self.shared.borrow().seating.get(index).cloned());
            if let Some(recipient) = recipient {
                let mut shared = self.shared.borrow_mut();
                shared.open = Some((self.seat.clone(), recipient));
                shared.opened = true;
                shared.plan.clear();
                return Ok(pick.clone());
            }
        }
        let mut stripped = choice.clone();
        stripped
            .options
            .retain(|option| contact_seat_index(&option.id).is_none());
        self.vehicle.choose_seeing(
            if stripped.options.is_empty() {
                choice
            } else {
                &stripped
            },
            seen,
        )
    }
}

/// How many of a counted item a seat holds, and the transfer of `n` of it.
fn counted(state: &GameState, holder: &PlayerId, id: &str, n: u8) -> Option<(i32, TransferAsset)> {
    let seat = state.player(holder)?;
    let fragment = |key: &str| seat.relic_fragments.get(key).copied().unwrap_or(0);
    Some(match id {
        "tg" => (seat.trade_goods, TransferAsset::TradeGoods(n)),
        "commodities" => (seat.commodities, TransferAsset::Commodities(n)),
        "cultural" => (fragment("CULTURAL"), TransferAsset::CulturalFragments(n)),
        "hazardous" => (fragment("HAZARDOUS"), TransferAsset::HazardousFragments(n)),
        "industrial" => (
            fragment("INDUSTRIAL"),
            TransferAsset::IndustrialFragments(n),
        ),
        "unknown" => (fragment("FRONTIER"), TransferAsset::UnknownFragments(n)),
        _ => return None,
    })
}

/// Each single item the builder offers on one side, as (option ids that add it, the term).
fn atoms(
    state: &GameState,
    content: &ContentStore,
    scope: &ti4_engine::diplomacy::builder::ContactScope,
    draft: &Draft,
    holder: &PlayerId,
) -> Vec<(Vec<String>, DealTerm)> {
    let mut out = Vec::new();
    for option in ti4_engine::diplomacy::builder::item_options(state, content, scope, draft) {
        let id = option.id.as_str();
        if let Some(kind) = id.strip_prefix("diplomacy|now|") {
            let held = counted(state, holder, kind, 1).map_or(0, |(held, _)| held);
            for n in 1..=held.min(MOST) {
                let n8 = u8::try_from(n).unwrap_or(1);
                if let Some((_, asset)) = counted(state, holder, kind, n8) {
                    out.push((
                        vec![id.to_owned(), format!("diplomacy|amount|{n}")],
                        DealTerm::ImmediateTransfer(asset),
                    ));
                }
            }
        } else if let Some(note) = id.strip_prefix("diplomacy|note|") {
            out.push((
                vec![id.to_owned()],
                DealTerm::ImmediateTransfer(TransferAsset::PromissoryNote(note.to_owned())),
            ));
        } else if let Some(card) = id.strip_prefix("diplomacy|card|") {
            out.push((
                vec![id.to_owned()],
                DealTerm::ImmediateTransfer(TransferAsset::ActionCard(
                    ti4_model::ActionCardId::new(card),
                )),
            ));
        }
    }
    out
}

/// The builder option ids that add one asked-for term.
fn ask_ids(term: &DealTerm) -> Option<Vec<String>> {
    let DealTerm::ImmediateTransfer(asset) = term else {
        return None;
    };
    let counted = |kind: &str, n: &u8| {
        Some(vec![
            format!("diplomacy|now|{kind}"),
            format!("diplomacy|amount|{n}"),
        ])
    };
    match asset {
        TransferAsset::TradeGoods(n) => counted("tg", n),
        TransferAsset::Commodities(n) => counted("commodities", n),
        TransferAsset::CulturalFragments(n) => counted("cultural", n),
        TransferAsset::HazardousFragments(n) => counted("hazardous", n),
        TransferAsset::IndustrialFragments(n) => counted("industrial", n),
        TransferAsset::UnknownFragments(n) => counted("unknown", n),
        TransferAsset::PromissoryNote(note) => Some(vec![format!("diplomacy|note|{note}")]),
        _ => None,
    }
}

/// The planner's deal, as the proposer's option ids; one "no offer" when nothing suits both.
fn plan(
    selfish: bool,
    state: &GameState,
    content: &ContentStore,
    galaxy: &ti4_content::galaxy::Galaxy,
    proposer: &PlayerId,
    recipient: &PlayerId,
) -> (VecDeque<String>, Option<(Vec<DealTerm>, Vec<DealTerm>)>) {
    let physical =
        ti4_engine::transactions::may_transact(state, content, galaxy, proposer, recipient);
    let context = ti4_engine::diplomacy::candidates::CandidateContext {
        state,
        content,
        galaxy,
        proposer,
        recipient,
        agenda: None,
    };
    let scope = ti4_engine::diplomacy::candidates::contact_scope(&context, physical);
    let offering = Draft::new(proposer.clone(), recipient.clone());
    let mut asking = offering.clone();
    asking.asking = true;
    let gives = atoms(state, content, &scope, &offering, proposer);
    // Asks come from what is public, exactly as the bot's lookahead fact sees them, and only those
    // the builder offers. A partner's other notes are hidden, so the planner does not ask for them.
    let position = deal_value::Position::of_state(state, content);
    let offered: std::collections::BTreeSet<String> =
        ti4_engine::diplomacy::builder::item_options(state, content, &scope, &asking)
            .into_iter()
            .map(|option| option.id)
            .collect();
    let takes: Vec<(Vec<String>, DealTerm)> = deal_value::public_asks(&position, recipient)
        .into_iter()
        .filter_map(|term| {
            let ids = ask_ids(&term)?;
            offered.contains(&ids[0]).then_some((ids, term))
        })
        .collect();
    // Each side contributes nothing or one item; both empty is not a deal.
    let choices = |atoms: Vec<(Vec<String>, DealTerm)>| {
        std::iter::once((Vec::new(), Vec::new()))
            .chain(atoms.into_iter().map(|(ids, term)| (ids, vec![term])))
            .collect::<Vec<(Vec<String>, Vec<DealTerm>)>>()
    };
    let (gives, takes) = (choices(gives), choices(takes));
    let mut best: Option<(f64, Vec<String>, Vec<DealTerm>, Vec<DealTerm>)> = None;
    for (give_ids, give) in &gives {
        for (take_ids, take) in &takes {
            if give.is_empty() && take.is_empty() {
                continue;
            }
            let score = deal_value::score(&position, proposer, recipient, give, take);
            // A selfish proposer ignores whether the partner gains; responders learn to decline it.
            if (!selfish && score.recipient_score <= 0.0) || score.proposer_score <= 0.0 {
                continue;
            }
            if best
                .as_ref()
                .is_none_or(|(value, ..)| score.proposer_score > *value)
            {
                let mut ids = give_ids.clone();
                ids.push(DONE_ID.to_owned());
                ids.extend(take_ids.iter().cloned());
                ids.push(DONE_ID.to_owned());
                ids.push(PROPOSE_ID.to_owned());
                best = Some((score.proposer_score, ids, give.clone(), take.clone()));
            }
        }
    }
    match best {
        Some((_, ids, give, take)) => (ids.into(), Some((give, take))),
        None => (VecDeque::from([CANCEL_ID.to_owned()]), None),
    }
}

#[derive(Default)]
struct Collected {
    examples: Vec<Example>,
    contacts: usize,
    planned: usize,
    mismatches: usize,
    items: BTreeMap<&'static str, usize>,
}

fn item_kind(term: &DealTerm) -> &'static str {
    match term {
        DealTerm::ImmediateTransfer(TransferAsset::TradeGoods(_)) => "trade goods",
        DealTerm::ImmediateTransfer(TransferAsset::Commodities(_)) => "commodities",
        DealTerm::ImmediateTransfer(TransferAsset::PromissoryNote(note))
            if note.starts_with(ti4_engine::promissory::SUPPORT_PREFIX) =>
        {
            "support"
        }
        DealTerm::ImmediateTransfer(TransferAsset::PromissoryNote(_)) => "other notes",
        DealTerm::ImmediateTransfer(TransferAsset::ActionCard(_)) => "action cards",
        DealTerm::ImmediateTransfer(_) => "fragments",
        _ => "promises",
    }
}

fn collect_game(
    actor: &Rc<ti4_mlp::Actor>,
    vocabulary: &ti4_policy::vocabulary::Vocabulary,
    pool: &Arc<ti4_sim::MapPool>,
    content: &ContentStore,
    seed: u64,
    temperature: f64,
    roster: ti4_engine::seating::FactionRoster,
) -> Result<Collected, String> {
    let players: Vec<PlayerId> = (0..6).map(|i| PlayerId::new(format!("seat{i}"))).collect();
    // `--roster wide` seats six distinct factions per seed from the whole roster (BF-22).
    let drawn =
        ti4_training::rollout::game_factions(content, roster, &FACTIONS, &players, DEFAULT, seed)?;
    let factions: BTreeMap<PlayerId, FactionId> = players
        .iter()
        .enumerate()
        .map(|(i, p)| {
            (
                p.clone(),
                ti4_training::rollout::seated_faction(&drawn, seed, 0, i),
            )
        })
        .collect();
    let shared = Rc::new(RefCell::new(Shared::default()));
    let held_out = seed % 10 == 0;
    let mut statuses = Vec::new();
    let mut game = ti4_training::rollout::setup_game_with_capabilities_and_decider_factory(
        content,
        &players,
        &factions,
        DEFAULT,
        seed,
        &ti4_training::rollout::OpeningMap::PythonPool {
            pool: Arc::clone(pool),
            tile_seed_offset: TILE_SEED_OFFSET,
        },
        ti4_training::rollout::SimulationCapabilities { diplomacy: true },
        |baselines| {
            let mut deciders: BTreeMap<PlayerId, Box<dyn Decider>> = BTreeMap::new();
            for (index, player) in players.iter().enumerate() {
                let row = ti4_mlp::FactionRow::of(factions[player].as_str())
                    .map_err(|error| format!("{player}: {error}"))?;
                let baseline = baselines[player];
                let stream = seed
                    .wrapping_mul(1_000_003)
                    .wrapping_add(u64::try_from(index).unwrap_or(0));
                let (vehicle, status) =
                    ti4_mlp::bot::MlpBot::sharing(actor, vocabulary.clone(), row, stream)
                        .at_temperature(temperature)
                        .from_setup(baseline)
                        .seat();
                statuses.push(status);
                let learner =
                    ti4_mlp::bot::MlpBot::sharing(actor, vocabulary.clone(), row, !stream)
                        .at_temperature(1.0)
                        .recording_ppo(CriticMode::BatchMean)
                        .from_setup(baseline);
                let records = learner.ppo_records();
                let (learner, status) = learner.seat();
                statuses.push(status);
                deciders.insert(
                    player.clone(),
                    Box::new(TeacherSeat {
                        seat: player.clone(),
                        vehicle,
                        learner,
                        records,
                        shared: Rc::clone(&shared),
                        rng: rand_chacha::ChaCha8Rng::seed_from_u64(stream ^ 0x7EAC),
                        force: 0.5,
                        held_out,
                    }),
                );
            }
            Ok(deciders)
        },
    )?;
    shared.borrow_mut().seating = game.state.seating_order.clone();
    let mut out = Collected::default();
    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(seed ^ 0x5E1F);
    let target = game.state.round + ROUNDS;
    let mut steps = 0usize;
    while !game.state.finished && game.state.round < target && steps < 400_000 {
        let result = game.step();
        if let Some(error) = result.error {
            return Err(format!("seed {seed}: {error:?}"));
        }
        steps += 1;
        let opened = std::mem::take(&mut shared.borrow_mut().opened);
        if opened {
            let open = shared.borrow().open.clone();
            if let (Some((proposer, recipient)), Some(galaxy)) = (open, game.galaxy()) {
                let selfish = rng.random::<f64>() < SELFISH;
                let (ids, deal) =
                    plan(selfish, &game.state, content, galaxy, &proposer, &recipient);
                shared.borrow_mut().label_proposer = !selfish;
                out.contacts += 1;
                if let Some((give, take)) = &deal {
                    out.planned += 1;
                    for term in give.iter().chain(take) {
                        *out.items.entry(item_kind(term)).or_default() += 1;
                    }
                }
                shared.borrow_mut().plan = ids;
            }
        }
    }
    for status in statuses {
        status.into_result().map_err(|error| error.to_string())?;
    }
    let mut shared = shared.borrow_mut();
    out.examples = std::mem::take(&mut shared.examples);
    out.mismatches = shared.mismatches;
    Ok(out)
}

/// `--trace`: log every decision a seat is asked through `choose_seeing` (for one stuck seed).
static TRACE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Mean cross-entropy of the labelled options over `batch`, and how many labels were ranked first.
fn loss_and_hits(
    actor: &ti4_mlp::Actor,
    batch: &[&Example],
    head: i64,
) -> Result<(ti4_tensor::Tensor, Vec<bool>), String> {
    let mut options = Vec::new();
    let mut heads = Vec::new();
    let mut rows = Vec::new();
    let mut segments = Vec::new();
    for example in batch {
        let start = i64::try_from(options.len()).map_err(|e| e.to_string())?;
        let len = i64::try_from(example.options.len()).map_err(|e| e.to_string())?;
        options.extend(example.options.iter().cloned());
        heads.extend(std::iter::repeat_n(head, example.options.len()));
        let row = i64::try_from(example.row.index()).map_err(|e| e.to_string())?;
        rows.extend(std::iter::repeat_n(row, example.options.len()));
        segments.push((
            start,
            len,
            i64::try_from(example.chosen).map_err(|e| e.to_string())?,
        ));
    }
    let logits = actor
        .logits_mixed(&options, &heads, &rows)
        .map_err(|error| error.to_string())?;
    let mut total: Option<ti4_tensor::Tensor> = None;
    let mut hits = Vec::with_capacity(segments.len());
    for (start, len, chosen) in segments {
        let segment = logits.narrow(0, start, len);
        let term = segment.logsumexp([0_i64].as_slice(), false) - segment.get(chosen);
        hits.push(segment.argmax(0, false).int64_value(&[]) == chosen);
        total = Some(match total {
            Some(sum) => sum + term,
            None => term,
        });
    }
    #[expect(clippy::cast_precision_loss, reason = "batch sizes are small")]
    let count = batch.len().max(1) as f64;
    Ok((total.ok_or("an empty batch")? / count, hits))
}

#[expect(
    clippy::too_many_lines,
    reason = "collection, then training, read top to bottom"
)]
fn main() {
    ti4_tensor::configure_deterministic(20_260_923).unwrap_or_else(|e| refuse(&e.to_string()));
    let bundle_path = argument("--bundle").unwrap_or_else(|| refuse("--bundle is required"));
    let out = argument("--out").unwrap_or_else(|| refuse("--out is required"));
    let games: u64 = parsed("--games", 240);
    let epochs: usize = parsed("--epochs", 20);
    let learning_rate: f64 = parsed("--learning-rate", 1e-3);
    let minibatch: usize = parsed("--minibatch", 256);
    let temperature: f64 = parsed("--temperature", 1.0);
    let seed_base: u64 = parsed("--seed-base", 1_265_000_000);
    // Collect and report only: for finding slow or stuck games without writing a checkpoint.
    let collect_only = std::env::args().any(|arg| arg == "--collect-only");
    if std::env::args().any(|arg| arg == "--trace") {
        TRACE.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    let roster = argument("--roster")
        .map_or(ti4_engine::seating::FactionRoster::InScope, |value| {
            ti4_training::rollout::parse_roster(&value).unwrap_or_else(|error| refuse(&error))
        });
    let device = match argument("--device").as_deref().unwrap_or("cuda") {
        "cuda" => ti4_tensor::Device::Cuda(0),
        "cpu" => ti4_tensor::Device::Cpu,
        other => refuse(&format!("--device {other}: expected cuda or cpu")),
    };

    let loaded = ti4_mlp::bundle::read(std::path::Path::new(&bundle_path))
        .unwrap_or_else(|error| refuse(&format!("reading {bundle_path}: {error}")));
    let vocabulary = loaded.vocabulary;
    let bundle_mode = loaded.critic_mode;
    let mut actor = loaded.actor;
    let slots_text = std::fs::read_to_string(std::path::Path::new(&bundle_path).join("slots.json"))
        .unwrap_or_else(|error| refuse(&format!("reading slots.json: {error}")));
    let rows: Vec<i64> = deal_value::FACT_NAMES
        .iter()
        .map(|name| {
            if !vocabulary.is_assigned(name) {
                refuse(&format!(
                    "the vocabulary does not place {name}; migrate it first"
                ));
            }
            i64::try_from(vocabulary.column_of(name)).unwrap_or_default()
        })
        .collect();
    let head = i64::try_from(
        actor
            .layout_head_index("diplomacy")
            .unwrap_or_else(|error| refuse(&format!("diplomacy head: {error}"))),
    )
    .unwrap_or_default();
    let pool_path =
        argument("--map-pool").unwrap_or_else(|| "out/pools/full_np8_12_train.json".to_owned());
    let pool_bytes = ti4_sim::artifacts::read_and_verify_pool_role(
        std::path::Path::new(&pool_path),
        &[ti4_sim::artifacts::ArtifactRole::Train],
    )
    .unwrap_or_else(|error| refuse(&format!("{pool_path}: {error}")));
    let pool = Arc::new(
        ti4_sim::MapPool::from_reader(std::io::Cursor::new(&pool_bytes))
            .unwrap_or_else(|error| refuse(&format!("{pool_path}: {error}"))),
    );
    let content = ContentStore::embedded();

    // ---- collection ----
    let started = Instant::now();
    let inference = actor.inference_copy().to_device(ti4_tensor::Device::Cpu);
    let workers = rayon::current_num_threads().max(1);
    let seeds: Vec<u64> = (seed_base..seed_base + games).collect();
    let jobs: Vec<(ti4_mlp::Actor, Vec<u64>)> = seeds
        .chunks(seeds.len().div_ceil(workers).max(1))
        .map(|seeds| (inference.inference_copy(), seeds.to_vec()))
        .collect();
    let collected: Vec<Collected> = jobs
        .into_par_iter()
        .flat_map_iter(|(copy, seeds)| {
            let copy = Rc::new(copy);
            seeds
                .into_iter()
                .map(|seed| {
                    let clock = Instant::now();
                    let game = collect_game(
                        &copy,
                        &vocabulary,
                        &pool,
                        content,
                        seed,
                        temperature,
                        roster,
                    )
                    .unwrap_or_else(|error| refuse(&error));
                    // Progress per game, so a long collection shows how far it is, and a game that
                    // never ends shows which seed it is.
                    eprintln!(
                        "  game {seed} in {:.1}s: {} contacts, {} examples",
                        clock.elapsed().as_secs_f64(),
                        game.contacts,
                        game.examples.len()
                    );
                    game
                })
                .collect::<Vec<_>>()
        })
        .collect();
    let mut examples = Vec::new();
    let (mut contacts, mut planned, mut mismatches) = (0, 0, 0);
    let mut items: BTreeMap<&str, usize> = BTreeMap::new();
    for game in collected {
        contacts += game.contacts;
        planned += game.planned;
        mismatches += game.mismatches;
        for (kind, count) in game.items {
            *items.entry(kind).or_default() += count;
        }
        examples.extend(game.examples);
    }
    let mut kinds: BTreeMap<&str, usize> = BTreeMap::new();
    for example in &examples {
        *kinds.entry(example.kind).or_default() += 1;
    }
    #[expect(clippy::cast_precision_loss, reason = "counts")]
    let share = planned as f64 / contacts.max(1) as f64 * 100.0;
    println!("trade teacher");
    println!("  bundle      {bundle_path}");
    println!(
        "  collected   {games} games in {:.1?}: {contacts} contacts, a deal planned in {share:.0}%, {mismatches} label mismatches",
        started.elapsed()
    );
    println!("  planned     items: {items:?}");
    println!(
        "  examples    {} ({} held out) by label: {kinds:?}",
        examples.len(),
        examples.iter().filter(|e| e.held_out).count()
    );

    if collect_only {
        println!("  --collect-only: no training, no checkpoint");
        return;
    }

    // ---- supervised training ----
    let settings = ti4_mlp::ppo::Settings {
        learning_rate,
        weight_decay: 0.0,
        ..ti4_mlp::ppo::Settings::default()
    };
    actor = actor.to_device(device);
    let mut optimizer = ti4_mlp::ppo::Adam::new(&mut actor, CriticMode::BatchMean, settings)
        .unwrap_or_else(|error| refuse(&format!("opening Adam: {error}")));
    optimizer
        .restrict(actor.head_and_rows_masks(head, &rows))
        .unwrap_or_else(|error| refuse(&error));
    let train: Vec<&Example> = examples.iter().filter(|e| !e.held_out).collect();
    let test: Vec<&Example> = examples.iter().filter(|e| e.held_out).collect();
    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(seed_base);
    let evaluate = |actor: &ti4_mlp::Actor| -> (f64, BTreeMap<&'static str, (usize, usize)>) {
        let mut by_kind: BTreeMap<&'static str, (usize, usize)> = BTreeMap::new();
        let mut loss = 0.0;
        let mut batches = 0;
        tch::no_grad(|| {
            for chunk in test.chunks(1024) {
                let (value, hits) =
                    loss_and_hits(actor, chunk, head).unwrap_or_else(|error| refuse(&error));
                loss += value.double_value(&[]);
                batches += 1;
                for (example, hit) in chunk.iter().zip(hits) {
                    let entry = by_kind.entry(example.kind).or_default();
                    entry.1 += 1;
                    if hit {
                        entry.0 += 1;
                    }
                }
            }
        });
        (loss / f64::from(batches.max(1)), by_kind)
    };
    let show = |label: &str, (loss, by_kind): (f64, BTreeMap<&'static str, (usize, usize)>)| {
        let (hit, all) = by_kind
            .values()
            .fold((0, 0), |(h, a), (hit, all)| (h + hit, a + all));
        #[expect(clippy::cast_precision_loss, reason = "counts")]
        let rate = |hit: usize, all: usize| hit as f64 / all.max(1) as f64 * 100.0;
        println!(
            "  {label:<10} held-out loss {loss:.4}  accuracy {:.1}%  | {}",
            rate(hit, all),
            by_kind
                .iter()
                .map(|(kind, (hit, all))| format!("{kind} {:.0}%", rate(*hit, *all)))
                .collect::<Vec<_>>()
                .join("  ")
        );
    };
    show("start", evaluate(&actor));
    for epoch in 0..epochs {
        let epoch_started = Instant::now();
        let mut order: Vec<usize> = (0..train.len()).collect();
        for index in (1..order.len()).rev() {
            order.swap(index, rng.random_range(0..=index));
        }
        let mut total = 0.0;
        let mut steps = 0;
        for chunk in order.chunks(minibatch) {
            let batch: Vec<&Example> = chunk.iter().map(|&index| train[index]).collect();
            let (loss, _) =
                loss_and_hits(&actor, &batch, head).unwrap_or_else(|error| refuse(&error));
            optimizer
                .zero_grad(&actor)
                .unwrap_or_else(|error| refuse(&error));
            loss.backward();
            optimizer
                .step(&actor)
                .unwrap_or_else(|error| refuse(&error));
            total += loss.double_value(&[]);
            steps += 1;
        }
        println!(
            "  epoch {epoch:>3}  train loss {:.4}  ({:.1?})",
            total / f64::from(steps.max(1)),
            epoch_started.elapsed()
        );
        if epoch % 5 == 4 || epoch + 1 == epochs {
            show("", evaluate(&actor));
        }
    }

    let destination = std::path::Path::new(&out).join(format!("checkpoint-{epochs}"));
    let cpu = actor.inference_copy().to_device(ti4_tensor::Device::Cpu);
    let written = ti4_mlp::bundle::write(
        &destination,
        &cpu,
        &slots_text,
        bundle_mode,
        &ti4_mlp::bundle::Provenance {
            source: format!(
                "trade teacher, {epochs} epoch(s) from {bundle_path}, roster {roster:?}"
            ),
            git_commit: std::env::var("GIT_COMMIT").unwrap_or_else(|_| "unrecorded".to_owned()),
            update: epochs as u64,
        },
    )
    .unwrap_or_else(|error| refuse(&format!("writing the checkpoint: {error}")));
    ti4_mlp::bundle::read(&written.directory)
        .unwrap_or_else(|error| refuse(&format!("the checkpoint does not load: {error}")));
    println!("  checkpoint  {}", written.directory.display());
}
