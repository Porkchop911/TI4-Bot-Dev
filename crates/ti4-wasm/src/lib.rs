//! The engine behind a wasm boundary, for local play in `web2/`.
//! See `README.md` for the measurements.
//!
//! The boundary is the plain C ABI rather than `wasm-bindgen`. An export returns a status: zero
//! or more is the length of a JSON result, less than zero is the negated length of an error text.
//! Either is read from the response buffer, whose address never changes ([`ti4_response_ptr`]).
//!
//! # Memory
//!
//! This crate does not allocate for its own data: results and the update for the host (the view
//! of the asked seat and its choice) are written straight into the fixed buffers of [`buffer`]. What still allocates
//! here is what the interfaces of the engine and of `ti4-view` ask for (owned ids, the decider
//! box, the copy of the state, a projected view) and the text of an error.

pub mod buffer;
pub mod host_api;
pub mod host_session;
pub mod host_setup;

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
use ti4_host::setup::{self, TableSetup, bot_kind};
use ti4_model::content_types::POK;
use ti4_model::id::PlayerId;
use ti4_model::state::{GameState, ReactionMode};
use ti4_view::components::catalog;
use ti4_view::map::{build_board_tiles, create_game_with_template};
use ti4_view::maps::{TemplateLoader, default_template_for};
use ti4_view::planning::audit::PlanScope;
use ti4_view::planning::draft::{DraftBase, DraftStep, ScriptAnswer, run_draft, script_answer};
use ti4_view::projection::{FactsContext, project_game_view_full, project_session_update};
use ti4_view::reaction_modes::{NeverOffer, NeverSet};
use ti4_view::secondary::SecondaryWatch;
use ti4_view::status::ViewerRole;
use ti4_view::view::{BoardTileView, GameView};

use crate::buffer::{DRAFT, REQUEST, RESPONSE, TooLarge, UPDATE, store};

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
/// scenario, the board of that scenario is set up on it (`ti4_view::scenario`).
fn new_game(
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
        ti4_view::scenario::start(content, scenario, &ids).map_err(failed)?
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

/// Who plays a seat of a table ([`TableRequest`]), by the names of a lobby.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
enum SeatKind {
    /// The host answers the seat.
    #[default]
    #[serde(rename = "human")]
    Human,
    /// The rule bot (`bot_kind::RULE`).
    #[serde(rename = "smart")]
    Rule,
    /// A seat that decides at random (`bot_kind::RANDOM`).
    #[serde(rename = "random")]
    Random,
}

/// One player of a table: who plays the seat, and the faction that was chosen for it.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SeatRequest {
    #[serde(default)]
    kind: SeatKind,
    #[serde(default)]
    faction: Option<String>,
    /// The name of the player at the table. With a name for every player, the updates of the
    /// game say who sits in each seat ([`TABLE_SEATS`]).
    #[serde(default)]
    name: Option<String>,
}

impl SeatKind {
    /// The kind as a fact of the update names it (`ti4_host::setup::SeatFact::kind`).
    const fn as_str(self) -> &'static str {
        match self {
            Self::Human => setup::HUMAN_KIND,
            Self::Rule => bot_kind::RULE,
            Self::Random => bot_kind::RANDOM,
        }
    }
}

/// The table of the page, as the setup exports read it from the request buffer: what the host
/// chose (`ti4_host::setup::TableSetup`), and its players in the order in which they came.
/// The player who came as number `i` has the id of [`SEATS`]`[i]`, wherever the draw seats it:
/// the masks of this host name a player by that letter.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct TableRequest {
    setup: TableSetup,
    /// Missing entries are seats of the host with no chosen faction.
    #[serde(default)]
    seats: Vec<SeatRequest>,
}

impl TableRequest {
    /// The table of the request buffer, with its values checked.
    fn read(request_len: u32) -> Result<Self, Failure> {
        request_filled(request_len)?;
        let mut table: Self = {
            let request = REQUEST.lock().expect("request lock");
            serde_json::from_slice(request.as_bytes())
                .map_err(|error| failed(format_args!("the table is not read: {error}")))?
        };
        table.setup.check().map_err(failed)?;
        let count = table.setup.player_count;
        if count > SEATS.len() || table.seats.len() > count {
            return Err(failed(format_args!(
                "a table of {count} seats has {} players",
                table.seats.len()
            )));
        }
        table.seats.resize_with(count, SeatRequest::default);
        // The values of a kind are the names of the setup options.
        debug_assert_eq!((bot_kind::RULE, bot_kind::RANDOM), ("smart", "random"));
        Ok(table)
    }

    /// The ids of the players in the order in which they came.
    fn joined(&self) -> Vec<PlayerId> {
        SEATS[..self.setup.player_count]
            .iter()
            .map(|name| PlayerId::new(*name))
            .collect()
    }

    fn drawn(&self) -> Result<setup::Drawn, Failure> {
        let chosen: Vec<Option<String>> =
            self.seats.iter().map(|seat| seat.faction.clone()).collect();
        setup::draw(&self.setup, &chosen).map_err(failed)
    }

    /// Who sits in each seat, by the id of its player. `None` when a player has no name: a
    /// table of an older page.
    fn seats(&self) -> Option<setup::TableSeats> {
        self.joined()
            .into_iter()
            .zip(&self.seats)
            .map(|(player, seat)| {
                Some((
                    player,
                    setup::SeatFact {
                        nickname: seat.name.clone()?,
                        kind: seat.kind.as_str().to_owned(),
                    },
                ))
            })
            .collect()
    }

    /// The players of `kind` as a mask (bit 0 is the player who came first).
    fn mask(&self, kind: SeatKind) -> u32 {
        self.seats
            .iter()
            .enumerate()
            .filter(|(_, seat)| seat.kind == kind)
            .fold(0, |mask, (place, _)| mask | (1 << place))
    }

    /// The game that the table starts, as the server starts it for the table of a lobby.
    fn game(&self, content: &'static ContentStore) -> Result<Game<'static>, Failure> {
        let drawn = self.drawn()?;
        let players = drawn.seated(&self.joined());
        let (state, galaxy) =
            setup::create(content, &self.setup, &players, &drawn).map_err(failed)?;
        Ok(
            Game::with_seeded_random(state, content, self.setup.game_seed())
                .with_sources(POK)
                .with_galaxy(galaxy),
        )
    }
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

/// What the facts of an update of this host are made from: its games are of one content.
fn facts_context(galaxy: Option<&Galaxy>) -> FactsContext<'_> {
    FactsContext {
        content: ContentStore::embedded(),
        sources: POK,
        galaxy,
        secondary: Some(&SECONDARY),
    }
}

/// Who sits in each seat of the game that is played, when it is the game of a table whose
/// players have names ([`ti4_play_setup`]). It is a fact of every update of that game.
static TABLE_SEATS: Mutex<Option<setup::TableSeats>> = Mutex::new(None);

/// [`project_session_update`] for the host: the update of the game, with the seats of its table.
fn host_update(
    state: &GameState,
    viewer: &ViewerRole,
    pending_choice: Option<(&Choice, &str)>,
    tiles: &[BoardTileView],
    galaxy: Option<&Galaxy>,
) -> ti4_view::projection::SessionUpdate {
    let mut update =
        project_session_update(state, viewer, pending_choice, tiles, &facts_context(galaxy));
    update
        .seats
        .clone_from(&TABLE_SEATS.lock().expect("table seats lock"));
    update
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
    /// An update did not fit its buffer. The engine's decider error carries text only, and the
    /// size must reach the host intact.
    too_large: Option<TooLarge>,
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
    /// The seed of the game: a rule seat decides from it ([`ti4_rule_bot::choose`]).
    seed: u64,
    /// The game is of a table ([`ti4_play_setup`]): a random seat decides as on the server, from
    /// the seed, the seat and the number of the decision, not from the stream of [`RANDOM`].
    table: bool,
    /// The game waits before a decision of a random seat ([`ti4_set_stepping`]), not in a choice.
    paused: bool,
    /// How many decisions the seats made, the random seats included. A checkpoint keeps it.
    decided: u64,
    /// The changes of the host's seats, in order ([`ti4_switch_seats`]).
    switches: Vec<Switch>,
    /// The host changed its seats in place of an answer: the decision is asked again.
    switched: bool,
    /// How many decisions of the game have their answer in a saved game ([`ti4_replay`]): the
    /// first ones, of every seat. The host gives each of them ([`saved`]).
    saved: u64,
    /// Hosted game: how many of the answers in `script` are those of a saved game that is played
    /// again (the rest are answers after an undo). They are kept for a later undo, and their
    /// progress is told to the host.
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
    too_large: None,
    undo: false,
    script: VecDeque::new(),
    recent: VecDeque::new(),
    oldest: None,
    players: 0,
    humans: 0,
    seed: 0,
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

/// The printed names of the action cards that the host is never offered
/// ([`ti4_set_reaction_mode`]). A setting of the host, not of the game.
pub(crate) static NEVER: LazyLock<NeverSet> =
    LazyLock::new(|| Arc::new(Mutex::new(BTreeMap::new())));

/// A window that was passed for the host: the option that declined it, and the cards.
#[derive(Serialize)]
struct Passed {
    option: ChoiceOption,
    cards: Vec<String>,
}

/// A decision that was made without asking the host: its answer, and the window when it was
/// passed for the host.
#[derive(Serialize)]
struct Made {
    id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    passed: Option<Passed>,
}

/// The decisions that were made without asking the host since it last read them, in order
/// ([`ti4_journal`]): what a seat decided that the host does not play, a window that was passed
/// for the host, and an answer that is given again after an undo.
static JOURNAL: Mutex<Vec<Made>> = Mutex::new(Vec::new());

/// Keep a decision that the host was not asked for [`JOURNAL`].
fn journal(option: &ChoiceOption) {
    // A hosted game is saved by its session ([`host_api`]); nobody reads the journal there.
    if host_api::hosted() {
        return;
    }
    JOURNAL.lock().expect("journal lock").push(Made {
        id: option.id.clone(),
        passed: None,
    });
}

/// The answer of the host that lets a random seat decide. See [`ti4_set_stepping`].
pub const GO: i32 = -3;

/// Whether the host is asked before a decision of a seat that it does not play: [`RUN`], 1
/// (before each decision) or [`PACE`]. See [`ti4_set_stepping`].
static STEPPING: AtomicU32 = AtomicU32::new(RUN);

/// The seats that the host does not play decide with no stop.
const RUN: u32 = 0;
/// The host is asked before each of their decisions, but for a reaction window that the seat
/// only declines: the wait of a table, as the server has it.
const PACE: u32 = 2;

/// The answer of the host after [`ti4_switch_seats`]: the decision is asked again, of the seat's
/// new decider.
pub const SWITCH: i32 = -4;

/// The answer of the host that takes back its last answer. See [`ti4_can_undo`].
pub const UNDO: i32 = -2;

/// How many checkpoints are kept: one after each step that asked the host. An answer older than
/// these is taken back by playing the game again from its seed, which the host does itself.
const CHECKPOINT_LIMIT: usize = 32;

/// The seats that the rule bot plays where the host does not ([`ti4_set_rule_seats`]).
static RULE_SEATS: AtomicU32 = AtomicU32::new(0);

/// The stream of the seats that decide at random. It is here, not inside the table, so that a
/// checkpoint can keep its position.
static RANDOM: Mutex<Option<SeededRandom>> = Mutex::new(None);

/// Every seat of a played game: the host answers the seats of its mask, the rule bot those of
/// [`RULE_SEATS`], and the others draw from [`RANDOM`]. The mask can change during the game ([`ti4_switch_seats`]), so the decider
/// of a seat is found at each decision.
struct Seat {
    /// The host, behind its Never cards: a window that offers only such cards is declined
    /// without asking, and any other window is asked without them. The same wrapper as on the
    /// server (`ti4_view::reaction_modes`).
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
            let mut host = host();
            host.asked += 1;
            let number = host.asked;
            host.recent.push_back((number, option.id.clone(), true));
            if host_api::hosted() {
                host_api::passed_window(&choice.player, cards);
            } else {
                JOURNAL.lock().expect("journal lock").push(Made {
                    id: option.id.clone(),
                    passed: Some(Passed {
                        option: option.clone(),
                        cards: cards.to_vec(),
                    }),
                });
            }
        });
        Self { host }
    }
}

