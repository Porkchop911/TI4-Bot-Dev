//! Concrete one-way signals: what a seat tells another, and whether it held.
//!
//! A signal is a typed statement ("stay out of system 18 through round 3"), never a bare word. It
//! is public, needs no answer, and is judged from what the two seats then do: at attacks, at system
//! activations, and when it expires.

use ti4_model::{
    DiplomacyError, DiplomacyEvent, DiplomacyJournalEntry, GameState, PlayerId, Signal, SignalId,
    SignalKind, SignalStatement, SignalStatus,
};

use super::promises::DiplomacyEventContext;
use super::relations::{RelationshipEvent, apply_relationship_event};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignalDraft {
    pub speaker: PlayerId,
    pub target: PlayerId,
    pub statement: SignalStatement,
    pub expires_round: u32,
}

/// Emit one public structured signal and consume the pair allowance.
///
/// # Errors
/// Returns [`SignalError`] for invalid parties, expiry, or an exhausted allowance.
pub fn emit_signal(state: &mut GameState, draft: SignalDraft) -> Result<SignalId, SignalError> {
    emit_signal_inner(state, draft, true)
}

/// Emit after a contact window has already consumed the pair allowance.
pub(crate) fn emit_signal_in_open_contact(
    state: &mut GameState,
    draft: SignalDraft,
) -> Result<SignalId, SignalError> {
    emit_signal_inner(state, draft, false)
}

fn emit_signal_inner(
    state: &mut GameState,
    draft: SignalDraft,
    consume_allowance: bool,
) -> Result<SignalId, SignalError> {
    if !state.diplomacy.enabled {
        return Err(SignalError::Disabled);
    }
    if draft.speaker == draft.target {
        return Err(SignalError::SelfSignal);
    }
    if state.player(&draft.speaker).is_none() || state.player(&draft.target).is_none() {
        return Err(SignalError::PlayerMissing);
    }
    if draft.expires_round < state.round || draft.expires_round > state.round.saturating_add(1) {
        return Err(SignalError::InvalidExpiry);
    }
    if consume_allowance
        && !state
            .diplomacy
            .consume_initiation(&draft.speaker, &draft.target)
    {
        return Err(SignalError::AlreadyInitiated);
    }
    let id = SignalId(state.diplomacy.next_signal_id);
    state.diplomacy.next_signal_id = state
        .diplomacy
        .next_signal_id
        .checked_add(1)
        .ok_or(SignalError::IdExhausted)?;
    let kind = draft.statement.kind();
    state.diplomacy.recent_signals.push(Signal {
        id,
        speaker: draft.speaker.clone(),
        target: draft.target.clone(),
        kind,
        statement: draft.statement,
        status: SignalStatus::Open,
        created_round: state.round,
        expires_round: draft.expires_round,
    });
    state.diplomacy.journal.push(DiplomacyJournalEntry {
        round: state.round,
        event: DiplomacyEvent::SignalEmitted { signal_id: id },
    });
    apply_relationship_event(
        state,
        &RelationshipEvent::SignalMade {
            speaker: draft.speaker,
            target: draft.target,
            kind,
        },
    )?;
    Ok(id)
}

/// Judge every undecided signal against one gameplay event.
///
/// # Errors
/// Returns [`DiplomacyError`] only if a resulting relationship change cannot be applied.
pub(crate) fn judge_event(
    state: &mut GameState,
    event: &DiplomacyEventContext,
) -> Result<(), DiplomacyError> {
    let round = state.round;
    let mut outcomes = Vec::new();
    for (index, signal) in state.diplomacy.recent_signals.iter().enumerate() {
        if round > signal.expires_round {
            continue;
        }
        let (speaker, target) = (&signal.speaker, &signal.target);
        let next = match (&signal.statement, signal.status, event) {
            (
                SignalStatement::WillVote { agenda, outcome },
                SignalStatus::Open,
                DiplomacyEventContext::VotesRecorded { agenda: actual },
            ) if agenda == actual => Some(
                if state
                    .agenda_votes
                    .get(speaker)
                    .is_some_and(|vote| vote == outcome)
                {
                    SignalStatus::Honoured
                } else {
                    SignalStatus::Broken
                },
            ),
            (
                SignalStatement::AttackIfYouActivate { .. },
                SignalStatus::Triggered,
                DiplomacyEventContext::HostileEngagement {
                    attacker, victim, ..
                },
            ) if attacker == speaker && victim == target => Some(SignalStatus::CarriedOut),
            (
                SignalStatement::AttackIfYouActivate { system },
                SignalStatus::Open,
                DiplomacyEventContext::SystemActivated {
                    player,
                    system: activated,
                },
            ) if player == target && activated == system => Some(SignalStatus::Triggered),
            _ => None,
        };
        if let Some(status) = next {
            outcomes.push((index, status));
        }
    }
    for (index, status) in outcomes {
        judge(state, index, status)?;
    }
    Ok(())
}

