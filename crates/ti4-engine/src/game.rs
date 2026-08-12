//! Game-level choice stepping and bounded execution.

use std::collections::BTreeMap;

use ti4_content::ContentStore;
use ti4_content::galaxy::Galaxy;
use ti4_model::content_types::{POK, SourceSet};
use ti4_model::id::{PlayerId, StrategyCardId, SystemId};
use ti4_model::state::{GameState, Phase};
use ti4_model::units::Unit;

use crate::agenda::{AgendaPhaseError, resolve_agenda_phase};
use crate::choice::{Choice, ChoiceOption, IllegalChoice, Resolving, SeededRandom, Table, Window};
use crate::dice::Dice;
use crate::draft::{DraftError, strategy_options, take_strategy_card};
use crate::event::{EventSequence, EventSequenceError};
use crate::movement::{Board, MovementRules};
use crate::objectives::{ScoringError, ScoringWindow};
use crate::phase::{PhaseOutcome, advance_phase, advance_turn, begin_next_round};
use crate::rng::GameRng;
use crate::status::{
    StatusPhaseError, StatusPhaseReport, resolve_after_token_gain, resolve_before_token_gain,
};
use crate::strategy::{
    ACTION_KIND, SecondaryResolution, StrategyActionError, StrategySecondaryError,
    StrategySecondaryWindow, begin_strategic_action, strategic_action_options,
};
use crate::tactical::{
    MoveSelection, TacticalError, activate, activation_options, movable, movement_options,
    read_move,
};
use crate::timing::{Resolver, TimingContext, TimingError};
use crate::tokens::{TokenGain, TokenGainError};
use crate::transit::{CargoError, CargoWindow, MoveOutcome, apply_move, survives_gravity_rifts};
use crate::vote::{AGAINST, VoteError, VoteWindow, is_law, outcomes};

/// Metadata returned after one attempted game step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepResult {
    /// The phase after this step.
    pub phase: Phase,
    /// The player whose action is next, if the phase has one.
    pub active: Option<PlayerId>,
    /// Whether the game has reached its terminal state.
    pub finished: bool,
    /// A precise reason this driver cannot safely continue.
    pub error: Option<GameError>,
    /// Whether this step resolved one generated player choice.
    pub resolved_choice: bool,
}