impl Seat {
    /// With `seen`, the seat can be played by the rule bot.
    fn decide(
        &mut self,
        choice: &Choice,
        seen: Option<&SeatObservation<'_>>,
    ) -> Result<ChoiceOption, IllegalChoice> {
        loop {
            let ours = host().plays(&choice.player);
            let rule = seen.filter(|_| rule_seat(&choice.player));
            let replayed = {
                let host = host();
                host.decided < host.saved
            };
            let answer = if replayed {
                // A decision of a saved game is given below the Never cards and with no bot, as
                // a replay on the server is: the game is the same whatever the cards are set to
                // now, and whoever plays the seat.
                Some(saved(choice, ours, rule.is_some())?)
            } else if !ours {
                let picked = draw(choice, rule)?;
                picked.iter().for_each(journal);
                picked
            } else if let Some(option) = given(choice)? {
                journal(&option);
                Some(option)
            } else if host_api::hosted()
                && let Some(option) = scripted(choice)?
            {
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
                host().decided += 1;
                if host_api::hosted() {
                    host_api::selected(choice, &option);
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

/// Whether the rule bot plays the seat of `player` when the host does not.
fn rule_seat(player: &PlayerId) -> bool {
    SEATS
        .iter()
        .position(|seat| *seat == player.as_str())
        .is_some_and(|index| RULE_SEATS.load(Ordering::Relaxed) & (1 << index) != 0)
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
        .option(&wanted)
        .cloned()
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

/// A decision of a saved game ([`ti4_replay`]): the host gives its answer, whoever plays the
/// seat. The host is asked with an empty update, and finds the option with
/// [`ti4_saved_option`]. Nothing is shown and no seat waits; what the host counts is counted as
/// in a game that is not saved, so the choices after it have the same numbers. `ours`: the host
/// plays the seat. `rule`: the rule bot would decide it.
fn saved(choice: &Choice, ours: bool, rule: bool) -> Result<ChoiceOption, IllegalChoice> {
    let failed = |reason: &str| IllegalChoice::DeciderFailed {
        player: choice.player.clone(),
        prompt: choice.prompt.clone(),
        reason: reason.to_owned(),
    };
    {
        let host = host();
        if host.left {
            return Err(failed(LEFT));
        }
        if host.undo {
            return Err(failed(TAKEN_BACK));
        }
    }
    UPDATE.lock().expect("update lock").clear();
    PENDING.set(Some(choice.clone()));
    let index = host_ask();
    PENDING.take();
    let mut host = host();
    let Some(option) = usize::try_from(index)
        .ok()
        .and_then(|index| choice.options.get(index))
        .cloned()
    else {
        host.left = true;
        return Err(failed(LEFT));
    };
    if ours {
        host.asked += 1;
        host.viewer = Some(choice.player.clone());
        let number = host.asked;
        host.recent.push_back((number, option.id.clone(), false));
    } else if !(rule || host.table) {
        // The seat would have drawn from the stream of the random seats: it moves as it did.
        drop(host);
        if let Some(stream) = RANDOM.lock().expect("random lock").as_mut() {
            let _ = stream.choose(choice);
        }
    }
    Ok(option)
}

/// A decision of a seat that the host does not play: of the rule bot with `rule`, else from the
/// stream that all random seats share, in decision order. None
/// when the host changed its seats while the seat waited for it.
fn draw(
    choice: &Choice,
    rule: Option<&SeatObservation<'_>>,
) -> Result<Option<ChoiceOption>, IllegalChoice> {
    let stepping = STEPPING.load(Ordering::Relaxed);
    if stepping != RUN {
        // What the seat picks is known before the host is asked, as on the server: the wait of a
        // table is not taken for a window that is only declined.
        let declines = stepping == PACE
            && ti4_view::projection::is_reaction_window(choice)
            && pick(choice, rule, true)?.is_decline();
        if !declines && !wait_for_go(choice)? {
            return Ok(None);
        }
    }
    pick(choice, rule, false).map(Some)
}

/// What a seat that the host does not play answers. With `peek` the stream of the random seats
/// stays where it is, so the same call gives the same answer again.
fn pick(
    choice: &Choice,
    rule: Option<&SeatObservation<'_>>,
    peek: bool,
) -> Result<ChoiceOption, IllegalChoice> {
    let (seed, index, table) = {
        let host = host();
        (host.seed, host.decided, host.table)
    };
    if (rule.is_some() || table) && !choice.options.is_empty() {
        // The seed, the seat and the number of the decision give the answer, so a checkpoint
        // keeps nothing for it, and the stream of the random seats does not move. In the game
        // of a table a random seat decides the same way, as on the server.
        let index = usize::try_from(index).expect("a game has fewer decisions than 2^32");
        let (picked, _) = ti4_rule_bot::choose(choice, rule, seed, &choice.player, index);
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
            .map_or(ViewerRole::Spectator, ViewerRole::Player)
    };
    let shown = show(&viewer, choice, "");
    PENDING.set(Some(choice.clone()));
    let index = if shown.is_ok() { host_ask() } else { LEAVE };
    PENDING.take();
    UPDATE.lock().expect("update lock").clear();
    let mut host = host();
    if let Err(error) = shown {
        host.too_large = Some(error);
    }
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
    /// The choice that the game is parked in, as the host is asked it: what a plan answers
    /// ([`ti4_plan`]). Before a decision of a random seat, the choice of that seat.
    static PENDING: RefCell<Option<Choice>> = const { RefCell::new(None) };
}

/// The game between two steps, with what the game does not hold itself.
struct Checkpoint {
    game: Game<'static>,
    random: Option<SeededRandom>,
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
        if host_api::hosted() {
            host_api::offer(records, state);
        }
    });
    table.on_auto_resolved(|note| {
        if host_api::hosted() {
            host_api::auto_resolved(&note.player, &note.prompt, &note.label, &note.reason);
        }
    });
    table
}

fn host() -> std::sync::MutexGuard<'static, Host> {
    HOST.lock().expect("host lock")
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
    if host_api::hosted() {
        let can_undo = host().can_undo();
        let mut number = NonceText::default();
        let _ = write!(number, "{nonce}");
        let state = LATEST
            .lock()
            .expect("latest lock")
            .as_ref()
            .map(|latest| latest.state.clone());
        if let Some(state) = state {
            host_api::ask(choice, number.as_str(), &state, can_undo);
        }
    } else if let Err(error) = offer(choice, nonce) {
        let mut host = host();
        host.too_large = Some(error);
        host.left = true;
        return Err(failed("what the host is shown did not fit its buffer"));
    }
    // No lock is held while the host is asked: it reads the update while this stack is parked.
    PENDING.set(Some(choice.clone()));
    let index = host_ask();
    PENDING.take();
    UPDATE.lock().expect("update lock").clear();
    if host_api::hosted() {
        host_api::answered();
    }
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
            "the host took back an answer that no checkpoint holds; ti4_can_undo says when one does"
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

/// Write what the asked seat is shown, and its choice, into the update buffer.
fn offer(choice: &Choice, nonce: u64) -> Result<(), TooLarge> {
    let mut number = NonceText::default();
    let _ = write!(number, "{nonce}");
    show(
        &ViewerRole::Player(choice.player.clone()),
        choice,
        number.as_str(),
    )
}

/// Write what `viewer` is shown while the game waits in `choice` into the update buffer. The
/// choice itself is in the update only when it is the viewer's.
fn show(viewer: &ViewerRole, choice: &Choice, nonce: &str) -> Result<(), TooLarge> {
    let latest = LATEST.lock().expect("latest lock");
    let Some(latest) = latest.as_ref() else {
        return Ok(());
    };
    let update = host_update(
        &latest.state,
        viewer,
        Some((choice, nonce)),
        &latest.tiles,
        latest.galaxy.as_ref(),
    );
    store(
        &mut UPDATE.lock().expect("update lock"),
        "the update for the host",
        "UPDATE_CAPACITY",
        &update,
    )?;
    Ok(())
}

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

/// The host of a native run: it is given the update and answers like the wasm import.
#[cfg(not(target_arch = "wasm32"))]
type NativeHost = Box<dyn FnMut(&str) -> i32 + Send>;

#[cfg(not(target_arch = "wasm32"))]
static NATIVE_HOST: Mutex<Option<NativeHost>> = Mutex::new(None);

/// Answer the choices of [`ti4_play`] in a native run. Without a host the first option is taken.
#[cfg(not(target_arch = "wasm32"))]
pub fn set_native_host(host: impl FnMut(&str) -> i32 + Send + 'static) {
    *NATIVE_HOST.lock().expect("native host lock") = Some(Box::new(host));
}

#[cfg(not(target_arch = "wasm32"))]
fn host_ask() -> i32 {
    let shown = if host_api::hosted() {
        host_api::out_text()
    } else {
        update()
    };
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

/// The decisions of the game that was played last, for callers on the native side.
#[cfg(not(target_arch = "wasm32"))]
static DECISIONS: Mutex<Vec<ti4_engine::choice::DecisionRecord>> = Mutex::new(Vec::new());

/// Play a seeded game of `players` seats to its end. A seat whose bit is set in `human_mask`
/// (bit 0 is seat `a`) is answered by the host; the other seats decide at random.
fn play(
    seed: u64,
    players: usize,
    human_mask: u32,
    scenario: Option<&str>,
) -> Result<usize, Failure> {
    let game = new_game(ContentStore::embedded(), seed, players, scenario)?;
    drive(game, seed, players, human_mask, false, None)
}

/// Play the game of a table to its end ([`ti4_play_setup`]): the host answers its players, the
/// rule bot and the random seats decide as on the server.
fn play_setup(request_len: u32) -> Result<usize, Failure> {
    let table = TableRequest::read(request_len)?;
    let game = table.game(ContentStore::embedded())?;
    // The rule seats are of this game: the mask of the host is back when it is over.
    let before = RULE_SEATS.swap(table.mask(SeatKind::Rule), Ordering::Relaxed);
    *TABLE_SEATS.lock().expect("table seats lock") = table.seats();
    let played = drive(
        game,
        table.setup.game_seed(),
        table.setup.player_count,
        table.mask(SeatKind::Human),
        true,
        None,
    );
    RULE_SEATS.store(before, Ordering::Relaxed);
    *TABLE_SEATS.lock().expect("table seats lock") = None;
    played
}

/// Play a hosted game ([`host_api`]) to its end, or until the host leaves.
fn play_hosted(request_len: usize) -> Result<usize, Failure> {
    let prepared = host_api::prepare(request_len).map_err(Failure::Setup)?;
    RULE_SEATS.store(prepared.rule_seats, Ordering::Relaxed);
    drive(
        prepared.game,
        prepared.seed,
        prepared.players,
        prepared.human_mask,
        false,
        Some(prepared.answers),
    )
}

/// The play loop. `game` is a new game; a seat of `human_mask` is answered by the host. `table`:
/// see [`Host::table`]. With `saved` the game is a hosted one: those answers of the person are
/// played again without asking, and the session of [`host_api`] follows the game.
#[allow(clippy::too_many_lines)]
fn drive(
    mut game: Game<'static>,
    seed: u64,
    players: usize,
    human_mask: u32,
    table: bool,
    saved: Option<VecDeque<String>>,
) -> Result<usize, Failure> {
    let content = ContentStore::embedded();
    let hosted = saved.is_some();
    game.table = host_table();
    watch_secondary(&mut game);
    *RANDOM.lock().expect("random lock") = Some(SeededRandom::new(seed));
    {
        let mut host = host();
        // The switches and the answers of a game that is played again were set before this
        // call.
        let switches = std::mem::take(&mut host.switches);
        let saved_answers = host.saved;
        let replayed = saved.as_ref().map_or(0, VecDeque::len) as u64;
        *host = Host {
            players,
            humans: human_mask,
            seed,
            table,
            switches,
            saved: saved_answers,
            script: saved.unwrap_or_default(),
            preload_left: replayed,
            ..Host::default()
        };
        if replayed == 0 && hosted {
            drop(host);
            host_api::replay_finished();
        }
    }
    JOURNAL.lock().expect("journal lock").clear();
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
            if hosted {
                host_api::mark(asked, game.table.log.records.len());
            }
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
            if hosted {
                host_api::after_step(&game, events_before);
            }
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
    let (viewer, too_large) = {
        let mut host = host();
        let viewer = host.shown_seat();
        // The switches and the saved decisions are of this game.
        host.switches.clear();
        host.saved = 0;
        (viewer, host.too_large.take())
    };
    // A buffer that was too small fails the call. It must not sit in a field of a result.
    if let Some(error) = too_large {
        return Err(error.into());
    }
    if hosted {
        let left = host().left;
        // The engine stops with an error when the host leaves; that is no failure of the game.
        let error = stopped.as_ref().filter(|_| !left).map(ToString::to_string);
        return Ok(host_api::finish_run(error.as_deref(), left));
    }
    // The last update: where the game stopped, with nothing left to answer.
    let viewer = viewer.map_or(ViewerRole::Spectator, ViewerRole::Player);
    let tiles = latest_of(&game, content).tiles;
    let update = host_update(&game.state, &viewer, None, &tiles, game.galaxy());
    store(
        &mut UPDATE.lock().expect("update lock"),
        "the update for the host",
        "UPDATE_CAPACITY",
        &update,
    )?;
    respond(
        "the result of ti4_play",
        &Report {
            decisions: game.table.log.records.len(),
            round: game.state.round,
            stopped: stopped.as_ref().map(Shown),
            state: None,
            view: None,
        },
    )
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
    // The decisions since the checkpoint are made again, and the host is told them again.
    JOURNAL.lock().expect("journal lock").clear();
    if host_api::hosted() {
        host_api::took_back(checkpoint.asked, rollback, host.script.len());
    }
    game
}

/// The seats that play on, on a copy of the game that is made again from a checkpoint: the rule
/// bot where the live game has it ([`RULE_SEATS`]), else the stream of the random seats. It
/// counts decisions as the live seats do, so a rule seat decides as it did in the live game.
struct Replayed {
    random: SeededRandom,
    decided: u64,
    seed: u64,
    /// The game is of a table: a random seat decides from the seed, as in [`pick`].
    table: bool,
}

impl Replayed {
    fn decide(
        &mut self,
        choice: &Choice,
        seen: Option<&SeatObservation<'_>>,
    ) -> Result<ChoiceOption, IllegalChoice> {
        let index = usize::try_from(self.decided).expect("a game has fewer decisions than 2^32");
        self.decided += 1;
        let rule = seen.filter(|_| rule_seat(&choice.player));
        if (rule.is_some() || self.table) && !choice.options.is_empty() {
            let (picked, _) = ti4_rule_bot::choose(choice, rule, self.seed, &choice.player, index);
            return Ok(choice.options[picked].clone());
        }
        self.random.choose(choice)
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
/// random seat in them ([`ti4_switch_seats`]).
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
        let (seed, table) = {
            let host = host();
            (host.seed, host.table)
        };
        game.table = Table::with_default(Box::new(Replayed {
            random,
            decided: checkpoint.decided,
            seed,
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

/// Run the draft script of the request buffer as the next turn of the host's seat, on a copy
/// of the game. The outcome is in the draft buffer.
fn draft() -> Result<usize, Failure> {
    #[derive(serde::Deserialize)]
    struct Request {
        script: Vec<DraftStep>,
        /// What the script is of. Left out: the next tactical action.
        #[serde(default)]
        scope: PlanScope,
    }
    let request: Request = serde_json::from_slice(REQUEST.lock().expect("request lock").as_bytes())
        .map_err(|error| failed(format_args!("the draft request is not read: {error}")))?;
    let outcome = draft_outcome(&request.script, &request.scope)?;
    let mut buffer = DRAFT.lock().expect("draft lock");
    Ok(store(
        &mut buffer,
        "the outcome of a draft",
        "DRAFT_CAPACITY",
        &outcome,
    )?)
}

/// Run `script` as the next turn of the host's seat on a copy of the game at its last finished
/// step.
pub(crate) fn draft_outcome(
    script: &[DraftStep],
    scope: &PlanScope,
) -> Result<ti4_view::planning::draft::DraftOutcome, Failure> {
    let player = host()
        .shown_seat()
        .ok_or_else(|| failed("no seat is played here: a draft is of one seat"))?;
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
        run_draft(&base, &player, scope, script)
    });
    Ok(outcome)
}

/// What the script of the request buffer answers to the choice that the game is parked in.
fn plan() -> Result<usize, Failure> {
    #[derive(serde::Deserialize)]
    struct Request {
        script: Vec<DraftStep>,
        next: usize,
        actor: PlayerId,
    }
    #[derive(Serialize)]
    #[serde(tag = "kind", rename_all = "snake_case")]
    enum Reply {
        Answer {
            /// The option of the pending choice, by its place.
            index: usize,
            next: usize,
            #[serde(skip_serializing_if = "Option::is_none")]
            script: Option<Vec<DraftStep>>,
        },
        Done,
        Interrupted {
            reason: String,
        },
        Mismatch {
            step: usize,
            reason: String,
        },
    }
    let request: Request = serde_json::from_slice(REQUEST.lock().expect("request lock").as_bytes())
        .map_err(|error| failed(format_args!("the plan request is not read: {error}")))?;
    let answer = PENDING.with_borrow(|pending| -> Result<Reply, Failure> {
        let choice = pending
            .as_ref()
            .ok_or_else(|| failed("the game is not parked in a choice: a plan answers one"))?;
        let answer = script_answer(&request.script, request.next, choice, &request.actor);
        Ok(match answer {
            ScriptAnswer::Answer {
                option,
                next,
                script,
            } => Reply::Answer {
                index: choice
                    .options
                    .iter()
                    .position(|offered| offered.id == option.id)
                    .expect("the answer is an option of the choice"),
                next,
                script,
            },
            ScriptAnswer::Done => Reply::Done,
            ScriptAnswer::Interrupted { reason } => Reply::Interrupted { reason },
            ScriptAnswer::Mismatch { step, reason } => Reply::Mismatch { step, reason },
        })
    })?;
    respond("the answer of a plan", &answer)
}

/// Set whether the host is offered a card: the request buffer holds `{"card", "mode"}`.
fn set_reaction_mode() -> Result<usize, Failure> {
    #[derive(serde::Deserialize)]
    struct Request {
        card: String,
        mode: ReactionMode,
    }
    let request: Request = serde_json::from_slice(REQUEST.lock().expect("request lock").as_bytes())
        .map_err(|error| failed(format_args!("the reaction mode is not read: {error}")))?;
    ti4_view::reaction_modes::set_mode(
        &mut NEVER.lock().expect("never set lock"),
        &request.card,
        request.mode,
    );
    Ok(0)
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

/// Play a game to its end, asking the host for the seats in `human_mask`. With JSPI the export is
/// wrapped in `WebAssembly.promising` and the import `host.ask` in `WebAssembly.Suspending`.
#[unsafe(no_mangle)]
pub extern "C" fn ti4_play(seed: u32, players: u32, human_mask: u32) -> i32 {
    status(play(u64::from(seed), players as usize, human_mask, None))
}

/// Play a scenario of `ti4_view::scenario` as [`ti4_play`] plays a new game: the request buffer
/// holds its id in `id_len` bytes. The seed and the number of seats are those of the scenario.
#[unsafe(no_mangle)]
pub extern "C" fn ti4_play_scenario(human_mask: u32, id_len: u32) -> i32 {
    // The id is copied: a draft writes its request into the same buffer while the game waits.
    let id = {
        let mut request = REQUEST.lock().expect("request lock");
        request
            .filled(id_len as usize)
            .then(|| String::from_utf8_lossy(request.as_bytes()).into_owned())
    };
    let scenario = id.as_deref().and_then(ti4_view::scenario::find);
    let Some(scenario) = scenario else {
        let known: Vec<&str> = ti4_view::scenario::SCENARIOS
            .iter()
            .map(|scenario| scenario.id)
            .collect();
        return status(Err(failed(format_args!(
            "no scenario '{}': the scenarios are {}",
            id.as_deref()
                .unwrap_or("(an id longer than the request buffer)"),
            known.join(", ")
        ))));
    };
    status(play(
        scenario.seed,
        scenario.players,
        human_mask,
        Some(scenario.id),
    ))
}

/// What the options of a table are, with the maps of its number of seats.
#[derive(Serialize)]
struct TableOptions {
    #[serde(flatten)]
    options: setup::SetupOptions,
    /// The maps that build for the number of seats, the one that such a table starts with
    /// flagged. With no number: every map.
    maps: Vec<ti4_view::maps::MapTemplateSummary>,
}

/// What a table looks like before it starts.
#[derive(Serialize)]
struct TablePreview {
    /// What the table says of its map: its name, its author, its size.
    map: ti4_host::maps::MapChoiceView,
    /// The board, with a place for each seat and no faction.
    #[serde(flatten)]
    preview: ti4_host::maps::MapPreview,
}

/// What Start draws for a table, with the ids of this host.
#[derive(Serialize)]
struct TableDraw {
    /// The players in the order of their seats: the player who came as number `i` is
    /// [`SEATS`]`[i]`.
    players: Vec<PlayerId>,
    #[serde(flatten)]
    drawn: setup::Drawn,
}

/// What a host can choose for a table, as JSON in the response buffer: the options of
/// `ti4_host::setup` (as `GET /api/setup-options` of the server gives them) and, as `maps`, the
/// maps that build for `players` seats (0: every map, as `GET /api/maps`).
#[unsafe(no_mangle)]
pub extern "C" fn ti4_setup_options(players: u32) -> i32 {
    let maps = ti4_host::maps::catalog::catalog((players > 0).then_some(players as usize));
    status(maps.map_err(failed).and_then(|maps| {
        respond(
            "the options of a table",
            &TableOptions {
                options: setup::setup_options("The page", 0),
                maps,
            },
        )
    }))
}

/// The board of the table in the request buffer (`request_len` bytes of JSON: `setup`, and
/// `seats` in the order in which the players came), with a place for each seat.
#[unsafe(no_mangle)]
pub extern "C" fn ti4_setup_preview(request_len: u32) -> i32 {
    status(TableRequest::read(request_len).and_then(|table| {
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

/// What Start draws for the table in the request buffer: who sits where, and the faction of
/// every seat (for a scenario, the factions that it seats). It is what [`ti4_play_setup`] plays,
/// so a host that shows it shows the game.
#[unsafe(no_mangle)]
pub extern "C" fn ti4_setup_draw(request_len: u32) -> i32 {
    status(TableRequest::read(request_len).and_then(|table| {
        let mut drawn = table.drawn()?;
        if drawn.factions.is_empty() {
            // A scenario seats the factions of its seats, in the order of the join: a host that
            // shows the table before the start names them, as the lobby of the server does.
            drawn.factions = (0..table.setup.player_count)
                .map(|seat| ti4_model::id::FactionId::new(ti4_engine::seating::seat_faction(seat)))
                .collect();
        }
        respond(
            "the draw of a table",
            &TableDraw {
                players: drawn.seated(&table.joined()),
                drawn,
            },
        )
    }))
}

/// Play the game of the table in the request buffer as [`ti4_play`] plays a seeded game. The
/// table is started as the server starts the table of a lobby (`ti4_host::setup`): the seats
/// and the factions that nobody chose are drawn from its seed, on its map, with its cards.
///
/// A player of kind `human` is answered by the host, `smart` by the rule bot and `random` at
/// random; the two decide as on the server, so a table of a seed with the same kinds is the
/// same game on both hosts. The mask of [`ti4_set_rule_seats`] is not read.
/// A game that is played again (a saved game, an undo past the checkpoints) needs the same
/// request.
#[unsafe(no_mangle)]
pub extern "C" fn ti4_play_setup(request_len: u32) -> i32 {
    status(play_setup(request_len))
}

/// The scenarios that [`ti4_play_scenario`] plays, as a JSON list in the response buffer.
#[unsafe(no_mangle)]
pub extern "C" fn ti4_scenarios() -> i32 {
    status(respond(
        "the list of scenarios",
        &ti4_view::scenario::SCENARIOS,
    ))
}

/// The printed cards as JSON in the response buffer: the exploration cards and the attachments,
/// and the cards of the play areas (abilities, leaders, technologies, relics). A host asks once:
/// an update names cards by id only.
#[unsafe(no_mangle)]
pub extern "C" fn ti4_catalog() -> i32 {
    status(respond(
        "the catalogue of printed cards",
        &catalog(ContentStore::embedded(), POK),
    ))
}

/// The two sides of a space combat, as [`ti4_space_odds`] reads them.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SpaceOddsRequest {
    attacker: ti4_calc::odds::SideInput,
    defender: ti4_calc::odds::SideInput,
}

/// The seed of every [`ti4_space_odds`]: the same sides give the same numbers.
const ODDS_SEED: u64 = 0;

/// The odds of the space combat in the request buffer (`request_len` bytes of JSON:
/// `{"attacker", "defender"}`, each a side of `ti4_calc::odds::SideInput` with no player) as a
/// `ti4_calc::odds::BattleResponse` in the response buffer: the rollouts of `ti4_calc`, as many
/// as it takes by default, from a fixed seed. A side of `MovementFacts.battle` is such a side.
///
/// Nothing of the game is read or changed: the host may ask while the game waits, and with no
/// game at all.
#[unsafe(no_mangle)]
pub extern "C" fn ti4_space_odds(request_len: u32) -> i32 {
    status(request_filled(request_len).and_then(|()| space_odds()))
}

fn space_odds() -> Result<usize, Failure> {
    let odds = {
        let request = REQUEST.lock().expect("request lock");
        odds_of(request.as_bytes())?
    };
    respond("the odds of a space combat", &odds)
}

/// The odds of the sides in `request`, as [`ti4_space_odds`] gives them.
fn odds_of(request: &[u8]) -> Result<ti4_calc::odds::BattleResponse, Failure> {
    let request: SpaceOddsRequest = serde_json::from_slice(request)
        .map_err(|error| failed(format_args!("the sides are not read: {error}")))?;
    ti4_calc::odds::space_odds_of(
        ContentStore::embedded(),
        POK,
        &request.attacker,
        &request.defender,
        ti4_calc::odds::SpaceOptions {
            seed: Some(ODDS_SEED),
            ..Default::default()
        },
    )
    .map_err(failed)
}

/// Ask [`ti4_space_odds`] for the odds of `request` (JSON), for callers on the native side.
///
/// # Errors
/// The error text of the export.
pub fn space_odds_native(request: &str) -> Result<String, String> {
    fill_request(request)?;
    response(status(space_odds()))
}

/// Ask the host before each decision of a seat that it does not play (1), or let these seats
/// play on (0). The host is then asked with an update that has no pending choice, and answers
/// [`GO`]. With 2 the host is asked the same way, but not before a reaction window that the seat
/// only declines: this is for the wait of a table, which the host takes before it answers. The
/// seat decides the same in every mode, so the game is the same game.
///
/// Only a flag is set: the host may call this while the game waits.
#[unsafe(no_mangle)]
pub extern "C" fn ti4_set_stepping(on: u32) {
    STEPPING.store(on.min(PACE), Ordering::Relaxed);
}

/// The rule bot plays the seats of `mask` (bit 0 is seat `a`) wherever the host does not; the
/// remaining seats decide at random. It decides from the seed, the seat and the number of the
/// decision, as on the server.
///
/// The host calls this before [`ti4_play`], and with the same mask for a game that is played
/// again. The mask stays until it is set again.
#[unsafe(no_mangle)]
pub extern "C" fn ti4_set_rule_seats(mask: u32) {
    RULE_SEATS.store(mask, Ordering::Relaxed);
}

/// How many decisions the seats of the game made, the random seats included. While the game
/// waits, this is the number of the decision that it waits for.
#[unsafe(no_mangle)]
pub extern "C" fn ti4_decided() -> u32 {
    u32::try_from(host().decided).expect("a game has fewer decisions than 2^32")
}

/// From the decision with the number `decision` on, the host answers the seats of `human_mask`
/// and the others decide at random.
///
/// While the game waits, the host calls this with [`ti4_decided`] and then answers [`SWITCH`]:
/// the decision is asked again, of the seat's new decider. The answers of the host from before
/// are then not taken back ([`ti4_can_undo`]). For a game that is played again, the host calls
/// this before [`ti4_play`] for each switch of the game, in order.
#[unsafe(no_mangle)]
pub extern "C" fn ti4_switch_seats(decision: u32, human_mask: u32) {
    host().switches.push(Switch {
        decision: u64::from(decision),
        mask: human_mask,
        asked: None,
    });
}

/// Where the host writes a request before the export that reads it. Constant for the life of
/// the instance; `REQUEST_CAPACITY` bytes.
#[unsafe(no_mangle)]
pub extern "C" fn ti4_request_ptr() -> *mut u8 {
    REQUEST.lock().expect("request lock").as_mut_ptr()
}

/// Run a draft: the request buffer holds `{"script": [...]}` in `request_len` bytes, the steps
/// of `ti4_view::planning::draft::DraftStep`. The script is played as the next turn of the
/// host's seat on a copy of the game at its last finished step, and the outcome
/// (`DraftOutcome`) is left at [`ti4_draft_ptr`] with the returned length. A status below 0 is
/// the negated length of an error text in the response buffer.
///
/// The game itself is not touched, so the host calls this while the game waits in a choice.
#[unsafe(no_mangle)]
pub extern "C" fn ti4_draft(request_len: u32) -> i32 {
    status(request_filled(request_len).and_then(|()| draft()))
}

/// What a script answers to the choice that the game is parked in: the request buffer holds
/// `{"script": [...], "next": n, "actor": seat}` in `request_len` bytes, the entries of
/// `ti4_view::planning::draft::DraftStep`, the first entry that is still to play, and the seat
/// whose script it is. The response is one of `{"kind":"answer","index","next","script"?}`
/// (answer the choice with the option at `index`; with `script` the script goes on in that
/// order), `{"kind":"done"}`, `{"kind":"interrupted","reason"}` and
/// `{"kind":"mismatch","step","reason"}` (`ti4_view::planning::draft::script_answer`).
///
/// This is how a host sends a plan or a draft to the game: it asks here at each choice and
/// answers with the index, until the script is done or stops. Nothing is changed by the call.
#[unsafe(no_mangle)]
pub extern "C" fn ti4_plan(request_len: u32) -> i32 {
    status(request_filled(request_len).and_then(|()| plan()))
}

/// Set the mode of an action card for the host: the request buffer holds
/// `{"card": <printed name>, "mode": "never" | "always"}` in `request_len` bytes. A window that
/// offers only Never cards is declined for the host without asking ([`ti4_journal`]), and any
/// other window is asked without them (`ti4_view::reaction_modes::NeverOffer`).
///
/// The modes are of the instance, not of the game: the host sets them before [`ti4_play`], or
/// while the game waits.
#[unsafe(no_mangle)]
pub extern "C" fn ti4_set_reaction_mode(request_len: u32) -> i32 {
    status(request_filled(request_len).and_then(|()| set_reaction_mode()))
}

/// The decisions that were made without asking the host since it last read them, as
/// `{"from", "entries"}` in the response buffer. An entry is `{"id"}`, the chosen option: of a
/// seat that the host does not play, or an answer of the host that is given again after an undo.
/// A window that was passed for the host has `"passed": {"option", "cards"}` too: the option
/// that declined it, and the printed names of its cards. It is a choice of the host with an
/// answer: it has a number, and an undo goes back over it.
///
/// `from` is the number of the decision of the first entry ([`ti4_decided`]). Between two
/// questions to the host only such decisions are made, so the entries have no gap: a host that
/// reads this at each question, cuts its list at `from` and adds the entries has every decision
/// of the game in order. That list is the saved game ([`ti4_replay`]), as the decision log is on
/// the server.
#[unsafe(no_mangle)]
pub extern "C" fn ti4_journal() -> i32 {
    #[derive(Serialize)]
    struct Journal {
        from: u64,
        entries: Vec<Made>,
    }
    let entries = std::mem::take(&mut *JOURNAL.lock().expect("journal lock"));
    let from = host().decided.saturating_sub(entries.len() as u64);
    status(respond("the journal", &Journal { from, entries }))
}

/// The first `decisions` decisions of the next [`ti4_play`] have their answer in a saved game,
/// those of every seat. For each the host is asked with an empty update and gives the answer
/// ([`ti4_saved_option`]). No seat decides or waits for it, and it is asked whatever the modes
/// of the cards are: a window that was passed when the game was played is one of the decisions.
#[unsafe(no_mangle)]
pub extern "C" fn ti4_replay(decisions: u32) {
    host().saved = u64::from(decisions);
}

/// What the option with the id in the request buffer is in the decision that the game is parked
/// in: `{"index", "own", "option"}` in the response buffer. `index` is the answer of the host
/// to a saved decision ([`ti4_replay`]); `own` says that the host plays its seat.
fn saved_option() -> Result<usize, Failure> {
    #[derive(Serialize)]
    struct Found<'a> {
        index: usize,
        own: bool,
        option: &'a ChoiceOption,
    }
    let wanted =
        String::from_utf8_lossy(REQUEST.lock().expect("request lock").as_bytes()).into_owned();
    PENDING.with_borrow(|pending| {
        let choice = pending
            .as_ref()
            .ok_or_else(|| failed("the game is not parked in a decision"))?;
        let index = choice
            .options
            .iter()
            .position(|option| option.id == wanted)
            .ok_or_else(|| {
                failed(format_args!(
                    "\"{}\" of seat {} offers {}",
                    choice.prompt,
                    choice.player.as_str(),
                    choice.ids().join(", ")
                ))
            })?;
        let own = in_mask(host().mask(), &choice.player);
        respond(
            "a saved option",
            &Found {
                index,
                own,
                option: &choice.options[index],
            },
        )
    })
}

/// See [`saved_option`]: the request buffer holds the id of an option, `request_len` bytes.
#[unsafe(no_mangle)]
pub extern "C" fn ti4_saved_option(request_len: u32) -> i32 {
    status(request_filled(request_len).and_then(|()| saved_option()))
}

/// Where the outcome of the last [`ti4_draft`] is kept. Constant for the life of the instance.
#[unsafe(no_mangle)]
pub extern "C" fn ti4_draft_ptr() -> *const u8 {
    DRAFT.lock().expect("draft lock").as_ptr()
}

/// Whether the host may answer the pending choice with [`UNDO`]: 1 when a checkpoint holds the
/// answer before it, else 0. Then that choice is asked again, as it was.
#[unsafe(no_mangle)]
pub extern "C" fn ti4_can_undo() -> u32 {
    u32::from(host().can_undo())
}

/// Where every export leaves its result or its error. Constant for the life of the instance.
#[unsafe(no_mangle)]
pub extern "C" fn ti4_response_ptr() -> *const u8 {
    RESPONSE.lock().expect("response lock").as_ptr()
}

/// Where the update for the host is kept: while the game is suspended in a choice, and after
/// [`ti4_play`] has returned. Constant for the life of the instance.
#[unsafe(no_mangle)]
pub extern "C" fn ti4_update_ptr() -> *const u8 {
    UPDATE.lock().expect("update lock").as_ptr()
}

/// The length of that update, or zero when there is none.
#[unsafe(no_mangle)]
pub extern "C" fn ti4_update_len() -> u32 {
    u32::try_from(UPDATE.lock().expect("update lock").len()).expect("a buffer is small")
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

/// A question of the setup form that needs no game: see `host_api::query`.
#[unsafe(no_mangle)]
pub extern "C" fn ti4_host_query(len: u32) -> i32 {
    host_api::query(len as usize)
}

/// Play a game for one person: the input holds `{"setup": {...}, "resume": {...}?}`
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

/// The update for the host, for callers on the native side.
#[must_use]
pub fn update() -> String {
    String::from_utf8_lossy(UPDATE.lock().expect("update lock").as_bytes()).into_owned()
}

/// Run a draft script (JSON, see [`ti4_draft`]) and return its outcome, for callers on the
/// native side.
///
/// # Errors
/// The error text of the export.
pub fn draft_native(request: &str) -> Result<String, String> {
    fill_request(request)?;
    let status = status(draft());
    if status < 0 {
        return response(status);
    }
    Ok(String::from_utf8_lossy(DRAFT.lock().expect("draft lock").as_bytes()).into_owned())
}

/// Ask what a script answers to the pending choice (JSON, see [`ti4_plan`]), for callers on the
/// native side.
///
/// # Errors
/// The error text of the export.
pub fn plan_native(request: &str) -> Result<String, String> {
    fill_request(request)?;
    response(status(plan()))
}

/// Set the mode of a card as [`ti4_set_reaction_mode`] does, for callers on the native side.
///
/// # Errors
/// The error text of the export.
pub fn set_reaction_mode_native(card: &str, mode: ReactionMode) -> Result<(), String> {
    let request = serde_json::json!({ "card": card, "mode": mode }).to_string();
    fill_request(&request)?;
    response(status(set_reaction_mode())).map(|_| ())
}

/// Ask [`ti4_saved_option`] for the option with `id`, for callers on the native side.
///
/// # Errors
/// The error text of the export.
pub fn saved_option_native(id: &str) -> Result<String, String> {
    fill_request(id)?;
    response(status(saved_option()))
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

/// Call a setup export (`ti4_setup_preview`, `ti4_setup_draw`, `ti4_play_setup`) with `table` as
/// its request, for callers on the native side.
#[must_use]
pub fn with_table_native(export: extern "C" fn(u32) -> i32, table: &str) -> i32 {
    let length = if fill_request(table).is_ok() {
        table.len()
    } else {
        usize::MAX
    };
    export(u32::try_from(length).unwrap_or(u32::MAX))
}

/// Play a scenario as [`ti4_play_scenario`] does, for callers on the native side.
#[must_use]
pub fn play_scenario_native(id: &str, human_mask: u32) -> i32 {
    let fits = {
        let mut buffer = REQUEST.lock().expect("request lock");
        buffer.clear();
        std::io::Write::write_all(&mut *buffer, id.as_bytes()).is_ok()
    };
    let length = if fits { id.len() } else { usize::MAX };
    ti4_play_scenario(human_mask, u32::try_from(length).unwrap_or(u32::MAX))
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

    /// Reads the journal as a page does: `log` is cut at its first decision and gets its
    /// entries, and is then every decision of the game. Returns the windows that were passed.
    fn read_journal(log: &mut Vec<String>) -> Vec<serde_json::Value> {
        let journal: serde_json::Value =
            serde_json::from_str(&response(ti4_journal()).unwrap()).unwrap();
        let from = usize::try_from(journal["from"].as_u64().unwrap()).unwrap();
        assert!(from <= log.len(), "the journal begins after the log ends");
        log.truncate(from);
        let mut passed = Vec::new();
        for entry in journal["entries"].as_array().unwrap() {
            log.push(entry["id"].as_str().unwrap().to_owned());
            passed.extend(entry.get("passed").cloned());
        }
        assert_eq!(log.len(), ti4_decided() as usize, "the log is the game");
        passed
    }

    /// The answer of a page to a saved decision: the place of the saved option.
    fn saved_index(id: &str) -> i32 {
        let found: serde_json::Value =
            serde_json::from_str(&saved_option_native(id).unwrap()).unwrap();
        i32::try_from(found["index"].as_u64().unwrap()).unwrap()
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
        for scenario in ti4_view::scenario::SCENARIOS {
            let mut game =
                new_game(content, scenario.seed, scenario.players, Some(scenario.id)).unwrap();
            match game.run(ROUNDS, 400) {
                Ok(_) | Err(RunError::StepLimit { .. }) => {}
                Err(error) => panic!("{}: {error}", scenario.id),
            }
            assert!(
                game.table.log.records.len() > 100,
                "{}: the seats decided",
                scenario.id
            );
        }
    }

    /// The odds of a space combat need no game: the sides of the request are all there is, and
    /// the same sides give the same numbers.
    #[test]
    fn the_odds_of_a_space_combat_come_from_the_sides_alone() {
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

    /// One test for everything that plays through the host: the host and the update buffer are
    /// statics, so two such tests would not be independent.
    #[test]
    fn slow_the_host_is_asked_for_its_seats_can_leave_and_a_replay_is_the_same_game() {
        use std::sync::{Arc, Mutex};
        let seen = Arc::new(Mutex::new(Vec::<serde_json::Value>::new()));
        let sink = seen.clone();
        set_native_host(move |update| {
            let mut seen = sink.lock().unwrap();
            seen.push(serde_json::from_str(update).unwrap());
            if seen.len() == 12 { -1 } else { 0 }
        });
        // Seats a and c.
        let status = ti4_play(3, 8, 0b101);
        let report: serde_json::Value = serde_json::from_str(&response(status).unwrap()).unwrap();
        assert!(
            report["stopped"]
                .as_str()
                .is_some_and(|text| text.contains(LEFT)),
            "{report}"
        );
        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 12);
        let mut seats = std::collections::BTreeSet::new();
        for (index, update) in seen.iter().enumerate() {
            let seat = update["pending_choice"]["choice"]["player"]
                .as_str()
                .unwrap();
            seats.insert(seat.to_owned());
            assert_eq!(update["viewer"]["seat"], seat);
            assert_eq!(update["pending_choice"]["nonce"], (index + 1).to_string());
            assert_eq!(update["turn_status"]["seat"], seat);
            assert_eq!(update["view"]["players"].as_array().unwrap().len(), 8);
        }
        assert!(
            seats.iter().all(|seat| seat == "a" || seat == "c"),
            "{seats:?}"
        );
        // After the game: the last update, with nothing to answer.
        let last: serde_json::Value = serde_json::from_str(&update()).unwrap();
        assert!(last.get("pending_choice").is_none());
        assert_eq!(last["viewer"]["role"], "player");
        drop(seen);

        // A saved game is the seed and the ids of the host's answers. Played again with them,
        // the game asks the same choices and reaches the same position.
        const ANSWERS: usize = 40;
        let options = |update: &serde_json::Value| -> Vec<String> {
            update["pending_choice"]["choice"]["options"]
                .as_array()
                .unwrap()
                .iter()
                .map(|option| option["id"].as_str().unwrap().to_owned())
                .collect()
        };
        // The page keeps every decision of the game, of every seat: its own answers, and what
        // the journal says of the others.
        const SAVED: usize = 30;
        #[derive(Default, Clone)]
        struct First {
            answers: Vec<String>,
            reached: String,
            log: Vec<String>,
            saved: Vec<String>,
        }
        let first = Arc::new(Mutex::new(First::default()));
        let sink = first.clone();
        set_native_host(move |update| {
            let first = &mut *sink.lock().unwrap();
            read_journal(&mut first.log);
            if first.answers.len() == SAVED {
                first.saved.clone_from(&first.log);
            }
            if first.answers.len() == ANSWERS {
                update.clone_into(&mut first.reached);
                return -1;
            }
            let ids = options(&serde_json::from_str(update).unwrap());
            let index = (first.answers.len() * 7) % ids.len();
            first.answers.push(ids[index].clone());
            first.log.push(ids[index].clone());
            i32::try_from(index).unwrap()
        });
        ti4_play(3, 8, 1);
        let First {
            answers,
            reached,
            saved,
            log,
        } = first.lock().unwrap().clone();
        assert_eq!(answers.len(), ANSWERS);
        assert!(saved.len() > SAVED, "the other seats decided too");
        // It is the decision log of the engine, which is what the server keeps of a game.
        let chosen = |records: Vec<ti4_engine::choice::DecisionRecord>| -> Vec<String> {
            records.into_iter().map(|record| record.chosen).collect()
        };
        let played = chosen(decisions_native());
        assert_eq!(log[..], played[..log.len()]);

        // A saved game is the seed and that list. The game is given each decision of it, of
        // every seat, and is shown nothing; the random seats then go on as in the game that was
        // never saved, so the same answers reach the same position.
        let resumed = Arc::new(Mutex::new((0, SAVED, String::new())));
        let sink = resumed.clone();
        let script = saved.clone();
        set_native_host(move |update| {
            let (given, answered, reached) = &mut *sink.lock().unwrap();
            if update.is_empty() {
                *given += 1;
                return saved_index(&script[*given - 1]);
            }
            assert_eq!(
                *given,
                script.len(),
                "no choice before the saved game is played"
            );
            if *answered == ANSWERS {
                update.clone_into(reached);
                return -1;
            }
            let ids = options(&serde_json::from_str(update).unwrap());
            let index = (*answered * 7) % ids.len();
            *answered += 1;
            i32::try_from(index).unwrap()
        });
        ti4_replay(u32::try_from(saved.len()).unwrap());
        ti4_play(3, 8, 1);
        let resumed = resumed.lock().unwrap().clone();
        assert_eq!(chosen(decisions_native())[..log.len()], log[..]);
        assert_eq!(resumed.0, saved.len());
        assert_eq!(resumed.2, reached);
        // A saved decision that the game does not offer is said with what it offers.
        assert!(
            saved_option_native("tactical").is_err(),
            "no game is parked"
        );

        let again = Arc::new(Mutex::new((0, String::new())));
        let sink = again.clone();
        set_native_host(move |update| {
            let (asked, reached) = &mut *sink.lock().unwrap();
            let Some(wanted) = answers.get(*asked) else {
                update.clone_into(reached);
                return -1;
            };
            *asked += 1;
            let ids = options(&serde_json::from_str(update).unwrap());
            let index = ids.iter().position(|id| id == wanted);
            i32::try_from(index.expect("the replayed game offers the saved answer")).unwrap()
        });
        ti4_play(3, 8, 1);
        assert_eq!(again.lock().unwrap().1, reached);
        assert!(
            reached.contains("\"nonce\":\"41\""),
            "the position after 40 answers"
        );

        // An undo opens the choice before the pending one exactly as it was shown, and the same
        // answers lead to the same choices again. Five in a row go back over several steps.
        #[derive(Default)]
        struct Undoing {
            shown: std::collections::BTreeMap<u64, String>,
            shown_again: usize,
            left: usize,
            active: bool,
        }
        let undoing = Arc::new(Mutex::new(Undoing {
            left: 5,
            ..Undoing::default()
        }));
        let sink = undoing.clone();
        set_native_host(move |update| {
            let mut undoing = sink.lock().unwrap();
            let value: serde_json::Value = serde_json::from_str(update).unwrap();
            let number: u64 = value["pending_choice"]["nonce"]
                .as_str()
                .unwrap()
                .parse()
                .unwrap();
            if let Some(before) = undoing.shown.get(&number) {
                assert_eq!(before, update, "choice {number} is shown as before");
                undoing.shown_again += 1;
            } else {
                undoing.shown.insert(number, update.to_owned());
            }
            if number == 1 {
                assert_eq!(ti4_can_undo(), 0, "there is no answer to take back yet");
            }
            if number == 41 && undoing.left > 0 {
                undoing.active = true;
            }
            if undoing.active && undoing.left > 0 {
                assert_eq!(ti4_can_undo(), 1);
                undoing.left -= 1;
                return UNDO;
            }
            undoing.active = false;
            if number == 42 {
                return -1;
            }
            let count = options(&value).len();
            i32::try_from(usize::try_from(number).unwrap() * 7 % count).unwrap()
        });
        let status = ti4_play(3, 8, 1);
        let report = response(status).unwrap();
        assert!(report.contains(LEFT), "{report}");
        let undoing = undoing.lock().unwrap();
        // Choices 40 down to 36 after each undo, then 37 to 41 after the same answers.
        assert_eq!(undoing.shown_again, 10);
        assert_eq!(undoing.shown.len(), 42);
        drop(undoing);

        // The facts of a tactical action say what the engine then offers: seat a activates the
        // system next to its home, moves a carrier, and is offered the cargo of the facts.
        let seen = Arc::new(Mutex::new(Vec::<serde_json::Value>::new()));
        let sink = seen.clone();
        set_native_host(move |update| {
            let value: serde_json::Value = serde_json::from_str(update).unwrap();
            let mut seen = sink.lock().unwrap();
            let script = ["pok8imperial", "tactical", "23", "move|01|0"];
            let wanted = script.get(seen.len()).copied();
            let index = wanted.and_then(|id| options(&value).iter().position(|have| have == id));
            seen.push(value);
            index.map_or(-1, |index| i32::try_from(index).unwrap())
        });
        let status = ti4_play(3, 8, 1);
        assert!(response(status).unwrap().contains(LEFT));
        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 5);
        assert!(
            seen[1].get("tactical").is_none(),
            "the turn menu has no facts"
        );

        let activation = &seen[2]["tactical"];
        assert_eq!(activation["kind"], "activation");
        assert_eq!(activation["tactic_tokens"], 3);
        let reach = activation["systems"].as_array().unwrap();
        assert_eq!(reach.len(), options(&seen[2]).len());
        let next_to_home = reach.iter().find(|fact| fact["system"] == "23").unwrap();
        // Two carriers and a destroyer; the fighters do not move on their own.
        assert_eq!(next_to_home["ships"], 3);
        assert_eq!(next_to_home["origins"], 1);
        // The seat produces at home only, and has no ships of another player to fire at.
        assert!(next_to_home.get("production").is_none());
        let producing: Vec<_> = reach
            .iter()
            .filter(|fact| fact.get("production").is_some())
            .collect();
        assert_eq!(producing.len(), 1);
        assert!(reach.iter().all(|fact| fact.get("cannon").is_none()));

        let movement = &seen[3]["tactical"];
        assert_eq!(movement["kind"], "movement");
        assert_eq!(movement["active"], "23");
        assert_eq!(movement["moved"], 0);
        let ships = movement["ships"].as_array().unwrap();
        assert_eq!(ships.len(), 3, "{ships:?}");
        // Every ship that the engine offers is a ship of the facts that can move.
        let offered = seen[3]["pending_choice"]["choice"]["options"]
            .as_array()
            .unwrap();
        for option in offered.iter().filter(|option| option["kind"] == "move") {
            let (_, place) = option["id"].as_str().unwrap().split_once('|').unwrap();
            let (origin, index) = place.split_once('|').unwrap();
            let ship = ships
                .iter()
                .find(|ship| {
                    ship["origin"] == origin && ship["index"].as_u64() == index.parse().ok()
                })
                .unwrap_or_else(|| panic!("no ship for {option}"));
            assert_eq!(ship["unit"], option["payload"]["unit"]);
            assert_eq!(ship["capacity"], option["payload"]["capacity"]);
            assert_eq!(ship["move"]["path"], serde_json::json!(["01", "23"]));
        }
        // The hold of the carrier is offered the pools that the facts name for it.
        let carrier = &ships[0];
        let pools = movement["cargo"].as_array().unwrap();
        let mut named: Vec<_> = carrier["loads"]
            .as_array()
            .unwrap()
            .iter()
            .map(|place| {
                let pool = &pools[usize::try_from(place.as_u64().unwrap()).unwrap()];
                assert!(pool["count"].as_u64().unwrap() > 0, "{pool}");
                (
                    pool["system"].clone(),
                    pool["source"].clone(),
                    pool["unit"].clone(),
                )
            })
            .collect();
        let mut loads: Vec<_> = seen[4]["pending_choice"]["choice"]["options"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|option| option["kind"] == "load")
            .map(|option| {
                let payload = &option["payload"];
                (
                    payload["pickup_system"].clone(),
                    payload["source"].clone(),
                    payload["unit"].clone(),
                )
            })
            .collect();
        let text = |value: &(serde_json::Value, serde_json::Value, serde_json::Value)| {
            format!("{value:?}")
        };
        named.sort_by_key(text);
        loads.sort_by_key(text);
        assert!(!loads.is_empty());
        assert_eq!(named, loads);
        assert!(seen[4].get("tactical").is_none(), "a hold has no facts");
        let live_movement = seen[3].clone();
        drop(seen);

        // The landings: the carrier takes two infantry along, and the facts of each landing say
        // what the engine offers, with the count of the units that are still in space.
        let seen = Arc::new(Mutex::new(Vec::<serde_json::Value>::new()));
        let sink = seen.clone();
        set_native_host(move |update| {
            let value: serde_json::Value = serde_json::from_str(update).unwrap();
            let mut seen = sink.lock().unwrap();
            let ids = options(&value);
            let infantry = value["pending_choice"]["choice"]["options"]
                .as_array()
                .unwrap()
                .iter()
                .position(|option| {
                    option["kind"] == "load" && option["payload"]["unit"] == "sol_infantry"
                });
            let pick = |id: &str| ids.iter().position(|have| have == id);
            let index = match seen.len() {
                0 => pick("pok8imperial"),
                1 => pick("tactical"),
                2 => pick("23"),
                3 => pick("move|01|0"),
                4 | 5 => infantry,
                6 => pick("done_loading"),
                7 => pick("done_moving"),
                8 => pick("commit|0|saudor"),
                _ => None,
            };
            seen.push(value);
            index.map_or(-1, |index| i32::try_from(index).unwrap())
        });
        let status = ti4_play(3, 8, 1);
        assert!(response(status).unwrap().contains(LEFT));
        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 10);
        for (update, left) in [(&seen[8], 2), (&seen[9], 1)] {
            let facts = &update["tactical"];
            assert_eq!(facts["kind"], "invasion");
            assert_eq!(facts["active"], "23");
            assert_eq!(
                facts["forces"],
                serde_json::json!([{ "unit": "sol_infantry", "damaged": false, "count": left }])
            );
            // Each landing that the engine offers is one force at one planet of the facts.
            let mut named = Vec::new();
            for planet in facts["planets"].as_array().unwrap() {
                for place in planet["lands"].as_array().unwrap() {
                    let force = &facts["forces"][usize::try_from(place.as_u64().unwrap()).unwrap()];
                    named.push((planet["planet"].clone(), force["unit"].clone()));
                }
            }
            let offered: Vec<_> = update["pending_choice"]["choice"]["options"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|option| option["kind"] == "commit")
                .map(|option| {
                    (
                        option["payload"]["planet"].clone(),
                        option["payload"]["unit"].clone(),
                    )
                })
                .collect();
            assert!(!offered.is_empty());
            assert_eq!(named, offered);
        }
        drop(seen);

        // A draft at the turn menu plays the same tactical action on a copy: it shows the
        // movement that the game then shows, and the game is as it was.
        let drafted = Arc::new(Mutex::new((0, Vec::<serde_json::Value>::new())));
        let sink = drafted.clone();
        set_native_host(move |update| {
            let value: serde_json::Value = serde_json::from_str(update).unwrap();
            let (asked, outcomes) = &mut *sink.lock().unwrap();
            let script = ["pok8imperial"];
            let Some(wanted) = script.get(*asked) else {
                for script in [
                    r#"[]"#,
                    r#"[{"kind":"choose","option_id":"tactical"},
                        {"kind":"choose","option_id":"23"},
                        {"kind":"move","origin":"01","unit":"sol_carrier","damaged":false,
                         "gravity_drive":false,"ionian":false},
                        {"kind":"done_loading"},
                        {"kind":"done_moving"}]"#,
                ] {
                    let outcome = draft_native(&format!(r#"{{"script":{script}}}"#)).unwrap();
                    outcomes.push(serde_json::from_str(&outcome).unwrap());
                }
                assert_eq!(
                    crate::update(),
                    update,
                    "a draft leaves the update as it was"
                );
                return -1;
            };
            *asked += 1;
            let index = options(&value).iter().position(|have| have == wanted);
            i32::try_from(index.unwrap()).unwrap()
        });
        ti4_play(3, 8, 1);
        let (_, outcomes) = &*drafted.lock().unwrap();
        assert_eq!(outcomes[0]["stop"]["kind"], "open");
        assert_eq!(options(&outcomes[0]["update"]), ["tactical"]);
        assert_eq!(
            outcomes[1]["stop"]["kind"], "complete",
            "{}",
            outcomes[1]["stop"]
        );
        assert_eq!(outcomes[1]["consumed"], 5);
        assert_eq!(
            outcomes[1]["movement"]["tactical"],
            live_movement["tactical"]
        );
        assert_eq!(
            outcomes[1]["movement"]["view"]["board"],
            live_movement["view"]["board"]
        );
        let arrived = outcomes[1]["update"]["view"]["board"]["systems"]["23"]["units"]
            .as_array()
            .unwrap();
        assert!(
            arrived
                .iter()
                .any(|unit| unit["unit_type"] == "sol_carrier")
        );

        // With stepping the host is asked before each decision of a random seat. The seats
        // still follow the seed: the same answers reach the same position. An undo there
        // opens the last choice of the host again.
        #[derive(Default)]
        struct Stepped {
            answers: usize,
            pauses: usize,
            undone: bool,
            again: bool,
            reached: String,
        }
        let stepped = Arc::new(Mutex::new(Stepped::default()));
        let sink = stepped.clone();
        ti4_set_stepping(1);
        set_native_host(move |update| {
            let mut stepped = sink.lock().unwrap();
            let value: serde_json::Value = serde_json::from_str(update).unwrap();
            if value.get("pending_choice").is_none() {
                stepped.pauses += 1;
                assert_eq!(value["viewer"]["seat"], "a");
                assert_ne!(value["turn_status"]["seat"], "a");
                if stepped.answers >= 10 && !stepped.undone {
                    assert_eq!(ti4_can_undo(), 1);
                    stepped.undone = true;
                    return UNDO;
                }
                return GO;
            }
            if stepped.undone && !stepped.again {
                // The choice of the last answer, with its number.
                let number = stepped.answers.to_string();
                assert_eq!(value["pending_choice"]["nonce"], number.as_str());
                stepped.answers -= 1;
                stepped.again = true;
            }
            if stepped.answers == ANSWERS {
                update.clone_into(&mut stepped.reached);
                return -1;
            }
            let count = options(&value).len();
            let index = (stepped.answers * 7) % count;
            stepped.answers += 1;
            i32::try_from(index).unwrap()
        });
        ti4_play(3, 8, 1);
        ti4_set_stepping(0);
        let stepped = stepped.lock().unwrap();
        assert!(stepped.pauses > 0);
        assert!(stepped.again, "the undo opened the last choice again");
        assert_eq!(stepped.reached, reached);
        drop(stepped);

        // A scenario starts at the turn menu of seat a, and its board is in the facts.
        let scenarios: serde_json::Value =
            serde_json::from_str(&response(ti4_scenarios()).unwrap()).unwrap();
        assert_eq!(scenarios[0]["id"], ti4_view::scenario::LATE_GAME);
        assert_eq!(scenarios[0]["players"], 8);
        let seen = Arc::new(Mutex::new(Vec::<serde_json::Value>::new()));
        let sink = seen.clone();
        set_native_host(move |update| {
            let value: serde_json::Value = serde_json::from_str(update).unwrap();
            let mut seen = sink.lock().unwrap();
            let index = seen
                .is_empty()
                .then(|| options(&value).iter().position(|have| have == "tactical"))
                .flatten();
            seen.push(value);
            index.map_or(-1, |index| i32::try_from(index).unwrap())
        });
        let status = play_scenario_native(ti4_view::scenario::LATE_GAME, 1);
        assert!(response(status).unwrap().contains(LEFT));
        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 2);
        assert_eq!(seen[0]["pending_choice"]["nonce"], "1");
        assert_eq!(seen[0]["view"]["round"], 4);
        let activation = &seen[1]["tactical"];
        assert_eq!(activation["kind"], "activation");
        assert_eq!(activation["tactic_tokens"], 7);
        let reach = activation["systems"].as_array().unwrap();
        let in_range = reach.iter().filter(|fact| fact["ships"] != 0).count();
        assert!(in_range >= 20, "{in_range} systems in range");
        drop(seen);

        let status = play_scenario_native("nope", 1);
        let error = response(status).unwrap_err();
        assert!(error.contains("late-game"), "{error}");

        // The host changes its seats during the game: a to c, c to e and at once to b, b back
        // to a. Each time the open decision and the later ones of the old seat are drawn, and
        // the host is asked for the new seat only. An undo stays with the seat.
        const PLAN: [(usize, &str, u32); 4] = [
            (6, "c", 0b100),
            (10, "e", 0b1_0000),
            (10, "b", 0b10),
            (14, "a", 0b1),
        ];
        #[derive(Default)]
        struct Switching {
            answers: Vec<String>,
            switches: Vec<(u32, u32)>,
            seat: &'static str,
            undone: bool,
            reached: String,
        }
        let switching = Arc::new(Mutex::new(Switching {
            seat: "a",
            ..Switching::default()
        }));
        let sink = switching.clone();
        set_native_host(move |update| {
            let mut run = sink.lock().unwrap();
            let value: serde_json::Value = serde_json::from_str(update).unwrap();
            assert_eq!(value["pending_choice"]["choice"]["player"], run.seat);
            assert_eq!(value["viewer"]["seat"], run.seat);
            if let Some((at, seat, mask)) = PLAN.get(run.switches.len()).copied()
                && run.answers.len() == at
            {
                let decision = ti4_decided();
                ti4_switch_seats(decision, mask);
                run.switches.push((decision, mask));
                run.seat = seat;
                return SWITCH;
            }
            let since = PLAN[..run.switches.len()]
                .last()
                .map_or(0, |(at, _, _)| *at);
            // The answers of the seat before are not taken back.
            assert_eq!(
                ti4_can_undo(),
                u32::from(run.answers.len() > since),
                "after {} answers",
                run.answers.len()
            );
            if run.answers.len() == 8 && !run.undone {
                run.undone = true;
                run.answers.pop();
                return UNDO;
            }
            if run.answers.len() == 20 {
                update.clone_into(&mut run.reached);
                return -1;
            }
            let ids = options(&value);
            let index = (run.answers.len() * 7) % ids.len();
            run.answers.push(ids[index].clone());
            i32::try_from(index).unwrap()
        });
        ti4_play(3, 8, 1);
        let run = std::mem::take(&mut *switching.lock().unwrap());
        assert_eq!(run.switches.len(), PLAN.len());
        assert!(run.undone);
        assert!(run.reached.contains("\"nonce\":\"21\""), "{}", run.reached);

        // A saved game names its switches by the number of the decision. Played again with
        // them and the same answers, the game reaches the same position.
        for (decision, mask) in &run.switches {
            ti4_switch_seats(*decision, *mask);
        }
        let again = Arc::new(Mutex::new((0, String::new())));
        let sink = again.clone();
        let answers = run.answers.clone();
        set_native_host(move |update| {
            let (asked, reached) = &mut *sink.lock().unwrap();
            let Some(wanted) = answers.get(*asked) else {
                update.clone_into(reached);
                return -1;
            };
            *asked += 1;
            let ids = options(&serde_json::from_str(update).unwrap());
            let index = ids.iter().position(|id| id == wanted);
            i32::try_from(index.expect("the replayed game offers the saved answer")).unwrap()
        });
        ti4_play(3, 8, 1);
        assert_eq!(again.lock().unwrap().1, run.reached);

        // The Never cards of the host. Seat d of the late game holds Nav Suite; its window
        // opens twice after an activation. Both are passed for the host: each is a choice with
        // a number and an answer, and an undo goes back over them to the activation. A script
        // answers the choice that the game waits in.
        const NAV_SUITE: &str = "Nav Suite";
        let asked_of = |value: &serde_json::Value| {
            let pending = &value["pending_choice"];
            (
                pending["nonce"].as_str().unwrap().to_owned(),
                pending["choice"]["context"]["subtype"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned(),
            )
        };
        set_reaction_mode_native(NAV_SUITE, ReactionMode::Never).unwrap();
        let seen = Arc::new(Mutex::new(Vec::<(String, String)>::new()));
        let sink = seen.clone();
        // Every decision of the game, the place of the activation in it, and the last update.
        let kept = Arc::new(Mutex::new((Vec::<String>::new(), 0, String::new())));
        let keeps = kept.clone();
        set_native_host(move |update| {
            let value: serde_json::Value = serde_json::from_str(update).unwrap();
            let mut seen = sink.lock().unwrap();
            let (log, activation, reached) = &mut *keeps.lock().unwrap();
            let passed = read_journal(log);
            seen.push(asked_of(&value));
            let pick = |id: &str| {
                let index = options(&value).iter().position(|have| have == id);
                i32::try_from(index.unwrap()).unwrap()
            };
            match seen.len() {
                1 => {
                    let script =
                        r#"[{"kind":"choose","option_id":"tactical"},{"kind":"done_moving"}]"#;
                    let answer: serde_json::Value = serde_json::from_str(
                        &plan_native(&format!(r#"{{"script":{script},"next":0,"actor":"d"}}"#))
                            .unwrap(),
                    )
                    .unwrap();
                    assert_eq!(answer["kind"], "answer", "{answer}");
                    assert_eq!(answer["next"], 1);
                    assert_eq!(answer["index"], pick("tactical"));
                    // The next entry is no answer to the turn menu, and no other seat answers.
                    let later: serde_json::Value = serde_json::from_str(
                        &plan_native(&format!(r#"{{"script":{script},"next":1,"actor":"d"}}"#))
                            .unwrap(),
                    )
                    .unwrap();
                    assert_eq!(later["kind"], "interrupted", "{later}");
                    log.push("tactical".to_owned());
                    pick("tactical")
                }
                2 | 4 => {
                    *activation = log.len();
                    log.push("41".to_owned());
                    pick("41")
                }
                3 => {
                    assert_eq!(passed.len(), 2);
                    assert_eq!(ti4_can_undo(), 1);
                    // The page takes the activation back, and what was decided after it.
                    log.truncate(*activation);
                    UNDO
                }
                _ => {
                    assert_eq!(passed.len(), 2);
                    assert_eq!(passed[0]["cards"], serde_json::json!([NAV_SUITE]));
                    assert_eq!(passed[0]["option"]["id"], "decline");
                    assert_eq!(log[log.len() - 2..], ["decline", "decline"]);
                    update.clone_into(reached);
                    -1
                }
            }
        });
        let status = play_scenario_native(ti4_view::scenario::LATE_GAME, 0b1000);
        assert!(response(status).unwrap().contains(LEFT));
        let asked: Vec<(String, String)> = std::mem::take(&mut *seen.lock().unwrap());
        let asked: Vec<(&str, &str)> = asked
            .iter()
            .map(|(nonce, subtype)| (nonce.as_str(), subtype.as_str()))
            .collect();
        assert_eq!(
            asked,
            [
                ("1", "action_menu"),
                ("2", "activate_system"),
                ("5", "movement_step"),
                ("2", "activate_system"),
                ("5", "movement_step"),
            ]
        );

        // A saved game has the answers of the windows. It is given them, whatever the cards are
        // set to, so it is the same game, and the choice after them has the same number.
        let (log, _, reached) = kept.lock().unwrap().clone();
        for never in [true, false] {
            let mode = if never {
                ReactionMode::Never
            } else {
                ReactionMode::Always
            };
            set_reaction_mode_native(NAV_SUITE, mode).unwrap();
            let seen = Arc::new(Mutex::new((0, Vec::<String>::new())));
            let sink = seen.clone();
            let script = log.clone();
            set_native_host(move |update| {
                let (given, shown) = &mut *sink.lock().unwrap();
                if update.is_empty() {
                    *given += 1;
                    return saved_index(&script[*given - 1]);
                }
                assert!(read_journal(&mut script.clone()).is_empty());
                shown.push(update.to_owned());
                -1
            });
            ti4_replay(u32::try_from(log.len()).unwrap());
            let status = play_scenario_native(ti4_view::scenario::LATE_GAME, 0b1000);
            assert!(response(status).unwrap().contains(LEFT));
            let (given, shown) = seen.lock().unwrap().clone();
            assert_eq!(given, log.len());
            assert_eq!(shown, std::slice::from_ref(&reached));
        }

        // The rule bot plays the seats of its mask. It decides from the seed, the seat and the
        // number of the decision, so the same answers of the host reach the same positions, and
        // they are not the positions of a game with random seats.
        let play = |rule: u32| {
            let seen = Arc::new(Mutex::new(Vec::<String>::new()));
            let sink = seen.clone();
            set_native_host(move |update| {
                let mut seen = sink.lock().unwrap();
                seen.push(update.to_owned());
                if seen.len() == 40 { -1 } else { 0 }
            });
            ti4_set_rule_seats(rule);
            let status = ti4_play(3, 6, 1);
            assert!(response(status).unwrap().contains(LEFT));
            let decided = ti4_decided();
            ti4_set_rule_seats(0);
            let seen = seen.lock().unwrap().clone();
            assert_eq!(seen.len(), 40);
            (seen, decided)
        };
        let ruled = play(0b11_1110);
        assert_eq!(ruled, play(0b11_1110));
        assert_ne!(ruled.0, play(0).0);

        // The table of a page. Its options are those of the server, with the maps of the count.
        {
            let json = |status: i32| -> serde_json::Value {
                serde_json::from_str(&response(status).unwrap()).unwrap()
            };
            let options = json(ti4_setup_options(6));
            assert_eq!(options["strategy_card_sets"].as_array().unwrap().len(), 3);
            assert_eq!(options["faction_groups"].as_array().unwrap().len(), 2);
            assert_eq!(options["bot_kinds"][0]["id"], "smart");
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
            assert!(json(ti4_setup_options(0))["maps"].as_array().unwrap().len() > maps.len());

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
            // The odds of a space combat, through the buffers of the export.
            let sides =
                r#"{"attacker":{"units":{"cruiser":1}},"defender":{"units":{"fighter":1}}}"#;
            let odds: serde_json::Value =
                serde_json::from_str(&space_odds_native(sides).unwrap()).unwrap();
            assert_eq!(odds["attacker_fielded"]["cruiser"], 1);
            assert!(space_odds_native("{}").unwrap_err().contains("not read"));

            let preview = json(with_table_native(ti4_setup_preview, &table(5, "human")));
            assert_eq!(preview["map"]["alias"], recommended.as_str());
            assert_eq!(preview["seats"].as_array().unwrap().len(), 6);
            assert!(!preview["tiles"].as_array().unwrap().is_empty());

            // The draw seats every player once, and a chosen faction stays with its player.
            let draw = json(with_table_native(ti4_setup_draw, &table(5, "human")));
            let players: Vec<&str> = draw["players"]
                .as_array()
                .unwrap()
                .iter()
                .map(|id| id.as_str().unwrap())
                .collect();
            let mut sorted = players.clone();
            sorted.sort_unstable();
            assert_eq!(sorted, &SEATS[..6]);
            let faction_of = |id: &str| {
                let seat = players.iter().position(|player| *player == id).unwrap();
                draw["factions"][seat].as_str().unwrap().to_owned()
            };
            assert_eq!(faction_of("a"), "hacan");
            assert_eq!(faction_of("c"), "ghost");
            assert!(
                (0..20).any(|seed| {
                    json(with_table_native(ti4_setup_draw, &table(seed, "human")))["players"]
                        != draw["players"]
                }),
                "some seed moves a player"
            );

            // The game is the drawn table: the host is asked as player `a`, which is Hacan,
            // with the cards of the set, and the mask of the rule seats is as it was.
            let seen = Arc::new(Mutex::new(Vec::<serde_json::Value>::new()));
            let sink = seen.clone();
            set_native_host(move |update| {
                sink.lock()
                    .unwrap()
                    .push(serde_json::from_str(update).unwrap());
                -1
            });
            ti4_set_rule_seats(0b1000);
            let status = with_table_native(ti4_play_setup, &table(5, "human"));
            assert!(response(status).unwrap().contains(LEFT));
            assert_eq!(RULE_SEATS.load(Ordering::Relaxed), 0b1000);
            ti4_set_rule_seats(0);
            let state = LATEST.lock().unwrap().as_ref().unwrap().state.clone();
            let seated: Vec<&str> = state.players.iter().map(|seat| seat.id.as_str()).collect();
            assert_eq!(seated, players);
            for (seat, player) in state.players.iter().enumerate() {
                assert_eq!(player.faction.as_str(), draw["factions"][seat]);
            }
            let (cards, _) =
                ti4_engine::setup::strategy_card_setup_for_set(ContentStore::embedded(), "pok")
                    .unwrap();
            assert!(
                cards
                    .iter()
                    .all(|card| state.card_initiative.contains_key(card)),
                "the game plays the cards of the set"
            );
            assert_eq!(
                seen.lock().unwrap()[0]["pending_choice"]["choice"]["player"],
                "a"
            );

            // With no person the seats play by themselves, and the same table is the same game.
            set_round_limit_native(Some(1));
            let bots = |seed: u64| {
                let status = with_table_native(ti4_play_setup, &table(seed, "smart"));
                let report = json(status);
                assert!(report["stopped"].is_null(), "{report}");
                decisions_native()
            };
            let first = bots(5);
            assert!(first.len() > 50, "{} decisions", first.len());
            assert_eq!(first, bots(5));
            assert_ne!(first, bots(6));

            // The host can be asked before each decision of these seats, and for the wait of a
            // table before each but a reaction window that the seat only declines. The game is
            // the same game.
            let pauses = Arc::new(Mutex::new(0_usize));
            let paused = |mode: u32| {
                *pauses.lock().unwrap() = 0;
                let count = pauses.clone();
                set_native_host(move |update| {
                    let value: serde_json::Value = serde_json::from_str(update).unwrap();
                    assert!(value.get("pending_choice").is_none(), "{update}");
                    *count.lock().unwrap() += 1;
                    GO
                });
                ti4_set_stepping(mode);
                let decisions = bots(5);
                ti4_set_stepping(0);
                assert_eq!(decisions, first, "mode {mode}");
                *pauses.lock().unwrap()
            };
            let stepped = paused(1);
            assert_eq!(stepped, first.len());
            let paced = paused(2);
            assert!(0 < paced && paced <= stepped, "{paced} of {stepped}");
            eprintln!("paced: {paced} of {stepped} decisions wait");
            set_round_limit_native(None);

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
                let error = response(with_table_native(ti4_play_setup, wrong)).unwrap_err();
                assert!(error.contains(reason), "{wrong}: {error}");
            }
        }

        // Without the host, the rule bot plays a game to its end.
        ti4_set_rule_seats(0b1111);
        let status = ti4_play(3, 4, 0);
        ti4_set_rule_seats(0);
        let report: serde_json::Value = serde_json::from_str(&response(status).unwrap()).unwrap();
        assert!(report["stopped"].is_null(), "{report}");
        assert!(
            LATEST.lock().unwrap().as_ref().unwrap().state.finished,
            "{report}"
        );
    }
}
