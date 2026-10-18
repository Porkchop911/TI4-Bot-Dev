//! The engine behind a wasm boundary, for local play in `web2/` and the offline game of `web/`.
//! See `README.md` for the measurements.
//!
//! The boundary is the plain C ABI rather than `wasm-bindgen`. A game is played through one
//! host, [`host_api`]: `ClientMessage` in, `ServerMessage` out, as on the server; the host also
//! answers queries that need no game (the catalog, the odds of a combat). The setup of a table
//! and the scenarios are exports of their own: such an export returns a status, zero or more is the length of a
//! JSON result, less than zero is the negated length of an error text. Either is read from the
//! response buffer, whose address never changes ([`ti4_response_ptr`]).
//!
//! # Memory
//!
//! The results of those exports are written straight into the fixed buffers of [`buffer`].

pub mod buffer;
pub mod host_api;
pub mod host_session;
pub mod host_setup;
#[cfg(feature = "ts")]
pub mod wire_ts;

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, VecDeque};
use std::fmt::{self, Write as _};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, LazyLock, Mutex};

use serde::{Deserialize, Serialize};
use ti4_content::ContentStore;
use ti4_content::galaxy::Galaxy;
use ti4_engine::choice::{
    Choice, ChoiceOption, Decider, IllegalChoice, SeatObservation, SeededRandom, Table,
};
use ti4_engine::game::{Game, RunError};
use ti4_host::bots::{Bot, Carry};
use ti4_host::map::{build_board_tiles, create_game_with_template};
use ti4_host::maps::{TemplateLoader, default_template_for};
use ti4_host::planning::audit::PlanScope;
use ti4_host::planning::draft::{DraftBase, DraftStep, run_draft};
use ti4_host::projection::project_game_view_full;
use ti4_host::protocol::status::ViewerRole;
use ti4_host::protocol::view::{BoardTileView, GameView};
use ti4_host::setup;
use ti4_host::view::reaction_modes::{NeverOffer, NeverSet};
use ti4_host::view::secondary::SecondaryWatch;
use ti4_model::content_types::POK;
use ti4_model::id::PlayerId;
use ti4_model::state::GameState;

use crate::buffer::{REQUEST, RESPONSE, TooLarge, store};
use crate::host_setup::TableRequest;

pub(crate) const SEATS: [&str; 8] = ["a", "b", "c", "d", "e", "f", "g", "h"];
const ROUNDS: u32 = 50;

#[derive(Debug)]
enum Failure {
    Setup(String),
    Run(RunError),
    TooLarge(TooLarge),
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Setup(error) => write!(f, "setup: {error}"),
            Self::Run(error) => write!(f, "run: {error}"),
            Self::TooLarge(error) => error.fmt(f),
        }
    }
}

impl From<TooLarge> for Failure {
    fn from(error: TooLarge) -> Self {
        Self::TooLarge(error)
    }
}

fn failed(error: impl fmt::Display) -> Failure {
    Failure::Setup(error.to_string())
}

/// A seeded game for `players` seats on the recommended map template for that count. With a
/// scenario, the game of that scenario (`ti4_host::view::scenario::start`): its map, and its
/// seed unless it plays on any seed.
pub(crate) fn new_game(
    content: &'static ContentStore,
    seed: u64,
    players: usize,
    scenario: Option<&str>,
) -> Result<Game<'static>, Failure> {
    let seats = SEATS
        .get(..players)
        .filter(|seats| seats.len() >= 3)
        .ok_or_else(|| {
            failed(format_args!(
                "{players} players: a game has 3 to {} seats",
                SEATS.len()
            ))
        })?;
    let ids: Vec<PlayerId> = seats.iter().map(|name| PlayerId::new(*name)).collect();
    let (state, galaxy) = if let Some(scenario) = scenario {
        // The game of the scenario, as the server starts it for a table ("Start from").
        ti4_host::view::scenario::start(content, scenario, &ids, seed).map_err(failed)?
    } else {
        let loader = TemplateLoader::load().map_err(failed)?;
        let template = default_template_for(content, &loader, players, POK)
            .ok_or_else(|| failed(format_args!("no map template builds for {players} players")))?;
        create_game_with_template(content, &ids, seed, Some(&template)).map_err(failed)?
    };
    Ok(Game::with_seeded_random(state, content, seed)
        .with_sources(POK)
        .with_galaxy(galaxy))
}

/// The table in the request buffer, `request_len` bytes of JSON, with its values checked.
fn read_table(request_len: u32) -> Result<TableRequest, Failure> {
    request_filled(request_len)?;
    let table: TableRequest = {
        let request = REQUEST.lock().expect("request lock");
        serde_json::from_slice(request.as_bytes())
            .map_err(|error| failed(format_args!("the table is not read: {error}")))?
    };
    table.checked().map_err(failed)
}

/// What a view is made from while the game itself is borrowed by its run: a decider is given no
/// state, so the run leaves a copy here after each step.
struct Latest {
    state: GameState,
    tiles: Vec<BoardTileView>,
    /// The map, for the facts of a tactical action. It is copied once: a game of this host is
    /// played on the map it began with.
    galaxy: Option<Galaxy>,
}

static LATEST: Mutex<Option<Latest>> = Mutex::new(None);

fn latest_of(game: &Game<'_>, content: &ContentStore) -> Latest {
    Latest {
        state: game.state.clone(),
        tiles: game
            .galaxy()
            .map_or_else(Vec::new, |galaxy| build_board_tiles(content, galaxy)),
        galaxy: game.galaxy().cloned(),
    }
}

/// The strategic action of the game that is played, from the card that is played to its last
/// follower: a fact of the update, and what says which secondary a draft may be of.
pub(crate) static SECONDARY: LazyLock<SecondaryWatch> = LazyLock::new(SecondaryWatch::default);

/// Follow the strategic actions of `game`, from where it is. A copy of a game is not followed:
/// the game after an undo is bound again.
fn watch_secondary(game: &mut Game<'_>) {
    SECONDARY.bind(game);
    SECONDARY.completed_step(game);
}

/// Serializes as the text a value displays as, without building that text first.
struct Shown<T>(T);

impl<T: fmt::Display> Serialize for Shown<T> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(&self.0)
    }
}

#[derive(Serialize)]
struct Report<'a> {
    decisions: usize,
    round: u32,
    /// Why the game stopped before its last round, when it did.
    stopped: Option<Shown<&'a RunError>>,
    state: Option<&'a GameState>,
    /// What seat `a` is shown of `state`.
    view: Option<GameView>,
}

fn respond(what: &'static str, report: &impl Serialize) -> Result<usize, Failure> {
    let mut response = RESPONSE.lock().expect("response lock");
    Ok(store(&mut response, what, "RESPONSE_CAPACITY", report)?)
}

/// Play a seeded game of `players` seats with random deciders for at most `max_steps` steps. The result
/// is the state reached and the number of decisions taken.
fn run_seeded(seed: u64, players: usize, max_steps: usize) -> Result<usize, Failure> {
    let content = ContentStore::embedded();
    let mut game = new_game(content, seed, players, None)?;
    match game.run(ROUNDS, max_steps) {
        Ok(_) | Err(RunError::StepLimit { .. }) => {}
        Err(error) => return Err(Failure::Run(error)),
    }
    let latest = latest_of(&game, content);
    let viewer = ViewerRole::Player(PlayerId::new("a"));
    respond(
        "the result of ti4_run_seeded",
        &Report {
            decisions: game.table.log.records.len(),
            round: game.state.round,
            stopped: None,
            state: Some(&game.state),
            view: Some(project_game_view_full(
                &latest.state,
                content,
                game.sources(),
                &viewer,
                &latest.tiles,
                None,
                &[],
            )),
        },
    )
}

/// What the seats on the host share during one game.
#[derive(Default)]
struct Host {
    /// How many choices the host was asked. The nonce of a choice is its number.
    asked: u64,
    /// The seat that was asked last: whose view the host is shown.
    viewer: Option<PlayerId>,
    /// The host left the game. Every later choice fails without asking, so the run unwinds.
    left: bool,
    /// The host takes back its last answer. Every later choice fails without asking, so the
    /// step unwinds, and the run goes back to a checkpoint.
    undo: bool,
    /// After an undo: the answers between the checkpoint and the choice that is open again.
    /// They are given without asking. (The engine's `Scripted` owns a boxed decider, which a
    /// static cannot hold.)
    script: VecDeque<String>,
    /// The answers that a checkpoint can still take back: the number of the choice, the id of
    /// the chosen option, and whether the choice was passed for the host ([`NEVER`]).
    recent: VecDeque<(u64, String, bool)>,
    /// How many choices the host had been asked at the oldest checkpoint.
    oldest: Option<u64>,
    /// The seats of the game, and which of them the host answers (bit 0 is seat `a`).
    players: usize,
    humans: u32,
    /// The seed of the game: a bot seat decides from it ([`ti4_host::bots::choose`]).
    seed: u64,
    /// The players in the seat order of the game: a seat decides by its place here, not by its
    /// id ([`ti4_host::bots::choose`]).
    order: Vec<PlayerId>,
    /// The game is of a table (`host_setup::TableRequest`): a random seat decides as on the
    /// server, from the seed, the seat and the number of the decision, not from the stream of
    /// [`RANDOM`].
    table: bool,
    /// The game waits before a decision of a seat that the host does not play ([`STEPPING`]),
    /// not in a choice.
    paused: bool,
    /// How many decisions the seats made, the random seats included. A checkpoint keeps it.
    decided: u64,
    /// The changes of the host's seats, in order (the local command `switch_seat`).
    switches: Vec<Switch>,
    /// The host changed its seats in place of an answer: the decision is asked again.
    switched: bool,
    /// How many decisions of the game have their answer in a saved game (`Resume::log`): the
    /// first ones, of every seat. Each of them is given again ([`saved`]).
    saved: u64,
    /// How many of the answers in `script` are those of a saved game that is played again (the
    /// rest are answers after an undo). They are kept for a later undo, and their progress is
    /// told to the host.
    preload_left: u64,
    /// The choice an undo goes back to, in place of the last one: a batch that failed goes back
    /// to the question it began at.
    undo_to: Option<u64>,
    /// The undo in progress is the rollback of a failed batch.
    rollback: bool,
}

