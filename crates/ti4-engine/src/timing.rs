//! Deterministic timing-ability registration.
//!
//! This module owns only the data and ordered registry needed to open a timing window. Event
//! execution deliberately remains in later M03 packages.

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use ti4_model::id::PlayerId;
use ti4_model::{content_types::SourceSet, state::GameState};

use crate::{
    choice::{Choice, ChoiceOption, IllegalChoice, Table},
    dice::Dice,
    event::{Event, EventSequence, EventSequenceError},
    rng::GameRng,
};
use ti4_content::ContentStore;
use ti4_model::state::Phase;

/// The point relative to an event at which an ability may resolve.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Relation {
    /// Resolve before the event's ordinary effect.
    When,
    /// Resolve after the event's ordinary effect.
    After,
}

/// The longest scope in which an ability may be used.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Frequency {
    /// Once for each triggering event occurrence.
    OncePerTrigger,
    /// Once during a turn.
    OncePerTurn,
    /// Once during a round.
    OncePerRound,
    /// No cross-window usage limit.
    Unlimited,
}

/// A side-effect performed when an ability is selected in an eligible timing window.
pub type AbilityEffect =
    Arc<dyn Fn(&mut Event, &mut Resolver) -> Result<(), TimingError> + Send + Sync>;

/// A timing ability effect that can make a checked game-state transition.
///
/// Unlike [`AbilityEffect`], this can only run through [`Resolver::emit_with_context`]. This
/// prevents an isolated resolver test or a detached caller from presenting a stateful rule as
/// having resolved without the state, table, content scope, dice, and RNG it requires.
pub type StatefulAbilityEffect = Arc<
    dyn for<'a> Fn(&mut Event, &mut Resolver, &mut TimingContext<'a>) -> Result<(), TimingError>
        + Send
        + Sync,
>;

/// A rule-specific eligibility predicate.
pub type AbilityCondition = Arc<dyn Fn(&Event, &Resolver) -> bool + Send + Sync>;

/// A state-aware eligibility predicate for a timing ability.
pub type StatefulAbilityCondition =
    Arc<dyn for<'a> Fn(&Event, &Resolver, &TimingContext<'a>) -> bool + Send + Sync>;

/// Factual context copied onto a choice offered for an optional ability.
pub type OptionPayload = Arc<dyn Fn(&Event, &Resolver) -> BTreeMap<String, Value> + Send + Sync>;

/// The mutable game services a stateful timing rule may use.
///
/// The driver creates this only while resolving a typed event. It deliberately carries the same
/// game-owned table, RNG, and dice history as ordinary actions, so a timing ability cannot make a
/// detached choice or consume a different entropy stream.
pub struct TimingContext<'a> {
    /// Game state the event and its timing abilities may transition.
    pub state: &'a mut GameState,
    /// Immutable content corpus used for rule lookups.
    pub content: &'a ContentStore,
    /// Expansion scope for the current game.
    pub sources: SourceSet,
    /// The game's single legal-choice table.
    pub table: &'a mut Table,
    /// The game's recorded dice history.
    pub dice: &'a mut Dice,
    /// The game's domain-separated random stream.
    pub rng: &'a mut GameRng,
    /// The game's typed-event allocator, shared by nested rule emissions.
    pub event_sequence: &'a mut EventSequence,
    /// The map, when the game has one.
    ///
    /// Optional for the same reason `objectives::Position` carries it optionally: a rule that
    /// asks about the shape of the board cannot resolve without one, and reporting it unmet is
    /// honest where guessing is not. Skilled Retreat is the first such card.
    pub galaxy: Option<&'a ti4_content::galaxy::Galaxy>,
}

impl TimingContext<'_> {
    /// Put a nested timing-window choice to a decider with the public game position attached.
    ///
    /// # Errors
    /// Returns [`crate::choice::IllegalChoice`] if the answer was not offered.
    pub fn ask_seeing(
        &mut self,
        choice: &crate::choice::Choice,
    ) -> Result<crate::choice::ChoiceOption, crate::choice::IllegalChoice> {
        self.table.ask_seeing(
            choice,
            &crate::choice::Observed::new(self.state, self.content, self.sources, self.galaxy),
        )
    }
}

/// An ability registered against one deterministic timing window.
#[derive(Clone)]
pub struct Ability {
    /// Stable rule or card identifier.
    pub id: String,
    /// Player who owns the ability.
    pub owner: PlayerId,
    /// Triggering event type.
    pub event_type: String,
    /// Whether this is a WHEN or AFTER ability.
    pub relation: Relation,
    /// Fallible rule effect invoked by the timing resolver.
    pub effect: AbilityEffect,
    /// State-aware effect, if this ability needs the active game's mutation context.
    pub stateful_effect: Option<StatefulAbilityEffect>,
    /// Optional rule-specific eligibility gate.
    pub condition: Option<AbilityCondition>,
    /// State-aware eligibility condition, evaluated only with a driver context.
    pub stateful_condition: Option<StatefulAbilityCondition>,
    /// Cross-window usage metadata.
    pub frequency: Frequency,
    /// Whether a decider may decline this ability.
    pub optional: bool,
    /// Whether multiple slots with this identifier may resolve in one window.
    pub repeatable_in_window: bool,
    /// Optional factual choice payload supplied by the rule.
    pub option_payload: Option<OptionPayload>,
}

impl Ability {
    /// Construct an ability with the oracle's default frequency and optional metadata.
    #[must_use]
    pub fn new(
        id: impl Into<String>,
        owner: PlayerId,
        event_type: impl Into<String>,
        relation: Relation,
        effect: AbilityEffect,
    ) -> Self {
        Self {
            id: id.into(),
            owner,
            event_type: event_type.into(),
            relation,
            effect,
            stateful_effect: None,
            condition: None,
            stateful_condition: None,
            frequency: Frequency::OncePerTrigger,
            optional: false,
            repeatable_in_window: false,
            option_payload: None,
        }
    }

    /// Construct an ability whose effect mutates the active game through [`TimingContext`].
    #[must_use]
    pub fn stateful(
        id: impl Into<String>,
        owner: PlayerId,
        event_type: impl Into<String>,
        relation: Relation,
        effect: StatefulAbilityEffect,
    ) -> Self {
        let mut ability = Self::new(id, owner, event_type, relation, Arc::new(|_, _| Ok(())));
        ability.stateful_effect = Some(effect);
        ability
    }

    /// Attach a rule-specific eligibility predicate.
    #[must_use]
    pub fn with_condition(mut self, condition: AbilityCondition) -> Self {
        self.condition = Some(condition);
        self
    }

    /// Attach an eligibility predicate that reads the active game state.
    #[must_use]
    pub fn with_stateful_condition(mut self, condition: StatefulAbilityCondition) -> Self {
        self.stateful_condition = Some(condition);
        self
    }

    /// Set cross-window frequency metadata.
    #[must_use]
    pub const fn with_frequency(mut self, frequency: Frequency) -> Self {
        self.frequency = frequency;
        self
    }

    /// Mark whether the ability can be declined by its owner.
    #[must_use]
    pub const fn with_optional(mut self, optional: bool) -> Self {
        self.optional = optional;
        self
    }

    /// Mark whether distinct slots may share this identifier in one window.
    #[must_use]
    pub const fn with_repeatable_in_window(mut self, repeatable_in_window: bool) -> Self {
        self.repeatable_in_window = repeatable_in_window;
        self
    }

