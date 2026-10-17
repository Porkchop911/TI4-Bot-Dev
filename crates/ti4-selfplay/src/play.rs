//! Playing one game with the rule bot on every seat, and what is measured on the way.
//!
//! The game is driven exactly as the browser host drives a rule seat (`ti4_wasm` `Seat::decide`
//! and `draw`): one decider answers for every seat, a question with the position in hand goes to
//! [`ti4_rule_bot::choose`] with the game seed, the seat and the number of the decision (counted
//! over all seats), and a question without goes to the shared seeded random stream. Same seed and
//! table give the same game, on any thread.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use ti4_engine::choice::{
    Choice, ChoiceOption, Decider, IllegalChoice, SeatObservation, SeededRandom,
};
use ti4_rule_bot::tuning::{self, Tuning};

use crate::spec::{GameSpec, Limits, SEATS};

/// Per kind of question: how often asked, how often the rule bot had an opinion, bot time.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct TypeStat {
    pub n: u64,
    /// Answered by the rule bot's own rule rather than the random pick.
    pub smart: u64,
    /// Nanoseconds spent inside the decider.
    pub ns: u64,
}

/// One line of a decision trace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceLine {
    pub i: usize,
    pub seat: String,
    pub round: u32,
    pub phase: String,
    pub subtype: String,
    pub prompt: String,
    pub chosen: String,
    pub offered: usize,
    /// "rule" (the bot's own rule chose) or "random" (no opinion, or no position to read).
    pub by: &'static str,
}

/// What came of a game.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameResult {
    pub spec: GameSpec,
    /// `victory`, `cutoff_rounds`, `cutoff_steps`, `stuck`, `engine_error`, `panic`, `setup_error`.
    pub termination: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// Seat index of the winner (`victory` only).
    pub winner: Option<usize>,
    pub winner_faction: Option<String>,
    /// Victory points per seat at the end.
    pub vp: Vec<i32>,
    pub rounds: u32,
    pub decisions: usize,
    pub steps: usize,
    pub wall_ms: f64,
    /// Time inside the bot's decisions.
    pub bot_ms: f64,
    /// Wall time of the engine steps by phase the step began in (the bot's time included).
    pub phase_ms: BTreeMap<String, f64>,
    /// Hash of the decision log (the one a save of the browser host carries).
    pub hash: String,
    pub by_subtype: BTreeMap<String, TypeStat>,
    /// How often each notable option was taken: `subtype=option`.
    pub usage: BTreeMap<String, u64>,
    /// Objectives scored per seat.
    pub scored: Vec<Vec<String>>,
    /// Victory points by source (the engine's ledger), summed over seats.
    pub vp_sources: BTreeMap<String, i32>,
    /// Units at the end, summed over seats: `ground` (infantry and mechs), `home_ground` (in the
    /// seat's home system), `ships` (non-fighter ships). A diagnostic (ground forces piling up).
    #[serde(default)]
    pub end_counts: BTreeMap<String, i32>,
    /// Victory points of every seat at the end of each round played before the last one
    /// (`vp_by_round[r - 1]` after round `r`); the final points are `vp`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub vp_by_round: Vec<Vec<i32>>,
    /// The per-round record of the game ([`crate::timeline`]), when it was recorded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeline: Option<serde_json::Value>,
    /// A/B: the seats that played the candidate bot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub candidate_seats: Option<Vec<bool>>,
}

/// How to play.
#[derive(Debug, Clone)]
pub struct PlayOptions {
    pub limits: Limits,
    /// The bot's numbers per seat (`len == players`); a shorter list repeats its last entry.
    pub tunings: Vec<Tuning>,
    pub candidate_seats: Option<Vec<bool>>,
    /// Keep every decision for a text trace.
    pub trace: bool,
    /// Record the per-round timeline of the game ([`crate::timeline`]).
    pub timeline: bool,
}

impl Default for PlayOptions {
    fn default() -> Self {
        Self {
            limits: Limits::default(),
            tunings: vec![Tuning::default()],
            candidate_seats: None,
            trace: false,
            timeline: true,
        }
    }
}

struct Shared {
    decided: usize,
    seed: u64,
    random: SeededRandom,
    tunings: Vec<Tuning>,
    by_subtype: BTreeMap<String, TypeStat>,
    usage: BTreeMap<String, u64>,
    bot_ns: u128,
    /// Whether the rule decided, per decision number (trace only).
    by_rule: Option<Vec<bool>>,
    last: (u64, usize),
    repeats: usize,
    loop_steps: usize,
    looped: Option<String>,
}