/// From the decision with this number on, the host answers the seats of `mask`.
struct Switch {
    decision: u64,
    mask: u32,
    /// How many choices the host had been asked when the switch came into force. The answers up
    /// to it are of other seats, and are not taken back.
    asked: Option<u64>,
}

/// Whether the bit of `player` is set in `mask` (bit 0 is seat `a`).
fn in_mask(mask: u32, player: &PlayerId) -> bool {
    SEATS
        .iter()
        .position(|seat| *seat == player.as_str())
        .is_some_and(|index| mask >> index & 1 == 1)
}

impl Host {
    /// Whether the answer before the pending choice can be taken back from a checkpoint.
    fn can_undo(&self) -> bool {
        // The pending choice has the number `asked`, so the answer before it is `asked - 1`.
        // Before a decision of a random seat no choice is pending: the last answer is `asked`.
        // An answer from before the last switch of the seats stays.
        let fence = self.switch().and_then(|switch| switch.asked).unwrap_or(0);
        self.oldest
            .is_some_and(|oldest| oldest.max(fence) < self.again() && !self.left)
    }

    /// The number of the choice that an undo opens again: the last one that the host answered
    /// itself. A window that was passed for it ([`NEVER`]) would be passed again.
    fn again(&self) -> u64 {
        let mut again = (self.asked + u64::from(self.paused)).saturating_sub(1);
        while self
            .recent
            .iter()
            .any(|(number, _, passed)| *number == again && *passed)
        {
            again -= 1;
        }
        again
    }

    /// The switch that is in force.
    fn switch(&self) -> Option<&Switch> {
        self.switches
            .iter()
            .rfind(|switch| switch.decision <= self.decided)
    }

    /// The seats that the host answers now (bit 0 is seat `a`).
    fn mask(&self) -> u32 {
        self.switch().map_or(self.humans, |switch| switch.mask)
    }

    /// Whether the host answers the next decision of `player`.
    fn plays(&mut self, player: &PlayerId) -> bool {
        let (asked, decided) = (self.asked, self.decided);
        let switch = self
            .switches
            .iter_mut()
            .rfind(|switch| switch.decision <= decided);
        let mask = switch.map_or(self.humans, |switch| {
            switch.asked.get_or_insert(asked);
            switch.mask
        });
        in_mask(mask, player)
    }

    /// The seat that the host is shown the game as: the seat asked last when the host still
    /// answers it, or the first of its seats.
    fn shown_seat(&self) -> Option<PlayerId> {
        let mask = self.mask();
        self.viewer
            .clone()
            .filter(|viewer| in_mask(mask, viewer))
            .or_else(|| {
                SEATS
                    .iter()
                    .take(self.players)
                    .enumerate()
                    .find(|(index, _)| mask >> index & 1 == 1)
                    .map(|(_, seat)| PlayerId::new(*seat))
            })
    }
}

static HOST: Mutex<Host> = Mutex::new(Host {
    asked: 0,
    viewer: None,
    left: false,
    undo: false,
    script: VecDeque::new(),
    recent: VecDeque::new(),
    oldest: None,
    players: 0,
    humans: 0,
    seed: 0,
    order: Vec::new(),
    table: false,
    paused: false,
    decided: 0,
    switches: Vec::new(),
    switched: false,
    saved: 0,
    preload_left: 0,
    undo_to: None,
    rollback: false,
});

/// The mode of each reaction that the host set (`ClientMessage::SetReactionMode`): a window that
/// offers only cards that are never offered is declined for it. A setting of the host, not of
/// the game.
pub(crate) static NEVER: LazyLock<NeverSet> =
    LazyLock::new(|| Arc::new(Mutex::new(BTreeMap::new())));

/// The answer of the host that lets a seat decide that the game waits for (the local command
/// `step`).
pub const GO: i32 = -3;

/// Whether the host is asked before a decision of a seat that it does not play: [`RUN`], 1
/// (before each decision) or [`PACE`]. See [`set_stepping`].
static STEPPING: AtomicU32 = AtomicU32::new(RUN);

/// The seats that the host does not play decide with no stop.
const RUN: u32 = 0;
/// The host is asked before each of their decisions, but for a reaction window that the seat
/// only declines: the wait of a table, as the server has it.
const PACE: u32 = 2;

/// The answer of the host after [`switch_seats`]: the decision is asked again, of the seat's
/// new decider.
pub const SWITCH: i32 = -4;

/// The answer of the host that takes back its last answer, when [`Host::can_undo`] says that a
/// checkpoint holds it.
pub const UNDO: i32 = -2;

/// How many checkpoints are kept: one after each step that asked the host. An answer older than
/// these is taken back by playing the game again from its seed, which the host does itself.
const CHECKPOINT_LIMIT: usize = 32;

/// The bot of each seat where the host does not play it, by the index of the seat in [`SEATS`]
/// (of the game that is played): the rule bot or an MLP model, or in the game of a table a
/// random seat. A seat with none draws from [`RANDOM`] (in the game of a table: at random from
/// the seed, as on the server). An MLP seat keeps its pipeline here between its decisions; a
/// checkpoint keeps its carry ([`Checkpoint::carries`]).
static BOTS: Mutex<Vec<Option<Bot>>> = Mutex::new(Vec::new());

/// The stream of the seats that decide at random. It is here, not inside the table, so that a
/// checkpoint can keep its position.
static RANDOM: Mutex<Option<SeededRandom>> = Mutex::new(None);

/// Every seat of a played game: the host answers the seats of its mask, the bots of [`BOTS`]
/// theirs, and the others draw from [`RANDOM`]. The mask can change during the game
/// ([`switch_seats`]), so the decider of a seat is found at each decision.
struct Seat {
    /// The host, behind its Never cards: a window that offers only such cards is declined
    /// without asking, and any other window is asked without them. The same wrapper as on the
    /// server (`ti4_host::view::reaction_modes`).
    host: NeverOffer,
}

impl Seat {
    fn new() -> Self {
        let host = NeverOffer::new(Box::new(Asked), NEVER.clone()).on_skip(|choice, cards| {
            let Some(option) = choice.options.iter().find(|option| option.is_decline()) else {
                return;
            };
            // The window is a choice of the host with an answer, so an undo and a saved game
            // have it. The host is told with its next choice.
            {
                let mut host = host();
                host.asked += 1;
                let number = host.asked;
                host.recent.push_back((number, option.id.clone(), true));
            }
            host_api::passed_window(&choice.player, cards);
        });
        Self { host }
    }
}

impl Seat {
    /// With `seen`, the seat can be played by its bot.
    fn decide(
        &mut self,
        choice: &Choice,
        seen: Option<&SeatObservation<'_>>,
    ) -> Result<ChoiceOption, IllegalChoice> {
        loop {
            let ours = host().plays(&choice.player);
            let replayed = {
                let host = host();
                host.decided < host.saved
            };
            let answer = if replayed {
                // A decision of a saved game is given below the Never cards and with no bot, as
                // a replay on the server is: the game is the same whatever the cards are set to
                // now, and whoever plays the seat.
                Some(saved(choice, ours, seen)?)
            } else if !ours {
                draw(choice, seen)?
            } else if let Some(option) = given(choice)? {
                Some(option)
            } else if !declined_for_never(choice)
                && let Some(option) = scripted(choice)?
            {
                // A window that the Never cards decline is declined below, before a script
                // sees it: it does not stop the script.
                Some(option)
            } else {
                match self.host.choose(choice) {
                    Ok(option) => Some(option),
                    Err(_) if std::mem::take(&mut host().switched) => None,
                    Err(error) => return Err(error),
                }
            };
            // Without an answer the host changed its seats here: the decision is asked again.
            if let Some(option) = answer {
                let (asked, replay_done) = {
                    let mut host = host();
                    host.decided += 1;
                    (host.asked, replayed && host.decided == host.saved)
                };
                host_api::selected(choice, &option, ours.then_some(asked));
                if replay_done {
                    host_api::replay_finished();
                }
                return Ok(option);
            }
        }
    }
}

