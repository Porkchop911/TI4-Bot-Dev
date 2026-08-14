//! Choices: the single way any actor decides anything.
//!
//! Every decision in the game — which ability to resolve in an open window, which action to
//! take on your turn, how to vote — is the same shape: *the engine enumerates legal options,
//! and an actor picks one of them*. Nothing else is a decision point.
//!
//! That shape is what makes several guarantees hold at once:
//!
//! * the engine is authoritative, because options are generated, never accepted from outside;
//! * a bot or an LLM can only ever select a legal option, with no channel to invent one;
//! * decisions are recorded, so a game replays exactly from a seed plus its decision log;
//! * an actor sees only the [`Choice`] it is handed and the public facts in [`Observed`],
//!   never another player's hand.
//!
//! [`Decider`] is deliberately tiny. Human input, a scripted conformance test, a seeded
//! random smoke run, and a scored bot are all the same interface.
//!
//! Ported from the oracle's `engine/choice.py`. The oracle's `Option` is [`ChoiceOption`]
//! here, because a type called `Option` in scope alongside `std::option::Option` is a trap.

use std::collections::{BTreeMap, BTreeSet};

use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ti4_content::ContentStore;
use ti4_content::galaxy::Galaxy;
use ti4_model::content_types::SourceSet;
use ti4_model::id::{ObjectiveId, PlanetId, PlayerId, SystemId, UnitTypeId};
use ti4_model::state::{GameState, SystemState};
use ti4_model::units::Unit;

use crate::movement::{Board, MovementRules};

/// The kind used by a declining option.
pub const DECLINE_KIND: &str = "decline";
/// The id used by a declining option.
pub const DECLINE_ID: &str = "decline";

/// One legal thing an actor may pick.
///
/// `id` must be stable for a given game state so decision logs replay faithfully.
#[derive(Debug, Clone, Eq, Serialize, Deserialize)]
pub struct ChoiceOption {
    pub id: String,
    pub kind: String,
    pub label: String,
    /// Structured detail for whatever will apply this option. Excluded from equality, as in
    /// the oracle: two options with the same id are the same option, and a payload that
    /// differed would mean the id was not stable.
    pub payload: BTreeMap<String, Value>,
}

impl PartialEq for ChoiceOption {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id && self.kind == other.kind && self.label == other.label
    }
}

impl ChoiceOption {
    #[must_use]
    pub fn new(id: impl Into<String>, kind: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            kind: kind.into(),
            label: String::new(),
            payload: BTreeMap::new(),
        }
    }

    #[must_use]
    pub fn labelled(
        id: impl Into<String>,
        kind: impl Into<String>,
        label: impl Into<String>,
    ) -> Self {
        Self {
            label: label.into(),
            ..Self::new(id, kind)
        }
    }

    /// The standard "do nothing" option.
    #[must_use]
    pub fn decline() -> Self {
        Self::labelled(DECLINE_ID, DECLINE_KIND, "Decline")
    }

    #[must_use]
    pub fn is_decline(&self) -> bool {
        self.kind == DECLINE_KIND
    }

    /// Attach structured detail. Does not affect equality or the option's identity.
    #[must_use]
    pub fn with(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.payload.insert(key.into(), value.into());
        self
    }

    /// What to show: the label if there is one, else the id.
    #[must_use]
    pub fn display(&self) -> &str {
        if self.label.is_empty() {
            &self.id
        } else {
            &self.label
        }
    }
}

/// A decision point handed to exactly one actor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Choice {
    pub player: PlayerId,
    pub prompt: String,
    pub options: Vec<ChoiceOption>,
}

impl Choice {
    #[must_use]
    pub fn new(player: PlayerId, prompt: impl Into<String>, options: Vec<ChoiceOption>) -> Self {
        Self {
            player,
            prompt: prompt.into(),
            options,
        }
    }

    #[must_use]
    pub fn option(&self, option_id: &str) -> Option<&ChoiceOption> {
        self.options.iter().find(|o| o.id == option_id)
    }

    #[must_use]
    pub fn ids(&self) -> Vec<&str> {
        self.options.iter().map(|o| o.id.as_str()).collect()
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.options.is_empty()
    }
}

/// A decider returned something that was not on offer.
///
/// This is the boundary that keeps an LLM or a buggy bot from inventing moves.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum IllegalChoice {
    #[error("{player} chose {chosen:?}, which was not offered: {offered:?}")]
    NotOffered {
        player: PlayerId,
        chosen: String,
        offered: Vec<String>,
    },
    #[error("script wanted {wanted:?} but {player} was offered {offered:?}")]
    ScriptDiverged {
        player: PlayerId,
        wanted: String,
        offered: Vec<String>,
    },
    #[error("{player} was asked {prompt:?} with no options")]
    NoOptions { player: PlayerId, prompt: String },
}

/// Reject any answer that was not among the offered options.
///
/// # Errors
/// [`IllegalChoice::NotOffered`].
pub fn validate(choice: &Choice, option: ChoiceOption) -> Result<ChoiceOption, IllegalChoice> {
    if choice.option(&option.id).is_some() {
        return Ok(option);
    }
    Err(IllegalChoice::NotOffered {
        player: choice.player.clone(),
        chosen: option.id,
        offered: choice.ids().into_iter().map(str::to_owned).collect(),
    })
}

/// What a window needs to resolve an answer: the corpus it reads against, and the pinned
/// random source.
///
/// This exists because a window that rolls dice cannot be given a fresh generator per call —
/// it would silently leave the game's seeded stream, and a replayed game would diverge with
/// nothing reporting it. Bundling them also keeps [`Window::resolve`] to one shape whether the
/// subsystem rolls anything or not.
pub struct Resolving<'a> {
    pub content: &'a ti4_content::ContentStore,
    pub sources: ti4_model::content_types::SourceSet,
    pub dice: &'a mut crate::dice::Dice,
    pub rng: &'a mut crate::rng::GameRng,
    /// Who answers questions raised *while* resolving.
    ///
    /// A window's own decisions come through [`Window::drive`], but resolving one can raise
    /// another: exploring a planet taken in an invasion draws a card that must be kept or
    /// discarded. Without the table here those follow-ups had to be decided by the engine on
    /// the player's behalf, which is a decision made silently rather than asked.
    pub table: &'a mut Table,
    /// The typed-event machinery, when the caller has it.
    ///
    /// A subsystem has to be able to emit *at the moment the thing happens*, not afterwards. A
    /// reaction to "at the start of a combat round" that fires once the round has resolved
    /// applies its bonus to the wrong round, so a driver that emitted around the window instead
    /// of inside it would be wrong rather than merely coarse.
    ///
    /// Optional because several callers — tests, and paths with no timing machinery — have no
    /// resolver to offer. Without one [`Resolving::emit`] does nothing and says so.
    pub timing: Option<TimingHandle<'a>>,
}