struct SelfPlay(Rc<RefCell<Shared>>);

fn seat_index(choice: &Choice) -> usize {
    SEATS
        .iter()
        .position(|s| *s == choice.player.as_str())
        .unwrap_or(0)
}

fn subtype_of(choice: &Choice) -> String {
    choice.context.as_ref().map_or_else(
        || {
            format!(
                "~{}",
                choice.options.first().map_or("none", |o| o.kind.as_str())
            )
        },
        |c| c.subtype.clone(),
    )
}

/// The options worth counting by name: strategy cards, strategic actions, technologies,
/// objectives, agenda placements, reactions (action cards and abilities played). Anything with
/// an open-ended id (a system, a planet, a unit placement) is counted by question only.
fn usage_key(subtype: &str, option: &ChoiceOption) -> Option<String> {
    const NAMED: &[&str] = &[
        "action_menu",
        "draft_strategy_card",
        "starting_technology",
        "research_technology",
        "score_objective",
        "score_secret_objective",
        "imperial_score_objective",
        "politics_place_agenda",
        "plagiarize_pick_technology",
        "warfare_free_tactical",
        "construction_choose_ability",
    ];
    if NAMED.contains(&subtype) {
        return Some(format!("{subtype}={}", option.id));
    }
    if (subtype.starts_with("reaction_") || subtype.starts_with("play_reaction"))
        && !option.is_decline()
        && option.id.len() <= 48
    {
        return Some(format!("reaction={}", option.id));
    }
    None
}

impl Decider for SelfPlay {
    fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
        self.decide(choice, None)
    }

    fn choose_seeing(
        &mut self,
        choice: &Choice,
        seen: &SeatObservation<'_>,
    ) -> Result<ChoiceOption, IllegalChoice> {
        self.decide(choice, Some(seen))
    }
}

impl SelfPlay {
    fn decide(
        &mut self,
        choice: &Choice,
        seen: Option<&SeatObservation<'_>>,
    ) -> Result<ChoiceOption, IllegalChoice> {
        let started = Instant::now();
        let mut sh = self.0.borrow_mut();
        let index = sh.decided;
        let (option, smart) = match seen {
            Some(seen) if !choice.options.is_empty() => {
                let seat = seat_index(choice);
                let t = sh
                    .tunings
                    .get(seat)
                    .or(sh.tunings.last())
                    .copied()
                    .unwrap_or_default();
                tuning::set(t);
                let (picked, smart, _) =
                    ti4_rule_bot::choose_explained(choice, Some(seen), sh.seed, seat, index);
                (choice.options[picked].clone(), smart)
            }
            _ => (sh.random.choose(choice)?, false),
        };
        sh.decided += 1;
        let ns = started.elapsed().as_nanos();
        sh.bot_ns += ns;
        let subtype = subtype_of(choice);
        if let Some(rule) = sh.by_rule.as_mut() {
            rule.push(smart);
        }
        let stat = if let Some(stat) = sh.by_subtype.get_mut(&subtype) {
            stat
        } else {
            sh.by_subtype.entry(subtype.clone()).or_default()
        };
        stat.n += 1;
        stat.smart += u64::from(smart);
        stat.ns += u64::try_from(ns).unwrap_or(u64::MAX);
        if let Some(key) = usage_key(&subtype, &option) {
            *sh.usage.entry(key).or_default() += 1;
        }
        // The same question, from the same seat, answered the same way, over and over.
        let mut hash = 0xcbf2_9ce4_8422_2325_u64;
        for b in choice.prompt.bytes().chain(option.id.bytes()) {
            hash = (hash ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3);
        }
        let key = (hash, seat_index(choice));
        if sh.last == key {
            sh.repeats += 1;
            if sh.repeats >= sh.loop_steps && sh.looped.is_none() {
                sh.looped = Some(format!(
                    "{} x{}: {:?} -> {}",
                    choice.player, sh.repeats, choice.prompt, option.id
                ));
            }
        } else {
            sh.last = key;
            sh.repeats = 0;
        }
        Ok(option)
    }
}

thread_local! {
    static PANIC_TEXT: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// Silence the default panic output and keep the message for the result of the game that panicked.
pub fn install_panic_capture() {
    std::panic::set_hook(Box::new(|info| {
        let place = info
            .location()
            .map(|l| format!(" at {}:{}", l.file(), l.line()))
            .unwrap_or_default();
        let text = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| (*s).to_owned())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_default();
        PANIC_TEXT.with(|t| *t.borrow_mut() = Some(format!("{text}{place}")));
    }));
}

