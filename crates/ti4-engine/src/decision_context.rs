//! Why the engine is asking this question (OBS-003a).
//!
//! A [`Choice`](crate::choice::Choice) today carries a player, a prompt string and a list of
//! options. Nothing in it says which rule raised the question, what kind of question it is, or what
//! is still owed from a decision already in progress. The learned router therefore recovers the
//! semantics from free prompt text, and `other` becomes a catch-all holding scoring, agenda riders,
//! exploration, transit, faction abilities and card effects together.
//!
//! The rule-dependency matrix measured what that costs: fourteen producers read law, agenda and
//! custodians state that reaches no feature at all, and the prompt — the one channel that currently
//! separates these questions — reaches the option-invariant part of the observation in only 1,753
//! of 3,678 decisions.
//!
//! This module defines the type. It does not populate producers, change any legal option set, or
//! touch replay: those are OBS-003b through OBS-003h.
//!
//! # Two things it carries that a prompt cannot
//!
//! **A stable subtype.** [`DecisionContext::subtype`] is a machine identifier chosen by the
//! producer, not a sentence. Rewording a prompt must never change how a decision is classified, and
//! today it can.
//!
//! **What is still outstanding.** Several decisions are one transaction spread over several
//! prompts, and a snapshot of the board between them does not say how much has already been paid or
//! produced. A seat that exhausted a four-influence planet toward a three-influence command token
//! holds one influence of credit that exists nowhere in the position; asking again without it
//! charged seven influence for two tokens. [`OutstandingConstraint`] is where that lives.
//!
//! # Visibility
//!
//! Every field here describes the question being put to one seat, so the whole record is visible to
//! that seat by construction. It is still redacted rather than assumed: [`DecisionContext::visible_to`]
//! answers what a *different* seat may see, because reviews, replays and any future observer of the
//! table read the same record.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use ti4_model::id::{PlanetId, PlayerId, SystemId, UnitTypeId};
use ti4_model::state::Phase;

/// The schema version of [`DecisionContext`].
///
/// Bumped when a field is added, removed, or changes meaning. Readers that do not recognise a
/// version must refuse it rather than guess: a context misread as an older shape would silently
/// mis-describe why a decision was asked, and every consumer downstream — router, features, replay
/// hash — would inherit that quietly.
pub const CONTEXT_VERSION: u16 = 1;

fn is_false(value: &bool) -> bool {
    !value
}

/// What raised this decision.
///
/// Named rather than free text so a producer's identity survives rewording. `Rule` carries an LRR
/// reference where one exists, because "the engine asked" is not an explanation anyone can check.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum DecisionSource {
    /// A core rule, by its LRR number where one applies (`"37.3"`, `"52.3"`).
    Rule(String),
    /// A strategy card's primary or secondary.
    StrategyCard { card: String, secondary: bool },
    /// An action card, by content alias.
    ActionCard(String),
    /// A faction ability, by content alias.
    FactionAbility(String),
    /// A leader, relic, breakthrough or other content effect, by alias.
    Content(String),
    /// An agenda or an enacted law.
    Agenda(String),
    /// A reaction window opened by a prior event.
    Reaction(String),
}

/// What the decision is about, when it points at something on the board.
///
/// Distinct from the option payloads: this is the subject of the *question*, shared by every
/// option, where an option payload describes one answer. "Which unit takes this hit" has a system
/// as its target and units as its options.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum DecisionTarget {
    System(SystemId),
    Planet { system: SystemId, planet: PlanetId },
    Unit { system: SystemId, unit: UnitTypeId },
    Player(PlayerId),
}

/// A quantity that is still owed or still available inside a decision already under way.
///
/// This is the continuation state a between-prompts board snapshot loses. `paid` is what the seat
/// has already committed within this same transaction, so `remaining` is what a further offer may
/// legitimately ask for; charging `amount` again is the defect this type exists to prevent.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct OutstandingConstraint {
    pub kind: ConstraintKind,
    /// The full bill or allowance for the transaction.
    pub amount: i64,
    /// Already committed within this transaction, including overpayment retained as credit.
    pub paid: i64,
}

impl OutstandingConstraint {
    #[must_use]
    pub const fn new(kind: ConstraintKind, amount: i64, paid: i64) -> Self {
        Self { kind, amount, paid }
    }