impl Resolving<'_> {
    /// Ask a nested choice with the public position available to the decider.
    ///
    /// Resumable windows are sometimes driven outside [`crate::game::Game`], where there is no
    /// map handle to attach.  The board state and content are still available and must not be
    /// discarded: a learned decider's position-free `choose` path deliberately cannot score.
    ///
    /// # Errors
    /// Returns [`IllegalChoice`] if the decider selects an option that was not offered.
    pub fn ask_seeing(
        &mut self,
        state: &ti4_model::state::GameState,
        choice: &Choice,
    ) -> Result<ChoiceOption, IllegalChoice> {
        self.table.ask_seeing(
            choice,
            &Observed::new(state, self.content, self.sources, None),
        )
    }
}

/// The pieces needed to put a typed event through the resolver.
pub struct TimingHandle<'a> {
    /// The resolver whose windows the event opens.
    pub resolver: &'a mut crate::timing::Resolver,
    /// The game's typed-event allocator, shared so nested emissions keep one numbering.
    pub sequence: &'a mut crate::event::EventSequence,
    /// The map, for rules that ask about the shape of the board.
    pub galaxy: Option<&'a ti4_content::galaxy::Galaxy>,
}

impl Resolving<'_> {
    /// Emit a typed event, opening its WHEN and AFTER windows.
    ///
    /// Returns whether the event survived: a cancelled event did not happen, and its caller must
    /// not carry on as though it did.
    ///
    /// # Errors
    /// [`crate::timing::TimingError`] when a decider answers illegally or the event id space is
    /// exhausted.
    pub fn emit(
        &mut self,
        state: &mut ti4_model::state::GameState,
        event_type: &str,
        payload: std::collections::BTreeMap<String, serde_json::Value>,
    ) -> Result<bool, crate::timing::TimingError> {
        let Some(handle) = self.timing.as_mut() else {
            return Ok(true); // no resolver: nothing can react, and the event still happened
        };
        let event = handle.sequence.next(event_type, payload)?;
        let mut context = crate::timing::TimingContext {
            state,
            content: self.content,
            sources: self.sources,
            table: self.table,
            dice: self.dice,
            rng: self.rng,
            event_sequence: handle.sequence,
            galaxy: handle.galaxy,
        };
        let emitted = handle
            .resolver
            .emit_with_context(&mut context, event, |_, _| {})?;
        Ok(!emitted.cancelled)
    }
}

/// A decision sequence the game driver can step one answer at a time.
///
/// The engine had five hand-rolled versions of this shape before it was named — strategy
/// secondary, token gain, scoring, voting, cargo — and three subsystems that skipped it and
/// asked inline instead, which is what broke the driver's one-decision-per-step contract.
///
/// Completion is "no choice is owed" rather than a separate flag, so a window cannot report
/// itself finished while still holding a question, or hold a question after it is done.
///
/// # Example
///
/// The contract, shown with a window small enough to read. A real one — production, combat,
/// cargo — differs only in what it asks and what answering does.
///
/// ```
/// use ti4_engine::choice::{Choice, ChoiceOption, IllegalChoice, Resolving, Window};
/// use ti4_model::id::PlayerId;
///
/// /// Asks a player to name a colour, once.
/// struct PickAColour {
///     asked: bool,
/// }
///
/// impl Window for PickAColour {
///     fn pending_choice(
///         &self,
///         _state: &ti4_model::state::GameState,
///         _content: &ti4_content::ContentStore,
///         _sources: ti4_model::content_types::SourceSet,
///     ) -> Option<Choice> {
///         // No flag says "finished": the window is done when it owes no question.
///         if self.asked {
///             return None;
///         }
///         Some(Choice::new(
///             PlayerId::new("a"),
///             "pick a colour",
///             vec![
///                 ChoiceOption::labelled("red", "colour", "red"),
///                 ChoiceOption::labelled("blue", "colour", "blue"),
///             ],
///         ))
///     }
///
///     fn resolve(
///         &mut self,
///         _state: &mut ti4_model::state::GameState,
///         _ctx: &mut Resolving<'_>,
///         _answer: ChoiceOption,
///     ) -> Result<(), IllegalChoice> {
///         self.asked = true;
///         Ok(())
///     }
/// }
///
/// let content = ti4_content::ContentStore::embedded();
/// let sources = ti4_model::content_types::POK;
/// let state =
///     ti4_engine::setup::start_game(content, &[PlayerId::new("a")], sources, None).unwrap();
///
/// let mut window = PickAColour { asked: false };
/// let choice = window
///     .pending_choice(&state, content, sources)
///     .expect("a colour is owed");
/// assert_eq!(choice.options.len(), 2);
/// ```
pub trait Window {
    /// The decision currently owed, or `None` when the sequence is finished.
    fn pending_choice(
        &self,
        state: &ti4_model::state::GameState,
        content: &ti4_content::ContentStore,
        sources: ti4_model::content_types::SourceSet,
    ) -> Option<Choice>;

    /// Apply one answer.
    ///
    /// # Errors
    /// [`IllegalChoice`] when the answer was not one of the generated options.
    fn resolve(
        &mut self,
        state: &mut ti4_model::state::GameState,
        ctx: &mut Resolving<'_>,
        answer: ChoiceOption,
    ) -> Result<(), IllegalChoice>;

    /// Drive the whole sequence against a table, for callers that do not need to step it.
    ///
    /// # Errors
    /// [`IllegalChoice`] when a decider answers with something not offered.
    fn drive(
        &mut self,
        state: &mut ti4_model::state::GameState,
        ctx: &mut Resolving<'_>,
    ) -> Result<(), IllegalChoice>
    where
        Self: Sized,
    {
        while let Some(choice) = self.pending_choice(state, ctx.content, ctx.sources) {
            let answer = ctx.ask_seeing(state, &choice)?;
            self.resolve(state, ctx, answer)?;
        }
        Ok(())
    }
}