/// Play `spec`; a panic is caught and reported as a result.
pub fn play_guarded(
    spec: &GameSpec,
    options: &PlayOptions,
) -> (GameResult, Option<Vec<TraceLine>>) {
    let started = Instant::now();
    match catch_unwind(AssertUnwindSafe(|| play(spec, options))) {
        Ok(done) => done,
        Err(_) => {
            let text = PANIC_TEXT
                .with(|t| t.borrow_mut().take())
                .unwrap_or_default();
            (
                empty_result(spec, "panic", Some(text), started, options),
                None,
            )
        }
    }
}

/// A result with nothing in it, for tests of code that reads results.
#[cfg(test)]
pub(crate) fn empty_for_tests() -> GameResult {
    let spec = GameSpec {
        setup: "t".to_owned(),
        seed: 1,
        players: 2,
        map: "random".to_owned(),
        factions: vec!["sol".to_owned(), "hacan".to_owned()],
        speaker: "seat".to_owned(),
        card_set: None,
        rotation: None,
        starting_tech: false,
    };
    empty_result(
        &spec,
        "victory",
        None,
        Instant::now(),
        &PlayOptions::default(),
    )
}

fn empty_result(
    spec: &GameSpec,
    termination: &str,
    detail: Option<String>,
    started: Instant,
    options: &PlayOptions,
) -> GameResult {
    GameResult {
        spec: spec.clone(),
        termination: termination.to_owned(),
        detail,
        winner: None,
        winner_faction: None,
        vp: vec![0; spec.players],
        rounds: 0,
        decisions: 0,
        steps: 0,
        wall_ms: started.elapsed().as_secs_f64() * 1e3,
        bot_ms: 0.0,
        phase_ms: BTreeMap::new(),
        hash: String::new(),
        by_subtype: BTreeMap::new(),
        usage: BTreeMap::new(),
        scored: vec![Vec::new(); spec.players],
        vp_sources: BTreeMap::new(),
        end_counts: BTreeMap::new(),
        vp_by_round: Vec::new(),
        timeline: None,
        candidate_seats: options.candidate_seats.clone(),
    }
}

/// Normalises the engine's ledger reasons (they carry ids) to a source name.
fn source_of(reason: &str) -> String {
    let head = reason
        .split([':', '(', '#'])
        .next()
        .unwrap_or(reason)
        .trim();
    head.chars().take(40).collect()
}