    /// What may still be asked for. Never negative: an overpayment is credit, not a debt owed back.
    #[must_use]
    pub const fn remaining(&self) -> i64 {
        let left = self.amount - self.paid;
        if left < 0 { 0 } else { left }
    }

    /// Whether the transaction is settled.
    #[must_use]
    pub const fn settled(&self) -> bool {
        self.remaining() == 0
    }
}

/// The kind of quantity an [`OutstandingConstraint`] tracks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ConstraintKind {
    Resources,
    Influence,
    TradeGoods,
    CommandTokens,
    /// One use of PRODUCTION, which does not reset between purchases (LRR 68).
    ProductionCapacity,
    /// Transport capacity for fighters and ground forces (LRR 16).
    TransportCapacity,
    /// Non-fighter ships permitted in one system (LRR 37).
    FleetSupply,
    /// Units the owner must still remove to become legal.
    UnitsToRemove,
    /// Votes still available to cast.
    Votes,
}

/// Why the engine is asking, in a form that does not depend on prompt wording.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecisionContext {
    pub version: u16,
    /// The seat being asked.
    pub actor: PlayerId,
    pub source: DecisionSource,
    /// A stable machine identifier for the question, e.g. `"score_public_objective"`. Never a
    /// sentence, and never derived from the prompt.
    pub subtype: String,
    pub phase: Phase,
    pub round: u32,
    /// Whether the seat may decline. Declining an obligation is not the same as choosing between
    /// obligations, and a policy cannot tell those apart from an option list alone.
    pub optional: bool,
    pub target: Option<DecisionTarget>,
    /// This decision belongs to the currently running space battle, not a separate cannon or invasion.
    #[serde(default, skip_serializing_if = "is_false")]
    pub space_battle: bool,
    /// Activation sequence of the invasion that owns this choice, including nested reactions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub invasion_seq: Option<u64>,
    /// Quantities still owed or available within a decision already under way.
    pub outstanding: Vec<OutstandingConstraint>,
    /// What happened to open this reaction window, for display only (public facts).
    ///
    /// Deliberately not part of [`DecisionContext::canonical`] or the replay fingerprint: it is
    /// derived deterministically from the event, so it adds no information a replay needs, and
    /// it is stripped from recorded decisions (`DecisionLog::record`). Additive and optional, so
    /// old saves and old clients are unaffected.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trigger: Option<DecisionTrigger>,
    /// What kind of hit a casualty decision assigns, for display only (public facts).
    ///
    /// Like [`DecisionContext::trigger`], deliberately not part of [`DecisionContext::canonical`]
    /// or the replay fingerprint, stripped from recorded decisions, additive and optional: it
    /// restates rules the engine already enforces, so a client need not guess them from labels.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hit: Option<HitDetail>,
}

/// What produced the hits a casualty decision assigns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HitCause {
    /// Assault Cannon (technology): the opponent destroys one of their non-fighter ships.
    AssaultCannon,
    /// Courageous to the End (action card): hits from its two dice.
    CourageousToTheEnd,
    /// Anti-fighter barrage hits widened to every ship by Waylay.
    AntiFighterBarrage,
    /// Space cannon (offense or defense) hits.
    SpaceCannon,
    /// The ordinary dice hits of a combat round.
    CombatRoll,
    /// Hits a card or module produced at the start of a combat.
    StartOfCombat,
    /// Hits a card or module produced after a combat round.
    EndOfRound,
    /// Anything else (an effect's hit).
    Other,
}

/// Which of the target's ships a hit may be assigned to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HitRestriction {
    Any,
    /// Non-fighter ships only (Assault Cannon; Graviton Laser System while one is left).
    NonFighter,
}

/// The kind of hit a casualty decision assigns (see [`DecisionContext::hit`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HitDetail {
    pub cause: HitCause,
    /// `true` for a destroy effect: not a hit, so SUSTAIN DAMAGE does not apply to it.
    pub destroy: bool,
    pub restriction: HitRestriction,
    /// The seat whose ability or roll produced it, when public.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub producer: Option<PlayerId>,
}