// --- what a decider may see -------------------------------------------------------------------

/// The public position, offered to a decider alongside the choice.
///
/// A choice on its own is not enough to play well. "Activate a system" lists ids; whether one of
/// them is worth a command token depends on what is in it, what defends it, and whether anything
/// of yours can reach it - facts about the board, not about the choice. A bot without them
/// activates systems its fleet cannot reach, which is legal, achieves nothing, and is exactly how
/// a scored bot came to move twice as many ships as a random one and still not score.
///
/// **Only public facts are reachable through this type.** The state is held privately and every
/// accessor answers something any player at the table may read: the board, who controls what, how
/// many cards somebody holds. A hand's *contents* are reachable only through
/// [`Observed::redacted_for`], which copies and redacts - named so that reading private
/// information is a deliberate act with a visible cost, rather than a field access.
///
/// The Rust counterpart of the oracle's `views.GameView`, differently shaped for the reason it
/// exists at all: the oracle hands a bot a facade over a live game, and Rust cannot hand a decider
/// a reference to the game that owns it.
pub struct Observed<'a> {
    state: &'a GameState,
    content: &'a ContentStore,
    sources: SourceSet,
    galaxy: Option<&'a Galaxy>,
}

impl<'a> Observed<'a> {
    /// Wrap a position. Public so tests and sibling crates can build one.
    #[must_use]
    pub const fn new(
        state: &'a GameState,
        content: &'a ContentStore,
        sources: SourceSet,
        galaxy: Option<&'a Galaxy>,
    ) -> Self {
        Self {
            state,
            content,
            sources,
            galaxy,
        }
    }

    /// The content corpus this game is played from.
    #[must_use]
    pub const fn content(&self) -> &'a ContentStore {
        self.content
    }

    /// The source scope in play.
    #[must_use]
    pub const fn sources(&self) -> SourceSet {
        self.sources
    }

    /// The map, when the game has one.
    #[must_use]
    pub const fn galaxy(&self) -> Option<&'a Galaxy> {
        self.galaxy
    }

    /// The round number.
    #[must_use]
    pub const fn round(&self) -> u32 {
        self.state.round
    }

    /// Every system holding anything. Absent systems are empty.
    #[must_use]
    pub const fn board(&self) -> &'a BTreeMap<SystemId, SystemState> {
        &self.state.board
    }

    /// One system's contents.
    #[must_use]
    pub fn system(&self, system: &SystemId) -> SystemState {
        self.state.system_state(system)
    }

    /// The system currently activated for a tactical action, when there is one.
    ///
    /// An activation token is public; exposing the active system lets a policy value the legal
    /// movement options it was offered without handing it the game state that owns the choice.
    #[must_use]
    pub fn active_system(&self) -> Option<&'a SystemId> {
        self.state.active_system.as_ref()
    }

    /// Whether a ship with this printed movement can reach a public destination.
    ///
    /// This is an observation query, not a second legality entry point. It reads only the public
    /// map, board occupancy, command tokens, and laws, then reuses the engine's movement search
    /// so a policy does not approximate a route with geometric distance. Effects from a card that
    /// has not been played are deliberately absent: a bot with no hand must not infer them.
    #[must_use]
    pub fn can_reach(
        &self,
        player: &PlayerId,
        origin: &SystemId,
        destination: &SystemId,
        move_value: i32,
    ) -> bool {
        let Some(galaxy) = self.galaxy else {
            return false;
        };
        let board = Board::for_player(self.state, self.content, self.sources, player);
        MovementRules::with_laws(
            galaxy,
            self.content,
            self.sources,
            destination.as_str(),
            board,
            Some(self.state),
        )
        .can_reach(origin.as_str(), move_value)
    }

    /// `(system, planet)` for every planet a player controls.
    #[must_use]
    pub fn controlled_planets(&self, player: &PlayerId) -> Vec<(&'a SystemId, &'a PlanetId)> {
        self.state.controlled_planets(player)
    }

    /// Public resources or influence a player can currently spend.
    ///
    /// This deliberately returns an aggregate rather than the exhausted-card set. Ready planet
    /// cards and trade goods are visible at the table; the engine remains the single owner of
    /// the exact payment accounting used by policy and later factual feature capture.
    #[must_use]
    pub fn available_spend(&self, player: &PlayerId, kind: crate::production::Spend) -> i64 {
        crate::production::available(self.state, self.content, self.sources, player, kind)
    }

    /// Systems holding any of a player's units.
    #[must_use]
    pub fn systems_with_units_of(&self, player: &PlayerId) -> BTreeSet<&'a SystemId> {
        self.state.systems_with_units_of(player)
    }

    /// Systems already holding a player's command token, which 89.1 forbids activating.
    #[must_use]
    pub fn systems_with_token(&self, player: &PlayerId) -> BTreeSet<&'a SystemId> {
        self.state.systems_with_token(player)
    }

    /// A seat's public standing: what anybody at the table can count.
    #[must_use]
    pub fn seat(&self, player: &PlayerId) -> Option<PublicSeat<'a>> {
        self.state.player(player).map(|seat| PublicSeat {
            faction: &seat.faction,
            victory_points: seat.victory_points,
            trade_goods: seat.trade_goods,
            commodities: seat.commodities,
            tactic_tokens: seat.tactic_tokens,
            fleet_tokens: seat.fleet_tokens,
            strategic_tokens: seat.strategic_tokens,
            technologies: &seat.technologies,
            action_cards_held: seat.action_cards.len(),
            secret_objectives_held: seat.secret_objectives.len(),
            passed: seat.passed,
        })
    }

    /// The seats, in seating order.
    #[must_use]
    pub fn players(&self) -> Vec<&'a PlayerId> {
        self.state.players.iter().map(|seat| &seat.id).collect()
    }

    /// Objectives revealed so far, which are faceup and public.
    #[must_use]
    pub const fn revealed_objectives(&self) -> &'a [ObjectiveId] {
        self.state.revealed_objectives.as_slice()
    }

    /// What a player has already scored, which is public once scored (61.18).
    #[must_use]
    pub fn scored_by(&self, player: &PlayerId) -> BTreeSet<ObjectiveId> {
        self.state.scored_by(player)
    }

    /// Every unit this player owns, in space **and** on planets.
    ///
    /// Ground forces and structures live on planets rather than in the space area, and counting
    /// only the space area makes "the seat built something" mean "the seat built a ship".
    #[must_use]
    pub fn units_held(&self, player: &PlayerId) -> usize {
        self.state
            .board
            .values()
            .map(|system| {
                system.units_of(player).len()
                    + system
                        .planet_units
                        .values()
                        .flatten()
                        .filter(|unit| &unit.owner == player)
                        .count()
            })
            .sum()
    }

    /// How many revealed public objectives this seat could score right now.
    ///
    /// A rules predicate, not an opinion about which objective is worth chasing. It exists so a
    /// policy has something to climb before it ever scores: a four-round game yields about 1.49
    /// victory points per faction, which is far too sparse to learn from on its own.
    #[must_use]
    pub fn scoreable_public(&self, player: &PlayerId) -> usize {
        crate::objectives::scoreable_on(self.state, self.content, self.sources, player, self.galaxy)
            .len()
    }

    /// The same for the secrets this seat holds.
    ///
    /// Private to its holder, and answered only for the seat asking — which is the one case where
    /// reading a hand is not reading somebody else's.
    #[must_use]
    pub fn scoreable_secret(&self, player: &PlayerId) -> usize {
        crate::secrets::scoreable_on(self.state, self.content, self.sources, player, self.galaxy)
            .len()
    }

    /// A full state with every other player's private holdings replaced by markers.
    ///
    /// Copies. That is the point: reading private information should cost something visible, so
    /// nobody reaches for it to answer a question the public accessors already answer.
    #[must_use]
    pub fn redacted_for(&self, viewer: &PlayerId) -> GameState {
        let mut view = self.state.clone();
        for seat in &mut view.players {
            if &seat.id != viewer {
                seat.action_cards = seat
                    .action_cards
                    .iter()
                    .map(|_| ti4_model::id::ActionCardId::new(HIDDEN))
                    .collect();
                seat.secret_objectives = seat
                    .secret_objectives
                    .iter()
                    .map(|_| ti4_model::id::SecretObjectiveId::new(HIDDEN))
                    .collect();
            }
        }
        view
    }
}