fn play(spec: &GameSpec, options: &PlayOptions) -> (GameResult, Option<Vec<TraceLine>>) {
    let started = Instant::now();
    let mut game = match spec.build() {
        Ok(game) => game,
        Err(error) => {
            return (
                empty_result(spec, "setup_error", Some(error), started, options),
                None,
            );
        }
    };
    let shared = Rc::new(RefCell::new(Shared {
        decided: 0,
        seed: spec.seed,
        random: SeededRandom::new(spec.seed),
        tunings: options.tunings.clone(),
        by_subtype: BTreeMap::new(),
        usage: BTreeMap::new(),
        bot_ns: 0,
        by_rule: options.trace.then(Vec::new),
        last: (0, 0),
        repeats: 0,
        loop_steps: options.limits.loop_steps,
        looped: None,
    }));
    game.table.set_default(Box::new(SelfPlay(shared.clone())));
    let limits = &options.limits;
    let mut phase_ns: BTreeMap<String, u128> = BTreeMap::new();
    let mut steps = 0usize;
    let mut round_log: Vec<(usize, u32, String)> = Vec::new();
    let mut vp_by_round: Vec<Vec<i32>> = Vec::new();
    let mut recorder = options
        .timeline
        .then(|| crate::timeline::Recorder::new(spec, &game.state))
        .flatten();
    let (termination, detail) = loop {
        while vp_by_round.len() + 1 < game.state.round as usize {
            vp_by_round.push(
                game.state
                    .players
                    .iter()
                    .map(|p| p.victory_points)
                    .collect(),
            );
        }
        if game.state.finished {
            break ("victory", None);
        }
        if game.state.round > limits.max_rounds {
            break (
                "cutoff_rounds",
                Some(format!("round {} reached", limits.max_rounds + 1)),
            );
        }
        if steps >= limits.max_steps {
            break (
                "cutoff_steps",
                Some(format!("{steps} steps in round {}", game.state.round)),
            );
        }
        if limits.max_seconds > 0.0
            && steps % 256 == 0
            && started.elapsed().as_secs_f64() > limits.max_seconds
        {
            break (
                "stuck",
                Some(format!(
                    "{:.0} s of wall time in round {} {:?}",
                    limits.max_seconds, game.state.round, game.state.phase
                )),
            );
        }
        if let Some(text) = shared.borrow().looped.clone() {
            break (
                "stuck",
                Some(format!("same question {} times: {text}", limits.loop_steps)),
            );
        }
        let phase = format!("{:?}", game.state.phase).to_lowercase();
        let t = Instant::now();
        let result = game.step();
        *phase_ns.entry(phase.clone()).or_default() += t.elapsed().as_nanos();
        steps += 1;
        if let Some(recorder) = recorder.as_mut() {
            recorder.observe(&game.state);
        }
        if options.trace {
            round_log.push((shared.borrow().decided, game.state.round, phase));
        }
        if let Some(error) = result.error {
            break (
                "engine_error",
                Some(format!(
                    "round {} {:?}: {error}",
                    game.state.round, game.state.phase
                )),
            );
        }
    };
    let timeline = recorder.map(|r| r.finish(&game.state));
    let state = &game.state;
    let vp: Vec<i32> = state.players.iter().map(|p| p.victory_points).collect();
    let winner = (termination == "victory")
        .then(|| ti4_engine::objectives::leader(state))
        .flatten()
        .and_then(|id| SEATS.iter().position(|s| *s == id.as_str()));
    let scored = state
        .players
        .iter()
        .map(|p| {
            state
                .scored_by(&p.id)
                .iter()
                .map(|o| o.as_str().to_owned())
                .collect()
        })
        .collect();
    let mut vp_sources: BTreeMap<String, i32> = BTreeMap::new();
    for (_, delta, reason) in &state.vp_ledger {
        *vp_sources.entry(source_of(reason)).or_default() += delta;
    }
    let mut end_counts: BTreeMap<String, i32> = BTreeMap::new();
    for seat in &state.players {
        for (id, system) in &state.board {
            let home = seat.home_system.as_ref() == Some(id);
            for unit in system
                .units
                .iter()
                .chain(system.planet_units.values().flatten())
            {
                if unit.owner != seat.id {
                    continue;
                }
                let t = unit.type_id.as_str();
                let key = if t.contains("infantry") || t.contains("mech") {
                    if home {
                        *end_counts.entry("home_ground".to_owned()).or_default() += 1;
                    }
                    "ground"
                } else if t.contains("fighter") || t.contains("pds") || t.contains("dock") {
                    continue;
                } else {
                    "ships"
                };
                *end_counts.entry(key.to_owned()).or_default() += 1;
            }
        }
    }
    let records = &game.table.log.records;
    let hash = ti4_wasm::host_session::decisions_hash(records);
    let sh = shared.borrow();
    let trace = sh.by_rule.as_ref().map(|by_rule| {
        let mut phase_at = round_log.iter().peekable();
        records
            .iter()
            .enumerate()
            .map(|(i, r)| {
                // The state the step that made decision `i` ended in: close enough for a trace.
                while phase_at.next_if(|(n, _, _)| *n <= i).is_some() {}
                let (round, phase) = phase_at
                    .peek()
                    .map_or((0, String::new()), |(_, r, p)| (*r, p.clone()));
                TraceLine {
                    i,
                    seat: r.player.as_str().to_owned(),
                    round: r.context.as_ref().map_or(round, |c| c.round),
                    phase: r
                        .context
                        .as_ref()
                        .map_or(phase, |c| format!("{:?}", c.phase).to_lowercase()),
                    subtype: r
                        .context
                        .as_ref()
                        .map_or_else(String::new, |c| c.subtype.clone()),
                    prompt: r.prompt.clone(),
                    chosen: r.chosen.clone(),
                    offered: r.offered.len(),
                    by: if by_rule.get(i).copied().unwrap_or(false) {
                        "rule"
                    } else {
                        "random"
                    },
                }
            })
            .collect()
    });
    let result = GameResult {
        spec: spec.clone(),
        termination: termination.to_owned(),
        detail,
        winner,
        winner_faction: winner.map(|w| spec.factions[w].clone()),
        vp,
        rounds: state.round,
        decisions: records.len(),
        steps,
        wall_ms: started.elapsed().as_secs_f64() * 1e3,
        bot_ms: sh.bot_ns as f64 / 1e6,
        phase_ms: phase_ns
            .into_iter()
            .map(|(k, v)| (k, v as f64 / 1e6))
            .collect(),
        hash,
        by_subtype: sh.by_subtype.clone(),
        usage: sh.usage.clone(),
        scored,
        vp_sources,
        end_counts,
        vp_by_round,
        timeline,
        candidate_seats: options.candidate_seats.clone(),
    };
    (result, trace)
}