impl Decider for Seat {
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

/// Whether the host's Never cards decline `choice` without asking ([`NEVER`]).
fn declined_for_never(choice: &Choice) -> bool {
    matches!(
        ti4_host::view::reaction_modes::filter(choice, &NEVER.lock().expect("never set lock")),
        ti4_host::view::reaction_modes::Filtered::Declined { .. }
    )
}

/// The index of the seat of `player` in [`SEATS`].
fn seat_slot(player: &PlayerId) -> Option<usize> {
    SEATS.iter().position(|seat| *seat == player.as_str())
}

/// The answer of the bot of a seat that the host does not play, from `bots` (by the index of the
/// seat in [`SEATS`]): `None` when the seat draws from the stream of the random seats. A seat
/// with a bot decides with `seen`; without it, in a game of a table it decides at random from the
/// seed, else from the stream. With `peek` the bot keeps nothing of the decision.
#[allow(clippy::too_many_arguments)]
fn bot_pick(
    bots: &mut [Option<Bot>],
    table: bool,
    choice: &Choice,
    seen: Option<&SeatObservation<'_>>,
    seed: u64,
    place: usize,
    index: u64,
    peek: bool,
) -> Option<usize> {
    let bot = seat_slot(&choice.player)
        .and_then(|slot| bots.get_mut(slot))
        .and_then(Option::as_mut);
    if choice.options.is_empty() || !(table || (seen.is_some() && bot.is_some())) {
        return None;
    }
    let index = usize::try_from(index).expect("a game has fewer decisions than 2^32");
    let picked = match bot {
        Some(bot) if peek => {
            let carry = bot.carry();
            let (picked, _) = ti4_host::bots::choose(bot, choice, seen, seed, place, index);
            bot.restore(carry);
            picked
        }
        Some(bot) => ti4_host::bots::choose(bot, choice, seen, seed, place, index).0,
        None => ti4_rule_bot::choose(choice, None, seed, place, index).0,
    };
    Some(picked)
}

/// The host itself: every choice that reaches it is handed out.
struct Asked;

impl Decider for Asked {
    fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
        ask_host(choice)?.ok_or_else(|| {
            host().switched = true;
            IllegalChoice::DeciderFailed {
                player: choice.player.clone(),
                prompt: choice.prompt.clone(),
                reason: "the host changed its seats".to_owned(),
            }
        })
    }
}

/// The answer to a choice of the host that needs no asking: the game left, or the answer is
/// given again after an undo. None when the host is asked.
fn given(choice: &Choice) -> Result<Option<ChoiceOption>, IllegalChoice> {
    let failed = |reason: &str| IllegalChoice::DeciderFailed {
        player: choice.player.clone(),
        prompt: choice.prompt.clone(),
        reason: reason.to_owned(),
    };
    let mut host = host();
    if host.left {
        return Err(failed(LEFT));
    }
    if host.undo {
        return Err(failed(TAKEN_BACK));
    }
    let Some(wanted) = host.script.pop_front() else {
        return Ok(None);
    };
    host.asked += 1;
    // The answers of a saved game that is played again can be taken back later, like any other.
    let preloaded = host.preload_left > 0;
    if preloaded {
        host.preload_left -= 1;
        let number = host.asked;
        host.recent.push_back((number, wanted.clone(), false));
    }
    let left = host.preload_left;
    drop(host);
    let option = choice
        .answer_for(&wanted)
        .ok_or_else(|| IllegalChoice::ScriptDiverged {
            player: choice.player.clone(),
            wanted,
            offered: choice.ids().into_iter().map(str::to_owned).collect(),
        })?;
    if preloaded {
        if left == 0 {
            host_api::replay_finished();
        } else if left.is_multiple_of(PROGRESS_EVERY)
            && let Some((done, total)) = host_api::replay_progress(left)
        {
            host_api::write_progress(done, total);
            if host_ask() == LEAVE {
                self::host().left = true;
                return Err(failed(LEFT));
            }
        }
    }
    Ok(Some(option))
}

/// How many replayed answers pass between two progress reports to the host.
const PROGRESS_EVERY: u64 = 20;

/// What a running batch or script answers to a choice of the host's seat. A batch that fails
/// takes the game back to the question it began at.
fn scripted(choice: &Choice) -> Result<Option<ChoiceOption>, IllegalChoice> {
    match host_api::scripted(choice) {
        host_session::Scripted::Answer(option) => {
            let mut host = host();
            host.asked += 1;
            let number = host.asked;
            host.recent.push_back((number, option.id.clone(), false));
            Ok(Some(option))
        }
        host_session::Scripted::Ask => Ok(None),
        host_session::Scripted::Rollback(start) => {
            let mut host = host();
            host.undo = true;
            host.undo_to = Some(start);
            host.rollback = true;
            Err(IllegalChoice::DeciderFailed {
                player: choice.player.clone(),
                prompt: choice.prompt.clone(),
                reason: TAKEN_BACK.to_owned(),
            })
        }
    }
}

/// A decision of a saved game (`Resume::log`): it is given again, whoever plays the seat. Nothing
/// is shown and no seat waits; what the host counts is counted as in a game that is not saved,
/// so the choices after it have the same numbers. `ours`: the host plays the seat. A bot of the
/// seat is given the decision (`Bot::follow`), so that it keeps what it would have kept.
fn saved(
    choice: &Choice,
    ours: bool,
    seen: Option<&SeatObservation<'_>>,
) -> Result<ChoiceOption, IllegalChoice> {
    let failed = |reason: &str| IllegalChoice::DeciderFailed {
        player: choice.player.clone(),
        prompt: choice.prompt.clone(),
        reason: reason.to_owned(),
    };
    let (index, total) = {
        let host = host();
        if host.left {
            return Err(failed(LEFT));
        }
        if host.undo {
            return Err(failed(TAKEN_BACK));
        }
        (host.decided, host.saved)
    };
    let wanted = host_api::saved_decision(index).ok_or_else(|| failed(LEFT))?;
    let Some(option) = choice.option(&wanted).cloned() else {
        // Another build of the engine asks another question here, or offers other options. The
        // game ends, and the host is told why (`end.diverged`).
        let text = format!(
            "decision {} of {total} was \"{wanted}\", but \"{}\" of seat {} offers {}",
            index + 1,
            choice.prompt,
            choice.player.as_str(),
            choice.ids().join(", ")
        );
        host_api::diverged(&text);
        host().left = true;
        return Err(failed(&text));
    };
    let done = index + 1;
    if done < total && done.is_multiple_of(PROGRESS_EVERY) {
        host_api::write_progress(done, total);
        if host_ask() == LEAVE {
            host().left = true;
            return Err(failed(LEFT));
        }
    }
    let mut host = host();
    if ours {
        host.asked += 1;
        host.viewer = Some(choice.player.clone());
        let number = host.asked;
        host.recent.push_back((number, option.id.clone(), false));
        return Ok(option);
    }
    let (seed, table, place) = (
        host.seed,
        host.table,
        seat_place(&host.order, &choice.player),
    );
    drop(host);
    let mut bots = BOTS.lock().expect("bots lock");
    let bot = seat_slot(&choice.player)
        .and_then(|slot| bots.get_mut(slot))
        .and_then(Option::as_mut);
    match bot {
        Some(bot) if seen.is_some() => {
            let index = usize::try_from(index).expect("a game has fewer decisions than 2^32");
            bot.follow(choice, seen, &option, seed, place, index);
        }
        _ if table => {}
        _ => {
            // The seat would have drawn from the stream of the random seats: it moves as it did.
            drop(bots);
            if let Some(stream) = RANDOM.lock().expect("random lock").as_mut() {
                let _ = stream.choose(choice);
            }
        }
    }
    Ok(option)
}

/// A decision of a seat that the host does not play: of its bot ([`BOTS`]), else from the
/// stream that all random seats share, in decision order. None
/// when the host changed its seats while the seat waited for it.
fn draw(
    choice: &Choice,
    seen: Option<&SeatObservation<'_>>,
) -> Result<Option<ChoiceOption>, IllegalChoice> {
    let stepping = STEPPING.load(Ordering::Relaxed);
    if stepping != RUN {
        // What the seat picks is known before the host is asked, as on the server: the wait of a
        // table is not taken for a window that is only declined.
        let declines = stepping == PACE
            && ti4_host::projection::is_reaction_window(choice)
            && pick(choice, seen, true)?.is_decline();
        if !declines && !wait_for_go(choice)? {
            return Ok(None);
        }
    }
    pick(choice, seen, false).map(Some)
}

/// What a seat that the host does not play answers. With `peek` the stream of the random seats
/// stays where it is, so the same call gives the same answer again.
fn pick(
    choice: &Choice,
    seen: Option<&SeatObservation<'_>>,
    peek: bool,
) -> Result<ChoiceOption, IllegalChoice> {
    #[cfg(not(target_arch = "wasm32"))]
    if !peek {
        PICKS.fetch_add(1, Ordering::Relaxed);
    }
    let (seed, index, table, place) = {
        let host = host();
        (
            host.seed,
            host.decided,
            host.table,
            seat_place(&host.order, &choice.player),
        )
    };
    // The seed, the place of the seat and the number of the decision give the answer, so the
    // stream of the random seats does not move (a checkpoint keeps the carry of an MLP seat). In
    // the game of a table a random seat decides the same way, as on the server.
    let picked = bot_pick(
        &mut BOTS.lock().expect("bots lock"),
        table,
        choice,
        seen,
        seed,
        place,
        index,
        peek,
    );
    if let Some(picked) = picked {
        return Ok(choice.options[picked].clone());
    }
    let mut random = RANDOM.lock().expect("random lock");
    let stream = random.as_mut().expect("a played game has a random stream");
    if peek {
        stream.clone().choose(choice)
    } else {
        stream.choose(choice)
    }
}

/// The place of `player` in the seat order `order` of the game, from 0.
fn seat_place(order: &[PlayerId], player: &PlayerId) -> usize {
    order
        .iter()
        .position(|seat| seat == player)
        .expect("a seat that decides is a seat of the game")
}