/// The family of event a reaction decision answers, in the words a client switches on.
///
/// `Other` plus [`DecisionTrigger::event_type`] is the open-ended fallback, so a new window never
/// breaks a client.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TriggerKind {
    ActionCardPlayed,
    ActionCardDiscarded,
    SystemActivated,
    ShipMoved,
    StrategicActionBegan,
    StrategyCardChosen,
    StrategyPhaseBegan,
    TurnBegan,
    TurnPassed,
    PlayerPassed,
    ActionCompleted,
    StrategyCardsWouldReturn,
    AgendaPhaseBegan,
    AgendaRevealed,
    VotesCast,
    AgendaResolved,
    Transaction,
    PlanetControlGained,
    InvasionBegan,
    UnitsCommitted,
    GroundRolls,
    CombatStarted,
    AntiFighterBarrage,
    SpaceCannonHits,
    HitsToAssign,
    SustainDamage,
    ShipDestroyed,
    Retreat,
    SpaceCombatWon,
    ProductionUsed,
    UnitAbilityRolled,
    #[serde(other)]
    Other,
}

/// A public unit count named by a trigger: owner, unit type and how many.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TriggerUnits {
    pub owner: PlayerId,
    pub unit_type: String,
    pub count: u32,
}

/// The public event that opened a reaction window (see [`DecisionContext::trigger`]).
///
/// Everything named here has already happened in public: a played action card, an activated
/// system, a moved fleet. It never carries a vote outcome, a hand, or any unplayed alternative.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecisionTrigger {
    pub kind: TriggerKind,
    /// The raw engine event type, for fallback and debugging.
    pub event_type: String,
    /// The engine event id, valid within one trace only: a session-local key, not a durable id.
    pub event_id: u64,
    /// `"when"` or `"after"`.
    pub relation: String,
    /// The seat that caused the event; absent for phase and agenda events.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actor: Option<PlayerId>,
    /// A second seat: a transaction partner, the victim of hits, an elected player, a defender.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject: Option<PlayerId>,
    /// An action card alias (public once played) or a strategy card id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub card: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agenda: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system: Option<SystemId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub planet: Option<PlanetId>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub units: Vec<TriggerUnits>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hits: Option<u32>,
    /// Ids of the events being resolved around this one, outermost first: non-empty when this is a
    /// reaction to a reaction.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub chain: Vec<u64>,
}

impl DecisionContext {
    /// A context at the current schema version.
    #[must_use]
    pub fn new(
        actor: PlayerId,
        source: DecisionSource,
        subtype: impl Into<String>,
        phase: Phase,
        round: u32,
    ) -> Self {
        Self {
            version: CONTEXT_VERSION,
            actor,
            source,
            subtype: subtype.into(),
            phase,
            round,
            optional: false,
            target: None,
            space_battle: false,
            invasion_seq: None,
            outstanding: Vec::new(),
            trigger: None,
            hit: None,
        }
    }

    /// Say what kind of hit this casualty decision assigns (display only).
    #[must_use]
    pub fn with_hit(mut self, hit: HitDetail) -> Self {
        self.hit = Some(hit);
        self
    }

    /// Attach the public event that opened this reaction window.
    #[must_use]
    pub fn with_trigger(mut self, trigger: DecisionTrigger) -> Self {
        self.trigger = Some(trigger);
        self
    }

    /// The same context without display-only fields, which is what a record and a fingerprint hold.
    #[must_use]
    pub fn without_display_fields(&self) -> Self {
        Self {
            trigger: None,
            hit: None,
            ..self.clone()
        }
    }

    #[must_use]
    pub const fn optional(mut self, optional: bool) -> Self {
        self.optional = optional;
        self
    }

    #[must_use]
    pub fn about(mut self, target: DecisionTarget) -> Self {
        self.target = Some(target);
        self
    }

    #[must_use]
    pub fn about_battle(mut self, state: &ti4_model::state::GameState) -> Self {
        if let Some((system, _, _)) = &state.active_space_combat {
            self.space_battle = true;
            self.target = Some(DecisionTarget::System(system.clone()));
        }
        self
    }

    #[must_use]
    pub fn about_invasion(mut self, state: &ti4_model::state::GameState) -> Self {
        if !self.space_battle {
            if let Some(active) = &state.active_invasion {
                let in_system = match &self.target {
                    Some(
                        DecisionTarget::System(system)
                        | DecisionTarget::Planet { system, .. }
                        | DecisionTarget::Unit { system, .. },
                    ) => system == &active.system,
                    _ => true,
                };
                if in_system {
                    self.invasion_seq = Some(active.seq);
                }
            }
        }
        self
    }