/// Stands in for a card whose identity is private.
///
/// Not a valid alias anywhere, so a lookup against real content fails rather than quietly matching
/// something.
pub const HIDDEN: &str = "?";

/// A seat as the rest of the table sees it: counts, never identities.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicSeat<'a> {
    /// Which faction sits here.
    pub faction: &'a ti4_model::id::FactionId,
    /// Points scored so far.
    pub victory_points: i32,
    /// Trade goods, which sit faceup in the play area.
    pub trade_goods: i32,
    /// Commodities, likewise faceup.
    pub commodities: i32,
    /// Tokens in the tactic pool.
    pub tactic_tokens: i32,
    /// Tokens in the fleet pool.
    pub fleet_tokens: i32,
    /// Tokens in the strategy pool.
    pub strategic_tokens: i32,
    /// Technologies owned, which are faceup.
    pub technologies: &'a BTreeSet<ti4_model::id::TechnologyId>,
    /// How many cards, never which. At a table you can see a hand's size.
    pub action_cards_held: usize,
    /// The same for unscored secrets (61.17).
    pub secret_objectives_held: usize,
    /// Whether this seat has passed for the round.
    pub passed: bool,
}

/// Anything that can answer a [`Choice`].
///
/// `&mut self` because a decider may carry state — a script position, an RNG stream.
///
/// # Example
///
/// ```
/// use ti4_engine::choice::{Decider, Choice, ChoiceOption, IllegalChoice};
///
/// struct AlwaysDecline;
///
/// impl Decider for AlwaysDecline {
///     fn choose(&mut self, _choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
///         Ok(ChoiceOption::decline())
///     }
/// }
/// ```
pub trait Decider {
    /// # Errors
    /// [`IllegalChoice`] if the decider cannot answer, e.g. an exhausted script whose next
    /// wanted option was not offered.
    fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice>;

    /// Answer a choice with the public position in hand.
    ///
    /// Defaulted to [`Decider::choose`], so a scripted test or a random smoke run needs to know
    /// nothing about the board, and a scorer overrides only this one. The engine calls this at
    /// every site that has a position to offer, and calls `choose` at the rest — a window that
    /// owns a slice of the game rather than the whole of it cannot honestly produce one.
    ///
    /// # Errors
    /// As [`Decider::choose`].
    fn choose_seeing(
        &mut self,
        choice: &Choice,
        seen: &Observed<'_>,
    ) -> Result<ChoiceOption, IllegalChoice> {
        let _ = seen;
        self.choose(choice)
    }
}

/// Always take the first option. Deterministic; the default in tests.
#[derive(Debug, Clone, Copy, Default)]
pub struct FirstOption;

impl Decider for FirstOption {
    fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
        choice
            .options
            .first()
            .cloned()
            .ok_or_else(|| IllegalChoice::NoOptions {
                player: choice.player.clone(),
                prompt: choice.prompt.clone(),
            })
    }
}

/// Decline whenever declining is legal, else take the first option.
#[derive(Debug, Clone, Copy, Default)]
pub struct AlwaysDecline;

impl Decider for AlwaysDecline {
    fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
        choice
            .options
            .iter()
            .find(|o| o.is_decline())
            .cloned()
            .map_or_else(|| FirstOption.choose(choice), Ok)
    }
}

/// Answer from a fixed sequence of option ids, falling back once exhausted.
///
/// The workhorse for conformance tests: it states the exact line of play a scenario is
/// asserting, and fails loudly if the engine offers something unexpected.
pub struct Scripted {
    queue: std::collections::VecDeque<String>,
    fallback: Box<dyn Decider>,
}

impl Scripted {
    #[must_use]
    pub fn new<I, S>(option_ids: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self::with_fallback(option_ids, Box::new(FirstOption))
    }

    #[must_use]
    pub fn with_fallback<I, S>(option_ids: I, fallback: Box<dyn Decider>) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            queue: option_ids.into_iter().map(Into::into).collect(),
            fallback,
        }
    }

    /// How many scripted answers remain.
    #[must_use]
    pub fn remaining(&self) -> usize {
        self.queue.len()
    }
}

