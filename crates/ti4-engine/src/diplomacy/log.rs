//! Denormalized, authenticated-export-ready diplomacy records.

use std::collections::BTreeMap;

use ti4_model::{
    DealId, DealRelationshipSnapshot, DealResponseKind, DealResponseRecord, DealRevision,
    DealStatus, DiplomacyEvent, DiplomacyJournalEntry, DiplomacyLogRecord, GameState,
};

pub const DIPLOMACY_LOG_SCHEMA_V1: &str = "ti4-diplomacy-log-v1";

#[derive(Debug, thiserror::Error)]
pub enum DiplomacyLogError {
    #[error("game id is empty")]
    EmptyGameId,
    #[error("deal {0:?} is offered more than once")]
    DuplicateOffer(DealId),
    #[error("event references deal {0:?} before its offer")]
    EventBeforeOffer(DealId),
    #[error("deal {0:?} has no status in either live state or terminal history")]
    MissingStatus(DealId),
}

struct Builder {
    created_round: u32,
    proposer: ti4_model::PlayerId,
    recipient: ti4_model::PlayerId,
    relationship_snapshot: DealRelationshipSnapshot,
    original_bundle: DealRevision,
    counters: Vec<DealRevision>,
    responses: Vec<DealResponseRecord>,
    final_status: Option<(DealStatus, u32)>,
    events: Vec<DiplomacyJournalEntry>,
}

/// Denormalize the append-only journal by deal ID. The journal remains checked output and is
/// never consulted by game transitions or replay.
///
/// # Errors
/// Returns [`DiplomacyLogError`] for an empty game ID or an inconsistent journal.
#[expect(
    clippy::too_many_lines,
    reason = "each journal event is folded into one explicit denormalized record"
)]
pub fn export_log_records(
    state: &GameState,
    game_id: &str,
) -> Result<Vec<DiplomacyLogRecord>, DiplomacyLogError> {
    if game_id.trim().is_empty() {
        return Err(DiplomacyLogError::EmptyGameId);
    }
    let mut builders: BTreeMap<DealId, Builder> = BTreeMap::new();
    for entry in &state.diplomacy.journal {
        match &entry.event {
            DiplomacyEvent::Offered {
                deal_id,
                proposer,
                recipient,
                relationship_snapshot,
                revision,
            } => {
                if builders
                    .insert(
                        *deal_id,
                        Builder {
                            created_round: entry.round,
                            proposer: proposer.clone(),
                            recipient: recipient.clone(),
                            relationship_snapshot: *relationship_snapshot,
                            original_bundle: revision.clone(),
                            counters: Vec::new(),
                            responses: Vec::new(),
                            final_status: None,
                            events: vec![entry.clone()],
                        },
                    )
                    .is_some()
                {
                    return Err(DiplomacyLogError::DuplicateOffer(*deal_id));
                }
            }
            DiplomacyEvent::Countered { deal_id, revision } => {
                let builder = builder(&mut builders, *deal_id)?;
                builder.counters.push(revision.clone());
                builder.responses.push(DealResponseRecord {
                    round: entry.round,
                    actor: revision.author.clone(),
                    revision: revision.number,
                    response: DealResponseKind::Counter,
                });
                builder.events.push(entry.clone());
            }
            DiplomacyEvent::Accepted {
                deal_id,
                actor,
                revision,
            } => {
                let builder = builder(&mut builders, *deal_id)?;
                builder.responses.push(DealResponseRecord {
                    round: entry.round,
                    actor: actor.clone(),
                    revision: *revision,
                    response: DealResponseKind::Accept,
                });
                builder.events.push(entry.clone());
            }
            DiplomacyEvent::Declined {
                deal_id,
                actor,
                revision,
            } => {
                let builder = builder(&mut builders, *deal_id)?;
                builder.responses.push(DealResponseRecord {
                    round: entry.round,
                    actor: actor.clone(),
                    revision: *revision,
                    response: DealResponseKind::Decline,
                });
                builder.final_status = Some((DealStatus::Declined, entry.round));
                builder.events.push(entry.clone());
            }
            DiplomacyEvent::DealSettled { deal_id, status } => {
                let builder = builder(&mut builders, *deal_id)?;
                builder.final_status = Some((*status, entry.round));
                builder.events.push(entry.clone());
            }
            DiplomacyEvent::ImmediateApplied { deal_id, .. }
            | DiplomacyEvent::PromiseSettled { deal_id, .. } => {
                builder(&mut builders, *deal_id)?.events.push(entry.clone());
            }
            DiplomacyEvent::SignalEmitted { .. } => {}
        }
    }

    builders
        .into_iter()
        .map(|(deal_id, builder)| {
            let (final_status, terminal_round) = builder
                .final_status
                .or_else(|| {
                    state
                        .diplomacy
                        .active_deals
                        .get(&deal_id)
                        .map(|deal| (deal.status, state.round))
                })
                .or_else(|| {
                    state
                        .diplomacy
                        .history
                        .iter()
                        .find(|deal| deal.id == deal_id)
                        .map(|deal| (deal.status, deal.terminal_round))
                })
                .ok_or(DiplomacyLogError::MissingStatus(deal_id))?;
            Ok(DiplomacyLogRecord {
                schema: DIPLOMACY_LOG_SCHEMA_V1.to_owned(),
                rules: state.diplomacy.rules_version.clone(),
                game_id: game_id.to_owned(),
                deal_id,
                created_round: builder.created_round,
                proposer: builder.proposer,
                recipient: builder.recipient,
                relationship_snapshot: builder.relationship_snapshot,
                original_bundle: builder.original_bundle,
                counters: builder.counters,
                responses: builder.responses,
                final_status,
                terminal_round,
                events: builder.events,
            })
        })
        .collect()
}

fn builder(
    builders: &mut BTreeMap<DealId, Builder>,
    deal_id: DealId,
) -> Result<&mut Builder, DiplomacyLogError> {
    builders
        .get_mut(&deal_id)
        .ok_or(DiplomacyLogError::EventBeforeOffer(deal_id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use ti4_model::{DealTerm, DiplomacyState, PlayerId, StrategyCardId, TransferAsset};

    fn pid(id: &str) -> PlayerId {
        PlayerId::new(id)
    }

    #[test]
    fn export_retains_the_original_bundle_after_terminal_history_can_prune() {
        let mut state = GameState::new(
            &[pid("a"), pid("b")],
            &[] as &[StrategyCardId],
            BTreeMap::new(),
            None,
            0,
        );
        state.diplomacy = DiplomacyState::for_players(&state.seating_order, true);
        let revision = DealRevision::new(
            0,
            pid("a"),
            vec![DealTerm::FuturePayment {
                asset: TransferAsset::TradeGoods(1),
                deadline_round: 1,
            }],
            vec![],
            1,
        )
        .unwrap();
        let id = state
            .diplomacy
            .create_deal(pid("a"), pid("b"), 1, revision.clone())
            .unwrap();
        state.diplomacy.journal.push(DiplomacyJournalEntry {
            round: 1,
            event: DiplomacyEvent::Declined {
                deal_id: id,
                actor: pid("b"),
                revision: 0,
            },
        });
        state.diplomacy.history.clear();

        let records = export_log_records(&state, "game-1").unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].original_bundle, revision);
        assert_eq!(records[0].final_status, DealStatus::Declined);
        assert_eq!(records[0].responses[0].actor, pid("b"));
    }
}
