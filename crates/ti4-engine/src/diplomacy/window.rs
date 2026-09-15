//! Resumable, two-counter structured negotiation window.

use serde::{Deserialize, Serialize};
use ti4_model::{
    DealId, DealStatus, DealTerm, DiplomacyEvent, DiplomacyJournalEntry, GameState, PlayerId,
};

use crate::choice::{Choice, ChoiceOption, IllegalChoice, Resolving, Window};
use crate::decision_context::{DecisionContext, DecisionSource, DecisionTarget};
use crate::transactions::Offer;

use super::candidates::{CandidateBundle, generate_counter_candidates};
use super::relations::{RelationshipEvent, apply_relationship_event};
use super::signals::{SignalDraft, emit_signal_in_open_contact};

pub const OFFER_KIND: &str = "diplomacy_offer";
pub const RESPONSE_KIND: &str = "diplomacy_response";
pub const COUNTER_KIND: &str = "diplomacy_counter";
pub const ACCEPT_ID: &str = "diplomacy|accept";
pub const DECLINE_ID: &str = "diplomacy|decline";
pub const SIGNAL_KIND: &str = "diplomacy_signal";
const SIGNAL_PREFIX: &str = "diplomacy|signal|";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiplomacyStage {
    Proposing,
    Responding,
    Done,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiplomacyWindow {
    pub proposer: PlayerId,
    pub recipient: PlayerId,
    pub stage: DiplomacyStage,
    pub candidates: Vec<CandidateBundle>,
    pub current: Option<CandidateBundle>,
    pub deal_id: Option<DealId>,
    pub responses: u8,
}

impl DiplomacyWindow {
    /// Open a bounded contact after consuming the ordered-pair turn allowance.
    ///
    /// # Errors
    /// Returns [`IllegalChoice`] if diplomacy is disabled or the allowance was already consumed.
    pub fn open(
        state: &mut GameState,
        proposer: PlayerId,
        recipient: PlayerId,
        candidates: Vec<CandidateBundle>,
    ) -> Result<Self, IllegalChoice> {
        // No surviving bundle is still a contact: a signal and "make no offer" remain legal, and
        // `available_contacts` cannot see bundle filtering without the map, so refusing here would
        // turn an offered option into a refused step.
        if !state.diplomacy.enabled || !state.diplomacy.consume_initiation(&proposer, &recipient) {
            return Err(failed(&proposer, "diplomatic contact is unavailable"));
        }
        Ok(Self {
            proposer,
            recipient,
            stage: DiplomacyStage::Proposing,
            candidates,
            current: None,
            deal_id: None,
            responses: 0,
        })
    }

    fn counters(
        &self,
        state: &GameState,
        current: &CandidateBundle,
        author: &PlayerId,
    ) -> Vec<CandidateBundle> {
        generate_counter_candidates(
            state,
            &self.proposer,
            &self.recipient,
            current,
            author,
            state.round,
        )
    }

    fn actor(&self) -> PlayerId {
        if self.responses.is_multiple_of(2) {
            self.recipient.clone()
        } else {
            self.proposer.clone()
        }
    }

    fn response_options(&self, state: &GameState) -> Vec<ChoiceOption> {
        let actor = self.actor();
        let actor_is_proposer = actor == self.proposer;
        // Accepting takes the bundle on the table, so the accept option carries it: a policy
        // answering "accept" has to see what it is accepting.
        let accept = ChoiceOption::labelled(ACCEPT_ID, RESPONSE_KIND, "Accept");
        let accept = match &self.current {
            Some(current) => bundle_payload(accept, current, actor_is_proposer),
            None => accept,
        };
        let mut options = vec![
            accept,
            ChoiceOption::labelled(DECLINE_ID, RESPONSE_KIND, "Decline"),
        ];
        if self.responses < 2
            && let Some(current) = &self.current
        {
            options.extend(
                self.counters(state, current, &actor)
                    .into_iter()
                    .map(|candidate| candidate_option(&candidate, COUNTER_KIND, actor_is_proposer)),
            );
        }
        options
    }
}

impl Window for DiplomacyWindow {
    fn pending_choice(
        &self,
        state: &GameState,
        _content: &ti4_content::ContentStore,
        _sources: ti4_model::SourceSet,
    ) -> Option<Choice> {
        match self.stage {
            DiplomacyStage::Done => None,
            DiplomacyStage::Proposing => {
                let mut options: Vec<_> = self
                    .candidates
                    .iter()
                    .map(|c| candidate_option(c, OFFER_KIND, true))
                    .collect();
                options.extend(signal_options());
                options.push(ChoiceOption::labelled(
                    DECLINE_ID,
                    RESPONSE_KIND,
                    "Make no offer",
                ));
                Some(
                    Choice::new(
                        self.proposer.clone(),
                        "Choose a structured diplomatic offer",
                        options,
                    )
                    .contextualized(
                        DecisionContext::new(
                            self.proposer.clone(),
                            DecisionSource::Rule("94".to_owned()),
                            "diplomacy_offer",
                            state.phase,
                            state.round,
                        )
                        .optional(true)
                        .about(DecisionTarget::Player(self.recipient.clone())),
                    ),
                )
            }
            DiplomacyStage::Responding => {
                let actor = self.actor();
                Some(
                    Choice::new(
                        actor.clone(),
                        "Respond to the structured diplomatic offer",
                        self.response_options(state),
                    )
                    .contextualized(
                        DecisionContext::new(
                            actor,
                            DecisionSource::Rule("94".to_owned()),
                            "diplomacy_response",
                            state.phase,
                            state.round,
                        )
                        .optional(true)
                        .about(DecisionTarget::Player(
                            if self.responses.is_multiple_of(2) {
                                self.proposer.clone()
                            } else {
                                self.recipient.clone()
                            },
                        )),
                    ),
                )
            }
        }
    }

    #[expect(
        clippy::too_many_lines,
        reason = "the resumable negotiation transition table remains explicit and linear"
    )]
    fn resolve(
        &mut self,
        state: &mut GameState,
        ctx: &mut Resolving<'_>,
        answer: ChoiceOption,
    ) -> Result<(), IllegalChoice> {
        let choice = self
            .pending_choice(state, ctx.content, ctx.sources)
            .ok_or_else(|| failed(&self.proposer, "negotiation is already complete"))?;
        let offered =
            choice
                .option(&answer.id)
                .cloned()
                .ok_or_else(|| IllegalChoice::NotOffered {
                    player: choice.player.clone(),
                    chosen: answer.id.clone(),
                    offered: choice.ids().into_iter().map(str::to_owned).collect(),
                })?;
        match self.stage {
            DiplomacyStage::Proposing => {
                if offered.id == DECLINE_ID {
                    self.stage = DiplomacyStage::Done;
                    return Ok(());
                }
                if let Some(kind) = signal_kind(&offered.id) {
                    emit_signal_in_open_contact(
                        state,
                        SignalDraft {
                            speaker: self.proposer.clone(),
                            target: self.recipient.clone(),
                            kind,
                            subject: ti4_model::SignalSubject::Player(self.recipient.clone()),
                            condition: None,
                            expires_round: state.round,
                        },
                    )
                    .map_err(|error| failed(&self.proposer, &error.to_string()))?;
                    self.stage = DiplomacyStage::Done;
                    return Ok(());
                }
                let candidate = self
                    .candidates
                    .iter()
                    .find(|candidate| candidate.id == offered.id)
                    .cloned()
                    .ok_or_else(|| failed(&self.proposer, "stored offer disappeared"))?;
                let id = state
                    .diplomacy
                    .create_deal(
                        self.proposer.clone(),
                        self.recipient.clone(),
                        state.round,
                        candidate.revision.clone(),
                    )
                    .map_err(|error| failed(&self.proposer, &error.to_string()))?;
                self.current = Some(candidate);
                self.deal_id = Some(id);
                self.stage = DiplomacyStage::Responding;
            }
            DiplomacyStage::Responding => {
                let id = self
                    .deal_id
                    .ok_or_else(|| failed(&choice.player, "negotiation has no deal"))?;
                if offered.id == ACCEPT_ID {
                    apply_acceptance(state, ctx, id, &choice.player)?;
                    self.stage = DiplomacyStage::Done;
                } else if offered.id == DECLINE_ID {
                    let deal = state
                        .diplomacy
                        .active_deals
                        .get_mut(&id)
                        .ok_or_else(|| failed(&choice.player, "deal disappeared"))?;
                    deal.status = DealStatus::Declined;
                    state.diplomacy.journal.push(DiplomacyJournalEntry {
                        round: state.round,
                        event: DiplomacyEvent::Declined {
                            deal_id: id,
                            actor: choice.player.clone(),
                            revision: deal.latest().number,
                        },
                    });
                    state
                        .diplomacy
                        .archive_deal(id, state.round)
                        .map_err(|error| failed(&choice.player, &error.to_string()))?;
                    self.stage = DiplomacyStage::Done;
                } else {
                    let current = self
                        .current
                        .as_ref()
                        .ok_or_else(|| failed(&choice.player, "current bundle disappeared"))?;
                    let candidate = self
                        .counters(state, current, &choice.player)
                        .into_iter()
                        .find(|candidate| candidate.id == offered.id)
                        .ok_or_else(|| failed(&choice.player, "stored counter disappeared"))?;
                    state
                        .diplomacy
                        .active_deals
                        .get_mut(&id)
                        .ok_or_else(|| failed(&choice.player, "deal disappeared"))?
                        .add_counter(candidate.revision.clone())
                        .map_err(|error| failed(&choice.player, &error.to_string()))?;
                    state.diplomacy.journal.push(DiplomacyJournalEntry {
                        round: state.round,
                        event: DiplomacyEvent::Countered {
                            deal_id: id,
                            revision: candidate.revision.clone(),
                        },
                    });
                    self.current = Some(candidate);
                    self.responses += 1;
                }
            }
            DiplomacyStage::Done => unreachable!(),
        }
        Ok(())
    }
}

