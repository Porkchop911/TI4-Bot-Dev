//! The single relationship-delta table for diplomacy rules v1.

use ti4_model::{DiplomacyError, GameState, PlayerId};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RelationshipEvent {
    FairTransaction {
        a: PlayerId,
        b: PlayerId,
    },
    DealAccepted {
        a: PlayerId,
        b: PlayerId,
    },
    PromiseFulfilled {
        promiser: PlayerId,
        beneficiary: PlayerId,
    },
    PromiseBroken {
        promiser: PlayerId,
        beneficiary: PlayerId,
    },
    DirectAttack {
        attacker: PlayerId,
        victim: PlayerId,
        activation_seq: u32,
    },
    PublicThreat {
        speaker: PlayerId,
        target: PlayerId,
    },
    PublicWarning {
        speaker: PlayerId,
        target: PlayerId,
    },
    AntiThirdPartyDeal {
        payer: PlayerId,
        attacker: PlayerId,
        target: PlayerId,
    },
}

/// Apply the fixed v1 delta for one semantic event.
///
/// # Errors
/// Returns [`DiplomacyError`] only if a validated directional pair cannot be mutated.
pub fn apply_relationship_event(
    state: &mut GameState,
    event: &RelationshipEvent,
) -> Result<(), DiplomacyError> {
    if !state.diplomacy.enabled || !participants_seated(state, event) {
        return Ok(());
    }
    match event {
        RelationshipEvent::FairTransaction { a, b } => {
            adjust(state, a, b, 3, 0, 3, 0)?;
            adjust(state, b, a, 3, 0, 3, 0)
        }
        RelationshipEvent::DealAccepted { a, b } => {
            adjust(state, a, b, 5, 0, 5, 0)?;
            adjust(state, b, a, 5, 0, 5, 0)
        }
        RelationshipEvent::PromiseFulfilled {
            promiser,
            beneficiary,
        } => adjust(state, beneficiary, promiser, 20, 0, 15, 0),
        RelationshipEvent::PromiseBroken {
            promiser,
            beneficiary,
        } => {
            adjust(state, beneficiary, promiser, -40, 15, 0, 25)?;
            state
                .diplomacy
                .last_breaches
                .entry(beneficiary.clone())
                .or_default()
                .insert(promiser.clone(), state.round);
            Ok(())
        }
        RelationshipEvent::DirectAttack {
            attacker,
            victim,
            activation_seq,
        } => {
            if !state.diplomacy.judged_attacks.insert((
                *activation_seq,
                attacker.clone(),
                victim.clone(),
            )) {
                return Ok(());
            }
            adjust(state, victim, attacker, -10, 15, 0, 20)?;
            state
                .diplomacy
                .last_attacks
                .entry(victim.clone())
                .or_default()
                .insert(attacker.clone(), state.round);
            Ok(())
        }
        RelationshipEvent::PublicThreat { speaker, target } => {
            adjust(state, target, speaker, 0, 10, 0, 5)
        }
        RelationshipEvent::PublicWarning { speaker, target } => {
            adjust(state, target, speaker, 0, 4, 0, 2)
        }
        RelationshipEvent::AntiThirdPartyDeal {
            payer,
            attacker,
            target,
        } => {
            adjust(state, target, payer, -3, 0, 0, 3)?;
            adjust(state, target, attacker, -3, 0, 0, 3)
        }
    }
}

/// Whether every player the event names holds a row in the relationship matrix.
///
/// Neutral units belong to an owner that is never seated, and fighting them is an engagement
/// like any other. Nobody has a relationship with that owner, so such an event changes nothing
/// rather than growing the matrix a row the state validator refuses.
fn participants_seated(state: &GameState, event: &RelationshipEvent) -> bool {
    let named: Vec<&PlayerId> = match event {
        RelationshipEvent::FairTransaction { a, b } | RelationshipEvent::DealAccepted { a, b } => {
            vec![a, b]
        }
        RelationshipEvent::PromiseFulfilled {
            promiser,
            beneficiary,
        }
        | RelationshipEvent::PromiseBroken {
            promiser,
            beneficiary,
        } => vec![promiser, beneficiary],
        RelationshipEvent::DirectAttack {
            attacker, victim, ..
        } => vec![attacker, victim],
        RelationshipEvent::PublicThreat { speaker, target }
        | RelationshipEvent::PublicWarning { speaker, target } => vec![speaker, target],
        RelationshipEvent::AntiThirdPartyDeal {
            payer,
            attacker,
            target,
        } => vec![payer, attacker, target],
    };
    named
        .into_iter()
        .all(|player| state.diplomacy.relationships.contains_key(player))
}