impl Decider for Scripted {
    fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
        let Some(wanted) = self.queue.pop_front() else {
            return self.fallback.choose(choice);
        };
        choice
            .option(&wanted)
            .cloned()
            .ok_or_else(|| IllegalChoice::ScriptDiverged {
                player: choice.player.clone(),
                wanted,
                offered: choice.ids().into_iter().map(str::to_owned).collect(),
            })
    }
}

/// Uniform random over legal options from a seed.
///
/// Not a bot. This is what drives bot-versus-bot smoke runs that assert every game
/// terminates and no illegal state is reachable.
///
/// **The stream is not the oracle's.** The oracle uses Python's Mersenne Twister via
/// `random.Random(seed)`; this uses `ChaCha8`, which is reproducible across platforms and
/// Rust versions in a way Python's is not. The same seed therefore plays a *different* legal
/// game. Reproducing an oracle game needs its decision log replayed through [`Scripted`],
/// or the legacy entropy translator planned in M03-007.
pub struct SeededRandom {
    rng: ChaCha8Rng,
}

impl SeededRandom {
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self {
            rng: ChaCha8Rng::seed_from_u64(seed),
        }
    }
}

impl Decider for SeededRandom {
    fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
        if choice.options.is_empty() {
            return Err(IllegalChoice::NoOptions {
                player: choice.player.clone(),
                prompt: choice.prompt.clone(),
            });
        }
        let index = self.rng.random_range(0..choice.options.len());
        Ok(choice.options[index].clone())
    }
}

/// One resolved choice, for replay (determinism) and for explaining bot play.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecisionRecord {
    pub player: PlayerId,
    pub prompt: String,
    pub chosen: String,
    pub offered: Vec<String>,
}

/// Ordered record of every choice made, sufficient to replay a game.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecisionLog {
    pub records: Vec<DecisionRecord>,
}

impl DecisionLog {
    pub fn record(&mut self, choice: &Choice, option: &ChoiceOption) {
        self.records.push(DecisionRecord {
            player: choice.player.clone(),
            prompt: choice.prompt.clone(),
            chosen: option.id.clone(),
            offered: choice.ids().into_iter().map(str::to_owned).collect(),
        });
    }

    /// Replay script — the chosen option ids, optionally for one player.
    #[must_use]
    pub fn as_script(&self, player: Option<&PlayerId>) -> Vec<String> {
        self.records
            .iter()
            .filter(|r| player.is_none_or(|p| &r.player == p))
            .map(|r| r.chosen.clone())
            .collect()
    }

    #[must_use]
    pub const fn len(&self) -> usize {
        self.records.len()
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.records.is_empty()
    }
}

/// Deciders by player, with a default for anyone unassigned.
///
/// # Example
///
/// ```
/// use ti4_engine::choice::{Table, Decider, Scripted};
/// use ti4_model::id::PlayerId;
///
/// let mut table = Table::with_default(Box::new(Scripted::new(vec![String::new()])));
/// table.seat(PlayerId::new("a"), Box::new(Scripted::new(vec!["first".to_owned()])));
/// ```
pub struct Table {
    deciders: BTreeMap<PlayerId, Box<dyn Decider>>,
    default: Box<dyn Decider>,
    pub log: DecisionLog,
}

impl Default for Table {
    fn default() -> Self {
        Self {
            deciders: BTreeMap::new(),
            default: Box::new(FirstOption),
            log: DecisionLog::default(),
        }
    }
}

impl Table {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// A table where everyone unassigned uses this decider.
    #[must_use]
    pub fn with_default(default: Box<dyn Decider>) -> Self {
        Self {
            default,
            ..Self::default()
        }
    }

    pub fn seat(&mut self, player: PlayerId, decider: Box<dyn Decider>) {
        self.deciders.insert(player, decider);
    }

    /// Put a choice to its actor, validate the answer, and record it.
    ///
    /// # Errors
    /// [`IllegalChoice`] if the answer was not on offer — the boundary that stops a bot
    /// inventing a move.
    pub fn ask(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
        let decider = self
            .deciders
            .get_mut(&choice.player)
            .unwrap_or(&mut self.default);
        let answer = decider.choose(choice)?;
        self.settle(choice, answer)
    }

    /// Put a choice to its actor along with the public position.
    ///
    /// Identical to [`Table::ask`] except for what the decider is shown, and the answer goes
    /// through the same validation and the same log — so a game driven through this path replays
    /// through the other one.
    ///
    /// # Errors
    /// As [`Table::ask`].
    pub fn ask_seeing(
        &mut self,
        choice: &Choice,
        seen: &Observed<'_>,
    ) -> Result<ChoiceOption, IllegalChoice> {
        let decider = self
            .deciders
            .get_mut(&choice.player)
            .unwrap_or(&mut self.default);
        let answer = decider.choose_seeing(choice, seen)?;
        self.settle(choice, answer)
    }

    /// Validate an answer and record it. Shared, so the two ask paths cannot drift.
    fn settle(
        &mut self,
        choice: &Choice,
        answer: ChoiceOption,
    ) -> Result<ChoiceOption, IllegalChoice> {
        let option = validate(choice, answer)?;
        self.log.record(choice, &option);
        Ok(option)
    }
}

// ─── building options ──────────────────────────────────────────────────────────

/// Build options from `(id, label)` pairs.
#[must_use]
pub fn options_from<'a, I>(items: I, kind: &str) -> Vec<ChoiceOption>
where
    I: IntoIterator<Item = (&'a str, &'a str)>,
{
    items
        .into_iter()
        .map(|(id, label)| ChoiceOption::labelled(id, kind, label))
        .collect()
}

/// The first index of each distinct item, keeping the original order.
///
/// The shape behind every duplicate-option fix in the engine: build options from
/// *distinguishable* things rather than from every thing, keeping the first index so the
/// option id still points at a real element.
#[must_use]
pub fn first_of_each<T, K, F>(items: &[T], key: F) -> Vec<(usize, &T)>
where
    K: Ord,
    F: Fn(&T) -> K,
{
    let mut seen = BTreeSet::new();
    items
        .iter()
        .enumerate()
        .filter(|(_, item)| seen.insert(key(item)))
        .collect()
}

/// How a unit is distinguished when offering it as an option.
///
/// Owner is not in the key: every caller is choosing among one player's own units.
pub type UnitKey = (UnitTypeId, bool);