    #[must_use]
    pub fn owing(mut self, constraint: OutstandingConstraint) -> Self {
        self.outstanding.push(constraint);
        self
    }

    /// What `seat` may see of this context.
    ///
    /// The actor sees all of it: it describes the question being put to them. Another seat sees the
    /// public shape — that a decision of this kind was asked, from this source, in this phase and
    /// round — and not the outstanding quantities, which describe how far through a private
    /// transaction the actor is and are not public until spent.
    #[must_use]
    pub fn visible_to(&self, seat: &PlayerId) -> Self {
        if *seat == self.actor {
            return self.clone();
        }
        Self {
            outstanding: Vec::new(),
            ..self.clone()
        }
    }

    /// A canonical rendering, stable across builds and platforms.
    ///
    /// Ordered and explicit rather than derived from a hash map, because this feeds the replay
    /// fingerprint in OBS-003b and a fingerprint that depends on iteration order is not a
    /// fingerprint. Constraints are emitted in a sorted order so that two contexts differing only
    /// in the order the producer happened to push them render identically.
    #[must_use]
    pub fn canonical(&self) -> String {
        let mut constraints: Vec<&OutstandingConstraint> = self.outstanding.iter().collect();
        constraints.sort();
        let owed = constraints
            .iter()
            .map(|c| format!("{:?}:{}/{}", c.kind, c.paid, c.amount))
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "v{}|actor={}|source={:?}|subtype={}|phase={:?}|round={}|optional={}|target={:?}|battle={}|{}owing=[{}]",
            self.version,
            self.actor,
            self.source,
            self.subtype,
            self.phase,
            self.round,
            self.optional,
            self.target,
            self.space_battle,
            self.invasion_seq
                .map_or_else(String::new, |seq| format!("invasion={seq}|")),
            owed
        )
    }

    /// Field visibility, as data rather than prose, so a reviewer can check the contract without
    /// reading `visible_to`.
    #[must_use]
    pub fn visibility() -> BTreeMap<&'static str, Visibility> {
        BTreeMap::from([
            ("version", Visibility::Public),
            ("actor", Visibility::Public),
            ("source", Visibility::Public),
            ("subtype", Visibility::Public),
            ("phase", Visibility::Public),
            ("round", Visibility::Public),
            ("optional", Visibility::Public),
            ("target", Visibility::Public),
            ("space_battle", Visibility::Public),
            ("invasion_seq", Visibility::Public),
            ("outstanding", Visibility::ActorOnly),
            // Public by construction: only facts that already happened at the table.
            ("trigger", Visibility::Public),
            ("hit", Visibility::Public),
        ])
    }
}