fn signal_options() -> Vec<ChoiceOption> {
    [
        ("request", ti4_model::SignalKind::Request),
        ("threat", ti4_model::SignalKind::Threat),
        ("assurance", ti4_model::SignalKind::Assurance),
        ("warning", ti4_model::SignalKind::Warning),
    ]
    .into_iter()
    .map(|(id, _)| ChoiceOption::labelled(format!("{SIGNAL_PREFIX}{id}"), SIGNAL_KIND, id))
    .collect()
}

fn signal_kind(id: &str) -> Option<ti4_model::SignalKind> {
    match id.strip_prefix(SIGNAL_PREFIX)? {
        "request" => Some(ti4_model::SignalKind::Request),
        "threat" => Some(ti4_model::SignalKind::Threat),
        "assurance" => Some(ti4_model::SignalKind::Assurance),
        "warning" => Some(ti4_model::SignalKind::Warning),
        _ => None,
    }
}

/// An option carrying a whole bundle.
///
/// Terms are stored against the original proposer and recipient however many counters later, so
/// `actor_is_proposer` says which side is the seat deciding; without it a policy could not tell
/// what it gives from what it gets.
fn candidate_option(
    candidate: &CandidateBundle,
    kind: &str,
    actor_is_proposer: bool,
) -> ChoiceOption {
    bundle_payload(
        ChoiceOption::labelled(
            candidate.id.clone(),
            kind,
            format!("{:?}", candidate.template),
        ),
        candidate,
        actor_is_proposer,
    )
}

