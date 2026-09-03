//! Game-level choice stepping and bounded execution.

use std::collections::BTreeMap;

use ti4_content::ContentStore;
use ti4_content::galaxy::Galaxy;
use ti4_model::content_types::{POK, SourceSet};
use ti4_model::id::{PlayerId, StrategyCardId, SystemId};
use ti4_model::state::{Feat, FeatOccurrence, GameState, Phase, TransientFlags};
use ti4_model::units::Unit;

use crate::agenda::{AgendaPhaseError, resolve_agenda_phase};
use crate::choice::{
    Choice, ChoiceOption, IllegalChoice, Observed, Resolving, SeededRandom, Table, Window,
};
use crate::dice::Dice;
use crate::draft::{DraftError, strategy_options, take_strategy_card};
use crate::event::{EventSequence, EventSequenceError};
use crate::movement::{Board, MovementRules};
use crate::objectives::{EventScoreLimit, ScoringError, ScoringWindow};
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
    #[error(
        "game did not progress within {max_steps} steps (round {round}, phase {phase:?});          the last {repeats} steps all asked {recent:?}"
    )]
    StepLimit {
        max_steps: usize,
        round: u32,
        phase: Phase,
        /// The prompt the run ended on, and how many consecutive steps asked it.
        ///
        /// A step limit says a game stopped advancing; it never said *what* it was stuck on, and
        /// the answer was previously only reachable by replaying weights that no longer exist. A
        /// decision loop repeats one prompt, so naming it and its run length turns a step-limit
        /// report into a lead.
        recent: String,
        repeats: usize,
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
    /// The system before the fight, for the secrets that ask what the fight destroyed.
    ///
    /// Held here rather than recomputed at the end because every fact in it is gone by then.
    before_combat: crate::combat::BeforeCombat,
    /// Whether the combat's feats have been recorded, so a `settle` that loops does not record
    /// them twice.
    feats_noted: bool,
    /// A scoring pause emitted by a concrete tactical event.
    pending_event_scoring: Option<(FeatOccurrence, EventScoreLimit)>,
    notes_at_tactical_start: crate::combat::NoteHoldings,
}

impl AftermathWindow {
    fn new(
        state: &mut GameState,
        ctx: &mut Resolving<'_>,
        player: &PlayerId,
        system: &SystemId,
        galaxy: Option<&Galaxy>,
        notes_at_tactical_start: crate::combat::NoteHoldings,
    ) -> Result<Self, GameError> {
        // Movement may take the only carrier out of a system and strand what it was holding, so
        // capacity is settled before anything shoots.
        crate::fleet::enforce_seeing(
            state,
            ctx.content,
            ctx.sources,
            galaxy,
            ctx.table,
            player,
            system,
        )
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
            galaxy,
        );
        // Reroll windows (Agnlan Oln, Scramble Frequency): one per firing player, opened
        // between the rolls and the absorption. The hits are then read from the possibly
        // rerolled dice.
        for (gunner, _, _) in &cannon {
            state.last_reroll_player = Some(gunner.clone());
            crate::combat::open_reroll_windows(state, ctx, gunner);
        }
        let cannon: Vec<(PlayerId, usize)> = cannon
            .iter()
            .map(|(who, _, _)| {
                (
                    who.clone(),
                    state
                        .reroll_staging
                        .get(who)
                        .map_or(0, crate::combat::staged_hits),
                )
            })
            .collect();
        state.reroll_staging.clear();
        state.last_reroll_player = None;
        // Turn Their Fleets to Dust names this step and no other, so the count is taken across
        // the absorption rather than after the combat: by then the ordinary rounds have taken
        // ships too, and nothing would say which step emptied the system.
        let before_cannon =
            crate::combat::non_fighter_ships_of(state, ctx.content, ctx.sources, player, system);
        let gunners: Vec<PlayerId> = cannon.iter().map(|(who, _)| who.clone()).collect();
        // "Before you assign hits produced by another player's SPACE CANNON roll." Emitted
        // per firing player and immediately followed by that player's absorption, so a card
        // played in the window (Maneuvering Jets) cancels a hit of *this* roll rather than
        // whatever absorption happens next -- and the window is what stands between the roll
        // and the loss, which space cannon previously resolved straight through.
        let (content, sources, galaxy) = (ctx.content, ctx.sources, galaxy);
        for (gunner, hits) in cannon {
            let mut payload = std::collections::BTreeMap::new();
            payload.insert("system".to_owned(), system.to_string().into());
            payload.insert("player".to_owned(), player.to_string().into());
            payload.insert("gunner".to_owned(), gunner.to_string().into());
            payload.insert("hits".to_owned(), i64::try_from(hits).unwrap_or(0).into());
            let _ = ctx.emit(state, "SPACE_CANNON_HITS", payload);
            crate::combat::absorb_hits_seeing(
                state, content, sources, galaxy, ctx, player, system, &gunner, hits,
            )?;
        }
        let mut pending_event_scoring = None;
        if before_cannon > 0
            && crate::combat::non_fighter_ships_of(state, ctx.content, ctx.sources, player, system)
                == 0
        {
            let occurrence = state.begin_feat_occurrence();
            for gunner in gunners {
                state.record_event_feat(
                    &gunner,
                    ti4_model::state::Feat::SpaceCannonTookTheLastNonFighters,
                    occurrence,
                );
            }
            pending_event_scoring = Some((occurrence, EventScoreLimit::AnyPerPlayer));
        }