/// Who may read a field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visibility {
    /// Every seat may see it.
    Public,
    /// Only the seat being asked.
    ActorOnly,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context() -> DecisionContext {
        DecisionContext::new(
            PlayerId::new("a"),
            DecisionSource::StrategyCard {
                card: "Leadership".to_owned(),
                secondary: true,
            },
            "buy_command_token",
            Phase::Action,
            3,
        )
        .optional(true)
        .owing(OutstandingConstraint::new(ConstraintKind::Influence, 3, 4))
    }

    #[test]
    fn invasion_association_keeps_planet_target_and_rejects_other_systems() {
        let content = ti4_content::ContentStore::embedded();
        let mut state = crate::setup::start_game(
            content,
            &[PlayerId::new("a"), PlayerId::new("b")],
            ti4_model::content_types::POK,
            None,
        )
        .expect("setup");
        state.active_invasion = Some(ti4_model::state::ActiveInvasion {
            system: SystemId::new("18"),
            invader: PlayerId::new("a"),
            seq: 9,
            phase: "landing".to_owned(),
            planet: None,
            defender: None,
            ground_round: 0,
            last_step: None,
        });
        let target = DecisionTarget::Planet {
            system: SystemId::new("18"),
            planet: PlanetId::new("jord"),
        };
        let associated = context().about(target.clone()).about_invasion(&state);
        assert_eq!(associated.invasion_seq, Some(9));
        assert_eq!(associated.target, Some(target));
        assert_eq!(
            associated.visible_to(&PlayerId::new("b")).invasion_seq,
            Some(9)
        );
        assert_eq!(
            context()
                .about(DecisionTarget::System(SystemId::new("19")))
                .about_invasion(&state)
                .invasion_seq,
            None
        );
    }

    #[test]
    fn overpayment_is_credit_and_never_a_debt_owed_back() {
        // A four-influence planet against a three-influence token leaves one of credit. The bug
        // this type exists to prevent asked for three again and charged seven for two tokens.
        let constraint = OutstandingConstraint::new(ConstraintKind::Influence, 3, 4);
        assert_eq!(
            constraint.remaining(),
            0,
            "overpaid is settled, not negative"
        );
        assert!(constraint.settled());

        let partial = OutstandingConstraint::new(ConstraintKind::Influence, 3, 1);
        assert_eq!(
            partial.remaining(),
            2,
            "the credit reduces the next instalment"
        );
        assert!(!partial.settled());
    }

    #[test]
    fn another_seat_does_not_see_what_the_actor_still_owes() {
        let ctx = context();
        let mine = ctx.visible_to(&PlayerId::new("a"));
        let theirs = ctx.visible_to(&PlayerId::new("b"));

        assert_eq!(mine, ctx, "the actor sees the question put to it, entire");
        assert!(
            theirs.outstanding.is_empty(),
            "how far through a private transaction the actor is, is not public"
        );
        assert_eq!(theirs.subtype, ctx.subtype, "the public shape survives");
        assert_eq!(theirs.source, ctx.source);
        assert_eq!(theirs.round, ctx.round);
    }

    #[test]
    fn the_canonical_form_does_not_depend_on_push_order() {
        let one = DecisionContext::new(
            PlayerId::new("a"),
            DecisionSource::Rule("68".to_owned()),
            "produce_unit",
            Phase::Action,
            2,
        )
        .owing(OutstandingConstraint::new(ConstraintKind::Resources, 6, 2))
        .owing(OutstandingConstraint::new(
            ConstraintKind::ProductionCapacity,
            5,
            1,
        ));
        let other = DecisionContext::new(
            PlayerId::new("a"),
            DecisionSource::Rule("68".to_owned()),
            "produce_unit",
            Phase::Action,
            2,
        )
        .owing(OutstandingConstraint::new(
            ConstraintKind::ProductionCapacity,
            5,
            1,
        ))
        .owing(OutstandingConstraint::new(ConstraintKind::Resources, 6, 2));

        assert_eq!(
            one.canonical(),
            other.canonical(),
            "a fingerprint that depends on the order a producer pushed constraints is not one"
        );
    }

    #[test]
    fn the_canonical_form_separates_what_the_prompt_would_not() {
        // The two questions the `other` catch-all conflates today: same seat, same phase, same
        // round, different rule. Nothing about the wording is involved.
        let scoring = DecisionContext::new(
            PlayerId::new("a"),
            DecisionSource::Rule("61.1".to_owned()),
            "score_public_objective",
            Phase::Status,
            3,
        );
        let rider = DecisionContext::new(
            PlayerId::new("a"),
            DecisionSource::ActionCard("imp_rider".to_owned()),
            "agenda_rider_prediction",
            Phase::Agenda,
            3,
        );
        assert_ne!(scoring.canonical(), rider.canonical());
        assert_ne!(scoring.subtype, rider.subtype);
    }

    #[test]
    fn the_schema_version_travels_with_the_record() {
        let ctx = context();
        assert_eq!(ctx.version, CONTEXT_VERSION);
        let json = serde_json::to_string(&ctx).expect("serialize");
        let back: DecisionContext = serde_json::from_str(&json).expect("round trip");
        assert_eq!(back, ctx);
        assert!(
            ctx.canonical().starts_with(&format!("v{CONTEXT_VERSION}|")),
            "the canonical form leads with the version a reader must check"
        );
    }

    #[test]
    fn only_the_outstanding_quantities_are_actor_only() {
        let visibility = DecisionContext::visibility();
        assert_eq!(
            visibility.get("outstanding"),
            Some(&Visibility::ActorOnly),
            "the private half of the contract"
        );
        let public = visibility
            .iter()
            .filter(|(_, v)| **v == Visibility::Public)
            .count();
        assert_eq!(public, 12, "every other field describes a public question");
    }
}
