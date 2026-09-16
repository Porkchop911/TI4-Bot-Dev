//! Typed promise predicates and deterministic settlement.

use ti4_model::{
    DealId, DealStatus, DealTerm, DiplomacyEvent, DiplomacyJournalEntry, GameState, PlayerId,
    PromiseStatus, SystemId,
};

use super::relations::{RelationshipEvent, apply_relationship_event};
use crate::transactions::{Offer, Terms};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiplomacyEventContext {
    SystemActivated {
        player: PlayerId,
        system: SystemId,
    },
    HostileEngagement {
        attacker: PlayerId,
        victim: PlayerId,
        activation_seq: u32,
    },
    VotesRecorded {
        agenda: String,
    },
    /// `by` refilled `beneficiary`'s commodities with the Trade primary.
    CommoditiesReplenished {
        by: PlayerId,
        beneficiary: PlayerId,
    },
    /// `user` resolved agent `leader` with `beneficiary` as the seat it helped.
    LeaderUsedFor {
        user: PlayerId,
        leader: String,
        beneficiary: PlayerId,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum PromiseError {
    #[error("deal {0:?} does not exist")]
    UnknownDeal(DealId),
    #[error("promise index does not exist")]
    UnknownTerm,
    #[error("promise is already settled")]
    AlreadySettled,
    #[error(transparent)]
    Diplomacy(#[from] ti4_model::DiplomacyError),
    #[error(transparent)]
    Transaction(#[from] crate::transactions::OfferError),
}

/// Settle one future payment only after the linked transfer succeeds atomically.
///
/// # Errors
/// Returns [`PromiseError`] when the linked term or transfer is not currently legal.
pub fn fulfill_payment(
    state: &mut GameState,
    content: &ti4_content::ContentStore,
    galaxy: &ti4_content::galaxy::Galaxy,
    deal_id: DealId,
    promiser: &PlayerId,
    term_index: usize,
) -> Result<(), PromiseError> {
    let deal = state
        .diplomacy
        .active_deals
        .get(&deal_id)
        .cloned()
        .ok_or(PromiseError::UnknownDeal(deal_id))?;
    if deal.status != DealStatus::Active {
        return Err(PromiseError::AlreadySettled);
    }
    let revision = deal.latest();
    let proposer_count = revision.proposer_terms.len();
    let (proposer_side, local, beneficiary, term, status) = if term_index < proposer_count {
        (
            true,
            term_index,
            deal.recipient.clone(),
            revision.proposer_terms.get(term_index),
            revision.proposer_statuses.get(term_index),
        )
    } else {
        let local = term_index - proposer_count;
        (
            false,
            local,
            deal.proposer.clone(),
            revision.recipient_terms.get(local),
            revision.recipient_statuses.get(local),
        )
    };
    let expected = if proposer_side {
        &deal.proposer
    } else {
        &deal.recipient
    };
    if expected != promiser {
        return Err(PromiseError::UnknownTerm);
    }
    if status != Some(&PromiseStatus::Pending) {
        return Err(PromiseError::AlreadySettled);
    }
    let DealTerm::FuturePayment { asset, .. } = term.ok_or(PromiseError::UnknownTerm)? else {
        return Err(PromiseError::UnknownTerm);
    };
    let given = super::transfers::one_asset(asset).map_err(|_| PromiseError::UnknownTerm)?;
    crate::transactions::resolve(
        state,
        content,
        galaxy,
        &Offer {
            proposer: promiser.clone(),
            partner: beneficiary.clone(),
            given,
            received: Terms::default(),
        },
    )?;
    state.record_transaction(promiser, &beneficiary);
    let _ = state.diplomacy.consume_initiation(promiser, &beneficiary);
    settle_term(
        state,
        deal_id,
        proposer_side,
        local,
        term_index,
        PromiseStatus::Fulfilled,
        promiser,
        &beneficiary,
    )?;
    update_deal_terminal(state, deal_id)
}

/// Evaluate a typed gameplay event against all pending promises.
///
/// # Errors
/// Returns [`PromiseError`] if a resulting relationship or deal transition is invalid.
pub fn evaluate_event(
    state: &mut GameState,
    event: &DiplomacyEventContext,
) -> Result<(), PromiseError> {
    if !state.diplomacy.enabled {
        return Ok(());
    }
    if let DiplomacyEventContext::HostileEngagement {
        attacker,
        victim,
        activation_seq,
    } = event
    {
        apply_relationship_event(
            state,
            &RelationshipEvent::DirectAttack {
                attacker: attacker.clone(),
                victim: victim.clone(),
                activation_seq: *activation_seq,
            },
        )?;
    }
    let deal_ids: Vec<DealId> = state.diplomacy.active_deals.keys().copied().collect();
    for id in deal_ids {
        let Some(deal) = state.diplomacy.active_deals.get(&id) else {
            continue;
        };
        if deal.status != DealStatus::Active {
            continue;
        }
        let revision = deal.latest().clone();
        let proposer = deal.proposer.clone();
        let recipient = deal.recipient.clone();
        let mut settlements = Vec::new();
        inspect_terms(
            state,
            event,
            &revision.proposer_terms,
            &revision.proposer_statuses,
            &proposer,
            &recipient,
            true,
            0,
            &mut settlements,
        );
        inspect_terms(
            state,
            event,
            &revision.recipient_terms,
            &revision.recipient_statuses,
            &recipient,
            &proposer,
            false,
            revision.proposer_terms.len(),
            &mut settlements,
        );
        for (proposer_side, index, global, status, promiser, beneficiary) in settlements {
            settle_term(
                state,
                id,
                proposer_side,
                index,
                global,
                status,
                &promiser,
                &beneficiary,
            )?;
        }
        update_deal_terminal(state, id)?;
    }
    // Signals are judged by the same moments as promises.
    super::signals::judge_event(state, event)?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn inspect_terms(
    state: &GameState,
    event: &DiplomacyEventContext,
    terms: &[DealTerm],
    statuses: &[PromiseStatus],
    promiser: &PlayerId,
    beneficiary: &PlayerId,
    proposer_side: bool,
    offset: usize,
    out: &mut Vec<(bool, usize, usize, PromiseStatus, PlayerId, PlayerId)>,
) {
    for (index, (term, status)) in terms.iter().zip(statuses).enumerate() {
        if *status != PromiseStatus::Pending {
            continue;
        }
        let result = match (term, event) {
            (
                DealTerm::DoNotActivate { system, .. },
                DiplomacyEventContext::SystemActivated {
                    player,
                    system: actual,
                },
            ) if player == promiser && actual == system => Some(PromiseStatus::Broken),
            (
                DealTerm::DoNotAttack { player, .. },
                DiplomacyEventContext::HostileEngagement {
                    attacker, victim, ..
                },
            ) if attacker == promiser && victim == player => Some(PromiseStatus::Broken),
            (
                DealTerm::Attack { player, .. },
                DiplomacyEventContext::HostileEngagement {
                    attacker, victim, ..
                },
            ) if attacker == promiser && victim == player => Some(PromiseStatus::Fulfilled),
            (
                DealTerm::Vote {
                    agenda, outcome, ..
                },
                DiplomacyEventContext::VotesRecorded { agenda: actual },
            ) if agenda == actual
                && state
                    .agenda_votes
                    .get(promiser)
                    .is_some_and(|vote| vote == outcome) =>
            {
                Some(PromiseStatus::Fulfilled)
            }
            (
                DealTerm::ReplenishFor {
                    beneficiary: promised,
                    ..
                },
                DiplomacyEventContext::CommoditiesReplenished { by, beneficiary },
            ) if by == promiser && beneficiary == promised => Some(PromiseStatus::Fulfilled),
            (
                DealTerm::UseLeaderFor {
                    leader,
                    beneficiary: helped,
                    ..
                },
                DiplomacyEventContext::LeaderUsedFor {
                    user,
                    leader: used,
                    beneficiary: actual,
                },
            ) if user == promiser && used == leader && actual == helped => {
                Some(PromiseStatus::Fulfilled)
            }
            _ => None,
        };
        if let Some(result) = result {
            // The side is passed, never inferred from `offset == 0`: a bundle whose proposer
            // promises nothing puts the recipient's terms at offset 0 too.
            out.push((
                proposer_side,
                index,
                offset + index,
                result,
                promiser.clone(),
                beneficiary.clone(),
            ));
        }
    }
}

fn settle_term(
    state: &mut GameState,
    deal_id: DealId,
    proposer_side: bool,
    index: usize,
    global: usize,
    status: PromiseStatus,
    promiser: &PlayerId,
    beneficiary: &PlayerId,
) -> Result<(), PromiseError> {
    let deal = state
        .diplomacy
        .active_deals
        .get_mut(&deal_id)
        .ok_or(PromiseError::UnknownDeal(deal_id))?;
    let statuses = if proposer_side {
        &mut deal
            .revisions
            .last_mut()
            .expect("deal revision")
            .proposer_statuses
    } else {
        &mut deal
            .revisions
            .last_mut()
            .expect("deal revision")
            .recipient_statuses
    };
    let slot = statuses.get_mut(index).ok_or(PromiseError::UnknownTerm)?;
    if *slot != PromiseStatus::Pending {
        return Err(PromiseError::AlreadySettled);
    }
    *slot = status;
    state.diplomacy.journal.push(DiplomacyJournalEntry {
        round: state.round,
        event: DiplomacyEvent::PromiseSettled {
            deal_id,
            term_index: u8::try_from(global).unwrap_or(u8::MAX),
            status,
        },
    });
    let relation = match status {
        PromiseStatus::Fulfilled => Some(RelationshipEvent::PromiseFulfilled {
            promiser: promiser.clone(),
            beneficiary: beneficiary.clone(),
        }),
        PromiseStatus::Broken => Some(RelationshipEvent::PromiseBroken {
            promiser: promiser.clone(),
            beneficiary: beneficiary.clone(),
        }),
        PromiseStatus::Pending | PromiseStatus::Expired => None,
    };
    if let Some(event) = relation {
        apply_relationship_event(state, &event)?;
    }
    Ok(())
}

/// Settle every promise whose deadline is the completed round.
///
/// # Errors
/// Returns [`PromiseError`] if a resulting deal transition is invalid.
pub fn settle_deadlines(state: &mut GameState, completed_round: u32) -> Result<(), PromiseError> {
    settle_due(state, Some(completed_round))?;
    if state.diplomacy.enabled {
        super::signals::settle_deadlines(state, completed_round)?;
    }
    Ok(())
}

/// Expire pending promises whose deadlines were never reached at game end.
///
/// # Errors
/// Returns [`PromiseError`] if a resulting deal transition is invalid.
pub fn settle_game_end(state: &mut GameState) -> Result<(), PromiseError> {
    settle_due(state, None)
}

fn settle_due(state: &mut GameState, deadline: Option<u32>) -> Result<(), PromiseError> {
    if !state.diplomacy.enabled {
        return Ok(());
    }
    let ids: Vec<DealId> = state.diplomacy.active_deals.keys().copied().collect();
    for id in ids {
        let Some(deal) = state.diplomacy.active_deals.get(&id) else {
            continue;
        };
        if deal.status != DealStatus::Active {
            continue;
        }
        let revision = deal.latest().clone();
        let proposer = deal.proposer.clone();
        let recipient = deal.recipient.clone();
        let mut settlements = Vec::new();
        deadline_terms(
            &revision.proposer_terms,
            &revision.proposer_statuses,
            deadline,
            &proposer,
            &recipient,
            true,
            0,
            &mut settlements,
        );
        deadline_terms(
            &revision.recipient_terms,
            &revision.recipient_statuses,
            deadline,
            &recipient,
            &proposer,
            false,
            revision.proposer_terms.len(),
            &mut settlements,
        );
        for (side, index, global, status, promiser, beneficiary) in settlements {
            settle_term(
                state,
                id,
                side,
                index,
                global,
                status,
                &promiser,
                &beneficiary,
            )?;
        }
        update_deal_terminal(state, id)?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn deadline_terms(
    terms: &[DealTerm],
    statuses: &[PromiseStatus],
    deadline: Option<u32>,
    promiser: &PlayerId,
    beneficiary: &PlayerId,
    proposer_side: bool,
    offset: usize,
    out: &mut Vec<(bool, usize, usize, PromiseStatus, PlayerId, PlayerId)>,
) {
    for (index, (term, status)) in terms.iter().zip(statuses).enumerate() {
        if *status != PromiseStatus::Pending {
            continue;
        }
        let due = deadline.is_some_and(|round| term.deadline_round().is_some_and(|d| d <= round));
        let result = if due {
            match term {
                // Restraint promises are kept by the deadline passing.
                DealTerm::DoNotActivate { .. } | DealTerm::DoNotAttack { .. } => {
                    PromiseStatus::Fulfilled
                }
                _ => PromiseStatus::Broken,
            }
        } else if deadline.is_none() {
            PromiseStatus::Expired
        } else {
            continue;
        };
        out.push((
            proposer_side,
            index,
            offset + index,
            result,
            promiser.clone(),
            beneficiary.clone(),
        ));
    }
}

fn update_deal_terminal(state: &mut GameState, id: DealId) -> Result<(), PromiseError> {
    let deal = state
        .diplomacy
        .active_deals
        .get_mut(&id)
        .ok_or(PromiseError::UnknownDeal(id))?;
    let statuses = deal
        .latest()
        .proposer_statuses
        .iter()
        .chain(&deal.latest().recipient_statuses);
    let terminal = if statuses.clone().any(|s| *s == PromiseStatus::Broken) {
        Some(DealStatus::Broken)
    } else if statuses.clone().all(|s| *s == PromiseStatus::Fulfilled) {
        Some(DealStatus::Fulfilled)
    } else if statuses.clone().all(|s| *s != PromiseStatus::Pending) {
        Some(DealStatus::Expired)
    } else {
        None
    };
    if let Some(status) = terminal {
        deal.status = status;
        state.diplomacy.journal.push(DiplomacyJournalEntry {
            round: state.round,
            event: DiplomacyEvent::DealSettled {
                deal_id: id,
                status,
            },
        });
        state.diplomacy.archive_deal(id, state.round)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use ti4_model::{DealRevision, DiplomacyState, StrategyCardId, TransferAsset};

    fn pid(s: &str) -> PlayerId {
        PlayerId::new(s)
    }
    fn active(term: DealTerm) -> GameState {
        let mut state = GameState::new(
            &[pid("a"), pid("b")],
            &[] as &[StrategyCardId],
            BTreeMap::new(),
            None,
            0,
        );
        state.diplomacy = DiplomacyState::for_players(&state.seating_order, true);
        let rev = DealRevision::new(0, pid("a"), vec![term], vec![], 1).unwrap();
        let id = state
            .diplomacy
            .create_deal(pid("a"), pid("b"), 1, rev)
            .unwrap();
        state.diplomacy.active_deals.get_mut(&id).unwrap().status = DealStatus::Active;
        state
    }

    #[test]
    fn prohibitions_fulfil_and_positive_promises_break_at_deadline() {
        let mut prohibition = active(DealTerm::DoNotAttack {
            player: pid("b"),
            deadline_round: 1,
        });
        settle_deadlines(&mut prohibition, 1).unwrap();
        assert_eq!(
            prohibition.diplomacy.history[0].status,
            DealStatus::Fulfilled
        );
        let mut payment = active(DealTerm::FuturePayment {
            asset: TransferAsset::TradeGoods(1),
            deadline_round: 1,
        });
        settle_deadlines(&mut payment, 1).unwrap();
        assert_eq!(payment.diplomacy.history[0].status, DealStatus::Broken);
    }

    #[test]
    fn a_refresh_promise_settles_when_the_trade_primary_names_that_seat() {
        let mut kept = active(DealTerm::ReplenishFor {
            beneficiary: pid("b"),
            deadline_round: 1,
        });
        evaluate_event(
            &mut kept,
            &DiplomacyEventContext::CommoditiesReplenished {
                by: pid("a"),
                beneficiary: pid("c"),
            },
        )
        .unwrap();
        assert!(
            kept.diplomacy.history.is_empty(),
            "refreshing somebody else keeps nothing"
        );
        evaluate_event(
            &mut kept,
            &DiplomacyEventContext::CommoditiesReplenished {
                by: pid("a"),
                beneficiary: pid("b"),
            },
        )
        .unwrap();
        assert_eq!(kept.diplomacy.history[0].status, DealStatus::Fulfilled);

        let mut ignored = active(DealTerm::ReplenishFor {
            beneficiary: pid("b"),
            deadline_round: 1,
        });
        settle_deadlines(&mut ignored, 1).unwrap();
        assert_eq!(
            ignored.diplomacy.history[0].status,
            DealStatus::Broken,
            "a refresh never given is a broken promise, not a lapsed one"
        );
    }

    #[test]
    fn an_agent_favour_settles_on_the_use_it_names() {
        let mut agent = active(DealTerm::UseLeaderFor {
            leader: "hacanagent".to_owned(),
            beneficiary: pid("b"),
            deadline_round: 1,
        });
        evaluate_event(
            &mut agent,
            &DiplomacyEventContext::LeaderUsedFor {
                user: pid("a"),
                leader: "hacanagent".to_owned(),
                beneficiary: pid("a"),
            },
        )
        .unwrap();
        assert!(
            agent.diplomacy.history.is_empty(),
            "an agent used for someone else keeps nothing"
        );
        evaluate_event(
            &mut agent,
            &DiplomacyEventContext::LeaderUsedFor {
                user: pid("a"),
                leader: "hacanagent".to_owned(),
                beneficiary: pid("b"),
            },
        )
        .unwrap();
        assert_eq!(agent.diplomacy.history[0].status, DealStatus::Fulfilled);
    }

    #[test]
    fn activation_breaks_only_the_matching_predicate() {
        let mut state = active(DealTerm::DoNotActivate {
            system: SystemId::new("18"),
            deadline_round: 2,
        });
        evaluate_event(
            &mut state,
            &DiplomacyEventContext::SystemActivated {
                player: pid("a"),
                system: SystemId::new("19"),
            },
        )
        .unwrap();
        assert!(state.diplomacy.history.is_empty());
        evaluate_event(
            &mut state,
            &DiplomacyEventContext::SystemActivated {
                player: pid("a"),
                system: SystemId::new("18"),
            },
        )
        .unwrap();
        assert_eq!(state.diplomacy.history[0].status, DealStatus::Broken);
    }

    #[test]
    fn a_promise_from_the_recipient_of_a_one_sided_deal_settles_at_its_deadline() {
        let mut state = GameState::new(
            &[pid("a"), pid("b")],
            &[] as &[StrategyCardId],
            BTreeMap::new(),
            None,
            0,
        );
        state.diplomacy = DiplomacyState::for_players(&state.seating_order, true);
        // a promises nothing; b promises to pay a trade good by the end of round 1.
        let revision = DealRevision::new(
            0,
            pid("a"),
            vec![],
            vec![DealTerm::FuturePayment {
                asset: TransferAsset::TradeGoods(1),
                deadline_round: 1,
            }],
            1,
        )
        .unwrap();
        let id = state
            .diplomacy
            .create_deal(pid("a"), pid("b"), 1, revision)
            .unwrap();
        state.diplomacy.active_deals.get_mut(&id).unwrap().status = DealStatus::Active;

        settle_deadlines(&mut state, 1).unwrap();

        assert_eq!(state.diplomacy.history[0].status, DealStatus::Broken);
        assert_eq!(
            state.diplomacy.relationship(&pid("a"), &pid("b")).trust,
            -40,
            "a, the beneficiary, loses trust in b"
        );
    }
}