        let before_combat = crate::combat::before_combat_with_notes(
            state,
            ctx.content,
            ctx.sources,
            system,
            notes_at_tactical_start.clone(),
        );
        let mut window = crate::combat::CombatWindow::new(state, ctx.content, ctx.sources, system);
        if let Some(galaxy) = galaxy {
            window = window.with_galaxy(galaxy.clone());
        }
        if pending_event_scoring.is_none() {
            window.settle_open(state, ctx)?;
        }
        Ok(Self {
            player: player.clone(),
            system: system.clone(),
            stage: Aftermath::Fighting(Box::new(window)),
            log: Vec::new(),
            before_combat,
            feats_noted: false,
            pending_event_scoring,
            notes_at_tactical_start,
        })
    }

    fn take_event_scoring(&mut self) -> Option<(FeatOccurrence, EventScoreLimit)> {
        self.pending_event_scoring.take()
    }

    /// Open the production step for this activation.
    ///
    /// "When 1 or more of your units use PRODUCTION" is a *before* window: the driver opens it
    /// once the step is built and before its first choice, so a War Machine changes this
    /// production rather than one that already spent its budget. The step produces nothing for
    /// a player with no budget at all, so the window stays shut for them (and a War Machine
    /// cannot create the production it answers). Reactions resolve inside the emit; [`ProductionWindow::refresh`]
    /// then re-derives the budget so faces a reaction added are spent, and a step that would
    /// otherwise have been done re-opens.
    fn enter_production(
        &self,
        state: &mut GameState,
        ctx: &mut Resolving<'_>,
    ) -> crate::production::ProductionWindow {
        let mut window = crate::production::ProductionWindow::new(
            state,
            ctx.content,
            ctx.sources,
            &self.player,
            &self.system,
        );
        if crate::production::capacity(state, ctx.content, ctx.sources, &self.player, &self.system)
            == 0
        {
            return window;
        }
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
        window.refresh(state, ctx.content, ctx.sources);
        window
    }

    /// Move to the next step once the current one owes nothing.
    #[allow(
        clippy::too_many_lines,
        reason = "one arm per aftermath stage, read as a table"
    )]
    fn settle(
        &mut self,
        state: &mut GameState,
        ctx: &mut Resolving<'_>,
    ) -> Result<(), crate::combat::CombatError> {
        loop {
            if self.pending_event_scoring.is_some() {
                return Ok(());
            }
            match &mut self.stage {
                Aftermath::Fighting(window) => {
                    if window
                        .pending_choice(state, ctx.content, ctx.sources)
                        .is_some()
                    {
                        return Ok(());
                    }
                    if let Some(occurrence) = window.take_scoring_occurrence() {
                        self.pending_event_scoring =
                            Some((occurrence, EventScoreLimit::OnePerPlayer));
                        return Ok(());
                    }
                    if window.outcome().is_none() {
                        // A scoring pause can leave the combat at an automatic transition
                        // (currently the ordinary dice immediately after barrage). Drive that
                        // transition before deciding whether the aftermath may invade.
                        window.settle_open(state, ctx)?;
                        continue;
                    }
                    // 49: an invasion only happens if the active player still holds the space.
                    // Membership, not seating order: the activator may be seated behind a
                    // survivor, and only the activator's holding matters.
                    let holds =
                        crate::combat::combatants(state, ctx.content, ctx.sources, &self.system)
                            .iter()
                            .any(|last| last == &self.player);
                    if let Some(outcome) = window.outcome()
                        && !self.feats_noted
                    {
                        self.feats_noted = true;
                        if let Some(occurrence) = window.combat_occurrence()
                            && crate::combat::note_combat_event_feats(
                                state,
                                ctx.content,
                                ctx.sources,
                                &self.system,
                                &self.before_combat,
                                &outcome,
                                occurrence,
                            )
                        {
                            self.pending_event_scoring =
                                Some((occurrence, EventScoreLimit::OnePerPlayer));
                            return Ok(());
                        }
                    }
                    if let Some(outcome) = window.outcome()
                        && outcome.rounds > 0
                    {
                        // "After you win a space combat." A draw wins nothing, so a fight that
                        // ended without a winner opens no window — which is what Skilled Retreat
                        // is for.
                        if let Some(winner) = outcome.winner.clone() {
                            // The losers' ships are off the board by now, so the opponents
                            // are named from the snapshot the fight opened with: every side
                            // but the winner. A two-player fight — the common case — is
                            // exactly "your opponent"; a wider fight is each of the others.
                            // The handoff exists beside the payload because the window that
                            // follows cannot read the payload itself.
                            let opponents: Vec<_> = self
                                .before_combat
                                .sides()
                                .iter()
                                .filter(|side| **side != winner)
                                .cloned()
                                .collect();
                            state.last_combat_sides =
                                Some((self.system.clone(), opponents.clone()));
                            let mut payload = BTreeMap::new();
                            payload.insert(
                                "player".to_owned(),
                                serde_json::Value::String(winner.to_string()),
                            );
                            payload.insert(
                                "system".to_owned(),
                                serde_json::Value::String(self.system.to_string()),
                            );
                            payload.insert(
                                "opponents".to_owned(),
                                serde_json::Value::Array(
                                    opponents
                                        .iter()
                                        .map(|side| serde_json::Value::String(side.to_string()))
                                        .collect(),
                                ),
                            );
                            // Shard of the Throne moves to whoever beat its owner.
                            if opponents.iter().any(|side| {
                                crate::laws::elected(state, "shard_of_the_throne")
                                    .is_some_and(|owner| *owner == side.to_string())
                            }) {
                                crate::laws::steal_throne_card(
                                    state,
                                    "shard_of_the_throne",
                                    &winner,
                                );
                            }
                            ctx.emit(state, "SPACE_COMBAT_WON", payload)?;
                        }
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
                        Aftermath::Invading(Box::new(
                            crate::invasion::InvasionWindow::new_with_notes(
                                state,
                                ctx.content,
                                ctx.sources,
                                ctx.dice,
                                ctx.rng,
                                &self.player,
                                &self.system,
                                self.notes_at_tactical_start.clone(),
                            ),
                        ))
                    } else {
                        // No invasion: straight to production, and the production window opens
                        // before the step makes its first choice (see `enter_production`).
                        let window = self.enter_production(state, ctx);
                        Aftermath::Producing(Box::new(window))
                    };
                }
                Aftermath::Invading(window) => {
                    if let Some((occurrence, combat)) = window.take_scoring_occurrence() {
                        self.pending_event_scoring = Some((
                            occurrence,
                            if combat {
                                EventScoreLimit::OnePerPlayer
                            } else {
                                EventScoreLimit::AnyPerPlayer
                            },
                        ));
                        return Ok(());
                    }
                    window.settle(state, ctx);
                    if let Some((occurrence, combat)) = window.take_scoring_occurrence() {
                        self.pending_event_scoring = Some((
                            occurrence,
                            if combat {
                                EventScoreLimit::OnePerPlayer
                            } else {
                                EventScoreLimit::AnyPerPlayer
                            },
                        ));
                        return Ok(());
                    }
                    if window
                        .pending_choice(state, ctx.content, ctx.sources)
                        .is_some()
                    {
                        return Ok(());
                    }
                    if !window.is_done() {
                        return Ok(());
                    }
                    self.log.push("INVASION_RESOLVED".to_owned());
                    // The production window opens before the step makes its first choice, so a
                    // reaction (War Machine) can change this step rather than one that has
                    // already spent its budget.
                    let window = self.enter_production(state, ctx);
                    self.stage = Aftermath::Producing(Box::new(window));
                }
                Aftermath::Producing(window) => {
                    if window
                        .pending_choice(state, ctx.content, ctx.sources)
                        .is_some()
                    {
                        return Ok(());
                    }
                    // The "when 1 or more of your units use PRODUCTION" window already opened
                    // when the step began, before its first choice was built; nothing else is
                    // owed once the step ends.
                    self.log.push("PRODUCTION_RESOLVED".to_owned());
                    self.stage = Aftermath::Done;
                    return Ok(());
                }
                Aftermath::Done => return Ok(()),
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
        self.settle(state, ctx)
            .map_err(crate::combat::CombatError::into_illegal_choice)?;
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
    notes_at_start: crate::combat::NoteHoldings,
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
    /// TE Warfare resolves its follower window after the free tactical action, not before it.
    secondary_after_tactical: Option<StrategySecondaryWindow>,
    /// The open 81.1 scoring window.
    scoring: Option<ScoringWindow>,
    /// The open window for an action- or agenda-timed secret.
    ///
    /// Kept apart from `scoring` because the two close differently: the 81.1 window hands off to
    /// the rest of the status phase, and this one simply ends. Sharing the field would run the
    /// status steps in the middle of somebody's turn.
    event_scoring: Option<ScoringWindow>,
    /// The open 81.5 token gain, and the report its remaining steps will extend.
    tokens: Option<(TokenGain, Box<StatusPhaseReport>)>,
    /// The open agenda vote, and the agendas still to be put after it.
    voting: Option<(Box<VoteWindow>, Vec<String>)>,
    /// Agendas retained while the just-resolved agenda's scoring occurrence is open.
    agenda_queue_after_event_scoring: Option<Vec<String>>,
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
    /// The first round's strategy phase has announced its start. `start_game` opens the
    /// game already inside that phase, so the `RoundEnded` branch — which announces the
    /// strategy phases of later rounds — never runs for it: without this one-time emit a
    /// card reading "at the start of the strategy phase" held in the starting hand would
    /// sleep until the start of round two, a phase late.
    strategy_phase_announced: bool,
    /// Component actions offered this turn that were taken and did not resolve.
    ///
    /// 22.4 says a component action *cancelled* while announced does not consume the turn, so the
    /// driver re-offers it. A component action that merely **failed** is a different thing: nothing
    /// about the position changed, so re-offering it produces the same failure, and a decider that
    /// keeps choosing it never advances the game. One MLP self-play game asked "action phase"
    /// 9,789 times in a row before the step limit caught it.
    ///
    /// Withholding just the option that failed keeps 22.4 -- the turn is still not consumed and
    /// every other action is still available -- while making progress unavoidable, because the
    /// offer shrinks by one each time.
    failed_component_actions: std::collections::BTreeSet<String>,
    /// Turn sequence whose free start-of-turn technology choices have been resolved.
    prepared_turn_seq: Option<u32>,
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
            secondary_after_tactical: None,
            scoring: None,
            event_scoring: None,
            tokens: None,
            voting: None,
            agenda_queue_after_event_scoring: None,
            galaxy: None,
            tactical: None,
            aftermath: None,
            trade: None,
            rng: GameRng::new(0),
            dice: Dice::new(),
            status_resolved: false,
            agenda_resolved: false,
            strategy_phase_announced: false,
            failed_component_actions: std::collections::BTreeSet::new(),
            prepared_turn_seq: None,
            blocked: None,
        }
    }

    /// The map this game is played on, when it has one.
    ///
    /// Read-only: the driver owns the board, and a caller that could swap it mid-game could move
    /// a system out from under an open tactical action.
    #[must_use]
    pub const fn galaxy(&self) -> Option<&Galaxy> {
        self.galaxy.as_ref()
    }

    /// Give the game its map, which is what makes a tactical action possible.
    #[must_use]
    pub fn with_galaxy(mut self, galaxy: Galaxy) -> Self {
        // 35.5: a frontier token sits on every planetless system from the start. Placing them
        // here rather than in setup is what the galaxy makes possible -- setup has no board yet,
        // and until this existed `frontier_tokens` was written by nothing at all, which left the
        // twenty-card frontier deck unreachable and frontier fragments impossible to gain.
        let placed = crate::exploration::place_frontier_tokens(
            &mut self.state,
            self.content,
            self.sources,
            &galaxy,
        );
        if placed > 0 {
            let mut payload = BTreeMap::new();
            payload.insert(
                "count".to_owned(),
                serde_json::Value::from(u64::try_from(placed).unwrap_or(0)),
            );
            let _ = self.emit_typed("FRONTIER_TOKENS_PLACED", payload);
        }
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
        if let Some(window) = &self.event_scoring {
            return window.pending_choice(&self.state, self.content, self.sources);
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
        // Space station control is a function of occupancy, not an event (rules 2, 2a, 2b), so it
        // is recomputed once per step rather than at each of the dozen places a unit can move or
        // die. Doing it here means a movement path added later cannot forget to.
        crate::space_stations::reconcile_all(&mut self.state, self.content, self.sources);
        // The two wormhole laws are switches on the map, and the map is owned here. Set once per
        // step for the same reason as station control: derived state is cheaper to recompute than
        // to keep in sync from every place a law can be enacted or repealed.
        if let Some(galaxy) = self.galaxy.as_mut() {
            crate::laws::apply_to_galaxy(&self.state, galaxy);
        }

        if self.state.finished {
            return self.result(false, None);
        }
        if let Some(error) = self.blocked.clone() {
            return self.result(false, Some(error));
        }

        if self.secondary.is_some() {
            return self.step_secondary();
        }
        if self.event_scoring.is_some() {
            return self.step_event_scoring();
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
        if self.state.phase == Phase::Action
            && self.prepared_turn_seq != Some(self.state.turn_seq)
            && let Some(active) = self.state.active.clone()
        {
            if let Err(error) = crate::technology::start_turn(
                &mut self.state,
                self.content,
                self.sources,
                self.galaxy.as_ref(),
                &mut self.table,
                &active,
            ) {
                return self.result(false, Some(error.into()));
            }
            self.prepared_turn_seq = Some(self.state.turn_seq);
            // "At the start of another player's turn" — this is that moment, typed so a
            // window can hang off it: the payload names the seat whose turn is beginning,
            // and the marker lands before the seat has chosen any action for it.
            self.sync_timing_context();
            let mut payload = BTreeMap::new();
            payload.insert(
                "player".to_owned(),
                serde_json::Value::String(active.to_string()),
            );
            if let Err(error) = self.emit_typed("TURN_BEGAN", payload) {
                return self.result(false, Some(error));
            }
        }
        if self.state.phase == Phase::Status && !self.status_resolved {
            return self.step_status();
        }
        if self.state.phase == Phase::Agenda && !self.agenda_resolved {
            return self.step_agenda();
        }
        if self.state.phase == Phase::Strategy
            && self.state.round == 1
            && !self.strategy_phase_announced
        {
            self.strategy_phase_announced = true;
            self.sync_timing_context();
            if let Err(error) = self.emit_typed("STRATEGY_PHASE_BEGAN", BTreeMap::new()) {
                return self.result(false, Some(error));
            }
        }

        let Some(choice) = self.legal_options() else {
            return self.step_phase();
        };
        // Field borrows, not `self`: the table answers while the position stays readable.
        let answer = match self.table.ask_seeing(
            &choice,
            &Observed::new(
                &self.state,
                self.content,
                self.sources,
                self.galaxy.as_ref(),
            ),
        ) {
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
        // What the last step asked, and how many in a row asked the same thing. Kept so a step
        // limit can name the loop it died in rather than only its round and phase.
        let mut last_prompt = String::new();
        let mut repeats = 0usize;
        while self.state.round < target && !self.state.finished {
            if steps >= max_steps {
                return Err(RunError::StepLimit {
                    max_steps,
                    round: self.state.round,
                    phase: self.state.phase,
                    recent: last_prompt,
                    repeats,
                });
            }
            let asked = self
                .legal_options()
                .map_or_else(String::new, |choice| choice.prompt.clone());
            if asked == last_prompt {
                repeats += 1;
            } else {
                last_prompt = asked;
                repeats = 1;
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
            return window.pending_choice(&self.state, self.content, self.sources);
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
                    self.content,
                    galaxy,
                    active,
                ));
        }
        choice.options.extend(crate::relics::available_actions(
            &self.state,
            self.content,
            self.sources,
            self.galaxy.as_ref(),
            active,
        ));
        choice.options.extend(crate::exploration::available_actions(
            &self.state,
            self.content,
            self.sources,
            active,
        ));
        choice.options.extend(crate::technology::component_actions(
            &self.state,
            self.content,
            self.sources,
            active,
        ));
        choice
            .options
            .extend(crate::thunders_edge::available_actions(
                &self.state,
                self.content,
                self.sources,
                active,
            ));
        // A faction's own component actions — Sol's Orbital Drop is the first.
        choice
            .options
            .extend(crate::faction_abilities::component_actions(
                &self.state,
                self.content,
                active,
            ));
        // 22.1: cards whose printed window is "Action" are played on your turn. Without this
        // every such card is drawn, held, discarded to the hand limit, and never played.
        choice
            .options
            .extend(crate::action_cards::available_actions(
                &self.state,
                self.content,
                self.sources,
                self.galaxy.as_ref(),
                active,
            ));
        // A component action taken this turn that did not resolve is not offered again: nothing
        // about the position changed, so it would fail identically, and a decider that keeps
        // choosing it never advances the game. 22.4 is preserved -- the turn was not consumed and
        // every other action, including passing, is still here.
        if !self.failed_component_actions.is_empty() {
            choice
                .options
                .retain(|option| !self.failed_component_actions.contains(&option.id));
        }
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

    #[allow(
        clippy::too_many_lines,
        reason = "one arm per kind of turn action, read as a table of what a turn may be"
    )]
    fn apply_choice(&mut self, choice: &Choice, answer: ChoiceOption) -> Result<(), GameError> {
        match self.state.phase {
            Phase::Strategy => {
                let player = self.take_timed_strategy_card(&choice.player, answer)?;
                self.emit("STRATEGY_CARD_CHOSEN");
                debug_assert_eq!(player, choice.player);
                // Imperial Arbiter is offered once the last card is gone: "at the end of the
                // strategy phase". Checked here rather than on the phase transition because the
                // swap has to happen while the phase's own state is still the current one.
                if strategy_options(&self.state, self.content).is_none() {
                    self.imperial_arbiter();
                }
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
                    // Prove Endurance. Recorded the moment the last seat passes, because the
                    // action phase ends immediately afterwards and the fact is gone by the time
                    // anything else could look for it. "Last to pass" is decided by there being
                    // nobody left who has not.
                    if self.state.players.iter().all(|seat| seat.passed) {
                        let occurrence = self.state.begin_feat_occurrence();
                        self.state
                            .record_event_feat(&active, Feat::LastToPass, occurrence);
                        self.open_occurrence_event_scoring(
                            crate::secrets::Timing::Action,
                            occurrence,
                            EventScoreLimit::AnyPerPlayer,
                        );
                    }
                    self.emit("PLAYER_PASSED");
                    let mut payload = BTreeMap::new();
                    payload.insert(
                        "player".to_owned(),
                        serde_json::Value::String(active.to_string()),
                    );
                    self.emit_typed("PLAYER_PASSED", payload)?;
                    // An explicit pass declines the extra action Master Plan may have granted.
                    self.state
                        .transient_flags
                        .clear(TransientFlags::ADDITIONAL_ACTION);
                    self.advance_turn()?;
                    return Ok(());
                }
                // 22.1: a component action costs the whole turn, so unlike a transaction this
                // advances it whether or not the relic did anything worth having.
                if answer.id.starts_with("faction|") {
                    let done = self.play_faction_action(&active, &answer);
                    // Extreme Duress bites once the action is taken: the played card is
                    // already out of the hand, so only what is left gets discarded.
                    self.settle_extreme_duress(&active, false)?;
                    self.emit(if done {
                        "COMPONENT_ACTION_RESOLVED"
                    } else {
                        "COMPONENT_ACTION_FAILED"
                    });
                    if !done {
                        self.failed_component_actions.insert(answer.id.clone());
                        return Ok(());
                    }
                    self.finish_action()?;
                    return Ok(());
                }
                if answer.id.starts_with("component|tech|") {
                    let done = crate::technology::perform_component(
                        &mut self.state,
                        self.content,
                        self.sources,
                        self.galaxy.as_ref(),
                        &mut self.table,
                        &active,
                        &answer,
                    )?;
                    self.settle_extreme_duress(&active, false)?;
                    self.emit(if done {
                        "COMPONENT_ACTION_RESOLVED"
                    } else {
                        "COMPONENT_ACTION_FAILED"
                    });
                    if !done {
                        self.failed_component_actions.insert(answer.id.clone());
                        return Ok(());
                    }
                    self.finish_action()?;
                    return Ok(());
                }
                if answer.id.starts_with("component|expedition|") {
                    let done = crate::thunders_edge::perform(
                        &mut self.state,
                        self.content,
                        self.sources,
                        self.galaxy.as_ref(),
                        &mut self.table,
                        &active,
                        &answer,
                    )?;
                    self.settle_extreme_duress(&active, false)?;
                    self.emit(if done {
                        "COMPONENT_ACTION_RESOLVED"
                    } else {
                        "COMPONENT_ACTION_FAILED"
                    });
                    if !done {
                        self.failed_component_actions.insert(answer.id.clone());
                        return Ok(());
                    }
                    self.finish_action()?;
                    return Ok(());
                }
                if let Some(index) = answer.id.strip_prefix("action_card|") {
                    let _ = index;
                    // Propagated, not `unwrap_or(false)`: a refused announcement is an engine
                    // error, never a silently failed action.
                    let done = self.play_component_action(&active, &answer)?;
                    self.settle_extreme_duress(&active, false)?;
                    self.emit(if done {
                        "COMPONENT_ACTION_RESOLVED"
                    } else {
                        "COMPONENT_ACTION_FAILED"
                    });
                    if !done {
                        // 22.4: the play was cancelled while announced (22.3 covers what
                        // cannot be resolved): the action is not used, so the same turn
                        // re-offers its options instead of advancing. The cancelled card is
                        // already out of the hand, so it cannot be chosen again.
                        return Ok(());
                    }
                    self.finish_action()?;
                    return Ok(());
                }
                if answer.kind == crate::relics::ACTION_KIND
                    && crate::exploration::perform_action(
                        &mut self.state,
                        self.content,
                        self.sources,
                        &mut self.table,
                        &active,
                        &answer,
                    )
                {
                    // An Enigmatic Device from the exploration deck, not a relic. It shares the
                    // relic action kind because it is the same kind of thing to a decider -- a
                    // component action from a card in the play area -- and `perform_action`
                    // declines anything that is not one of its own. A device action cannot be
                    // cancelled, so it always resolves.
                    self.settle_extreme_duress(&active, false)?;
                    self.emit("COMPONENT_ACTION_RESOLVED");
                    self.finish_action()?;
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
                        &mut self.table,
                        self.galaxy.as_ref(),
                        &active,
                        &answer,
                    );
                    self.dice = dice;
                    self.rng = rng;
                    self.settle_extreme_duress(&active, false)?;
                    self.emit(if done {
                        "COMPONENT_ACTION_RESOLVED"
                    } else {
                        "COMPONENT_ACTION_FAILED"
                    });
                    if !done {
                        self.failed_component_actions.insert(answer.id.clone());
                        return Ok(());
                    }
                    self.finish_action()?;
                    return Ok(());
                }
                if let Some(partner) = crate::transactions::opens_with(&self.state, &answer) {
                    self.settle_extreme_duress(&active, false)?;
                    self.trade = Some(crate::transactions::TradeWindow::open(
                        &mut self.state,
                        &active,
                        &partner,
                    ));
                    // Typed as well as logged, so the card that reads "when you are negotiating
                    // a transaction" has a window to be played into. The payload names both
                    // chairs, because either of them may hold the card; the window it opens
                    // settles before the first question of the negotiation is asked.
                    self.sync_timing_context();
                    let mut payload = BTreeMap::new();
                    payload.insert(
                        "player".to_owned(),
                        serde_json::Value::String(active.to_string()),
                    );
                    payload.insert(
                        "partner".to_owned(),
                        serde_json::Value::String(partner.to_string()),
                    );
                    self.emit_typed("TRANSACTION_OPENED", payload)?;
                    return Ok(());
                }
                if answer.kind != ACTION_KIND {
                    return Err(GameError::UnsupportedAction(answer.id));
                }
                if answer.id == TACTICAL_ACTION_ID {
                    self.settle_extreme_duress(&active, false)?;
                    self.tactical = Some(TacticalWindow {
                        player: active,
                        stage: TacticalStage::Activating,
                        notes_at_start: crate::combat::note_holdings(&self.state),
                    });
                    self.emit("TACTICAL_ACTION_BEGAN");
                    return Ok(());
                }
                let window =
                    begin_strategic_action(&mut self.state, self.content, &active, answer)?;
                // Strategic is the one action Extreme Duress does not punish: it lifts
                // quietly, settled after the action is set up so a cancelled action (Coup
                // d'Etat) leaves nothing behind but the duress the target no longer owes.
                self.settle_extreme_duress(&active, true)?;
                let card = window.card().to_string();
                // "When another player would perform a strategic action" — typed, and fired
                // before anything is resolved, so Coup d'Etat can end the turn in time to
                // undo the action entirely. Cleared up front so a stale value from an
                // interrupted step cannot veto this action.
                self.state
                    .transient_flags
                    .clear(TransientFlags::STRATEGIC_CANCELLED);
                let mut payload = BTreeMap::new();
                payload.insert(
                    "player".to_owned(),
                    serde_json::Value::String(active.to_string()),
                );
                self.emit_typed("STRATEGIC_ACTION_BEGAN", payload)?;
                if self
                    .state
                    .transient_flags
                    .has(TransientFlags::STRATEGIC_CANCELLED)
                {
                    // Coup d'Etat: "End that player's turn, the strategic action is not
                    // resolved and the strategy card is not exhausted." Nothing above
                    // changed state — the card is still in hand, unexhausted, no token
                    // placed, no ability resolved — so passing the turn undoes the action
                    // exactly as the card says.
                    self.state
                        .transient_flags
                        .clear(TransientFlags::STRATEGIC_CANCELLED);
                    self.emit(&format!("STRATEGIC_ACTION_CANCELLED:{card}"));
                    self.advance_turn()?;
                    return Ok(());
                }
                let outcome = crate::strategy_cards::primary(
                    &mut self.state,
                    self.content,
                    self.sources,
                    self.galaxy.as_ref(),
                    &mut self.table,
                    &active,
                    &card,
                )?;
                self.resolve_faction_strategy(&active, &card);
                match outcome {
                    crate::strategy_cards::Ability::FreeTactical(system) => {
                        // TE Warfare explicitly waives the token and permits an already-tokened
                        // system, but the rest is the ordinary movement/aftermath pipeline.
                        self.state.active_system = Some(system);
                        self.state.pending = Some("move".to_owned());
                        self.state.activation_seq = self.state.activation_seq.saturating_add(1);
                        self.tactical = Some(TacticalWindow {
                            player: active,
                            stage: TacticalStage::Moving,
                            notes_at_start: crate::combat::note_holdings(&self.state),
                        });
                        self.secondary_after_tactical = Some(window);
                        self.emit("SYSTEM_ACTIVATED");
                        self.emit("FREE_TACTICAL_ACTION");
                    }
                    crate::strategy_cards::Ability::Resolved
                    | crate::strategy_cards::Ability::Unresolved => {
                        self.secondary = Some(window);
                    }
                }
                self.emit("STRATEGIC_ACTION_BEGAN");
                Ok(())
            }
            Phase::Status | Phase::Agenda => {
                Err(GameError::UnsupportedAction(choice.prompt.clone()))
            }
        }
    }

    /// Extreme Duress settles when the target takes an action. A strategic action is the
    /// one case the card does not punish: the duress lifts quietly and the action
    /// proceeds. Every other action the target takes triggers the punishment — discard
    /// every action card, hand over every trade good, show every secret objective — and
    /// the action proceeds with what the target has left, settled after synchronous
    /// actions resolve so the card just played is not among the discarded ones. (Showing
    /// the secret objectives is a hidden peek: the holder already sees their own view, so
    /// there is no state to change, the same reading as Insider Information.) A pass is
    /// not an action, so it neither triggers nor lifts the duress: it stays armed until
    /// the next action.
    /// Settle an Extreme Duress the player owes, once their action is taken.
    ///
    /// # Errors
    /// [`GameError`] when the windows a discarded card opens get an illegal answer.
    fn settle_extreme_duress(
        &mut self,
        player: &PlayerId,
        strategic: bool,
    ) -> Result<(), GameError> {
        let Some(by) = self
            .state
            .player(player)
            .and_then(|seat| seat.duress_by.clone())
        else {
            return Ok(());
        };
        if strategic {
            self.state.player_mut(player).expect("just read").duress_by = None;
            return Ok(());
        }
        let (goods, discarded) = {
            let seat = self.state.player_mut(player).expect("just read");
            let goods = seat.trade_goods;
            seat.trade_goods = 0;
            // The cards are named one by one below, so they are taken out, not cleared away.
            let discarded = std::mem::take(&mut seat.action_cards);
            seat.duress_by = None;
            (goods, discarded)
        };
        if let Some(holder) = self.state.player_mut(&by) {
            holder.trade_goods += goods;
        }
        self.emit(&format!("EXTREME_DURESS:{player}"));
        // The punishment discards every action card left in the hand, and every discarded
        // component action is a moment another player's Reverse Engineer may take.
        for card in discarded {
            self.state.discarded_action_cards.push(card.clone());
            self.sync_timing_context();
            let mut payload = BTreeMap::new();
            payload.insert(
                "player".to_owned(),
                serde_json::Value::String(player.to_string()),
            );
            payload.insert(
                "card".to_owned(),
                serde_json::Value::String(card.to_string()),
            );
            self.emit_typed("ACTION_CARD_DISCARDED", payload)?;
        }
        Ok(())
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
        // Field borrows, not `self`: the table answers while the position stays readable.
        let answer = match self.table.ask_seeing(
            &choice,
            &Observed::new(
                &self.state,
                self.content,
                self.sources,
                self.galaxy.as_ref(),
            ),
        ) {
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
    /// The result says whether the action was performed. A performed component action costs
    /// the turn; a cancelled one (22.4) does not, and the caller keeps the turn instead of
    /// advancing it.
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

    /// Perform a faction component action through the game's own timing context.
    fn play_faction_action(&mut self, player: &PlayerId, answer: &ChoiceOption) -> bool {
        let (content, sources) = (self.content, self.sources);
        let galaxy = self.galaxy.clone();
        let logged = self.timing.log().len();
        let done = {
            let (state, table, dice, rng, event_sequence) = (
                &mut self.state,
                &mut self.table,
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
            crate::faction_abilities::perform_component(&mut context, player, answer)
        };
        self.mirror_timing_log(logged);
        done
    }

    /// Let a faction react to a strategy card finishing.
    fn resolve_faction_strategy(&mut self, player: &PlayerId, card: &str) {
        let name =
            crate::strategy_cards::card_name(self.content, card).unwrap_or_else(|| card.to_owned());
        let (content, sources) = (self.content, self.sources);
        let galaxy = self.galaxy.clone();
        let (state, table, dice, rng, event_sequence) = (
            &mut self.state,
            &mut self.table,
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
        crate::faction_abilities::strategy_resolved(&mut context, player, &name);
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
                self.state.gravleash_move_values.clear();
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
                // "Before you move units during a tactical action, you may purge this card." The
                // activation has happened and the move has not, which is the window the card names.
                // Minister of Peace: "After a player activates a system that contains 1 or more of
                // a different player's units, the owner of this card may discard this card --
                // immediately end the active player's turn."
                if self.minister_of_peace(&system, &window.player) {
                    self.tactical = None;
                    self.state.active_system = None;
                    self.state.pending = None;
                    self.emit("TURN_ENDED_BY_MINISTER_OF_PEACE");
                    self.advance_turn()?;
                    return Ok(self.result(true, None));
                }
                crate::relics::offer_dominus_orb(&mut self.state, &mut self.table, &window.player);
                window.stage = TacticalStage::Moving;
                self.tactical = Some(window);
                Ok(self.result(true, None))
            }
            TacticalStage::Moving => match read_move(choice, answer)? {
                MoveSelection::Done => {
                    self.tactical = Some(window);
                    Ok(self.finish_tactical())
                }
                MoveSelection::Ship {
                    origin,
                    index,
                    gravity_drive,
                } => self.begin_one_move(window, &origin, index, gravity_drive),
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
                    // Ceasefire: the holder stops this player moving in, and the note is spent
                    // doing it. Checked here rather than at activation because this is the
                    // moment the denial bites.
                    if let Some(active) = self.state.active_system.clone()
                        && crate::promissory::denies_movement_into(
                            &self.state,
                            &window.player,
                            &active,
                        )
                    {
                        crate::promissory::use_ceasefire(&mut self.state, &window.player);
                        self.emit("CEASEFIRE_USED");
                        window.stage = TacticalStage::Moving;
                        self.tactical = Some(window);
                        return Ok(self.result(true, None));
                    }
                    let outcome = self.sail(&origin, &ship, &path, cargo);
                    // The ion storm flips when ships use it. "Use the wormhole" means the move
                    // crossed it, so the storm's system has to be one end of the hop -- checked
                    // against the origin and the destination rather than the whole path, because a
                    // ship passing *through* the storm's system by ordinary hex adjacency has not
                    // used the wormhole at all.
                    if let Some(destination) = self.state.active_system.clone() {
                        crate::exploration::flip_ion_storm(&mut self.state, &origin, &destination);
                    }
                    self.note_arrival(&window.player, &outcome);
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

    /// Announce "a player moved ships into" a system.
    ///
    /// Ending movement on a frontier token is *not* exploring it: LRR 35 allows exploration
    /// only for a player who owns the Dark Energy Tap technology or another game effect, and
    /// DET's own trigger fires when the tactical action ends (`close_tactical`), not on the
    /// move that landed the ship. The arrival here only emits the event other cards react to.
    fn note_arrival(&mut self, player: &PlayerId, outcome: &MoveOutcome) {
        if !matches!(outcome, MoveOutcome::Arrived { .. }) {
            return;
        }
        // Three printed windows read "after a player moves ships into" a system.
        let mut payload = BTreeMap::new();
        payload.insert(
            "player".to_owned(),
            serde_json::Value::String(player.to_string()),
        );
        let _ = self.emit_typed("SHIP_MOVED", payload);
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
        gravity_drive: bool,
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
                    crate::tactical::effective_move_value_with_gravity(
                        &self.state,
                        kind,
                        &window.player,
                        origin,
                        gravity_drive,
                    ),
                )
            })
            .ok_or_else(|| TacticalError::UnknownSystem(origin.clone()))?;

        if gravity_drive && !crate::technology::use_gravity_drive(&mut self.state, &window.player) {
            return Err(TacticalError::IllegalChoice(IllegalChoice::NotOffered {
                player: window.player.clone(),
                chosen: format!("move_gd|{origin}|{index}"),
                offered: Vec::new(),
            })
            .into());
        }

        if self
            .state
            .player(&window.player)
            .and_then(|seat| seat.breakthrough.as_ref())
            .is_some_and(|alias| alias.as_str() == "letnevbt")
        {
            let own_move =
                ti4_content::units::unit_type(self.content, ship.type_id.as_str(), self.sources)
                    .map_or(0, |kind| i32::try_from(kind.move_value()).unwrap_or(0))
                    + crate::action_cards::move_bonus(
                        &self.state,
                        &window.player,
                        self.state.activation_seq,
                    );
            self.state
                .gravleash_move_values
                .entry(origin.clone())
                .and_modify(|value| *value = (*value).max(own_move))
                .or_insert(own_move);
        }
        let hold = CargoWindow::for_ship(
            &self.state,
            self.content,
            self.sources,
            &window.player,
            origin,
            &ship,
            &path,
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
        let tactical = self
            .tactical
            .as_ref()
            .map(|window| (window.player.clone(), window.notes_at_start.clone()));
        self.tactical = None;
        let system = self.state.active_system.clone();

        let (Some((player, notes_at_start)), Some(system)) = (tactical, system) else {
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
        let opened = AftermathWindow::new(
            &mut self.state,
            &mut ctx,
            &player,
            &system,
            galaxy.as_ref(),
            notes_at_start,
        );
        let mut window = match opened {
            Ok(window) => window,
            Err(error) => {
                self.dice = dice;
                self.rng = rng;
                self.mirror_timing_log(logged);
                return self.result(false, Some(error));
            }
        };
        if let Err(error) = window.settle(&mut self.state, &mut ctx) {
            self.dice = dice;
            self.rng = rng;
            self.mirror_timing_log(logged);
            return self.result(false, Some(error.into()));
        }
        self.dice = dice;
        self.rng = rng;
        self.mirror_timing_log(logged);
        self.events.append(&mut window.log);

        if let Some((occurrence, limit)) = window.take_event_scoring() {
            self.aftermath = Some(window);
            self.open_occurrence_event_scoring(crate::secrets::Timing::Action, occurrence, limit);
            return self.result(false, None);
        }

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
    #[allow(
        clippy::too_many_lines,
        reason = "one case per aftermath stage, read as a table"
    )]
    fn step_aftermath(&mut self) -> StepResult {
        let Some(choice) = self.legal_options() else {
            // An event-scoped scoring pause can leave combat in an automatic intermediate
            // stage. Resume it here rather than mistaking its lack of a player choice for a
            // completed tactical action.
            let Some(mut window) = self.aftermath.take() else {
                return self.close_tactical();
            };
            let mut dice = std::mem::take(&mut self.dice);
            let mut rng = self.rng.clone();
            let galaxy = self.galaxy.clone();
            let logged = self.timing.log().len();
            {
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
                if let Err(error) = window.settle(&mut self.state, &mut ctx) {
                    self.dice = dice;
                    self.rng = rng;
                    self.mirror_timing_log(logged);
                    return self.result(false, Some(error.into()));
                }
            }
            self.dice = dice;
            self.rng = rng;
            self.mirror_timing_log(logged);
            self.events.append(&mut window.log);
            if let Some((occurrence, limit)) = window.take_event_scoring() {
                self.aftermath = Some(window);
                self.open_occurrence_event_scoring(
                    crate::secrets::Timing::Action,
                    occurrence,
                    limit,
                );
                return self.result(false, None);
            }
            if window
                .pending_choice(&self.state, self.content, self.sources)
                .is_some()
            {
                self.aftermath = Some(window);
                return self.result(false, None);
            }
            return self.close_tactical();
        };
        // Field borrows, not `self`: the table answers while the position stays readable.
        let answer = match self.table.ask_seeing(
            &choice,
            &Observed::new(
                &self.state,
                self.content,
                self.sources,
                self.galaxy.as_ref(),
            ),
        ) {
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
        if let Some((occurrence, limit)) = window.take_event_scoring() {
            self.aftermath = Some(window);
            self.open_occurrence_event_scoring(crate::secrets::Timing::Action, occurrence, limit);
            return self.result(true, None);
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
        // The Crown of Emphidia: "After you perform a tactical action, you may exhaust this card to
        // explore 1 planet you control." Here rather than in `finish_tactical`, because a tactical
        // action can end down either path and only this one is common to both.
        if let Some(player) = self.state.active.clone() {
            crate::relics::crown_of_emphidia_explore(
                &mut self.state,
                self.content,
                self.sources,
                &mut self.table,
                &player,
            );
        }
        // Dark Energy Tap: "After you perform a tactical action in a system that contains a
        // frontier token, if you have 1 or more ships in that system, explore that token."
        // The trigger is the action ending, not any move inside it: a fleet already parked on
        // the token explores, and a move that lands on the token does not — `note_arrival`
        // only announces the arrival, so the exploration must happen here.
        if let (Some(player), Some(system)) =
            (self.state.active.clone(), self.state.active_system.clone())
            && self.state.frontier_tokens.contains(&system)
            && crate::technology::owns_det(&self.state, &player)
            && self
                .state
                .system_state(&system)
                .units
                .iter()
                .any(|unit| unit.owner == player)
        {
            let mut dice = crate::dice::Dice::new();
            let mut rng = crate::rng::GameRng::new(0);
            let mut ctx = crate::choice::Resolving {
                content: self.content,
                sources: self.sources,
                dice: &mut dice,
                rng: &mut rng,
                table: &mut self.table,
                timing: None,
            };
            if crate::exploration::explore_frontier(&mut self.state, &mut ctx, &player, &system)
                .is_some()
            {
                self.emit("FRONTIER_EXPLORED");
            }
        }
        self.aftermath = None;
        self.state.active_system = None;
        self.state.pending = None;
        self.emit("TACTICAL_ACTION_COMPLETE");
        if let Some(window) = self.secondary_after_tactical.take() {
            self.secondary = Some(window);
            return self.result(false, None);
        }
        // A Warfare action completes when its window does, which is where the event fires;
        // only a plain tactical action completes here.
        if let Err(error) = self.finish_action() {
            return self.result(false, Some(error));
        }
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
            .pending_choice(&self.state, self.content);
        let Some(choice) = choice else {
            self.trade = None;
            return self.result(false, None);
        };
        // Field borrows, not `self`: the table answers while the position stays readable.
        let answer = match self.table.ask_seeing(
            &choice,
            &Observed::new(
                &self.state,
                self.content,
                self.sources,
                self.galaxy.as_ref(),
            ),
        ) {
            Ok(answer) => answer,
            Err(error) => return self.result(false, Some(error.into())),
        };
        let outcome = self.trade.as_mut().expect("window remains open").resolve(
            &mut self.state,
            self.content,
            &galaxy,
            &answer,
        );
        // A resolved deal opens Lie in Wait's window. Emitted only on Resolved: the card counts
        // transactions that happened, not offers that were made.
        if matches!(outcome, crate::transactions::Traded::Resolved) {
            let payload = BTreeMap::new();
            if let Err(error) = self.emit_typed("TRANSACTION_RESOLVED", payload) {
                return self.result(false, Some(error));
            }
        }
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
            // The negotiation the Black Market marker was set for is over; the marker dies
            // with it, or it would unlock the next player's table as well.
            self.state
                .transient_flags
                .clear(TransientFlags::BLACK_MARKET);
        }
        self.result(true, None)
    }

    fn step_secondary(&mut self) -> StepResult {
        let choice = self.secondary.as_mut().expect("checked above").next_choice(
            &mut self.state,
            self.content,
            self.sources,
        );
        let Some(choice) = choice else {
            self.secondary = None;
            self.emit("STRATEGIC_ACTION_COMPLETE");
            if let Err(error) = self.finish_action() {
                return self.result(false, Some(error));
            }
            return self.result(false, None);
        };
        let follower = choice.player.clone();
        let card = self
            .secondary
            .as_ref()
            .expect("window remains open")
            .card()
            .to_string();
        // Field borrows, not `self`: the table answers while the position stays readable.
        let answer = match self.table.ask_seeing(
            &choice,
            &Observed::new(
                &self.state,
                self.content,
                self.sources,
                self.galaxy.as_ref(),
            ),
        ) {
            Ok(answer) => answer,
            Err(error) => return self.result(false, Some(error.into())),
        };
        let (resolution, complete) = match self
            .secondary
            .as_mut()
            .expect("window remains open")
            .take_choice(&mut self.state, self.content, self.sources, answer)
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
        if resolution == SecondaryResolution::Followed {
            let name = crate::strategy_cards::card_name(self.content, &card)
                .unwrap_or_else(|| card.clone());
            let outcome = if crate::faction_abilities::substitutes_primary(
                &self.state,
                self.content,
                &follower,
                &name,
            ) {
                crate::strategy_cards::primary(
                    &mut self.state,
                    self.content,
                    self.sources,
                    self.galaxy.as_ref(),
                    &mut self.table,
                    &follower,
                    &card,
                )
            } else {
                crate::strategy_cards::secondary(
                    &mut self.state,
                    self.content,
                    self.sources,
                    self.galaxy.as_ref(),
                    &mut self.table,
                    &follower,
                    &card,
                )
            };
            if let Err(error) = outcome {
                return self.result(false, Some(error.into()));
            }
            self.resolve_faction_strategy(&follower, &card);
        }
        if complete {
            self.secondary = None;
            self.emit("STRATEGIC_ACTION_COMPLETE");
            if let Err(error) = self.finish_action() {
                return self.result(false, Some(error));
            }
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
        // Field borrows, not `self`: the table answers while the position stays readable.
        let answer = match self.table.ask_seeing(
            &choice,
            &Observed::new(
                &self.state,
                self.content,
                self.sources,
                self.galaxy.as_ref(),
            ),
        ) {
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

    /// Open a scoring window for one exact event occurrence.
    ///
    /// An empty window is intentional: it closes without a choice on the next step, preserving
    /// the tactical continuation while keeping event detection independent from card holdings.
    fn open_occurrence_event_scoring(
        &mut self,
        timing: crate::secrets::Timing,
        occurrence: FeatOccurrence,
        limit: EventScoreLimit,
    ) {
        debug_assert!(self.event_scoring.is_none());
        let mut window = ScoringWindow::for_occurrence(
            &self.state.initiative_order(),
            timing,
            occurrence,
            limit,
        );
        if let Some(galaxy) = self.galaxy.clone() {
            window = window.with_galaxy(galaxy);
        }
        self.event_scoring = Some(window);
    }

    /// Resolve one player's decision in an action- or agenda-timed secret window.
    fn step_event_scoring(&mut self) -> StepResult {
        let Some(choice) = self.legal_options() else {
            self.event_scoring = None;
            if let Some(queue) = self.agenda_queue_after_event_scoring.take() {
                return self.open_next_vote(queue);
            }
            return self.result(false, None);
        };
        let answer = match self.table.ask_seeing(
            &choice,
            &Observed::new(
                &self.state,
                self.content,
                self.sources,
                self.galaxy.as_ref(),
            ),
        ) {
            Ok(answer) => answer,
            Err(error) => return self.result(false, Some(error.into())),
        };
        let Some(mut window) = self.event_scoring.take() else {
            unreachable!("the event window is open");
        };
        let outcome = window.resolve(&mut self.state, self.content, self.sources, answer);
        self.event_scoring = Some(window);

        match outcome {
            Ok(scored) => {
                if let Some(alias) = scored {
                    self.emit(&format!("OBJECTIVE_SCORED:{alias}"));
                }
                if self.state.finished {
                    self.event_scoring = None;
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
        // 51.7 again, with the map this time. A commander unlocks on a condition that changes
        // during play — trade goods, planets held, ships in a system — not only when an
        // objective is scored, so the check has to happen somewhere a round reaches.
        let seats: Vec<PlayerId> = self
            .state
            .players
            .iter()
            .map(|seat| seat.id.clone())
            .collect();
        let galaxy = self.galaxy.clone();
        for player in seats {
            let unlocked = crate::leaders::check_unlocks(
                &mut self.state,
                self.content,
                self.sources,
                galaxy.as_ref(),
                &player,
            );
            for leader in unlocked {
                self.events.push(format!("LEADER_UNLOCKED:{leader}"));
            }
        }
        match resolve_before_token_gain(&mut self.state) {
            Ok(report) if report.game_ended => {
                self.emit("GAME_FINISHED");
                self.result(false, None)
            }
            Ok(mut report) => {
                // The Neuraloop reacts to the objective this reveal just turned up. Here rather
                // than inside the status phase because it needs a decider and the deck's rng, and
                // `resolve_before_token_gain` deliberately has neither.
                if let Some(revealed) = report.revealed_objective.clone()
                    && let Some(replacement) = crate::relics::neuraloop(
                        &mut self.state,
                        &mut self.table,
                        &mut self.rng,
                        &revealed,
                    )
                {
                    self.emit(&format!("OBJECTIVE_REPLACED:{revealed}:{replacement}"));
                    report.revealed_objective = Some(replacement);
                }
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
        // Field borrows, not `self`: the table answers while the position stays readable.
        let answer = match self.table.ask_seeing(
            &choice,
            &Observed::new(
                &self.state,
                self.content,
                self.sources,
                self.galaxy.as_ref(),
            ),
        ) {
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
        // 81.5, second sentence: "Then, each player can redistribute each command token on their
        // command sheet ... among their strategy, tactic, and fleet pools." The gain above places
        // two tokens by choice; this is the separate rearrangement of everything already held, and
        // it happens every round for every player. Initiative order, like the rest of 81.
        for player in report.initiative_order.clone() {
            let _ = crate::strategy_cards::redistribute_tokens(
                &mut self.state,
                self.content,
                self.sources,
                self.galaxy.as_ref(),
                &mut self.table,
                &player,
            );
        }
        // "When you would return strategy cards during the status phase" — fired per seat
        // that holds cards, so every holder's window opens (and every card plays) before
        // 81.8 returns the cards: a Political Stability played here sets its seat's
        // marker, and 81.8 honors it.
        let seats: Vec<PlayerId> = self
            .state
            .players
            .iter()
            .filter(|seat| !seat.strategy_cards.is_empty())
            .map(|seat| seat.id.clone())
            .collect();
        self.sync_timing_context();
        for player in seats {
            let mut payload = BTreeMap::new();
            payload.insert(
                "player".to_owned(),
                serde_json::Value::String(player.to_string()),
            );
            if let Err(error) = self.emit_typed("STRATEGY_CARDS_WOULD_RETURN", payload) {
                return self.result(false, Some(error));
            }
        }
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
    /// 8.19 with an empty ballot is not a decision anyone can be asked to make. Veto, played
    /// into the reveal window, discards the revealed agenda and reveals the one it drew from
    /// the top of the deck; [`Self::reveal_agenda`] follows that chain to the agenda that
    /// actually goes to a vote.
    fn open_next_vote(&mut self, mut queue: Vec<String>) -> StepResult {
        while let Some(alias) = queue.first().cloned() {
            queue.remove(0);
            match self.reveal_agenda(&alias) {
                Ok(Some((alias, choices))) => {
                    // Committee Formation: "Before players vote on an agenda that requires a
                    // player to be elected, the owner of this card may discard this card to choose
                    // a player to be elected. Players do not vote on that agenda." So it is offered
                    // before the window opens, and taking it skips the vote entirely.
                    let elects_a_player = self
                        .content
                        .get(ti4_model::content_types::ContentType::Agendas, &alias)
                        .and_then(|record| record.text("target"))
                        .is_some_and(|target| target.starts_with("Elect Player"));
                    if elects_a_player && let Some(chosen) = self.committee_formation(&alias) {
                        self.emit(&format!("AGENDA_RESOLVED:{alias}:{chosen}"));
                        if is_law(self.content, &alias) {
                            self.state.enact_law(&alias, &chosen);
                            self.emit(&format!("LAW_ENACTED:{alias}:{chosen}"));
                        }
                        continue;
                    }
                    let mut window = VoteWindow::new(&self.state, &alias, choices);
                    window.open(&self.state, self.content, self.sources);
                    self.voting = Some((Box::new(window), queue));
                    return self.result(false, None);
                }
                // This agenda — and every Veto replacement it drew — elected nothing, so the
                // queue moves on to the next slot.
                Ok(None) => {}
                Err(error) => return self.result(false, Some(error)),
            }
        }
        self.voting = None;
        self.emit("AGENDA_PHASE_RESOLVED");
        self.result(false, None)
    }

    /// Reveal an agenda and follow the Veto replacement chain it triggers. Returns the agenda
    /// to vote on and its choices, or `None` when the reveal (and every replacement) elected
    /// nothing.
    ///
    /// The initial agenda is the one drawn from this phase's queue; each Veto played into a
    /// reveal window draws its replacement from the top of the agenda deck and discards the
    /// agenda it interrupted, and the chain continues from there. A Veto on a Veto is legal —
    /// an agenda revealed by a Veto is still "an agenda revealed" — so the chain is a loop,
    /// bounded by the finite deck.
    fn reveal_agenda(
        &mut self,
        initial_alias: &str,
    ) -> Result<Option<(String, Vec<String>)>, GameError> {
        let mut alias = initial_alias.to_owned();
        loop {
            // Cards scoped to "this agenda" hang off this counter, so it moves before the reveal
            // window opens and any of them can be played.
            self.state.agenda_seq = self.state.agenda_seq.saturating_add(1);
            self.emit(&format!("AGENDA_REVEALED:{alias}"));
            let choices = outcomes(&self.state, self.content, self.sources, &alias);
            if choices.is_empty() {
                self.emit(&format!("AGENDA_DISCARDED:{alias}"));
                return Ok(None);
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
            self.emit_typed("AGENDA_REVEALED", payload)?;

            // Veto, played into the window, discards this agenda and reveals the one it drew
            // from the top of the deck: continue the chain from the replacement.
            let Some(replacement) = self.state.agenda_veto_replacement.take() else {
                return Ok(Some((alias, choices)));
            };
            self.emit(&format!("AGENDA_DISCARDED:{alias}"));
            alias = replacement;
        }
    }

    /// Resolve one vote decision, applying the outcome when the vote closes.
    fn step_vote(&mut self) -> StepResult {
        let Some(choice) = self.legal_options() else {
            return self.close_vote();
        };
        // Field borrows, not `self`: the table answers while the position stays readable.
        let answer = match self.table.ask_seeing(
            &choice,
            &Observed::new(
                &self.state,
                self.content,
                self.sources,
                self.galaxy.as_ref(),
            ),
        ) {
            Ok(answer) => answer,
            Err(error) => return self.result(false, Some(error.into())),
        };
        let Some((mut window, queue)) = self.voting.take() else {
            unreachable!("a vote is open");
        };
        let voter = choice.player.clone();
        let outcome = window.resolve(&mut self.state, self.content, self.sources, answer);
        let complete = window.is_complete();
        self.voting = Some((window, queue));
        // "After you cast votes on an outcome of an agenda", and "after the speaker votes".
        let mut payload = BTreeMap::new();
        payload.insert(
            "player".to_owned(),
            serde_json::Value::String(voter.to_string()),
        );
        let _ = self.emit_typed("VOTES_CAST", payload);

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

    /// The seat the outcome names if it is a player, else the controller of the planet it
    /// names (the "you or a planet you control are elected" reading). An outcome that is
    /// neither — a law, "for", "against" — matches nobody.
    ///
    /// `close_vote` calls this after the outcome, the predictions and the law have been
    /// settled and the agenda's own effect has been resolved through `crate::agenda_effects`;
    /// an effect the corpus does not resolve is announced as `AGENDA_EFFECT_UNRESOLVED`
    /// rather than silently skipped.
    fn elected_seat_or_planet(&self, outcome: &str) -> Option<PlayerId> {
        let elected = PlayerId::new(outcome.to_owned());
        if self.state.player(&elected).is_some() {
            return Some(elected);
        }
        let planet = ti4_model::id::PlanetId::new(outcome.to_owned());
        self.state
            .board
            .values()
            .find_map(|board| board.planet_control.get(&planet).cloned())
    }

    /// Resolve an agenda that was not discarded, returning the elected player if there was one.
    ///
    /// The whole non-discarded path: the redirect, the prediction payouts, enacting a law, and the
    /// agenda's own effect. Split out of `close_vote` because it is the only branch that does
    /// anything -- the discard branch clears three fields -- and because the effect needs the
    /// game's own dice, table and map, which is a block of borrowing that reads better alone.
    fn resolve_agenda_outcome(
        &mut self,
        alias: &str,
        outcome: &str,
        ballot: &crate::vote::Ballot,
    ) -> Option<ti4_model::id::PlayerId> {
        // Confusing / Confounding Legal Text, played into the window above, redirect
        // who is the elected player. The vote's own result (`outcome`) still settles
        // predictions and any law; the agenda's effect on the elected player and the
        // "elected by an agenda" feat follow the redirect.
        let effective = if let Some(override_player) = self.state.agenda_elected_override.take() {
            self.emit(&format!(
                "AGENDA_OUTCOME_REDIRECTED:{outcome}:{override_player}"
            ));
            override_player.to_string()
        } else {
            outcome.to_owned()
        };

        // Drive the Debate: "you or a planet you control are elected". The outcome
        // names a player, a planet, or an outcome like "for" -- so both readings
        // have to be tried, and an outcome that is neither matches nobody. Read
        // from the (possibly redirected) elected player.
        let elected = self.elected_seat_or_planet(&effective);

        // Imperial Rider pays out before the agenda's own effect, and clears the
        // predictions. A prediction left behind would pay again on the next agenda,
        // for a card that was spent on this one.
        for player in crate::action_cards::resolve_predictions(&mut self.state, outcome) {
            self.emit(&format!("AGENDA_PREDICTION_CORRECT:{player}"));
        }

        // 8.20 first: an elected or "For" law stays in play, and an effect that reads
        // the laws must see this one already there. 8.21 discards everything else.
        if is_law(self.content, alias) && outcome != AGAINST {
            self.state.enact_law(alias, outcome);
            // Classified Document Leaks: "The elected secret objective becomes a public
            // objective." Only for its own enactment -- `outcome` here is whatever *this* agenda
            // elected, so calling it for every law would publish a player name or a planet as an
            // objective the moment Classified was anywhere in play.
            if alias == "classified" {
                crate::laws::classified_leak(&mut self.state, outcome);
            }
            self.emit(&format!("LAW_ENACTED:{alias}:{outcome}"));
        }

        // With the game's own dice, table and map: several agendas roll, ask, or read
        // the shape of the board, and one borrowed from nowhere would roll off a
        // stream no seed covers. The speaker's tie-break (8.18) is asked through the
        // same table.
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
            alias,
            &effective,
            ballot,
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
        elected
    }

    /// Committee Formation, offered before a vote that would elect a player.
    ///
    /// Returns the player the card's owner chose, having discarded it.
    fn committee_formation(&mut self, alias: &str) -> Option<String> {
        let owner = crate::laws::offer_discard(
            &mut self.state,
            &mut self.table,
            "committee",
            &format!("Committee Formation: discard to choose who {alias} elects"),
        )?;
        let options: Vec<crate::choice::ChoiceOption> = self
            .state
            .players
            .iter()
            .map(|seat| {
                crate::choice::ChoiceOption::labelled(
                    seat.id.to_string(),
                    "player",
                    format!("elect {}", seat.id),
                )
            })
            .collect();
        if options.is_empty() {
            return None;
        }
        let choice =
            crate::choice::Choice::new(owner, format!("{alias}: elect which player"), options);
        self.table.ask(&choice).ok().map(|answer| answer.id)
    }

    /// Minister of Peace, offered after an activation that met an enemy.
    fn minister_of_peace(&mut self, system: &SystemId, active: &PlayerId) -> bool {
        let contested = self
            .state
            .system_state(system)
            .units
            .iter()
            .any(|unit| unit.owner != *active);
        if !contested {
            return false;
        }
        // Its owner ending their own turn is legal and pointless; the card does not forbid it, and
        // a decider that asks for it gets it.
        crate::laws::offer_discard(
            &mut self.state,
            &mut self.table,
            "minister_peace",
            "Minister of Peace: discard to end the active player's turn",
        )
        .is_some()
    }

    /// Minister of War, offered after an action: a token back, and another action.
    ///
    /// Returns nothing -- the additional action is signalled through `ADDITIONAL_ACTION`, the same
    /// flag the action cards use, so one mechanism grants extra turns rather than two.
    fn minister_of_war(&mut self) {
        let placed: Vec<SystemId> = self
            .state
            .laws
            .get("minister_war")
            .map(|owner| PlayerId::new(owner.clone()))
            .map(|owner| {
                self.state
                    .board
                    .iter()
                    .filter(|(_, here)| here.command_tokens.contains(&owner))
                    .map(|(system, _)| system.clone())
                    .collect()
            })
            .unwrap_or_default();
        if placed.is_empty() {
            return; // 22.3: no token on the board means the card cannot do what it says
        }
        let Some(owner) = crate::laws::offer_discard(
            &mut self.state,
            &mut self.table,
            "minister_war",
            "Minister of War: discard to retrieve a command token and act again",
        ) else {
            return;
        };
        let options: Vec<crate::choice::ChoiceOption> = placed
            .iter()
            .map(|system| {
                crate::choice::ChoiceOption::labelled(
                    system.to_string(),
                    "system",
                    format!("retrieve your token from {system}"),
                )
            })
            .collect();
        let choice =
            crate::choice::Choice::new(owner.clone(), "which command token comes back", options);
        let Ok(answer) = self.table.ask(&choice) else {
            return;
        };
        let system = SystemId::new(answer.id);
        if let Some(here) = self.state.board.get_mut(&system) {
            here.command_tokens.remove(&owner);
        }
        self.state
            .gain_token(&owner, ti4_model::state::TokenPool::Tactic, 1);
        self.state
            .transient_flags
            .set(TransientFlags::ADDITIONAL_ACTION);
    }

    /// Imperial Arbiter, offered at the end of the strategy phase.
    ///
    /// > The owner of this card may discard this card to swap 1 of their strategy cards with 1 of
    /// > another player's.
    ///
    /// A swap, not a theft: both seats end up with a card, which is why this asks for one of each
    /// rather than picking a target and taking it.
    fn imperial_arbiter(&mut self) {
        let Some(owner) = self
            .state
            .laws
            .get("arbiter")
            .map(|held| PlayerId::new(held.clone()))
        else {
            return;
        };
        let mine: Vec<StrategyCardId> = self
            .state
            .player(&owner)
            .map(|seat| seat.strategy_cards.clone())
            .unwrap_or_default();
        let theirs: Vec<(PlayerId, StrategyCardId)> = self
            .state
            .players
            .iter()
            .filter(|seat| seat.id != owner)
            .flat_map(|seat| {
                seat.strategy_cards
                    .iter()
                    .map(move |card| (seat.id.clone(), card.clone()))
            })
            .collect();
        if mine.is_empty() || theirs.is_empty() {
            return; // 22.3: a swap needs a card on both sides
        }
        if crate::laws::offer_discard(
            &mut self.state,
            &mut self.table,
            "arbiter",
            "Imperial Arbiter: discard to swap a strategy card",
        )
        .is_none()
        {
            return;
        }

        let choice = crate::choice::Choice::new(
            owner.clone(),
            "give away which of your strategy cards",
            mine.iter()
                .map(|card| {
                    crate::choice::ChoiceOption::labelled(
                        card.to_string(),
                        "strategy_card",
                        format!("give {card}"),
                    )
                })
                .collect(),
        );
        let Ok(given) = self.table.ask(&choice) else {
            return;
        };
        let choice = crate::choice::Choice::new(
            owner.clone(),
            "and take which",
            theirs
                .iter()
                .map(|(holder, card)| {
                    crate::choice::ChoiceOption::labelled(
                        format!("{holder}|{card}"),
                        "strategy_card",
                        format!("take {card} from {holder}"),
                    )
                })
                .collect(),
        );
        let Ok(taken) = self.table.ask(&choice) else {
            return;
        };
        let Some((holder, card)) = theirs
            .iter()
            .find(|(holder, card)| taken.id == format!("{holder}|{card}"))
        else {
            return;
        };
        let given = StrategyCardId::new(given.id);
        if let Some(seat) = self.state.player_mut(&owner) {
            seat.strategy_cards.retain(|held| *held != given);
            seat.strategy_cards.push(card.clone());
        }
        if let Some(seat) = self.state.player_mut(holder) {
            seat.strategy_cards.retain(|held| held != card);
            seat.strategy_cards.push(given);
        }
        self.emit(&format!("STRATEGY_CARDS_SWAPPED:{owner}:{holder}"));
    }

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
            // The outcome names a player when the agenda elects one, which is what the "when
            // you are elected" windows read. `elected_player` is set only for a real seat, so
            // a window can tell "a player was elected" from an outcome that is a law, a planet,
            // or For/Against.
            let outcome_is_player = self.state.player(&PlayerId::new(outcome.clone())).is_some();
            let mut payload = BTreeMap::new();
            payload.insert(
                "agenda".to_owned(),
                serde_json::Value::String(alias.clone()),
            );
            payload.insert(
                "player".to_owned(),
                serde_json::Value::String(outcome.clone()),
            );
            if outcome_is_player {
                payload.insert(
                    "elected_player".to_owned(),
                    serde_json::Value::String(outcome.clone()),
                );
            }
            // Mirror the ballot for a Deadly Plot guard played into the window below: the
            // ballot itself lives in the vote window this function holds, and a guard can
            // only read the game state.
            self.state.agenda_votes = window.ballot().votes.clone();
            if let Err(error) = self.emit_typed("AGENDA_RESOLVED", payload) {
                return self.result(false, Some(error));
            }
            self.state.agenda_votes.clear();
            let discarded = self
                .state
                .transient_flags
                .has(TransientFlags::AGENDA_DISCARDED);
            self.state
                .transient_flags
                .clear(TransientFlags::AGENDA_DISCARDED);

            let elected_player: Option<ti4_model::id::PlayerId> = if discarded {
                // Deadly Plot: "discard the agenda instead. The agenda is resolved with no
                // effect and it is not replaced." The vote still happened, so the
                // occurrence window below still opens — but the agenda's own effect, the
                // prediction payouts, any law, and the elected feat are all suppressed. A
                // redirect played into the same window (Confusing / Confounding) is spent
                // on an election that has no effect, so it is dropped with it. (This
                // engine does not draw a replacement after a resolution, so "not
                // replaced" has nothing to suppress.)
                self.state.agenda_elected_override = None;
                self.state.agenda_predictions.clear();
                self.emit(&format!("AGENDA_DISCARDED:{alias}"));
                None
            } else {
                self.resolve_agenda_outcome(&alias, &outcome, window.ballot())
            };
            // Agenda-timed secrets are offered only after the complete agenda outcome is live:
            // Dictate Policy therefore sees a law enacted by this agenda. A Deadly Plot
            // discard suppresses the agenda's effect, not the fact that the vote resolved.
            let occurrence = self.state.begin_feat_occurrence();
            if let Some(elected_player) = elected_player {
                self.state
                    .record_event_feat(&elected_player, Feat::ElectedByAnAgenda, occurrence);
            }
            // Every resolved agenda is its own agenda-phase occurrence. Objectives such as
            // Dictate Policy depend on the completed outcome rather than on a player election,
            // so For/Against and other non-player outcomes must open this window too.
            self.open_occurrence_event_scoring(
                crate::secrets::Timing::Agenda,
                occurrence,
                EventScoreLimit::AnyPerPlayer,
            );
        }
        if self.event_scoring.is_some() {
            self.agenda_queue_after_event_scoring = Some(queue);
            self.result(false, None)
        } else {
            self.open_next_vote(queue)
        }
    }

    fn step_phase(&mut self) -> StepResult {
        if self.state.phase == Phase::Action && !self.state.all_passed() {
            return self.result(false, Some(GameError::MissingActivePlayer));
        }
        let outcome = advance_phase(&mut self.state);
        match outcome {
            PhaseOutcome::ActionBegan(_) => self.emit("ACTION_PHASE_BEGAN"),
            PhaseOutcome::StatusBegan => self.emit("STATUS_PHASE_BEGAN"),
            PhaseOutcome::AgendaBegan => {
                self.emit("AGENDA_PHASE_BEGAN");
                // Two cards read "at the start of the agenda phase".
                if let Err(error) = self.emit_typed("AGENDA_PHASE_BEGAN", BTreeMap::new()) {
                    return self.result(false, Some(error));
                }
            }
            PhaseOutcome::RoundEnded => {
                begin_next_round(&mut self.state, self.strategy_cards.clone());
                self.status_resolved = false;
                self.agenda_resolved = false;
                self.emit("ROUND_BEGAN");
                // A round begins with the strategy phase, and cards read "at the start of the
                // strategy phase" -- a window this engine had no moment for. `ROUND_BEGAN` is not
                // it: that is the round's transition, and a card reading the strategy phase must
                // not fire on a round that never reaches one.
                // Only `emit_typed`: `mirror_timing_log` copies what the resolver emitted into
                // `events`, so calling `emit` as well would log the phase twice and fire any card
                // reading it twice with it.
                // Anti-Intellectual Revolution (Against): "At the start of the next strategy
                // phase, each player chooses and exhausts 1 planet for each technology that they
                // own." Charged here, where the phase actually begins for rounds after the first.
                crate::laws::revolution_levy(&mut self.state);
                if let Err(error) = self.emit_typed("STRATEGY_PHASE_BEGAN", BTreeMap::new()) {
                    return self.result(false, Some(error));
                }
            }
        }
        self.result(false, None)
    }

    /// "After you perform an action" — one typed event for both kinds of action, since a
    /// turn's action is either strategic or tactical. The payload names the player who
    /// performed it, which is the "you" the window reads.
    ///
    /// # Errors
    /// [`GameError`] when the event id space is exhausted or a decider answers illegally.
    fn emit_action_completed(&mut self, player: &PlayerId) -> Result<(), GameError> {
        let mut payload = BTreeMap::new();
        payload.insert(
            "player".to_owned(),
            serde_json::Value::String(player.to_string()),
        );
        self.emit_typed("ACTION_COMPLETED", payload)?;
        Ok(())
    }

    /// The player's action is over: announce it, give Master Plan its window, pass the turn.
    fn finish_action(&mut self) -> Result<(), GameError> {
        if let Some(active) = self.state.active.clone() {
            self.emit_action_completed(&active)?;
        }
        self.advance_turn()
    }

    /// Pass the turn on, unless the current player keeps it.
    ///
    /// Master Plan's "perform an additional action" is a continuation of the *same* turn, so
    /// the retention short-circuits before anything that begins a new one: no `turn_seq`
    /// increment, no `technology::end_turn` for the player, no transaction reset (the Fleet
    /// Logistics reading in `phase.rs`).
    ///
    /// `TURN_PASSED` is the "at the end of any player's turn" moment, typed so a window can
    /// hang off it: the payload names the player whose turn ended, and the turn has already
    /// moved when it fires, which is what Crisis's "skip the next player's turn" needs — the
    /// skip happens here, still inside the advance, on the seat the turn just arrived at. A
    /// skipped turn is not a turn, so it emits `TURN_SKIPPED` and opens no end-of-turn
    /// window of its own (a chain of Crisis cards on skipped turns is not a reading the card
    /// supports).
    ///
    /// # Errors
    /// [`GameError`] when the `TURN_PASSED` window's deciders answer illegally.
    fn advance_turn(&mut self) -> Result<(), GameError> {
        // A new turn re-offers everything: the withholding below is scoped to the turn whose
        // action failed, not to the game.
        self.failed_component_actions.clear();
        if self
            .state
            .transient_flags
            .has(TransientFlags::ADDITIONAL_ACTION)
        {
            self.state
                .transient_flags
                .clear(TransientFlags::ADDITIONAL_ACTION);
            self.emit("TURN_RETAINED");
            return Ok(());
        }
        // Minister of War: "The owner of this card may discard this card after performing an
        // action to remove 1 of their command tokens from the game board and return it to their
        // reinforcements -- then they may perform 1 additional action." Offered before the turn
        // moves on, since the additional action is this player's.
        if self.state.phase == Phase::Action {
            self.minister_of_war();
            if self
                .state
                .transient_flags
                .has(TransientFlags::ADDITIONAL_ACTION)
            {
                self.state
                    .transient_flags
                    .clear(TransientFlags::ADDITIONAL_ACTION);
                self.emit("TURN_RETAINED");
                return Ok(());
            }
        }
        let ended = self.state.active.clone();
        // A Black Market marker outlives no turn: the negotiation it unlocked is closed by
        // the time the turn ends, and the marker is cleared here too so a flag no window
        // consumed can never leak into another player's table.
        self.state
            .transient_flags
            .clear(TransientFlags::BLACK_MARKET);
        if self.state.phase == Phase::Action
            && let Some(active) = ended.clone()
        {
            let _ = crate::technology::end_turn(
                &mut self.state,
                self.content,
                self.sources,
                self.galaxy.as_ref(),
                &mut self.table,
                &active,
            );
        }
        if advance_turn(&mut self.state).is_none()
            && !self
                .state
                .transient_flags
                .has(TransientFlags::PUPPET_ACTION)
        {
            return Ok(());
        }
        let Some(ended) = ended else {
            return Ok(());
        };
        self.emit("TURN_PASSED");
        let mut payload = BTreeMap::new();
        payload.insert(
            "player".to_owned(),
            serde_json::Value::String(ended.to_string()),
        );
        self.emit_typed("TURN_PASSED", payload)?;
        if self
            .state
            .transient_flags
            .has(TransientFlags::SKIP_NEXT_TURN)
        {
            self.state
                .transient_flags
                .clear(TransientFlags::SKIP_NEXT_TURN);
            if let Some(skipped) = self.state.active.clone() {
                self.emit(&format!("TURN_SKIPPED:{skipped}"));
                let _ = advance_turn(&mut self.state);
            }
        }
        // Puppets on a String: the passer's card gives the turn straight back — a fresh
        // turn (new `turn_seq`, start-of-turn hooks, `TURN_BEGAN` window) rather than a
        // continuation of the old one — and the seat stays passed: the grant is one
        // action, not a return from pass. The next step re-runs turn preparation, since
        // `prepared_turn_seq` no longer matches.
        if self
            .state
            .transient_flags
            .has(TransientFlags::PUPPET_ACTION)
        {
            self.state
                .transient_flags
                .clear(TransientFlags::PUPPET_ACTION);
            crate::phase::begin_action_turn(&mut self.state, &ended);
            self.emit(&format!("TURN_PUPPET:{ended}"));
        }
        Ok(())
    }

    fn take_timed_strategy_card(
        &mut self,
        player: &PlayerId,
        answer: ChoiceOption,
    ) -> Result<PlayerId, GameError> {
        self.sync_timing_context();
        let card = StrategyCardId::new(answer.id.clone());
        // The card's payload names the picker and the card, but the payload is consumed by
        // the timing machinery and invisible to effects: the slot is the handoff Public
        // Disgrace reads back in the After window that follows the emission.
        self.state.last_strategy_choice = Some((player.clone(), card.clone()));
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

    /// A component action that fails is not offered again on the same turn.
    ///
    /// 22.4 says a *cancelled* component action does not consume the turn, so the driver re-offers
    /// it. A component action that merely **failed** is different: nothing about the position
    /// changed, so it fails identically, and a decider that keeps choosing it never advances. One
    /// MLP self-play game asked "action phase" 9,789 consecutive times before the step limit caught
    /// it -- a livelock reachable by any decider that prefers the same option twice, which a
    /// low-temperature policy does by construction.
    ///
    /// The turn is still not consumed, which is the half 22.4 asks for; only the failing option
    /// goes away, so the offer shrinks and progress is unavoidable.
    #[test]
    fn a_failed_component_action_is_withheld_for_the_rest_of_the_turn() {
        let players = [PlayerId::new("a"), PlayerId::new("b")];
        let state = start_game(ContentStore::embedded(), &players, POK, None).unwrap();
        let mut game = Game::new(state, ContentStore::embedded());
        game.state.phase = Phase::Action;
        game.state.active = Some(PlayerId::new("a"));

        let offered = |game: &Game<'_>| -> Vec<String> {
            game.action_options()
                .map(|choice| choice.options.iter().map(|o| o.id.clone()).collect())
                .unwrap_or_default()
        };

        let before = offered(&game);
        assert!(!before.is_empty(), "the action phase offers something");
        let victim = before[0].clone();

        // Mark it failed, as every one of the four component-action paths now does when its
        // handler returns `false`.
        game.failed_component_actions.insert(victim.clone());

        let after = offered(&game);
        assert!(
            !after.contains(&victim),
            "the failed action is withheld: {after:?}"
        );
        assert_eq!(
            after.len(),
            before.len() - 1,
            "and only that one -- every other action, including passing, survives"
        );

        // A new turn restores it: the withholding is scoped to the turn that failed.
        game.advance_turn().expect("the turn advances");
        game.state.active = Some(PlayerId::new("a"));
        assert!(
            offered(&game).contains(&victim),
            "the next turn offers it again"
        );
    }
    use std::sync::{Arc, Mutex};

    use ti4_content::ContentStore;
    use ti4_model::content_types::POK;
    use ti4_model::id::{PlanetId, PlayerId};
    use ti4_model::state::Phase;

    use super::*;
    use crate::choice::{AlwaysDecline, Decider, Scripted};
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
        // The step also carries the one-time announcement of round one's strategy
        // phase, which start_game opens the game already inside.
        assert_eq!(
            game.events,
            vec!["STRATEGY_PHASE_BEGAN", "STRATEGY_CARD_CHOSEN"]
        );
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
        // Only the one-time round-1 strategy phase announcement reached the log; the
        // cancelled draft choice produced nothing.
        assert_eq!(game.events, vec!["STRATEGY_PHASE_BEGAN"]);
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
        let tokens_before_primary = game.state.player(&primary).unwrap().total_tokens();

        // Only follower "b" can afford the 52.3 influence purchase; "c" must be skipped with no prompt.
        game.state
            .player_mut(&PlayerId::new("b"))
            .unwrap()
            .trade_goods = 3;

        let primary_step = game.step();
        assert!(primary_step.resolved_choice);
        assert_eq!(game.state.active, Some(primary.clone()));
        assert_eq!(
            game.state.player(&primary).unwrap().total_tokens(),
            tokens_before_primary + 3,
            "the driven strategic action invokes Leadership's primary"
        );
        assert_eq!(
            game.table.log.len(),
            10,
            "six draft picks, the strategic action, and Leadership's three pool choices"
        );
        let before_inspection = game.state.clone();
        // Oracle identity: the window's question is the influence-purchase question itself.
        let pending = game.legal_options().unwrap();
        assert_eq!(pending.player, PlayerId::new("b"));
        assert_eq!(pending.prompt, "spend 3 influence for a command token");
        assert_eq!(pending.ids(), vec!["no", "yes"]);
        assert!(game.state.identical(&before_inspection));

        let first_secondary = game.step();
        assert!(first_secondary.resolved_choice);
        assert_eq!(game.state.active, Some(primary.clone()));

        // "c" cannot pay three influence: it is skipped automatically and the window completes.
        let final_secondary = game.step();
        assert!(!final_secondary.resolved_choice);
        assert_ne!(game.state.active, Some(primary));
        assert_eq!(game.table.log.len(), 11);
        assert!(game.events.contains(&"STRATEGIC_ACTION_BEGAN".to_owned()));
        assert_eq!(
            game.events
                .iter()
                .filter(|event| event.as_str() == "STRATEGY_SECONDARY_DECLINED")
                .count(),
            1,
            "only the affordable follower was asked; the other was ineligible"
        );
    }

    #[test]
    fn leadership_follower_yes_pays_through_the_payment_loop() {
        // Oracle `_leadership_secondary`: the window's `yes` is followed by a payment (silent —
        // lone options auto-pick) and one pool choice, recorded for the follower; an
        // unaffordable follower makes no decision at all.
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

        // Only b can afford the influence purchase (setup grants tokens, not influence).
        let b_before = game.state.player(&PlayerId::new("b")).unwrap().clone();
        game.state
            .player_mut(&PlayerId::new("b"))
            .unwrap()
            .trade_goods = 3;
        // A trade good covers one influence at a time; with goods as the only asset every payment
        // step is a lone option, which oracle pay() takes without asking (P1-g f5).
        game.table.seat(
            PlayerId::new("b"),
            Box::new(Scripted::new(["yes", "fleet_tokens"])),
        );

        assert!(game.step().resolved_choice); // a's Leadership primary (no purchase: unaffordable)
        let follow = game.step(); // b answers the window question
        assert!(follow.resolved_choice);

        let tail = &game.table.log.records[10..];
        // window yes, the payment settles silently (lone options auto-pick), then the pool.
        assert_eq!(tail.len(), 2);
        for record in tail {
            assert_eq!(record.player, PlayerId::new("b"));
        }
        assert_eq!(tail[0].prompt, "spend 3 influence for a command token");
        assert_eq!(tail[0].offered, vec!["no", "yes"]);
        assert_eq!(tail[0].chosen, "yes");
        assert_eq!(tail[1].prompt, "gain a command token into which pool");

        let b = game.state.player(&PlayerId::new("b")).unwrap();
        assert_eq!(b.fleet_tokens, b_before.fleet_tokens + 1);
        assert_eq!(b.tactic_tokens, b_before.tactic_tokens);
        assert_eq!(b.strategic_tokens, b_before.strategic_tokens);
        assert_eq!(b.trade_goods, 0);

        // c was unaffordable: no decision for it, and the window completes by itself.
        let finish = game.step();
        assert!(!finish.resolved_choice);
        assert_eq!(game.state.active, Some(PlayerId::new("b")));
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

        // 81.5 has two sentences and this test is about the first: the gain goes into the pool
        // the player chose. The second sentence -- redistribution -- then moves tokens between
        // pools, which is why the per-pool assertion became a total. Redistribution conserves the
        // total by construction, so this still pins that the tokens were gained at all and that
        // nothing minted or lost one on the way.
        let after = game.state.player(&PlayerId::new("a")).unwrap();
        let total = |seat: &ti4_model::state::Player| {
            seat.tactic_tokens + seat.fleet_tokens + seat.strategic_tokens
        };
        assert_eq!(
            total(after),
            total(&before) + i32::try_from(STATUS_TOKENS).unwrap(),
            "two tokens gained, and redistribution moved rather than minted"
        );
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

    /// A tactical fixture whose combat actually fights: ids[0] is the hub of the seven-system
    /// galaxy, adjacent to every other one, so a's dummy fighter in each of them leaves the
    /// defender (b) with nowhere to retreat to — the defender is never asked to announce —
    /// while the attacker (a) can always retreat and is asked every round. Cards are granted
    /// straight into the hands, skipping the draft.
    fn combat_fixture(a_cards: &[&str], b_cards: &[&str]) -> (GameState, Galaxy, Vec<SystemId>) {
        let (mut state, galaxy, ids) = tactical_fixture();
        let a = PlayerId::new("a");
        let b = PlayerId::new("b");
        for id in ids.iter().skip(1) {
            crate::fixtures::put(&mut state, id, "fighter", &a, 1);
        }
        state.player_mut(&a).unwrap().action_cards = a_cards
            .iter()
            .map(|alias| ti4_model::id::ActionCardId::new(*alias))
            .collect();
        state.player_mut(&b).unwrap().action_cards = b_cards
            .iter()
            .map(|alias| ti4_model::id::ActionCardId::new(*alias))
            .collect();
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
    fn arriving_on_a_frontier_token_without_det_does_not_explore_it() {
        // LRR 35: a frontier token is explored only by a player who owns the Dark Energy Tap
        // technology "or if another game effect allows them to". Arriving with a ship is
        // neither, so the token must survive the move. Before the fix `note_arrival` explored
        // it on every `MoveOutcome::Arrived`, tripping a draw for any fleet that drifted in.
        let (mut state, galaxy, ids) = tactical_fixture();
        // A ship with capacity, plus a loadable passenger: the move then takes the cargo
        // path, whose sailing is where arrivals are announced (the bare-move shortcut sails
        // inside the offer step and skips the arrival note entirely).
        crate::fixtures::put(&mut state, &ids[1], "carrier", &PlayerId::new("a"), 1);
        crate::fixtures::put(&mut state, &ids[1], "infantry", &PlayerId::new("a"), 1);
        let table = Table::with_default(Box::new(Scripted::new([
            TACTICAL_ACTION_ID.to_owned(),
            ids[0].to_string(),
            format!("move|{}|0", ids[1]),
            "done_loading".to_owned(),
            "done_moving".to_owned(),
        ])));
        let mut game = Game::with_table(state, ContentStore::embedded(), table).with_galaxy(galaxy);
        game.state.frontier_tokens.insert(ids[0].clone());

        for _ in 0..40 {
            let result = game.step();
            assert_eq!(result.error, None, "no tactical step should refuse");
            if game.events.iter().any(|e| e == "TACTICAL_ACTION_COMPLETE") {
                break;
            }
        }

        assert!(
            game.events.iter().any(|e| e == "TACTICAL_ACTION_COMPLETE"),
            "the action ran to completion"
        );
        assert!(
            !game.events.iter().any(|e| e == "FRONTIER_EXPLORED"),
            "no Dark Energy Tap and no other effect: the token stays"
        );
        assert!(
            game.state.frontier_tokens.contains(&ids[0]),
            "the arrival did not consume the frontier token"
        );
    }

    #[test]
    fn a_det_owner_explores_the_frontier_token_when_their_tactical_action_ends() {
        // DET's printed trigger: "After you perform a tactical action in a system that
        // contains a frontier token, if you have 1 or more ships in that system, explore
        // that token." The trigger is the tactical action ending, not any move, so a fleet
        // already sitting on the token explores when its owner performs an action there and
        // simply finishes moving.
        let (mut state, galaxy, ids) = tactical_fixture();
        state
            .player_mut(&PlayerId::new("a"))
            .unwrap()
            .technologies
            .insert(ti4_model::id::TechnologyId::new("det"));
        crate::fixtures::put(&mut state, &ids[0], "carrier", &PlayerId::new("a"), 1);
        crate::fixtures::put(&mut state, &ids[0], "infantry", &PlayerId::new("a"), 1);
        let table = Table::with_default(Box::new(Scripted::new([
            TACTICAL_ACTION_ID.to_owned(),
            ids[0].to_string(),
            "done_moving".to_owned(),
        ])));
        let mut game = Game::with_table(state, ContentStore::embedded(), table).with_galaxy(galaxy);
        game.state.frontier_tokens.insert(ids[0].clone());

        for _ in 0..40 {
            let result = game.step();
            assert_eq!(result.error, None, "no tactical step should refuse");
            if game.events.iter().any(|e| e == "TACTICAL_ACTION_COMPLETE") {
                break;
            }
        }

        assert!(
            game.events.iter().any(|e| e == "TACTICAL_ACTION_COMPLETE"),
            "the action ran to completion"
        );
        assert!(
            game.events.iter().any(|e| e == "FRONTIER_EXPLORED"),
            "the DET trigger fired when the tactical action ended"
        );
        assert!(
            !game.state.frontier_tokens.contains(&ids[0]),
            "exploring consumed the token"
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

    /// `tactical_fixture` plus a landable, non-station planet of the activated system, so an
    /// invasion test has somewhere for ground forces to go.
    fn invasion_fixture() -> (
        GameState,
        ti4_content::galaxy::Galaxy,
        ti4_model::id::SystemId,
        ti4_model::id::PlanetId,
    ) {
        let (state, galaxy, ids) = tactical_fixture();
        let content = ContentStore::embedded();
        let (system, planet) = ids
            .iter()
            .find_map(|system| {
                let record = content
                    .get(
                        ti4_model::content_types::ContentType::Systems,
                        system.as_str(),
                    )
                    .expect("the fixture system is in the corpus");
                record
                    .strings("planets")
                    .into_iter()
                    .find(|name| !ti4_content::galaxy::is_space_station(content, name, POK))
                    .map(|name| (system.clone(), ti4_model::id::PlanetId::new(name)))
            })
            .expect("the fixture has a system with a non-station planet");
        (state, galaxy, system, planet)
    }

    #[test]
    fn blitz_grants_bombardment_to_the_invaders_non_bombarding_ships() {
        // Blitz: "Each of your non-fighter ships in the active system that do not have
        // BOMBARDMENT gain BOMBARDMENT 6 until the end of the invasion." The invader's
        // destroyer has no BOMBARDMENT of its own; the dreadnought hits on 5. In the
        // invader's roll the destroyer's granted die (a 6) destroys one of the defender's
        // infantry and the dreadnought's 3 destroys nothing. Without the card only the
        // dreadnought can bombard at all — its 3 is a miss — and both infantry survive.
        let run = |with_card: bool| -> (
            GameState,
            Dice,
            ti4_model::id::SystemId,
            ti4_model::id::PlanetId,
        ) {
            let (mut state, galaxy, system, planet) = invasion_fixture();
            let a = PlayerId::new("a");
            let b = PlayerId::new("b");
            // The invader's ships stand in the system to be activated, so the movement step
            // has nothing to do; the defender's two infantry take a hit in placed order.
            crate::fixtures::put(&mut state, &system, "destroyer", &a, 1);
            crate::fixtures::put(&mut state, &system, "dreadnought", &a, 1);
            state
                .system_mut(&system)
                .set_control(planet.clone(), b.clone());
            crate::fixtures::put_on_planet(&mut state, &system, &planet, "infantry", &b, 2);
            if with_card {
                state.player_mut(&a).unwrap().action_cards =
                    vec![ti4_model::id::ActionCardId::new("blitz")];
            }
            // Preload feeds the rolls in order: the destroyer's granted roll first, then the
            // dreadnought's own.
            let preload: Vec<u32> = if with_card { vec![6, 3] } else { vec![3] };
            let script: Vec<String> = if with_card {
                vec![
                    TACTICAL_ACTION_ID.to_owned(),
                    system.to_string(),
                    "done_moving".to_owned(),
                    "reaction:generic:INVASION_BEGAN:after".to_owned(),
                ]
            } else {
                vec![
                    TACTICAL_ACTION_ID.to_owned(),
                    system.to_string(),
                    "done_moving".to_owned(),
                ]
            };
            let table = Table::with_default(Box::new(Scripted::new(script)));
            let mut game =
                Game::with_table(state, ContentStore::embedded(), table).with_galaxy(galaxy);
            game.dice = Dice::from_faces(preload);
            for _ in 0..16 {
                let result = game.step();
                assert_eq!(result.error, None, "no tactical step should refuse");
                if game.events.iter().any(|e| e == "TACTICAL_ACTION_COMPLETE") {
                    break;
                }
            }
            assert!(
                game.events.iter().any(|e| e == "TACTICAL_ACTION_COMPLETE"),
                "the action ran every step it has and closed; log was {:?}",
                game.events
            );
            (game.state, game.dice, system, planet)
        };

        // No card: the shield-less planet is bombarded by the dreadnought alone, its 3
        // misses, and the defender keeps both infantry.
        let (state, dice, system, planet) = run(false);
        let on_planet = state.system_state(&system).on_planet(&planet).to_vec();
        assert_eq!(
            on_planet.len(),
            2,
            "the 3 destroys nothing, so both infantry survive; log was {:?}",
            dice.rolled("bombardment")
                .iter()
                .map(|roll| format!("{} {:?}", roll.reason, roll.faces))
                .collect::<Vec<_>>()
        );
        assert!(
            dice.rolled("bombardment")
                .iter()
                .all(|roll| roll.faces == [3]),
            "only the dreadnought has BOMBARDMENT without the card"
        );
        assert!(
            state
                .player(&PlayerId::new("a"))
                .unwrap()
                .blitz_invasion
                .is_empty(),
            "no card, no marker"
        );

        // With Blitz: the destroyer rolls a granted die that hits on 6 (a 6) and takes one
        // infantry; the dreadnought's 3 still misses. The card is spent and the marker is
        // keyed to the activation that owns the invasion.
        let (state, dice, _, _) = run(true);
        let on_planet = state.system_state(&system).on_planet(&planet).to_vec();
        assert_eq!(
            on_planet.len(),
            1,
            "the destroyer's granted 6 destroyed one infantry; log was {:?}",
            dice.rolled("bombardment")
                .iter()
                .map(|roll| format!("{} {:?}", roll.reason, roll.faces))
                .collect::<Vec<_>>()
        );
        assert_eq!(
            dice.rolled("bombardment").len(),
            2,
            "the destroyer's granted roll ran beside the dreadnought's own"
        );
        let seat = state.player(&PlayerId::new("a")).unwrap();
        assert_eq!(
            seat.blitz_invasion.len(),
            1,
            "the marker fired once, for this invasion"
        );
        assert!(seat.action_cards.is_empty(), "the card was spent");
    }

    #[test]
    fn disable_strips_the_opponents_pds_effects_for_the_invasion() {
        // Disable: "Your opponents' PDS units lose PLANETARY SHIELD and SPACE CANNON during
        // this invasion." The defender's PDS shields the planet, so without the card the
        // invader's dreadnought cannot bombard at all. The PDS's own cannon fires first, but
        // its preloaded 3 is below its hit value, so the invader's ships survive either way.
        // With the card the shield is stripped: the dreadnought's 5 destroys one of the
        // defender's infantry (placed before the PDS, so it takes the hit in placed order),
        // and the PDS itself stands on the planet.
        let run = |with_card: bool| -> (
            GameState,
            Dice,
            ti4_model::id::SystemId,
            ti4_model::id::PlanetId,
        ) {
            let (mut state, galaxy, system, planet) = invasion_fixture();
            let a = PlayerId::new("a");
            let b = PlayerId::new("b");
            crate::fixtures::put(&mut state, &system, "destroyer", &a, 1);
            crate::fixtures::put(&mut state, &system, "dreadnought", &a, 1);
            state
                .system_mut(&system)
                .set_control(planet.clone(), b.clone());
            crate::fixtures::put_on_planet(&mut state, &system, &planet, "infantry", &b, 2);
            crate::fixtures::put_on_planet(&mut state, &system, &planet, "pds", &b, 1);
            if with_card {
                state.player_mut(&a).unwrap().action_cards =
                    vec![ti4_model::id::ActionCardId::new("disable")];
            }
            // The cannon's roll first (a 3, below its 6), then — only with the card — the
            // dreadnought's bombardment (a 5, a hit against the shield-less planet).
            let preload: Vec<u32> = if with_card { vec![3, 5] } else { vec![3] };
            let script: Vec<String> = if with_card {
                vec![
                    TACTICAL_ACTION_ID.to_owned(),
                    system.to_string(),
                    "done_moving".to_owned(),
                    "reaction:generic:INVASION_BEGAN:after".to_owned(),
                ]
            } else {
                vec![
                    TACTICAL_ACTION_ID.to_owned(),
                    system.to_string(),
                    "done_moving".to_owned(),
                ]
            };
            let table = Table::with_default(Box::new(Scripted::new(script)));
            let mut game =
                Game::with_table(state, ContentStore::embedded(), table).with_galaxy(galaxy);
            game.dice = Dice::from_faces(preload);
            for _ in 0..16 {
                let result = game.step();
                assert_eq!(result.error, None, "no tactical step should refuse");
                if game.events.iter().any(|e| e == "TACTICAL_ACTION_COMPLETE") {
                    break;
                }
            }
            assert!(
                game.events.iter().any(|e| e == "TACTICAL_ACTION_COMPLETE"),
                "the action ran every step it has and closed; log was {:?}",
                game.events
            );
            (game.state, game.dice, system, planet)
        };

        // No card: the PDS shields the planet, so no bombardment roll is ever made and the
        // defender keeps everything on it.
        let (state, dice, system, planet) = run(false);
        assert!(
            dice.rolled("bombardment").is_empty(),
            "the planetary shield blocks bombardment entirely"
        );
        let on_planet = state.system_state(&system).on_planet(&planet).to_vec();
        assert_eq!(on_planet.len(), 3, "2 infantry and the PDS all stand");
        assert_eq!(
            dice.rolled("space cannon").len(),
            1,
            "the PDS's own cannon still fired, harmlessly"
        );

        // With Disable: the shield is gone for this invasion, so the dreadnought's 5 takes
        // one infantry; the PDS remains, the card is spent, and the marker is keyed to the
        // activation that owns the invasion.
        let (state, dice, _, _) = run(true);
        assert_eq!(
            dice.rolled("bombardment").len(),
            1,
            "with the shield stripped, the dreadnought's roll went off"
        );
        let on_planet = state.system_state(&system).on_planet(&planet).to_vec();
        assert_eq!(
            on_planet.len(),
            2,
            "one infantry fell to the 5; the PDS itself is not a bombardment victim here"
        );
        let still_shielded = state
            .system_state(&system)
            .on_planet(&planet)
            .iter()
            .filter(|unit| unit.type_id.as_str() == "pds")
            .count();
        assert_eq!(
            still_shielded, 1,
            "the PDS structure survived the bombardment"
        );
        let seat = state.player(&PlayerId::new("a")).unwrap();
        assert_eq!(seat.disable_invasion.len(), 1, "the marker fired once");
        assert!(seat.action_cards.is_empty(), "the card was spent");
    }

    #[test]
    fn sabotage_cancels_the_card_being_played() {
        // Sabotage: "When another player plays an action card other than 'Sabotage': cancel
        // that action card." A plays Flank Speed in the after window of his own activation,
        // which announces an ACTION_CARD_PLAYED event. B's Sabotage hooks that event's WHEN
        // window and cancels it: A's card is still spent (1.15 cancels the effect, not the
        // spend), but its marker never lands. Without the card the announcement is not
        // interrupted and the marker is set.
        let a = PlayerId::new("a");
        let b = PlayerId::new("b");
        let run = |with_card: bool| -> (GameState, Vec<String>) {
            let (mut state, galaxy, ids) = tactical_fixture();
            state.player_mut(&a).unwrap().action_cards =
                vec![ti4_model::id::ActionCardId::new("fs1")];
            if with_card {
                state.player_mut(&b).unwrap().action_cards =
                    vec![ti4_model::id::ActionCardId::new("sabo1")];
            }
            // A: start the action, activate, answer his own after window (play Flank
            // Speed). With the card, B then answers the Sabotage window that A's card
            // announcement opens, and A finishes the empty movement step.
            let script: Vec<String> = if with_card {
                vec![
                    TACTICAL_ACTION_ID.to_owned(),
                    ids[0].to_string(),
                    "reaction:generic:SYSTEM_ACTIVATED:after".to_owned(),
                    "reaction:generic:ACTION_CARD_PLAYED:when".to_owned(),
                    "done_moving".to_owned(),
                ]
            } else {
                vec![
                    TACTICAL_ACTION_ID.to_owned(),
                    ids[0].to_string(),
                    "reaction:generic:SYSTEM_ACTIVATED:after".to_owned(),
                    "done_moving".to_owned(),
                ]
            };
            let table = Table::with_default(Box::new(Scripted::new(script)));
            let mut game =
                Game::with_table(state, ContentStore::embedded(), table).with_galaxy(galaxy);
            for _ in 0..16 {
                let result = game.step();
                assert_eq!(result.error, None, "no tactical step should refuse");
                if game.events.iter().any(|e| e == "TACTICAL_ACTION_COMPLETE") {
                    break;
                }
            }
            assert!(
                game.events.iter().any(|e| e == "TACTICAL_ACTION_COMPLETE"),
                "the action ran every step it has and closed; log was {:?}",
                game.events
            );
            (game.state, game.events)
        };

        // No Sabotage: the announcement stands and Flank Speed sets its marker for the
        // tactical action in flight.
        let (state, _events) = run(false);
        let seat = state.player(&a).unwrap();
        assert_eq!(
            seat.move_bonus_activation,
            Some(state.activation_seq),
            "an uninterrupted play applies its effect"
        );
        assert!(seat.action_cards.is_empty(), "the card was spent");

        // With Sabotage: the play's effect never happens. The card is spent anyway, and so
        // is the Sabotage that answered it; the play itself happened and is logged, and a
        // cancelled card is not a registry gap.
        let (state, events) = run(true);
        let seat = state.player(&a).unwrap();
        assert_eq!(
            seat.move_bonus_activation, None,
            "a cancelled play applies no effect"
        );
        assert!(
            seat.action_cards.is_empty(),
            "the played card is still spent"
        );
        assert!(
            state.player(&b).unwrap().action_cards.is_empty(),
            "the Sabotage that answered it is spent too"
        );
        assert!(
            events.iter().any(|e| e == "ACTION_CARD_PLAYED"),
            "the play happened and was logged"
        );
        assert!(
            !events.iter().any(|e| e == "ACTION_CARD_UNRESOLVED"),
            "a cancelled card is not an unimplemented one"
        );
    }

    #[test]
    fn solar_flare_keeps_the_opponents_space_cannon_dark_for_the_action() {
        // Solar Flare: "During the 'Movement' step of this tactical action, other players
        // cannot use SPACE CANNON against your ships." A's cruiser sits in the system he
        // activates, and B's PDS is the gun that would shoot it in the action's cannon step.
        // With the flare the step never happens — no roll, no hit, no announcement; without
        // it the PDS fires. The card's marker is activation-scoped, so it cannot keep the
        // gun dark in a later action.
        let a = PlayerId::new("a");
        let b = PlayerId::new("b");
        let run = |with_card: bool| -> (GameState, Vec<String>, Vec<String>, bool) {
            let (mut state, galaxy, ids) = tactical_fixture();
            crate::fixtures::put(&mut state, &ids[0], "cruiser", &a, 1);
            crate::fixtures::put(&mut state, &ids[0], "pds", &b, 1);
            if with_card {
                state.player_mut(&a).unwrap().action_cards =
                    vec![ti4_model::id::ActionCardId::new("solar_flare")];
            }
            // A: start the action, activate, play the flare in his own after window (with the
            // card), then declare the empty movement step done.
            let mut script = vec![TACTICAL_ACTION_ID.to_owned(), ids[0].to_string()];
            if with_card {
                script.push("reaction:generic:SYSTEM_ACTIVATED:after".to_owned());
            }
            script.push("done_moving".to_owned());
            let table = Table::with_default(Box::new(Scripted::new(script)));
            let mut game =
                Game::with_table(state, ContentStore::embedded(), table).with_galaxy(galaxy);
            // PDS I hits on 6: pin the gun's single die so the control arm's shot is a hit.
            game.dice = Dice::from_faces([6]);

            for _ in 0..40 {
                let result = game.step();
                assert_eq!(
                    result.error, None,
                    "no tactical step should refuse; log was {:?}",
                    game.events
                );
                if game.events.iter().any(|e| e == "TACTICAL_ACTION_COMPLETE") {
                    break;
                }
            }
            assert!(
                game.events.iter().any(|e| e == "TACTICAL_ACTION_COMPLETE"),
                "the action ran every step it has and closed; log was {:?}",
                game.events
            );
            (
                game.state.clone(),
                game.events.clone(),
                game.dice
                    .history()
                    .iter()
                    .map(|roll| roll.reason.clone())
                    .collect(),
                game.state
                    .system_state(&ids[0])
                    .units
                    .iter()
                    .any(|unit| unit.owner == a && unit.type_id.as_str() == "cruiser"),
            )
        };

        // Without the flare the PDS fires and its hit is announced.
        let (state, events, rolls, _cruiser) = run(false);
        assert!(
            rolls.iter().any(|reason| reason == "space cannon"),
            "the PDS rolled its gun: {rolls:?}"
        );
        assert!(
            events.iter().any(|e| e == "SPACE_CANNON_HITS"),
            "the shot was announced"
        );
        assert!(
            state.player(&a).unwrap().solar_flare.is_empty(),
            "no card, no marker"
        );

        // With the flare the gun never fires during the action: no roll, no announcement,
        // and the cruiser that the shot was aimed at is still there when the action ends.
        let (state, events, rolls, cruiser) = run(true);
        assert!(
            !rolls.iter().any(|reason| reason == "space cannon"),
            "the flare keeps every opponent gun dark: {rolls:?}"
        );
        assert!(
            !events.iter().any(|e| e == "SPACE_CANNON_HITS"),
            "no shot, no announcement"
        );
        let seat = state.player(&a).unwrap();
        assert_eq!(
            seat.solar_flare,
            vec![state.activation_seq],
            "the marker scopes the card to the action it was played in"
        );
        assert!(seat.action_cards.is_empty(), "the card was spent");
        assert!(cruiser, "the aimed-at ship survived the dark cannon step");
    }

    #[test]
    fn lost_star_points_the_map_at_the_chart_for_the_players_action() {
        // Lost Star Chart: "During this tactical action, systems that contain alpha and beta
        // wormholes are adjacent to each other." The adjacency is a switch on the map that
        // `laws::apply_to_galaxy` re-derives every step from the active player's marker, so
        // this test drives the real tactical action and pins the wiring: the map points at
        // the chart while the chart-holder's action is in flight, and at nothing else.
        //
        // On this map 82b Mallice - Nexus is the only system carrying both wormholes, so the
        // card changes no actual adjacency in a base game — a single system has no partner.
        // The switch is still implemented as printed, and the link rule itself is pinned by
        // the galaxy's own tests (`the_star_chart_rule_links_the_both_wormhole_systems`).
        let a = PlayerId::new("a");
        let run = |with_card: bool| -> (GameState, Vec<String>, bool) {
            let (mut state, galaxy, ids) = tactical_fixture();
            if with_card {
                state.player_mut(&a).unwrap().action_cards =
                    vec![ti4_model::id::ActionCardId::new("lost_star")];
            }
            // A: start the action, activate, play the chart in his own after window (with the
            // card), then declare the empty movement step done.
            let script: Vec<String> = if with_card {
                vec![
                    TACTICAL_ACTION_ID.to_owned(),
                    ids[0].to_string(),
                    "reaction:generic:SYSTEM_ACTIVATED:after".to_owned(),
                    "done_moving".to_owned(),
                ]
            } else {
                vec![
                    TACTICAL_ACTION_ID.to_owned(),
                    ids[0].to_string(),
                    "done_moving".to_owned(),
                ]
            };
            let table = Table::with_default(Box::new(Scripted::new(script)));
            let mut game =
                Game::with_table(state, ContentStore::embedded(), table).with_galaxy(galaxy);

            let mut pointed_at_chart = false;
            for _ in 0..16 {
                // The switch is re-derived at the top of every step, so the value it held
                // during the step is what the step's movement actually used. Sampling
                // before and after each step sees it either way: the step that completes
                // the action is also the step whose top set it.
                let map_points =
                    |game: &Game<'_>| game.galaxy.as_ref().is_some_and(|g| g.wormhole_star_links);
                if map_points(&game) {
                    pointed_at_chart = true;
                }
                let result = game.step();
                assert_eq!(
                    result.error, None,
                    "no tactical step should refuse; log was {:?}",
                    game.events
                );
                if map_points(&game) {
                    pointed_at_chart = true;
                }
                if game.events.iter().any(|e| e == "TACTICAL_ACTION_COMPLETE") {
                    break;
                }
            }
            assert!(
                game.events.iter().any(|e| e == "TACTICAL_ACTION_COMPLETE"),
                "the action ran every step it has and closed; log was {:?}",
                game.events
            );
            (game.state, game.events, pointed_at_chart)
        };

        // Without the card the map never points at the chart.
        let (state, _events, pointed) = run(false);
        assert!(!pointed, "nothing on the map links without the card");
        assert!(
            state.player(&a).unwrap().lost_star.is_empty(),
            "no card, no marker"
        );

        // With the card the map points at the chart while the action is in flight, the marker
        // records exactly that activation, the card is spent, and a played chart is a
        // resolved card, not a registry gap.
        let (state, events, pointed) = run(true);
        assert!(
            pointed,
            "the map pointed at the chart during its action; log was {events:?}"
        );
        let seat = state.player(&a).unwrap();
        assert_eq!(
            seat.lost_star,
            vec![state.activation_seq],
            "the marker scopes the card to the action it was played in"
        );
        assert!(seat.action_cards.is_empty(), "the card was spent");
        assert!(events.iter().any(|e| e == "ACTION_CARD_PLAYED"));
        assert!(
            !events.iter().any(|e| e == "ACTION_CARD_UNRESOLVED"),
            "a played chart is not an unimplemented card"
        );
    }

    /// Drive a tactical action through to the production step and report the budget the step
    /// was offered. `reaction`, when set, is the answer to the one question the production
    /// window asks the holder of a playable card (playing is optional, so even a single
    /// playable card is offered with a decline); returns the prompt's "(n left)" figure.
    fn drive_to_production(
        state: GameState,
        galaxy: ti4_content::galaxy::Galaxy,
        ids: &[SystemId],
        reaction: Option<&str>,
    ) -> Option<u32> {
        let answers: Vec<String> = match reaction {
            Some(reaction) => vec![
                TACTICAL_ACTION_ID.to_owned(),
                ids[0].to_string(),
                format!("move|{}|0", ids[1]),
                "done_moving".to_owned(),
                reaction.to_owned(),
                "done_producing".to_owned(),
            ],
            None => vec![
                TACTICAL_ACTION_ID.to_owned(),
                ids[0].to_string(),
                format!("move|{}|0", ids[1]),
                "done_moving".to_owned(),
                "done_producing".to_owned(),
            ],
        };
        let table = Table::with_default(Box::new(Scripted::new(answers)));
        let mut game = Game::with_table(state, ContentStore::embedded(), table).with_galaxy(galaxy);

        let mut budget: Option<u32> = None;
        for _ in 0..80 {
            if let Some(choice) = game.legal_options()
                && choice.prompt.starts_with("produce in ")
            {
                let figure = choice
                    .prompt
                    .rsplit(" (")
                    .next()
                    .expect("the prompt carries its budget")
                    .strip_suffix(" left)")
                    .expect("the prompt carries its budget")
                    .parse()
                    .expect("the budget is a plain integer");
                budget = Some(figure);
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
            game.events.iter().any(|event| event == "PRODUCTION_USED"),
            "the window must have opened for a player with a budget to spend"
        );
        budget
    }

    #[test]
    fn a_war_machine_played_in_the_production_window_grows_that_steps_budget() {
        // The window opens when the step is about to happen, not after it spent its budget, so
        // a War Machine played there buys into this step. The control game, running the same
        // action without the card, spends the unboosted budget.
        //
        // The fixture producer is a Hel-Titan I (production 1, no planet involved) and trade
        // goods pay for anything, so the only number the card can move is the step's budget.
        let (mut base, galaxy, ids) = tactical_fixture();
        let a = PlayerId::new("a");
        crate::fixtures::put(&mut base, &ids[1], "destroyer", &a, 1);
        crate::fixtures::put(&mut base, &ids[0], "titans_pds", &a, 1);
        base.player_mut(&a).unwrap().trade_goods = 10;

        let bare_state = base.clone();
        base.player_mut(&a)
            .unwrap()
            .action_cards
            .push(ti4_model::id::ActionCardId::new("war_machine1"));

        let plain_budget =
            drive_to_production(bare_state, galaxy.clone(), &ids, None).expect("the step asks");
        let boosted_budget = drive_to_production(
            base,
            galaxy,
            &ids,
            Some("reaction:generic:PRODUCTION_USED:after"),
        )
        .expect("the step asks");

        assert_eq!(
            boosted_budget,
            plain_budget + 5,
            "the machine played at the window buys five faces into the step it answers"
        );
    }

    #[test]
    fn barrage_scoring_pauses_combat_and_caps_the_whole_combat_occurrence() {
        let (mut state, galaxy, ids) = tactical_fixture();
        let a = PlayerId::new("a");
        let b = PlayerId::new("b");
        state.player_mut(&a).unwrap().secret_objectives = vec![
            ti4_model::id::SecretObjectiveId::new("fwp"),
            ti4_model::id::SecretObjectiveId::new("dyp"),
        ];
        crate::fixtures::put(&mut state, &ids[0], "destroyer", &a, 4);
        crate::fixtures::put(&mut state, &ids[0], "fighter", &b, 1);
        crate::fixtures::put(&mut state, &ids[0], "cruiser", &b, 1);

        let table = Table::with_default(Box::new(Scripted::new([
            TACTICAL_ACTION_ID.to_owned(),
            ids[0].to_string(),
            "done_moving".to_owned(),
        ])));
        let mut game = Game::with_table(state, ContentStore::embedded(), table).with_galaxy(galaxy);
        // Four destroyers make eight guaranteed barrage hits, then four combat hits; the cruiser
        // misses in return. The pause must happen between those two roll groups.
        game.dice = Dice::from_faces([10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 1]);

        let mut saw_barrage_window = false;
        for _ in 0..20 {
            if game
                .legal_options()
                .is_some_and(|choice| choice.ids().contains(&"fwp"))
            {
                saw_barrage_window = true;
                assert!(
                    game.aftermath.is_some(),
                    "the tactical continuation is retained"
                );
                assert!(
                    game.event_scoring.is_some(),
                    "the exact barrage opens scoring"
                );
                assert!(
                    !game.dice.history().is_empty()
                        && game
                            .dice
                            .history()
                            .iter()
                            .all(|roll| roll.reason == "anti-fighter barrage"),
                    "ordinary combat has not rolled yet"
                );
                assert_eq!(
                    game.step().error,
                    None,
                    "Fight with Precision scores cleanly"
                );
                break;
            }
            assert_eq!(game.step().error, None, "the tactical action remains legal");
        }
        assert!(
            saw_barrage_window,
            "the barrage event reached its scoring pause"
        );

        for _ in 0..40 {
            assert!(
                !game
                    .legal_options()
                    .is_some_and(|choice| choice.ids().contains(&"dyp")),
                "one combat occurrence never offers a second secret"
            );
            assert_eq!(game.step().error, None, "the paused combat resumes cleanly");
            if game
                .events
                .iter()
                .any(|event| event == "TACTICAL_ACTION_COMPLETE")
            {
                break;
            }
        }
        assert!(
            game.events
                .iter()
                .any(|event| event == "SPACE_COMBAT_RESOLVED"),
            "events: {:?}, rolls: {:?}",
            game.events,
            game.dice.history()
        );
        assert!(
            game.events
                .iter()
                .any(|event| event == "TACTICAL_ACTION_COMPLETE")
        );
        assert!(
            game.state
                .scored_by(&a)
                .iter()
                .any(|alias| alias.as_str() == "fwp")
        );
        assert!(
            game.state
                .player(&a)
                .unwrap()
                .secret_objectives
                .iter()
                .any(|alias| alias.as_str() == "dyp"),
            "the later combat secret remains unscored"
        );
    }

    /// A decider that records every (player, prompt, option ids) it answers, then answers from
    /// a scripted queue: the recorded choices are what the engine *offered*, so a test can
    /// assert on the shape of a decision (a forced retreat lists no "stay") independent of the
    /// answer it got.
    /// The (player, prompt, option ids) of every question a decider answered.
    type SeenLog = Vec<(String, String, Vec<String>)>;

    struct ObservingDecider {
        inner: Scripted,
        seen: std::rc::Rc<std::cell::RefCell<SeenLog>>,
    }

    impl Decider for ObservingDecider {
        fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
            self.seen.borrow_mut().push((
                choice.player.to_string(),
                choice.prompt.clone(),
                choice.ids().into_iter().map(str::to_owned).collect(),
            ));
            self.inner.choose(choice)
        }
    }

    #[test]
    #[allow(
        clippy::too_many_lines,
        reason = "one run closure plus one assertion per pause-stage transition"
    )]
    fn waylay_widens_the_owners_barrage_hits_to_all_ships() {
        // Waylay: "Before you roll dice for ANTI-FIGHTER BARRAGE: hits from this roll are
        // produced against all ships (not just fighters)." A's two destroyers make one
        // guaranteed barrage hit. Without the card the hit takes a fighter; with it the hit
        // is assigned like any other, and B spends it on the cruiser instead.
        let a = PlayerId::new("a");
        let b = PlayerId::new("b");
        let run = |with_card: bool| -> (GameState, Vec<String>) {
            let (mut state, galaxy, ids) =
                combat_fixture(if with_card { &["waylay"] } else { &[] }, &[]);
            crate::fixtures::put(&mut state, &ids[0], "destroyer", &a, 2);
            crate::fixtures::put(&mut state, &ids[0], "fighter", &b, 2);
            crate::fixtures::put(&mut state, &ids[0], "cruiser", &b, 1);

            // B's ships in unit order: fighter, fighter, cruiser. The barrage hit is assigned
            // before the fleet's, so the test arm's two of B's decisions are cruiser, fighter.
            let script: Vec<String> = if with_card {
                vec![
                    TACTICAL_ACTION_ID,
                    ids[0].as_str(),
                    "done_moving",
                    "stay",
                    "reaction:generic:ANTI_FIGHTER_BARRAGE_STARTED:when",
                    "destroy|2",
                    "destroy|0",
                    "stay",
                    "retreat",
                    "02",
                ]
            } else {
                vec![
                    TACTICAL_ACTION_ID,
                    ids[0].as_str(),
                    "done_moving",
                    "stay",
                    "destroy|0",
                    "stay",
                    "retreat",
                    "02",
                ]
            }
            .into_iter()
            .map(str::to_owned)
            .collect();
            let table = Table::with_default(Box::new(Scripted::new(script)));
            let mut game =
                Game::with_table(state, ContentStore::embedded(), table).with_galaxy(galaxy);
            // Barrage: four faces, one hit. Fleet: A [hit, miss] both arms; B misses
            // everywhere. The survivor counts make the last round one B die short in the
            // test arm, so the two arms pin different tails.
            // Both arms pin the same faces: one barrage hit, one fleet hit in round one,
            // then nothing. The waylay changes *where* the one barrage hit lands, not how
            // many dice leave the bag, so the same array serves both.
            let faces: [u32; 17] = [
                10, 1, 1, 1, // A's four barrage dice: one hit
                10, 1, // A's first fleet round: one hit
                1, 1, 1, // B's first fleet round
                1, 1, // A's second round
                1, 1, // B's second round
                1, 1, // A's third round
                1, 1, // B's third round
            ];
            game.dice = Dice::from_faces(faces);

            for _ in 0..60 {
                assert_eq!(game.step().error, None, "log: {:?}", game.events);
                if game.events.iter().any(|e| e == "TACTICAL_ACTION_COMPLETE") {
                    break;
                }
            }
            assert!(
                game.events.iter().any(|e| e == "SPACE_COMBAT_RESOLVED"),
                "the combat settled; log: {:?}",
                game.events
            );
            assert!(
                game.events.iter().any(|e| e == "TACTICAL_ACTION_COMPLETE"),
                "log: {:?}",
                game.events
            );
            (game.state.clone(), game.events.clone())
        };

        // Control: the barrage hit takes a fighter automatically, the fleet hit the other.
        let (state, events) = run(false);
        assert!(
            events
                .iter()
                .filter(|e| *e == "ANTI_FIGHTER_BARRAGE_STARTED")
                .count()
                == 2,
            "both sides' barrages were announced; log: {events:?}"
        );
        let board = state.system_state(&SystemId::new("01"));
        let survivors: Vec<&str> = board
            .units
            .iter()
            .filter(|unit| unit.owner == b)
            .map(|unit| unit.type_id.as_str())
            .collect();
        assert_eq!(
            survivors,
            vec!["cruiser"],
            "both fighters die: the barrage takes one, the fleet hit the other; {events:?}"
        );
        assert_eq!(
            state.player(&a).unwrap().waylay_barrage_round,
            None,
            "no card, no marker"
        );

        // Test arm: the barrage hit is assigned, and B sinks the cruiser with it.
        let (state, events) = run(true);
        assert_eq!(
            state.player(&a).unwrap().waylay_barrage_round,
            Some(1),
            "the marker keys to the round the barrage was rolled in"
        );
        assert!(
            state.player(&a).unwrap().action_cards.is_empty(),
            "the card is spent; log: {events:?}"
        );
        let board = state.system_state(&SystemId::new("01"));
        let survivors: Vec<&str> = board
            .units
            .iter()
            .filter(|unit| unit.owner == b)
            .map(|unit| unit.type_id.as_str())
            .collect();
        assert_eq!(
            survivors,
            vec!["fighter"],
            "the waylay hit took the cruiser; log: {events:?}"
        );
    }

    #[test]
    #[allow(
        clippy::too_many_lines,
        reason = "one run closure plus one assertion per pause-stage transition"
    )]
    fn rout_forces_the_opponents_retreat_announcement() {
        // Rout: "your opponent must announce a retreat, if able," played by the defender at
        // the start of the announcement step. Without it the attacker stays and the fight
        // runs three rounds; with it the attacker's only option is to retreat, and the fight
        // is one round long. The defender's forced-ness is asserted on the options offered,
        // not just on the answer: a decider that only sees the answer could not tell a
        // forced retreat from a voluntary one.
        let a = PlayerId::new("a");
        let b = PlayerId::new("b");
        let run = |with_card: bool| -> (GameState, Vec<String>, SeenLog) {
            let (mut state, galaxy, ids) =
                combat_fixture(&[], if with_card { &["rout"] } else { &[] });
            crate::fixtures::put(&mut state, &ids[0], "cruiser", &a, 1);
            crate::fixtures::put(&mut state, &ids[0], "cruiser", &b, 2);

            let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
            let script: Vec<String> = if with_card {
                vec![
                    TACTICAL_ACTION_ID,
                    ids[0].as_str(),
                    "done_moving",
                    "reaction:generic:RETREAT_STEP_STARTED:after",
                    "retreat",
                    "02",
                ]
            } else {
                vec![
                    TACTICAL_ACTION_ID,
                    ids[0].as_str(),
                    "done_moving",
                    "stay",
                    "stay",
                    "retreat",
                    "02",
                ]
            }
            .into_iter()
            .map(str::to_owned)
            .collect();
            let decider = ObservingDecider {
                inner: Scripted::new(script),
                seen: seen.clone(),
            };
            let table = Table::with_default(Box::new(decider));
            let mut game =
                Game::with_table(state, ContentStore::embedded(), table).with_galaxy(galaxy);
            let faces: [u32; 18] = if with_card {
                [
                    1, 1, // A's fleet, the single round that happens
                    1, 1, 1, 1, // B's fleet
                    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, // padding, never drawn
                ]
            } else {
                // Each of the three rounds rolls both fleets; round three's misses precede
                // the voluntary retreat that ends the fight.
                [
                    1, 1, 1, 1, 1, 1, // round one
                    1, 1, 1, 1, 1, 1, // round two
                    1, 1, 1, 1, 1, 1, // round three
                ]
            };
            game.dice = Dice::from_faces(faces);

            for _ in 0..60 {
                assert_eq!(game.step().error, None, "log: {:?}", game.events);
                if game.events.iter().any(|e| e == "TACTICAL_ACTION_COMPLETE") {
                    break;
                }
            }
            assert!(
                game.events.iter().any(|e| e == "SPACE_COMBAT_RESOLVED"),
                "log: {:?}",
                game.events
            );
            (
                game.state.clone(),
                game.events.clone(),
                seen.borrow().clone(),
            )
        };

        // Control: the attacker stays twice and retreats voluntarily on round three.
        let (state, events, offered) = run(false);
        let rounds = events
            .iter()
            .filter(|e| *e == "COMBAT_ROUND_STARTED")
            .count();
        assert_eq!(rounds, 3, "control: {events:?}");
        let announcements: Vec<Vec<String>> = offered
            .iter()
            .filter(|(_, prompt, _)| prompt.starts_with("announce a retreat"))
            .map(|(_, _, options)| options.clone())
            .collect();
        assert_eq!(
            announcements,
            vec![
                vec!["stay".to_owned(), "retreat".to_owned()],
                vec!["stay".to_owned(), "retreat".to_owned()],
                vec!["stay".to_owned(), "retreat".to_owned()],
            ],
            "no rout: every announcement offers both options; {offered:?}"
        );
        assert!(
            state.player(&b).unwrap().rout_round.is_none(),
            "no card, no marker"
        );

        // Test arm: the defender's Rout leaves the attacker no choice on round one.
        let (state, events, offered) = run(true);
        let rounds = events
            .iter()
            .filter(|e| *e == "COMBAT_ROUND_STARTED")
            .count();
        assert_eq!(
            rounds, 1,
            "the forced retreat ends the fight in one round; {events:?}"
        );
        let announcements: Vec<Vec<String>> = offered
            .iter()
            .filter(|(_, prompt, _)| prompt.starts_with("announce a retreat"))
            .map(|(_, _, options)| options.clone())
            .collect();
        assert_eq!(
            announcements,
            vec![vec!["retreat".to_owned()]],
            "the forced announcement offers retreat and nothing else; {offered:?}"
        );
        assert_eq!(
            state.player(&b).unwrap().rout_round,
            Some(0),
            "the marker keys to the round counter as it stands when the announcement step
             opens, which the window compares before that round's dice increment it"
        );
        assert!(
            state.player(&b).unwrap().action_cards.is_empty(),
            "the card is spent"
        );
    }

    #[test]
    #[allow(
        clippy::too_many_lines,
        reason = "one run closure plus one assertion per pause-stage transition"
    )]
    fn direct_hit_destroys_the_ship_that_sustained_the_holders_hit() {
        // Direct Hit: "after another player's ship uses SUSTAIN DAMAGE to cancel a hit
        // produced by your units or abilities: destroy that ship." Every die is pinned to
        // 10, which hits on any threshold: A's cruiser (two dice) scores twice on B, and
        // B's lone die scores once on A. The defender's hits assign first: B's
        // dreadnought cancels one of A's hits, and in the test arm the card then destroys
        // the dreadnought that just sustained it; the remaining hits trade away, the
        // combat ends in round one, and B keeps the fighter. Without the card B instead
        // keeps the damaged dreadnought and trades the fighter, and the dreadnought is
        // what survives.
        let a = PlayerId::new("a");
        let b = PlayerId::new("b");
        let run = |with_card: bool| -> (GameState, Vec<String>) {
            let (mut state, galaxy, ids) =
                combat_fixture(if with_card { &["dh1"] } else { &[] }, &[]);
            crate::fixtures::put(&mut state, &ids[0], "cruiser", &a, 1);
            crate::fixtures::put(&mut state, &ids[0], "dreadnought", &b, 1);
            crate::fixtures::put(&mut state, &ids[0], "fighter", &b, 1);

            // B's units in board order: A's cruiser, then the dreadnought (sustain option
            // "sustain|1") and the fighter (destruction option "destroy|1"). B has no
            // retreat destination, so only A is ever asked to announce.
            let script: Vec<String> = (if with_card {
                [
                    TACTICAL_ACTION_ID,
                    ids[0].as_str(),
                    "done_moving",
                    "stay",
                    "sustain|1",
                    "reaction:generic:SUSTAIN_DAMAGE_USED:after",
                ]
            } else {
                [
                    TACTICAL_ACTION_ID,
                    ids[0].as_str(),
                    "done_moving",
                    "stay",
                    "sustain|1",
                    "destroy|1",
                ]
            })
            .into_iter()
            .map(str::to_owned)
            .collect();
            let table = Table::with_default(Box::new(Scripted::new(script)));
            let mut game =
                Game::with_table(state, ContentStore::embedded(), table).with_galaxy(galaxy);
            // One combat round draws A's two dice and B's one; the rest is never drawn.
            game.dice = Dice::from_faces([
                10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10,
            ]);
            for _ in 0..60 {
                assert_eq!(game.step().error, None, "log: {:?}", game.events);
                if game.events.iter().any(|e| e == "TACTICAL_ACTION_COMPLETE") {
                    break;
                }
            }
            assert!(
                game.events.iter().any(|e| e == "SPACE_COMBAT_RESOLVED"),
                "the combat settled; log: {:?}",
                game.events
            );
            (game.state.clone(), game.events.clone())
        };

        let (state, events) = run(true);
        let ships = |state: &GameState, owner: &PlayerId| -> Vec<(String, bool)> {
            state
                .system_state(&SystemId::new("01"))
                .units
                .iter()
                .filter(|unit| unit.owner == *owner)
                .map(|unit| (unit.type_id.to_string(), unit.sustained_damage))
                .collect()
        };
        assert_eq!(ships(&state, &a), Vec::<(String, bool)>::new());
        assert_eq!(
            ships(&state, &b),
            vec![("fighter".to_owned(), false)],
            "Direct Hit destroyed the sustained dreadnought and B keeps the fighter;
             log: {events:?}"
        );
        assert_eq!(
            events
                .iter()
                .filter(|e| e == &"COMBAT_ROUND_STARTED")
                .count(),
            1,
            "the card ends the fight in round one; log: {events:?}"
        );
        assert_eq!(
            events.iter().filter(|e| e == &"SHIP_DESTROYED").count(),
            2,
            "dreadnought (card) and A's cruiser (B's hit); log: {events:?}"
        );
        assert_eq!(
            events
                .iter()
                .filter(|e| e == &"SUSTAIN_DAMAGE_USED")
                .count(),
            1,
            "the dreadnought cancels exactly one hit before the card destroys it;
             log: {events:?}"
        );
        assert!(
            events.iter().any(|e| e == "ACTION_CARD_PLAYED"),
            "the card was played; log: {events:?}"
        );
        assert!(state.player(&a).unwrap().action_cards.is_empty());

        let (state, events) = run(false);
        assert_eq!(ships(&state, &a), Vec::<(String, bool)>::new());
        assert_eq!(
            ships(&state, &b),
            vec![
                ("dreadnought".to_owned(), true),
                ("fighter".to_owned(), false),
            ],
            "without the card the sustained dreadnought survives damaged; log: {events:?}"
        );
        assert_eq!(
            events
                .iter()
                .filter(|e| e == &"COMBAT_ROUND_STARTED")
                .count(),
            1,
            "log: {events:?}"
        );
        assert_eq!(
            events.iter().filter(|e| e == &"SHIP_DESTROYED").count(),
            1,
            "A's cruiser is the only loss; log: {events:?}"
        );
        assert!(
            !events.iter().any(|e| e == "ACTION_CARD_PLAYED"),
            "no card, no play; log: {events:?}"
        );
    }

    #[test]
    fn maneuvering_jets_cancels_a_hit_from_the_opponents_cannon_roll() {
        // Maneuvering Jets: "before you assign hits produced by another player's SPACE CANNON
        // roll: cancel 1 hit." B's PDS fires one die that hits A's cruiser. With the card the
        // hit is cancelled before anything is assigned, so the cruiser keeps fighting; without
        // it the single hit takes the cruiser (the cannon path removes ships without an
        // announcement, a known gap shared with the anti-fighter barrage).
        let a = PlayerId::new("a");
        let b = PlayerId::new("b");
        let run = |with_card: bool| -> (GameState, Vec<String>) {
            let (mut state, galaxy, ids) =
                combat_fixture(if with_card { &["mjets1"] } else { &[] }, &[]);
            crate::fixtures::put(&mut state, &ids[0], "cruiser", &a, 1);
            crate::fixtures::put(&mut state, &ids[0], "pds", &b, 1);

            // B has no ships, so the combat window ends as soon as it opens: nobody is ever
            // asked to announce, and only the cannon's one die leaves the bag.
            let script: Vec<String> = (if with_card {
                vec![
                    TACTICAL_ACTION_ID,
                    ids[0].as_str(),
                    "done_moving",
                    "reaction:generic:SPACE_CANNON_HITS:when",
                ]
            } else {
                vec![TACTICAL_ACTION_ID, ids[0].as_str(), "done_moving"]
            })
            .into_iter()
            .map(str::to_owned)
            .collect();
            let table = Table::with_default(Box::new(Scripted::new(script)));
            let mut game =
                Game::with_table(state, ContentStore::embedded(), table).with_galaxy(galaxy);
            game.dice =
                Dice::from_faces([10, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1]);

            for _ in 0..60 {
                assert_eq!(game.step().error, None, "log: {:?}", game.events);
                if game.events.iter().any(|e| e == "TACTICAL_ACTION_COMPLETE") {
                    break;
                }
            }
            assert!(
                game.events.iter().any(|e| e == "SPACE_CANNON_HITS"),
                "the roll announced before its hits were assigned; log: {:?}",
                game.events
            );
            // A degenerate combat (no opposing fleet) concludes silently, so the action
            // completing is the observable end.
            assert!(
                game.events.iter().any(|e| e == "TACTICAL_ACTION_COMPLETE"),
                "the action ran to completion; log: {:?}",
                game.events
            );
            (game.state.clone(), game.events.clone())
        };

        // Test arm: the hit is cancelled, the cruiser survives.
        let (state, events) = run(true);
        let space = |state: &GameState, owner: &PlayerId| -> Vec<String> {
            state
                .system_state(&SystemId::new("01"))
                .units
                .iter()
                .filter(|unit| unit.owner == *owner)
                .map(|unit| unit.type_id.to_string())
                .collect()
        };
        assert_eq!(
            space(&state, &a),
            vec!["cruiser".to_owned()],
            "the cancelled hit left the cruiser untouched; log: {events:?}"
        );
        assert_eq!(space(&state, &b), vec!["pds".to_owned()]);
        assert!(
            events.iter().any(|e| e == "ACTION_CARD_PLAYED"),
            "the card was played; log: {events:?}"
        );
        assert!(state.player(&a).unwrap().action_cards.is_empty());
        assert_eq!(
            events.iter().filter(|e| e == &"SHIP_DESTROYED").count(),
            0,
            "a cancelled hit destroys nothing; log: {events:?}"
        );

        // Control: the hit lands on the single cruiser and takes it.
        let (state, events) = run(false);
        assert_eq!(
            space(&state, &a),
            Vec::<String>::new(),
            "the uncancelled hit took the cruiser; log: {events:?}"
        );
        assert!(
            !events.iter().any(|e| e == "ACTION_CARD_PLAYED"),
            "no card, no play; log: {events:?}"
        );
    }

    #[test]
    #[allow(
        clippy::too_many_lines,
        reason = "one run closure plus one assertion per stage of the sustained exchange"
    )]
    fn reflective_shielding_turns_the_holds_sustain_into_hits_on_the_producer() {
        // Reflective Shielding: "when one of your ships uses SUSTAIN DAMAGE during combat:
        // produce 2 hits against your opponent's ships in the active system." Every die is
        // pinned to 10, which hits on any threshold: A's dreadnought scores once on B, and B
        // scores twice on A. B's dreadnought cancels A's hit. A's dreadnought then cancels
        // one of B's two; in the test arm the card turns the cancelled hit into two of its
        // own, and B — the sustained hit's producer — absorbs them: the cruiser is chosen, the
        // damaged dreadnought takes the second, and B is empty. A's second hit still lands,
        // and the fight ends in round one. Without the card B keeps both ships and A's
        // dreadnought is the only loss.
        let a = PlayerId::new("a");
        let b = PlayerId::new("b");
        let run = |with_card: bool| -> (GameState, Vec<String>) {
            let (mut state, galaxy, ids) =
                combat_fixture(if with_card { &["reflective"] } else { &[] }, &[]);
            crate::fixtures::put(&mut state, &ids[0], "dreadnought", &a, 1);
            crate::fixtures::put(&mut state, &ids[0], "dreadnought", &b, 1);
            crate::fixtures::put(&mut state, &ids[0], "cruiser", &b, 1);

            // Board order: A's dreadnought (sustain "sustain|0"), B's ("sustain|1"), B's
            // cruiser (destruction option "destroy|1" to B, whose own list is owner-filtered).
            // B has no retreat destination, so only A is ever asked to announce.
            let script: Vec<String> = (if with_card {
                vec![
                    TACTICAL_ACTION_ID,
                    ids[0].as_str(),
                    "done_moving",
                    "stay",
                    "sustain|1",
                    "sustain|0",
                    "reaction:generic:SUSTAIN_DAMAGE_USED:when",
                    "destroy|1",
                ]
            } else {
                vec![
                    TACTICAL_ACTION_ID,
                    ids[0].as_str(),
                    "done_moving",
                    "stay",
                    "sustain|1",
                    "sustain|0",
                ]
            })
            .into_iter()
            .map(str::to_owned)
            .collect();
            let table = Table::with_default(Box::new(Scripted::new(script)));
            let mut game =
                Game::with_table(state, ContentStore::embedded(), table).with_galaxy(galaxy);
            // A's one die, then B's two; the combat ends in round one, so nothing else draws.
            game.dice = Dice::from_faces([
                10, 10, 10, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
            ]);

            for _ in 0..60 {
                assert_eq!(game.step().error, None, "log: {:?}", game.events);
                if game.events.iter().any(|e| e == "TACTICAL_ACTION_COMPLETE") {
                    break;
                }
            }
            assert!(
                game.events.iter().any(|e| e == "SPACE_COMBAT_RESOLVED"),
                "the combat settled; log: {:?}",
                game.events
            );
            (game.state.clone(), game.events.clone())
        };

        let ships = |state: &GameState, owner: &PlayerId| -> Vec<(String, bool)> {
            state
                .system_state(&SystemId::new("01"))
                .units
                .iter()
                .filter(|unit| unit.owner == *owner)
                .map(|unit| (unit.type_id.to_string(), unit.sustained_damage))
                .collect()
        };

        // Test arm: the sustained hit reflects; B absorbs the two hits it earned back.
        let (state, events) = run(true);
        assert_eq!(
            ships(&state, &b),
            Vec::<(String, bool)>::new(),
            "the reflected hits took the cruiser and the damaged dreadnought;
             log: {events:?}"
        );
        assert_eq!(
            ships(&state, &a),
            Vec::<(String, bool)>::new(),
            "A's second hit still landed on the sustained dreadnought; log: {events:?}"
        );
        assert_eq!(
            events
                .iter()
                .filter(|e| e == &"SUSTAIN_DAMAGE_USED")
                .count(),
            2,
            "both dreadnoughts cancelled one hit; log: {events:?}"
        );
        assert_eq!(
            events.iter().filter(|e| e == &"SHIP_DESTROYED").count(),
            1,
            "only A's dreadnought was announced: B's losses came through the
             absorption path, which stays silent like the barrage; log: {events:?}"
        );
        assert!(
            events.iter().any(|e| e == "ACTION_CARD_PLAYED"),
            "the card was played; log: {events:?}"
        );
        assert!(state.player(&a).unwrap().action_cards.is_empty());

        // Control: B keeps the damaged dreadnought and the cruiser.
        let (state, events) = run(false);
        assert_eq!(
            ships(&state, &b),
            vec![
                ("dreadnought".to_owned(), true),
                ("cruiser".to_owned(), false),
            ],
            "nothing reflected, so B's ships survive the round; log: {events:?}"
        );
        assert_eq!(
            ships(&state, &a),
            Vec::<(String, bool)>::new(),
            "A's second hit still landed; log: {events:?}"
        );
        assert!(
            !events.iter().any(|e| e == "ACTION_CARD_PLAYED"),
            "no card, no play; log: {events:?}"
        );
    }

    #[test]
    #[allow(
        clippy::too_many_lines,
        reason = "one run closure plus one assertion per die of the revenge roll"
    )]
    fn courageous_to_the_end_makes_the_opponent_choose_the_revenge_losses() {
        // Courageous to the End: "after 1 of your ships is destroyed during a space combat:
        // roll 2 dice. For each result equal to or higher than that ship's combat value,
        // your opponent must choose and destroy 1 of their ships." A's lone cruiser scores
        // on one of B's four ships, which B cancels with the dreadnought's sustain; all
        // four of B's dice score on the cruiser, which falls as A's last ship. In the test
        // arm the card rolls two revenge dice at the cruiser's combat value (7): both
        // succeed on face 10, and B chooses the cruiser and the destroyer, keeping the
        // damaged dreadnought and the carrier. Without the card B keeps all four ships.
        // No fighters, so no space-capacity removal is asked during the move.
        let a = PlayerId::new("a");
        let b = PlayerId::new("b");
        let run = |with_card: bool| -> (GameState, Vec<String>) {
            let (mut state, galaxy, ids) =
                combat_fixture(if with_card { &["courageous"] } else { &[] }, &[]);
            crate::fixtures::put(&mut state, &ids[0], "cruiser", &a, 1);
            crate::fixtures::put(&mut state, &ids[0], "dreadnought", &b, 1);
            crate::fixtures::put(&mut state, &ids[0], "cruiser", &b, 1);
            crate::fixtures::put(&mut state, &ids[0], "destroyer", &b, 1);
            crate::fixtures::put(&mut state, &ids[0], "carrier", &b, 1);

            // Only A is asked to announce: A's fighters in the six other systems give it
            // retreat destinations, while B is anchored in the system and skipped (78.4c).
            // B's owner-filtered revenge order: dreadnought "destroy|0", cruiser
            // "destroy|1", destroyer "destroy|2", carrier "destroy|3"; after the first
            // loss the list reindexes, so the destroyer is "destroy|1" again.
            let script: Vec<String> = (if with_card {
                vec![
                    TACTICAL_ACTION_ID,
                    ids[0].as_str(),
                    "done_moving",
                    "stay",
                    "sustain|1",
                    "reaction:generic:SHIP_DESTROYED:after",
                    "destroy|1",
                    "destroy|1",
                ]
            } else {
                vec![
                    TACTICAL_ACTION_ID,
                    ids[0].as_str(),
                    "done_moving",
                    "stay",
                    "sustain|1",
                ]
            })
            .into_iter()
            .map(str::to_owned)
            .collect();
            let table = Table::with_default(Box::new(Scripted::new(script)));
            let mut game =
                Game::with_table(state, ContentStore::embedded(), table).with_galaxy(galaxy);
            // A's die, B's four, B's destroyer ANTI-FIGHTER BARRAGE (two dice, no A
            // fighters to hit), then the card's two revenge dice; nothing else draws.
            game.dice = Dice::from_faces([
                10, 10, 10, 10, 10, 10, 10, 10, 10, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
            ]);

            for _ in 0..60 {
                assert_eq!(game.step().error, None, "log: {:?}", game.events);
                if game.events.iter().any(|e| e == "TACTICAL_ACTION_COMPLETE") {
                    break;
                }
            }
            assert!(
                game.events.iter().any(|e| e == "SPACE_COMBAT_RESOLVED"),
                "the combat settled; log: {:?}",
                game.events
            );
            (game.state.clone(), game.events.clone())
        };

        let space = |state: &GameState, owner: &PlayerId| -> Vec<String> {
            state
                .system_state(&SystemId::new("01"))
                .units
                .iter()
                .filter(|unit| unit.owner == *owner)
                .map(|unit| unit.type_id.to_string())
                .collect()
        };

        // Test arm: both revenge dice meet the cruiser's value; B loses the two ships it
        // chooses, keeping the damaged dreadnought and the carrier.
        let (state, events) = run(true);
        assert_eq!(
            space(&state, &b),
            vec!["dreadnought".to_owned(), "carrier".to_owned()],
            "the card's dice took the chosen cruiser and destroyer;
             log: {events:?}"
        );
        assert_eq!(space(&state, &a), Vec::<String>::new());
        assert_eq!(
            events.iter().filter(|e| e == &"SHIP_DESTROYED").count(),
            3,
            "A's auto-casualty plus the card's two staged losses, all announced;
             log: {events:?}"
        );
        assert!(
            events.iter().any(|e| e == "ACTION_CARD_PLAYED"),
            "the card was played; log: {events:?}"
        );
        assert!(state.player(&a).unwrap().action_cards.is_empty());

        // Control: the round's own hits are all that lands, and the dreadnought's sustain
        // soaks A's one; B keeps all four ships.
        let (state, events) = run(false);
        assert_eq!(
            space(&state, &b),
            vec![
                "dreadnought".to_owned(),
                "cruiser".to_owned(),
                "destroyer".to_owned(),
                "carrier".to_owned(),
            ],
            "without the card the round took A's ship only;
             log: {events:?}"
        );
        assert_eq!(
            events.iter().filter(|e| e == &"SHIP_DESTROYED").count(),
            1,
            "A's last ship is the only announced loss; log: {events:?}"
        );
        assert!(
            !events.iter().any(|e| e == "ACTION_CARD_PLAYED"),
            "no card, no play; log: {events:?}"
        );
    }

    #[test]
    #[allow(
        clippy::too_many_lines,
        reason = "one run closure plus one assertion per placement fact of the landing"
    )]
    fn crashlanding_lands_the_holders_ground_force_when_their_last_ship_falls() {
        // Crash Landing: "when your last ship in the active system is destroyed: place 1 of
        // your ground forces from the space area of the active system onto a planet in that
        // system (other than Mecatol Rex). If the planet contains other players' units, place
        // your ground forces into coexistence." Every die is pinned to 10: A's cruiser scores
        // on B's dreadnought, and B's dreadnought scores on A's cruiser, so both sides are
        // empty when the round ends. In the test arm A's lone ship was their last, so the
        // infantry in A's space area lands on Jord — the system's only planet — and joins
        // B's infantry there in coexistence. Without the card the infantry stays in the
        // space area and nothing coexists.
        let a = PlayerId::new("a");
        let b = PlayerId::new("b");
        let run = |with_card: bool| -> (GameState, Vec<String>) {
            let (mut state, galaxy, ids) =
                combat_fixture(if with_card { &["crashlanding"] } else { &[] }, &[]);
            // A carrier, not a cruiser: the infantry has to be legally in the space area, and a
            // cruiser has no capacity. Every die is pinned to 10, so the swap does not change who
            // hits -- only whether the position could have arisen in play. (16.3 is now enforced
            // for ground forces as well as fighters, which is what caught this.)
            crate::fixtures::put(&mut state, &ids[0], "carrier", &a, 1);
            crate::fixtures::put(&mut state, &ids[0], "infantry", &a, 1);
            crate::fixtures::put(&mut state, &ids[0], "dreadnought", &b, 1);
            crate::fixtures::put_on_planet(
                &mut state,
                &ids[0],
                &PlanetId::new("jord"),
                "infantry",
                &b,
                1,
            );

            // B has no retreat destination, so only A is ever asked to announce.
            // A alone is asked to announce (it holds the fighters in the other systems);
            // B's dreadnought could sustain A's one hit, so the script declines and the
            // hit lands.
            let script: Vec<String> = (if with_card {
                vec![
                    TACTICAL_ACTION_ID,
                    ids[0].as_str(),
                    "done_moving",
                    "stay",
                    "decline",
                    "reaction:generic:SHIP_DESTROYED:when",
                ]
            } else {
                vec![
                    TACTICAL_ACTION_ID,
                    ids[0].as_str(),
                    "done_moving",
                    "stay",
                    "decline",
                ]
            })
            .into_iter()
            .map(str::to_owned)
            .collect();
            let table = Table::with_default(Box::new(Scripted::new(script)));
            let mut game =
                Game::with_table(state, ContentStore::embedded(), table).with_galaxy(galaxy);
            // A's die, B's die; the combat ends in round one, so nothing else draws.
            game.dice =
                Dice::from_faces([10, 10, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1]);

            for _ in 0..60 {
                assert_eq!(game.step().error, None, "log: {:?}", game.events);
                if game.events.iter().any(|e| e == "TACTICAL_ACTION_COMPLETE") {
                    break;
                }
            }
            assert!(
                game.events.iter().any(|e| e == "SPACE_COMBAT_RESOLVED"),
                "the combat settled; log: {:?}",
                game.events
            );
            (game.state.clone(), game.events.clone())
        };

        let space = |state: &GameState, owner: &PlayerId| -> Vec<String> {
            state
                .system_state(&SystemId::new("01"))
                .units
                .iter()
                .filter(|unit| unit.owner == *owner)
                .map(|unit| unit.type_id.to_string())
                .collect()
        };
        let on_jord = |state: &GameState| -> Vec<String> {
            state
                .system_state(&SystemId::new("01"))
                .planet_units
                .get(&PlanetId::new("jord"))
                .map(|units| {
                    units
                        .iter()
                        .map(|unit| format!("{}:{}", unit.owner, unit.type_id))
                        .collect()
                })
                .unwrap_or_default()
        };
        let jord_coexisting = |state: &GameState| -> Vec<String> {
            state
                .system_state(&SystemId::new("01"))
                .coexisting
                .get(&PlanetId::new("jord"))
                .map(|players| players.iter().map(ToString::to_string).collect())
                .unwrap_or_default()
        };

        // Test arm: A's last ship fell, so the infantry lands on Jord and coexists.
        let (state, events) = run(true);
        assert_eq!(
            space(&state, &a),
            Vec::<String>::new(),
            "the infantry left the space area with the last ship gone; log: {events:?}"
        );
        assert_eq!(
            on_jord(&state),
            vec!["b:infantry".to_owned(), "a:infantry".to_owned()],
            "the landing put A's infantry on Jord beside B's; log: {events:?}"
        );
        assert_eq!(
            jord_coexisting(&state),
            vec!["a".to_owned()],
            "B's infantry was there, so A coexists rather than controls; log: {events:?}"
        );
        assert_eq!(
            events.iter().filter(|e| e == &"SHIP_DESTROYED").count(),
            2,
            "each side's last ship; log: {events:?}"
        );
        assert!(
            events.iter().any(|e| e == "ACTION_CARD_PLAYED"),
            "the card was played; log: {events:?}"
        );
        assert!(state.player(&a).unwrap().action_cards.is_empty());

        // Control: the infantry never leaves the space area.
        let (state, events) = run(false);
        assert_eq!(
            space(&state, &a),
            vec!["infantry".to_owned()],
            "no card, no landing; log: {events:?}"
        );
        assert_eq!(on_jord(&state), vec!["b:infantry".to_owned()]);
        assert_eq!(jord_coexisting(&state), Vec::<String>::new());
        assert!(
            !events.iter().any(|e| e == "ACTION_CARD_PLAYED"),
            "no card, no play; log: {events:?}"
        );
    }

    #[test]
    fn space_cannon_opens_an_unlimited_occurrence_scoring_window() {
        let (mut state, galaxy, ids) = tactical_fixture();
        let a = PlayerId::new("a");
        let b = PlayerId::new("b");
        state.player_mut(&b).unwrap().secret_objectives =
            vec![ti4_model::id::SecretObjectiveId::new("ttfd")];
        crate::fixtures::put(&mut state, &ids[0], "cruiser", &a, 1);
        crate::fixtures::put(&mut state, &ids[0], "fighter", &a, 1);
        crate::fixtures::put(&mut state, &ids[0], "pds", &b, 1);
        crate::fixtures::put(&mut state, &ids[0], "destroyer", &b, 1);

        let table = Table::with_default(Box::new(Scripted::new([
            TACTICAL_ACTION_ID.to_owned(),
            ids[0].to_string(),
            "done_moving".to_owned(),
        ])));
        let mut game = Game::with_table(state, ContentStore::embedded(), table).with_galaxy(galaxy);
        game.dice = Dice::from_faces([10]);

        for _ in 0..10 {
            if game
                .legal_options()
                .is_some_and(|choice| choice.ids().contains(&"ttfd"))
            {
                assert!(
                    game.aftermath.is_some(),
                    "the tactical continuation is retained"
                );
                assert!(
                    game.event_scoring.is_some(),
                    "space cannon opened the event window"
                );
                assert!(
                    game.state
                        .player(&b)
                        .unwrap()
                        .event_feats
                        .iter()
                        .any(|(feat, _)| *feat == Feat::SpaceCannonTookTheLastNonFighters)
                );
                assert!(
                    game.dice
                        .history()
                        .iter()
                        .all(|roll| roll.reason.contains("space cannon")),
                    "combat and barrage remain paused until cannon scoring closes: {:?}",
                    game.dice.history()
                );
                return;
            }
            assert_eq!(game.step().error, None, "the cannon sequence remains legal");
        }
        panic!("the space-cannon occurrence never offered Turn Their Fleets to Dust");
    }

    // -- M07-019: nested-window revalidation ---------------------------------------------------
    //
    // The M06 event-scoped secret windows pause tactical resolution mid-combat and mid-invasion.
    // These tests pin the integration boundary: faction/TE effects in flight when a window
    // opened must resume on exactly the retained frame, and sequence-scoped markers must expire
    // by their identity rather than because of the pause.

    /// Activates one system, pays Munitions Reserves in round one, declines it in round two,
    /// declines the fwp window, and takes every other offered option (casualty assignments
    /// included).
    struct MunitionsDriver {
        target: String,
        paid: bool,
    }

    impl crate::choice::Decider for MunitionsDriver {
        fn choose(
            &mut self,
            choice: &crate::choice::Choice,
        ) -> Result<crate::choice::ChoiceOption, crate::choice::IllegalChoice> {
            let ids = choice.ids();
            if ids.contains(&TACTICAL_ACTION_ID) {
                return Ok(choice
                    .options
                    .iter()
                    .find(|o| o.id == TACTICAL_ACTION_ID)
                    .cloned()
                    .expect("the action names itself"));
            }
            if let Some(option) = choice.options.iter().find(|o| o.id == self.target) {
                return Ok(option.clone());
            }
            if ids.contains(&"done_moving") {
                return Ok(choice
                    .options
                    .iter()
                    .find(|o| o.id == "done_moving")
                    .cloned()
                    .expect("movement always offers done"));
            }
            if ids.contains(&"munitions") {
                let pay = !self.paid;
                self.paid = true;
                return Ok(choice
                    .options
                    .iter()
                    .find(|o| o.id == (if pay { "munitions" } else { "decline" }))
                    .cloned()
                    .expect("the offer names its own options"));
            }
            if ids.contains(&"fwp") {
                return Ok(choice
                    .options
                    .iter()
                    .find(|o| o.id == "decline")
                    .cloned()
                    .expect("a scoring window always offers decline"));
            }
            choice
                .options
                .first()
                .cloned()
                .ok_or(crate::choice::IllegalChoice::NotOffered {
                    player: choice.player.clone(),
                    chosen: String::new(),
                    offered: ids.into_iter().map(str::to_owned).collect(),
                })
        }
    }

    #[test]
    fn munitions_reserves_survive_the_barrage_scoring_pause() {
        let (mut state, galaxy, ids) = tactical_fixture();
        let a = PlayerId::new("a");
        let b = PlayerId::new("b");
        state.player_mut(&a).unwrap().faction = ti4_model::id::FactionId::new("letnev");
        state.player_mut(&a).unwrap().trade_goods = 4;
        state.player_mut(&a).unwrap().secret_objectives =
            vec![ti4_model::id::SecretObjectiveId::new("fwp")];
        state.player_mut(&b).unwrap().secret_objectives.clear();
        crate::fixtures::put(&mut state, &ids[0], "destroyer", &a, 2);
        crate::fixtures::put(&mut state, &ids[0], "fighter", &b, 1);
        // Eight cruisers: the barrage takes the fighter, and even a fully successful munitions
        // reroll (two hits) cannot wipe b out, so the fight is guaranteed to reach round two.
        crate::fixtures::put(&mut state, &ids[0], "cruiser", &b, 8);

        let table = Table::with_default(Box::new(MunitionsDriver {
            target: ids[0].to_string(),
            paid: false,
        }));
        let mut game = Game::with_table(state, ContentStore::embedded(), table).with_galaxy(galaxy);
        // The barrage's four anti-fighter dice all hit; every ordinary roll after that is a
        // miss for both sides. Rerolls draw from the seeded stream rather than this preload,
        // so round one's munitions reroll may land anywhere — which is exactly why b keeps
        // eight cruisers: the test must hold whatever the reroll does.
        let mut faces = vec![10u32; 4];
        faces.extend(std::iter::repeat_n(1, 600));
        game.dice = Dice::from_faces(faces);

        let mut saw_pause = false;
        for _ in 0..30 {
            if game
                .legal_options()
                .is_some_and(|choice| choice.ids().contains(&"fwp"))
            {
                saw_pause = true;
                assert!(
                    game.aftermath.is_some(),
                    "the tactical continuation is retained"
                );
                assert!(
                    game.event_scoring.is_some(),
                    "the barrage opened the event window"
                );
                let seat = game.state.player(&a).unwrap();
                assert_eq!(
                    seat.munitions_round,
                    Some(1),
                    "the paid marker survives the pause"
                );
                assert_eq!(seat.trade_goods, 2, "the cost was paid before the pause");
                assert_eq!(game.step().error, None, "declining closes the window");
                break;
            }
            assert_eq!(game.step().error, None, "the sequence remains legal");
        }
        assert!(saw_pause, "the barrage never reached its scoring pause");

        // Fifty rounds of mutual misses; each round costs a few steps (offer, auto-advance), so
        // allow plenty.
        for _ in 0..600 {
            assert_eq!(game.step().error, None, "the paused combat resumes cleanly");
            if game
                .events
                .iter()
                .any(|event| event == "TACTICAL_ACTION_COMPLETE")
            {
                break;
            }
        }
        assert!(
            game.events
                .iter()
                .any(|event| event == "SPACE_COMBAT_RESOLVED"),
            "events: {:?}, rolls: {:?}",
            game.events,
            game.dice.history()
        );

        // The marker was honored exactly once — in the round it was paid for — and expired by
        // identity when the next round opened, not because of the pause.
        let rerolls = game
            .dice
            .history()
            .iter()
            .filter(|roll| roll.reason.starts_with("munitions:"))
            .count();
        assert_eq!(rerolls, 1, "rolls: {:?}", game.dice.history());

        // The round-2 offer appeared and was declined rather than inherited.
        let payments = game
            .table
            .log
            .records
            .iter()
            .filter(|r| r.chosen == "munitions")
            .count();
        assert_eq!(payments, 1, "log: {:?}", game.table.log);
        let paid_at = game
            .table
            .log
            .records
            .iter()
            .position(|r| r.chosen == "munitions")
            .expect("the round-1 payment is logged");
        assert!(
            game.table.log.records[paid_at + 1..]
                .iter()
                .any(|r| r.chosen == "decline" && r.offered.contains(&"munitions".to_owned())),
            "round 2 re-offered the ability: {:?}",
            game.table.log
        );
    }

    /// Commits the invader's ground forces to one planet and nothing else; ground-combat
    /// prompts (fight, casualty assignments) take their first option.
    struct CommitTo(ti4_model::id::PlanetId);

    impl crate::choice::Decider for CommitTo {
        fn choose(
            &mut self,
            choice: &crate::choice::Choice,
        ) -> Result<crate::choice::ChoiceOption, crate::choice::IllegalChoice> {
            let wanted = format!("commit|0|{}", self.0.as_str());
            if let Some(option) = choice.options.iter().find(|o| o.id == wanted) {
                return Ok(option.clone());
            }
            if let Some(done) = choice.options.iter().find(|o| o.id == "done_committing") {
                return Ok(done.clone());
            }
            choice
                .options
                .first()
                .cloned()
                .ok_or(crate::choice::IllegalChoice::NoOptions {
                    player: choice.player.clone(),
                    prompt: choice.prompt.clone(),
                })
        }
    }

    #[test]
    #[allow(
        clippy::too_many_lines,
        reason = "one fixture plus one assertion per pause-stage transition"
    )]
    fn the_home_loss_pause_holds_the_invasion_at_finalizing_control() {
        // The home-loss scoring pause holds the invasion at FinalizingControl: control
        // transfers before the pause, capture happens exactly once, and the window resumes to
        // done after the settle that follows the scoring window.
        //
        // LRR 49 (KD-2): b's planet holds only structures, which are not ground forces, so it
        // falls without resistance — no fight is offered at all. At the pause the three rival
        // structures still stand unconverted; after the resume, Assimilate converts them
        // one-for-one to a's own (L1Z1X has no structure variants of its own, so they come back
        // as generic pds/spacedock owned by a), and nothing is left rival-owned. Assimilate's
        // conversion itself is covered by the direct `control_gained` tests in
        // faction_abilities.rs.
        let content = ContentStore::embedded();
        let (mut state, system, planet) = {
            let players = [PlayerId::new("a"), PlayerId::new("b")];
            let state = start_game(content, &players, POK, None).unwrap();
            let planets = ti4_content::galaxy::all_planets(content, POK);
            let (id, p) = planets
                .iter()
                .find(|(_, p)| {
                    p.system_id().is_some()
                        && !p.is_placed_during_play()
                        && p.system_id() != Some(crate::seating::MECATOL)
                })
                .expect("the corpus has a placed planet outside Mecatol Rex");
            (
                state,
                SystemId::new(p.system_id().unwrap()),
                ti4_model::id::PlanetId::new(*id),
            )
        };
        let a = PlayerId::new("a");
        let b = PlayerId::new("b");
        state.player_mut(&a).unwrap().faction = ti4_model::id::FactionId::new("l1z1x");
        state.player_mut(&b).unwrap().home_system = Some(system.clone());
        state.player_mut(&b).unwrap().secret_objectives =
            vec![ti4_model::id::SecretObjectiveId::new("bam")];
        state
            .system_mut(&system)
            .set_control(planet.clone(), b.clone());
        for kind in ["pds", "pds", "spacedock"] {
            state
                .system_mut(&system)
                .planet_units
                .entry(planet.clone())
                .or_default()
                .push(Unit::new(ti4_model::id::UnitTypeId::new(kind), b.clone()));
        }
        for _ in 0..3 {
            state.system_mut(&system).units.push(Unit::new(
                ti4_model::id::UnitTypeId::new("infantry"),
                a.clone(),
            ));
        }

        let mut dice = Dice::new();
        let mut rng = crate::rng::GameRng::new(1);
        let mut table = Table::with_default(Box::new(CommitTo(planet.clone())));
        let mut window = crate::invasion::InvasionWindow::new(
            &mut state, content, POK, &mut dice, &mut rng, &a, &system,
        );
        let mut ctx = crate::choice::Resolving {
            content,
            sources: POK,
            dice: &mut dice,
            rng: &mut rng,
            table: &mut table,
            timing: None,
        };

        // Commit the ground forces and take control. The sequence mirrors what a Game-level
        // driver sees: with no rival ground forces on the planet there is no fight, so the
        // invasion settles straight to control — establishing it and queueing the home-loss
        // occurrence, which pauses at FinalizingControl before any gain-control effect may run.
        crate::choice::Window::drive(&mut window, &mut state, &mut ctx).unwrap();

        let (occurrence, combat) = window
            .take_scoring_occurrence()
            .expect("the control loss creates a scoring occurrence");
        assert!(!combat, "control loss is not a combat occurrence");
        assert!(state.did_at_occurrence(&b, Feat::LostAHomePlanet, occurrence));

        let standing = state
            .system_state(&system)
            .planet_units
            .get(&planet)
            .cloned()
            .unwrap();
        // The planet fell without resistance: at the pause b's three structures still stand,
        // unconverted — Assimilate runs only when the window closes.
        assert_eq!(
            standing.len(),
            6,
            "a's infantry plus b's intact structures: {standing:?}"
        );
        let rival_structures = standing
            .iter()
            .filter(|unit| {
                unit.owner == b
                    && (unit.type_id.as_str() == "pds" || unit.type_id.as_str() == "spacedock")
            })
            .count();
        assert_eq!(
            rival_structures, 3,
            "no conversion has run before the window closes: {standing:?}"
        );
        // Control itself already changed hands before the pause (establish_control runs first);
        // what waits is the gain-control effect.
        assert_eq!(
            state.system_state(&system).planet_control.get(&planet),
            Some(&a)
        );

        window.settle(&mut state, &mut ctx);
        assert!(
            window.is_done(),
            "the retained invasion resumes after scoring"
        );
        let report = window.into_report();
        assert_eq!(report.captured, vec![(planet.clone(), Some(b.clone()))]);

        // Assimilate converted the structures one-for-one — L1Z1X has no structure variants of
        // its own, so they come back as generic pds/spacedock owned by a. Nothing is duplicated,
        // lost, or left rival-owned.
        let after = state
            .system_state(&system)
            .planet_units
            .get(&planet)
            .cloned()
            .unwrap();
        assert_eq!(
            after.len(),
            standing.len(),
            "no unit is duplicated or lost: {after:?}"
        );
        assert!(
            after.iter().all(|unit| unit.owner == a),
            "nothing is left rival-owned after the resume: {after:?}"
        );
        let count = |units: &[Unit], id: &str| {
            units
                .iter()
                .filter(|unit| unit.type_id.as_str() == id)
                .count()
        };
        assert_eq!(
            count(&after, "pds"),
            2,
            "one-for-one PDS conversion: {after:?}"
        );
        assert_eq!(
            count(&after, "spacedock"),
            1,
            "one-for-one spacedock conversion: {after:?}"
        );
    }

    #[test]
    fn flank_speed_expires_at_the_activation_boundary_across_a_scoring_pause() {
        // Flank Speed scopes its +1 to the tactical action it is played in. A scoring pause
        // inside that action must not advance the activation identity early, and the bonus must
        // expire exactly when the next activation begins.
        let (mut state, galaxy, ids) = tactical_fixture();
        let a = PlayerId::new("a");
        let b = PlayerId::new("b");
        state.player_mut(&a).unwrap().action_cards = vec![ti4_model::id::ActionCardId::new("fs1")];
        state.player_mut(&a).unwrap().secret_objectives =
            vec![ti4_model::id::SecretObjectiveId::new("fwp")];
        state.player_mut(&b).unwrap().secret_objectives.clear();
        crate::fixtures::put(&mut state, &ids[0], "destroyer", &a, 2);
        // b's only ship is a fighter: the anti-fighter barrage takes it, so the fight ends
        // right after its scoring pause — no rounds, no casualty choices to script around.
        crate::fixtures::put(&mut state, &ids[0], "fighter", &b, 1);

        let table = Table::with_default(Box::new(Scripted::new([
            TACTICAL_ACTION_ID.to_owned(),
            ids[0].to_string(),
            // The after-activation window asks for the guarded slot; with exactly one playable
            // card in hand, playing it is not a further choice — the slot plays it directly.
            "reaction:generic:SYSTEM_ACTIVATED:after".to_owned(),
            "done_moving".to_owned(),
            "decline".to_owned(), // the barrage's fwp window
        ])));
        let mut game = Game::with_table(state, ContentStore::embedded(), table).with_galaxy(galaxy);
        // Four anti-fighter dice (two per destroyer), all hits. The fight is already over by
        // then, but the round still rolls: a's two destroyers roll once more against nothing.
        game.dice = Dice::from_faces([10, 10, 10, 10, 1, 1]);

        let mut saw_pause = false;
        for _ in 0..30 {
            if game
                .legal_options()
                .is_some_and(|choice| choice.ids().contains(&"fwp"))
            {
                saw_pause = true;
                let activation = game.state.activation_seq;
                assert_eq!(
                    game.state.player(&a).unwrap().move_bonus_activation,
                    Some(activation),
                    "the bonus is still scoped to this activation during the pause"
                );
                let content = ContentStore::embedded();
                let types = ti4_content::units::catalogue(content, POK);
                let destroyer = types.get("destroyer").expect("the corpus has a destroyer");
                assert_eq!(
                    crate::tactical::effective_move_value_with_gravity(
                        &game.state,
                        destroyer,
                        &a,
                        &ids[0],
                        false
                    ),
                    3,
                    "base 2 plus the live Flank Speed bonus"
                );
                let paused_at = game.state.activation_seq;
                assert_eq!(paused_at, 1, "one activation has happened so far");
                assert_eq!(game.step().error, None, "declining closes the window");
                break;
            }
            assert_eq!(game.step().error, None, "the sequence remains legal");
        }
        assert!(saw_pause, "the barrage never reached its scoring pause");

        for _ in 0..40 {
            assert_eq!(game.step().error, None, "the paused combat resumes cleanly");
            if game
                .events
                .iter()
                .any(|event| event == "TACTICAL_ACTION_COMPLETE")
            {
                break;
            }
        }
        // The pause neither advanced the activation identity nor cleared the marker: it still
        // names this action's activation, and simply stops matching once the next one begins.
        assert_eq!(
            game.state.activation_seq, 1,
            "the pause did not advance the sequence"
        );
        assert_eq!(
            game.state.player(&a).unwrap().move_bonus_activation,
            Some(1),
            "the marker is not cleared early; it simply stops matching"
        );
        assert_eq!(crate::action_cards::move_bonus(&game.state, &a, 1), 1);
        assert_eq!(
            crate::action_cards::move_bonus(&game.state, &a, 2),
            0,
            "expired by identity at the next activation"
        );
    }

    #[test]
    #[allow(
        clippy::too_many_lines,
        reason = "one fixture plus one assertion per pause-stage transition"
    )]
    fn te_breakthrough_survives_the_combat_scoring_pause() {
        // A Thunder's Edge expedition grants a persistent breakthrough. The tactical action that
        // follows can pause mid-combat on an event-scoped secret; the TE state and its effect
        // must be intact when it resumes.
        use ti4_model::content_types::FULL;
        let content = ContentStore::embedded();
        let players = [PlayerId::new("a"), PlayerId::new("b")];
        let mut state = start_game(content, &players, FULL, None).unwrap();
        let ids: Vec<String> = ti4_content::galaxy::all_systems(content, FULL)
            .iter()
            .filter(|(_, system)| !system.is_anomaly() && !system.is_hyperlane())
            .map(|(id, _)| (*id).to_owned())
            .take(7)
            .collect();
        let refs: Vec<&str> = ids.iter().map(String::as_str).collect();
        let galaxy = ti4_content::galaxy::Galaxy::build(content, &refs, FULL, 1).unwrap();
        let ids: Vec<SystemId> = ids.into_iter().map(SystemId::new).collect();

        state.phase = Phase::Action;
        state.active = Some(PlayerId::new("a"));
        let a = PlayerId::new("a");
        state.player_mut(&a).unwrap().faction = ti4_model::id::FactionId::new("letnev");
        state.player_mut(&a).unwrap().trade_goods = 3;
        state.player_mut(&a).unwrap().secret_objectives =
            vec![ti4_model::id::SecretObjectiveId::new("fwp")];
        state
            .player_mut(&PlayerId::new("b"))
            .unwrap()
            .secret_objectives
            .clear();

        // The expedition: spend 3 trade goods for the trade_goods slice; the first slice grants
        // Letnev's breakthrough.
        let option = crate::thunders_edge::available_actions(&state, content, FULL, &a)
            .into_iter()
            .find(|option| option.id == "component|expedition|trade_goods")
            .expect("the trade_goods slice is offered");
        let mut setup_table = Table::new();
        assert!(
            crate::thunders_edge::perform(
                &mut state,
                content,
                FULL,
                None,
                &mut setup_table,
                &a,
                &option
            )
            .unwrap()
        );
        assert_eq!(
            state.player(&a).unwrap().breakthrough,
            Some(ti4_model::id::BreakthroughId::new("letnevbt"))
        );

        crate::fixtures::put(&mut state, &ids[0], "destroyer", &a, 2);
        // One more destroyer in the neighbouring system: moving it establishes letnevbt's
        // Gravleash origin anchor before combat starts.
        crate::fixtures::put(&mut state, &ids[1], "destroyer", &a, 1);
        crate::fixtures::put(&mut state, &ids[0], "fighter", &PlayerId::new("b"), 1);
        // Four cruisers: the barrage takes the fighter, and round one's four hits wipe a's
        // three destroyers so the fight ends in exactly one round.
        crate::fixtures::put(&mut state, &ids[0], "cruiser", &PlayerId::new("b"), 4);

        let table = Table::with_default(Box::new(Scripted::new([
            TACTICAL_ACTION_ID.to_owned(),
            ids[0].to_string(),
            format!("move|{}|0", ids[1]),
            "done_moving".to_owned(),
            "decline".to_owned(), // the barrage's fwp window
        ])));
        let mut game = Game::with_table(state, content, table)
            .with_galaxy(galaxy)
            .with_sources(FULL);
        // Six anti-fighter dice (three destroyers after the move), all hits; round one: a
        // misses with three dice while b's four cruisers all hit and end the fight.
        game.dice = Dice::from_faces([10, 10, 10, 10, 10, 10, 1, 1, 1, 7, 7, 7, 7]);

        let mut saw_pause = false;
        for _ in 0..30 {
            if game
                .legal_options()
                .is_some_and(|choice| choice.ids().contains(&"fwp"))
            {
                saw_pause = true;
                assert_eq!(
                    game.state.expedition_slices.get("trade_goods"),
                    Some(&a),
                    "the claimed slice survives the pause"
                );
                assert_eq!(
                    game.state.player(&a).unwrap().breakthrough,
                    Some(ti4_model::id::BreakthroughId::new("letnevbt")),
                    "the breakthrough is intact during the pause"
                );
                assert_eq!(
                    game.state.gravleash_move_values.get(&ids[1]),
                    Some(&2),
                    "the Gravleash anchor established by the move survives the pause"
                );
                assert_eq!(game.step().error, None, "declining closes the window");
                break;
            }
            assert_eq!(game.step().error, None, "the sequence remains legal");
        }
        assert!(saw_pause, "the barrage never reached its scoring pause");

        for _ in 0..40 {
            assert_eq!(game.step().error, None, "the paused combat resumes cleanly");
            if game
                .events
                .iter()
                .any(|event| event == "TACTICAL_ACTION_COMPLETE")
            {
                break;
            }
        }
        let seat = game.state.player(&a).unwrap();
        assert_eq!(
            seat.breakthrough,
            Some(ti4_model::id::BreakthroughId::new("letnevbt")),
            "the breakthrough is intact after the pause"
        );
        assert_eq!(game.state.expedition_slices.get("trade_goods"), Some(&a));
        // The anchor outlives the tactical action: it is per-origin state, not per-action.
        assert_eq!(game.state.gravleash_move_values.get(&ids[1]), Some(&2));
    }

    #[test]
    fn the_last_pass_opens_its_own_action_occurrence_before_status() {
        let a = PlayerId::new("a");
        let b = PlayerId::new("b");
        let mut state = start_game(ContentStore::embedded(), &[a.clone(), b.clone()], POK, None)
            .expect("fixture starts");
        state.phase = Phase::Action;
        state.active = Some(b.clone());
        state.player_mut(&a).unwrap().passed = true;
        state.player_mut(&b).unwrap().secret_objectives =
            vec![ti4_model::id::SecretObjectiveId::new("pe")];
        let table = Table::with_default(Box::new(Scripted::new([
            "pass".to_owned(),
            "pe".to_owned(),
        ])));
        let mut game = Game::with_table(state, ContentStore::embedded(), table);

        assert_eq!(game.step().error, None, "the final pass resolves");
        let choice = game
            .legal_options()
            .expect("the last-pass scoring pause remains open");
        assert!(choice.ids().contains(&"pe"));
        let occurrence = FeatOccurrence(game.state.feat_occurrence_seq);
        assert!(
            game.state
                .did_at_occurrence(&b, Feat::LastToPass, occurrence)
        );
        assert!(
            !game
                .state
                .did_at_occurrence(&a, Feat::LastToPass, occurrence)
        );

        assert_eq!(game.step().error, None, "Prove Endurance scores");
        assert!(
            game.state
                .scored_by(&b)
                .iter()
                .any(|alias| alias.as_str() == "pe")
        );
    }

    #[test]
    fn a_last_pass_occurrence_replays_with_identical_state_and_choices() {
        let run = || {
            let a = PlayerId::new("a");
            let b = PlayerId::new("b");
            let mut state =
                start_game(ContentStore::embedded(), &[a.clone(), b.clone()], POK, None).unwrap();
            state.phase = Phase::Action;
            state.active = Some(b.clone());
            state.player_mut(&a).unwrap().passed = true;
            state.player_mut(&b).unwrap().secret_objectives =
                vec![ti4_model::id::SecretObjectiveId::new("pe")];
            let table = Table::with_default(Box::new(Scripted::new([
                "pass".to_owned(),
                "pe".to_owned(),
            ])));
            let mut game = Game::with_table(state, ContentStore::embedded(), table);
            assert_eq!(game.step().error, None);
            assert_eq!(game.step().error, None);
            (game.state, game.table.log)
        };

        let (left_state, left_log) = run();
        let (right_state, right_log) = run();
        assert!(left_state.identical(&right_state));
        assert_eq!(left_log, right_log);
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
        assert_eq!(hold.prompt, "load carrier (4 free)");
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
        // Any For/Against law: the point of this test is that a law passing goes through the whole
        // vote and stays in play afterwards.
        //
        // It used to insist on one with *no* registered effect, to show that an unresolvable agenda
        // still voted cleanly. Every agenda is registered now, so that premise no longer exists in
        // the corpus and the search found nothing. A test whose fixture depends on the engine being
        // incomplete expires the moment it is completed.
        let law = ContentStore::embedded()
            .records(ti4_model::content_types::ContentType::Agendas)
            .iter()
            .find(|record| {
                record.text("type") == Some("Law") && record.text("target") == Some("For/Against")
            })
            .and_then(|record| record.text("alias"))
            .expect("the corpus has a For/Against law")
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
        let occurrence_before = state.feat_occurrence_seq;
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
            !game
                .events
                .iter()
                .any(|e| e.starts_with("AGENDA_EFFECT_UNRESOLVED:")),
            "every agenda is registered now, so none may report itself unresolved"
        );
        assert_eq!(
            game.state.laws.get(&law).map(String::as_str),
            Some("for"),
            "8.20: a passed law stays in play"
        );
        assert!(
            game.state.feat_occurrence_seq > occurrence_before,
            "a resolved For/Against agenda receives an occurrence even without an elected player"
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

        let occurrence_before = state.feat_occurrence_seq;
        let mut game = Game::with_seeded_random(state, ContentStore::embedded(), 11);
        let mut guard = 0;
        let mut checked_between_agendas = false;
        while game.state.phase == Phase::Agenda && guard < 200 {
            assert_eq!(game.step().error, None, "no agenda step should refuse");
            if !checked_between_agendas
                && game
                    .events
                    .iter()
                    .filter(|event| event.starts_with("AGENDA_RESOLVED:"))
                    .count()
                    == 1
            {
                assert!(game.event_scoring.is_some());
                assert!(
                    game.voting.is_none(),
                    "the next agenda is not revealed early"
                );
                assert!(game.agenda_queue_after_event_scoring.is_some());
                checked_between_agendas = true;
            }
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
        let resolved = game
            .events
            .iter()
            .filter(|event| event.starts_with("AGENDA_RESOLVED:"))
            .count();
        assert!(resolved >= 1, "the fixture resolves at least one agenda");
        assert!(
            checked_between_agendas,
            "the fixture crossed an agenda boundary"
        );
        assert_eq!(
            game.state.feat_occurrence_seq - occurrence_before,
            u64::try_from(resolved).unwrap(),
            "each resolved agenda receives one distinct occurrence"
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
        // The round begins, and the strategy phase it opens with announces itself. Both, in that
        // order: cards read "at the start of the strategy phase" and must not fire on a round
        // transition that never reaches one.
        assert_eq!(
            game.events,
            vec![
                "AGENDA_PHASE_RESOLVED",
                "ROUND_BEGAN",
                "STRATEGY_PHASE_BEGAN"
            ]
        );
    }

    /// Build a two-seat agenda-phase game: both players hold influence planets (so the vote
    /// decides by exhausted ballots rather than the speaker's tie-break), `holder` starts with
    /// `card`, and the agenda deck holds `deck`. Drives the whole phase with the first-option
    /// table — which takes a reaction slot when one is offered and votes the first seat with
    /// every planet — and returns the final state and the event log.
    fn agenda_run(
        card: Option<(&'static str, PlayerId)>,
        deck: &[&'static str],
    ) -> (GameState, Vec<String>) {
        let content = ContentStore::embedded();
        let players = [PlayerId::new("a"), PlayerId::new("b")];
        let mut state = start_game(content, &players, POK, None).unwrap();
        state.phase = Phase::Agenda;
        state.custodians_removed = true;

        // Two influence planets per seat, so the first seat out-votes the second and the
        // election outcome is the first seat's.
        let catalogue = ti4_content::galaxy::all_planets(content, POK);
        let (mut for_a, mut for_b) = (0usize, 0usize);
        for (id, record) in &catalogue {
            if record.influence() > 0 && !record.is_placed_during_play() {
                let system = record.system_id().unwrap_or("18");
                if for_a < 2 {
                    state
                        .system_mut(&ti4_model::id::SystemId::new(system))
                        .set_control(ti4_model::id::PlanetId::new(*id), players[0].clone());
                    for_a += 1;
                } else if for_b < 2 {
                    state
                        .system_mut(&ti4_model::id::SystemId::new(system))
                        .set_control(ti4_model::id::PlanetId::new(*id), players[1].clone());
                    for_b += 1;
                }
            }
            if for_a >= 2 && for_b >= 2 {
                break;
            }
        }

        if let Some((alias, holder)) = card {
            state.player_mut(&holder).unwrap().action_cards =
                vec![ti4_model::id::ActionCardId::new(alias)];
        }
        state.agenda_deck = deck.iter().map(ToString::to_string).collect();

        let mut game = Game::new(state, content);
        let mut guard = 0;
        while game.state.phase == Phase::Agenda && guard < 150 {
            assert_eq!(game.step().error, None, "no agenda step should refuse");
            guard += 1;
        }
        (game.state, game.events.clone())
    }

    fn elected_by_an_agenda(state: &GameState, player: &PlayerId) -> bool {
        state.player(player).is_some_and(|seat| {
            seat.event_feats
                .iter()
                .any(|(feat, _)| *feat == ti4_model::state::Feat::ElectedByAnAgenda)
        })
    }

    #[test]
    fn veto_reveals_the_next_agenda_instead_of_the_vetoed_one() {
        // Veto: "Discard that agenda and reveal 1 agenda from the top of the deck. Players
        // vote on this agenda instead." The deck reveals secret and execution into the queue
        // and leaves prophecy behind them; a's Veto, played when secret is revealed, discards
        // secret and puts prophecy to the vote instead. The election's outcome is untouched —
        // only which agenda is decided changes. All three copies are the same card, so each is
        // driven end to end.
        let a = PlayerId::new("a");
        for copy in ["veto", "veto3", "veto4"] {
            let (state, events) = agenda_run(
                Some((copy, a.clone())),
                &["secret", "execution", "prophecy"],
            );

            assert!(
                events.iter().any(|e| e == "AGENDA_DISCARDED:secret"),
                "{copy}: the vetoed agenda is discarded; log {events:?}"
            );
            assert!(
                events
                    .iter()
                    .any(|e| e.starts_with("AGENDA_RESOLVED:prophecy:")),
                "{copy}: the replacement from the top of the deck is voted on; log {events:?}"
            );
            assert!(
                !events
                    .iter()
                    .any(|e| e.starts_with("AGENDA_RESOLVED:secret:")),
                "{copy}: the vetoed agenda is never put to a vote; log {events:?}"
            );
            assert!(
                !events
                    .iter()
                    .any(|e| e.starts_with("AGENDA_OUTCOME_REDIRECTED")),
                "{copy}: Veto changes which agenda is decided, not who is elected"
            );
            assert!(
                state.player(&a).unwrap().action_cards.is_empty(),
                "{copy}: the Veto is spent"
            );
            assert!(
                !events
                    .iter()
                    .any(|e| e.starts_with("AGENDA_EFFECT_UNRESOLVED:")),
                "{copy}: every revealed agenda still resolves"
            );
        }
    }

    #[test]
    fn confusing_redirects_the_election_to_a_chosen_seat() {
        // Confusing Legal Text: "When you are elected as the outcome of an agenda: choose 1
        // player. That player is the elected player instead." The ballots elect a; a's
        // Confusing, played into the resolution window, redirects the election to the one
        // other seat. The ballots' result (a) still stands in the log, but the elected-player
        // effect and the feat go to the chosen seat.
        let a = PlayerId::new("a");
        let b = PlayerId::new("b");
        let (state, events) = agenda_run(Some(("confusing", a.clone())), &["secret"]);

        assert!(
            events.iter().any(|e| e == "AGENDA_RESOLVED:secret:a"),
            "the ballots elected a; log {events:?}"
        );
        assert!(
            events.iter().any(|e| e == "AGENDA_OUTCOME_REDIRECTED:a:b"),
            "a's Confusing redirects the election to the other seat; log {events:?}"
        );
        assert!(
            elected_by_an_agenda(&state, &b),
            "the chosen seat is the one recorded as elected"
        );
        assert!(
            !elected_by_an_agenda(&state, &a),
            "the seat the ballots named is no longer the elected player"
        );
        assert!(
            state.player(&a).unwrap().action_cards.is_empty(),
            "the card is spent"
        );
    }

    #[test]
    fn confounding_is_silent_on_an_agenda_that_elects_no_player() {
        // The window is "when ANOTHER PLAYER is elected". An agenda that elects a planet —
        // `disarmament` — names a planet, not a seat, so Confounding must not be offered on it.
        // The guard reads the `elected_player` payload, which the driver sets only for a real
        // seat; a plain "outcome is not me" guard would match a planet (or a law, or "for")
        // against every chair and let the card fire on an agenda that elects no player.
        let b = PlayerId::new("b");
        let (state, events) = agenda_run(Some(("confounding", b.clone())), &["disarmament"]);

        assert!(
            events
                .iter()
                .any(|e| e.starts_with("AGENDA_RESOLVED:disarmament:")),
            "the planet agenda still votes and resolves; log {events:?}"
        );
        assert!(
            !events
                .iter()
                .any(|e| e.starts_with("AGENDA_OUTCOME_REDIRECTED")),
            "a planet was elected, not a player, so there is nothing to redirect; log {events:?}"
        );
        assert_eq!(
            state.player(&b).unwrap().action_cards,
            vec![ti4_model::id::ActionCardId::new("confounding")],
            "the never-offered Confounding is still in hand; log {events:?}"
        );
    }

    #[test]
    fn confounding_makes_the_holder_the_elected_player() {
        // Confounding Legal Text: "When another player is elected as the outcome of an agenda:
        // you are the elected player instead." The ballots elect a; b's Confounding, played
        // into the resolution window, takes the election for b. The window is silent on an
        // agenda that elects no player, so only a real other-player election offers it.
        let b = PlayerId::new("b");
        let (state, events) = agenda_run(Some(("confounding", b.clone())), &["secret"]);

        assert!(
            events.iter().any(|e| e == "AGENDA_RESOLVED:secret:a"),
            "the ballots elected a, another player from b's seat; log {events:?}"
        );
        assert!(
            events.iter().any(|e| e == "AGENDA_OUTCOME_REDIRECTED:a:b"),
            "b's Confounding takes the election for itself; log {events:?}"
        );
        assert!(
            elected_by_an_agenda(&state, &b),
            "the holder is recorded as the elected player"
        );
        assert!(
            state.player(&b).unwrap().action_cards.is_empty(),
            "the card is spent"
        );
    }

    // ---------- Agenda and turn flow, part 2: Deadly Plot / Coup d'Etat / Crisis / Master Plan ----------

    /// The Deadly Plot fixture: a two-player agenda phase. `a` holds Deadly Plot and backs
    /// outcome `b` with two influence-1 planets; `b` backs outcome `a` with two influence-2+
    /// planets, so `a` is always the outcome (4+ against 2) and `a`'s vote is always for the
    /// other one. Returns the state plus the planet ids the scripted vote answers need.
    fn deadly_plot_state() -> (GameState, Vec<String>, Vec<String>) {
        let content = ContentStore::embedded();
        let players = [PlayerId::new("a"), PlayerId::new("b")];
        let mut state = start_game(content, &players, POK, None).unwrap();
        state.phase = Phase::Agenda;
        state.custodians_removed = true;

        let catalogue = ti4_content::galaxy::all_planets(content, POK);
        let weak: Vec<(String, String)> = catalogue
            .iter()
            .filter(|(_, planet)| planet.influence() == 1 && !planet.is_placed_during_play())
            .take(2)
            .map(|(id, planet)| {
                (
                    id.to_string(),
                    planet.system_id().unwrap_or("18").to_owned(),
                )
            })
            .collect();
        let strong: Vec<(String, String)> = catalogue
            .iter()
            .filter(|(_, planet)| planet.influence() >= 2 && !planet.is_placed_during_play())
            .take(2)
            .map(|(id, planet)| {
                (
                    id.to_string(),
                    planet.system_id().unwrap_or("18").to_owned(),
                )
            })
            .collect();
        assert_eq!(weak.len(), 2, "the corpus has two influence-1 planets");
        assert_eq!(strong.len(), 2, "the corpus has two influence-2+ planets");

        let a = players[0].clone();
        let b = players[1].clone();
        for (id, system) in &weak {
            state
                .system_mut(&SystemId::new(system))
                .set_control(PlanetId::new(id), a.clone());
        }
        for (id, system) in &strong {
            state
                .system_mut(&SystemId::new(system))
                .set_control(PlanetId::new(id), b.clone());
        }
        state.player_mut(&a).unwrap().action_cards =
            vec![ti4_model::id::ActionCardId::new("deadly_plot")];
        state.agenda_deck = vec!["secret".to_owned()];

        // `votable_planets` offers in (system, planet) map order, so the scripted answers
        // must match that order, not the catalogue order the fixture picked in.
        let order = |pairs: &[(String, String)]| {
            let mut keyed: Vec<(String, String)> = pairs
                .iter()
                .map(|(planet, system)| (system.clone(), planet.clone()))
                .collect();
            keyed.sort();
            keyed
                .into_iter()
                .map(|(_, planet)| planet)
                .collect::<Vec<String>>()
        };
        (state, order(&weak), order(&strong))
    }

    #[test]
    fn deadly_plot_discards_the_agenda_when_the_holder_voted_otherwise() {
        // Deadly Plot: "If you voted for or predicted another outcome, discard the agenda
        // instead. The agenda is resolved with no effect and it is not replaced. Then,
        // exhaust all of your planets." `a` votes `b` with one of its two planets and keeps
        // the other ready; `b` votes `a` with both of its stronger ones (influence 4+
        // against 1), so the outcome is the side `a` did not back. `b` is never asked to
        // decline: a voter who runs out of votable planets settles straight on. The guard
        // passes, the agenda is spent on nothing — no effect, no payouts, no elected feat —
        // and the kept planet is exhausted by the card's tail.
        let (state, a_planets, b_planets) = deadly_plot_state();
        let table = Table::with_default(Box::new(Scripted::new([
            "a".to_owned(),
            b_planets[0].clone(),
            b_planets[1].clone(),
            "b".to_owned(),
            a_planets[0].clone(),
            "decline".to_owned(),
            "reaction:generic:AGENDA_RESOLVED:when".to_owned(),
        ])));
        let mut game = Game::with_table(state, ContentStore::embedded(), table);
        let mut guard = 0;
        while !game.events.iter().any(|e| e == "AGENDA_DISCARDED:secret") && guard < 100 {
            assert_eq!(game.step().error, None, "no agenda step should refuse");
            guard += 1;
        }
        let state = &game.state;
        let events = &game.events;

        assert!(
            events.iter().any(|e| e == "AGENDA_RESOLVED:secret:a"),
            "the vote resolved in favour of a; log {events:?}"
        );
        assert!(
            events.iter().any(|e| e == "AGENDA_DISCARDED:secret"),
            "the holder voted for the other outcome, so the agenda is discarded; log {events:?}"
        );
        assert!(
            !events
                .iter()
                .any(|e| e.starts_with("AGENDA_EFFECT_RESOLVED:secret")
                    || e.starts_with("AGENDA_EFFECT_UNRESOLVED:secret")
                    || e.starts_with("AGENDA_EFFECT_DEFERRED:secret")),
            "a discarded agenda resolves with no effect; log {events:?}"
        );
        assert!(
            !events
                .iter()
                .any(|e| e.starts_with("AGENDA_PREDICTION_CORRECT:")),
            "the payouts are suppressed with the effect; log {events:?}"
        );
        assert!(
            !elected_by_an_agenda(state, &PlayerId::new("a"))
                && !elected_by_an_agenda(state, &PlayerId::new("b")),
            "no seat is the elected player once the election is spent on nothing; log {events:?}"
        );
        assert!(
            state
                .player(&PlayerId::new("a"))
                .unwrap()
                .action_cards
                .is_empty(),
            "the plot was played and spent"
        );
        assert!(
            state
                .exhausted_planets
                .contains(&PlanetId::new(a_planets[0].as_str())),
            "the planet a voted with is exhausted by the vote itself"
        );
        assert!(
            state
                .exhausted_planets
                .contains(&PlanetId::new(a_planets[1].as_str())),
            "the planet a kept ready is exhausted by the card's tail; log {events:?}"
        );
    }

    #[test]
    fn deadly_plot_stays_silent_when_the_holder_backed_the_winner() {
        // The guard is "if you voted for or predicted another outcome". On the first-option
        // table both seats back the first candidate with all their planets, so the outcome
        // is the side a voted for: the guard fails, the card is never offered, and the
        // agenda resolves exactly as if the card were not there.
        let a = PlayerId::new("a");
        let (state, events) = agenda_run(Some(("deadly_plot", a.clone())), &["secret"]);

        assert!(
            events
                .iter()
                .any(|e| e.starts_with("AGENDA_RESOLVED:secret:")),
            "the agenda is voted on and resolves; log {events:?}"
        );
        assert!(
            !events.iter().any(|e| e == "AGENDA_DISCARDED:secret"),
            "a's vote matched the outcome, so there is nothing to discard; log {events:?}"
        );
        assert!(
            !events
                .iter()
                .any(|e| e.starts_with("AGENDA_EFFECT_UNRESOLVED:secret")),
            "the agenda's effect still runs; log {events:?}"
        );
        assert_eq!(
            state.player(&a).unwrap().action_cards,
            vec![ti4_model::id::ActionCardId::new("deadly_plot")],
            "the never-offered plot is still in hand; log {events:?}"
        );
    }

    /// The Coup d'Etat fixture: a two-player action phase with no map. `b` is active and
    /// holds exactly one strategy card (`diplomacy`), so the menu offers one strategic
    /// action; `a` holds the action card under test when `holder` is set. Diplomacy in a
    /// world with no planets under `b`'s control resolves without any questions.
    fn coup_state(holder: Option<PlayerId>) -> GameState {
        let content = ContentStore::embedded();
        let players = [PlayerId::new("a"), PlayerId::new("b")];
        let mut state = start_game(content, &players, POK, None).unwrap();
        state.phase = Phase::Action;
        state.active = Some(PlayerId::new("b"));
        let b = players[1].clone();
        state.player_mut(&b).unwrap().strategy_cards =
            vec![ti4_model::id::StrategyCardId::new("diplomacy")];
        if let Some(holder) = holder {
            state.player_mut(&holder).unwrap().action_cards =
                vec![ti4_model::id::ActionCardId::new("coup")];
        }
        state
    }

    #[test]
    fn coup_ends_the_turn_before_the_strategic_action_is_resolved() {
        // Coup d'Etat: "When another player would perform a strategic action: End that
        // player's turn, the strategic action is not resolved and the strategy card is not
        // exhausted." The typed STRATEGIC_ACTION_BEGAN event fires before the card's effect
        // runs, so the play lands in that gap: the card goes back in hand unexhausted, no
        // token is placed, and the turn simply moves on.
        let a = PlayerId::new("a");
        let b = PlayerId::new("b");
        let table = Table::with_default(Box::new(Scripted::new([
            "strategic".to_owned(),
            "reaction:generic:STRATEGIC_ACTION_BEGAN:when".to_owned(),
        ])));
        let mut game =
            Game::with_table(coup_state(Some(a.clone())), ContentStore::embedded(), table);
        let mut guard = 0;
        while !game
            .events
            .iter()
            .any(|e| e == "STRATEGIC_ACTION_CANCELLED:diplomacy")
            && guard < 100
        {
            assert_eq!(game.step().error, None, "no coup step should refuse");
            guard += 1;
        }
        let state = &game.state;
        let events = &game.events;

        assert!(
            events
                .iter()
                .any(|e| e == "STRATEGIC_ACTION_CANCELLED:diplomacy"),
            "the strategic action is cancelled; log {events:?}"
        );
        assert!(
            !events.iter().any(|e| e == "STRATEGIC_ACTION_COMPLETE"),
            "nothing completed: the action never resolved; log {events:?}"
        );
        let diplomacy = ti4_model::id::StrategyCardId::new("diplomacy");
        let victim = state.player(&b).unwrap();
        assert_eq!(
            victim.strategy_cards,
            vec![diplomacy.clone()],
            "the strategy card is still in hand"
        );
        assert!(
            !victim.exhausted_strategy_cards.contains(&diplomacy),
            "and it is not exhausted; log {events:?}"
        );
        assert!(
            victim
                .unused_strategy_cards()
                .iter()
                .any(|card| card.as_str() == "diplomacy"),
            "the cancelled action leaves the card fully usable for the next turn"
        );
        assert!(
            state.player(&a).unwrap().action_cards.is_empty(),
            "the coup was spent"
        );
        assert_eq!(
            state.active.clone(),
            Some(PlayerId::new("a")),
            "the turn ended and moved on to the next seat"
        );
    }

    #[test]
    fn without_a_coup_the_same_strategic_action_completes() {
        // The control for the coup test: with no coup in hand, b's diplomacy runs to
        // completion — the card is exhausted and removed from hand, and the turn passes on
        // only after the action has resolved.
        let b = PlayerId::new("b");
        let table = Table::with_default(Box::new(Scripted::new(["strategic".to_owned()])));
        let mut game = Game::with_table(coup_state(None), ContentStore::embedded(), table);
        let mut guard = 0;
        while !game.events.iter().any(|e| e == "STRATEGIC_ACTION_COMPLETE") && guard < 100 {
            assert_eq!(game.step().error, None, "no strategic step should refuse");
            guard += 1;
        }
        let state = &game.state;
        let events = &game.events;

        assert!(
            events.iter().any(|e| e == "STRATEGIC_ACTION_COMPLETE"),
            "the action resolved; log {events:?}"
        );
        assert!(
            !events
                .iter()
                .any(|e| e.starts_with("STRATEGIC_ACTION_CANCELLED")),
            "nothing was cancelled; log {events:?}"
        );
        let diplomacy = ti4_model::id::StrategyCardId::new("diplomacy");
        let victim = state.player(&b).unwrap();
        assert!(
            victim.exhausted_strategy_cards.contains(&diplomacy),
            "the resolved action exhausted its card"
        );
        assert!(
            victim.unused_strategy_cards().is_empty(),
            "an exhausted card is no longer a usable action; log {events:?}"
        );
        assert_eq!(
            state.active.clone(),
            Some(PlayerId::new("a")),
            "the turn passed on; log {events:?}"
        );
    }

    /// The Crisis fixture: an action phase; the first seat is active and holds the card
    /// under test when `holder` is set. No map, so the only thing any seat can do is pass.
    fn crisis_state(holder: Option<PlayerId>, players: &[PlayerId]) -> GameState {
        let content = ContentStore::embedded();
        let mut state = start_game(content, players, POK, None).unwrap();
        state.phase = Phase::Action;
        state.active = Some(players[0].clone());
        if let Some(holder) = holder {
            state.player_mut(&holder).unwrap().action_cards =
                vec![ti4_model::id::ActionCardId::new("crisis")];
        }
        state
    }

    #[test]
    fn crisis_skips_the_next_players_turn() {
        // Crisis: "At the end of any player's turn, if there are at least 2 players who have
        // not passed: Skip the next player's turn." a passes while b and c are both unpassed,
        // so the guard counts two; the play arms the skip, which lands on b — b never takes
        // a turn, and c is the next seat to act. A skipped turn is not a turn: no end-of-turn
        // window of its own.
        let a = PlayerId::new("a");
        let b = PlayerId::new("b");
        let c = PlayerId::new("c");
        let table = Table::with_default(Box::new(Scripted::new([
            "pass".to_owned(),
            "reaction:generic:TURN_PASSED:after".to_owned(),
        ])));
        let mut game = Game::with_table(
            crisis_state(Some(a.clone()), &[a.clone(), b.clone(), c.clone()]),
            ContentStore::embedded(),
            table,
        );
        let mut guard = 0;
        while !game.events.iter().any(|e| e == "TURN_SKIPPED:b") && guard < 100 {
            assert_eq!(game.step().error, None, "no crisis step should refuse");
            guard += 1;
        }
        let state = &game.state;
        let events = &game.events;

        assert!(
            events.iter().any(|e| e == "TURN_SKIPPED:b"),
            "the seat after the one that passed is skipped; log {events:?}"
        );
        assert_eq!(
            state.active.clone(),
            Some(c.clone()),
            "the turn landed on the seat after the skipped one; log {events:?}"
        );
        assert!(
            state.player(&a).unwrap().action_cards.is_empty(),
            "the card was spent"
        );
        let skipped = state.player(&b).unwrap();
        assert_eq!(
            skipped.tactic_tokens, 3,
            "the skipped seat never took a turn, so it spent nothing; log {events:?}"
        );
        assert!(
            !skipped.passed,
            "the skipped seat never passed either; log {events:?}"
        );
        assert!(
            !events.iter().any(|e| e == "TACTICAL_ACTION_BEGAN"),
            "the skipped seat performed no action; log {events:?}"
        );
    }

    #[test]
    fn crisis_never_fires_when_fewer_than_two_players_have_not_passed() {
        // With two seats, the moment one has passed, the other is the only seat left that
        // has not passed — so the "at least 2 players who have not passed" guard can never
        // be met at either turn ending. The window is never offered and the card stays in
        // hand through both passes.
        let a = PlayerId::new("a");
        let b = PlayerId::new("b");
        let table = Table::with_default(Box::new(Scripted::new([
            "pass".to_owned(),
            "pass".to_owned(),
        ])));
        let mut game = Game::with_table(
            crisis_state(Some(a.clone()), &[a.clone(), b.clone()]),
            ContentStore::embedded(),
            table,
        );
        let mut guard = 0;
        while !(game.state.player(&a).unwrap().passed && game.state.player(&b).unwrap().passed)
            && guard < 100
        {
            assert_eq!(game.step().error, None, "no pass step should refuse");
            guard += 1;
        }
        let state = &game.state;
        let events = &game.events;

        assert!(
            !events.iter().any(|e| e.starts_with("TURN_SKIPPED")),
            "with fewer than two unpassed players the guard never passes; log {events:?}"
        );
        assert_eq!(
            state.player(&a).unwrap().action_cards,
            vec![ti4_model::id::ActionCardId::new("crisis")],
            "the never-offered crisis is still in hand; log {events:?}"
        );
        assert!(
            state.player(&a).unwrap().passed && state.player(&b).unwrap().passed,
            "both seats passed regardless; log {events:?}"
        );
    }

    #[test]
    fn master_plan_grants_an_additional_action_on_the_same_turn() {
        // Master Plan: "After you perform an action: Perform an additional action." The
        // retention is the same turn — no turn-sequence bump, no end-of-turn tech, no
        // transaction reset — so a with two tactic tokens plays two tactical actions before
        // the turn ever leaves the seat.
        let (mut state, galaxy, ids) = tactical_fixture();
        let a = PlayerId::new("a");
        let seat = state.player_mut(&a).unwrap();
        seat.tactic_tokens = 2;
        seat.action_cards = vec![ti4_model::id::ActionCardId::new("master_plan")];
        let table = Table::with_default(Box::new(Scripted::new([
            "tactical".to_owned(),
            ids[0].to_string(),
            "done_moving".to_owned(),
            "reaction:generic:ACTION_COMPLETED:after".to_owned(),
            "tactical".to_owned(),
            ids[1].to_string(),
            "done_moving".to_owned(),
        ])));
        let mut game = Game::with_table(state, ContentStore::embedded(), table).with_galaxy(galaxy);
        let mut guard = 0;
        while game.state.active.as_ref().is_some_and(|p| p == &a) && guard < 100 {
            assert_eq!(game.step().error, None, "no action step should refuse");
            guard += 1;
        }
        let state = &game.state;
        let events = &game.events;

        assert_eq!(
            events
                .iter()
                .filter(|e| *e == "TACTICAL_ACTION_BEGAN")
                .count(),
            2,
            "the seat took two actions before the turn moved; log {events:?}"
        );
        assert!(
            events.iter().any(|e| e == "TURN_RETAINED"),
            "the turn was kept, not passed; log {events:?}"
        );
        assert!(
            state.player(&a).unwrap().action_cards.is_empty(),
            "the plan was spent on the first action"
        );
        assert_eq!(
            state.player(&a).unwrap().tactic_tokens,
            0,
            "both actions were paid for"
        );
        assert_eq!(
            state.active.clone(),
            Some(PlayerId::new("b")),
            "only after the extra action did the turn pass on; log {events:?}"
        );
    }

    #[test]
    fn without_master_plan_the_turn_passes_after_one_action() {
        // The control: the same seat and tokens, no card in hand. One tactical action is all
        // the turn buys — after it completes the turn moves on, and the second token stays
        // in the pool.
        let (mut state, galaxy, ids) = tactical_fixture();
        let a = PlayerId::new("a");
        state.player_mut(&a).unwrap().tactic_tokens = 2;
        let table = Table::with_default(Box::new(Scripted::new([
            "tactical".to_owned(),
            ids[0].to_string(),
            "done_moving".to_owned(),
        ])));
        let mut game = Game::with_table(state, ContentStore::embedded(), table).with_galaxy(galaxy);
        let mut guard = 0;
        while game.state.active.as_ref().is_some_and(|p| p == &a) && guard < 100 {
            assert_eq!(game.step().error, None, "no action step should refuse");
            guard += 1;
        }
        let events = &game.events;

        assert_eq!(
            events
                .iter()
                .filter(|e| *e == "TACTICAL_ACTION_BEGAN")
                .count(),
            1,
            "one action per turn without the card; log {events:?}"
        );
        assert!(
            !events.iter().any(|e| e == "TURN_RETAINED"),
            "nothing kept the turn; log {events:?}"
        );
        assert_eq!(
            game.state.player(&a).unwrap().tactic_tokens,
            1,
            "the second token is still in the pool; log {events:?}"
        );
        assert_eq!(
            game.state.active.clone(),
            Some(PlayerId::new("b")),
            "the turn passed on after the single action; log {events:?}"
        );
    }

    #[test]
    fn the_end_of_action_window_opens_after_a_component_action() {
        // LRR 3.1 and note 2: the end-of-action window ("after you perform an action")
        // fires after ANY action, including component actions, before the end-of-turn
        // effects. Master Plan: "After you perform an action: Perform an additional
        // action." The first action here is the component action Economic Initiative
        // ("Ready each cultural planet you control" -- no choices), so the window must
        // open around it exactly as it does around a tactical action: the seat then
        // spends the grant on a tactical move, and only after that does the turn leave.
        let (mut state, galaxy, ids) = tactical_fixture();
        let a = PlayerId::new("a");
        let b = PlayerId::new("b");
        let seat = state.player_mut(&a).unwrap();
        seat.tactic_tokens = 1;
        seat.action_cards = vec![
            ti4_model::id::ActionCardId::new("economic_initiative"),
            ti4_model::id::ActionCardId::new("master_plan"),
        ];
        // b holds nothing that could react to the play's announcement or the turn's start.
        state.player_mut(&b).unwrap().action_cards = vec![];
        let table = Table::with_default(Box::new(Scripted::new([
            "action_card|0".to_owned(),
            "reaction:generic:ACTION_COMPLETED:after".to_owned(),
            "tactical".to_owned(),
            ids[0].to_string(),
            "done_moving".to_owned(),
        ])));
        let mut game = Game::with_table(state, ContentStore::embedded(), table).with_galaxy(galaxy);
        let mut guard = 0;
        while game.state.active.as_ref().is_some_and(|p| p == &a) && guard < 100 {
            assert_eq!(game.step().error, None, "no action step should refuse");
            guard += 1;
        }
        let events = &game.events;

        let position = |name: &str| events.iter().position(|e| e == name);
        let component = position("COMPONENT_ACTION_RESOLVED");
        let completed = position("ACTION_COMPLETED");
        let retained = position("TURN_RETAINED");
        let tactical = position("TACTICAL_ACTION_BEGAN");
        for (name, at) in [
            ("COMPONENT_ACTION_RESOLVED", component),
            ("ACTION_COMPLETED", completed),
            ("TURN_RETAINED", retained),
            ("TACTICAL_ACTION_BEGAN", tactical),
        ] {
            assert!(at.is_some(), "{name} fired; log {events:?}");
        }
        assert!(
            component.unwrap() < completed.unwrap()
                && completed.unwrap() < retained.unwrap()
                && retained.unwrap() < tactical.unwrap(),
            "component, end-of-action, retention, then the extra action; log {events:?}"
        );
        assert_eq!(
            game.state.player(&a).unwrap().action_cards,
            Vec::<ti4_model::id::ActionCardId>::new(),
            "both cards were spent: the initiative as the action, the plan in the window it opened"
        );
        assert_eq!(
            game.state.active.clone(),
            Some(PlayerId::new("b")),
            "the turn left only after the extra action the plan granted; log {events:?}"
        );
    }

    #[test]
    fn a_canceled_component_action_does_not_use_the_players_action() {
        // LRR 22.4: if a component action is canceled, that player's action is not used.
        // a's action is the component card Economic Initiative; b's Sabotage cancels the
        // play while it is announced. The played card is spent anyway (a spent play is
        // not undone), but the turn must be re-offered to a instead of advancing: a still
        // performs an action -- here a tactical move -- before the turn finally leaves.
        let (mut state, galaxy, ids) = tactical_fixture();
        let a = PlayerId::new("a");
        let b = PlayerId::new("b");
        let seat = state.player_mut(&a).unwrap();
        seat.tactic_tokens = 1;
        seat.action_cards = vec![ti4_model::id::ActionCardId::new("economic_initiative")];
        state.player_mut(&b).unwrap().action_cards =
            vec![ti4_model::id::ActionCardId::new("sabo1")];
        let table = Table::with_default(Box::new(Scripted::new([
            "action_card|0".to_owned(),
            "reaction:generic:ACTION_CARD_PLAYED:when".to_owned(),
            "tactical".to_owned(),
            ids[0].to_string(),
            "done_moving".to_owned(),
        ])));
        let mut game = Game::with_table(state, ContentStore::embedded(), table).with_galaxy(galaxy);
        let mut guard = 0;
        while game.state.active.as_ref().is_some_and(|p| p == &a) && guard < 100 {
            assert_eq!(game.step().error, None, "no action step should refuse");
            guard += 1;
        }
        let events = &game.events;

        let failed = events.iter().position(|e| e == "COMPONENT_ACTION_FAILED");
        let tactical = events.iter().position(|e| e == "TACTICAL_ACTION_BEGAN");
        assert!(
            failed.is_some(),
            "the canceled play was recorded as not resolved; log {events:?}"
        );
        assert!(
            tactical.is_some(),
            "the same turn re-offered after the cancellation: a still had an action to take; log {events:?}"
        );
        assert!(
            failed.unwrap() < tactical.unwrap(),
            "the cancellation happened before a's real action; log {events:?}"
        );
        assert_eq!(
            game.state.player(&a).unwrap().action_cards,
            Vec::<ti4_model::id::ActionCardId>::new(),
            "the canceled card was spent even though its play was canceled"
        );
        assert_eq!(
            game.state.player(&b).unwrap().action_cards,
            Vec::<ti4_model::id::ActionCardId>::new(),
            "the Sabotage that answered the play was spent too"
        );
        assert_eq!(
            game.state.active.clone(),
            Some(PlayerId::new("b")),
            "the turn left only after a's second action ended; log {events:?}"
        );
    }

    #[test]
    fn the_activator_may_invade_when_seated_second_and_still_holding() {
        // LRR 49: the invasion step of a tactical action happens when the active player
        // -- the one who activated the system -- still holds the space after any combat.
        // The gate must test the activator's membership among the survivors, not "who is
        // seated first of them": here b activates a system where both players still keep
        // ships, and b is seated second, so a `first()` reading sends the turn straight
        // to production instead of the invasion step.
        let run = |active_seat: PlayerId| -> Vec<String> {
            let (mut state, galaxy, ids) = tactical_fixture();
            let a = PlayerId::new("a");
            let b = PlayerId::new("b");
            // The combat must run out all its rounds with both fleets still in the
            // system -- the only way the `first()` reading of LRR 49 can be wrong: both
            // players hold the space, and the activator is seated behind the other. So
            // both hands are emptied (no card can sustain, cancel or reroll anything),
            // both seats hold enough fleet tokens for the whole fleet, and the fleets
            // are shaped against the dice: the opponent's winnu flagships roll no dice
            // at all, so they never hurt the activator, yet each one sustains the
            // activator's hits -- eighty of them outlast the activator's two flagships
            // over all fifty combat rounds, and neither fleet is ever emptied.
            let active = active_seat.clone();
            let opponent = if active == a { b.clone() } else { a.clone() };
            for seat in [&a, &b] {
                state.player_mut(seat).unwrap().action_cards = Vec::new();
                state.player_mut(seat).unwrap().fleet_tokens = 200;
            }
            for _ in 0..2 {
                crate::fixtures::put(&mut state, &ids[0], "flagship", &active, 1);
            }
            for _ in 0..80 {
                crate::fixtures::put(&mut state, &ids[0], "winnu_flagship", &opponent, 1);
            }
            state.player_mut(&active_seat).unwrap().tactic_tokens = 1;
            state.active = Some(active_seat);
            // The queue covers the two tactical answers; FirstOption answers the rest
            // (the retreat announcements, the hit assignments, the production step).
            let table = Table::with_default(Box::new(Scripted::new([
                "tactical".to_owned(),
                ids[0].to_string(),
            ])));
            let mut game =
                Game::with_table(state, ContentStore::embedded(), table).with_galaxy(galaxy);
            for _ in 0..1000 {
                let result = game.step();
                assert_eq!(result.error, None, "no tactical step should refuse");
                if game.events.iter().any(|e| e == "TACTICAL_ACTION_COMPLETE") {
                    break;
                }
            }
            game.events
        };

        // b activates, and b is seated second: the invasion step opens anyway.
        let events = run(PlayerId::new("b"));
        assert!(
            events.iter().any(|e| e == "INVASION_BEGAN"),
            "the activator still holds the system, seated second or not; log {events:?}"
        );

        // The control: the same game with the activator seated first -- the case the old
        // `first()` reading happened to get right.
        let events = run(PlayerId::new("a"));
        assert!(
            events.iter().any(|e| e == "INVASION_BEGAN"),
            "seated-first still invades; log {events:?}"
        );
    }

    /// A decider that answers by rule rather than by queue, so a test can drive whole
    /// rounds and script only the one window play it is about. The rules:
    ///
    /// * a timing window — the one scripted play, if it is addressed to this seat for
    ///   this event; a play the test expects but is never offered fails loudly (the
    ///   window that should have opened did not), and anything else declines;
    /// * the action phase — the offered `action_card|0` when the mode says so,
    ///   otherwise the seat's strategic action when the mode says so, otherwise a pass;
    /// * everything else (the strategy draft, the token gains, a strategic sub-flow) —
    ///   the first option, which the test's assertions pin down.
    #[derive(Clone)]
    struct TurnDecider {
        /// Plays to make, in order, as `(seat, event, play option id)`.
        plays: Vec<(String, String, String)>,
        /// The action phase plays the offered action card instead of passing.
        prefer_action_card: bool,
        /// The action phase takes the first option, the seat's strategic action.
        prefer_first: bool,
    }

    impl TurnDecider {
        fn new(plays: &[(&str, &str, &str)]) -> Self {
            Self {
                plays: plays
                    .iter()
                    .map(|&(a, b, c)| (a.to_owned(), b.to_owned(), c.to_owned()))
                    .collect(),
                prefer_action_card: false,
                prefer_first: false,
            }
        }

        /// The action phase plays the offered action card instead of passing.
        fn playing_the_action_card(mut self) -> Self {
            self.prefer_action_card = true;
            self
        }

        /// The action phase takes the seat's strategic action instead of passing.
        fn taking_the_strategic_action(mut self) -> Self {
            self.prefer_first = true;
            self
        }
    }

    impl Decider for TurnDecider {
        fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
            let offered = choice
                .options
                .iter()
                .map(|option| option.id.clone())
                .collect::<Vec<_>>();
            let window = choice.prompt.starts_with("when ") || choice.prompt.starts_with("after ");
            if window
                && self.plays.first().is_some_and(|(seat, event, _)| {
                    choice.player.as_str() == seat && choice.prompt.ends_with(event)
                })
            {
                let (.., id) = self.plays.remove(0);
                return choice
                    .options
                    .iter()
                    .find(|option| option.id == id)
                    .cloned()
                    .ok_or_else(|| IllegalChoice::ScriptDiverged {
                        player: choice.player.clone(),
                        wanted: id,
                        offered,
                    });
            }
            if window {
                if let Some(decline) = choice
                    .options
                    .iter()
                    .find(|option| option.is_decline())
                    .cloned()
                {
                    return Ok(decline);
                }
            } else if choice.prompt == "gain a command token into which pool" {
                // Every token the test's play gains is named into the fleet pool, so a
                // card's gain is worth exactly its tokens.
                if let Some(pool) = choice
                    .options
                    .iter()
                    .find(|option| option.id == "fleet_tokens")
                    .cloned()
                {
                    return Ok(pool);
                }
            } else if choice.prompt == "action phase" {
                if self.prefer_action_card
                    && let Some(card) = choice
                        .options
                        .iter()
                        .find(|option| option.id == "action_card|0")
                        .cloned()
                {
                    return Ok(card);
                }
                if !self.prefer_first {
                    // The pass is the phase's decline option.
                    if let Some(pass) = choice
                        .options
                        .iter()
                        .find(|option| option.is_decline())
                        .cloned()
                    {
                        return Ok(pass);
                    }
                }
            }
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

    #[test]
    fn summit_gains_two_command_tokens_at_the_start_of_the_strategy_phase() {
        // Summit: "At the start of the strategy phase: Gain 2 command tokens." a holds
        // it in the starting hand, so it plays in the one-time window that announces
        // round one's strategy phase, before that phase's first draft choice. Both
        // games below are the same seeded game — they draft the same eight-card mat
        // and take the same forced strategic actions, so the only difference the runs
        // can show is the card's: two fleet tokens (the decider names every gained
        // token into the fleet pool, card tokens included) and a spent card.
        let a = PlayerId::new("a");
        let b = PlayerId::new("b");
        let content = ContentStore::embedded();

        // The arm: a Summit in the starting hand.
        let mut state = start_game(content, &[a.clone(), b.clone()], POK, None).unwrap();
        state
            .player_mut(&a)
            .unwrap()
            .action_cards
            .push(ti4_model::id::ActionCardId::new("summit"));
        let table = Table::with_default(Box::new(TurnDecider::new(&[(
            "a",
            "STRATEGY_PHASE_BEGAN",
            "reaction:generic:STRATEGY_PHASE_BEGAN:after",
        )])));
        let mut arm = Game::with_table(state, content, table);
        arm.run(1, 4000).unwrap();

        // The control: the same game without the card. Its window is never offered —
        // nobody can play into it — so it is the arm's baseline.
        let state = start_game(content, &[a.clone(), b.clone()], POK, None).unwrap();
        let table = Table::with_default(Box::new(TurnDecider::new(&[])));
        let mut control = Game::with_table(state, content, table);
        control.run(1, 4000).unwrap();

        let pools = |game: &Game<'_>| {
            let seat = game.state.player(&a).unwrap();
            (seat.tactic_tokens, seat.fleet_tokens, seat.strategic_tokens)
        };
        let (arm_tactic, arm_fleet, arm_strategic) = pools(&arm);
        let (control_tactic, control_fleet, control_strategic) = pools(&control);

        assert_eq!(
            arm_fleet,
            control_fleet + 2,
            "Summit is worth exactly two tokens, both named into the fleet pool; arm log {:?} / control log {:?}",
            arm.events,
            control.events
        );
        assert_eq!(
            (arm_tactic, arm_strategic),
            (control_tactic, control_strategic),
            "the card touched only the pool its gain named; arm log {:?} / control log {:?}",
            arm.events,
            control.events
        );
        // The play itself: an action card played in the one-time window that
        // announces round one's strategy phase — before that phase's first draft
        // choice — in the arm and in no one's hand in the control.
        let played_at_phase_start = |game: &Game<'_>| {
            game.events
                .iter()
                .take_while(|event| **event != "STRATEGY_CARD_CHOSEN")
                .any(|event| *event == "ACTION_CARD_PLAYED")
        };
        assert!(
            played_at_phase_start(&arm),
            "Summit played into the window that announces round one's strategy phase; arm log {:?}",
            arm.events
        );
        assert!(
            !played_at_phase_start(&control),
            "the control's window was never offered — nobody held a card to play into it; control log {:?}",
            control.events
        );
    }

    #[test]
    fn political_stability_keeps_the_cards_and_skips_the_next_draft() {
        // Political Stability: "When you would return your strategy card(s) during the
        // status phase: Do not return your strategy card(s). You do not choose strategy
        // cards during the next strategy phase." a plays it in round one's status
        // phase: a keeps the two cards the draft just dealt and returns them only in
        // the next round's status phase, and round two's draft deals around a. With
        // eight cards on the mat and six picks, skipping one seat leaves four cards
        // unclaimed where a full draft would leave two.
        let a = PlayerId::new("a");
        let b = PlayerId::new("b");
        let c = PlayerId::new("c");
        let mut state = start_game(
            ContentStore::embedded(),
            &[a.clone(), b.clone(), c.clone()],
            POK,
            None,
        )
        .unwrap();
        state
            .player_mut(&a)
            .unwrap()
            .action_cards
            .push(ti4_model::id::ActionCardId::new("stability"));
        let table = Table::with_default(Box::new(TurnDecider::new(&[(
            "a",
            "STRATEGY_CARDS_WOULD_RETURN",
            "reaction:generic:STRATEGY_CARDS_WOULD_RETURN:when",
        )])));
        let mut game = Game::with_table(state, ContentStore::embedded(), table);
        game.run(1, 4000).unwrap();

        // Round one ends with a's marker set and its cards kept, the other seats empty.
        {
            let state = &game.state;
            assert!(
                state.player(&a).unwrap().stability,
                "the marker is set at the status phase it fired in; log {:?}",
                game.events
            );
            assert_eq!(
                state.player(&a).unwrap().strategy_cards.len(),
                2,
                "a kept the cards the round dealt it; log {:?}",
                game.events
            );
            assert!(
                state.player(&b).unwrap().strategy_cards.is_empty(),
                "b returned its cards; log {:?}",
                game.events
            );
            assert!(
                state.player(&c).unwrap().strategy_cards.is_empty(),
                "c returned its cards; log {:?}",
                game.events
            );
        }

        // Round two's draft deals around a, and its status phase returns everything,
        // the retained cards included.
        game.run(1, 4000).unwrap();
        let state = &game.state;
        assert!(
            !state.player(&a).unwrap().stability,
            "the marker is spent by the action phase it skipped; log {:?}",
            game.events
        );
        assert!(
            state
                .players
                .iter()
                .all(|player| player.strategy_cards.is_empty()),
            "round two's status returned every card, the retained ones included; log {:?}",
            game.events
        );
        // The mat is re-dealt at the round boundary, so the skip shows up in the
        // draft's pick count: round one dealt six picks, round two's draft dealt
        // four around the marked seat where a full draft would have dealt six.
        let picks = game
            .events
            .iter()
            .filter(|event| **event == "STRATEGY_CARD_CHOSEN")
            .count();
        assert_eq!(
            picks, 10,
            "round two's draft skipped the marked seat: six plus four picks; a full draft would have made twelve; log {:?}",
            game.events
        );
    }

    #[test]
    fn without_political_stability_every_seat_returns_and_drafts() {
        // The control: the same three-seat game, no card played. Every seat returns its
        // cards at the status phase and every seat drafts again, so round two's six
        // picks from the eight-card mat leave two unclaimed, not four.
        let a = PlayerId::new("a");
        let b = PlayerId::new("b");
        let c = PlayerId::new("c");
        let state = start_game(
            ContentStore::embedded(),
            &[a.clone(), b.clone(), c.clone()],
            POK,
            None,
        )
        .unwrap();
        let table = Table::with_default(Box::new(TurnDecider::new(&[])));
        let mut game = Game::with_table(state, ContentStore::embedded(), table);
        game.run(1, 4000).unwrap();
        game.run(1, 4000).unwrap();

        let state = &game.state;
        assert!(
            state
                .players
                .iter()
                .all(|player| player.strategy_cards.is_empty()),
            "every seat returned its cards; log {:?}",
            game.events
        );
        // Both drafts dealt the full six picks.
        let picks = game
            .events
            .iter()
            .filter(|event| **event == "STRATEGY_CARD_CHOSEN")
            .count();
        assert_eq!(
            picks, 12,
            "every seat drafted in both rounds: twelve picks; log {:?}",
            game.events
        );
    }

    #[test]
    fn public_disgrace_puts_the_pickers_choice_back_on_the_mat() {
        // Public Disgrace: "When another player chooses a strategy card during the
        // strategy phase: That player must choose a different strategy card instead, if
        // able." b plays it as a makes the draft's first choice: a's card goes back to
        // the mat, a re-chooses — the decider takes the first card the re-choice
        // offers, the second from the top of the mat — and the draft completes around
        // it. a never keeps its first pick, and the displaced card ends the draft on
        // the mat: six picks from eight leave two unclaimed, the displaced one among
        // them.
        let a = PlayerId::new("a");
        let b = PlayerId::new("b");
        let c = PlayerId::new("c");
        let mut state = start_game(
            ContentStore::embedded(),
            &[a.clone(), b.clone(), c.clone()],
            POK,
            None,
        )
        .unwrap();
        let first_pick = state.unclaimed_strategy_cards[0].clone();
        let re_choice = state.unclaimed_strategy_cards[1].clone();
        state
            .player_mut(&b)
            .unwrap()
            .action_cards
            .push(ti4_model::id::ActionCardId::new("disgrace"));
        let table = Table::with_default(Box::new(TurnDecider::new(&[(
            "b",
            "STRATEGY_CARD_CHOSEN",
            "reaction:generic:STRATEGY_CARD_CHOSEN:after",
        )])));
        let mut game = Game::with_table(state, ContentStore::embedded(), table);
        let mut guard = 0;
        while game.state.phase == Phase::Strategy && guard < 100 {
            assert_eq!(game.step().error, None, "no draft step should refuse");
            guard += 1;
        }

        let state = &game.state;
        let hand = state.player(&a).unwrap().strategy_cards.clone();
        assert!(
            !hand.contains(&first_pick),
            "a's first choice went back to the mat; log {:?}",
            game.events
        );
        assert!(
            hand.contains(&re_choice),
            "the re-choice took effect; log {:?}",
            game.events
        );
        assert!(
            state.unclaimed_strategy_cards.contains(&first_pick),
            "the displaced card ended the draft on the mat; log {:?}",
            game.events
        );
    }

    #[test]
    fn without_public_disgrace_the_draft_keeps_the_first_choice() {
        // The control: the same draft, no card played. a's first pick is final and
        // stays in a's hand at the end of the phase.
        let a = PlayerId::new("a");
        let b = PlayerId::new("b");
        let c = PlayerId::new("c");
        let state = start_game(
            ContentStore::embedded(),
            &[a.clone(), b.clone(), c.clone()],
            POK,
            None,
        )
        .unwrap();
        let first_pick = state.unclaimed_strategy_cards[0].clone();
        let table = Table::with_default(Box::new(TurnDecider::new(&[])));
        let mut game = Game::with_table(state, ContentStore::embedded(), table);
        let mut guard = 0;
        while game.state.phase == Phase::Strategy && guard < 100 {
            assert_eq!(game.step().error, None, "no draft step should refuse");
            guard += 1;
        }

        let hand = game.state.player(&a).unwrap().strategy_cards.clone();
        assert!(
            hand.contains(&first_pick),
            "an unchallenged first choice stands; log {:?}",
            game.events
        );
    }

    #[test]
    fn puppets_on_a_string_gives_the_passer_one_fresh_action_turn() {
        // Puppets on a String: "At the end of a player's turn, if you have passed:
        // Perform 1 action." a holds it and passes: the turn comes back to a as a fresh
        // turn — a new turn sequence, so start-of-turn hooks run again — and the seat
        // stays passed: the grant is one action, not a return from pass.
        let a = PlayerId::new("a");
        let b = PlayerId::new("b");
        let mut state =
            start_game(ContentStore::embedded(), &[a.clone(), b.clone()], POK, None).unwrap();
        state.phase = Phase::Action;
        state.active = Some(a.clone());
        state
            .player_mut(&a)
            .unwrap()
            .action_cards
            .push(ti4_model::id::ActionCardId::new("puppetsonastring"));
        let table = Table::with_default(Box::new(TurnDecider::new(&[(
            "a",
            "PLAYER_PASSED",
            "reaction:generic:PLAYER_PASSED:after",
        )])));
        let mut game = Game::with_table(state, ContentStore::embedded(), table);
        let mut guard = 0;
        while !game.events.iter().any(|e| e == "TURN_PUPPET:a") && guard < 50 {
            assert_eq!(game.step().error, None, "no turn step should refuse");
            guard += 1;
        }

        let state = &game.state;
        let events = &game.events;
        assert!(
            events.iter().any(|e| e == "TURN_PUPPET:a"),
            "the passed turn came back to the passer; log {events:?}"
        );
        assert_eq!(
            state.turn_seq, 2,
            "a's first turn and the puppet's: the returned turn is fresh, so its sequence moved; log {events:?}"
        );
        assert_eq!(
            state.active.clone(),
            Some(a.clone()),
            "the turn sits back on the passer while it spends its action; log {events:?}"
        );
        assert!(
            state.player(&a).unwrap().passed,
            "the grant is one action, not a return from pass; log {events:?}"
        );
        assert!(
            state.player(&a).unwrap().action_cards.is_empty(),
            "the card was spent playing its window; log {events:?}"
        );
    }

    #[test]
    fn without_puppets_a_pass_is_final_until_the_phase_ends() {
        // The control: the same seat and turn, no card in hand. A pass is a pass: the
        // turn moves on to the next seat and never comes back during the phase.
        let a = PlayerId::new("a");
        let b = PlayerId::new("b");
        let mut state =
            start_game(ContentStore::embedded(), &[a.clone(), b.clone()], POK, None).unwrap();
        state.phase = Phase::Action;
        state.active = Some(a.clone());
        let table = Table::with_default(Box::new(TurnDecider::new(&[])));
        let mut game = Game::with_table(state, ContentStore::embedded(), table);
        let mut guard = 0;
        while game.state.active.as_ref().is_some_and(|p| p == &a) && guard < 50 {
            assert_eq!(game.step().error, None, "no turn step should refuse");
            guard += 1;
        }

        let state = &game.state;
        assert!(
            state.player(&a).unwrap().passed,
            "the pass stuck; log {:?}",
            game.events
        );
        assert!(
            !game.events.iter().any(|e| e == "TURN_PUPPET:a"),
            "nothing returned the turn; log {:?}",
            game.events
        );
        assert_eq!(
            state.active.clone(),
            Some(b.clone()),
            "the turn moved on after a single pass; log {:?}",
            game.events
        );
        assert_eq!(
            state.turn_seq, 1,
            "one turn has passed; log {:?}",
            game.events
        );
    }

    #[test]
    fn extreme_duress_punishes_the_first_nonstrategic_action() {
        // Extreme Duress: "At the start of another player's turn, if they have a
        // readied strategy card: If that player's next action is not a strategic
        // action, they discard all of their action cards, give you all of their trade
        // goods, and show you all of their secret objectives." b arms it at the start
        // of a's turn; a's first action is a played action card (Spy — a card whose
        // window is "Action", so it is offered on a plain turn), not a strategic
        // one, so the punishment settles: the goods move to b, the rest of the hand
        // is discarded, the duress is spent, and the turn passes on.
        let a = PlayerId::new("a");
        let b = PlayerId::new("b");
        let mut state =
            start_game(ContentStore::embedded(), &[a.clone(), b.clone()], POK, None).unwrap();
        state.phase = Phase::Action;
        state.active = Some(a.clone());
        state.deal_strategy_card(&a, StrategyCardId::new("leadership"));
        state.deal_strategy_card(&b, StrategyCardId::new("imperial"));
        state.player_mut(&a).unwrap().trade_goods = 4;
        state
            .player_mut(&a)
            .unwrap()
            .action_cards
            .push(ti4_model::id::ActionCardId::new("spy"));
        // A card the punishment will discard, proving the hand is lost wholesale.
        state
            .player_mut(&a)
            .unwrap()
            .action_cards
            .push(ti4_model::id::ActionCardId::new("emergency"));
        state
            .player_mut(&b)
            .unwrap()
            .action_cards
            .push(ti4_model::id::ActionCardId::new("extremeduress"));
        let table = Table::with_default(Box::new(
            TurnDecider::new(&[("b", "TURN_BEGAN", "reaction:generic:TURN_BEGAN:after")])
                .playing_the_action_card(),
        ));
        let mut game = Game::with_table(state, ContentStore::embedded(), table);
        let mut guard = 0;
        while !game.events.iter().any(|e| e == "EXTREME_DURESS:a") && guard < 50 {
            assert_eq!(game.step().error, None, "no turn step should refuse");
            guard += 1;
        }

        let state = &game.state;
        let events = &game.events;
        assert!(
            events.iter().any(|e| e == "EXTREME_DURESS:a"),
            "the punishment settled when a took its non-strategic action; log {events:?}"
        );
        assert_eq!(
            state.player(&a).unwrap().trade_goods,
            0,
            "a's trade goods went to b; log {events:?}"
        );
        assert_eq!(
            state.player(&b).unwrap().trade_goods,
            4,
            "b holds the confiscated goods; log {events:?}"
        );
        assert!(
            state.player(&a).unwrap().action_cards.is_empty(),
            "a's action cards were discarded; log {events:?}"
        );
        assert_eq!(
            state.player(&a).unwrap().duress_by,
            None,
            "the duress is spent once it bites; log {events:?}"
        );
    }

    #[test]
    fn without_extreme_duress_an_action_keeps_the_target_whole() {
        // The control: the same turn, no card armed. a's action card plays out and the
        // seat keeps its trade goods: the punishment has no card to come from.
        let a = PlayerId::new("a");
        let b = PlayerId::new("b");
        let mut state =
            start_game(ContentStore::embedded(), &[a.clone(), b.clone()], POK, None).unwrap();
        state.phase = Phase::Action;
        state.active = Some(a.clone());
        state.deal_strategy_card(&a, StrategyCardId::new("leadership"));
        state.deal_strategy_card(&b, StrategyCardId::new("imperial"));
        state.player_mut(&a).unwrap().trade_goods = 4;
        state
            .player_mut(&a)
            .unwrap()
            .action_cards
            .push(ti4_model::id::ActionCardId::new("spy"));
        state
            .player_mut(&a)
            .unwrap()
            .action_cards
            .push(ti4_model::id::ActionCardId::new("emergency"));
        let table = Table::with_default(Box::new(TurnDecider::new(&[]).playing_the_action_card()));
        let mut game = Game::with_table(state, ContentStore::embedded(), table);
        let mut guard = 0;
        while game.state.active.as_ref().is_some_and(|p| p == &a) && guard < 50 {
            assert_eq!(game.step().error, None, "no turn step should refuse");
            guard += 1;
        }

        let state = &game.state;
        assert!(
            !game.events.iter().any(|e| e == "EXTREME_DURESS:a"),
            "nothing was armed, nothing punished; log {:?}",
            game.events
        );
        assert_eq!(
            state.player(&a).unwrap().trade_goods,
            4,
            "an un-pressured action keeps the seat's goods; log {:?}",
            game.events
        );
        assert_eq!(
            state.player(&a).unwrap().action_cards,
            vec![ti4_model::id::ActionCardId::new("emergency")],
            "the played card left the hand, but nothing else was discarded; log {:?}",
            game.events
        );
        assert_eq!(
            state.player(&a).unwrap().duress_by,
            None,
            "there was no duress to carry; log {:?}",
            game.events
        );
        assert_eq!(
            state.active.clone(),
            Some(b.clone()),
            "the turn passed on after the single action; log {:?}",
            game.events
        );
    }

    #[test]
    fn extreme_duress_lifts_when_the_target_takes_a_strategic_action() {
        // The card punishes the target "if that player's next action is not a strategic
        // action": a strategic action is the one that owes nothing. b arms the duress
        // at the start of a's turn, a takes the strategic action (the only one a
        // holds), and the duress lifts quietly: no goods move, no hand is discarded,
        // and the target's action proceeds.
        let a = PlayerId::new("a");
        let b = PlayerId::new("b");
        let mut state =
            start_game(ContentStore::embedded(), &[a.clone(), b.clone()], POK, None).unwrap();
        state.phase = Phase::Action;
        state.active = Some(a.clone());
        state.deal_strategy_card(&a, StrategyCardId::new("leadership"));
        state.deal_strategy_card(&b, StrategyCardId::new("imperial"));
        state.player_mut(&a).unwrap().trade_goods = 3;
        state
            .player_mut(&b)
            .unwrap()
            .action_cards
            .push(ti4_model::id::ActionCardId::new("extremeduress"));
        let table = Table::with_default(Box::new(
            TurnDecider::new(&[("b", "TURN_BEGAN", "reaction:generic:TURN_BEGAN:after")])
                .taking_the_strategic_action(),
        ));
        let mut game = Game::with_table(state, ContentStore::embedded(), table);
        let mut guard = 0;
        while !game.events.iter().any(|e| e == "STRATEGIC_ACTION_BEGAN") && guard < 100 {
            assert_eq!(game.step().error, None, "no turn step should refuse");
            guard += 1;
        }

        let state = &game.state;
        let events = &game.events;
        assert!(
            events.iter().any(|e| e == "STRATEGIC_ACTION_BEGAN"),
            "a's strategic action began; log {events:?}"
        );
        assert!(
            !events.iter().any(|e| e == "EXTREME_DURESS:a"),
            "a strategic action is not the action the card punishes; log {events:?}"
        );
        assert_eq!(
            state.player(&a).unwrap().duress_by,
            None,
            "the duress lifted with the strategic action; log {events:?}"
        );
        assert_eq!(
            state.player(&a).unwrap().trade_goods,
            3,
            "the punishment never settled, so the goods never moved; log {events:?}"
        );
    }

    /// A decider that records every (player, prompt) it answers, then answers on its own:    /// A decider that records every (player, prompt) it answers, then answers on its own:
    /// b votes "b", every other seat votes "a", and anything else (playing a reaction
    /// card, exhausting a planet, scoring an objective) takes the first option offered.
    /// The recorded sequence *is* the question order, which is the observable shape of a
    /// vote.
    struct RecordingDecider {
        seen: Arc<Mutex<Vec<(String, String)>>>,
    }

    impl Decider for RecordingDecider {
        fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
            self.seen
                .lock()
                .unwrap()
                .push((choice.player.to_string(), choice.prompt.clone()));
            let prefer = if choice.player == PlayerId::new("b") {
                "b"
            } else {
                "a"
            };
            if let Some(option) = choice.option(prefer).cloned() {
                return Ok(option);
            }
            choice
                .options
                .first()
                .cloned()
                .ok_or_else(|| IllegalChoice::ScriptDiverged {
                    player: choice.player.clone(),
                    wanted: prefer.to_owned(),
                    offered: choice.ids().into_iter().map(str::to_owned).collect(),
                })
        }
    }

    /// Three seats in the agenda phase: a is the speaker, every seat controls one
    /// influence-1 planet, and the deck holds the single `secret` agenda. With `with_hack`
    /// b holds a Hack Election card and plays it in the reveal window.
    fn hack_state(with_hack: bool) -> GameState {
        let content = ContentStore::embedded();
        let players = [PlayerId::new("a"), PlayerId::new("b"), PlayerId::new("c")];
        let mut state = start_game(content, &players, POK, None).unwrap();
        state.phase = Phase::Agenda;
        state.custodians_removed = true;

        let catalogue = ti4_content::galaxy::all_planets(content, POK);
        let grants: Vec<(String, String)> = catalogue
            .iter()
            .filter(|(_, planet)| planet.influence() == 1 && !planet.is_placed_during_play())
            .take(3)
            .map(|(id, planet)| {
                (
                    id.to_string(),
                    planet.system_id().unwrap_or("18").to_owned(),
                )
            })
            .collect();
        assert_eq!(grants.len(), 3, "the corpus has three influence-1 planets");
        for (who, (planet, system)) in players.iter().zip(&grants) {
            state
                .system_mut(&SystemId::new(system))
                .set_control(PlanetId::new(planet), who.clone());
        }
        if with_hack {
            state.player_mut(&players[1]).unwrap().action_cards =
                vec![ti4_model::id::ActionCardId::new("hack")];
        }
        state.agenda_deck = vec!["secret".to_owned()];
        state
    }

    /// Drives the scenario to the agenda's resolution and hands back the final state, the
    /// event log, and the question sequence the table recorded along the way.
    fn run_hack_scenario(with_hack: bool) -> (GameState, Vec<String>, Vec<(String, String)>) {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let table = Table::with_default(Box::new(RecordingDecider { seen: seen.clone() }));
        let mut game = Game::with_table(hack_state(with_hack), ContentStore::embedded(), table);
        let mut guard = 0;
        while guard < 100
            && !game
                .events
                .iter()
                .any(|e| e.starts_with("AGENDA_RESOLVED:"))
        {
            assert_eq!(game.step().error, None, "no agenda step should refuse");
            guard += 1;
        }
        (game.state, game.events, seen.lock().unwrap().clone())
    }

    #[test]
    fn hack_votes_last_in_the_agenda_vote() {
        // Hack Election: "After an agenda is revealed: During this agenda, you vote last."
        // b plays it in the reveal window, so b's vote is the last one asked even though a
        // is the speaker: c, then the speaker a, then b.
        let (state, events, seen) = run_hack_scenario(true);

        assert!(
            seen.iter()
                .any(|(p, q)| p == "b" && q.contains("AGENDA_REVEALED")),
            "the reveal window asked the card's holder; saw {seen:?}"
        );
        let voters: Vec<&str> = seen
            .iter()
            .filter(|(_, q)| q.starts_with("vote for which outcome"))
            .map(|(p, _)| p.as_str())
            .collect();
        assert_eq!(
            voters,
            ["c", "a", "b"],
            "b voted dead last and the speaker's seat moved ahead of the hacker; saw {seen:?}"
        );
        assert!(
            events.iter().any(|e| *e == "AGENDA_RESOLVED:secret:a"),
            "c and a backed `a` against b's lone vote; log {events:?}"
        );
        assert!(
            state
                .player(&PlayerId::new("b"))
                .unwrap()
                .action_cards
                .is_empty(),
            "the card was spent on the reveal window"
        );
    }

    #[test]
    fn without_hack_the_speaker_still_votes_last() {
        // The control: no reveal-window card anywhere, so the ordinary order stands —
        // b, then c, then the speaker a.
        let (state, events, seen) = run_hack_scenario(false);

        assert!(
            !seen.iter().any(|(_, q)| q.contains("AGENDA_REVEALED")),
            "nobody held a reveal-window card, so nobody was asked; saw {seen:?}"
        );
        let voters: Vec<&str> = seen
            .iter()
            .filter(|(_, q)| q.starts_with("vote for which outcome"))
            .map(|(p, _)| p.as_str())
            .collect();
        assert_eq!(
            voters,
            ["b", "c", "a"],
            "from the speaker's left all the way around to the speaker; saw {seen:?}"
        );
        assert!(
            events.iter().any(|e| *e == "AGENDA_RESOLVED:secret:a"),
            "same two-to-one tally in the ordinary order; log {events:?}"
        );
        let _ = state;
    }

    #[test]
    fn run_reports_its_step_horizon_instead_of_looping() {
        let players = [PlayerId::new("a"), PlayerId::new("b"), PlayerId::new("c")];
        let state = start_game(ContentStore::embedded(), &players, POK, None).unwrap();
        let mut game = Game::new(state, ContentStore::embedded());

        let stopped = game.run(1, 3).expect_err("three steps is not a round");
        let RunError::StepLimit {
            max_steps,
            round,
            phase,
            recent,
            repeats,
        } = stopped
        else {
            panic!("a step limit, not {stopped:?}");
        };
        assert_eq!((max_steps, round, phase), (3, 1, Phase::Strategy));
        // The limit now names what it stopped on, so a stalled run is a lead rather than a
        // round number: three strategy-phase steps in a row are three picks from the same prompt.
        assert!(
            recent.to_lowercase().contains("strategy"),
            "the prompt is reported: {recent:?}"
        );
        assert!(repeats >= 1, "and how many steps in a row asked it");
    }

    // ------------------------------------------------------------------
    // C3: Salvage, Reparations, Infiltrate, Reverse Engineer, Black Market Dealings.
    // ------------------------------------------------------------------

    /// The tactical fixture with b moved off the shared default faction, so a transaction
    /// offered "with loam" resolves to b instead of the first generic seat in the room.
    fn market_fixture(a_cards: &[&str]) -> (GameState, Galaxy) {
        let (mut state, galaxy, ids) = tactical_fixture();
        let a = PlayerId::new("a");
        let b = PlayerId::new("b");
        state.player_mut(&b).unwrap().faction = ti4_model::id::FactionId::new("loam");
        // Both seats hold ships in the same hub system: LRR 60 neighbours, so a trade with
        // b is even offered (a home system alone is not presence).
        state.player_mut(&a).unwrap().home_system = Some(ids[0].clone());
        state.player_mut(&b).unwrap().home_system = Some(ids[0].clone());
        crate::fixtures::put(&mut state, &ids[0], "fighter", &a, 1);
        crate::fixtures::put(&mut state, &ids[0], "fighter", &b, 1);
        state.player_mut(&a).unwrap().action_cards = a_cards
            .iter()
            .map(|alias| ti4_model::id::ActionCardId::new(*alias))
            .collect();
        (state, galaxy)
    }

    /// A non-space-station planet in a system other than `avoid`.
    fn a_distant_planet(avoid: &SystemId) -> (SystemId, PlanetId) {
        let content = ContentStore::embedded();
        let planets = ti4_content::galaxy::all_planets(content, POK);
        for (id, planet) in &planets {
            let id = *id;
            let system = planet.system_id().unwrap_or("00");
            if system != avoid.as_str() && !ti4_content::galaxy::is_space_station(content, id, POK)
            {
                return (SystemId::new(system), PlanetId::new(id));
            }
        }
        panic!("the corpus has a distant planet")
    }

    /// Drives an invasion turn: plays the tactical action on the named system, moves
    /// nothing, commits the first landable unit to `commit` and to no other planet, and
    /// plays a scripted reaction at the first window question it is offered. Every other
    /// question takes its first option, which is the deterministic answer the exploration
    /// and sustain prompts need.
    struct InvasionDecider {
        system: ti4_model::id::SystemId,
        commit: ti4_model::id::PlanetId,
        reactions: std::collections::BTreeMap<ti4_model::id::PlayerId, String>,
    }

    impl crate::choice::Decider for InvasionDecider {
        fn choose(
            &mut self,
            choice: &crate::choice::Choice,
        ) -> Result<crate::choice::ChoiceOption, crate::choice::IllegalChoice> {
            let no_options = || crate::choice::IllegalChoice::NoOptions {
                player: choice.player.clone(),
                prompt: choice.prompt.clone(),
            };
            if choice.options.iter().any(|o| o.id == TACTICAL_ACTION_ID) {
                return choice
                    .options
                    .iter()
                    .find(|o| o.id == TACTICAL_ACTION_ID)
                    .cloned()
                    .ok_or_else(no_options);
            }
            if choice.prompt == "activate a system"
                && let Some(o) = choice.options.iter().find(|o| o.id == self.system.as_str())
            {
                return Ok(o.clone());
            }
            if let Some(o) = choice.options.iter().find(|o| o.id == "done_moving") {
                return Ok(o.clone());
            }
            let wanted = format!("commit|0|{}", self.commit.as_str());
            if let Some(o) = choice.options.iter().find(|o| o.id == wanted) {
                return Ok(o.clone());
            }
            if let Some(o) = choice.options.iter().find(|o| o.id == "done_committing") {
                return Ok(o.clone());
            }
            if let Some(reaction) = self.reactions.get(&choice.player)
                && let Some(o) = choice
                    .options
                    .iter()
                    .find(|o| o.id.as_str() == reaction.as_str())
            {
                return Ok(o.clone());
            }
            choice.options.first().cloned().ok_or_else(no_options)
        }
    }

    /// One recorded ask: `(player, prompt, option ids)`.
    type Ask = (String, String, Vec<String>);

    /// Drives a two-seat transaction end to end. Records every ask as
    /// `(player, prompt, option ids)`; in the arm it plays Black Market Dealings when the
    /// window opens, a's action phase opens the trade, the proposer takes the first offer
    /// shape unless `want_secret_deal` (then the secret shape, and b accepts the answer).
    struct MarketDecider {
        arm: bool,
        want_secret_deal: bool,
        seen: Arc<Mutex<Vec<Ask>>>,
    }

    impl Decider for MarketDecider {
        fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
            let offered = choice
                .ids()
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>();
            self.seen.lock().unwrap().push((
                choice.player.to_string(),
                choice.prompt.clone(),
                offered.clone(),
            ));
            let window = choice.prompt.starts_with("when ") || choice.prompt.starts_with("after ");
            if window {
                if self.arm
                    && choice.player == PlayerId::new("a")
                    && choice.prompt.ends_with("TRANSACTION_OPENED")
                    && let Some(play) = choice
                        .options
                        .iter()
                        .find(|option| option.id == "reaction:generic:TRANSACTION_OPENED:when")
                        .cloned()
                {
                    return Ok(play);
                }
                if let Some(decline) = choice
                    .options
                    .iter()
                    .find(|option| option.is_decline())
                    .cloned()
                {
                    return Ok(decline);
                }
            }
            if choice.prompt == "action phase"
                && choice.player == PlayerId::new("a")
                && let Some(open) = choice
                    .options
                    .iter()
                    .find(|option| option.id.starts_with("component|trade|"))
                    .cloned()
            {
                return Ok(open);
            }
            if choice.prompt.starts_with("transaction with ") {
                if self.want_secret_deal {
                    if let Some(secret) = choice
                        .options
                        .iter()
                        .find(|option| option.id.starts_with("so"))
                        .cloned()
                    {
                        return Ok(secret);
                    }
                    return Err(IllegalChoice::ScriptDiverged {
                        player: choice.player.clone(),
                        wanted: "so…".to_owned(),
                        offered,
                    });
                }
                if let Some(offer) = choice.options.first().cloned() {
                    return Ok(offer);
                }
            }
            if choice.prompt.contains("-- accept?") {
                let wanted = if self.want_secret_deal {
                    "accept"
                } else {
                    "refuse"
                };
                if let Some(answer) = choice
                    .options
                    .iter()
                    .find(|option| option.id == wanted)
                    .cloned()
                {
                    return Ok(answer);
                }
                return Err(IllegalChoice::ScriptDiverged {
                    player: choice.player.clone(),
                    wanted: wanted.to_owned(),
                    offered,
                });
            }
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

    /// Steps until the table has settled the transaction (or the guard trips), sharing the
    /// decider's question log through `seen`.
    fn settle_market(game: &mut Game, seen: &Arc<Mutex<Vec<Ask>>>) {
        let mut guard = 0;
        loop {
            guard += 1;
            assert!(guard < 60, "the trade never settled; log {:?}", game.events);
            assert_eq!(game.step().error, None, "log: {:?}", game.events);
            if game.trade.is_none()
                && seen
                    .lock()
                    .unwrap()
                    .iter()
                    .any(|(_, prompt, _)| prompt.starts_with("transaction with "))
            {
                return;
            }
        }
    }

    #[test]
    fn salvage_sweeps_the_losers_commodities_after_a_won_space_combat() {
        // Salvage: "After you win a space combat: Your opponent gives you all of their
        // commodities." a wins: three cruisers on pinned 10s destroy b's flagship (pinned
        // 1s never hit). b enters the fight with three commodities and hands them over
        // when a plays salvage from the After window.
        let a = PlayerId::new("a");
        let b = PlayerId::new("b");
        let (mut state, galaxy, ids) = combat_fixture(&["salvage"], &[]);
        crate::fixtures::put(&mut state, &ids[0], "cruiser", &a, 3);
        crate::fixtures::put(&mut state, &ids[0], "flagship", &b, 1);
        state.player_mut(&a).unwrap().commodities = 0;
        state.player_mut(&b).unwrap().commodities = 3;
        let script = vec![
            TACTICAL_ACTION_ID.to_owned(),
            ids[0].to_string(),
            "done_moving".to_owned(),
            "stay".to_owned(),
            "decline".to_owned(),
            "reaction:generic:SPACE_COMBAT_WON:after".to_owned(),
        ];
        let table = Table::with_default(Box::new(Scripted::new(script)));
        let mut game = Game::with_table(state, ContentStore::embedded(), table).with_galaxy(galaxy);
        game.dice = Dice::from_faces([10, 10, 10, 1, 1, 10, 10, 10, 1, 1, 10, 10, 10, 1, 1]);
        let mut guard = 0;
        while !game.events.iter().any(|e| e == "TACTICAL_ACTION_COMPLETE") && guard < 60 {
            assert_eq!(game.step().error, None, "log: {:?}", game.events);
            guard += 1;
        }
        let events = &game.events;
        assert!(
            events.iter().any(|e| e == "SPACE_COMBAT_WON"),
            "a won the combat; log {events:?}"
        );
        assert_eq!(
            game.state.player(&a).unwrap().commodities,
            3,
            "the commodities moved to the winner; log {events:?}"
        );
        assert_eq!(
            game.state.player(&b).unwrap().commodities,
            0,
            "the loser was left without commodities; log {events:?}"
        );
        assert!(
            game.state.player(&a).unwrap().action_cards.is_empty(),
            "salvage was spent on the play; log {events:?}"
        );
    }

    #[test]
    fn without_salvage_a_won_space_combat_leaves_the_losers_commodities_alone() {
        // The control: the same fight, nobody armed. b keeps every commodity — the sweep
        // has no card to come from.
        let a = PlayerId::new("a");
        let b = PlayerId::new("b");
        let (mut state, galaxy, ids) = combat_fixture(&[], &[]);
        crate::fixtures::put(&mut state, &ids[0], "cruiser", &a, 3);
        crate::fixtures::put(&mut state, &ids[0], "flagship", &b, 1);
        state.player_mut(&a).unwrap().commodities = 0;
        state.player_mut(&b).unwrap().commodities = 3;
        let script = vec![
            TACTICAL_ACTION_ID.to_owned(),
            ids[0].to_string(),
            "done_moving".to_owned(),
            "stay".to_owned(),
            "decline".to_owned(),
        ];
        let table = Table::with_default(Box::new(Scripted::new(script)));
        let mut game = Game::with_table(state, ContentStore::embedded(), table).with_galaxy(galaxy);
        game.dice = Dice::from_faces([10, 10, 10, 1, 1, 10, 10, 10, 1, 1, 10, 10, 10, 1, 1]);
        let mut guard = 0;
        while !game.events.iter().any(|e| e == "TACTICAL_ACTION_COMPLETE") && guard < 60 {
            assert_eq!(game.step().error, None, "log: {:?}", game.events);
            guard += 1;
        }
        let events = &game.events;
        assert!(
            events.iter().any(|e| e == "SPACE_COMBAT_WON"),
            "a still won; log {events:?}"
        );
        assert_eq!(
            game.state.player(&a).unwrap().commodities,
            0,
            "nothing swept, nothing gained; log {events:?}"
        );
        assert_eq!(
            game.state.player(&b).unwrap().commodities,
            3,
            "the loser keeps their commodities; log {events:?}"
        );
    }

    #[test]
    fn reparations_exhausts_the_gainers_planet_and_readies_the_holders() {
        // Reparations: "After another player gains control of a planet you control:
        // Exhaust 1 planet that player controls and ready 1 planet you control." a invades
        // b's planet: the dreadnought's bombardment (5, 4) destroys b's infantry
        // and the planet falls, exhausting it on the capture. b holds the planet and the
        // card, so b plays it in the After window: a's single controlled planet (the
        // capture) is re-exhausted, and b's one exhausted planet elsewhere is readied.
        let (mut state, galaxy, system, planet) = invasion_fixture();
        let a = PlayerId::new("a");
        let b = PlayerId::new("b");
        crate::fixtures::put(&mut state, &system, "destroyer", &a, 1);
        crate::fixtures::put(&mut state, &system, "dreadnought", &a, 1);
        crate::fixtures::put(&mut state, &system, "infantry", &a, 1);
        state
            .system_mut(&system)
            .set_control(planet.clone(), b.clone());
        crate::fixtures::put_on_planet(&mut state, &system, &planet, "infantry", &b, 1);
        let (other, b_planet) = a_distant_planet(&system);
        state
            .system_mut(&other)
            .set_control(b_planet.clone(), b.clone());
        state.exhaust_planet(b_planet.clone());
        state.player_mut(&b).unwrap().action_cards =
            vec![ti4_model::id::ActionCardId::new("reparations")];
        let mut reactions = std::collections::BTreeMap::new();
        reactions.insert(
            b.clone(),
            "reaction:generic:PLANET_CONTROL_GAINED:after".to_owned(),
        );
        let table = Table::with_default(Box::new(InvasionDecider {
            system: system.clone(),
            commit: planet.clone(),
            reactions,
        }));
        let mut game = Game::with_table(state, ContentStore::embedded(), table).with_galaxy(galaxy);
        game.dice = Dice::from_faces([6]);
        let mut guard = 0;
        while !game.events.iter().any(|e| e == "TACTICAL_ACTION_COMPLETE") && guard < 60 {
            assert_eq!(game.step().error, None, "log: {:?}", game.events);
            guard += 1;
        }
        let events = &game.events;
        let state = &game.state;
        assert!(
            state.system_state(&system).planet_control.get(&planet) == Some(&a),
            "a took the planet; log {events:?}"
        );
        assert!(
            state.exhausted_planets.contains(&planet),
            "the capture exhausted the planet (LRR 42.3); log {events:?}"
        );
        assert!(
            !state.exhausted_planets.contains(&b_planet),
            "b's exhausted planet was readied by the card; log {events:?}"
        );
        assert!(
            state.player(&b).unwrap().action_cards.is_empty(),
            "reparations was spent on the play; log {events:?}"
        );
    }

    #[test]
    fn without_reparations_a_capture_exhausts_nothing_of_the_losers() {
        // The control: the same invasion, nobody armed. The capture still exhausts the
        // planet it takes, but b's other planet stays exhausted — nothing was readied.
        let (mut state, galaxy, system, planet) = invasion_fixture();
        let a = PlayerId::new("a");
        crate::fixtures::put(&mut state, &system, "destroyer", &a, 1);
        crate::fixtures::put(&mut state, &system, "dreadnought", &a, 1);
        crate::fixtures::put(&mut state, &system, "infantry", &a, 1);
        state
            .system_mut(&system)
            .set_control(planet.clone(), PlayerId::new("b"));
        crate::fixtures::put_on_planet(
            &mut state,
            &system,
            &planet,
            "infantry",
            &PlayerId::new("b"),
            1,
        );
        let (other, b_planet) = a_distant_planet(&system);
        state
            .system_mut(&other)
            .set_control(b_planet.clone(), PlayerId::new("b"));
        state.exhaust_planet(b_planet.clone());
        let table = Table::with_default(Box::new(InvasionDecider {
            system: system.clone(),
            commit: planet.clone(),
            reactions: std::collections::BTreeMap::new(),
        }));
        let mut game = Game::with_table(state, ContentStore::embedded(), table).with_galaxy(galaxy);
        game.dice = Dice::from_faces([6]);
        let mut guard = 0;
        while !game.events.iter().any(|e| e == "TACTICAL_ACTION_COMPLETE") && guard < 60 {
            assert_eq!(game.step().error, None, "log: {:?}", game.events);
            guard += 1;
        }
        let events = &game.events;
        let state = &game.state;
        assert!(
            state.system_state(&system).planet_control.get(&planet) == Some(&a),
            "a still took the planet; log {events:?}"
        );
        assert!(
            state.exhausted_planets.contains(&b_planet),
            "nothing readied b's planet; log {events:?}"
        );
    }

    #[test]
    fn infiltrate_is_played_when_the_planet_changes_hands() {
        // Infiltrate: "When you gain control of a planet: Replace each PDS and space
        // dock that is on that planet with a matching unit from your reinforcements."
        // a holds its own PDS on the planet it is about to capture (b holds one
        // infantry); a's war sun disables the planet's shield so the bombardment
        // reaches the infantry. a plays the card in the When window as the planet
        // changes hands; with a full box the replacement is the same unit for the
        // unit-less model, so the test pins the play, the spend, and that the capture
        // proceeds undisturbed: the PDS a owned is still standing (LRR 49 destroys
        // only rival structures), and a owns the planet.
        let (mut state, galaxy, system, planet) = invasion_fixture();
        let a = PlayerId::new("a");
        let b = PlayerId::new("b");
        crate::fixtures::put(&mut state, &system, "destroyer", &a, 1);
        crate::fixtures::put(&mut state, &system, "warsun", &a, 1);
        crate::fixtures::put(&mut state, &system, "infantry", &a, 1);
        state
            .system_mut(&system)
            .set_control(planet.clone(), b.clone());
        crate::fixtures::put_on_planet(&mut state, &system, &planet, "infantry", &b, 1);
        crate::fixtures::put_on_planet(&mut state, &system, &planet, "pds", &a, 1);
        state.player_mut(&a).unwrap().action_cards =
            vec![ti4_model::id::ActionCardId::new("infiltrate")];
        let mut reactions = std::collections::BTreeMap::new();
        reactions.insert(
            a.clone(),
            "reaction:generic:PLANET_CONTROL_GAINED:when".to_owned(),
        );
        let table = Table::with_default(Box::new(InvasionDecider {
            system: system.clone(),
            commit: planet.clone(),
            reactions,
        }));
        let mut game = Game::with_table(state, ContentStore::embedded(), table).with_galaxy(galaxy);
        game.dice = Dice::from_faces([4, 4, 4]);
        let mut guard = 0;
        while !game.events.iter().any(|e| e == "TACTICAL_ACTION_COMPLETE") && guard < 60 {
            assert_eq!(game.step().error, None, "log: {:?}", game.events);
            guard += 1;
        }
        let events = &game.events;
        let state = &game.state;
        assert!(
            state.system_state(&system).planet_control.get(&planet) == Some(&a),
            "a took the planet; log {events:?}"
        );
        let units: Vec<(String, String)> = state
            .system_state(&system)
            .planet_units
            .get(&planet)
            .map(|list| {
                list.iter()
                    .map(|unit| (unit.owner.to_string(), unit.type_id.to_string()))
                    .collect()
            })
            .unwrap_or_default();
        assert_eq!(
            units,
            vec![
                ("a".to_owned(), "pds".to_owned()),
                ("a".to_owned(), "infantry".to_owned()),
            ],
            "a's own PDS stood through the capture and its committed infantry landed on it; log {events:?}"
        );
        assert!(
            state.player(&a).unwrap().action_cards.is_empty(),
            "infiltrate was spent on the play; log {events:?}"
        );
    }

    #[test]
    fn reparations_do_nothing_when_the_holder_never_controlled_the_planet() {
        // The no-op branch: b holds Reparations but the planet a captures was owned by
        // nobody, so it was never a planet b controlled. The card still plays (the window
        // is coarse) and the effect verifies and declines: nothing is exhausted or
        // readied beyond the capture's own LRR 42.3 exhaustion.
        let (mut state, galaxy, system, planet) = invasion_fixture();
        let a = PlayerId::new("a");
        let b = PlayerId::new("b");
        crate::fixtures::put(&mut state, &system, "destroyer", &a, 1);
        crate::fixtures::put(&mut state, &system, "dreadnought", &a, 1);
        crate::fixtures::put(&mut state, &system, "infantry", &a, 1);
        // No set_control and no units: the planet was unowned and falls without a fight.
        // Taking an unowned planet sends a drawing from the exploration deck — the
        // decider's fallback answers whatever the draw asks in a deterministic first
        // option — and the capture needs a committed ground unit.
        let (other, b_planet) = a_distant_planet(&system);
        state
            .system_mut(&other)
            .set_control(b_planet.clone(), b.clone());
        state.exhaust_planet(b_planet.clone());
        state.player_mut(&b).unwrap().action_cards =
            vec![ti4_model::id::ActionCardId::new("reparations")];
        let mut reactions = std::collections::BTreeMap::new();
        reactions.insert(
            b.clone(),
            "reaction:generic:PLANET_CONTROL_GAINED:after".to_owned(),
        );
        let table = Table::with_default(Box::new(InvasionDecider {
            system: system.clone(),
            commit: planet.clone(),
            reactions,
        }));
        let mut game = Game::with_table(state, ContentStore::embedded(), table).with_galaxy(galaxy);
        game.dice = Dice::from_faces([1]);
        let mut guard = 0;
        while !game.events.iter().any(|e| e == "TACTICAL_ACTION_COMPLETE") && guard < 60 {
            assert_eq!(game.step().error, None, "log: {:?}", game.events);
            guard += 1;
        }
        let events = &game.events;
        let state = &game.state;
        assert!(
            state.system_state(&system).planet_control.get(&planet) == Some(&a),
            "a still took the unowned planet; log {events:?}"
        );
        assert!(
            state.exhausted_planets.contains(&b_planet),
            "the no-op effect readied nothing; log {events:?}"
        );
        assert!(
            state.player(&b).unwrap().action_cards.is_empty(),
            "the card was spent even though the effect declined; log {events:?}"
        );
    }

    #[test]
    fn black_market_dealings_puts_secrets_and_cards_on_the_table() {
        // Black Market Dealings: "When you are negotiating a transaction: You and the
        // other player may include relics, action cards, and unscored secret objectives
        // as part of the transaction." a opens a trade with b (loam) and plays the card
        // from the When window, so the offer question carries a secret shape (a holds
        // one unscored secret) and an action-card shape (a still holds Spy) on top of the
        // plain shapes. b refuses; when the window closes the marker is gone.
        let (mut state, galaxy) = market_fixture(&["blackmarketdealing", "spy"]);
        let a = PlayerId::new("a");
        let b = PlayerId::new("b");
        state.player_mut(&a).unwrap().trade_goods = 2;
        state.player_mut(&b).unwrap().trade_goods = 2;
        state.player_mut(&a).unwrap().secret_objectives =
            vec![ti4_model::id::SecretObjectiveId::new("ctr")];
        let seen = Arc::new(Mutex::new(Vec::new()));
        let table = Table::with_default(Box::new(MarketDecider {
            arm: true,
            want_secret_deal: false,
            seen: seen.clone(),
        }));
        let mut game = Game::with_table(state, ContentStore::embedded(), table).with_galaxy(galaxy);
        settle_market(&mut game, &seen);

        let log = seen.lock().unwrap();
        let (_, _prompt, ids) = log
            .iter()
            .find(|(player, prompt, _)| player == "a" && prompt.starts_with("transaction with "))
            .unwrap_or_else(|| panic!("a was asked to open the offer; log {:?}", game.events));
        assert!(
            ids.iter().any(|id| id.starts_with("so")),
            "a's unscored secret reached the table; log {ids:?}"
        );
        assert!(
            ids.iter().any(|id| id.starts_with("ac")),
            "an action card reached the table; log {ids:?}"
        );
        assert!(
            !game.state.transient_flags.has(TransientFlags::BLACK_MARKET),
            "the marker clears when the negotiation ends; log {:?}",
            game.events
        );
    }

    #[test]
    fn without_black_market_dealings_no_secret_or_card_shape_reaches_the_table() {
        // The control: the same negotiation without the card — the offer question carries
        // only the plain shapes (goods, notes), and no secret or action-card shape ever
        // appears even though a holds both a secret and a card.
        let (mut state, galaxy) = market_fixture(&["spy"]);
        let a = PlayerId::new("a");
        let b = PlayerId::new("b");
        state.player_mut(&a).unwrap().trade_goods = 2;
        state.player_mut(&b).unwrap().trade_goods = 2;
        state.player_mut(&a).unwrap().secret_objectives =
            vec![ti4_model::id::SecretObjectiveId::new("ctr")];
        let seen = Arc::new(Mutex::new(Vec::new()));
        let table = Table::with_default(Box::new(MarketDecider {
            arm: false,
            want_secret_deal: false,
            seen: seen.clone(),
        }));
        let mut game = Game::with_table(state, ContentStore::embedded(), table).with_galaxy(galaxy);
        settle_market(&mut game, &seen);

        let log = seen.lock().unwrap();
        let (_, _prompt, ids) = log
            .iter()
            .find(|(player, prompt, _)| player == "a" && prompt.starts_with("transaction with "))
            .unwrap_or_else(|| panic!("a was asked to open the offer; log {:?}", game.events));
        assert!(
            !ids.iter()
                .any(|id| id.starts_with("so") || id.starts_with("ac") || id.starts_with("fr")),
            "the plain table offers no secrets, cards, or fragments; log {ids:?}"
        );
        assert!(
            !game.state.transient_flags.has(TransientFlags::BLACK_MARKET),
            "no card was played, no marker exists; log {:?}",
            game.events
        );
    }

    #[test]
    fn a_black_market_deal_moves_an_unscored_secret_objective() {
        // The full deal: a plays Black Market Dealings, offers its unscored secret for
        // one trade good, and b accepts — the secret lands in b's hand, one good crosses
        // over, and the marker is cleared by the completion.
        let (mut state, galaxy) = market_fixture(&["blackmarketdealing", "spy"]);
        let a = PlayerId::new("a");
        let b = PlayerId::new("b");
        state.player_mut(&a).unwrap().trade_goods = 2;
        state.player_mut(&b).unwrap().trade_goods = 2;
        state.player_mut(&a).unwrap().secret_objectives =
            vec![ti4_model::id::SecretObjectiveId::new("ctr")];
        state.player_mut(&b).unwrap().secret_objectives = vec![];
        let seen = Arc::new(Mutex::new(Vec::new()));
        let table = Table::with_default(Box::new(MarketDecider {
            arm: true,
            want_secret_deal: true,
            seen: seen.clone(),
        }));
        let mut game = Game::with_table(state, ContentStore::embedded(), table).with_galaxy(galaxy);
        settle_market(&mut game, &seen);

        let log = seen.lock().unwrap();
        let ids = log
            .iter()
            .find(|(player, prompt, _)| player == "a" && prompt.starts_with("transaction with "))
            .map_or_else(
                || panic!("a was asked to open the offer; log {:?}", game.events),
                |(_, _, ids)| ids.clone(),
            );
        assert!(
            ids.iter().any(|id| id.starts_with("soctr")),
            "the unscored secret was offered by id; log {ids:?}"
        );
        let state = &game.state;
        assert!(
            state.player(&a).unwrap().secret_objectives.is_empty(),
            "a gave away the secret; log {:?}",
            game.events
        );
        assert_eq!(
            state.player(&b).unwrap().secret_objectives,
            vec![ti4_model::id::SecretObjectiveId::new("ctr")],
            "b holds the secret; log {:?}",
            game.events
        );
        assert_eq!(
            state.player(&a).unwrap().trade_goods,
            3,
            "the flat price paid into a's pool; log {:?}",
            game.events
        );
        assert_eq!(
            state.player(&b).unwrap().trade_goods,
            1,
            "b paid one good for the secret; log {:?}",
            game.events
        );
        assert!(
            !state.transient_flags.has(TransientFlags::BLACK_MARKET),
            "completion cleared the marker; log {:?}",
            game.events
        );
    }

    #[test]
    fn reverse_engineer_takes_a_played_component_card_out_of_the_pile() {
        // Reverse Engineer: "After another player discards an action card that has a
        // component action: Take that action card from the discard pile." b's turn action
        // is Industrial Initiative — a component action — so its play ends in the discard
        // event, and a, holding the card, plays it from the After window to take the card
        // straight out of the pile into a's hand. a's own spend (Reverse Engineer) still
        // lands in the pile: it was played too. (Spy would do the same, but its forced
        // steal robs the RE holder of the very card it needs, so the question-free
        // component action keeps the driver honest.)
        let a = PlayerId::new("a");
        let b = PlayerId::new("b");
        let mut state =
            start_game(ContentStore::embedded(), &[a.clone(), b.clone()], POK, None).unwrap();
        state.phase = Phase::Action;
        state.active = Some(b.clone());
        state.deal_strategy_card(&a, StrategyCardId::new("leadership"));
        state.deal_strategy_card(&b, StrategyCardId::new("imperial"));
        state.player_mut(&a).unwrap().action_cards =
            vec![ti4_model::id::ActionCardId::new("reverse_engineer")];
        state.player_mut(&b).unwrap().action_cards =
            vec![ti4_model::id::ActionCardId::new("industrial_initiative")];
        let table = Table::with_default(Box::new(
            TurnDecider::new(&[(
                "a",
                "ACTION_CARD_DISCARDED",
                "reaction:generic:ACTION_CARD_DISCARDED:after",
            )])
            .playing_the_action_card(),
        ));
        let mut game = Game::with_table(state, ContentStore::embedded(), table);
        let mut guard = 0;
        while game.state.active.as_ref().is_some_and(|p| p == &b) && guard < 50 {
            assert_eq!(game.step().error, None, "log: {:?}", game.events);
            guard += 1;
        }
        let events = &game.events;
        let state = &game.state;
        assert!(
            events.iter().any(|e| e == "ACTION_CARD_DISCARDED"),
            "b's play ended in the discard event; log {events:?}"
        );
        assert_eq!(
            state.player(&a).unwrap().action_cards,
            vec![ti4_model::id::ActionCardId::new("industrial_initiative")],
            "a took the played card out of the pile; log {events:?}"
        );
        assert!(
            state.player(&b).unwrap().action_cards.is_empty(),
            "b's hand is empty after the play; log {events:?}"
        );
        assert_eq!(
            state.discarded_action_cards,
            vec![ti4_model::id::ActionCardId::new("reverse_engineer")],
            "only a's own spend rests in the pile; log {events:?}"
        );
    }

    #[test]
    fn without_reverse_engineer_a_played_component_card_rests_in_the_pile() {
        // The control: b plays Industrial Initiative, nobody takes it from the discard
        // pile, and the pile keeps it for whoever gets there next. a's unplayed card
        // stays in a's hand.
        let a = PlayerId::new("a");
        let b = PlayerId::new("b");
        let mut state =
            start_game(ContentStore::embedded(), &[a.clone(), b.clone()], POK, None).unwrap();
        state.phase = Phase::Action;
        state.active = Some(b.clone());
        state.deal_strategy_card(&a, StrategyCardId::new("leadership"));
        state.deal_strategy_card(&b, StrategyCardId::new("imperial"));
        state.player_mut(&a).unwrap().action_cards =
            vec![ti4_model::id::ActionCardId::new("reverse_engineer")];
        state.player_mut(&b).unwrap().action_cards =
            vec![ti4_model::id::ActionCardId::new("industrial_initiative")];
        let table = Table::with_default(Box::new(TurnDecider::new(&[]).playing_the_action_card()));
        let mut game = Game::with_table(state, ContentStore::embedded(), table);
        let mut guard = 0;
        while game.state.active.as_ref().is_some_and(|p| p == &b) && guard < 50 {
            assert_eq!(game.step().error, None, "log: {:?}", game.events);
            guard += 1;
        }
        let events = &game.events;
        let state = &game.state;
        assert_eq!(
            state.discarded_action_cards,
            vec![ti4_model::id::ActionCardId::new("industrial_initiative")],
            "the played card rests in the pile; log {events:?}"
        );
        assert_eq!(
            state.player(&a).unwrap().action_cards,
            vec![ti4_model::id::ActionCardId::new("reverse_engineer")],
            "a's card was never played; log {events:?}"
        );
    }
}