/// Wait until the host lets the seat of `choice` decide. The host is shown the game as its own
/// seat sees it, with no choice of its own; the turn status names the seat that decides. False
/// when the host changed its seats instead.
fn wait_for_go(choice: &Choice) -> Result<bool, IllegalChoice> {
    let failed = |reason: &str| IllegalChoice::DeciderFailed {
        player: choice.player.clone(),
        prompt: choice.prompt.clone(),
        reason: reason.to_owned(),
    };
    let viewer = {
        let mut host = host();
        if host.left {
            return Err(failed(LEFT));
        }
        if host.undo {
            return Err(failed(TAKEN_BACK));
        }
        host.paused = true;
        host.shown_seat()
    };
    let state = LATEST
        .lock()
        .expect("latest lock")
        .as_ref()
        .map(|latest| latest.state.clone());
    let given = state.and_then(|state| host_api::paused(choice, viewer.as_ref(), &state));
    let index = given.unwrap_or_else(host_ask);
    host_api::answered();
    let mut host = host();
    if index == GO || index == SWITCH {
        host.paused = false;
        return Ok(index == GO);
    }
    if index == UNDO && host.can_undo() {
        // As if a choice with the next number were pending: `take_back` opens the one before it.
        host.paused = false;
        host.asked += 1;
        host.undo = true;
        return Err(failed(TAKEN_BACK));
    }
    host.paused = false;
    host.left = true;
    Err(failed(LEFT))
}

/// The answer that leaves the game.
pub(crate) const LEAVE: i32 = -1;

thread_local! {
    /// The game after each step that asked the host, oldest first; at first, the game at its
    /// start. Here, not in [`play`], so that a draft can start from the last one while the game
    /// is parked in a choice. A game is not `Send`, so this is not a static behind a lock.
    static CHECKPOINTS: RefCell<VecDeque<Checkpoint>> = const { RefCell::new(VecDeque::new()) };
    /// How many steps the game finished since the last checkpoint. None of them asked the host.
    static SINCE: Cell<usize> = const { Cell::new(0) };
    /// The game at its last finished step, for a draft. Made when the first draft asks for it,
    /// and dropped when the game goes on.
    static DRAFT_BASE: RefCell<Option<Game<'static>>> = const { RefCell::new(None) };
}

/// The game between two steps, with what the game does not hold itself.
struct Checkpoint {
    game: Game<'static>,
    random: Option<SeededRandom>,
    /// What the bot of each seat kept ([`BOTS`], `Bot::carry`).
    carries: Vec<Option<Carry>>,
    /// How many choices the host had been asked.
    asked: u64,
    /// How many decisions the log had.
    decisions: usize,
    /// How many decisions the seats had made ([`Host::decided`]).
    decided: u64,
}

/// The table of a played game: its seats, and the hook that keeps [`LATEST`] current inside a
/// step.
fn host_table() -> Table {
    let mut table = Table::with_default(Box::new(Seat::new()));
    // A choice inside a step is offered on the position it is asked in, not on the one the step
    // began with.
    table.on_observed_offer(|records, state| {
        if let Some(latest) = LATEST.lock().expect("latest lock").as_mut() {
            latest.state.clone_from(state);
        }
        host_api::offer(records, state);
    });
    table.on_auto_resolved(|note| {
        host_api::auto_resolved(&note.player, &note.prompt, &note.label, &note.reason);
    });
    table
}

fn host() -> std::sync::MutexGuard<'static, Host> {
    HOST.lock().expect("host lock")
}

/// What the bot of each seat keeps now ([`BOTS`], `Bot::carry`).
fn carries() -> Vec<Option<Carry>> {
    BOTS.lock()
        .expect("bots lock")
        .iter()
        .map(|bot| bot.as_ref().and_then(Bot::carry))
        .collect()
}

/// A decision of a seat of the host: the choice is handed out and the game waits for the
/// answer. None when the host changed its seats instead.
fn ask_host(choice: &Choice) -> Result<Option<ChoiceOption>, IllegalChoice> {
    let failed = |reason: &str| IllegalChoice::DeciderFailed {
        player: choice.player.clone(),
        prompt: choice.prompt.clone(),
        reason: reason.to_owned(),
    };
    let nonce = {
        let mut host = host();
        if host.left {
            return Err(failed(LEFT));
        }
        if host.undo {
            return Err(failed(TAKEN_BACK));
        }
        host.asked += 1;
        host.viewer = Some(choice.player.clone());
        host.asked
    };
    let mut number = NonceText::default();
    let _ = write!(number, "{nonce}");
    let state = LATEST
        .lock()
        .expect("latest lock")
        .as_ref()
        .map(|latest| latest.state.clone());
    // No lock is held while the host is asked: it sends commands while this stack is parked.
    let given = state.and_then(|state| host_api::ask(choice, number.as_str(), &state));
    let index = given.unwrap_or_else(host_ask);
    host_api::answered();
    let mut host = host();
    if index == SWITCH {
        // The choice was not answered: the next one of the host gets its number.
        host.asked -= 1;
        return Ok(None);
    }
    if index == UNDO && host.can_undo() {
        host.undo = true;
        return Err(failed(TAKEN_BACK));
    }
    let Ok(index) = usize::try_from(index) else {
        host.left = true;
        return Err(failed(if index == UNDO {
            "the host took back an answer that no checkpoint holds"
        } else {
            LEFT
        }));
    };
    let option = choice
        .options
        .get(index)
        .cloned()
        .ok_or_else(|| failed("the host answered with an index that is no option"))?;
    host.recent.push_back((nonce, option.id.clone(), false));
    Ok(Some(option))
}

const LEFT: &str = "the host left the game";
const TAKEN_BACK: &str = "the host took back its last answer";

/// The decimal digits of a `u64`, without an allocation.
#[derive(Default)]
struct NonceText {
    bytes: [u8; 20],
    len: usize,
}

impl NonceText {
    fn as_str(&self) -> &str {
        std::str::from_utf8(&self.bytes[..self.len]).expect("digits are ASCII")
    }
}

impl fmt::Write for NonceText {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        let end = self.len + text.len();
        self.bytes
            .get_mut(self.len..end)
            .ok_or(fmt::Error)?
            .copy_from_slice(text.as_bytes());
        self.len = end;
        Ok(())
    }
}

#[cfg(target_arch = "wasm32")]
fn host_ask() -> i32 {
    #[link(wasm_import_module = "host")]
    unsafe extern "C" {
        /// Returns the index of the chosen option, or a negative number to leave the game. The
        /// host may suspend the caller (JSPI).
        safe fn ask() -> i32;
    }
    ask()
}

/// The host of a native run: it is given the output of the host and answers like the wasm import.
#[cfg(not(target_arch = "wasm32"))]
type NativeHost = Box<dyn FnMut(&str) -> i32 + Send>;

#[cfg(not(target_arch = "wasm32"))]
static NATIVE_HOST: Mutex<Option<NativeHost>> = Mutex::new(None);

/// Answer the questions of [`ti4_host_start`] in a native run. Without a host the first option
/// is taken.
#[cfg(not(target_arch = "wasm32"))]
pub fn set_native_host(host: impl FnMut(&str) -> i32 + Send + 'static) {
    *NATIVE_HOST.lock().expect("native host lock") = Some(Box::new(host));
}

#[cfg(not(target_arch = "wasm32"))]
fn host_ask() -> i32 {
    let shown = host_api::out_text();
    NATIVE_HOST
        .lock()
        .expect("native host lock")
        .as_mut()
        .map_or(0, |host| host(&shown))
}

/// How many rounds a game is played at most.
#[cfg(target_arch = "wasm32")]
const fn round_limit() -> u32 {
    ROUNDS
}

/// A native caller can end a game earlier ([`set_round_limit_native`]).
#[cfg(not(target_arch = "wasm32"))]
static ROUND_LIMIT: AtomicU32 = AtomicU32::new(ROUNDS);

#[cfg(not(target_arch = "wasm32"))]
fn round_limit() -> u32 {
    ROUND_LIMIT.load(Ordering::Relaxed)
}

/// How many decisions the seats that the host does not play have made (the rule bot and the
/// random seats), for callers on the native side.
#[cfg(not(target_arch = "wasm32"))]
static PICKS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// The decisions of the game that was played last, for callers on the native side.
#[cfg(not(target_arch = "wasm32"))]
static DECISIONS: Mutex<Vec<ti4_engine::choice::DecisionRecord>> = Mutex::new(Vec::new());

/// Play a hosted game ([`host_api`]) to its end, or until the host leaves.
fn play_hosted(request_len: usize) -> Result<usize, Failure> {
    let prepared = host_api::prepare(request_len).map_err(Failure::Setup)?;
    // The bots of the seats, from the game as it starts. A kind whose model is not loaded
    // (`ti4_model_load`) refuses the start.
    let bots = prepared
        .bots
        .iter()
        .enumerate()
        .map(|(slot, kind)| {
            kind.as_deref()
                .map(|kind| {
                    let kind = ti4_host::bots::BotKind::parse(kind).map_err(failed)?;
                    Ok(Bot::new(
                        &kind,
                        &prepared.game.state,
                        prepared.game.galaxy(),
                        &PlayerId::new(SEATS[slot]),
                    ))
                })
                .transpose()
        })
        .collect::<Result<Vec<_>, Failure>>()?;
    *BOTS.lock().expect("bots lock") = bots;
    set_stepping(prepared.stepping);
    *RANDOM.lock().expect("random lock") = Some(SeededRandom::new(prepared.seed));
    {
        let mut host = host();
        *host = Host {
            players: prepared.players,
            humans: prepared.human_mask,
            seed: prepared.seed,
            order: prepared
                .game
                .state
                .players
                .iter()
                .map(|player| player.id.clone())
                .collect(),
            table: prepared.table,
            switches: prepared
                .switches
                .iter()
                .map(|switch| Switch {
                    decision: switch.decision,
                    mask: switch.humans,
                    asked: None,
                })
                .collect(),
            saved: prepared.saved,
            preload_left: prepared.answers.len() as u64,
            script: prepared.answers,
            ..Host::default()
        };
        if host.preload_left == 0 && host.saved == 0 {
            drop(host);
            host_api::replay_finished();
        }
    }
    drive(prepared.game)
}

