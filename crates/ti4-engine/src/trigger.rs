//! Builds the public [`DecisionTrigger`] a reaction decision carries (display only).
//!
//! One pure function over the [`Event`] already in hand where the decision is raised: no state
//! lookups, so it recomputes identically on replay and cannot leak anything the event payload does
//! not already say publicly.

use ti4_model::id::{PlanetId, PlayerId, SystemId};

use crate::decision_context::{DecisionTrigger, TriggerKind, TriggerUnits};
use crate::event::Event;

/// The kind of trigger an event type belongs to, one `match` so a new window is one line.
#[must_use]
pub fn kind_of(event_type: &str) -> TriggerKind {
    match event_type {
        "ACTION_CARD_PLAYED" => TriggerKind::ActionCardPlayed,
        "ACTION_CARD_DISCARDED" => TriggerKind::ActionCardDiscarded,
        "SYSTEM_ACTIVATED" => TriggerKind::SystemActivated,
        "SHIP_MOVED" => TriggerKind::ShipMoved,
        "STRATEGIC_ACTION_BEGAN" => TriggerKind::StrategicActionBegan,
        "STRATEGY_CARD_CHOSEN" => TriggerKind::StrategyCardChosen,
        "STRATEGY_PHASE_BEGAN" => TriggerKind::StrategyPhaseBegan,
        "TURN_BEGAN" => TriggerKind::TurnBegan,
        "TURN_PASSED" => TriggerKind::TurnPassed,
        "PLAYER_PASSED" => TriggerKind::PlayerPassed,
        "ACTION_COMPLETED" => TriggerKind::ActionCompleted,
        "STRATEGY_CARDS_WOULD_RETURN" => TriggerKind::StrategyCardsWouldReturn,
        "AGENDA_PHASE_BEGAN" => TriggerKind::AgendaPhaseBegan,
        "AGENDA_REVEALED" => TriggerKind::AgendaRevealed,
        "VOTES_CAST" => TriggerKind::VotesCast,
        "AGENDA_RESOLVED" => TriggerKind::AgendaResolved,
        "TRANSACTION_OPENED"
        | "TRANSACTION_RESOLVED"
        | "TRANSACTION_REFUSED"
        | "TRANSACTION_OFFERED"
        | "TRANSACTION_REJECTED"
        | "TRANSACTION_ABANDONED" => TriggerKind::Transaction,
        "PLANET_CONTROL_GAINED" => TriggerKind::PlanetControlGained,
        "INVASION_BEGAN" => TriggerKind::InvasionBegan,
        "UNITS_COMMITTED" => TriggerKind::UnitsCommitted,
        "GROUND_ROLLS_MADE" => TriggerKind::GroundRolls,
        "SPACE_COMBAT_STARTED" | "COMBAT_ROUND_STARTED" => TriggerKind::CombatStarted,
        "ANTI_FIGHTER_BARRAGE_STARTED" => TriggerKind::AntiFighterBarrage,
        "SPACE_CANNON_HITS" => TriggerKind::SpaceCannonHits,
        "HITS_TO_ASSIGN" => TriggerKind::HitsToAssign,
        "SUSTAIN_DAMAGE_USED" => TriggerKind::SustainDamage,
        "SHIP_DESTROYED" => TriggerKind::ShipDestroyed,
        "RETREAT_STEP_STARTED" | "RETREAT_DECLARED" => TriggerKind::Retreat,
        "SPACE_COMBAT_WON" => TriggerKind::SpaceCombatWon,
        "PRODUCTION_USED" => TriggerKind::ProductionUsed,
        "UNIT_ABILITY_ROLLED" => TriggerKind::UnitAbilityRolled,
        _ => TriggerKind::Other,
    }
}

fn seat(event: &Event, key: &str) -> Option<PlayerId> {
    event
        .text(key)
        .filter(|value| !value.is_empty())
        .map(PlayerId::new)
}

impl DecisionTrigger {
    /// The trigger for a reaction window opened by `event`.
    ///
    /// `relation` is `"when"` or `"after"`; `chain` is the emission stack around the event (ids,
    /// outermost first, the event itself excluded).
    #[must_use]
    pub fn from_event(event: &Event, relation: &str, chain: &[u64]) -> Self {
        let kind = kind_of(&event.event_type);
        // Whose act it was. `player` names the actor for most events, but it names the *victim* of
        // hits and shots, the *outcome* of an agenda and nobody for phase and ground-roll events.
        let (actor, subject) = match kind {
            TriggerKind::SpaceCannonHits => (seat(event, "gunner"), seat(event, "player")),
            TriggerKind::HitsToAssign | TriggerKind::ShipDestroyed => (None, seat(event, "player")),
            TriggerKind::AgendaResolved => (None, seat(event, "elected_player")),
            TriggerKind::AgendaRevealed
            | TriggerKind::AgendaPhaseBegan
            | TriggerKind::StrategyPhaseBegan
            | TriggerKind::GroundRolls => (None, None),
            TriggerKind::Transaction => (seat(event, "player"), seat(event, "partner")),
            TriggerKind::CombatStarted => (
                seat(event, "attacker").or_else(|| seat(event, "player")),
                seat(event, "defender"),
            ),
            _ => (seat(event, "player"), None),
        };
        let card = event.text("card").map(str::to_owned);
        let units = event
            .payload
            .get("units")
            .and_then(serde_json::Value::as_array)
            .map(|entries| {
                entries
                    .iter()
                    .filter_map(|entry| {
                        Some(TriggerUnits {
                            owner: PlayerId::new(entry.get("owner")?.as_str()?),
                            unit_type: entry.get("unit_type")?.as_str()?.to_owned(),
                            count: u32::try_from(entry.get("count")?.as_u64()?).ok()?,
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        Self {
            kind,
            event_type: event.event_type.clone(),
            event_id: event.id,
            relation: relation.to_owned(),
            actor,
            subject,
            card: if matches!(
                kind,
                TriggerKind::ActionCardPlayed
                    | TriggerKind::ActionCardDiscarded
                    | TriggerKind::StrategicActionBegan
                    | TriggerKind::StrategyCardChosen
            ) {
                card
            } else {
                None
            },
            agenda: if kind == TriggerKind::AgendaRevealed || kind == TriggerKind::AgendaResolved {
                event.text("agenda").map(str::to_owned)
            } else {
                None
            },
            system: event.text("system").map(SystemId::new),
            planet: event.text("planet").map(PlanetId::new),
            units,
            hits: event
                .integer("hits")
                .and_then(|hits| u32::try_from(hits).ok()),
            chain: chain.to_vec(),
        }
    }
}