fn bundle_payload(
    option: ChoiceOption,
    candidate: &CandidateBundle,
    actor_is_proposer: bool,
) -> ChoiceOption {
    option
        .with(
            "bundle",
            serde_json::to_value(candidate).expect("candidate serializes"),
        )
        .with("actor_is_proposer", actor_is_proposer)
}

#[expect(
    clippy::too_many_lines,
    reason = "validation, atomic transfer, journaling and lifecycle transition share one boundary"
)]
fn apply_acceptance(
    state: &mut GameState,
    ctx: &mut Resolving<'_>,
    id: DealId,
    actor: &PlayerId,
) -> Result<(), IllegalChoice> {
    let deal = state
        .diplomacy
        .active_deals
        .get(&id)
        .cloned()
        .ok_or_else(|| failed(&PlayerId::new(""), "deal disappeared"))?;
    let revision = deal.latest();
    let given = super::transfers::immediate_terms(&revision.proposer_terms)
        .map_err(|reason| failed(&deal.proposer, reason))?;
    let received = super::transfers::immediate_terms(&revision.recipient_terms)
        .map_err(|reason| failed(&deal.recipient, reason))?;
    if !given.is_empty() || !received.is_empty() {
        let galaxy = ctx
            .timing
            .as_ref()
            .and_then(|timing| timing.galaxy)
            .ok_or_else(|| {
                failed(
                    &deal.proposer,
                    "immediate diplomacy terms require a galaxy context",
                )
            })?;
        crate::transactions::resolve(
            state,
            ctx.content,
            galaxy,
            &Offer {
                proposer: deal.proposer.clone(),
                partner: deal.recipient.clone(),
                given,
                received,
            },
        )
        .map_err(|error| failed(&deal.proposer, &error.to_string()))?;
    }
    let active = state
        .diplomacy
        .active_deals
        .get_mut(&id)
        .expect("checked deal");
    active.status = DealStatus::Accepted;
    state.diplomacy.journal.push(DiplomacyJournalEntry {
        round: state.round,
        event: DiplomacyEvent::Accepted {
            deal_id: id,
            actor: actor.clone(),
            revision: revision.number,
        },
    });
    for (index, term) in revision
        .proposer_terms
        .iter()
        .chain(&revision.recipient_terms)
        .enumerate()
    {
        if term.is_immediate() {
            state.diplomacy.journal.push(DiplomacyJournalEntry {
                round: state.round,
                event: DiplomacyEvent::ImmediateApplied {
                    deal_id: id,
                    term_index: u8::try_from(index).unwrap_or(u8::MAX),
                },
            });
        }
    }
    let future = revision
        .proposer_terms
        .iter()
        .chain(&revision.recipient_terms)
        .any(|term| !term.is_immediate());
    active.status = if future {
        DealStatus::Active
    } else {
        DealStatus::Fulfilled
    };
    apply_relationship_event(
        state,
        &RelationshipEvent::DealAccepted {
            a: deal.proposer.clone(),
            b: deal.recipient.clone(),
        },
    )
    .map_err(|error| failed(&PlayerId::new(""), &error.to_string()))?;
    for term in &revision.recipient_terms {
        if let DealTerm::Attack { player: target, .. } = term
            && target != &deal.proposer
            && target != &deal.recipient
        {
            apply_relationship_event(
                state,
                &RelationshipEvent::AntiThirdPartyDeal {
                    payer: deal.proposer.clone(),
                    attacker: deal.recipient.clone(),
                    target: target.clone(),
                },
            )
            .map_err(|error| failed(&PlayerId::new(""), &error.to_string()))?;
        }
    }
    if !future {
        state.diplomacy.journal.push(DiplomacyJournalEntry {
            round: state.round,
            event: DiplomacyEvent::DealSettled {
                deal_id: id,
                status: DealStatus::Fulfilled,
            },
        });
        state
            .diplomacy
            .archive_deal(id, state.round)
            .map_err(|error| failed(&PlayerId::new(""), &error.to_string()))?;
    }
    Ok(())
}