/// Settle every undecided signal that expires with the completed round.
///
/// A warning nobody triggered was heeded; a triggered warning the speaker never acted on was a
/// bluff; a vote assurance the ballot never bore out was broken.
///
/// # Errors
/// Returns [`DiplomacyError`] only if a resulting relationship change cannot be applied.
pub(crate) fn settle_deadlines(
    state: &mut GameState,
    completed_round: u32,
) -> Result<(), DiplomacyError> {
    let due: Vec<(usize, SignalStatus)> = state
        .diplomacy
        .recent_signals
        .iter()
        .enumerate()
        .filter(|(_, signal)| {
            signal.expires_round <= completed_round && !signal.status.is_settled()
        })
        .map(|(index, signal)| {
            let status = match (&signal.statement, signal.status) {
                // A vote assurance whose ballot never named that outcome was not kept.
                (SignalStatement::WillVote { .. }, _) => SignalStatus::Broken,
                (_, SignalStatus::Triggered) => SignalStatus::Bluffed,
                _ => SignalStatus::Heeded,
            };
            (index, status)
        })
        .collect();
    for (index, status) in due {
        judge(state, index, status)?;
    }
    Ok(())
}

fn judge(state: &mut GameState, index: usize, status: SignalStatus) -> Result<(), DiplomacyError> {
    let signal = {
        let signal = &mut state.diplomacy.recent_signals[index];
        signal.status = status;
        signal.clone()
    };
    state.diplomacy.journal.push(DiplomacyJournalEntry {
        round: state.round,
        event: DiplomacyEvent::SignalJudged {
            signal: signal.clone(),
        },
    });
    let (speaker, target) = (signal.speaker, signal.target);
    let relationship = match status {
        SignalStatus::Honoured => Some(RelationshipEvent::AssuranceKept { speaker, target }),
        SignalStatus::Broken => Some(RelationshipEvent::AssuranceBroken { speaker, target }),
        SignalStatus::Heeded if signal.kind == SignalKind::Request => {
            Some(RelationshipEvent::RequestHeeded { speaker, target })
        }
        SignalStatus::Ignored => Some(RelationshipEvent::RequestIgnored { speaker, target }),
        SignalStatus::CarriedOut => Some(RelationshipEvent::ThreatCarriedOut { speaker, target }),
        SignalStatus::Bluffed => Some(RelationshipEvent::ThreatBluffed { speaker, target }),
        SignalStatus::Open | SignalStatus::Triggered | SignalStatus::Heeded => None,
    };
    if let Some(event) = relationship {
        apply_relationship_event(state, &event)?;
    }
    Ok(())
}

#[derive(Debug, thiserror::Error)]
pub enum SignalError {
    #[error("diplomacy is disabled")]
    Disabled,
    #[error("a player cannot signal themselves")]
    SelfSignal,
    #[error("signal participant is not seated")]
    PlayerMissing,
    #[error("signal expiry must be this round or the next")]
    InvalidExpiry,
    #[error("this pair has already initiated contact this turn")]
    AlreadyInitiated,
    #[error("signal identifier space is exhausted")]
    IdExhausted,
    #[error(transparent)]
    Diplomacy(#[from] ti4_model::DiplomacyError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use ti4_model::{DiplomacyState, StrategyCardId, SystemId};

    fn pid(s: &str) -> PlayerId {
        PlayerId::new(s)
    }

    fn table() -> GameState {
        let mut state = GameState::new(
            &[pid("a"), pid("b")],
            &[] as &[StrategyCardId],
            BTreeMap::new(),
            None,
            0,
        );
        state.diplomacy = DiplomacyState::for_players(&state.seating_order, true);
        state
    }

    fn draft(statement: SignalStatement) -> SignalDraft {
        SignalDraft {
            speaker: pid("a"),
            target: pid("b"),
            statement,
            expires_round: 1,
        }
    }

    #[test]
    fn a_warning_never_triggered_is_heeded_and_a_triggered_one_left_alone_is_a_bluff() {
        let mut state = table();
        let system = SystemId::new("18");
        emit_signal(
            &mut state,
            draft(SignalStatement::AttackIfYouActivate {
                system: system.clone(),
            }),
        )
        .unwrap();
        emit_signal_in_open_contact(
            &mut state,
            SignalDraft {
                speaker: pid("b"),
                target: pid("a"),
                ..draft(SignalStatement::AttackIfYouActivate {
                    system: system.clone(),
                })
            },
        )
        .unwrap();
        judge_event(
            &mut state,
            &DiplomacyEventContext::SystemActivated {
                player: pid("a"),
                system,
            },
        )
        .unwrap();
        settle_deadlines(&mut state, 1).unwrap();
        assert_eq!(
            state.diplomacy.recent_signals[0].status,
            SignalStatus::Heeded
        );
        assert_eq!(
            state.diplomacy.recent_signals[1].status,
            SignalStatus::Bluffed
        );
    }
}
