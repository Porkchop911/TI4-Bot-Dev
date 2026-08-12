//! Deterministic timing-ability registration.
//!
//! This module owns only the data and ordered registry needed to open a timing window. Event
//! execution, player priority, nested emission, and frequency consumption deliberately remain in
//! their later M03 packages.

use std::{collections::BTreeMap, sync::Arc};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use ti4_model::id::PlayerId;

use crate::event::Event;

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

/// The timing resolver's registration state.
///
/// M03-010 adds event emission and timing-window execution to this concrete type. Declaring the
/// callback target here means abilities have a stable, typed resolver API from the first package
/// instead of an opaque context that rules could not use to emit nested events.
#[derive(Default, Clone)]
pub struct Resolver {
    registry: AbilityRegistry,
}

impl Resolver {
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
    use super::*;

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
}