#[allow(clippy::too_many_arguments)]
fn adjust(
    state: &mut GameState,
    observer: &PlayerId,
    subject: &PlayerId,
    trust: i16,
    threat: i16,
    cooperation: i16,
    hostility: i16,
) -> Result<(), DiplomacyError> {
    let relation = state.diplomacy.relationship_mut(observer, subject)?;
    *relation = relation.adjusted(trust, threat, cooperation, hostility);
    Ok(())
}

pub fn decay_relationships(state: &mut GameState) {
    if !state.diplomacy.enabled {
        return;
    }
    for row in state.diplomacy.relationships.values_mut() {
        for relationship in row.values_mut() {
            relationship.decay();
        }
    }
    state.diplomacy.prune_signals(state.round.saturating_add(1));
}

#[must_use]
pub fn recent_attack(state: &GameState, observer: &PlayerId, subject: &PlayerId) -> bool {
    recent(
        &state.diplomacy.last_attacks,
        observer,
        subject,
        state.round,
    )
}

#[must_use]
pub fn recent_breach(state: &GameState, observer: &PlayerId, subject: &PlayerId) -> bool {
    recent(
        &state.diplomacy.last_breaches,
        observer,
        subject,
        state.round,
    )
}

fn recent(
    rounds: &std::collections::BTreeMap<PlayerId, std::collections::BTreeMap<PlayerId, u32>>,
    observer: &PlayerId,
    subject: &PlayerId,
    round: u32,
) -> bool {
    rounds
        .get(observer)
        .and_then(|row| row.get(subject))
        .is_some_and(|event_round| round <= event_round.saturating_add(1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use ti4_model::StrategyCardId;

    fn pid(s: &str) -> PlayerId {
        PlayerId::new(s)
    }
    fn game() -> GameState {
        let mut game = GameState::new(
            &[pid("a"), pid("b")],
            &[] as &[StrategyCardId],
            BTreeMap::new(),
            None,
            0,
        );
        game.diplomacy = ti4_model::DiplomacyState::for_players(&game.seating_order, true);
        game
    }

    #[test]
    fn deltas_are_directional_and_attacks_deduplicate() {
        let mut state = game();
        apply_relationship_event(
            &mut state,
            &RelationshipEvent::DirectAttack {
                attacker: pid("a"),
                victim: pid("b"),
                activation_seq: 7,
            },
        )
        .unwrap();
        apply_relationship_event(
            &mut state,
            &RelationshipEvent::DirectAttack {
                attacker: pid("a"),
                victim: pid("b"),
                activation_seq: 7,
            },
        )
        .unwrap();
        assert_eq!(
            state.diplomacy.relationship(&pid("b"), &pid("a")),
            ti4_model::Relationship {
                trust: -10,
                threat: 15,
                cooperation: 0,
                hostility: 20
            }
        );
        assert_eq!(
            state.diplomacy.relationship(&pid("a"), &pid("b")),
            ti4_model::Relationship::default()
        );
    }

    #[test]
    fn flags_cover_event_round_and_following_round() {
        let mut state = game();
        apply_relationship_event(
            &mut state,
            &RelationshipEvent::DirectAttack {
                attacker: pid("a"),
                victim: pid("b"),
                activation_seq: 1,
            },
        )
        .unwrap();
        assert!(recent_attack(&state, &pid("b"), &pid("a")));
        state.round += 1;
        assert!(recent_attack(&state, &pid("b"), &pid("a")));
        state.round += 1;
        assert!(!recent_attack(&state, &pid("b"), &pid("a")));
    }

    #[test]
    fn an_engagement_with_an_unseated_owner_changes_no_relationship() {
        let mut state = game();
        apply_relationship_event(
            &mut state,
            &RelationshipEvent::DirectAttack {
                attacker: pid("a"),
                victim: pid("neutral"),
                activation_seq: 3,
            },
        )
        .unwrap();

        assert!(!state.diplomacy.relationships.contains_key(&pid("neutral")));
        assert!(state.diplomacy.judged_attacks.is_empty());
        assert!(
            state
                .diplomacy
                .validate(&state.seating_order, state.round)
                .is_ok()
        );
    }
}