/// The first index of each *distinguishable* unit, keyed by type and damage.
///
/// A unit is its type, its owner, and whether it has taken damage. Units matching on those
/// are interchangeable, so offering one option each is offering the same move several times
/// — and that is not merely verbose. **Deciders weigh options one by one**, and a sampling
/// bot draws from the option list, so a move written five times drew five times the
/// probability of an equally good move written once. In the oracle a player holding five
/// fighters and one dreadnought assigned its hits to a fighter five times in six no matter
/// what its scoring thought of the trade, because the count decided rather than the score.
#[must_use]
pub fn distinct_units(units: &[Unit]) -> Vec<(usize, &Unit)> {
    first_of_each(units, |u: &Unit| {
        (u.type_id.clone(), u.sustained_damage) as UnitKey
    })
}

/// `destroy dreadnought` / `destroy dreadnought (damaged)`.
///
/// Damage is shown rather than folded away: losing a ship that has already taken a hit is a
/// different proposition from losing a fresh one, and collapsing the two would hide a real
/// choice instead of removing a false one.
#[must_use]
pub fn unit_label(verb: &str, type_id: &UnitTypeId, damaged: bool) -> String {
    let suffix = if damaged { " (damaged)" } else { "" };
    format!("{verb} {type_id}{suffix}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pid(id: &str) -> PlayerId {
        PlayerId::new(id)
    }

    fn unit(type_id: &str, damaged: bool) -> Unit {
        Unit {
            sustained_damage: damaged,
            ..Unit::new(UnitTypeId::new(type_id), pid("a"))
        }
    }

    fn choice(options: Vec<ChoiceOption>) -> Choice {
        Choice::new(pid("a"), "pick one", options)
    }

    fn three() -> Choice {
        choice(vec![
            ChoiceOption::labelled("x", "action", "Do X"),
            ChoiceOption::labelled("y", "action", "Do Y"),
            ChoiceOption::decline(),
        ])
    }

    // -- options ---------------------------------------------------------------

    #[test]
    fn an_option_is_identified_by_its_id_not_its_payload() {
        // A payload that changed equality would mean the id was not stable, which is
        // exactly what replay depends on.
        let bare = ChoiceOption::new("move", "movement");
        let loaded = ChoiceOption::new("move", "movement").with("from", "18");
        assert_eq!(bare, loaded);
        assert_ne!(bare.payload, loaded.payload);
    }

    #[test]
    fn an_option_displays_its_label_or_falls_back_to_its_id() {
        assert_eq!(ChoiceOption::labelled("x", "k", "Do X").display(), "Do X");
        assert_eq!(ChoiceOption::new("x", "k").display(), "x");
    }

    #[test]
    fn declining_is_recognised_by_kind_not_by_id() {
        assert!(ChoiceOption::decline().is_decline());
        assert!(!ChoiceOption::new("x", "action").is_decline());
        // A differently-named decline still counts.
        assert!(ChoiceOption::new("pass_window", DECLINE_KIND).is_decline());
    }

    #[test]
    fn a_choice_finds_its_options_by_id() {
        let c = three();
        assert_eq!(c.option("y").unwrap().label, "Do Y");
        assert!(c.option("nonesuch").is_none());
        assert_eq!(c.ids(), vec!["x", "y", "decline"]);
    }

    // -- validation ------------------------------------------------------------

    #[test]
    fn an_answer_that_was_not_offered_is_rejected() {
        // The boundary that keeps a bot or an LLM from inventing a move.
        let err = validate(&three(), ChoiceOption::new("invented", "action")).unwrap_err();
        assert!(
            matches!(err, IllegalChoice::NotOffered { ref chosen, .. } if chosen == "invented"),
            "{err}"
        );
    }

    #[test]
    fn an_answer_that_was_offered_passes_through() {
        let answer = validate(&three(), ChoiceOption::new("y", "action")).unwrap();
        assert_eq!(answer.id, "y");
    }

    #[test]
    fn the_rejection_names_what_was_on_offer() {
        let err = validate(&three(), ChoiceOption::new("z", "action")).unwrap_err();
        let message = err.to_string();
        assert!(message.contains('a'), "{message}");
        assert!(message.contains('z'), "{message}");
        assert!(message.contains("decline"), "{message}");
    }

    // -- deciders ---------------------------------------------------------------

    #[test]
    fn first_option_is_deterministic() {
        assert_eq!(FirstOption.choose(&three()).unwrap().id, "x");
        assert_eq!(FirstOption.choose(&three()).unwrap().id, "x");
    }

    #[test]
    fn always_decline_takes_the_decline_when_there_is_one() {
        assert_eq!(AlwaysDecline.choose(&three()).unwrap().id, "decline");
    }

    #[test]
    fn always_decline_falls_back_to_the_first_option() {
        let no_decline = choice(vec![
            ChoiceOption::new("x", "action"),
            ChoiceOption::new("y", "action"),
        ]);
        assert_eq!(AlwaysDecline.choose(&no_decline).unwrap().id, "x");
    }

    #[test]
    fn a_decider_asked_with_no_options_reports_it_rather_than_panicking() {
        let empty = choice(vec![]);
        assert!(matches!(
            FirstOption.choose(&empty).unwrap_err(),
            IllegalChoice::NoOptions { .. }
        ));
        assert!(matches!(
            SeededRandom::new(1).choose(&empty).unwrap_err(),
            IllegalChoice::NoOptions { .. }
        ));
    }

    #[test]
    fn a_script_states_the_exact_line_of_play() {
        let mut scripted = Scripted::new(["y", "decline"]);
        assert_eq!(scripted.choose(&three()).unwrap().id, "y");
        assert_eq!(scripted.choose(&three()).unwrap().id, "decline");
        assert_eq!(scripted.remaining(), 0);
    }

    #[test]
    fn a_script_that_diverges_fails_loudly() {
        // The point of a script: if the engine offers something unexpected the test must
        // fail, not quietly take a different line.
        let mut scripted = Scripted::new(["nonesuch"]);
        let err = scripted.choose(&three()).unwrap_err();
        assert!(
            matches!(err, IllegalChoice::ScriptDiverged { ref wanted, .. } if wanted == "nonesuch"),
            "{err}"
        );
    }

    #[test]
    fn an_exhausted_script_falls_back() {
        let mut scripted = Scripted::new(["y"]);
        assert_eq!(scripted.choose(&three()).unwrap().id, "y");
        assert_eq!(
            scripted.choose(&three()).unwrap().id,
            "x",
            "fallback is first"
        );
    }

    #[test]
    fn a_script_can_be_given_its_own_fallback() {
        let mut scripted = Scripted::with_fallback(Vec::<String>::new(), Box::new(AlwaysDecline));
        assert_eq!(scripted.choose(&three()).unwrap().id, "decline");
    }

    #[test]
    fn a_seeded_random_repeats_itself_and_only_picks_legal_options() {
        let run = || {
            let mut rng = SeededRandom::new(42);
            (0..20)
                .map(|_| rng.choose(&three()).unwrap().id)
                .collect::<Vec<_>>()
        };
        let first = run();
        assert_eq!(first, run(), "the same seed must play the same game");
        assert!(first.iter().all(|id| three().option(id).is_some()));
    }

    #[test]
    fn different_seeds_diverge() {
        let run = |seed| {
            let mut rng = SeededRandom::new(seed);
            (0..30)
                .map(|_| rng.choose(&three()).unwrap().id)
                .collect::<Vec<_>>()
        };
        assert_ne!(run(1), run(2));
    }

    // -- the table ----------------------------------------------------------------

    #[test]
    fn a_table_routes_a_choice_to_its_own_players_decider() {
        let mut table = Table::new();
        table.seat(pid("a"), Box::new(AlwaysDecline));
        assert_eq!(table.ask(&three()).unwrap().id, "decline");

        let other = Choice::new(pid("b"), "pick one", three().options);
        assert_eq!(table.ask(&other).unwrap().id, "x", "b uses the default");
    }

    #[test]
    fn a_table_records_every_answer() {
        let mut table = Table::new();
        table.ask(&three()).unwrap();
        table.ask(&three()).unwrap();

        assert_eq!(table.log.len(), 2);
        let record = &table.log.records[0];
        assert_eq!(record.player, pid("a"));
        assert_eq!(record.chosen, "x");
        assert_eq!(record.offered, vec!["x", "y", "decline"]);
        assert_eq!(record.prompt, "pick one");
    }

    #[test]
    fn a_table_rejects_an_invented_answer_before_recording_it() {
        struct Cheat;
        impl Decider for Cheat {
            fn choose(&mut self, _: &Choice) -> Result<ChoiceOption, IllegalChoice> {
                Ok(ChoiceOption::new("invented", "action"))
            }
        }
        let mut table = Table::new();
        table.seat(pid("a"), Box::new(Cheat));

        assert!(table.ask(&three()).is_err());
        assert!(table.log.is_empty(), "a rejected answer must not be logged");
    }

    // -- the decision log -----------------------------------------------------------

    #[test]
    fn a_log_replays_as_a_script() {
        let mut table = Table::with_default(Box::new(AlwaysDecline));
        table.ask(&three()).unwrap();
        table.ask(&three()).unwrap();

        let script = table.log.as_script(None);
        assert_eq!(script, vec!["decline", "decline"]);

        // Feeding it back to a Scripted decider reproduces the same answers.
        let mut replay = Scripted::new(script);
        assert_eq!(replay.choose(&three()).unwrap().id, "decline");
        assert_eq!(replay.choose(&three()).unwrap().id, "decline");
    }

    #[test]
    fn a_log_can_be_filtered_to_one_player() {
        let mut table = Table::new();
        table.ask(&three()).unwrap();
        table
            .ask(&Choice::new(pid("b"), "pick one", three().options))
            .unwrap();

        assert_eq!(table.log.as_script(Some(&pid("a"))).len(), 1);
        assert_eq!(table.log.as_script(None).len(), 2);
    }

    #[test]
    fn a_seeded_run_replays_exactly_from_its_own_log() {
        // The determinism guarantee: a seed plus a decision log reproduces a game.
        let options = || {
            choice(vec![
                ChoiceOption::new("p", "action"),
                ChoiceOption::new("q", "action"),
                ChoiceOption::new("r", "action"),
            ])
        };
        let mut table = Table::with_default(Box::new(SeededRandom::new(7)));
        for _ in 0..25 {
            table.ask(&options()).unwrap();
        }

        let mut replay = Table::with_default(Box::new(Scripted::new(table.log.as_script(None))));
        for _ in 0..25 {
            replay.ask(&options()).unwrap();
        }
        assert_eq!(replay.log, table.log);
    }

    // -- distinguishable options -------------------------------------------------------

    #[test]
    fn interchangeable_units_are_offered_once() {
        // Five fighters are one option, not five. A sampling bot draws from the option
        // list, so writing a move five times gives it five times the probability of an
        // equally good move written once.
        let units = vec![
            unit("fighter", false),
            unit("fighter", false),
            unit("fighter", false),
            unit("dreadnought", false),
            unit("fighter", false),
        ];
        let distinct = distinct_units(&units);
        assert_eq!(distinct.len(), 2);
        assert_eq!(distinct[0].0, 0, "the first fighter");
        assert_eq!(distinct[1].0, 3, "the dreadnought");
    }

    #[test]
    fn a_damaged_unit_is_a_different_option_from_a_fresh_one() {
        // Losing a ship that has already taken a hit is a different proposition.
        let units = vec![
            unit("dreadnought", false),
            unit("dreadnought", true),
            unit("dreadnought", false),
        ];
        let distinct = distinct_units(&units);
        assert_eq!(distinct.len(), 2);
        assert!(!distinct[0].1.sustained_damage);
        assert!(distinct[1].1.sustained_damage);
    }

    #[test]
    fn the_kept_index_points_at_a_real_element() {
        let units = vec![unit("fighter", false), unit("carrier", false)];
        for (index, unit) in distinct_units(&units) {
            assert_eq!(&units[index], unit);
        }
    }

    #[test]
    fn first_of_each_keeps_the_original_order() {
        let items = ["b", "a", "b", "c", "a"];
        let firsts = first_of_each(&items, |s: &&str| *s);
        assert_eq!(
            firsts.iter().map(|(i, s)| (*i, **s)).collect::<Vec<_>>(),
            vec![(0, "b"), (1, "a"), (3, "c")]
        );
    }

    #[test]
    fn a_unit_label_shows_damage_rather_than_folding_it_away() {
        let dread = UnitTypeId::new("dreadnought");
        assert_eq!(unit_label("destroy", &dread, false), "destroy dreadnought");
        assert_eq!(
            unit_label("destroy", &dread, true),
            "destroy dreadnought (damaged)"
        );
    }

    #[test]
    fn options_are_built_from_id_label_pairs() {
        let built = options_from([("x", "Do X"), ("y", "Do Y")], "action");
        assert_eq!(built.len(), 2);
        assert_eq!(built[0].id, "x");
        assert_eq!(built[0].label, "Do X");
        assert_eq!(built[1].kind, "action");
    }

    #[test]
    fn a_choice_round_trips_through_json() {
        let json = serde_json::to_string(&three()).unwrap();
        assert_eq!(serde_json::from_str::<Choice>(&json).unwrap(), three());
    }

    // --- what a decider may see ---------------------------------------------------------------

    use ti4_model::content_types::POK;

    fn watched() -> ti4_model::state::GameState {
        let mut state = crate::fixtures::game(&["a", "b"]);
        let seat = state.player_mut(&pid("b")).unwrap();
        seat.action_cards = vec![
            ti4_model::id::ActionCardId::new("sabotage"),
            ti4_model::id::ActionCardId::new("direct_hit"),
        ];
        seat.secret_objectives = vec![ti4_model::id::SecretObjectiveId::new("become_a_legend")];
        seat.victory_points = 4;
        state
    }

    #[test]
    fn a_seat_is_seen_as_counts_never_as_identities() {
        // The whole point of the type. At a table you can count somebody's cards without reading
        // them, and `PublicSeat` has no field that could carry one.
        let state = watched();
        let seen = Observed::new(&state, ContentStore::embedded(), POK, None);
        let rival = seen.seat(&pid("b")).expect("b is seated");

        assert_eq!(rival.action_cards_held, 2);
        assert_eq!(rival.secret_objectives_held, 1);
        assert_eq!(rival.victory_points, 4, "and public facts survive");
    }

    #[test]
    fn reading_a_hand_costs_a_copy_and_returns_markers() {
        let state = watched();
        let seen = Observed::new(&state, ContentStore::embedded(), POK, None);
        let view = seen.redacted_for(&pid("a"));

        let rival = view.player(&pid("b")).unwrap();
        assert_eq!(rival.action_cards.len(), 2, "the count is public");
        assert!(
            rival
                .action_cards
                .iter()
                .all(|card| card.as_str() == HIDDEN),
            "the names are not: {:?}",
            rival.action_cards
        );
        assert_eq!(rival.secret_objectives[0].as_str(), HIDDEN);

        let own = view.player(&pid("a")).unwrap();
        assert_eq!(own.id, pid("a"), "your own seat is untouched");
    }

    #[test]
    fn you_can_read_your_own_hand() {
        let state = watched();
        let seen = Observed::new(&state, ContentStore::embedded(), POK, None);
        let view = seen.redacted_for(&pid("b"));

        assert_eq!(
            view.player(&pid("b")).unwrap().action_cards[0].as_str(),
            "sabotage"
        );
    }

    #[test]
    fn the_marker_matches_no_real_card() {
        // A redacted hand must not resolve against content, or a bot reading it would find a card
        // nobody holds rather than failing.
        assert!(
            ContentStore::embedded()
                .get(ti4_model::content_types::ContentType::ActionCards, HIDDEN)
                .is_none()
        );
    }

    #[test]
    fn public_spend_capacity_counts_only_ready_planets_and_faceup_goods() {
        let mut state = watched();
        let (system, planet) = crate::fixtures::a_placed_planet();
        state
            .system_mut(&system)
            .set_control(planet.clone(), pid("a"));
        state.player_mut(&pid("a")).unwrap().trade_goods = 2;
        let resources = crate::production::planet_value(
            ContentStore::embedded(),
            POK,
            &planet,
            crate::production::Spend::Resources,
        );
        let influence = crate::production::planet_value(
            ContentStore::embedded(),
            POK,
            &planet,
            crate::production::Spend::Influence,
        );

        let seen = Observed::new(&state, ContentStore::embedded(), POK, None);
        assert_eq!(
            seen.available_spend(&pid("a"), crate::production::Spend::Resources),
            resources + 2
        );
        assert_eq!(
            seen.available_spend(&pid("a"), crate::production::Spend::Influence),
            influence + 2
        );

        state.exhaust_planet(planet);
        let seen = Observed::new(&state, ContentStore::embedded(), POK, None);
        assert_eq!(
            seen.available_spend(&pid("a"), crate::production::Spend::Resources),
            2
        );
        assert_eq!(
            seen.available_spend(&pid("a"), crate::production::Spend::Influence),
            2
        );
    }

    #[test]
    fn a_decider_that_does_not_look_gets_the_same_answer_either_way() {
        // The default on `choose_seeing` is what lets every scripted test and random smoke run
        // stay ignorant of the board. If it ever stopped delegating, those would silently change.
        let state = watched();
        let seen = Observed::new(&state, ContentStore::embedded(), POK, None);
        let asked = three();

        let mut blind = FirstOption;
        assert_eq!(
            blind.choose(&asked).unwrap(),
            blind.choose_seeing(&asked, &seen).unwrap()
        );
    }

    #[test]
    fn both_ask_paths_validate_and_record_alike() {
        // A game driven through `ask_seeing` must replay through `ask`, which needs the log and
        // the validation to be the same on both. They share `settle` for exactly that reason.
        let state = watched();
        let seen = Observed::new(&state, ContentStore::embedded(), POK, None);

        let mut blind = Table::new();
        blind.ask(&three()).unwrap();
        let mut looking = Table::new();
        looking.ask_seeing(&three(), &seen).unwrap();

        assert_eq!(blind.log.records, looking.log.records);
    }

    #[test]
    fn an_answer_that_was_not_offered_is_refused_on_the_seeing_path_too() {
        struct Inventing;
        impl Decider for Inventing {
            fn choose(&mut self, _choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
                Ok(ChoiceOption::new("not_offered", "invented"))
            }
        }
        let state = watched();
        let seen = Observed::new(&state, ContentStore::embedded(), POK, None);

        let mut table = Table::with_default(Box::new(Inventing));
        assert!(
            table.ask_seeing(&three(), &seen).is_err(),
            "the boundary holds on both paths"
        );
    }
}