/// A game-level failure that is safe to expose to a caller.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum GameError {
    #[error(transparent)]
    Timing(#[from] TimingError),
    #[error(transparent)]
    EventSequence(#[from] EventSequenceError),
    #[error(transparent)]
    IllegalChoice(#[from] IllegalChoice),
    #[error(transparent)]
    Draft(#[from] DraftError),
    #[error(transparent)]
    StrategyAction(#[from] StrategyActionError),
    #[error(transparent)]
    StrategySecondary(#[from] StrategySecondaryError),
    #[error(transparent)]
    Status(#[from] StatusPhaseError),
    #[error(transparent)]
    Agenda(#[from] AgendaPhaseError),
    #[error("action phase has no active player while players remain")]
    MissingActivePlayer,
    #[error("action {0:?} is not implemented by the structural game driver")]
    UnsupportedAction(String),
    #[error("timing cancelled required game event {0:?}")]
    TimingEventCancelled(String),
    #[error(transparent)]
    TokenGain(#[from] TokenGainError),
    #[error(transparent)]
    Scoring(#[from] ScoringError),
    #[error(transparent)]
    Tactical(#[from] TacticalError),
    #[error(transparent)]
    Cargo(#[from] CargoError),
    #[error(transparent)]
    Combat(#[from] crate::combat::CombatError),
    #[error(transparent)]
    Vote(#[from] VoteError),
}

/// A bounded `run` stopped instead of silently looping forever.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RunError {
    #[error(transparent)]
    Step(#[from] GameError),
    #[error("game did not progress within {max_steps} steps (round {round}, phase {phase:?})")]
    StepLimit {
        max_steps: usize,
        round: u32,
        phase: Phase,
    },
}

/// The action id that opens a tactical action.
pub const TACTICAL_ACTION_ID: &str = "tactical";

/// The steps after movement, as one resumable sequence (LRR 49 and around it).
///
/// Capacity enforcement and space cannon happen when it opens: neither is a player decision in
/// this engine yet — capacity only asks when something must be removed, and space cannon is
/// rolled, not chosen — and both must land before combat.
enum Aftermath {
    Fighting(Box<crate::combat::CombatWindow>),
    Invading(Box<crate::invasion::InvasionWindow>),
    Producing(Box<crate::production::ProductionWindow>),
    Done,
}

/// The post-movement sequence for one tactical action.
struct AftermathWindow {
    player: PlayerId,
    system: SystemId,
    stage: Aftermath,
    /// Events the window observed, drained by the driver.
    ///
    /// A window cannot reach `Game::emit`, and inventing a second event sink would give the
    /// game two logs that could disagree. Draining keeps one.
    log: Vec<String>,
}

impl AftermathWindow {
    fn new(
        state: &mut GameState,
        ctx: &mut Resolving<'_>,
        player: &PlayerId,
        system: &SystemId,
        galaxy: Option<&Galaxy>,
    ) -> Result<Self, GameError> {
        // Movement may take the only carrier out of a system and strand what it was holding, so
        // capacity is settled before anything shoots.
        crate::fleet::enforce(state, ctx.content, ctx.sources, ctx.table, player, system)
            .map_err(GameError::IllegalChoice)?;

        // Fired by everyone except the active player, before combat.
        let cannon = crate::combat::space_cannon_offense(
            state,
            ctx.content,
            ctx.sources,
            ctx.dice,
            ctx.rng,
            system,
            player,
        );
        for (_, hits) in cannon {
            crate::combat::absorb_hits(
                state,
                ctx.content,
                ctx.sources,
                ctx.table,
                player,
                system,
                hits,
            )?;
        }

        let mut window = crate::combat::CombatWindow::new(state, ctx.content, ctx.sources, system);
        if let Some(galaxy) = galaxy {
            window = window.with_galaxy(galaxy.clone());
        }
        window.settle_open(state, ctx);
        Ok(Self {
            player: player.clone(),
            system: system.clone(),
            stage: Aftermath::Fighting(Box::new(window)),
            log: Vec::new(),
        })
    }

    /// Move to the next step once the current one owes nothing.
    fn settle(&mut self, state: &mut GameState, ctx: &mut Resolving<'_>) {
        loop {
            match &mut self.stage {
                Aftermath::Fighting(window) => {
                    if window
                        .pending_choice(state, ctx.content, ctx.sources)
                        .is_some()
                    {
                        return;
                    }
                    // 49: an invasion only happens if the active player still holds the space.
                    let holds =
                        crate::combat::combatants(state, ctx.content, ctx.sources, &self.system)
                            .first()
                            .is_some_and(|last| last == &self.player);
                    if let Some(outcome) = window.outcome()
                        && outcome.rounds > 0
                    {
                        self.log.push("SPACE_COMBAT_RESOLVED".to_owned());
                    }
                    self.stage = if holds {
                        // Two cards read "at the start of an invasion", so the window opens
                        // before the invasion does rather than after it has resolved.
                        let mut payload = BTreeMap::new();
                        payload.insert(
                            "player".to_owned(),
                            serde_json::Value::String(self.player.to_string()),
                        );
                        payload.insert(
                            "system".to_owned(),
                            serde_json::Value::String(self.system.to_string()),
                        );
                        let _ = ctx.emit(state, "INVASION_BEGAN", payload);
                        Aftermath::Invading(Box::new(crate::invasion::InvasionWindow::new(
                            state,
                            ctx.content,
                            ctx.sources,
                            ctx.dice,
                            ctx.rng,
                            &self.player,
                            &self.system,
                        )))
                    } else {
                        Aftermath::Producing(Box::new(crate::production::ProductionWindow::new(
                            state,
                            ctx.content,
                            ctx.sources,
                            &self.player,
                            &self.system,
                        )))
                    };
                }
                Aftermath::Invading(window) => {
                    if window
                        .pending_choice(state, ctx.content, ctx.sources)
                        .is_some()
                    {
                        return;
                    }
                    self.log.push("INVASION_RESOLVED".to_owned());
                    self.stage =
                        Aftermath::Producing(Box::new(crate::production::ProductionWindow::new(
                            state,
                            ctx.content,
                            ctx.sources,
                            &self.player,
                            &self.system,
                        )));
                }
                Aftermath::Producing(window) => {
                    if window
                        .pending_choice(state, ctx.content, ctx.sources)
                        .is_some()
                    {
                        return;
                    }
                    // "When 1 or more of your units use PRODUCTION" — after the step, which is
                    // when the units have used it.
                    let mut payload = BTreeMap::new();
                    payload.insert(
                        "player".to_owned(),
                        serde_json::Value::String(self.player.to_string()),
                    );
                    payload.insert(
                        "system".to_owned(),
                        serde_json::Value::String(self.system.to_string()),
                    );
                    let _ = ctx.emit(state, "PRODUCTION_USED", payload);
                    self.log.push("PRODUCTION_RESOLVED".to_owned());
                    self.stage = Aftermath::Done;
                    return;
                }
                Aftermath::Done => return,
            }
        }
    }
}

impl Window for AftermathWindow {
    fn pending_choice(
        &self,
        state: &GameState,
        content: &ContentStore,
        sources: SourceSet,
    ) -> Option<Choice> {
        match &self.stage {
            Aftermath::Fighting(window) => window.pending_choice(state, content, sources),
            Aftermath::Invading(window) => window.pending_choice(state, content, sources),
            Aftermath::Producing(window) => window.pending_choice(state, content, sources),
            Aftermath::Done => None,
        }
    }

    fn resolve(
        &mut self,
        state: &mut GameState,
        ctx: &mut Resolving<'_>,
        answer: ChoiceOption,
    ) -> Result<(), IllegalChoice> {
        match &mut self.stage {
            Aftermath::Fighting(window) => window.resolve(state, ctx, answer)?,
            Aftermath::Invading(window) => window.resolve(state, ctx, answer)?,
            Aftermath::Producing(window) => window.resolve(state, ctx, answer)?,
            Aftermath::Done => {}
        }
        self.settle(state, ctx);
        Ok(())
    }
}

/// Where an open tactical action has reached.
///
/// The steps after movement — space cannon, space combat, invasion, production — are not
/// implemented. The action *finishes* rather than blocking, announcing `TACTICAL_STEPS_UNRESOLVED`
/// so the gap is visible: moving into an enemy system currently has no consequence.
#[derive(Debug, Clone)]
enum TacticalStage {
    /// Choosing which system to activate (89.1).
    Activating,
    /// Choosing a ship to move, or finishing (89.2b).
    Moving,
    /// Filling the selected ship's hold before it sails (LRR 95).
    Loading {
        origin: SystemId,
        ship: Box<Unit>,
        path: Vec<String>,
        window: Box<CargoWindow>,
    },
}

#[derive(Debug, Clone)]
struct TacticalWindow {
    player: PlayerId,
    stage: TacticalStage,
}

/// The stateful owner of generated choices, their decision log, and observable events.
///
/// The structure mirrors the oracle's `Game`: state remains public for inspection, while all
/// external decisions pass through [`Table`] and only generated choices are applied.
pub struct Game<'a> {
    pub state: GameState,
    pub table: Table,
    pub events: Vec<String>,
    /// The resolver that owns registered timing abilities for this game.
    /// The timing resolver. Public so wiring guards can read what a window actually did.
    pub timing: Resolver,
    /// One-based allocator for driver-emitted typed timing events.
    event_sequence: EventSequence,
    content: &'a ContentStore,
    /// The source scope this game is played under.
    ///
    /// Held here because `GameState` does not carry it. A game set up under a different scope
    /// and then driven with the default would resolve its planet catalogue against the wrong
    /// corpus, so this is set explicitly by [`Game::with_sources`] when it is not `PoK`.
    sources: SourceSet,
    strategy_cards: Vec<StrategyCardId>,
    secondary: Option<StrategySecondaryWindow>,
    /// The open 81.1 scoring window.
    scoring: Option<ScoringWindow>,
    /// The open 81.5 token gain, and the report its remaining steps will extend.
    tokens: Option<(TokenGain, Box<StatusPhaseReport>)>,
    /// The open agenda vote, and the agendas still to be put after it.
    voting: Option<(Box<VoteWindow>, Vec<String>)>,
    /// The map, when one has been built. Without it no tactical action is offered.
    galaxy: Option<Galaxy>,
    /// The open tactical action.
    tactical: Option<TacticalWindow>,
    /// The open post-movement sequence: combat, invasion, production.
    aftermath: Option<AftermathWindow>,
    /// The open transaction. Free (94.1a), so closing it does not end the turn.
    trade: Option<crate::transactions::TradeWindow>,
    /// The pinned source of gravity-rift rolls.
    rng: GameRng,
    dice: Dice,
    status_resolved: bool,
    agenda_resolved: bool,
    blocked: Option<GameError>,
}

impl<'a> Game<'a> {
    /// Create a game with the default first-option table.
    #[must_use]
    pub fn new(state: GameState, content: &'a ContentStore) -> Self {
        Self::with_table(state, content, Table::new())
    }

    /// Create a game whose every unseated player draws uniformly from one seeded legal stream.
    ///
    /// This is the oracle's `Table(default=SeededRandom(seed))` shape: the stream advances in
    /// decision order across all players, so it remains reproducible without assigning a
    /// potentially divergent RNG seed to each seat.
    #[must_use]
    pub fn with_seeded_random(state: GameState, content: &'a ContentStore, seed: u64) -> Self {
        let mut game = Self::with_table(
            state,
            content,
            Table::with_default(Box::new(SeededRandom::new(seed))),
        );
        // The dice share the game's seed, so a replayed game rolls the same rifts.
        game.rng = GameRng::new(seed);
        game
    }

    /// Create a game with explicit deciders for generated choices.
    #[must_use]
    pub fn with_table(state: GameState, content: &'a ContentStore, table: Table) -> Self {
        let mut timing =
            Resolver::new(state.initiative_order(), state.active.clone(), Table::new());
        // Standing reaction slots, registered once while the game is seated. The resolver has no
        // unregister — a "cannot" effect must not be removable (LRR 1.6) — so registering hands
        // instead of slots would leak an ability for every card ever drawn.
        crate::reactions::arm(&mut timing, &state);
        Self {
            strategy_cards: state.unclaimed_strategy_cards.clone(),
            state,
            table,
            events: Vec::new(),
            timing,
            event_sequence: EventSequence::new(),
            content,
            sources: POK,
            secondary: None,
            scoring: None,
            tokens: None,
            voting: None,
            galaxy: None,
            tactical: None,
            aftermath: None,
            trade: None,
            rng: GameRng::new(0),
            dice: Dice::new(),
            status_resolved: false,
            agenda_resolved: false,
            blocked: None,
        }
    }

    /// Give the game its map, which is what makes a tactical action possible.
    #[must_use]
    pub fn with_galaxy(mut self, galaxy: Galaxy) -> Self {
        self.galaxy = Some(galaxy);
        self
    }

    /// Play under a different source scope than the `PoK` default.
    #[must_use]
    pub const fn with_sources(mut self, sources: SourceSet) -> Self {
        self.sources = sources;
        self
    }

    /// Register or inspect timing abilities owned by this game.
    ///
    /// The driver synchronizes resolver priority metadata before each timed event and provides its
    /// single choice table through [`TimingContext`].
    pub fn timing_mut(&mut self) -> &mut Resolver {
        &mut self.timing
    }

    /// The choice currently offered, without resolving automatic followers or phase work.
    #[must_use]
    pub fn legal_options(&self) -> Option<Choice> {
        if self.state.finished || self.blocked.is_some() {
            return None;
        }
        if let Some(window) = &self.scoring {
            return window.pending_choice(&self.state, self.content, self.sources);
        }
        if let Some((window, _)) = &self.tokens {
            return window.pending_choice();
        }
        if let Some((window, _)) = &self.voting {
            return window.pending_choice(&self.state, self.content, self.sources);
        }
        if let Some(window) = &self.aftermath {
            return window.pending_choice(&self.state, self.content, self.sources);
        }
        if let Some(window) = &self.tactical {
            return self.tactical_choice(window);
        }
        match self.state.phase {
            Phase::Strategy => strategy_options(&self.state, self.content),
            Phase::Action => self.action_options(),
            Phase::Status | Phase::Agenda => None,
        }
    }

    /// Resolve one generated decision, or one choice-free phase/window transition.
    #[must_use]
    pub fn step(&mut self) -> StepResult {
        if self.state.finished {
            return self.result(false, None);
        }
        if let Some(error) = self.blocked.clone() {
            return self.result(false, Some(error));
        }

        if self.secondary.is_some() {
            return self.step_secondary();
        }
        if self.scoring.is_some() {
            return self.step_scoring();
        }
        if self.tokens.is_some() {
            return self.step_token_gain();
        }
        if self.voting.is_some() {
            return self.step_vote();
        }
        if self.aftermath.is_some() {
            return self.step_aftermath();
        }
        if self.trade.is_some() {
            return self.step_trade();
        }
        if self.tactical.is_some() {
            return self.step_tactical();
        }
        if self.state.phase == Phase::Status && !self.status_resolved {
            return self.step_status();
        }
        if self.state.phase == Phase::Agenda && !self.agenda_resolved {
            return self.step_agenda();
        }

        let Some(choice) = self.legal_options() else {
            return self.step_phase();
        };
        let answer = match self.table.ask(&choice) {
            Ok(answer) => answer,
            Err(error) => return self.result(false, Some(error.into())),
        };
        match self.apply_choice(&choice, answer) {
            Ok(()) => self.result(true, None),
            Err(error) => self.result(false, Some(error)),
        }
    }

    /// Play whole rounds, bounded so an incomplete or stalled model fails loudly.
    ///
    /// # Errors
    /// [`RunError::StepLimit`] when `max_steps` is reached, or [`RunError::Step`] when a required
    /// rule/choice remains deliberately unimplemented.
    pub fn run(&mut self, rounds: u32, max_steps: usize) -> Result<&GameState, RunError> {
        let target = self.state.round.saturating_add(rounds);
        let mut steps = 0;
        while self.state.round < target && !self.state.finished {
            if steps >= max_steps {
                return Err(RunError::StepLimit {
                    max_steps,
                    round: self.state.round,
                    phase: self.state.phase,
                });
            }
            let result = self.step();
            if let Some(error) = result.error {
                return Err(error.into());
            }
            steps += 1;
        }
        Ok(&self.state)
    }

    fn action_options(&self) -> Option<Choice> {
        if let Some(window) = &self.secondary {
            return window.pending_choice(&self.state);
        }
        let active = self.state.active.as_ref()?;
        let mut choice = strategic_action_options(&self.state, self.content, active)
            .unwrap_or_else(|| {
                Choice::new(
                    active.clone(),
                    "action phase",
                    vec![ChoiceOption::labelled("pass", ACTION_KIND, "pass")],
                )
            });
        // Appended rather than inserted: a table that always takes the first option keeps
        // taking the action it took before, so adding this does not silently rewrite the
        // behaviour of every existing seeded game.
        if self.can_take_tactical(active) {
            choice.options.push(ChoiceOption::labelled(
                TACTICAL_ACTION_ID,
                ACTION_KIND,
                "take a tactical action",
            ));
        }
        if let Some(galaxy) = self.galaxy.as_ref() {
            choice
                .options
                .extend(crate::transactions::available_actions(
                    &self.state,
                    galaxy,
                    active,
                ));
        }
        choice.options.extend(crate::relics::available_actions(
            &self.state,
            self.content,
            self.sources,
            active,
        ));
        // 22.1: cards whose printed window is "Action" are played on your turn. Without this
        // every such card is drawn, held, discarded to the hand limit, and never played.
        choice
            .options
            .extend(crate::action_cards::available_actions(
                &self.state,
                self.content,
                active,
            ));
        Some(choice)
    }

    /// 89.1 needs a map and a tactic token to spend.
    ///
    /// Without a galaxy there is no board to activate anything on, so a game built without one
    /// is never offered the action at all rather than being offered one that cannot resolve.
    fn can_take_tactical(&self, player: &PlayerId) -> bool {
        let Some(galaxy) = self.galaxy.as_ref() else {
            return false;
        };
        activation_options(&self.state, galaxy, player).is_some()
    }

    fn apply_choice(&mut self, choice: &Choice, answer: ChoiceOption) -> Result<(), GameError> {
        match self.state.phase {
            Phase::Strategy => {
                let player = self.take_timed_strategy_card(&choice.player, answer)?;
                self.emit("STRATEGY_CARD_CHOSEN");
                debug_assert_eq!(player, choice.player);
                Ok(())
            }
            Phase::Action => {
                let active = self
                    .state
                    .active
                    .clone()
                    .ok_or(GameError::MissingActivePlayer)?;
                if answer.id == "pass" {
                    self.state
                        .player_mut(&active)
                        .expect("active player exists")
                        .passed = true;
                    self.emit("PLAYER_PASSED");
                    self.advance_turn();
                    return Ok(());
                }
                // 22.1: a component action costs the whole turn, so unlike a transaction this
                // advances it whether or not the relic did anything worth having.
                if let Some(index) = answer.id.strip_prefix("action_card|") {
                    let _ = index;
                    let played = self.play_component_action(&active, &answer);
                    self.emit(if played.unwrap_or(false) {
                        "COMPONENT_ACTION_RESOLVED"
                    } else {
                        "COMPONENT_ACTION_FAILED"
                    });
                    self.advance_turn();
                    return Ok(());
                }
                if answer.kind == crate::relics::ACTION_KIND {
                    let mut dice = std::mem::take(&mut self.dice);
                    let mut rng = self.rng.clone();
                    let done = crate::relics::perform(
                        &mut self.state,
                        self.content,
                        self.sources,
                        &mut dice,
                        &mut rng,
                        &active,
                        &answer,
                    );
                    self.dice = dice;
                    self.rng = rng;
                    self.emit(if done {
                        "COMPONENT_ACTION_RESOLVED"
                    } else {
                        "COMPONENT_ACTION_FAILED"
                    });
                    self.advance_turn();
                    return Ok(());
                }
                if let Some(partner) = crate::transactions::opens_with(&answer) {
                    self.trade = Some(crate::transactions::TradeWindow::open(
                        &mut self.state,
                        &active,
                        &partner,
                    ));
                    self.emit("TRANSACTION_OPENED");
                    return Ok(());
                }
                if answer.kind != ACTION_KIND {
                    return Err(GameError::UnsupportedAction(answer.id));
                }
                if answer.id == TACTICAL_ACTION_ID {
                    self.tactical = Some(TacticalWindow {
                        player: active,
                        stage: TacticalStage::Activating,
                    });
                    self.emit("TACTICAL_ACTION_BEGAN");
                    return Ok(());
                }
                self.secondary = Some(begin_strategic_action(
                    &mut self.state,
                    self.content,
                    &active,
                    answer,
                )?);
                self.emit("STRATEGIC_ACTION_BEGAN");
                Ok(())
            }
            Phase::Status | Phase::Agenda => {
                Err(GameError::UnsupportedAction(choice.prompt.clone()))
            }
        }
    }

    /// The decision an open tactical action currently owes.
    fn tactical_choice(&self, window: &TacticalWindow) -> Option<Choice> {
        let galaxy = self.galaxy.as_ref()?;
        match &window.stage {
            TacticalStage::Activating => activation_options(&self.state, galaxy, &window.player),
            TacticalStage::Moving => Some(movement_options(
                &window.player,
                &movable(
                    &self.state,
                    self.content,
                    self.sources,
                    galaxy,
                    &window.player,
                ),
            )),
            TacticalStage::Loading { window, .. } => window.pending_choice(),
        }
    }

    /// Resolve one decision of the open tactical action.
    fn step_tactical(&mut self) -> StepResult {
        let Some(choice) = self.legal_options() else {
            // Nothing left to ask: the action is over.
            return self.finish_tactical();
        };
        let answer = match self.table.ask(&choice) {
            Ok(answer) => answer,
            Err(error) => return self.result(false, Some(error.into())),
        };
        let Some(window) = self.tactical.take() else {
            unreachable!("a tactical action is open");
        };
        match self.apply_tactical(window, &choice, answer) {
            Ok(result) => result,
            Err(error) => self.result(false, Some(error)),
        }
    }

    /// Copy anything the resolver emitted since `from` into the game's event log.
    ///
    /// There are two event streams: the driver's string labels and the resolver's typed events.
    /// A reaction played inside a timing window only ever appears in the second, so a report
    /// reading the first saw a batch in which no action card was ever played — while five
    /// hundred of them were. One observable stream, or observability is a lie.
    fn mirror_timing_log(&mut self, from: usize) {
        let emitted: Vec<String> = self
            .timing
            .log()
            .iter()
            .skip(from)
            .filter_map(|line| line.strip_prefix("emit "))
            .map(|line| line.split('#').next().unwrap_or(line).to_owned())
            .collect();
        self.events.extend(emitted);
    }

    /// Emit a typed event through the resolver, opening its WHEN and AFTER windows.
    ///
    /// The reaction slots registered by [`crate::reactions::arm`] hang off exactly this: a
    /// subsystem that announces itself only as a string label is a subsystem no card can react
    /// to, however complete the timing machinery behind it.
    ///
    /// # Errors
    /// [`GameError`] when the event id space is exhausted or a decider answers illegally.
    fn emit_typed(
        &mut self,
        event_type: &str,
        payload: BTreeMap<String, serde_json::Value>,
    ) -> Result<bool, GameError> {
        let event = self.event_sequence.next(event_type, payload)?;
        let (content, sources) = (self.content, self.sources);
        let galaxy = self.galaxy.clone();
        let logged = self.timing.log().len();
        let emitted = {
            let (state, table, timing, dice, rng, event_sequence) = (
                &mut self.state,
                &mut self.table,
                &mut self.timing,
                &mut self.dice,
                &mut self.rng,
                &mut self.event_sequence,
            );
            let mut context = TimingContext {
                state,
                content,
                sources,
                table,
                dice,
                rng,
                event_sequence,
                galaxy: galaxy.as_ref(),
            };
            timing.emit_with_context(&mut context, event, |_, _| {})?
        };
        self.mirror_timing_log(logged);
        Ok(!emitted.cancelled)
    }

    /// Play an action card as a component action, through the game's own timing context.
    ///
    /// 22.1: it costs the whole turn, which is why the caller advances the turn whatever the
    /// card managed to do.
    fn play_component_action(
        &mut self,
        player: &PlayerId,
        answer: &ChoiceOption,
    ) -> Result<bool, GameError> {
        let (content, sources) = (self.content, self.sources);
        let galaxy = self.galaxy.clone();
        let logged = self.timing.log().len();
        let played = {
            let (state, table, timing, dice, rng, event_sequence) = (
                &mut self.state,
                &mut self.table,
                &mut self.timing,
                &mut self.dice,
                &mut self.rng,
                &mut self.event_sequence,
            );
            let mut context = TimingContext {
                state,
                content,
                sources,
                table,
                dice,
                rng,
                event_sequence,
                galaxy: galaxy.as_ref(),
            };
            crate::action_cards::perform(&mut context, timing, player, answer)
                .map_err(GameError::from)
        };
        self.mirror_timing_log(logged);
        played
    }

    fn apply_tactical(
        &mut self,
        mut window: TacticalWindow,
        choice: &Choice,
        answer: ChoiceOption,
    ) -> Result<StepResult, GameError> {
        match window.stage {
            TacticalStage::Activating => {
                let system = SystemId::new(answer.id);
                activate(&mut self.state, &window.player, &system)?;
                self.emit(&format!("SYSTEM_ACTIVATED:{system}"));
                // Typed as well as logged, so the eight cards that read "After you activate a
                // system" have a window to be played into.
                let mut payload = BTreeMap::new();
                payload.insert(
                    "player".to_owned(),
                    serde_json::Value::String(window.player.to_string()),
                );
                payload.insert(
                    "system".to_owned(),
                    serde_json::Value::String(system.to_string()),
                );
                self.emit_typed("SYSTEM_ACTIVATED", payload)?;
                window.stage = TacticalStage::Moving;
                self.tactical = Some(window);
                Ok(self.result(true, None))
            }
            TacticalStage::Moving => match read_move(choice, answer)? {
                MoveSelection::Done => {
                    self.tactical = Some(window);
                    Ok(self.finish_tactical())
                }
                MoveSelection::Ship { origin, index } => {
                    self.begin_one_move(window, &origin, index)
                }
            },
            TacticalStage::Loading {
                origin,
                ship,
                path,
                window: mut hold,
            } => {
                hold.resolve(answer)?;
                if hold.is_complete() {
                    let cargo = hold.cargo();
                    let outcome = self.sail(&origin, &ship, &path, cargo);
                    self.emit(match outcome {
                        MoveOutcome::Arrived { .. } => "SHIP_MOVED",
                        MoveOutcome::LostToGravityRift { .. } => "SHIP_LOST_TO_GRAVITY_RIFT",
                    });
                    window.stage = TacticalStage::Moving;
                } else {
                    window.stage = TacticalStage::Loading {
                        origin,
                        ship,
                        path,
                        window: hold,
                    };
                }
                self.tactical = Some(window);
                Ok(self.result(true, None))
            }
        }
    }

    /// Select one ship and open its hold.
    ///
    /// The route is computed once, here, and carried through loading. Cargo cannot change which
    /// systems the ship passes, and recomputing the route after the hold was filled would risk
    /// rolling rifts for a different path than the one that was legal when the move was offered.
    fn begin_one_move(
        &mut self,
        mut window: TacticalWindow,
        origin: &SystemId,
        index: usize,
    ) -> Result<StepResult, GameError> {
        let galaxy = self.galaxy.clone().ok_or(TacticalError::NoActiveSystem)?;
        let active = self
            .state
            .active_system
            .clone()
            .ok_or(TacticalError::NoActiveSystem)?;
        let ship = self
            .state
            .ships_of(&window.player, origin)
            .get(index)
            .map(|unit| (*unit).clone())
            .ok_or_else(|| TacticalError::UnknownSystem(origin.clone()))?;

        let mut rules = MovementRules::new(
            &galaxy,
            self.content,
            self.sources,
            active.as_str(),
            Board::for_player(&self.state, self.content, self.sources, &window.player),
        );
        crate::action_cards::apply_movement_effects(&mut rules, &self.state, &window.player);
        let path = ti4_content::units::catalogue(self.content, self.sources)
            .get(ship.type_id.as_str())
            .and_then(|kind| {
                rules.path_from(
                    origin.as_str(),
                    i32::try_from(kind.move_value()).unwrap_or(0)
                        + crate::action_cards::move_bonus(
                            &self.state,
                            &window.player,
                            self.state.activation_seq,
                        ),
                )
            })
            .ok_or_else(|| TacticalError::UnknownSystem(origin.clone()))?;

        let hold = CargoWindow::for_ship(
            &self.state,
            self.content,
            self.sources,
            &window.player,
            origin,
            &ship,
        );
        if hold.is_complete() {
            // No capacity, or nothing to carry: sail immediately.
            let outcome = self.sail(origin, &ship, &path, Vec::new());
            self.emit(match outcome {
                MoveOutcome::Arrived { .. } => "SHIP_MOVED",
                MoveOutcome::LostToGravityRift { .. } => "SHIP_LOST_TO_GRAVITY_RIFT",
            });
            window.stage = TacticalStage::Moving;
        } else {
            window.stage = TacticalStage::Loading {
                origin: origin.clone(),
                ship: Box::new(ship),
                path,
                window: Box::new(hold),
            };
        }
        self.tactical = Some(window);
        Ok(self.result(true, None))
    }

    /// Roll the route's gravity rifts, then move the ship or lose it.
    fn sail(
        &mut self,
        origin: &SystemId,
        ship: &Unit,
        path: &[String],
        cargo: Vec<crate::transit::Cargo>,
    ) -> MoveOutcome {
        let galaxy = self.galaxy.clone().expect("a tactical action needs a map");
        let active = self
            .state
            .active_system
            .clone()
            .expect("a move needs an active system");
        let mut rules = MovementRules::new(
            &galaxy,
            self.content,
            self.sources,
            active.as_str(),
            Board::default(),
        );
        // Through the same door as the pathing rules. `survives_gravity_rifts` honours
        // `anomalies_ignored` as well as `rifts_ignored`, so setting only the Circlet's immunity
        // here would let Nav Suite route around an anomaly and then roll for the rift anyway.
        crate::action_cards::apply_movement_effects(&mut rules, &self.state, &ship.owner);
        let survives = survives_gravity_rifts(&mut self.dice, &mut self.rng, &rules, path);
        apply_move(&mut self.state, origin, &active, ship, cargo, survives)
    }

    /// End the tactical action.
    ///
    /// Space cannon, space combat, invasion and production are unimplemented. The oracle runs
    /// all four here. Announcing the gap and moving on keeps a driven game playable while making
    /// it plain that moving into an enemy system currently has no consequence — the same choice
    /// made for unimplemented agenda effects.
    fn finish_tactical(&mut self) -> StepResult {
        let player = self.tactical.as_ref().map(|window| window.player.clone());
        self.tactical = None;
        let system = self.state.active_system.clone();

        let (Some(player), Some(system)) = (player, system) else {
            return self.close_tactical();
        };

        let mut dice = std::mem::take(&mut self.dice);
        let mut rng = self.rng.clone();
        let galaxy = self.galaxy.clone();
        let logged = self.timing.log().len();
        // The same timing handle the stepping path gets. Most of an aftermath happens here, in
        // the settle that runs it forward until something needs asking — so leaving this one
        // without a resolver meant combat rounds, invasions and production all passed unannounced
        // while the stepping path looked correctly wired.
        let mut ctx = Resolving {
            content: self.content,
            sources: self.sources,
            dice: &mut dice,
            rng: &mut rng,
            table: &mut self.table,
            timing: Some(crate::choice::TimingHandle {
                resolver: &mut self.timing,
                sequence: &mut self.event_sequence,
                galaxy: galaxy.as_ref(),
            }),
        };
        let opened =
            AftermathWindow::new(&mut self.state, &mut ctx, &player, &system, galaxy.as_ref());
        let mut window = match opened {
            Ok(mut window) => {
                window.settle(&mut self.state, &mut ctx);
                window
            }
            Err(error) => {
                self.dice = dice;
                self.rng = rng;
                self.mirror_timing_log(logged);
                return self.result(false, Some(error));
            }
        };
        window.settle(&mut self.state, &mut ctx);
        self.dice = dice;
        self.rng = rng;
        self.mirror_timing_log(logged);
        self.events.append(&mut window.log);

        if window
            .pending_choice(&self.state, self.content, self.sources)
            .is_none()
        {
            return self.close_tactical();
        }
        self.aftermath = Some(window);
        self.result(false, None)
    }

    /// Resolve one decision of the post-movement sequence.
    fn step_aftermath(&mut self) -> StepResult {
        let Some(choice) = self.legal_options() else {
            self.aftermath = None;
            return self.close_tactical();
        };
        let answer = match self.table.ask(&choice) {
            Ok(answer) => answer,
            Err(error) => return self.result(false, Some(error.into())),
        };
        let Some(mut window) = self.aftermath.take() else {
            unreachable!("an aftermath is open");
        };
        let mut dice = std::mem::take(&mut self.dice);
        let mut rng = self.rng.clone();
        let galaxy = self.galaxy.clone();
        let logged = self.timing.log().len();
        // With the timing machinery, so combat, invasion and production can emit at the moment
        // the thing happens rather than after it. A reaction to "at the start of a combat round"
        // that fires once the round has resolved applies its bonus to the wrong round.
        let outcome = {
            let mut ctx = Resolving {
                content: self.content,
                sources: self.sources,
                dice: &mut dice,
                rng: &mut rng,
                table: &mut self.table,
                timing: Some(crate::choice::TimingHandle {
                    resolver: &mut self.timing,
                    sequence: &mut self.event_sequence,
                    galaxy: galaxy.as_ref(),
                }),
            };
            window.resolve(&mut self.state, &mut ctx, answer)
        };
        self.dice = dice;
        self.rng = rng;
        self.mirror_timing_log(logged);
        self.events.append(&mut window.log);

        if let Err(error) = outcome {
            self.aftermath = Some(window);
            return self.result(false, Some(error.into()));
        }
        if window
            .pending_choice(&self.state, self.content, self.sources)
            .is_none()
        {
            return self.close_tactical();
        }
        self.aftermath = Some(window);
        self.result(true, None)
    }

    /// Close the action and pass the turn.
    fn close_tactical(&mut self) -> StepResult {
        self.aftermath = None;
        self.state.active_system = None;
        self.state.pending = None;
        self.emit("TACTICAL_ACTION_COMPLETE");
        self.advance_turn();
        self.result(false, None)
    }

    /// Resolve one decision of an open transaction.
    ///
    /// Unlike every other window here, finishing does **not** advance the turn: 94.1a puts a
    /// transaction "at any time during your turn", and the turn continues afterwards.
    fn step_trade(&mut self) -> StepResult {
        let Some(galaxy) = self.galaxy.clone() else {
            self.trade = None;
            return self.result(false, None);
        };
        let choice = self
            .trade
            .as_ref()
            .expect("checked above")
            .pending_choice(&self.state);
        let Some(choice) = choice else {
            self.trade = None;
            return self.result(false, None);
        };
        let answer = match self.table.ask(&choice) {
            Ok(answer) => answer,
            Err(error) => return self.result(false, Some(error.into())),
        };
        let outcome = self.trade.as_mut().expect("window remains open").resolve(
            &mut self.state,
            &galaxy,
            &answer,
        );
        self.emit(match outcome {
            crate::transactions::Traded::Resolved => "TRANSACTION",
            crate::transactions::Traded::Refused => "TRANSACTION_REFUSED",
            crate::transactions::Traded::Offered => "TRANSACTION_OFFERED",
            crate::transactions::Traded::Countered => "COUNTEROFFER",
            crate::transactions::Traded::Rejected(_) => "TRANSACTION_REJECTED",
            crate::transactions::Traded::NothingOffered => "TRANSACTION_ABANDONED",
        });
        if self
            .trade
            .as_ref()
            .is_some_and(crate::transactions::TradeWindow::is_complete)
        {
            self.trade = None;
        }
        self.result(true, None)
    }

    fn step_secondary(&mut self) -> StepResult {
        let choice = self
            .secondary
            .as_mut()
            .expect("checked above")
            .next_choice(&mut self.state);
        let Some(choice) = choice else {
            self.secondary = None;
            self.emit("STRATEGIC_ACTION_COMPLETE");
            self.advance_turn();
            return self.result(false, None);
        };
        let answer = match self.table.ask(&choice) {
            Ok(answer) => answer,
            Err(error) => return self.result(false, Some(error.into())),
        };
        let (resolution, complete) = match self
            .secondary
            .as_mut()
            .expect("window remains open")
            .take_choice(&mut self.state, answer)
        {
            Ok(resolution) => (
                resolution,
                self.secondary
                    .as_ref()
                    .expect("window remains open")
                    .is_complete(),
            ),
            Err(error) => return self.result(false, Some(error.into())),
        };
        self.emit(match resolution {
            SecondaryResolution::Declined => "STRATEGY_SECONDARY_DECLINED",
            SecondaryResolution::Followed => "STRATEGY_SECONDARY_FOLLOWED",
            SecondaryResolution::Ineligible => unreachable!("ineligible followers are automatic"),
        });
        if complete {
            self.secondary = None;
            self.emit("STRATEGIC_ACTION_COMPLETE");
            self.advance_turn();
        }
        self.result(true, None)
    }

    /// Open the 81.1 scoring window, which precedes every other status step.
    ///
    /// Scoring can end the game (98.7), which is why LRR 81 puts it first and why the rest of
    /// the phase is not touched until the window closes.
    fn step_status(&mut self) -> StepResult {
        self.status_resolved = true;
        // With the map, so objectives that ask about the board's shape can be scored at all.
        let mut window = ScoringWindow::new(&self.state.initiative_order());
        if let Some(galaxy) = self.galaxy.clone() {
            window = window.with_galaxy(galaxy);
        }
        self.scoring = Some(window);
        self.emit("STATUS_SCORING_BEGAN");
        self.result(false, None)
    }

    /// Resolve one player's 81.1 decision, then run 81.2 to 81.4 when the window closes.
    fn step_scoring(&mut self) -> StepResult {
        let Some(choice) = self.legal_options() else {
            return self.begin_status_bookkeeping();
        };
        let answer = match self.table.ask(&choice) {
            Ok(answer) => answer,
            Err(error) => return self.result(false, Some(error.into())),
        };
        let Some(mut window) = self.scoring.take() else {
            unreachable!("the scoring window is open");
        };
        let outcome = window.resolve(&mut self.state, self.content, self.sources, answer);
        self.scoring = Some(window);

        match outcome {
            Ok(scored) => {
                if let Some(alias) = scored {
                    self.emit(&format!("OBJECTIVE_SCORED:{alias}"));
                }
                if self.state.finished {
                    self.scoring = None;
                    self.emit("GAME_FINISHED");
                    return self.result(true, None);
                }
                self.result(true, None)
            }
            Err(error) => self.result(false, Some(error.into())),
        }
    }

    /// Steps 81.2 to 81.4, then open the 81.5 token gain.
    fn begin_status_bookkeeping(&mut self) -> StepResult {
        self.scoring = None;
        match resolve_before_token_gain(&mut self.state) {
            Ok(report) if report.game_ended => {
                self.emit("GAME_FINISHED");
                self.result(false, None)
            }
            Ok(report) => {
                self.emit("STATUS_BOOKKEEPING_RESOLVED");
                self.tokens = Some((
                    TokenGain::for_status(&report.initiative_order),
                    Box::new(report),
                ));
                self.result(false, None)
            }
            Err(error) => self.result(false, Some(error.into())),
        }
    }

    /// Resolve one token of the 81.5 gain, finishing the status phase when the window closes.
    fn step_token_gain(&mut self) -> StepResult {
        let Some(choice) = self.legal_options() else {
            return self.finish_status_phase();
        };
        let answer = match self.table.ask(&choice) {
            Ok(answer) => answer,
            Err(error) => return self.result(false, Some(error.into())),
        };
        // Taken out and put back so the window and `state` are not borrowed from `self` at
        // once. On the error path it goes back unchanged, leaving the token still owed.
        let Some((mut window, report)) = self.tokens.take() else {
            unreachable!("the token window is open");
        };
        let outcome = window.resolve(&mut self.state, answer);
        let complete = window.is_complete();
        self.tokens = Some((window, report));

        match outcome {
            Ok(pool) => {
                self.emit(&format!("COMMAND_TOKEN_GAINED:{pool:?}"));
                if complete {
                    return self.finish_status_phase();
                }
                self.result(true, None)
            }
            Err(error) => self.result(false, Some(error.into())),
        }
    }

    /// Steps 81.6 to 81.8, completing the status phase.
    fn finish_status_phase(&mut self) -> StepResult {
        let Some((_, mut report)) = self.tokens.take() else {
            unreachable!("the token window was open");
        };
        resolve_after_token_gain(&mut self.state, &mut report);
        self.emit("COMMAND_TOKENS_GAINED");
        self.emit("STATUS_PHASE_RESOLVED");
        self.result(false, None)
    }

    /// Reveal this phase's agendas and put the first one to a vote.
    fn step_agenda(&mut self) -> StepResult {
        self.agenda_resolved = true;
        match resolve_agenda_phase(&mut self.state) {
            Ok(report) if report.agendas.is_empty() => {
                self.emit("AGENDA_PHASE_RESOLVED");
                self.result(false, None)
            }
            Ok(report) => {
                let queue: Vec<String> = report.agendas.iter().map(|a| a.alias.clone()).collect();
                self.open_next_vote(queue)
            }
            Err(error) => self.result(false, Some(error.into())),
        }
    }

    /// Put the next revealed agenda to a vote, or finish the phase when none remain.
    ///
    /// An agenda whose election has no legal candidate is discarded rather than voted on —
    /// 8.19 with an empty ballot is not a decision anyone can be asked to make.
    fn open_next_vote(&mut self, mut queue: Vec<String>) -> StepResult {
        while let Some(alias) = queue.first().cloned() {
            queue.remove(0);
            self.emit(&format!("AGENDA_REVEALED:{alias}"));
            let choices = outcomes(&self.state, self.content, self.sources, &alias);
            if choices.is_empty() {
                self.emit(&format!("AGENDA_DISCARDED:{alias}"));
                continue;
            }

            // The outcomes have to be on the state before the window opens, because a card
            // played into it predicts one of them. Nineteen action cards read "when" or "after
            // an agenda is revealed", and this is the event they hang off.
            self.state.agenda_choices.clone_from(&choices);
            let mut payload = BTreeMap::new();
            payload.insert(
                "agenda".to_owned(),
                serde_json::Value::String(alias.clone()),
            );
            if let Err(error) = self.emit_typed("AGENDA_REVEALED", payload) {
                return self.result(false, Some(error));
            }

            let mut window = VoteWindow::new(&self.state, &alias, choices);
            window.open(&self.state, self.content, self.sources);
            self.voting = Some((Box::new(window), queue));
            return self.result(false, None);
        }
        self.voting = None;
        self.emit("AGENDA_PHASE_RESOLVED");
        self.result(false, None)
    }

    /// Resolve one vote decision, applying the outcome when the vote closes.
    fn step_vote(&mut self) -> StepResult {
        let Some(choice) = self.legal_options() else {
            return self.close_vote();
        };
        let answer = match self.table.ask(&choice) {
            Ok(answer) => answer,
            Err(error) => return self.result(false, Some(error.into())),
        };
        let Some((mut window, queue)) = self.voting.take() else {
            unreachable!("a vote is open");
        };
        let outcome = window.resolve(&mut self.state, self.content, self.sources, answer);
        let complete = window.is_complete();
        self.voting = Some((window, queue));

        match outcome {
            Ok(()) => {
                if complete {
                    return self.close_vote();
                }
                self.result(true, None)
            }
            Err(error) => self.result(false, Some(error.into())),
        }
    }

    /// Record a finished vote's outcome, then move to the next agenda.
    ///
    /// No agenda *effect* is applied: this engine has no effect registry. The oracle emits
    /// `AGENDA_EFFECT_UNRESOLVED` in exactly this situation rather than silently doing
    /// nothing, and so does this — proceeding without saying so is how a rule goes missing.
    fn close_vote(&mut self) -> StepResult {
        let Some((window, queue)) = self.voting.take() else {
            unreachable!("a vote was open");
        };
        let alias = window.alias().to_owned();
        self.state.agenda_choices.clear();
        if window.winner().is_none() {
            // No outcome, so nobody predicted correctly — but the cards were still spent, and a
            // prediction left behind would pay out on the next agenda.
            self.state.agenda_predictions.clear();
        }
        if let Some(outcome) = window.winner() {
            let outcome = outcome.to_owned();
            self.emit(&format!("AGENDA_RESOLVED:{alias}:{outcome}"));

            // Imperial Rider pays out before the agenda's own effect, and clears the
            // predictions. A prediction left behind would pay again on the next agenda, for a
            // card that was spent on this one.
            for player in crate::action_cards::resolve_predictions(&mut self.state, &outcome) {
                self.emit(&format!("AGENDA_PREDICTION_CORRECT:{player}"));
            }

            // 8.20 first: an elected or "For" law stays in play, and an effect that reads the
            // laws must see this one already there. 8.21 discards everything else.
            if is_law(self.content, &alias) && outcome != AGAINST {
                self.state.enact_law(&alias, &outcome);
                self.emit(&format!("LAW_ENACTED:{alias}:{outcome}"));
            }

            // With the game's own dice, table and map: several agendas roll, ask, or read
            // the shape of the board, and one borrowed from nowhere would roll off a stream no
            // seed covers. The speaker's tie-break (8.18) is asked through the same table.
            let mut dice = std::mem::take(&mut self.dice);
            let mut rng = self.rng.clone();
            let galaxy = self.galaxy.clone();
            let mut ctx = Resolving {
                content: self.content,
                sources: self.sources,
                dice: &mut dice,
                rng: &mut rng,
                table: &mut self.table,
                timing: None,
            };
            let effect = crate::agenda_effects::resolve_with(
                &mut self.state,
                &mut ctx,
                galaxy.as_ref(),
                &alias,
                &outcome,
                window.ballot(),
            );
            self.dice = dice;
            self.rng = rng;

            match effect {
                crate::agenda_effects::Effect::Resolved { .. } => {
                    self.emit(&format!("AGENDA_EFFECT_RESOLVED:{alias}"));
                }
                crate::agenda_effects::Effect::Unresolved { .. } => {
                    self.emit(&format!("AGENDA_EFFECT_UNRESOLVED:{alias}"));
                }
                crate::agenda_effects::Effect::Deferred { .. } => {
                    self.emit(&format!("AGENDA_EFFECT_DEFERRED:{alias}"));
                }
            }
        }
        self.open_next_vote(queue)
    }

    fn step_phase(&mut self) -> StepResult {
        if self.state.phase == Phase::Action && !self.state.all_passed() {
            return self.result(false, Some(GameError::MissingActivePlayer));
        }
        let outcome = advance_phase(&mut self.state);
        match outcome {
            PhaseOutcome::ActionBegan(_) => self.emit("ACTION_PHASE_BEGAN"),
            PhaseOutcome::StatusBegan => self.emit("STATUS_PHASE_BEGAN"),
            PhaseOutcome::AgendaBegan => self.emit("AGENDA_PHASE_BEGAN"),
            PhaseOutcome::RoundEnded => {
                begin_next_round(&mut self.state, self.strategy_cards.clone());
                self.status_resolved = false;
                self.agenda_resolved = false;
                self.emit("ROUND_BEGAN");
            }
        }
        self.result(false, None)
    }

    fn advance_turn(&mut self) {
        if advance_turn(&mut self.state).is_some() {
            self.emit("TURN_PASSED");
        }
    }

    fn take_timed_strategy_card(
        &mut self,
        player: &PlayerId,
        answer: ChoiceOption,
    ) -> Result<PlayerId, GameError> {
        self.sync_timing_context();
        let card = StrategyCardId::new(answer.id.clone());
        let mut payload = BTreeMap::new();
        payload.insert(
            "player".to_owned(),
            serde_json::Value::String(player.to_string()),
        );
        payload.insert(
            "card".to_owned(),
            serde_json::Value::String(answer.id.clone()),
        );
        payload.insert(
            "goods".to_owned(),
            serde_json::Value::from(
                self.state
                    .strategy_card_goods
                    .get(&card)
                    .copied()
                    .unwrap_or_default(),
            ),
        );
        let event = self.event_sequence.next("STRATEGY_CARD_CHOSEN", payload)?;
        let content = self.content;
        let sources = self.sources;
        let galaxy = self.galaxy.clone();
        let mut selected = None;
        let emitted = {
            let (state, table, timing, dice, rng, event_sequence) = (
                &mut self.state,
                &mut self.table,
                &mut self.timing,
                &mut self.dice,
                &mut self.rng,
                &mut self.event_sequence,
            );
            let mut context = TimingContext {
                state,
                content,
                sources,
                table,
                dice,
                rng,
                event_sequence,
                galaxy: galaxy.as_ref(),
            };
            timing.emit_with_context(&mut context, event, |_, context| {
                selected = Some(take_strategy_card(context.state, content, answer));
            })?
        };
        if emitted.cancelled {
            return Err(GameError::TimingEventCancelled(emitted.event_type));
        }
        selected
            .ok_or(GameError::TimingEventCancelled(emitted.event_type))?
            .map_err(GameError::Draft)
    }

    fn sync_timing_context(&mut self) {
        self.timing.set_phase(self.state.phase);
        self.timing
            .sync_lifecycle(self.state.round, self.state.turn_seq);
        self.timing
            .set_seating_order(self.state.seating_order.clone());
        self.timing.set_active_player(self.state.active.clone());
        self.timing.set_speaker(Some(self.state.speaker.clone()));
    }

    fn emit(&mut self, event: &str) {
        self.events.push(event.to_owned());
    }

    fn result(&self, resolved_choice: bool, error: Option<GameError>) -> StepResult {
        StepResult {
            phase: self.state.phase,
            active: self.state.active.clone(),
            finished: self.state.finished,
            error,
            resolved_choice,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use ti4_content::ContentStore;
    use ti4_model::content_types::POK;
    use ti4_model::id::PlayerId;
    use ti4_model::state::Phase;

    use super::*;
    use crate::choice::{AlwaysDecline, Scripted};
    use crate::setup::start_game;
    use crate::timing::{Ability, Relation};
    use crate::tokens::STATUS_TOKENS;

    #[test]
    fn one_step_resolves_exactly_one_generated_strategy_choice() {
        let players = [PlayerId::new("a"), PlayerId::new("b"), PlayerId::new("c")];
        let state = start_game(ContentStore::embedded(), &players, POK, None).unwrap();
        let mut game = Game::new(state, ContentStore::embedded());

        let result = game.step();

        assert!(result.error.is_none());
        assert!(result.resolved_choice);
        assert_eq!(game.table.log.len(), 1);
        assert_eq!(game.state.unclaimed_strategy_cards.len(), 7);
        assert_eq!(game.events, vec!["STRATEGY_CARD_CHOSEN"]);
    }

    #[test]
    fn strategy_selection_reaches_stateful_timing_with_the_games_table_and_state() {
        let players = [PlayerId::new("a"), PlayerId::new("b")];
        let state = start_game(ContentStore::embedded(), &players, POK, None).unwrap();
        let mut game = Game::with_table(
            state,
            ContentStore::embedded(),
            Table::with_default(Box::new(Scripted::new(["pok1leadership", "decline"]))),
        );
        game.timing_mut().register([Ability::stateful(
            "strategy-point",
            PlayerId::new("a"),
            "STRATEGY_CARD_CHOSEN",
            Relation::When,
            Arc::new(|event, _, context| {
                assert_eq!(event.text("player"), Some("a"));
                assert_eq!(event.text("card"), Some("pok1leadership"));
                assert_eq!(event.integer("goods"), Some(0));
                context
                    .state
                    .player_mut(&PlayerId::new("a"))
                    .expect("timing owner is seated")
                    .victory_points += 1;
                Ok(())
            }),
        )
        .with_optional(true)]);

        let result = game.step();

        assert!(result.error.is_none());
        assert_eq!(
            game.state
                .player(&PlayerId::new("a"))
                .unwrap()
                .victory_points,
            0
        );
        assert_eq!(game.table.log.len(), 2, "one table answered both decisions");
        assert_eq!(game.table.log.records[1].chosen, "decline");
        assert!(
            game.timing
                .log()
                .iter()
                .any(|line| line.contains("declines")),
            "the driver did not run the registered timing ability"
        );
    }

    #[test]
    fn timing_cancellation_keeps_a_strategy_card_on_the_mat() {
        let players = [PlayerId::new("a"), PlayerId::new("b")];
        let state = start_game(ContentStore::embedded(), &players, POK, None).unwrap();
        let mut game = Game::new(state, ContentStore::embedded());
        game.timing_mut().register([Ability::stateful(
            "cancel-strategy-pick",
            PlayerId::new("a"),
            "STRATEGY_CARD_CHOSEN",
            Relation::When,
            Arc::new(|event, _, _| {
                event.cancel();
                Ok(())
            }),
        )]);

        let result = game.step();

        assert!(matches!(
            result.error,
            Some(GameError::TimingEventCancelled(ref event)) if event == "STRATEGY_CARD_CHOSEN"
        ));
        assert_eq!(game.state.unclaimed_strategy_cards.len(), 8);
        assert!(
            game.state
                .player(&PlayerId::new("a"))
                .unwrap()
                .strategy_cards
                .is_empty()
        );
        assert!(game.events.is_empty());
    }

    #[test]
    fn mandatory_stateful_timing_effect_mutates_the_live_game() {
        let players = [PlayerId::new("a"), PlayerId::new("b")];
        let state = start_game(ContentStore::embedded(), &players, POK, None).unwrap();
        let mut game = Game::new(state, ContentStore::embedded());
        game.timing_mut().register([Ability::stateful(
            "strategy-point",
            PlayerId::new("a"),
            "STRATEGY_CARD_CHOSEN",
            Relation::When,
            Arc::new(|_, _, context| {
                context
                    .state
                    .player_mut(&PlayerId::new("a"))
                    .expect("timing owner is seated")
                    .victory_points += 1;
                Ok(())
            }),
        )]);

        let result = game.step();

        assert!(result.error.is_none());
        assert_eq!(
            game.state
                .player(&PlayerId::new("a"))
                .unwrap()
                .victory_points,
            1
        );
        assert_eq!(game.state.unclaimed_strategy_cards.len(), 7);
    }

    #[test]
    fn seeded_random_game_records_only_offered_choices() {
        let players = [PlayerId::new("a"), PlayerId::new("b"), PlayerId::new("c")];
        let state = start_game(ContentStore::embedded(), &players, POK, None).unwrap();
        let mut game = Game::with_seeded_random(state, ContentStore::embedded(), 42);

        for _ in 0..6 {
            assert!(game.step().error.is_none());
        }

        assert_eq!(game.table.log.len(), 6);
        assert!(
            game.table
                .log
                .records
                .iter()
                .all(|record| record.offered.contains(&record.chosen))
        );
    }

    #[test]
    fn seeded_random_game_repeats_its_event_and_decision_trace() {
        let trace = |seed| {
            let players = [PlayerId::new("a"), PlayerId::new("b"), PlayerId::new("c")];
            let state = start_game(ContentStore::embedded(), &players, POK, None).unwrap();
            let mut game = Game::with_seeded_random(state, ContentStore::embedded(), seed);
            for _ in 0..12 {
                assert!(game.step().error.is_none());
            }
            (game.events, game.table.log)
        };

        assert_eq!(trace(2024), trace(2024));
    }

    #[test]
    fn different_seeded_games_choose_different_legal_traces() {
        let trace = |seed| {
            let players = [PlayerId::new("a"), PlayerId::new("b"), PlayerId::new("c")];
            let state = start_game(ContentStore::embedded(), &players, POK, None).unwrap();
            let mut game = Game::with_seeded_random(state, ContentStore::embedded(), seed);
            for _ in 0..6 {
                assert!(game.step().error.is_none());
            }
            game.table.log
        };

        assert_ne!(trace(1), trace(2));
    }

    #[test]
    fn a_seeded_random_game_completes_a_whole_round() {
        let players = [PlayerId::new("a"), PlayerId::new("b"), PlayerId::new("c")];
        let state = start_game(ContentStore::embedded(), &players, POK, None).unwrap();
        let mut game = Game::with_seeded_random(state, ContentStore::embedded(), 7);

        assert!(game.run(1, 200).is_ok(), "no step should refuse");
        assert_eq!(game.state.round, 2, "the round advanced");
        for event in [
            "ACTION_PHASE_BEGAN",
            "STATUS_PHASE_BEGAN",
            "STATUS_SCORING_BEGAN",
            "STATUS_PHASE_RESOLVED",
        ] {
            assert!(game.events.contains(&event.to_owned()), "missing {event}");
        }
    }

    #[test]
    fn one_hundred_seeded_generic_games_complete_a_round_with_only_offered_choices() {
        for seed in 0..100_u64 {
            let player_count = 2 + usize::try_from(seed % 5).unwrap();
            let players = (0..player_count)
                .map(|index| PlayerId::new(format!("p{index}")))
                .collect::<Vec<_>>();
            let state = start_game(ContentStore::embedded(), &players, POK, None).unwrap();
            let mut game = Game::with_seeded_random(state, ContentStore::embedded(), seed);

            assert!(
                game.run(1, 500).is_ok(),
                "seed {seed}, players {player_count}"
            );
            assert_eq!(game.state.round, 2, "seed {seed}");
            assert!(game.events.contains(&"ACTION_PHASE_BEGAN".to_owned()));
            assert!(game.events.contains(&"STATUS_PHASE_BEGAN".to_owned()));
            assert!(
                game.table
                    .log
                    .records
                    .iter()
                    .all(|record| record.offered.contains(&record.chosen))
            );
        }
    }

    #[test]
    fn seeded_game_has_matching_state_event_and_decision_snapshots_at_every_step() {
        let snapshots = |seed| {
            let players = [PlayerId::new("a"), PlayerId::new("b"), PlayerId::new("c")];
            let state = start_game(ContentStore::embedded(), &players, POK, None).unwrap();
            let mut game = Game::with_seeded_random(state, ContentStore::embedded(), seed);
            let mut snapshots = Vec::new();
            loop {
                let result = game.step();
                snapshots.push((
                    serde_json::to_string(&game.state).unwrap(),
                    game.events.clone(),
                    game.table.log.clone(),
                    result.clone(),
                ));
                if result.error.is_some() || result.finished {
                    break;
                }
            }
            snapshots
        };

        assert_eq!(snapshots(42), snapshots(42));
    }

    #[test]
    fn strategy_primary_and_each_secondary_are_separate_steps() {
        let players = [PlayerId::new("a"), PlayerId::new("b"), PlayerId::new("c")];
        let state = start_game(ContentStore::embedded(), &players, POK, None).unwrap();
        let mut game = Game::with_table(
            state,
            ContentStore::embedded(),
            Table::with_default(Box::new(AlwaysDecline)),
        );
        while game.state.phase == Phase::Strategy {
            assert!(game.step().error.is_none());
        }
        let primary = game.state.active.clone().unwrap();

        let primary_step = game.step();
        assert!(primary_step.resolved_choice);
        assert_eq!(game.state.active, Some(primary.clone()));
        assert_eq!(game.table.log.len(), 7, "six draft picks and one primary");
        let before_inspection = game.state.clone();
        assert_eq!(game.legal_options().unwrap().player, PlayerId::new("b"));
        assert!(game.state.identical(&before_inspection));

        let first_secondary = game.step();
        assert!(first_secondary.resolved_choice);
        assert_eq!(game.state.active, Some(primary.clone()));

        let final_secondary = game.step();
        assert!(final_secondary.resolved_choice);
        assert_ne!(game.state.active, Some(primary));
        assert_eq!(game.table.log.len(), 9);
        assert!(game.events.contains(&"STRATEGIC_ACTION_BEGAN".to_owned()));
        assert_eq!(
            game.events
                .iter()
                .filter(|event| event.as_str() == "STRATEGY_SECONDARY_DECLINED")
                .count(),
            2
        );
    }

    #[test]
    fn the_status_phase_scores_then_gains_tokens_then_completes() {
        let players = [PlayerId::new("a")];
        let mut state = start_game(ContentStore::embedded(), &players, POK, None).unwrap();
        state.phase = Phase::Status;
        let before = state.player(&PlayerId::new("a")).unwrap().clone();
        let mut game = Game::new(state, ContentStore::embedded());

        // 81.1 opens first, because scoring can end the game.
        assert_eq!(game.step().error, None);
        assert!(game.events.contains(&"STATUS_SCORING_BEGAN".to_owned()));

        // Nothing is scoreable in a fresh game, so the window closes without a question and
        // the phase runs to the end without ever refusing a step.
        let mut guard = 0;
        while game.state.phase == Phase::Status && guard < 20 {
            assert_eq!(game.step().error, None, "no status step should refuse");
            guard += 1;
        }

        let after = game.state.player(&PlayerId::new("a")).unwrap();
        assert_eq!(
            after.total_tokens(),
            before.total_tokens() + i32::try_from(STATUS_TOKENS).unwrap(),
            "81.5 still placed both tokens"
        );
        assert!(game.events.contains(&"STATUS_PHASE_RESOLVED".to_owned()));
    }

    #[test]
    fn a_token_gained_in_the_status_phase_goes_where_it_was_chosen() {
        let players = [PlayerId::new("a")];
        let mut state = start_game(ContentStore::embedded(), &players, POK, None).unwrap();
        state.phase = Phase::Status;
        let before = state.player(&PlayerId::new("a")).unwrap().clone();
        let table = Table::new(); // FirstOption always takes the tactic pool.
        let mut game = Game::with_table(state, ContentStore::embedded(), table);

        // Bounded: the status phase now completes, so an unbounded loop here would run
        // rounds forever rather than stopping at a boundary.
        let mut guard = 0;
        while game.state.phase == Phase::Status && guard < 20 {
            assert_eq!(game.step().error, None);
            guard += 1;
        }

        let after = game.state.player(&PlayerId::new("a")).unwrap();
        assert_eq!(
            after.tactic_tokens,
            before.tactic_tokens + i32::try_from(STATUS_TOKENS).unwrap()
        );
        assert_eq!(after.fleet_tokens, before.fleet_tokens);
        assert_eq!(after.strategic_tokens, before.strategic_tokens);
    }

    /// A one-ring map plus a fleet, ready to take a tactical action.
    fn tactical_fixture() -> (GameState, ti4_content::galaxy::Galaxy, Vec<SystemId>) {
        let players = [PlayerId::new("a"), PlayerId::new("b")];
        let mut state = start_game(ContentStore::embedded(), &players, POK, None).unwrap();
        let ids: Vec<String> = ti4_content::galaxy::all_systems(ContentStore::embedded(), POK)
            .iter()
            .filter(|(_, system)| !system.is_anomaly() && !system.is_hyperlane())
            .map(|(id, _)| (*id).to_owned())
            .take(7)
            .collect();
        let refs: Vec<&str> = ids.iter().map(String::as_str).collect();
        let galaxy =
            ti4_content::galaxy::Galaxy::build(ContentStore::embedded(), &refs, POK, 1).unwrap();
        let ids: Vec<SystemId> = ids.into_iter().map(SystemId::new).collect();

        state.phase = Phase::Action;
        state.active = Some(PlayerId::new("a"));
        (state, galaxy, ids)
    }

    #[test]
    fn a_tactical_action_is_not_offered_without_a_map() {
        // Without a galaxy there is no board to activate anything on, so the action is never
        // offered rather than being offered and then failing to resolve.
        let (state, _, _) = tactical_fixture();
        let game = Game::new(state, ContentStore::embedded());

        let choice = game.legal_options().unwrap();
        assert!(
            !choice.ids().contains(&TACTICAL_ACTION_ID),
            "no map, no tactical action"
        );
    }

    #[test]
    fn a_tactical_action_is_offered_once_the_map_exists() {
        let (state, galaxy, _) = tactical_fixture();
        let game = Game::new(state, ContentStore::embedded()).with_galaxy(galaxy);

        let choice = game.legal_options().unwrap();
        assert!(choice.ids().contains(&TACTICAL_ACTION_ID));
    }

    #[test]
    fn a_player_with_no_tactic_token_is_not_offered_one() {
        let (mut state, galaxy, _) = tactical_fixture();
        state.player_mut(&PlayerId::new("a")).unwrap().tactic_tokens = 0;
        let game = Game::new(state, ContentStore::embedded()).with_galaxy(galaxy);

        let choice = game.legal_options().unwrap();
        assert!(!choice.ids().contains(&TACTICAL_ACTION_ID));
    }

    #[test]
    fn a_driven_tactical_action_activates_moves_and_completes() {
        // The end-to-end join: activation, then a real move through the movement rules, then
        // the action finishing. Scripted so the route is the one under test rather than
        // whatever a sampler happened to pick.
        let (mut state, galaxy, ids) = tactical_fixture();
        let ship = Unit::new(
            ti4_model::id::UnitTypeId::new("destroyer"),
            PlayerId::new("a"),
        );
        state.system_mut(&ids[1]).units.push(ship);
        let tokens_before = state.player(&PlayerId::new("a")).unwrap().tactic_tokens;

        let table = Table::with_default(Box::new(Scripted::new([
            TACTICAL_ACTION_ID.to_owned(),
            ids[0].to_string(),
            format!("move|{}|0", ids[1]),
            "done_moving".to_owned(),
        ])));
        let mut game = Game::with_table(state, ContentStore::embedded(), table).with_galaxy(galaxy);

        for _ in 0..8 {
            let result = game.step();
            assert_eq!(result.error, None, "no tactical step should refuse");
            if game.events.iter().any(|e| e == "TACTICAL_ACTION_COMPLETE") {
                break;
            }
        }

        assert!(game.events.iter().any(|e| e == "TACTICAL_ACTION_BEGAN"));
        assert!(
            game.events
                .iter()
                .any(|e| e.starts_with("SYSTEM_ACTIVATED:")),
            "89.1 placed a token"
        );
        assert!(game.events.iter().any(|e| e == "SHIP_MOVED"));
        assert!(
            game.events.iter().any(|e| e == "TACTICAL_ACTION_COMPLETE"),
            "the action ran every step it has and closed"
        );

        assert_eq!(
            game.state
                .player(&PlayerId::new("a"))
                .unwrap()
                .tactic_tokens,
            tokens_before - 1,
            "the activation was paid for"
        );
        assert!(
            game.state.system_state(&ids[1]).units.is_empty(),
            "the ship left"
        );
        assert_eq!(
            game.state.system_state(&ids[0]).units.len(),
            1,
            "and arrived in the active system"
        );
        assert!(
            game.state.active_system.is_none(),
            "the action closed the active system"
        );
    }

    #[test]
    fn a_tactical_action_now_fights_for_the_system() {
        // The payoff of wiring: moving into an enemy has a consequence. Before this the action
        // emitted TACTICAL_STEPS_UNRESOLVED and two fleets shared a system indefinitely.
        let (mut state, galaxy, ids) = tactical_fixture();
        crate::fixtures::put(&mut state, &ids[1], "destroyer", &PlayerId::new("a"), 4);
        crate::fixtures::put(&mut state, &ids[0], "fighter", &PlayerId::new("b"), 2);

        let table = Table::with_default(Box::new(Scripted::new([
            TACTICAL_ACTION_ID.to_owned(),
            ids[0].to_string(),
            format!("move|{}|0", ids[1]),
            "done_moving".to_owned(),
        ])));
        let mut game = Game::with_table(state, ContentStore::embedded(), table).with_galaxy(galaxy);

        // Every decision arrives on its own step, with the game a whole value in between.
        // Before the Window conversions the entire post-movement sequence - combat, invasion,
        // production - resolved inside a single step().
        let mut steps_with_a_choice = 0;
        for _ in 0..80 {
            if game.legal_options().is_some() {
                steps_with_a_choice += 1;
                let snapshot = game.state.clone();
                assert!(snapshot.identical(&game.state));
            }
            assert_eq!(game.step().error, None, "no tactical step should refuse");
            if game
                .events
                .iter()
                .any(|event| event == "TACTICAL_ACTION_COMPLETE")
            {
                break;
            }
        }
        assert!(
            steps_with_a_choice >= 3,
            "activation, movement and the fight were each stepped separately"
        );

        assert!(
            game.events.iter().any(|e| e == "SPACE_COMBAT_RESOLVED"),
            "arriving in an enemy system started a fight"
        );
        let survivors =
            crate::combat::combatants(&game.state, ContentStore::embedded(), POK, &ids[0]);
        assert!(
            survivors.len() <= 1,
            "a combat does not end with both fleets standing"
        );
    }

    #[test]
    fn a_carrier_is_asked_what_to_load_before_it_sails() {
        // LRR 95: the hold is filled before the ship moves, so a carrier produces an extra
        // decision that a destroyer does not.
        let (mut state, galaxy, ids) = tactical_fixture();
        let player = PlayerId::new("a");
        state.system_mut(&ids[1]).units.push(Unit::new(
            ti4_model::id::UnitTypeId::new("carrier"),
            player.clone(),
        ));
        state.system_mut(&ids[1]).units.push(Unit::new(
            ti4_model::id::UnitTypeId::new("infantry"),
            player,
        ));

        let table = Table::with_default(Box::new(Scripted::new([
            TACTICAL_ACTION_ID.to_owned(),
            ids[0].to_string(),
            format!("move|{}|0", ids[1]),
        ])));
        let mut game = Game::with_table(state, ContentStore::embedded(), table).with_galaxy(galaxy);

        for _ in 0..3 {
            assert_eq!(game.step().error, None);
        }

        let hold = game.legal_options().expect("the hold is open");
        assert_eq!(hold.prompt, "load which unit");
        assert!(
            hold.options.iter().any(|o| o.id.starts_with("load|")),
            "the infantry is offered as cargo"
        );
    }

    #[test]
    fn an_agenda_is_voted_on_and_a_passed_law_stays_in_play() {
        let players = [PlayerId::new("a"), PlayerId::new("b")];
        let mut state = start_game(ContentStore::embedded(), &players, POK, None).unwrap();
        state.phase = Phase::Agenda;
        state.custodians_removed = true;

        // A For/Against law, so the vote has the ordinary two outcomes and passing it
        // leaves something behind on the table.
        // Deliberately one with no registered effect, because the point of this test is that
        // an agenda the engine cannot resolve still goes through the whole vote and says so.
        let registered = crate::agenda_effects::registered_aliases();
        let law = ContentStore::embedded()
            .records(ti4_model::content_types::ContentType::Agendas)
            .iter()
            .find(|record| {
                record.text("type") == Some("Law")
                    && record.text("target") == Some("For/Against")
                    && record
                        .text("alias")
                        .is_some_and(|alias| !registered.contains(&alias))
            })
            .and_then(|record| record.text("alias"))
            .expect("the corpus has an unregistered For/Against law")
            .to_owned();
        state.agenda_deck = vec![law.clone()];

        // Give both players influence so the vote is decided by votes, not by the speaker.
        let catalogue = ti4_content::galaxy::all_planets(ContentStore::embedded(), POK);
        for (index, (id, record)) in catalogue
            .iter()
            .filter(|(_, p)| p.influence() > 0 && !p.is_placed_during_play())
            .take(2)
            .enumerate()
        {
            state
                .system_mut(&ti4_model::id::SystemId::new(
                    record.system_id().unwrap_or("18"),
                ))
                .set_control(ti4_model::id::PlanetId::new(*id), players[index].clone());
        }

        // FirstOption always takes the first offered option: vote "for", then exhaust the
        // first planet, then decline further planets.
        let mut game = Game::new(state, ContentStore::embedded());
        let mut guard = 0;
        while game.state.phase == Phase::Agenda && guard < 60 {
            assert_eq!(game.step().error, None, "no agenda step should refuse");
            guard += 1;
        }

        assert!(
            game.events
                .iter()
                .any(|e| e.starts_with("AGENDA_RESOLVED:")),
            "the agenda was put to a vote and decided"
        );
        assert!(
            game.events
                .iter()
                .any(|e| e.starts_with("AGENDA_EFFECT_UNRESOLVED:")),
            "an unimplemented effect must be announced, not silently skipped"
        );
        assert_eq!(
            game.state.laws.get(&law).map(String::as_str),
            Some("for"),
            "8.20: a passed law stays in play"
        );
    }

    #[test]
    fn a_resolved_agenda_runs_its_effect() {
        // agenda_effects existed and nothing called it - the sixth module in this project to
        // arrive correct, tested and unwired.
        let players = [PlayerId::new("a"), PlayerId::new("b")];
        let mut state = start_game(ContentStore::embedded(), &players, POK, None).unwrap();
        state.phase = Phase::Agenda;
        state.custodians_removed = true;
        state.agenda_deck = vec!["economic_equality".to_owned()];
        state.player_mut(&PlayerId::new("a")).unwrap().trade_goods = 9;

        let mut game = Game::with_seeded_random(state, ContentStore::embedded(), 6);
        for _ in 0..80 {
            if game.state.phase != Phase::Agenda {
                break;
            }
            assert_eq!(game.step().error, None);
        }

        assert!(
            game.events
                .iter()
                .any(|event| event.starts_with("AGENDA_EFFECT_RESOLVED:")),
            "the effect ran; events {:?}",
            game.events
        );
        assert_ne!(
            game.state.player(&PlayerId::new("a")).unwrap().trade_goods,
            9,
            "and it actually touched the state"
        );
    }

    #[test]
    fn an_agenda_vote_records_only_offered_options() {
        let players = [PlayerId::new("a"), PlayerId::new("b"), PlayerId::new("c")];
        let mut state = start_game(ContentStore::embedded(), &players, POK, None).unwrap();
        state.phase = Phase::Agenda;
        state.custodians_removed = true;
        state.agenda_deck = ContentStore::embedded()
            .records(ti4_model::content_types::ContentType::Agendas)
            .iter()
            .filter_map(|record| record.text("alias"))
            .take(2)
            .map(ToOwned::to_owned)
            .collect();

        let mut game = Game::with_seeded_random(state, ContentStore::embedded(), 11);
        let mut guard = 0;
        while game.state.phase == Phase::Agenda && guard < 200 {
            assert_eq!(game.step().error, None, "no agenda step should refuse");
            guard += 1;
        }

        assert!(game.state.phase != Phase::Agenda, "the phase completed");
        assert!(
            game.table
                .log
                .records
                .iter()
                .all(|record| record.offered.contains(&record.chosen)),
            "every recorded decision was one the engine offered"
        );
    }

    #[test]
    fn an_empty_agenda_needs_no_invented_vote_and_then_starts_a_round() {
        let players = [PlayerId::new("a")];
        let mut state = start_game(ContentStore::embedded(), &players, POK, None).unwrap();
        state.phase = Phase::Agenda;
        state.custodians_removed = true;
        state.agenda_deck.clear();
        let mut game = Game::new(state, ContentStore::embedded());

        assert!(game.step().error.is_none());
        assert_eq!(game.events, vec!["AGENDA_PHASE_RESOLVED"]);
        assert!(game.step().error.is_none());
        assert_eq!(game.state.phase, Phase::Strategy);
        assert_eq!(game.state.round, 2);
        assert_eq!(game.events.last().unwrap(), "ROUND_BEGAN");
    }

    #[test]
    fn run_reports_its_step_horizon_instead_of_looping() {
        let players = [PlayerId::new("a"), PlayerId::new("b"), PlayerId::new("c")];
        let state = start_game(ContentStore::embedded(), &players, POK, None).unwrap();
        let mut game = Game::new(state, ContentStore::embedded());

        assert_eq!(
            game.run(1, 3),
            Err(RunError::StepLimit {
                max_steps: 3,
                round: 1,
                phase: Phase::Strategy,
            })
        );
    }
}
