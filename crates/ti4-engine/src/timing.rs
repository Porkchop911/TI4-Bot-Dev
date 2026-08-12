//! Deterministic timing-ability registration.
//!
//! This module owns only the data and ordered registry needed to open a timing window. Event
//! execution, player priority, nested emission, and frequency consumption deliberately remain in
//! their later M03 packages.

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use ti4_model::id::PlayerId;

use crate::{
    choice::{Choice, ChoiceOption, IllegalChoice, Table},
    event::Event,
};
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
pub type AbilityEffect = Arc<dyn Fn(&mut Event, &mut Resolver) + Send + Sync>;

/// A rule-specific eligibility predicate.
pub type AbilityCondition = Arc<dyn Fn(&Event, &Resolver) -> bool + Send + Sync>;

/// Factual context copied onto a choice offered for an optional ability.
pub type OptionPayload = Arc<dyn Fn(&Event, &Resolver) -> BTreeMap<String, Value> + Send + Sync>;

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
    /// Rule effect, invoked only by the later resolver.
    pub effect: AbilityEffect,
    /// Optional rule-specific eligibility gate.
    pub condition: Option<AbilityCondition>,
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
            condition: None,
            frequency: Frequency::OncePerTrigger,
            optional: false,
            repeatable_in_window: false,
            option_payload: None,
        }
    }

    /// Attach a rule-specific eligibility predicate.
    #[must_use]
    pub fn with_condition(mut self, condition: AbilityCondition) -> Self {
        self.condition = Some(condition);
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

    /// Emit an event through WHEN, ordinary resolution, and AFTER windows.
    ///
    /// # Errors
    /// Returns [`TimingError::IllegalChoice`] if a decider attempts an option that was not offered.
    pub fn emit(
        &mut self,
        mut event: Event,
        resolve: impl FnOnce(&mut Event),
    ) -> Result<Event, TimingError> {
        self.log
            .push(format!("emit {}#{}", event.event_type, event.id));
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
                    resolved_here.insert(ability.id.clone());
                    self.log.push(format!(
                        "  [{}] {} -> {}",
                        relation_name(relation),
                        player,
                        ability.id
                    ));
                    (ability.effect)(event, self);
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
                    && ability
                        .condition
                        .as_ref()
                        .is_none_or(|condition| condition(event, self))
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
}

impl Default for Resolver {
    fn default() -> Self {
        Self::new(Vec::new(), None, Table::default())
    }
}

/// A resolver failure at the generated-choice boundary.
#[derive(Debug, thiserror::Error)]
pub enum TimingError {
    /// A decider attempted to choose an option that this timing window did not offer.
    #[error(transparent)]
    IllegalChoice(IllegalChoice),
}

const fn relation_name(relation: Relation) -> &'static str {
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
    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::choice::Scripted;

    fn ability(id: &str, event_type: &str, relation: Relation) -> Ability {
        Ability::new(
            id,
            PlayerId::new("sol"),
            event_type,
            relation,
            Arc::new(|_, _| {}),
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
            Arc::new(move |_, _| log.lock().unwrap().push(name.clone())),
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
                Arc::new(|event, _| event.cancel()),
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
}