fn failed(player: &PlayerId, reason: &str) -> IllegalChoice {
    IllegalChoice::DeciderFailed {
        player: player.clone(),
        prompt: "structured diplomacy".to_owned(),
        reason: reason.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diplomacy::candidates::{CandidateFeatures, DealTemplate};
    use std::collections::BTreeMap;
    use ti4_model::{DealRevision, DiplomacyState, StrategyCardId, TransferAsset};

    fn pid(id: &str) -> PlayerId {
        PlayerId::new(id)
    }

    #[test]
    fn caller_payload_is_ignored_and_a_third_counter_is_never_offered() {
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
            vec![],
            vec![DealTerm::FuturePayment {
                asset: TransferAsset::TradeGoods(2),
                deadline_round: 1,
            }],
            1,
        )
        .unwrap();
        let candidate = CandidateBundle {
            id: "diplomacy|test".to_owned(),
            template: DealTemplate::FuturePayment,
            revision,
            features: CandidateFeatures {
                immediate_value_self: 0.0,
                immediate_value_other: 0.0,
                future_value_self: 0.0,
                future_value_other: 2.0,
                target_relationship_effect: 0.0,
                objective_relevance: 0.0,
                military_relevance: 0.0,
            },
        };
        let mut window =
            DiplomacyWindow::open(&mut state, pid("a"), pid("b"), vec![candidate]).unwrap();
        let content = ti4_content::ContentStore::embedded();
        let mut dice = crate::Dice::new();
        let mut rng = crate::GameRng::new(1);
        let mut table = crate::Table::new();
        let mut resolving = Resolving {
            content,
            sources: ti4_model::POK,
            dice: &mut dice,
            rng: &mut rng,
            table: &mut table,
            timing: None,
        };

        let tampered =
            ChoiceOption::new("diplomacy|test", "invented").with("bundle", "caller controlled");
        window
            .resolve(&mut state, &mut resolving, tampered)
            .unwrap();
        assert_eq!(
            state.diplomacy.active_deals[&DealId(1)]
                .latest()
                .recipient_terms[0],
            DealTerm::FuturePayment {
                asset: TransferAsset::TradeGoods(2),
                deadline_round: 1
            }
        );

        for _ in 0..2 {
            let choice = window
                .pending_choice(&state, content, ti4_model::POK)
                .unwrap();
            let counter = choice
                .options
                .iter()
                .find(|option| option.kind == COUNTER_KIND)
                .unwrap()
                .clone();
            window.resolve(&mut state, &mut resolving, counter).unwrap();
        }
        let final_response = window
            .pending_choice(&state, content, ti4_model::POK)
            .unwrap();
        assert!(
            !final_response
                .options
                .iter()
                .any(|option| option.kind == COUNTER_KIND)
        );
    }

    #[test]
    fn selecting_a_signal_closes_without_opening_a_deal() {
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
            vec![],
            vec![DealTerm::FuturePayment {
                asset: TransferAsset::TradeGoods(1),
                deadline_round: 1,
            }],
            1,
        )
        .unwrap();
        let candidate = CandidateBundle {
            id: "deal".to_owned(),
            template: DealTemplate::FuturePayment,
            revision,
            features: CandidateFeatures {
                immediate_value_self: 0.0,
                immediate_value_other: 0.0,
                future_value_self: 0.0,
                future_value_other: 1.0,
                target_relationship_effect: 0.0,
                objective_relevance: 0.0,
                military_relevance: 0.0,
            },
        };
        let mut window =
            DiplomacyWindow::open(&mut state, pid("a"), pid("b"), vec![candidate]).unwrap();
        let content = ti4_content::ContentStore::embedded();
        let mut dice = crate::Dice::new();
        let mut rng = crate::GameRng::new(1);
        let mut table = crate::Table::new();
        let mut resolving = Resolving {
            content,
            sources: ti4_model::POK,
            dice: &mut dice,
            rng: &mut rng,
            table: &mut table,
            timing: None,
        };
        window
            .resolve(
                &mut state,
                &mut resolving,
                ChoiceOption::new("diplomacy|signal|threat", SIGNAL_KIND),
            )
            .unwrap();
        assert!(
            window
                .pending_choice(&state, content, ti4_model::POK)
                .is_none()
        );
        assert!(state.diplomacy.active_deals.is_empty());
        assert_eq!(state.diplomacy.recent_signals.len(), 1);
    }
}
