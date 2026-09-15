//! Bounded one-way structured signals.

use ti4_model::{
    DiplomacyEvent, DiplomacyJournalEntry, GameState, PlayerId, Signal, SignalCondition, SignalId,
    SignalKind, SignalSubject,
};

use super::relations::{RelationshipEvent, apply_relationship_event};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignalDraft {
    pub speaker: PlayerId,
    pub target: PlayerId,
    pub kind: SignalKind,
    pub subject: SignalSubject,
    pub condition: Option<SignalCondition>,
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
    let signal = Signal {
        id,
        speaker: draft.speaker.clone(),
        target: draft.target.clone(),
        kind: draft.kind,
        subject: draft.subject,
        condition: draft.condition,
        created_round: state.round,
        expires_round: draft.expires_round,
    };
    state.diplomacy.recent_signals.push(signal);
    state.diplomacy.journal.push(DiplomacyJournalEntry {
        round: state.round,
        event: DiplomacyEvent::SignalEmitted { signal_id: id },
    });
    let relationship = match draft.kind {
        SignalKind::Threat => Some(RelationshipEvent::PublicThreat {
            speaker: draft.speaker,
            target: draft.target,
        }),
        SignalKind::Warning => Some(RelationshipEvent::PublicWarning {
            speaker: draft.speaker,
            target: draft.target,
        }),
        SignalKind::Request | SignalKind::Assurance => None,
    };
    if let Some(event) = relationship {
        apply_relationship_event(state, &event)?;
    }
    Ok(id)
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
    use ti4_model::{DiplomacyState, StrategyCardId};

    fn pid(s: &str) -> PlayerId {
        PlayerId::new(s)
    }

    #[test]
    fn threats_apply_once_and_close_the_pair_allowance() {
        let mut state = GameState::new(
            &[pid("a"), pid("b")],
            &[] as &[StrategyCardId],
            BTreeMap::new(),
            None,
            0,
        );
        state.diplomacy = DiplomacyState::for_players(&state.seating_order, true);
        let draft = SignalDraft {
            speaker: pid("a"),
            target: pid("b"),
            kind: SignalKind::Threat,
            subject: SignalSubject::Player(pid("b")),
            condition: None,
            expires_round: 1,
        };
        assert_eq!(emit_signal(&mut state, draft.clone()).unwrap(), SignalId(1));
        assert!(matches!(
            emit_signal(&mut state, draft),
            Err(SignalError::AlreadyInitiated)
        ));
        assert_eq!(
            state.diplomacy.relationship(&pid("b"), &pid("a")).hostility,
            5
        );
    }
}