    /// Attach factual choice context for an optional ability.
    #[must_use]
    pub fn with_option_payload(mut self, option_payload: OptionPayload) -> Self {
        self.option_payload = Some(option_payload);
        self
    }
}

/// A registered absolute "cannot" effect.
#[derive(Clone)]
struct CannotEffect {
    label: String,
    predicate: Arc<dyn Fn(&Event) -> bool + Send + Sync>,
}

impl CannotEffect {
    fn new(label: impl Into<String>, predicate: Arc<dyn Fn(&Event) -> bool + Send + Sync>) -> Self {
        Self {
            label: label.into(),
            predicate,
        }
    }
}

/// Ordered registry of timing abilities and absolute prohibitions.
///
/// `by_event` is an index only: each bucket stores insertion indexes, so retrieval does not rely
/// on map iteration and exactly preserves the registration order used by the Python oracle.
#[derive(Default, Clone)]
pub struct AbilityRegistry {
    abilities: Vec<Ability>,
    by_event: BTreeMap<String, BTreeMap<Relation, Vec<usize>>>,
    cannot: Vec<CannotEffect>,
}

/// A timing-window resolver.
///
/// It owns registration, player order, and decision routing. Frequency scopes are deliberately
/// not consumed here: M03-012 adds those cross-event lifecycle rules after this resolver's window
/// ordering contract is stable.
pub struct Resolver {
    registry: AbilityRegistry,
    initiative_order: Vec<PlayerId>,
    seating_order: Vec<PlayerId>,
    active_player: Option<PlayerId>,
    speaker: Option<PlayerId>,
    phase: Phase,
    table: Table,
    log: Vec<String>,
    relation_being_resolved: Option<Relation>,
    emission_stack: Vec<(String, u64)>,
    maximum_depth: usize,
    used: BTreeSet<(String, FrequencyScope)>,
    round_number: u64,
    turn_number: u64,
}

impl Resolver {
    /// Construct an action-phase resolver with the supplied initiative order and table.
    #[must_use]
    pub fn new(
        initiative_order: Vec<PlayerId>,
        active_player: Option<PlayerId>,
        table: Table,
    ) -> Self {
        Self {
            registry: AbilityRegistry::default(),
            initiative_order,
            seating_order: Vec::new(),
            active_player,
            speaker: None,
            phase: Phase::Action,
            table,
            log: Vec::new(),
            relation_being_resolved: None,
            emission_stack: Vec::new(),
            maximum_depth: Self::DEFAULT_MAXIMUM_DEPTH,
            used: BTreeSet::new(),
            round_number: 1,
            turn_number: 1,
        }
    }

    /// Configure the game phase used to select window priority order.
    pub fn set_phase(&mut self, phase: Phase) {
        self.phase = phase;
    }

    /// Configure clockwise seating order for strategy and agenda timing windows.
    pub fn set_seating_order(&mut self, seating_order: Vec<PlayerId>) {
        self.seating_order = seating_order;
    }

    /// Configure the active player for initiative-ordered timing windows.
    pub fn set_active_player(&mut self, active_player: Option<PlayerId>) {
        self.active_player = active_player;
    }

    /// Configure the speaker for seating-ordered timing windows.
    pub fn set_speaker(&mut self, speaker: Option<PlayerId>) {
        self.speaker = speaker;
    }

    /// Synchronize frequency-scope counters from the game driver before an emission.
    ///
    /// The state owns these counters. Copying their exact values prevents a timing ability from
    /// retaining once-per-turn or once-per-round usage past the driver's corresponding boundary.
    pub(crate) fn sync_lifecycle(&mut self, round_number: u32, turn_number: u32) {
        self.round_number = u64::from(round_number);
        self.turn_number = u64::from(turn_number);
    }

    /// Advance the turn counter and change the active player.
    ///
    /// # Errors
    /// Returns [`TimingError::CounterExhausted`] instead of wrapping the replay-visible counter.
    pub fn begin_turn(&mut self, active_player: PlayerId) -> Result<(), TimingError> {
        self.turn_number = self
            .turn_number
            .checked_add(1)
            .ok_or(TimingError::CounterExhausted("turn"))?;
        self.active_player = Some(active_player);
        Ok(())
    }

    /// Advance the round and turn counters.
    ///
    /// Existing usage entries need not be removed: their scope keys include the old counter, so
    /// they can no longer match. This exactly preserves the oracle's set semantics while keeping
    /// the mutation path atomic.
    ///
    /// # Errors
    /// Returns [`TimingError::CounterExhausted`] instead of wrapping either counter.
    pub fn begin_round(&mut self) -> Result<(), TimingError> {
        let round_number = self
            .round_number
            .checked_add(1)
            .ok_or(TimingError::CounterExhausted("round"))?;
        let turn_number = self
            .turn_number
            .checked_add(1)
            .ok_or(TimingError::CounterExhausted("turn"))?;
        self.round_number = round_number;
        self.turn_number = turn_number;
        Ok(())
    }

    /// Borrow the table that answers optional timing choices.
    pub fn table_mut(&mut self) -> &mut Table {
        &mut self.table
    }

    /// The relation currently being resolved, if a timing window is open.
    #[must_use]
    pub const fn relation_being_resolved(&self) -> Option<Relation> {
        self.relation_being_resolved
    }

    /// Read the deterministic resolution trace accumulated so far.
    #[must_use]
    pub fn log(&self) -> &[String] {
        &self.log
    }

    /// Default maximum count of simultaneously resolving events.
    pub const DEFAULT_MAXIMUM_DEPTH: usize = 100;

    /// Set the maximum simultaneous event depth for a bounded resolver.
    ///
    /// # Panics
    /// Panics if `maximum_depth` is zero, because even a root emission then has no legal state.
    pub fn set_maximum_depth(&mut self, maximum_depth: usize) {
        assert!(
            maximum_depth > 0,
            "a resolver needs room for its root event"
        );
        self.maximum_depth = maximum_depth;
    }

    /// Emit an event through WHEN, ordinary resolution, and AFTER windows.
    ///
    /// # Errors
    /// Returns an error for an illegal decider answer or an exhausted nested-emission depth budget.
    pub fn emit(
        &mut self,
        mut event: Event,
        resolve: impl FnOnce(&mut Event),
    ) -> Result<Event, TimingError> {
        if self.emission_stack.len() == self.maximum_depth {
            return Err(TimingError::NestedEmissionDepthExceeded {
                maximum_depth: self.maximum_depth,
                event_chain: self.emission_stack.clone(),
            });
        }
        self.emission_stack
            .push((event.event_type.clone(), event.id));
        self.log
            .push(format!("emit {}#{}", event.event_type, event.id));
        let result = (|| {
            self.run_window(&mut event, Relation::When)?;

            if event.cancelled {
                self.log
                    .push(format!("  {}#{} cancelled", event.event_type, event.id));
                return Ok(event);
            }

            if let Some(label) = self.is_forbidden(&event).map(str::to_owned) {
                event.cancel();
                self.log.push(format!(
                    "  {}#{} forbidden by {label}",
                    event.event_type, event.id
                ));
                return Ok(event);
            }

            self.log
                .push(format!("  resolve {}#{}", event.event_type, event.id));
            resolve(&mut event);
            self.run_window(&mut event, Relation::After)?;
            Ok(event)
        })();
        self.emission_stack.pop();
        result
    }