/// The play loop. `game` is a new game; the seats of the mask of [`HOST`] are answered by the
/// host, and the session of [`host_api`] follows the game.
fn drive(mut game: Game<'static>) -> Result<usize, Failure> {
    let content = ContentStore::embedded();
    game.table = host_table();
    watch_secondary(&mut game);
    CHECKPOINTS.with_borrow_mut(VecDeque::clear);
    *LATEST.lock().expect("latest lock") = Some(latest_of(&game, content));
    let target = game.state.round.saturating_add(round_limit());
    let mut stopped = None;
    // A checkpoint is made after each step that asked the host ([`CHECKPOINTS`]). A copy after
    // every step would cost a third of the run. With these, an undo plays the steps of the
    // random seats since the checkpoint again, which is the wait before any choice.
    let mut marked = None;
    while stopped.is_none() && game.state.round < target && !game.state.finished && !host().left {
        let asked = host().asked;
        if marked != Some(asked) {
            let oldest = CHECKPOINTS.with_borrow_mut(|checkpoints| {
                checkpoints.push_back(Checkpoint {
                    game: game.fork(),
                    random: RANDOM.lock().expect("random lock").clone(),
                    carries: carries(),
                    asked,
                    decisions: game.table.log.records.len(),
                    decided: host().decided,
                });
                if checkpoints.len() > CHECKPOINT_LIMIT {
                    checkpoints.pop_front();
                }
                checkpoints.front().map_or(asked, |first| first.asked)
            });
            SINCE.set(0);
            marked = Some(asked);
            host_api::mark(asked, game.table.log.records.len());
            let mut host = host();
            // An answer from before the oldest checkpoint cannot be taken back from one.
            host.oldest = Some(oldest);
            while host
                .recent
                .front()
                .is_some_and(|(number, ..)| *number <= oldest)
            {
                host.recent.pop_front();
            }
        }
        DRAFT_BASE.take();
        // No borrow of the checkpoints is held here: the step may park in a choice, and a
        // draft reads them then.
        let events_before = game.events.len();
        let error = game.step().error;
        DRAFT_BASE.take();
        if std::mem::take(&mut host().undo) {
            game = CHECKPOINTS.with_borrow_mut(|checkpoints| take_back(checkpoints, game));
            watch_secondary(&mut game);
            SINCE.set(0);
            marked = Some(host().asked);
        } else {
            SINCE.set(SINCE.get() + 1);
            SECONDARY.completed_step(&game);
            stopped = error.map(RunError::from);
            host_api::after_step(&game, events_before);
        }
        if let Some(latest) = LATEST.lock().expect("latest lock").as_mut() {
            latest.state.clone_from(&game.state);
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    DECISIONS
        .lock()
        .expect("decisions lock")
        .clone_from(&game.table.log.records);
    let left = {
        let mut host = host();
        // The switches and the saved decisions are of this game.
        host.switches.clear();
        host.saved = 0;
        host.left
    };
    // The engine stops with an error when the host leaves; that is no failure of the game.
    let error = stopped.as_ref().filter(|_| !left).map(ToString::to_string);
    Ok(host_api::finish_run(error.as_deref(), left))
}

/// Go back to the choice before the pending one: the game of the last checkpoint before it, and
/// the answers between the two, which are given again without asking.
fn take_back(checkpoints: &mut VecDeque<Checkpoint>, mut left: Game<'static>) -> Game<'static> {
    let mut host = host();
    // The pending choice has the number `asked`; the one to open again is the last one before
    // it that the host answered itself.
    let rollback = std::mem::take(&mut host.rollback);
    let again = host.undo_to.take().unwrap_or_else(|| host.again());
    while checkpoints.len() > 1
        && checkpoints
            .back()
            .is_some_and(|checkpoint| checkpoint.asked >= again)
    {
        checkpoints.pop_back();
    }
    let checkpoint = checkpoints
        .back()
        .expect("the oldest checkpoint is before the choice: Host::can_undo");
    let mut game = checkpoint.game.fork();
    game.table = host_table();
    // The log goes on from where the checkpoint was taken.
    let mut records = std::mem::take(&mut left.table.log.records);
    records.truncate(checkpoint.decisions);
    game.table.log.records = records;
    RANDOM
        .lock()
        .expect("random lock")
        .clone_from(&checkpoint.random);
    for (bot, carry) in BOTS
        .lock()
        .expect("bots lock")
        .iter_mut()
        .zip(&checkpoint.carries)
    {
        if let Some(bot) = bot {
            bot.restore(carry.clone());
        }
    }
    host.recent.retain(|(number, ..)| *number < again);
    host.script = host
        .recent
        .iter()
        .filter(|(number, ..)| *number > checkpoint.asked)
        .map(|(_, id, _)| id.clone())
        .collect();
    // What is decided from here on is decided now, not played again from a saved game.
    host.saved = host.saved.min(checkpoint.decided);
    host.asked = checkpoint.asked;
    host.decided = checkpoint.decided;
    let (asked, reanswers) = (checkpoint.asked, host.script.len());
    drop(host);
    host_api::took_back(asked, rollback, reanswers);
    game
}

/// The seats that play on, on a copy of the game that is made again from a checkpoint: a copy of
/// the bot where the live game has one ([`BOTS`], with the carry of the checkpoint), else the
/// stream of the random seats. It counts decisions as the live seats do, so a bot decides as it
/// did in the live game.
struct Replayed {
    random: SeededRandom,
    bots: Vec<Option<Bot>>,
    decided: u64,
    seed: u64,
    /// The players in the seat order of the game, as [`Host::order`].
    order: Vec<PlayerId>,
    /// The game is of a table: a random seat decides from the seed, as in [`pick`].
    table: bool,
}

impl Replayed {
    fn decide(
        &mut self,
        choice: &Choice,
        seen: Option<&SeatObservation<'_>>,
    ) -> Result<ChoiceOption, IllegalChoice> {
        let index = self.decided;
        self.decided += 1;
        let place = seat_place(&self.order, &choice.player);
        let picked = bot_pick(
            &mut self.bots,
            self.table,
            choice,
            seen,
            self.seed,
            place,
            index,
            false,
        );
        match picked {
            Some(picked) => Ok(choice.options[picked].clone()),
            None => self.random.choose(choice),
        }
    }
}

impl Decider for Replayed {
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

/// The game at its last finished step: the last checkpoint, and the steps of the random seats
/// since then played again on a copy of their stream. The game that is parked is not touched.
/// These steps asked the host nothing, so every seat draws: a seat of the host may have been a
/// random seat in them ([`switch_seats`]).
fn draft_base() -> Result<Game<'static>, Failure> {
    CHECKPOINTS.with_borrow(|checkpoints| {
        let checkpoint = checkpoints
            .back()
            .ok_or_else(|| failed("no game is played: a draft needs one"))?;
        let mut game = checkpoint.game.fork();
        let random = checkpoint
            .random
            .clone()
            .ok_or_else(|| failed("the game has no random stream"))?;
        // One lock: a guard in a field of this literal would live to the end of the statement.
        let (seed, order, table) = {
            let host = host();
            (host.seed, host.order.clone(), host.table)
        };
        let bots = BOTS
            .lock()
            .expect("bots lock")
            .iter()
            .zip(&checkpoint.carries)
            .map(|(bot, carry)| bot.as_ref().map(|bot| bot.detached(carry.clone())))
            .collect();
        game.table = Table::with_default(Box::new(Replayed {
            random,
            bots,
            decided: checkpoint.decided,
            seed,
            order,
            table,
        }));
        for _ in 0..SINCE.get() {
            if let Some(error) = game.step().error {
                return Err(Failure::Run(error.into()));
            }
        }
        Ok(game)
    })
}

/// A copy of the game at its last finished step, for the check of a script that is sent
/// ([`ti4_host::planning::script::check_script`]), and how many decisions the log had at that
/// step. `None` while no game is played.
pub(crate) fn script_copy() -> Option<(Game<'static>, usize)> {
    if DRAFT_BASE.with_borrow(Option::is_none) {
        let base = draft_base().ok()?;
        DRAFT_BASE.set(Some(base));
    }
    let at_checkpoint = CHECKPOINTS
        .with_borrow(|checkpoints| checkpoints.back().map(|checkpoint| checkpoint.decisions))?;
    DRAFT_BASE.with_borrow(|base| {
        let base = base.as_ref()?;
        // The steps since the checkpoint were played again on the base, each decision logged.
        Some((base.fork(), at_checkpoint + base.table.log.records.len()))
    })
}

/// Run `script` as the next turn of the host's seat on a copy of the game at its last finished
/// step.
pub(crate) fn draft_outcome(
    script: &[DraftStep],
    scope: &PlanScope,
) -> Result<ti4_host::planning::draft::DraftOutcome, Failure> {
    let player = host()
        .shown_seat()
        .ok_or_else(|| failed("no seat is played here: a draft is of one seat"))?;
    draft_outcome_of(&player, script, scope)
}

/// Run `script` of `player` on a copy of the game at its last finished step: the check of a
/// ready draft of a secondary ([`ti4_host::staged::ready`]).
pub(crate) fn draft_outcome_of(
    player: &PlayerId,
    script: &[DraftStep],
    scope: &PlanScope,
) -> Result<ti4_host::planning::draft::DraftOutcome, Failure> {
    if DRAFT_BASE.with_borrow(Option::is_none) {
        let base = draft_base()?;
        DRAFT_BASE.set(Some(base));
    }
    let latest = LATEST.lock().expect("latest lock");
    let latest = latest
        .as_ref()
        .ok_or_else(|| failed("no game is played: a draft needs one"))?;
    let outcome = DRAFT_BASE.with_borrow(|base| {
        let base = DraftBase {
            game: base.as_ref().expect("the base was made above"),
            content: ContentStore::embedded(),
            sources: POK,
            tiles: &latest.tiles,
            galaxy: latest.galaxy.as_ref(),
            secondary: Some(&SECONDARY),
        };
        run_draft(&base, player, scope, script)
    });
    Ok(outcome)
}

/// Whether the request buffer holds `request_len` bytes; else the error that says so.
fn request_filled(request_len: u32) -> Result<(), Failure> {
    if REQUEST
        .lock()
        .expect("request lock")
        .filled(request_len as usize)
    {
        return Ok(());
    }
    Err(failed(format_args!(
        "ti4-wasm buffer full: the request is {request_len} bytes, but REQUEST_CAPACITY is {} \
         bytes. Raise REQUEST_CAPACITY in crates/ti4-wasm/src/buffer.rs and rebuild, or make \
         the request smaller.",
        buffer::REQUEST_CAPACITY
    )))
}

/// The status of an export: a length, negated when the response buffer holds an error text.
fn status(result: Result<usize, Failure>) -> i32 {
    let length = |bytes: usize| i32::try_from(bytes).expect("a buffer is smaller than 2 GiB");
    match result {
        Ok(bytes) => length(bytes),
        Err(failure) => {
            let mut response = RESPONSE.lock().expect("response lock");
            response.clear();
            let _ = write!(response, "{failure}");
            -length(response.len())
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ti4_run_seeded(seed: u32, players: u32, max_steps: u32) -> i32 {
    status(run_seeded(
        u64::from(seed),
        players as usize,
        max_steps as usize,
    ))
}

/// What the options of a table are, with the maps of its number of seats.
#[derive(Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
struct TableOptions {
    #[serde(flatten)]
    options: setup::SetupOptions,
    /// The maps that build for the number of seats, the one that such a table starts with
    /// flagged. With no number: every map.
    maps: Vec<ti4_host::maps::MapTemplateSummary>,
}

/// What a table looks like before it starts.
#[derive(Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
struct TablePreview {
    /// What the table says of its map: its name, its author, its size.
    map: ti4_host::maps::MapChoiceView,
    /// The board, with a place for each seat and no faction.
    #[serde(flatten)]
    preview: ti4_host::maps::MapPreview,
}

/// What Start draws for a table, with the ids of this host.
#[derive(Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
struct TableDraw {
    /// The players in the order of their seats: the player of seat `i` is [`SEATS`]`[i]`.
    players: Vec<PlayerId>,
    #[serde(flatten)]
    drawn: setup::Drawn,
}

/// The request of [`ti4_setup_options`].
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
struct OptionsRequest {
    /// The number of seats whose maps are listed; 0 (or left out): every map.
    #[serde(default)]
    players: usize,
    /// The models that the page can load (from the server): each is the kind `mlp:<name>`.
    #[serde(default)]
    models: Vec<ti4_host::bots::ModelEntry>,
}

/// What a host can choose for a table, as JSON in the response buffer: the options of
/// `ti4_host::setup` (as `GET /api/setup-options` of the server gives them) and, as `maps`, the
/// maps that build for `players` seats (0: every map, as `GET /api/maps`). The request buffer
/// holds `request_len` bytes of JSON ([`OptionsRequest`]): `players`, and the `models` whose
/// kinds are listed. A model need not be loaded to be listed; it is loaded before a game that
/// seats it starts ([`ti4_model_load`]).
#[unsafe(no_mangle)]
pub extern "C" fn ti4_setup_options(request_len: u32) -> i32 {
    status(
        read_request::<OptionsRequest>(request_len).and_then(|request| {
            let players = request.players;
            let maps = ti4_host::maps::catalog::catalog((players > 0).then_some(players))
                .map_err(failed)?;
            respond(
                "the options of a table",
                &TableOptions {
                    options: setup::setup_options("The page", 0, &request.models),
                    maps,
                },
            )
        }),
    )
}

/// The request in the request buffer, `request_len` bytes of JSON.
fn read_request<T: serde::de::DeserializeOwned>(request_len: u32) -> Result<T, Failure> {
    request_filled(request_len)?;
    let request = REQUEST.lock().expect("request lock");
    serde_json::from_slice(request.as_bytes())
        .map_err(|error| failed(format_args!("the request is not read: {error}")))
}

/// The board of the table in the request buffer (`request_len` bytes of JSON: `setup`, and
/// `seats` in the order in which the players came), with a place for each seat.
#[unsafe(no_mangle)]
pub extern "C" fn ti4_setup_preview(request_len: u32) -> i32 {
    status(read_table(request_len).and_then(|table| {
        let preview = setup::preview(&table.setup).map_err(failed)?;
        respond(
            "the preview of a table",
            &TablePreview {
                map: ti4_host::maps::catalog::describe(&table.setup.map, table.setup.player_count),
                preview,
            },
        )
    }))
}

/// What Start draws for the table in the request buffer: the faction of every seat (for a scenario, the factions that it seats). It is what the start of the table
/// plays (`ti4_host_start` with `table`), so a host that shows it shows the game.
#[unsafe(no_mangle)]
pub extern "C" fn ti4_setup_draw(request_len: u32) -> i32 {
    status(read_table(request_len).and_then(|table| {
        let mut drawn = table.drawn().map_err(failed)?;
        if drawn.factions.is_empty() {
            // A scenario seats the factions of its seats: a host that shows the table before the
            // start names them, as the lobby of the server does.
            drawn.factions = (0..table.setup.player_count)
                .map(|seat| ti4_model::id::FactionId::new(ti4_engine::seating::seat_faction(seat)))
                .collect();
        }
        respond(
            "the draw of a table",
            &TableDraw {
                players: table.joined(),
                drawn,
            },
        )
    }))
}

/// The scenarios that a game can start from (`host_setup::Local::scenario`), as a JSON list in
/// the response buffer.
#[unsafe(no_mangle)]
pub extern "C" fn ti4_scenarios() -> i32 {
    status(respond(
        "the list of scenarios",
        &ti4_host::view::scenario::SCENARIOS,
    ))
}

/// The two sides of a space combat, as `{"query":"space_odds"}` of [`ti4_host_query`] reads
/// them: each a side of `ti4_calc::odds::SideInput` with no player. A side of
/// `MovementFacts.battle` is such a side.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub(crate) struct SpaceOddsRequest {
    attacker: ti4_calc::odds::SideInput,
    defender: ti4_calc::odds::SideInput,
    /// The combat has begun: the rollouts start at a combat round, with no space cannon and no
    /// anti-fighter barrage before it (they are behind the combat).
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    in_progress: bool,
}

/// The seed of every space odds query: the same sides give the same numbers.
const ODDS_SEED: u64 = 0;

/// The odds of a space combat: the default number of rollouts of `ti4_calc`, from
/// [`ODDS_SEED`]. Nothing of the game is read or changed: the host may ask while the game
/// waits, and with no game at all.
pub(crate) fn space_odds_of(
    request: &SpaceOddsRequest,
) -> Result<ti4_calc::odds::BattleResponse, ti4_calc::odds::CalcError> {
    ti4_calc::odds::space_odds_of(
        ContentStore::embedded(),
        POK,
        &request.attacker,
        &request.defender,
        ti4_calc::odds::SpaceOptions {
            seed: Some(ODDS_SEED),
            in_progress: Some(request.in_progress),
            ..Default::default()
        },
    )
}

/// Ask the host before each decision of a seat that it does not play (1), or let these seats
/// play on (0). The host is then asked with an update that has no pending choice, and answers
/// [`GO`]. With 2 the host is asked the same way, but not before a reaction window that the seat
/// only declines: this is for the wait of a table, which the host takes before it answers. The
/// seat decides the same in every mode, so the game is the same game.
///
/// Only a flag is set: the host may set it while the game waits (the local command `stepping`).
pub(crate) fn set_stepping(mode: u32) {
    STEPPING.store(mode.min(PACE), Ordering::Relaxed);
}

/// How many decisions the seats of the game made, the random seats included. While the game
/// waits, this is the number of the decision that it waits for.
pub(crate) fn decided() -> u64 {
    host().decided
}

/// From the decision with the number `decision` on, the host answers the seats of `human_mask`
/// and the others decide as seats that it does not play. The host then answers [`SWITCH`]: the
/// decision is asked again, of the seat's new decider. The answers of the host from before are
/// not taken back ([`Host::can_undo`]).
pub(crate) fn switch_seats(decision: u64, human_mask: u32) {
    host().switches.push(Switch {
        decision,
        mask: human_mask,
        asked: None,
    });
}

/// Whether an undo through the checkpoints reaches back to the answer of the host that had the
/// number `asked`.
pub(crate) fn undo_reaches(asked: u64) -> bool {
    let host = host();
    let fence = host.switch().and_then(|switch| switch.asked).unwrap_or(0);
    !host.left && host.oldest.is_some_and(|oldest| oldest.max(fence) < asked)
}

/// Where the host writes a request before the export that reads it. Constant for the life of
/// the instance; `REQUEST_CAPACITY` bytes.
#[unsafe(no_mangle)]
pub extern "C" fn ti4_request_ptr() -> *mut u8 {
    REQUEST.lock().expect("request lock").as_mut_ptr()
}

/// Where every export leaves its result or its error. Constant for the life of the instance.
#[unsafe(no_mangle)]
pub extern "C" fn ti4_response_ptr() -> *const u8 {
    RESPONSE.lock().expect("response lock").as_ptr()
}

// --- the hosted game (`host_api`) ---------------------------------------------------------------

/// Reserve `len` bytes of input for [`ti4_host_start`], [`ti4_host_command`] or
/// [`ti4_host_query`] and return where to write them. Valid until the next call of this export.
#[unsafe(no_mangle)]
pub extern "C" fn ti4_host_in(len: u32) -> *mut u8 {
    host_api::reserve_input(len as usize)
}

/// Where the last output is. Read it again after every call: it can move.
#[unsafe(no_mangle)]
pub extern "C" fn ti4_host_out_ptr() -> *const u8 {
    host_api::out_ptr()
}

/// The length of the last output.
#[unsafe(no_mangle)]
pub extern "C" fn ti4_host_out_len() -> u32 {
    host_api::out_len()
}

/// Tell the engine the time (`Date.now()` and `performance.now()`): events carry the time of day
/// of the last call. Call it before [`ti4_host_start`] and each time the parked game resumes.
#[unsafe(no_mangle)]
pub extern "C" fn ti4_host_set_time(unix_ms: f64, monotonic_ms: f64) {
    host_api::set_time(unix_ms as u64, monotonic_ms as u64);
}

/// A question that needs no game (the setup form, the catalogue, the odds of a combat): see
/// `host_api::query`. The host may ask while a game waits.
#[unsafe(no_mangle)]
pub extern "C" fn ti4_host_query(len: u32) -> i32 {
    host_api::query(len as usize)
}

/// Play a game: the input holds `{"setup" | "table" | "local": {...}, "resume": {...}?, ...}`
/// (`host_setup::Start`). Does not return until the game ends or the host leaves; with JSPI the
/// export is wrapped in `WebAssembly.promising`. A status of 0 or more is the length of the
/// final `end` output, below 0 the negated length of an error text in the output.
#[unsafe(no_mangle)]
pub extern "C" fn ti4_host_start(len: u32) -> i32 {
    match play_hosted(len as usize) {
        Ok(bytes) => i32::try_from(bytes).expect("an output is smaller than 2 GiB"),
        Err(failure) => host_api::fail(failure),
    }
}

/// A command while the game waits for the person (`ClientMessage` or a local command), or after
/// it ended. The output is a `command` envelope; with `resume` the host answers the parked `ask`
/// with that code. A status below 0 is the negated length of an error text in the output.
#[unsafe(no_mangle)]
pub extern "C" fn ti4_host_command(len: u32) -> i32 {
    host_api::command(len as usize)
}

/// Load an MLP model: the input (`ti4_host_in(len)`) holds the bytes of a `.ti4m` file. Its
/// trailer is checked and it is kept by its name, in place of a model of the same name; a game
/// with a seat of the kind `mlp:<name>` starts only when the model of the name is loaded. The
/// output is `{"name", "label", "sha256", "backend"}` (`sha256`: of the whole file). A status of
/// 0 or more is its length, below 0 the negated length of an error text in the output.
#[unsafe(no_mangle)]
pub extern "C" fn ti4_model_load(len: u32) -> i32 {
    host_api::load_model(len as usize)
}

/// The logits of a synthetic MLP model on the decisions of the input (`host_api::infer_check`),
/// for the test that the forward of this build gives the bits of the native one. A status of 0 or
/// more is the length of the answer, below 0 the negated length of an error text in the output.
#[unsafe(no_mangle)]
pub extern "C" fn ti4_infer_check(len: u32) -> i32 {
    host_api::infer_check(len as usize)
}

fn fill_request(request: &str) -> Result<(), String> {
    let mut buffer = REQUEST.lock().expect("request lock");
    buffer.clear();
    std::io::Write::write_all(&mut *buffer, request.as_bytes())
        .map_err(|_| "the request does not fit REQUEST_CAPACITY".to_owned())
}

/// Play at most `rounds` rounds from now on (`None`: as the wasm build does), for callers on the
/// native side: a test that compares two hosts does not need a whole game.
#[cfg(not(target_arch = "wasm32"))]
pub fn set_round_limit_native(rounds: Option<u32>) {
    ROUND_LIMIT.store(rounds.unwrap_or(ROUNDS), Ordering::Relaxed);
}

/// Every decision of the game that was played last, in order, for callers on the native side.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn decisions_native() -> Vec<ti4_engine::choice::DecisionRecord> {
    DECISIONS.lock().expect("decisions lock").clone()
}

/// How many decisions the seats that the host does not play have made so far (the rule bot and
/// the random seats), for callers on the native side: a game that is played again from a save
/// makes none of its saved decisions this way.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn picks_native() -> u64 {
    PICKS.load(Ordering::Relaxed)
}

/// Call a setup export (`ti4_setup_preview`, `ti4_setup_draw`) with `table` as its request, for
/// callers on the native side.
#[must_use]
pub fn with_table_native(export: extern "C" fn(u32) -> i32, table: &str) -> i32 {
    let length = if fill_request(table).is_ok() {
        table.len()
    } else {
        usize::MAX
    };
    export(u32::try_from(length).unwrap_or(u32::MAX))
}

/// The response of the last export, for callers on the native side.
///
/// # Errors
/// The error text, when `status` is negative.
pub fn response(status: i32) -> Result<String, String> {
    let response = RESPONSE.lock().expect("response lock");
    let text = String::from_utf8_lossy(response.as_bytes()).into_owned();
    if status < 0 { Err(text) } else { Ok(text) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_saved_free_form_offer_is_given_again() {
        // A transaction answered with a composed offer ("x;...") is not a listed id; a saved
        // game played again must accept it like the live game did (server runs s2-02-2037,
        // s1-03-2102 and s2-07-2050 failed the same way on their replays).
        let choice = Choice::new(
            PlayerId::new("p1"),
            "transaction with hacan",
            vec![ChoiceOption::labelled("decline", "decline", "decline")],
        )
        .accepting_free_form("x;");
        host().script = VecDeque::from(["x;rn=cf:hacan;".to_owned()]);
        let given = given(&choice);
        *host() = Host::default();
        assert_eq!(given.unwrap().unwrap().id, "x;rn=cf:hacan;");
    }

    #[test]
    fn slow_an_eight_player_game_runs_with_random_deciders() {
        let content = ContentStore::embedded();
        let mut game = new_game(content, 3, 8, None).unwrap();
        assert_eq!(game.state.players.len(), 8);
        match game.run(ROUNDS, 2000) {
            Ok(_) | Err(RunError::StepLimit { .. }) => {}
            Err(error) => panic!("{error}"),
        }
        assert!(game.state.round > 1, "the game left its first round");
    }

    #[test]
    fn the_scenarios_run_with_random_deciders() {
        let content = ContentStore::embedded();
        for scenario in ti4_host::view::scenario::SCENARIOS {
            let mut game =
                new_game(content, scenario.seed, scenario.players, Some(scenario.id)).unwrap();
            match game.run(ROUNDS, 400) {
                Ok(_) | Err(RunError::StepLimit { .. }) => {}
                Err(error) => panic!("{}: {error}", scenario.id),
            }
            assert!(
                game.state.finished || game.table.log.records.len() > 100,
                "{}: the seats decided",
                scenario.id
            );
        }
    }

    /// The odds of a space combat need no game: the sides of the request are all there is, and
    /// the same sides give the same numbers.
    #[test]
    fn the_odds_of_a_space_combat_come_from_the_sides_alone() {
        let odds_of = |request: &[u8]| -> Result<ti4_calc::odds::BattleResponse, String> {
            let request: SpaceOddsRequest =
                serde_json::from_slice(request).map_err(|error| error.to_string())?;
            space_odds_of(&request).map_err(|error| error.to_string())
        };
        let request = serde_json::json!({
            "attacker": { "faction": "sol", "units": { "dreadnought": 2, "carrier": 1, "fighter": 3 } },
            "defender": {
                "faction": "letnev",
                "units": { "cruiser": 2, "destroyer": 1 },
                "damaged": {},
                "guns": { "pds": 1 },
            },
        })
        .to_string();
        let odds = odds_of(request.as_bytes()).unwrap();
        assert_eq!(odds.simulations, ti4_calc::odds::DEFAULT_SIMULATIONS);
        assert!(odds.attacker_win_rate > odds.defender_win_rate, "{odds:?}");
        assert_eq!(odds.attacker_fielded.get("dreadnought"), Some(&2));
        assert_eq!(odds.defender_fielded.get("cruiser"), Some(&2));
        assert!(odds.attacker_expected_survivors["dreadnought"] <= 2.0);
        assert_eq!(odds, odds_of(request.as_bytes()).unwrap());

        let wire: serde_json::Value = serde_json::to_value(&odds).unwrap();
        for key in [
            "attacker_win_rate",
            "defender_win_rate",
            "mutual_destruction_rate",
            "unresolved_rate",
            "attacker_expected_survivors",
            "defender_expected_survivors",
            "attacker_fielded",
            "defender_fielded",
        ] {
            assert!(wire.get(key).is_some(), "no {key} in {wire}");
        }

        for wrong in [
            serde_json::json!({ "attacker": { "player": "a" }, "defender": {} }),
            serde_json::json!({ "attacker": { "units": { "nonexistent": 1 } }, "defender": {} }),
            serde_json::json!({ "attacker": {}, "defender": {}, "simulations": 5 }),
            serde_json::json!({ "attacker": {} }),
        ] {
            assert!(odds_of(wrong.to_string().as_bytes()).is_err(), "{wrong}");
        }
    }

    /// A seat that draws from a stream which a checkpoint can copy.
    struct Shared(std::sync::Arc<Mutex<ti4_engine::choice::SeededRandom>>);

    impl Decider for Shared {
        fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
            self.0.lock().unwrap().choose(choice)
        }
    }

    /// `Game::fork` at a step boundary, with a copy of the random seats' stream, is a full
    /// checkpoint: played on, it makes the decisions of the game it was taken from and reaches
    /// the same state. A game made again from the state alone does not.
    ///
    /// Prints how often a window is open at a step boundary and what a fork costs:
    /// `cargo test -p ti4-wasm fork -- --nocapture`. The numbers in the README are from seeds 3
    /// and 11 with 700 steps, a copy every 7 steps, played 30 steps on.
    #[test]
    fn a_fork_at_a_step_boundary_plays_the_same_game() {
        use std::sync::Arc;
        use std::time::{Duration, Instant};
        use ti4_engine::choice::{SeededRandom, Table};

        const STEPS: usize = 300;
        const EVERY: usize = 10;
        const AHEAD: usize = 20;
        /// A copy of the game at one step, played `AHEAD` steps on.
        struct Probe {
            step: usize,
            from: usize,
            decisions: Vec<ti4_engine::choice::DecisionRecord>,
            state: String,
        }
        let content = ContentStore::embedded();
        let state_of =
            |game: &Game<'_>| serde_json::to_string(&game.state).expect("a state serializes");
        let ahead = |mut copy: Game<'static>, step: usize, from: usize| {
            for _ in 0..AHEAD {
                if copy.state.finished || copy.step().error.is_some() {
                    break;
                }
            }
            Probe {
                step,
                from,
                decisions: copy.table.log.records.clone(),
                state: state_of(&copy),
            }
        };

        for seed in [3] {
            let mut game = new_game(content, seed, 8, None).unwrap();
            let stream = Arc::new(Mutex::new(SeededRandom::new(seed)));
            game.table = Table::with_default(Box::new(Shared(stream.clone())));
            let table = || {
                let copy = stream.lock().unwrap().clone();
                Table::with_default(Box::new(Shared(Arc::new(Mutex::new(copy)))))
            };

            let (mut forks, mut remade) = (Vec::new(), Vec::new());
            let (mut quiet, mut open) = (0, std::collections::BTreeMap::new());
            let (mut fork_time, mut largest) = (Duration::ZERO, 0);
            let mut states = Vec::new();
            for step in 0..STEPS {
                if game.state.finished {
                    break;
                }
                states.push(state_of(&game));
                let windows = game.open_windows();
                if windows.is_empty() {
                    quiet += 1;
                }
                for window in &windows {
                    *open.entry(*window).or_insert(0) += 1;
                }
                // A copy is compared over `AHEAD` steps, so the run must go that far.
                if step % EVERY == 0 && step + AHEAD < STEPS {
                    let from = game.table.log.records.len();
                    let start = Instant::now();
                    let mut fork = game.fork();
                    fork_time += start.elapsed();
                    largest = largest.max(states[step].len());
                    fork.table = table();
                    forks.push(ahead(fork, step, from));
                    // What the server's snapshot holds: the state, and nothing else of the game.
                    let again = Game::with_table(game.state.clone(), content, table())
                        .with_sources(POK)
                        .with_galaxy(game.galaxy().unwrap().clone());
                    remade.push((ahead(again, step, from), windows.is_empty()));
                }
                assert!(game.step().error.is_none());
            }
            states.push(state_of(&game));

            let log = &game.table.log.records;
            let same = |probe: &Probe| {
                let end = probe.from + probe.decisions.len();
                log.get(probe.from..end) == Some(&probe.decisions[..])
                    && states.get(probe.step + AHEAD) == Some(&probe.state)
            };
            let wrong: Vec<usize> = forks
                .iter()
                .filter(|probe| !same(probe))
                .map(|probe| probe.step)
                .collect();
            let count = |quiet_only: bool| {
                let of: Vec<_> = remade
                    .iter()
                    .filter(|(_, quiet)| *quiet || !quiet_only)
                    .collect();
                (of.iter().filter(|(probe, _)| same(probe)).count(), of.len())
            };
            println!(
                "seed {seed}: {} steps, {} decisions; no window open at {quiet} boundaries; open: {open:?}",
                states.len() - 1,
                log.len()
            );
            println!(
                "  forks: {} of {} play the same game; one fork takes {:?}; the state is up to {largest} bytes of JSON",
                forks.len() - wrong.len(),
                forks.len(),
                fork_time / u32::try_from(forks.len()).unwrap()
            );
            println!(
                "  made again from the state alone: {:?} the same; at boundaries with no window open: {:?}",
                count(false),
                count(true)
            );
            assert!(
                wrong.is_empty(),
                "forks at steps {wrong:?} played another game"
            );
        }
    }

    /// The setup of a table needs no game: its options, its board, its draw, and the odds of a
    /// combat are read through the buffers of the exports.
    #[test]
    fn a_table_is_described_previewed_and_drawn() {
        // The table of a page. Its options are those of the server, with the maps of the count.
        {
            let json = |status: i32| -> serde_json::Value {
                serde_json::from_str(&response(status).unwrap()).unwrap()
            };
            let model = ti4_host::bots::test_model();
            let request = |players: usize| {
                serde_json::json!({ "players": players, "models": [model.entry()] }).to_string()
            };
            let options = json(with_table_native(ti4_setup_options, &request(6)));
            assert_eq!(options["strategy_card_sets"].as_array().unwrap().len(), 3);
            assert_eq!(options["faction_groups"].as_array().unwrap().len(), 2);
            let kinds: Vec<(&str, &str)> = options["bot_kinds"]
                .as_array()
                .unwrap()
                .iter()
                .map(|kind| (kind["id"].as_str().unwrap(), kind["name"].as_str().unwrap()))
                .collect();
            assert_eq!(
                kinds,
                [
                    ("smart", "Rule bot"),
                    ("random", "Random"),
                    ("mlp:test", "Test model")
                ]
            );
            let maps = options["maps"].as_array().unwrap();
            assert!(maps.iter().all(|map| map["player_count"] == 6));
            assert_eq!(
                maps.iter().filter(|map| map["recommended"] == true).count(),
                1
            );
            let recommended = maps
                .iter()
                .find(|map| map["recommended"] == true)
                .and_then(|map| map["alias"].as_str())
                .unwrap()
                .to_owned();
            assert!(
                json(with_table_native(ti4_setup_options, &request(0)))["maps"]
                    .as_array()
                    .unwrap()
                    .len()
                    > maps.len()
            );
            assert_eq!(
                json(with_table_native(ti4_setup_options, "{}"))["bot_kinds"]
                    .as_array()
                    .unwrap()
                    .len(),
                2
            );

            let table = |seed: u64, kind: &str| {
                serde_json::json!({
                    "setup": {
                        "player_count": 6,
                        "seed": seed,
                        "map": { "kind": "template", "alias": recommended },
                        "strategy_cards": "pok",
                    },
                    "seats": [
                        { "kind": kind, "faction": "hacan" },
                        { "kind": "smart" },
                        { "kind": "smart", "faction": "ghost" },
                        { "kind": "random" },
                        { "kind": "smart" },
                        { "kind": "random", "faction": null },
                    ],
                })
                .to_string()
            };
            let preview = json(with_table_native(ti4_setup_preview, &table(5, "human")));
            assert_eq!(preview["map"]["alias"], recommended.as_str());
            assert_eq!(preview["seats"].as_array().unwrap().len(), 6);
            assert!(!preview["tiles"].as_array().unwrap().is_empty());

            // The players sit in the order of the table, and a chosen faction stays with its
            // player; the others are drawn from the seed.
            let draw = json(with_table_native(ti4_setup_draw, &table(5, "human")));
            let players: Vec<&str> = draw["players"]
                .as_array()
                .unwrap()
                .iter()
                .map(|id| id.as_str().unwrap())
                .collect();
            assert_eq!(players, &SEATS[..6]);
            assert_eq!(draw["factions"][0], "hacan");
            assert_eq!(draw["factions"][2], "ghost");
            assert!(
                (0..20).any(|seed| {
                    json(with_table_native(ti4_setup_draw, &table(seed, "human")))["factions"]
                        != draw["factions"]
                }),
                "some seed draws other factions"
            );

            // What a table cannot be is refused, with the reason.
            for (wrong, reason) in [
                (r#"{"setup":{"player_count":9,"seed":1}}"#, "player_count"),
                (
                    r#"{"setup":{"player_count":4,"seed":1,"strategy_cards":"base_game"}}"#,
                    "strategy_card_set",
                ),
                (
                    r#"{"setup":{"player_count":4,"seed":1},"seats":[{},{},{},{},{}]}"#,
                    "5 players",
                ),
                (
                    r#"{"setup":{"player_count":4,"seed":1},"seats":[{"faction":"sol"},{"faction":"sol"}]}"#,
                    "sol",
                ),
                (r#"{"setup":{"player_count":4}}"#, "seed"),
            ] {
                let error = response(with_table_native(ti4_setup_draw, wrong)).unwrap_err();
                assert!(error.contains(reason), "{wrong}: {error}");
            }
        }
    }
}