    /// Emit a driver-owned event through timing windows with the active game services.
    ///
    /// WHEN abilities run before `resolve`, and may cancel the event. The ordinary transition
    /// and AFTER abilities therefore share the same [`TimingContext`] and cannot escape the
    /// game's choice or entropy boundaries.
    ///
    /// # Errors
    ///
    /// Returns [`TimingError`] for an invalid timing choice, depth exhaustion, or an attempt to
    /// evaluate a stateful rule outside the supplied context.
    pub fn emit_with_context(
        &mut self,
        context: &mut TimingContext<'_>,
        mut event: Event,
        resolve: impl FnOnce(&mut Event, &mut TimingContext<'_>),
    ) -> Result<Event, TimingError> {
        if self.emission_stack.len() == self.maximum_depth {
            return Err(TimingError::NestedEmissionDepthExceeded {
                maximum_depth: self.maximum_depth,
                event_chain: self.emission_stack.clone(),
            });
        }
        self.emission_stack
            .push((event.event_type.clone(), event.id));
        self.log
            .push(format!("emit {}#{}", event.event_type, event.id));
        let result = (|| {
            self.run_window_with_context(&mut event, Relation::When, context)?;

            if event.cancelled {
                self.log
                    .push(format!("  {}#{} cancelled", event.event_type, event.id));
                return Ok(event);
            }

            if let Some(label) = self.is_forbidden(&event).map(str::to_owned) {
                event.cancel();
                self.log.push(format!(
                    "  {}#{} forbidden by {label}",
                    event.event_type, event.id
                ));
                return Ok(event);
            }

            self.log
                .push(format!("  resolve {}#{}", event.event_type, event.id));
            resolve(&mut event, context);
            self.run_window_with_context(&mut event, Relation::After, context)?;
            Ok(event)
        })();
        self.emission_stack.pop();
        result
    }

    /// Register abilities in the supplied order.
    pub fn register(&mut self, abilities: impl IntoIterator<Item = Ability>) {
        self.registry.register(abilities);
    }

    /// Return matching abilities in their original registration order.
    pub fn for_event(
        &self,
        event_type: &str,
        relation: Relation,
    ) -> impl Iterator<Item = &Ability> {
        self.registry.for_event(event_type, relation)
    }

    /// Register an absolute "cannot" effect. Registrations are intentionally persistent.
    pub fn forbid(
        &mut self,
        label: impl Into<String>,
        predicate: Arc<dyn Fn(&Event) -> bool + Send + Sync>,
    ) {
        self.registry.forbid(label, predicate);
    }

    /// Return the first registered absolute prohibition that matches an event.
    #[must_use]
    pub fn is_forbidden(&self, event: &Event) -> Option<&str> {
        self.registry.is_forbidden(event)
    }

    fn run_window(&mut self, event: &mut Event, relation: Relation) -> Result<(), TimingError> {
        let previous = self.relation_being_resolved;
        self.relation_being_resolved = Some(relation);
        let mut resolved_here = BTreeSet::<String>::new();
        let result = (|| {
            let mut passed = BTreeSet::<PlayerId>::new();
            loop {
                let mut resolved_this_pass = false;
                for player in self.player_order() {
                    if passed.contains(&player) {
                        continue;
                    }
                    let eligible = self.eligible(event, &player, relation, &resolved_here);
                    if eligible.is_empty() {
                        continue;
                    }
                    let Some(ability) = self.pick(eligible, event, &player, relation)? else {
                        passed.insert(player);
                        continue;
                    };
                    if ability.stateful_effect.is_some() || ability.stateful_condition.is_some() {
                        return Err(TimingError::StatefulContextRequired(ability.id));
                    }
                    resolved_here.insert(ability.id.clone());
                    self.mark_used(&ability, event);
                    self.log.push(format!(
                        "  [{}] {} -> {}",
                        relation_name(relation),
                        player,
                        ability.id
                    ));
                    (ability.effect)(event, self)?;
                    resolved_this_pass = true;
                    if event.cancelled {
                        return Ok(());
                    }
                }
                if !resolved_this_pass {
                    return Ok(());
                }
                passed.clear();
            }
        })();
        self.relation_being_resolved = previous;
        result
    }

    fn run_window_with_context(
        &mut self,
        event: &mut Event,
        relation: Relation,
        context: &mut TimingContext<'_>,
    ) -> Result<(), TimingError> {
        let previous = self.relation_being_resolved;
        self.relation_being_resolved = Some(relation);
        let mut resolved_here = BTreeSet::<String>::new();
        let result = (|| {
            let mut passed = BTreeSet::<PlayerId>::new();
            loop {
                let mut resolved_this_pass = false;
                for player in self.player_order() {
                    if passed.contains(&player) {
                        continue;
                    }
                    let eligible = self.eligible_with_context(
                        event,
                        &player,
                        relation,
                        &resolved_here,
                        context,
                    );
                    if eligible.is_empty() {
                        continue;
                    }
                    let Some(ability) =
                        self.pick_with_context(eligible, event, &player, relation, context)?
                    else {
                        passed.insert(player);
                        continue;
                    };
                    resolved_here.insert(ability.id.clone());
                    self.mark_used(&ability, event);
                    self.log.push(format!(
                        "  [{}] {} -> {}",
                        relation_name(relation),
                        player,
                        ability.id
                    ));
                    if let Some(effect) = ability.stateful_effect {
                        effect(event, self, context)?;
                    } else {
                        (ability.effect)(event, self)?;
                    }
                    resolved_this_pass = true;
                    if event.cancelled {
                        return Ok(());
                    }
                }
                if !resolved_this_pass {
                    return Ok(());
                }
                passed.clear();
            }
        })();
        self.relation_being_resolved = previous;
        result
    }

    fn eligible(
        &self,
        event: &Event,
        player: &PlayerId,
        relation: Relation,
        resolved_here: &BTreeSet<String>,
    ) -> Vec<Ability> {
        self.for_event(&event.event_type, relation)
            .filter(|ability| {
                ability.owner == *player
                    && (ability.repeatable_in_window || !resolved_here.contains(&ability.id))
                    && !self.is_used(ability, event)
                    && ability
                        .condition
                        .as_ref()
                        .is_none_or(|condition| condition(event, self))
            })
            .cloned()
            .collect()
    }

    fn eligible_with_context(
        &self,
        event: &Event,
        player: &PlayerId,
        relation: Relation,
        resolved_here: &BTreeSet<String>,
        context: &TimingContext<'_>,
    ) -> Vec<Ability> {
        self.for_event(&event.event_type, relation)
            .filter(|ability| {
                ability.owner == *player
                    && (ability.repeatable_in_window || !resolved_here.contains(&ability.id))
                    && !self.is_used(ability, event)
                    && ability
                        .condition
                        .as_ref()
                        .is_none_or(|condition| condition(event, self))
                    && ability
                        .stateful_condition
                        .as_ref()
                        .is_none_or(|condition| condition(event, self, context))
            })
            .cloned()
            .collect()
    }

    fn pick(
        &mut self,
        eligible: Vec<Ability>,
        event: &Event,
        player: &PlayerId,
        relation: Relation,
    ) -> Result<Option<Ability>, TimingError> {
        if eligible.len() == 1 && !eligible[0].optional {
            return Ok(eligible.into_iter().next());
        }
        let declinable = eligible.iter().any(|ability| ability.optional);
        let mut options = eligible
            .iter()
            .map(|ability| {
                let mut option = ChoiceOption::labelled(&ability.id, "ability", &ability.id)
                    .with("event", event.event_type.clone());
                if let Some(payload) = &ability.option_payload {
                    option.payload.extend(payload(event, self));
                }
                option
            })
            .collect::<Vec<_>>();
        if declinable {
            options.push(ChoiceOption::decline());
        }
        let choice = Choice::new(
            player.clone(),
            format!("{} {}", relation_name(relation), event.event_type),
            options,
        );
        let chosen = self
            .table
            .ask(&choice)
            .map_err(TimingError::IllegalChoice)?;
        if chosen.is_decline() {
            self.log.push(format!(
                "  [{}] {} declines",
                relation_name(relation),
                player
            ));
            return Ok(None);
        }
        Ok(eligible.into_iter().find(|ability| ability.id == chosen.id))
    }

    fn pick_with_context(
        &mut self,
        eligible: Vec<Ability>,
        event: &Event,
        player: &PlayerId,
        relation: Relation,
        context: &mut TimingContext<'_>,
    ) -> Result<Option<Ability>, TimingError> {
        if eligible.len() == 1 && !eligible[0].optional {
            return Ok(eligible.into_iter().next());
        }
        let declinable = eligible.iter().any(|ability| ability.optional);
        let mut options = eligible
            .iter()
            .map(|ability| {
                let mut option = ChoiceOption::labelled(&ability.id, "ability", &ability.id)
                    .with("event", event.event_type.clone());
                if let Some(payload) = &ability.option_payload {
                    option.payload.extend(payload(event, self));
                }
                option
            })
            .collect::<Vec<_>>();
        if declinable {
            options.push(ChoiceOption::decline());
        }
        let choice = Choice::new(
            player.clone(),
            format!("{} {}", relation_name(relation), event.event_type),
            options,
        );
        let chosen = context
            .ask_seeing(&choice)
            .map_err(TimingError::IllegalChoice)?;
        if chosen.is_decline() {
            self.log.push(format!(
                "  [{}] {} declines",
                relation_name(relation),
                player
            ));
            return Ok(None);
        }
        Ok(eligible.into_iter().find(|ability| ability.id == chosen.id))
    }

    fn player_order(&self) -> Vec<PlayerId> {
        let (sequence, first) = if self.phase.uses_speaker_order() {
            let sequence = if self.seating_order.is_empty() {
                &self.initiative_order
            } else {
                &self.seating_order
            };
            (sequence, self.speaker.as_ref())
        } else {
            (&self.initiative_order, self.active_player.as_ref())
        };
        let Some(first) = first else {
            return sequence.clone();
        };
        let Some(index) = sequence.iter().position(|player| player == first) else {
            let mut order = vec![first.clone()];
            order.extend(sequence.iter().filter(|player| *player != first).cloned());
            return order;
        };
        sequence[index..]
            .iter()
            .chain(&sequence[..index])
            .cloned()
            .collect()
    }

    fn scope_key(&self, ability: &Ability, event: &Event) -> Option<(String, FrequencyScope)> {
        let scope = match ability.frequency {
            Frequency::OncePerTrigger => FrequencyScope::Trigger(event.id),
            Frequency::OncePerTurn => FrequencyScope::Turn(self.turn_number),
            Frequency::OncePerRound => FrequencyScope::Round(self.round_number),
            Frequency::Unlimited => return None,
        };
        Some((ability.id.clone(), scope))
    }

    fn is_used(&self, ability: &Ability, event: &Event) -> bool {
        self.scope_key(ability, event)
            .is_some_and(|key| self.used.contains(&key))
    }

    fn mark_used(&mut self, ability: &Ability, event: &Event) {
        if let Some(key) = self.scope_key(ability, event) {
            self.used.insert(key);
        }
    }
}

impl Default for Resolver {
    fn default() -> Self {
        Self::new(Vec::new(), None, Table::default())
    }
}

/// A resolver failure at the generated-choice boundary.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TimingError {
    /// A decider attempted to choose an option that this timing window did not offer.
    #[error(transparent)]
    IllegalChoice(IllegalChoice),
    /// A nested event would exceed the resolver's explicit depth budget.
    #[error("nested emission depth exceeded (maximum {maximum_depth}): {event_chain:?}")]
    NestedEmissionDepthExceeded {
        /// Highest permitted simultaneously resolving-event count.
        maximum_depth: usize,
        /// Root-to-leaf chain that was already open when the next event was refused.
        event_chain: Vec<(String, u64)>,
    },
    /// A timing lifecycle counter cannot advance without losing replay identity.
    #[error("timing {0} counter is exhausted")]
    CounterExhausted(&'static str),
    /// A stateful rule could not allocate a distinct nested typed event.
    #[error(transparent)]
    EventSequence(#[from] EventSequenceError),
    /// A stateful rule was emitted without the active game's services.
    #[error("stateful timing ability {0:?} requires a game timing context")]
    StatefulContextRequired(String),
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum FrequencyScope {
    Trigger(u64),
    Turn(u64),
    Round(u64),
}

/// The lowercase relation word the oracle uses in ability ids and prompts (`"when"` / `"after"`).
pub(crate) const fn relation_name(relation: Relation) -> &'static str {
    match relation {
        Relation::When => "when",
        Relation::After => "after",
    }
}

impl AbilityRegistry {
    /// Register abilities in the supplied order.
    ///
    /// Duplicate identifiers are retained because the oracle's registry does not reject them;
    /// the resolver later applies its per-window semantics to registered slots.
    pub fn register(&mut self, abilities: impl IntoIterator<Item = Ability>) {
        for ability in abilities {
            let index = self.abilities.len();
            self.by_event
                .entry(ability.event_type.clone())
                .or_default()
                .entry(ability.relation)
                .or_default()
                .push(index);
            self.abilities.push(ability);
        }
    }

    /// Return matching abilities in their original registration order.
    pub fn for_event(
        &self,
        event_type: &str,
        relation: Relation,
    ) -> impl Iterator<Item = &Ability> {
        self.by_event
            .get(event_type)
            .and_then(|by_relation| by_relation.get(&relation))
            .into_iter()
            .flatten()
            .map(|&index| &self.abilities[index])
    }

    /// Register an absolute "cannot" effect. Registrations are intentionally persistent.
    pub fn forbid(
        &mut self,
        label: impl Into<String>,
        predicate: Arc<dyn Fn(&Event) -> bool + Send + Sync>,
    ) {
        self.cannot.push(CannotEffect::new(label, predicate));
    }

    /// Return the first registered absolute prohibition that matches an event.
    #[must_use]
    pub fn is_forbidden(&self, event: &Event) -> Option<&str> {
        self.cannot
            .iter()
            .find(|cannot| (cannot.predicate)(event))
            .map(|cannot| cannot.label.as_str())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    };

    use super::*;
    use crate::choice::{AlwaysDecline, Scripted};
    use crate::{dice::Dice, rng::GameRng};
    use proptest::prelude::*;
    use ti4_content::ContentStore;
    use ti4_model::content_types::POK;

    type GeneratedAbility = (u8, bool, bool, bool, u8, bool);

    fn generated_registry() -> impl Strategy<Value = Vec<GeneratedAbility>> {
        prop::collection::vec(
            (
                0_u8..2,
                any::<bool>(),
                any::<bool>(),
                any::<bool>(),
                0_u8..4,
                any::<bool>(),
            ),
            0..25,
        )
    }

    const fn generated_frequency(value: u8) -> Frequency {
        match value {
            0 => Frequency::OncePerTrigger,
            1 => Frequency::OncePerTurn,
            2 => Frequency::OncePerRound,
            3 => Frequency::Unlimited,
            _ => unreachable!(),
        }
    }

    fn generated_owner(value: u8) -> &'static str {
        match value {
            0 => "sol",
            1 => "letnev",
            _ => unreachable!(),
        }
    }

    fn generated_relation(is_when: bool) -> Relation {
        if is_when {
            Relation::When
        } else {
            Relation::After
        }
    }

    fn generated_ability(
        id: String,
        (owner, is_when, event_matches, enabled, frequency, repeatable): GeneratedAbility,
        fired: Arc<Mutex<Vec<String>>>,
    ) -> Ability {
        let available = Arc::new(AtomicBool::new(enabled));
        let condition_available = available.clone();
        let effect_available = available.clone();
        let effect_id = id.clone();
        Ability::new(
            id,
            PlayerId::new(generated_owner(owner)),
            if event_matches { "E" } else { "OTHER" },
            generated_relation(is_when),
            Arc::new(move |_, _| {
                fired.lock().unwrap().push(effect_id.clone());
                // An unlimited repeatable reaction slot is legal only while its backing
                // rule resource remains available. Model its effect consuming that resource
                // so generated cases exercise this real termination precondition.
                effect_available.store(false, Ordering::SeqCst);
                Ok(())
            }),
        )
        .with_condition(Arc::new(move |_, _| {
            condition_available.load(Ordering::SeqCst)
        }))
        .with_frequency(generated_frequency(frequency))
        .with_repeatable_in_window(repeatable)
    }

    fn generated_trace(specification: &[GeneratedAbility]) -> Vec<String> {
        let fired = Arc::new(Mutex::new(Vec::new()));
        let mut timing = resolver(&["sol", "letnev"], "sol");
        timing.register(specification.iter().enumerate().map(
            |(index, &(owner, is_when, event_matches, enabled, frequency, repeatable))| {
                generated_ability(
                    format!("generated-{index}"),
                    (
                        owner,
                        is_when,
                        event_matches,
                        enabled,
                        frequency,
                        repeatable,
                    ),
                    fired.clone(),
                )
            },
        ));
        timing
            .emit(Event::new(1, "E", BTreeMap::new()), |_| {})
            .unwrap();
        timing.log().to_vec()
    }

    fn ability(id: &str, event_type: &str, relation: Relation) -> Ability {
        Ability::new(
            id,
            PlayerId::new("sol"),
            event_type,
            relation,
            Arc::new(|_, _| Ok(())),
        )
    }

    #[test]
    fn registration_is_partitioned_by_event_and_relation_without_reordering() {
        let mut resolver = Resolver::default();
        resolver.register([
            ability("first", "MOVE", Relation::When),
            ability("other", "COMBAT", Relation::When),
            ability("after", "MOVE", Relation::After),
            ability("second", "MOVE", Relation::When),
        ]);

        assert_eq!(
            resolver
                .for_event("MOVE", Relation::When)
                .map(|ability| ability.id.as_str())
                .collect::<Vec<_>>(),
            ["first", "second"]
        );
        assert_eq!(
            resolver
                .for_event("MOVE", Relation::After)
                .map(|ability| ability.id.as_str())
                .collect::<Vec<_>>(),
            ["after"]
        );
        assert!(
            resolver
                .for_event("ABSENT", Relation::When)
                .next()
                .is_none()
        );
    }

    #[test]
    fn ability_defaults_and_builder_metadata_match_the_oracle() {
        let ability = ability("reaction", "CARD_PLAYED", Relation::After)
            .with_frequency(Frequency::OncePerRound)
            .with_optional(true)
            .with_repeatable_in_window(true)
            .with_condition(Arc::new(|event, _| event.text("owner") == Some("sol")))
            .with_option_payload(Arc::new(|event, _| {
                [("event_id".to_owned(), Value::from(event.id))]
                    .into_iter()
                    .collect()
            }));
        let event = Event::new(
            7,
            "CARD_PLAYED",
            [("owner".to_owned(), Value::from("sol"))]
                .into_iter()
                .collect(),
        );

        assert_eq!(ability.frequency, Frequency::OncePerRound);
        assert!(ability.optional);
        assert!(ability.repeatable_in_window);
        assert!((ability.condition.as_ref().unwrap())(
            &event,
            &Resolver::default()
        ));
        assert_eq!(
            (ability.option_payload.as_ref().unwrap())(&event, &Resolver::default()),
            [("event_id".to_owned(), Value::from(7))]
                .into_iter()
                .collect()
        );
    }

    #[test]
    fn cannot_effects_are_persistent_and_first_match_wins() {
        let mut resolver = Resolver::default();
        resolver.forbid("first", Arc::new(|_| true));
        resolver.forbid("later", Arc::new(|_| true));
        let event = Event::new(1, "MOVE", BTreeMap::new());

        assert_eq!(resolver.is_forbidden(&event), Some("first"));
        assert_eq!(resolver.is_forbidden(&event), Some("first"));
    }

    fn player(name: &str) -> PlayerId {
        PlayerId::new(name)
    }

    fn resolver(players: &[&str], active_player: &str) -> Resolver {
        Resolver::new(
            players
                .iter()
                .map(|player| PlayerId::new(*player))
                .collect(),
            Some(player(active_player)),
            Table::default(),
        )
    }

    fn stateful_context<'a>(
        state: &'a mut GameState,
        table: &'a mut Table,
        dice: &'a mut Dice,
        rng: &'a mut GameRng,
        event_sequence: &'a mut EventSequence,
    ) -> TimingContext<'a> {
        TimingContext {
            state,
            content: ContentStore::embedded(),
            sources: POK,
            table,
            dice,
            rng,
            event_sequence,
            galaxy: None,
        }
    }

    #[test]
    fn stateful_abilities_mutate_the_active_game_context() {
        let owner = player("sol");
        let mut state = GameState::new(std::slice::from_ref(&owner), &[], BTreeMap::new(), None, 1);
        let mut table = Table::new();
        let mut dice = Dice::new();
        let mut rng = GameRng::new(0);
        let mut event_sequence = EventSequence::new();
        let mut context = stateful_context(
            &mut state,
            &mut table,
            &mut dice,
            &mut rng,
            &mut event_sequence,
        );
        let mut timing = resolver(&["sol"], "sol");
        timing.register([Ability::stateful(
            "gain-point",
            owner.clone(),
            "E",
            Relation::When,
            Arc::new(move |_, _, context| {
                context
                    .state
                    .player_mut(&owner)
                    .expect("owner is seated")
                    .victory_points += 1;
                Ok(())
            }),
        )]);

        timing
            .emit_with_context(
                &mut context,
                Event::new(1, "E", BTreeMap::new()),
                |_, context| {
                    context.state.round = 2;
                },
            )
            .unwrap();

        assert_eq!(
            context.state.player(&player("sol")).unwrap().victory_points,
            1
        );
        assert_eq!(context.state.round, 2);
    }

    #[test]
    fn stateful_nested_events_share_the_game_event_sequence() {
        let owner = player("sol");
        let mut state = GameState::new(std::slice::from_ref(&owner), &[], BTreeMap::new(), None, 1);
        let mut table = Table::new();
        let mut dice = Dice::new();
        let mut rng = GameRng::new(0);
        let mut event_sequence = EventSequence::new();
        let mut context = stateful_context(
            &mut state,
            &mut table,
            &mut dice,
            &mut rng,
            &mut event_sequence,
        );
        let mut timing = resolver(&["sol"], "sol");
        timing.register([Ability::stateful(
            "nested",
            owner,
            "OUTER",
            Relation::When,
            Arc::new(|_, resolver, context| {
                let inner = context.event_sequence.next("INNER", BTreeMap::new())?;
                resolver.emit_with_context(context, inner, |_, _| {})?;
                Ok(())
            }),
        )]);

        let outer = context
            .event_sequence
            .next("OUTER", BTreeMap::new())
            .unwrap();
        timing
            .emit_with_context(&mut context, outer, |_, _| {})
            .unwrap();

        assert_eq!(
            timing.log(),
            [
                "emit OUTER#1",
                "  [when] sol -> nested",
                "emit INNER#2",
                "  resolve INNER#2",
                "  resolve OUTER#1",
            ]
        );
    }

    #[test]
    fn stateful_abilities_are_refused_without_a_game_context() {
        let mut timing = resolver(&["sol"], "sol");
        timing.register([Ability::stateful(
            "stateful",
            player("sol"),
            "E",
            Relation::When,
            Arc::new(|_, _, _| Ok(())),
        )]);

        assert!(matches!(
            timing.emit(Event::new(1, "E", BTreeMap::new()), |_| {}),
            Err(TimingError::StatefulContextRequired(id)) if id == "stateful"
        ));
    }

    fn recording_ability(
        id: &str,
        owner: &str,
        event_type: &str,
        relation: Relation,
        log: Arc<Mutex<Vec<String>>>,
    ) -> Ability {
        let name = id.to_owned();
        Ability::new(
            id,
            player(owner),
            event_type,
            relation,
            Arc::new(move |_, _| {
                log.lock().unwrap().push(name.clone());
                Ok(())
            }),
        )
    }

    #[test]
    fn when_window_precedes_event_resolution_and_after_window() {
        let order = Arc::new(Mutex::new(Vec::new()));
        let mut timing = resolver(&["sol"], "sol");
        timing.register([
            recording_ability("after", "sol", "E", Relation::After, order.clone()),
            recording_ability("when", "sol", "E", Relation::When, order.clone()),
        ]);

        timing
            .emit(Event::new(1, "E", BTreeMap::new()), |_| {
                order.lock().unwrap().push("resolve".to_owned());
            })
            .unwrap();

        assert_eq!(*order.lock().unwrap(), ["when", "resolve", "after"]);
    }

    #[test]
    fn action_windows_rotate_initiative_from_the_active_player() {
        let order = Arc::new(Mutex::new(Vec::new()));
        let mut timing = resolver(&["sol", "letnev", "xxcha"], "letnev");
        timing.register([
            recording_ability("sol", "sol", "E", Relation::When, order.clone()),
            recording_ability("letnev", "letnev", "E", Relation::When, order.clone()),
            recording_ability("xxcha", "xxcha", "E", Relation::When, order.clone()),
        ]);

        timing
            .emit(Event::new(1, "E", BTreeMap::new()), |_| {})
            .unwrap();

        assert_eq!(*order.lock().unwrap(), ["letnev", "xxcha", "sol"]);
    }

    #[test]
    fn strategy_and_agenda_windows_rotate_seating_from_the_speaker() {
        let order = Arc::new(Mutex::new(Vec::new()));
        let mut timing = resolver(&["xxcha", "sol", "letnev"], "xxcha");
        timing.set_phase(Phase::Agenda);
        timing.set_seating_order(vec![player("sol"), player("letnev"), player("xxcha")]);
        timing.set_speaker(Some(player("letnev")));
        timing.register([
            recording_ability("sol", "sol", "E", Relation::When, order.clone()),
            recording_ability("letnev", "letnev", "E", Relation::When, order.clone()),
            recording_ability("xxcha", "xxcha", "E", Relation::When, order.clone()),
        ]);

        timing
            .emit(Event::new(1, "E", BTreeMap::new()), |_| {})
            .unwrap();

        assert_eq!(*order.lock().unwrap(), ["letnev", "xxcha", "sol"]);
    }

    #[test]
    fn players_resolve_one_ability_each_before_the_next_pass() {
        let order = Arc::new(Mutex::new(Vec::new()));
        let mut timing = resolver(&["sol", "letnev"], "sol");
        timing.register([
            recording_ability("sol_1", "sol", "E", Relation::When, order.clone()),
            recording_ability("letnev", "letnev", "E", Relation::When, order.clone()),
            recording_ability("sol_2", "sol", "E", Relation::When, order.clone()),
        ]);

        timing
            .emit(Event::new(1, "E", BTreeMap::new()), |_| {})
            .unwrap();

        assert_eq!(*order.lock().unwrap(), ["sol_1", "letnev", "sol_2"]);
    }

    #[test]
    fn a_pass_is_reoffered_after_another_player_resolves() {
        let order = Arc::new(Mutex::new(Vec::new()));
        let mut timing = resolver(&["sol", "letnev"], "sol");
        timing
            .table_mut()
            .seat(player("sol"), Box::new(Scripted::new(["decline", "sol"])));
        let sol =
            recording_ability("sol", "sol", "E", Relation::When, order.clone()).with_optional(true);
        timing.register([
            sol,
            recording_ability("letnev", "letnev", "E", Relation::When, order.clone()),
        ]);

        timing
            .emit(Event::new(1, "E", BTreeMap::new()), |_| {})
            .unwrap();

        assert_eq!(*order.lock().unwrap(), ["letnev", "sol"]);
        assert_eq!(
            timing
                .log()
                .iter()
                .filter(|line| line.contains("sol declines"))
                .count(),
            1
        );
    }

    #[test]
    fn a_when_cancellation_skips_resolution_and_after() {
        let order = Arc::new(Mutex::new(Vec::new()));
        let mut timing = resolver(&["sol", "letnev"], "sol");
        timing.register([
            Ability::new(
                "cancel",
                player("sol"),
                "E",
                Relation::When,
                Arc::new(|event, _| {
                    event.cancel();
                    Ok(())
                }),
            ),
            recording_ability("after", "letnev", "E", Relation::After, order.clone()),
        ]);

        let event = timing
            .emit(Event::new(1, "E", BTreeMap::new()), |_| {
                order.lock().unwrap().push("resolve".to_owned());
            })
            .unwrap();

        assert!(event.cancelled);
        assert!(order.lock().unwrap().is_empty());
    }

    #[test]
    fn a_cannot_cancels_the_event_before_resolution_or_after() {
        let order = Arc::new(Mutex::new(Vec::new()));
        let mut timing = resolver(&["sol"], "sol");
        timing.forbid("cannot", Arc::new(|event| event.event_type == "E"));
        timing.register([recording_ability(
            "after",
            "sol",
            "E",
            Relation::After,
            order.clone(),
        )]);

        let event = timing
            .emit(Event::new(1, "E", BTreeMap::new()), |_| {
                order.lock().unwrap().push("resolve".to_owned());
            })
            .unwrap();

        assert!(event.cancelled);
        assert!(order.lock().unwrap().is_empty());
    }

    #[test]
    fn an_illegal_timing_choice_is_returned_without_executing_the_ability() {
        let order = Arc::new(Mutex::new(Vec::new()));
        let mut timing = resolver(&["sol"], "sol");
        timing
            .table_mut()
            .seat(player("sol"), Box::new(Scripted::new(["invented"])));
        timing.register([
            recording_ability("optional", "sol", "E", Relation::When, order.clone())
                .with_optional(true),
        ]);

        let result = timing.emit(Event::new(1, "E", BTreeMap::new()), |_| {});

        assert!(matches!(result, Err(TimingError::IllegalChoice(_))));
        assert!(order.lock().unwrap().is_empty());
    }

    #[test]
    fn nested_events_resolve_depth_first_before_the_outer_event_continues() {
        let order = Arc::new(Mutex::new(Vec::new()));
        let mut timing = resolver(&["sol"], "sol");
        let outer_order = order.clone();
        let inner_order = order.clone();
        timing.register([
            Ability::new(
                "outer",
                player("sol"),
                "OUTER",
                Relation::When,
                Arc::new(move |_, resolver| {
                    outer_order.lock().unwrap().push("outer_before".to_owned());
                    resolver.emit(Event::new(2, "INNER", BTreeMap::new()), |_| {})?;
                    outer_order.lock().unwrap().push("outer_after".to_owned());
                    Ok(())
                }),
            ),
            Ability::new(
                "inner",
                player("sol"),
                "INNER",
                Relation::After,
                Arc::new(move |_, _| {
                    inner_order.lock().unwrap().push("inner_after".to_owned());
                    Ok(())
                }),
            ),
        ]);

        timing
            .emit(Event::new(1, "OUTER", BTreeMap::new()), |_| {
                order.lock().unwrap().push("outer_resolve".to_owned());
            })
            .unwrap();

        assert_eq!(
            *order.lock().unwrap(),
            [
                "outer_before",
                "inner_after",
                "outer_after",
                "outer_resolve"
            ]
        );
        let trace = timing.log();
        assert!(
            trace
                .iter()
                .position(|line| line.contains("resolve INNER#2"))
                < trace
                    .iter()
                    .position(|line| line.contains("resolve OUTER#1"))
        );
    }

    #[test]
    fn an_inner_cancellation_stays_local_to_the_inner_event() {
        let order = Arc::new(Mutex::new(Vec::new()));
        let mut timing = resolver(&["sol"], "sol");
        let after_order = order.clone();
        timing.register([
            Ability::new(
                "outer",
                player("sol"),
                "OUTER",
                Relation::When,
                Arc::new(|_, resolver| {
                    resolver.emit(Event::new(2, "INNER", BTreeMap::new()), |_| {})?;
                    Ok(())
                }),
            ),
            Ability::new(
                "cancel_inner",
                player("sol"),
                "INNER",
                Relation::When,
                Arc::new(|event, _| {
                    event.cancel();
                    Ok(())
                }),
            ),
            Ability::new(
                "outer_after",
                player("sol"),
                "OUTER",
                Relation::After,
                Arc::new(move |_, _| {
                    after_order.lock().unwrap().push("outer_after".to_owned());
                    Ok(())
                }),
            ),
        ]);

        timing
            .emit(Event::new(1, "OUTER", BTreeMap::new()), |_| {})
            .unwrap();

        assert_eq!(*order.lock().unwrap(), ["outer_after"]);
    }

    #[test]
    fn depth_limit_returns_the_open_event_chain_and_unwinds_cleanly() {
        let mut timing = resolver(&["sol"], "sol");
        timing.set_maximum_depth(2);
        timing.register([
            Ability::new(
                "first",
                player("sol"),
                "A",
                Relation::When,
                Arc::new(|_, resolver| {
                    resolver.emit(Event::new(2, "B", BTreeMap::new()), |_| {})?;
                    Ok(())
                }),
            ),
            Ability::new(
                "second",
                player("sol"),
                "B",
                Relation::When,
                Arc::new(|_, resolver| {
                    resolver.emit(Event::new(3, "C", BTreeMap::new()), |_| {})?;
                    Ok(())
                }),
            ),
        ]);

        let result = timing.emit(Event::new(1, "A", BTreeMap::new()), |_| {});

        assert!(matches!(
            result,
            Err(TimingError::NestedEmissionDepthExceeded {
                maximum_depth: 2,
                event_chain,
            }) if event_chain == vec![("A".to_owned(), 1), ("B".to_owned(), 2)]
        ));
        timing
            .emit(Event::new(4, "SAFE", BTreeMap::new()), |_| {})
            .unwrap();
    }

    #[test]
    fn default_depth_limit_refuses_the_one_hundred_and_first_open_event() {
        let mut timing = resolver(&["sol"], "sol");
        timing.register([Ability::new(
            "loop",
            player("sol"),
            "LOOP",
            Relation::When,
            Arc::new(|event, resolver| {
                resolver.emit(Event::new(event.id + 1, "LOOP", BTreeMap::new()), |_| {})?;
                Ok(())
            }),
        )]);

        let result = timing.emit(Event::new(1, "LOOP", BTreeMap::new()), |_| {});

        assert!(matches!(
            result,
            Err(TimingError::NestedEmissionDepthExceeded {
                maximum_depth: Resolver::DEFAULT_MAXIMUM_DEPTH,
                event_chain,
            }) if event_chain.len() == Resolver::DEFAULT_MAXIMUM_DEPTH
                && event_chain.first() == Some(&("LOOP".to_owned(), 1))
                && event_chain.last() == Some(&("LOOP".to_owned(), 100))
        ));
    }

    #[test]
    fn once_per_trigger_is_available_again_for_a_distinct_event_id() {
        let fired = Arc::new(Mutex::new(Vec::new()));
        let mut timing = resolver(&["sol"], "sol");
        timing.register([recording_ability(
            "once",
            "sol",
            "E",
            Relation::When,
            fired.clone(),
        )]);

        timing
            .emit(Event::new(1, "E", BTreeMap::new()), |_| {})
            .unwrap();
        timing
            .emit(Event::new(2, "E", BTreeMap::new()), |_| {})
            .unwrap();

        assert_eq!(*fired.lock().unwrap(), ["once", "once"]);
    }

    #[test]
    fn once_per_turn_lapses_only_when_the_turn_counter_advances() {
        let fired = Arc::new(Mutex::new(Vec::new()));
        let mut timing = resolver(&["sol"], "sol");
        timing.register([
            recording_ability("turn", "sol", "E", Relation::When, fired.clone())
                .with_frequency(Frequency::OncePerTurn),
        ]);

        timing
            .emit(Event::new(1, "E", BTreeMap::new()), |_| {})
            .unwrap();
        timing
            .emit(Event::new(2, "E", BTreeMap::new()), |_| {})
            .unwrap();
        timing.begin_turn(player("sol")).unwrap();
        timing
            .emit(Event::new(3, "E", BTreeMap::new()), |_| {})
            .unwrap();

        assert_eq!(*fired.lock().unwrap(), ["turn", "turn"]);
    }

    #[test]
    fn once_per_round_survives_turns_and_lapses_when_the_round_advances() {
        let fired = Arc::new(Mutex::new(Vec::new()));
        let mut timing = resolver(&["sol"], "sol");
        timing.register([
            recording_ability("round", "sol", "E", Relation::When, fired.clone())
                .with_frequency(Frequency::OncePerRound),
        ]);

        timing
            .emit(Event::new(1, "E", BTreeMap::new()), |_| {})
            .unwrap();
        timing.begin_turn(player("sol")).unwrap();
        timing
            .emit(Event::new(2, "E", BTreeMap::new()), |_| {})
            .unwrap();
        timing.begin_round().unwrap();
        timing
            .emit(Event::new(3, "E", BTreeMap::new()), |_| {})
            .unwrap();

        assert_eq!(*fired.lock().unwrap(), ["round", "round"]);
    }

    #[test]
    fn unlimited_abilities_remain_available_across_events_and_lifecycle_transitions() {
        let fired = Arc::new(Mutex::new(Vec::new()));
        let mut timing = resolver(&["sol"], "sol");
        timing.register([recording_ability(
            "unlimited",
            "sol",
            "E",
            Relation::When,
            fired.clone(),
        )
        .with_frequency(Frequency::Unlimited)]);

        timing
            .emit(Event::new(1, "E", BTreeMap::new()), |_| {})
            .unwrap();
        timing.begin_turn(player("sol")).unwrap();
        timing
            .emit(Event::new(2, "E", BTreeMap::new()), |_| {})
            .unwrap();
        timing.begin_round().unwrap();
        timing
            .emit(Event::new(3, "E", BTreeMap::new()), |_| {})
            .unwrap();

        assert_eq!(
            *fired.lock().unwrap(),
            ["unlimited", "unlimited", "unlimited"]
        );
    }

    #[test]
    fn oracle_emit_order_fixture_matches_line_for_line() {
        // Generated from the pinned oracle on 2026-08-12 with PYTHONDONTWRITEBYTECODE=1:
        // Resolver(initiative_order=["sol", "letnev"], active_player="sol") with AFTER
        // registered before WHEN, then Event("E") emitted with a non-null no-op resolver.
        let mut timing = resolver(&["sol", "letnev"], "sol");
        timing.register([
            ability("after", "E", Relation::After),
            ability("when", "E", Relation::When),
        ]);

        let event = timing
            .emit(Event::new(1, "E", BTreeMap::new()), |_| {})
            .unwrap();

        assert_eq!(event.id, 1);
        assert!(!event.cancelled);
        assert_eq!(
            timing.log(),
            [
                "emit E#1",
                "  [when] sol -> when",
                "  resolve E#1",
                "  [after] sol -> after",
            ]
        );
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(128))]

        #[test]
        fn generated_self_consuming_registries_terminate(specification in generated_registry()) {
            let fired = Arc::new(Mutex::new(Vec::new()));
            let mut timing = resolver(&["sol", "letnev"], "sol");
            timing.register(specification.iter().enumerate().map(
                |(index, &(owner, is_when, event_matches, enabled, frequency, repeatable))| {
                    generated_ability(
                        format!("generated-{index}"),
                        (owner, is_when, event_matches, enabled, frequency, repeatable),
                        fired.clone(),
                    )
                },
            ));

            timing.emit(Event::new(1, "E", BTreeMap::new()), |_| {}).unwrap();

            let maximum_resolutions = specification
                .iter()
                .filter(|(_, _, event_matches, enabled, _, _)| *event_matches && *enabled)
                .count();
            prop_assert!(fired.lock().unwrap().len() <= maximum_resolutions);
        }

        #[test]
        fn frequency_scopes_bound_each_generated_ability(frequencies in prop::collection::vec(0_u8..4, 1..17)) {
            let fired = Arc::new(Mutex::new(BTreeMap::<String, usize>::new()));
            let mut timing = resolver(&["sol"], "sol");
            timing.register(frequencies.iter().enumerate().map(|(index, &frequency)| {
                let id = format!("frequency-{index}");
                let effect_id = id.clone();
                let fired = fired.clone();
                Ability::new(
                    id,
                    player("sol"),
                    "E",
                    Relation::When,
                    Arc::new(move |_, _| {
                        *fired.lock().unwrap().entry(effect_id.clone()).or_default() += 1;
                        Ok(())
                    }),
                )
                .with_frequency(generated_frequency(frequency))
            }));

            for event_id in [1, 2] {
                timing.emit(Event::new(event_id, "E", BTreeMap::new()), |_| {}).unwrap();
            }
            timing.begin_turn(player("sol")).unwrap();
            timing.emit(Event::new(3, "E", BTreeMap::new()), |_| {}).unwrap();
            timing.begin_round().unwrap();
            timing.emit(Event::new(4, "E", BTreeMap::new()), |_| {}).unwrap();

            let counts = fired.lock().unwrap();
            for (index, &frequency) in frequencies.iter().enumerate() {
                let expected = match generated_frequency(frequency) {
                    Frequency::OncePerTrigger | Frequency::Unlimited => 4,
                    Frequency::OncePerTurn => 3,
                    Frequency::OncePerRound => 2,
                };
                prop_assert_eq!(counts[&format!("frequency-{index}")], expected);
            }
        }

        #[test]
        fn generated_ineligible_abilities_never_execute(specification in generated_registry()) {
            let fired = Arc::new(Mutex::new(Vec::new()));
            let mut timing = resolver(&["sol", "letnev"], "sol");
            timing.register(specification.iter().enumerate().map(
                |(index, &(owner, is_when, event_matches, enabled, frequency, repeatable))| {
                    generated_ability(
                        format!("generated-{index}"),
                        (owner, is_when, event_matches, enabled, frequency, repeatable),
                        fired.clone(),
                    )
                },
            ));
            timing.emit(Event::new(1, "E", BTreeMap::new()), |_| {}).unwrap();

            let eligible_ids = specification
                .iter()
                .enumerate()
                .filter(|(_, (_, _, event_matches, enabled, _, _))| *event_matches && *enabled)
                .map(|(index, _)| format!("generated-{index}"))
                .collect::<BTreeSet<_>>();
            prop_assert!(fired.lock().unwrap().iter().all(|id| eligible_ids.contains(id)));
        }

        #[test]
        fn nonrepeatable_duplicate_identifiers_resolve_once_per_window(slot_ids in prop::collection::vec(0_u8..6, 0..25)) {
            let fired = Arc::new(Mutex::new(Vec::new()));
            let mut timing = resolver(&["sol"], "sol");
            timing.register(slot_ids.iter().enumerate().map(|(index, slot_id)| {
                let fired = fired.clone();
                let effect_slot = format!("slot-{index}");
                Ability::new(
                    format!("duplicate-{slot_id}"),
                    player("sol"),
                    "E",
                    Relation::When,
                    Arc::new(move |_, _| {
                        fired.lock().unwrap().push(effect_slot.clone());
                        Ok(())
                    }),
                )
            }));
            timing.emit(Event::new(1, "E", BTreeMap::new()), |_| {}).unwrap();

            prop_assert!(fired.lock().unwrap().len() <= slot_ids.iter().collect::<BTreeSet<_>>().len());
        }

        #[test]
        fn generated_registry_trace_is_deterministic(specification in generated_registry()) {
            prop_assert_eq!(generated_trace(&specification), generated_trace(&specification));
        }

        #[test]
        fn generated_optional_passes_terminate(slot_owners in prop::collection::vec(0_u8..2, 1..25)) {
            let mut timing = Resolver::new(
                vec![player("sol"), player("letnev")],
                Some(player("sol")),
                Table::with_default(Box::new(AlwaysDecline)),
            );
            timing.register(slot_owners.iter().enumerate().map(|(index, owner)| {
                Ability::new(
                    format!("optional-{index}"),
                    PlayerId::new(generated_owner(*owner)),
                    "E",
                    Relation::When,
                    Arc::new(|_, _| Ok(())),
                )
                .with_optional(true)
            }));
            timing.emit(Event::new(1, "E", BTreeMap::new()), |_| {}).unwrap();

            let pass_count = timing.log().iter().filter(|line| line.contains("declines")).count();
            prop_assert!(pass_count <= 2);
            prop_assert!(!timing.log().iter().any(|line| line.contains("-> optional-")));
        }
    }
}
