//! Resumable, two-counter structured negotiation window, with deals built item by item.
//!
//! `plans/TRADE_REWORK_2026-09-22.md`: the side building a revision adds what it gives, then what it
//! asks for, one item at a time, reviews, and proposes. The other side accepts, declines, or builds
//! a counter the same way, at most twice.

use serde::{Deserialize, Serialize};
use ti4_model::{
    DealId, DealRevision, DealStatus, DealTerm, DiplomacyEvent, DiplomacyJournalEntry, GameState,
    PlayerId, SignalStatement,
};

use crate::choice::{Choice, ChoiceOption, IllegalChoice, Resolving, Window};
use crate::decision_context::{DecisionContext, DecisionSource, DecisionTarget};
use crate::transactions::Offer;

use super::builder::{
    CANCEL_ID, ContactScope, DONE_ID, Draft, EDIT_ID, ITEM_KIND, PROPOSE_ID, REVIEW_KIND,
    amount_options, apply_item, describe, item_options,
};
use super::candidates::{CandidateBundle, built_bundle};
use super::relations::{RelationshipEvent, apply_relationship_event};
use super::signals::{SignalDraft, emit_signal_in_open_contact};

pub const RESPONSE_KIND: &str = "diplomacy_response";
pub const COUNTER_KIND: &str = "diplomacy_counter";
pub const ACCEPT_ID: &str = "diplomacy|accept";
pub const DECLINE_ID: &str = "diplomacy|decline";
pub const COUNTER_ID: &str = "diplomacy|counter";
pub const SIGNAL_KIND: &str = "diplomacy_signal";
const SIGNAL_PREFIX: &str = "diplomacy|signal|";
/// Counter-offers allowed in one negotiation (operator decision, 2026-09-22).
pub const MAX_COUNTERS: u8 = ti4_model::diplomacy::MAX_COUNTERS;
/// Steps of the build budget one "edit" spends: at most two edits per revision.
pub const EDIT_COST: u8 = crate::diplomacy::builder::MAX_BUILD_STEPS / 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiplomacyStage {
    /// A revision is being built: the opening offer, or a counter.
    Building,
    Responding,
    Done,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiplomacyWindow {
    pub proposer: PlayerId,
    pub recipient: PlayerId,
    pub stage: DiplomacyStage,
    /// What the contact knew when it opened (physical trade allowed, promise subjects).
    pub scope: ContactScope,
    /// The concrete signals the proposer may send instead of an offer, generated at contact.
    #[serde(default)]
    pub signals: Vec<SignalStatement>,
    /// The revision under construction while `stage` is `Building`.
    pub draft: Option<Draft>,
    /// The revision on the table.
    pub current: Option<CandidateBundle>,
    pub deal_id: Option<DealId>,
    /// Counters made so far.
    pub responses: u8,
    /// A counter was started and abandoned. It is not a counter made, but the choice to counter
    /// is spent: otherwise "counter, walk away, counter" could run forever.
    #[serde(default)]
    pub counter_abandoned: bool,
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
        scope: ContactScope,
        signals: Vec<SignalStatement>,
    ) -> Result<Self, IllegalChoice> {
        if !state.diplomacy.enabled || !state.diplomacy.consume_initiation(&proposer, &recipient) {
            return Err(failed(&proposer, "diplomatic contact is unavailable"));
        }
        let draft = Draft::new(proposer.clone(), recipient.clone());
        Ok(Self {
            proposer,
            recipient,
            stage: DiplomacyStage::Building,
            scope,
            signals,
            draft: Some(draft),
            current: None,
            deal_id: None,
            responses: 0,
            counter_abandoned: false,
        })
    }

    /// Who answers the revision on the table.
    fn actor(&self) -> PlayerId {
        if self.responses.is_multiple_of(2) {
            self.recipient.clone()
        } else {
            self.proposer.clone()
        }
    }

    /// The draft's terms as a revision stored against the original proposer and recipient.
    fn revision(&self, state: &GameState, draft: &Draft) -> Result<DealRevision, IllegalChoice> {
        let (proposer_terms, recipient_terms) = if draft.builder == self.proposer {
            (draft.give.clone(), draft.take.clone())
        } else {
            (draft.take.clone(), draft.give.clone())
        };
        // Revision 0 is the opening offer; each counter is the next number.
        let number = if self.deal_id.is_some() {
            self.responses.saturating_add(1)
        } else {
            0
        };
        DealRevision::new(
            number,
            draft.builder.clone(),
            proposer_terms,
            recipient_terms,
            state.round,
        )
        .map_err(|error| failed(&draft.builder, &error.to_string()))
    }

    fn response_options(&self) -> Vec<ChoiceOption> {
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
        if self.responses < MAX_COUNTERS && self.current.is_some() && !self.counter_abandoned {
            options.push(ChoiceOption::labelled(
                COUNTER_ID,
                COUNTER_KIND,
                "Counter: change the terms",
            ));
        }
        options
    }

    fn building_choice(
        &self,
        state: &GameState,
        content: &ti4_content::ContentStore,
    ) -> Option<Choice> {
        let draft = self.draft.as_ref()?;
        let summary = format!(
            "you give: {} · you ask: {}",
            list(&draft.give),
            list(&draft.take)
        );
        let (prompt, subtype, mut options) = if draft.pending.is_some() {
            (
                format!("How many? ({summary})"),
                "diplomacy_amount",
                amount_options(state, draft),
            )
        } else if draft.reviewing {
            let mut options = Vec::new();
            if !draft.is_empty() {
                options.push(ChoiceOption::labelled(
                    PROPOSE_ID,
                    REVIEW_KIND,
                    "Propose these terms",
                ));
            }
            if draft.can_edit() {
                options.push(ChoiceOption::labelled(
                    EDIT_ID,
                    REVIEW_KIND,
                    "Edit the terms",
                ));
            }
            options.push(ChoiceOption::labelled(
                CANCEL_ID,
                REVIEW_KIND,
                if self.current.is_some() {
                    "Back to the offer on the table"
                } else {
                    "Make no offer"
                },
            ));
            (format!("Review: {summary}"), "diplomacy_review", options)
        } else {
            let mut options = item_options(state, content, &self.scope, draft);
            options.push(ChoiceOption::labelled(
                DONE_ID,
                ITEM_KIND,
                if draft.asking {
                    "done asking"
                } else {
                    "done offering"
                },
            ));
            // The opening screen of a fresh contact also sends a signal or walks away.
            if self.current.is_none() && draft.is_empty() && !draft.asking {
                options.extend(
                    self.signals
                        .iter()
                        .map(|statement| signal_option(statement, state.round)),
                );
                options.push(ChoiceOption::labelled(
                    CANCEL_ID,
                    REVIEW_KIND,
                    "Make no offer",
                ));
            }
            let (prompt, subtype) = if draft.asking {
                (format!("Ask for what? ({summary})"), "diplomacy_ask_item")
            } else {
                (format!("Offer what? ({summary})"), "diplomacy_offer_item")
            };
            (prompt, subtype, options)
        };
        let draft_value = serde_json::to_value(draft).expect("a draft serializes");
        options = options
            .into_iter()
            .map(|option| option.with("draft", draft_value.clone()))
            .collect();
        Some(
            Choice::new(draft.builder.clone(), prompt, options).contextualized(
                DecisionContext::new(
                    draft.builder.clone(),
                    DecisionSource::Rule("94".to_owned()),
                    subtype,
                    state.phase,
                    state.round,
                )
                .optional(true)
                .about(DecisionTarget::Player(draft.other.clone())),
            ),
        )
    }
}

fn list(terms: &[DealTerm]) -> String {
    if terms.is_empty() {
        "nothing".to_owned()
    } else {
        terms.iter().map(describe).collect::<Vec<_>>().join(", ")
    }
}

impl Window for DiplomacyWindow {
    fn pending_choice(
        &self,
        state: &GameState,
        content: &ti4_content::ContentStore,
        _sources: ti4_model::SourceSet,
    ) -> Option<Choice> {
        match self.stage {
            DiplomacyStage::Done => None,
            DiplomacyStage::Building => self.building_choice(state, content),
            DiplomacyStage::Responding => {
                let actor = self.actor();
                Some(
                    Choice::new(
                        actor.clone(),
                        "Respond to the structured diplomatic offer",
                        self.response_options(),
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
            DiplomacyStage::Building => {
                let mut draft = self
                    .draft
                    .clone()
                    .ok_or_else(|| failed(&choice.player, "no draft is open"))?;
                if let Some(statement) = self
                    .signals
                    .iter()
                    .find(|statement| signal_id(statement) == offered.id)
                    .cloned()
                {
                    // A vote assurance is about this agenda, so it is judged before the round ends.
                    let expires_round = if matches!(&statement, SignalStatement::WillVote { .. }) {
                        state.round
                    } else {
                        state.round.saturating_add(1)
                    };
                    emit_signal_in_open_contact(
                        state,
                        SignalDraft {
                            speaker: self.proposer.clone(),
                            target: self.recipient.clone(),
                            statement,
                            expires_round,
                        },
                    )
                    .map_err(|error| failed(&self.proposer, &error.to_string()))?;
                    self.draft = None;
                    self.stage = DiplomacyStage::Done;
                    return Ok(());
                }
                match offered.id.as_str() {
                    DONE_ID if !draft.asking => draft.asking = true,
                    DONE_ID => draft.reviewing = true,
                    EDIT_ID => {
                        draft.reviewing = false;
                        draft.asking = false;
                        // Going back to edit spends the budget too: a near-greedy seat otherwise
                        // cycles done -> done -> review -> edit without end (seen in the replayer).
                        draft.steps = draft.steps.saturating_add(EDIT_COST);
                    }
                    CANCEL_ID => {
                        self.draft = None;
                        if self.current.is_some() {
                            self.counter_abandoned = true;
                        }
                        // Walking away from a counter leaves the offer on the table to answer.
                        self.stage = if self.current.is_some() {
                            DiplomacyStage::Responding
                        } else {
                            DiplomacyStage::Done
                        };
                        return Ok(());
                    }
                    PROPOSE_ID => {
                        let revision = self.revision(state, &draft)?;
                        let candidate = built_bundle(revision.clone());
                        if let Some(id) = self.deal_id {
                            state
                                .diplomacy
                                .active_deals
                                .get_mut(&id)
                                .ok_or_else(|| failed(&choice.player, "deal disappeared"))?
                                .add_counter(revision.clone())
                                .map_err(|error| failed(&choice.player, &error.to_string()))?;
                            state.diplomacy.journal.push(DiplomacyJournalEntry {
                                round: state.round,
                                event: DiplomacyEvent::Countered {
                                    deal_id: id,
                                    revision,
                                },
                            });
                            self.responses += 1;
                        } else {
                            let id = state
                                .diplomacy
                                .create_deal(
                                    self.proposer.clone(),
                                    self.recipient.clone(),
                                    state.round,
                                    revision,
                                )
                                .map_err(|error| failed(&self.proposer, &error.to_string()))?;
                            self.deal_id = Some(id);
                        }
                        self.current = Some(candidate);
                        self.draft = None;
                        self.stage = DiplomacyStage::Responding;
                        return Ok(());
                    }
                    id => {
                        if !apply_item(state, &mut draft, id) {
                            return Err(failed(&choice.player, "unknown deal item"));
                        }
                    }
                }
                self.draft = Some(draft);
            }
            DiplomacyStage::Responding => {
                let id = self
                    .deal_id
                    .ok_or_else(|| failed(&choice.player, "negotiation has no deal"))?;
                // A deal that cannot settle when accepted (its transfers no longer legal) is void,
                // like a decline: settlement checks everything before anything moves, so nothing
                // has changed, and one bad deal must not end a game.
                let accepted = offered.id == ACCEPT_ID
                    && apply_acceptance(state, ctx, id, &choice.player).is_ok();
                if accepted {
                    self.stage = DiplomacyStage::Done;
                } else if offered.id == DECLINE_ID || offered.id == ACCEPT_ID {
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
                    // Counter: the answering seat builds from the terms on the table, its own
                    // side first.
                    let current = self
                        .current
                        .as_ref()
                        .ok_or_else(|| failed(&choice.player, "current bundle disappeared"))?;
                    let builder = choice.player.clone();
                    let (give, take) = if builder == self.proposer {
                        (
                            current.revision.proposer_terms.clone(),
                            current.revision.recipient_terms.clone(),
                        )
                    } else {
                        (
                            current.revision.recipient_terms.clone(),
                            current.revision.proposer_terms.clone(),
                        )
                    };
                    let other = if builder == self.proposer {
                        self.recipient.clone()
                    } else {
                        self.proposer.clone()
                    };
                    let mut draft = Draft::new(builder, other);
                    draft.give = give;
                    draft.take = take;
                    self.draft = Some(draft);
                    self.stage = DiplomacyStage::Building;
                }
            }
            DiplomacyStage::Done => unreachable!(),
        }
        Ok(())
    }
}

/// The canonical option id of a statement: its template and, where it names one, the system.
fn signal_id(statement: &SignalStatement) -> String {
    match statement {
        SignalStatement::WillVote { agenda, outcome } => {
            format!("{SIGNAL_PREFIX}will_vote|{agenda}|{outcome}")
        }
        SignalStatement::AttackIfYouActivate { system } => {
            format!("{SIGNAL_PREFIX}attack_if_you_activate|{system}")
        }
    }
}

/// A signal as the proposer sees it: the full sentence, sent to last through the next round.
fn signal_option(statement: &SignalStatement, round: u32) -> ChoiceOption {
    let until = round.saturating_add(1);
    let label = match statement {
        SignalStatement::WillVote { agenda, outcome } => {
            format!("Assurance: I will vote {outcome} on {agenda}")
        }
        SignalStatement::AttackIfYouActivate { system } => format!(
            "Warning: if you activate system {system} before round {until} ends, I will attack you"
        ),
    };
    let kind = match statement.kind() {
        ti4_model::SignalKind::Request => "request",
        ti4_model::SignalKind::Threat => "threat",
        ti4_model::SignalKind::Assurance => "assurance",
        ti4_model::SignalKind::Warning => "warning",
    };
    let statement_kind = match statement {
        SignalStatement::WillVote { .. } => "will-vote",
        SignalStatement::AttackIfYouActivate { .. } => "attack-if-activated",
    };
    ChoiceOption::labelled(signal_id(statement), SIGNAL_KIND, label)
        .with("signal_kind", kind)
        .with("signal_statement", statement_kind)
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
    // Priced before anything moves, exactly as the legacy transaction window prices a deal.
    let fair = (given.worth_to_receiver(state, ctx.content)
        - received.worth_to_receiver(state, ctx.content))
    .abs()
        <= 0.5;
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
    // Immediate transfers only is a transaction and is judged as one: a fair trade builds trust and
    // an unfair one builds nothing. A bundle with any promise in it is a deal.
    let relationship = if future {
        Some(RelationshipEvent::DealAccepted {
            a: deal.proposer.clone(),
            b: deal.recipient.clone(),
        })
    } else if fair {
        Some(RelationshipEvent::FairTransaction {
            a: deal.proposer.clone(),
            b: deal.recipient.clone(),
        })
    } else {
        None
    };
    if let Some(event) = relationship {
        apply_relationship_event(state, &event)
            .map_err(|error| failed(&PlayerId::new(""), &error.to_string()))?;
    }
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
    use std::collections::BTreeMap;
    use ti4_model::{DiplomacyState, StrategyCardId, TransferAsset};

    fn pid(id: &str) -> PlayerId {
        PlayerId::new(id)
    }

    fn table(trade_goods: i32) -> GameState {
        let mut state = GameState::new(
            &[pid("a"), pid("b")],
            &[] as &[StrategyCardId],
            BTreeMap::new(),
            None,
            0,
        );
        state.diplomacy = DiplomacyState::for_players(&state.seating_order, true);
        state.player_mut(&pid("a")).unwrap().trade_goods = trade_goods;
        state.player_mut(&pid("b")).unwrap().commodities = 2;
        state
    }

    fn scope(physical: bool) -> ContactScope {
        ContactScope {
            physical,
            ..ContactScope::default()
        }
    }

    /// Answer the window's pending choice with the option whose id is `id`.
    fn answer(window: &mut DiplomacyWindow, state: &mut GameState, id: &str) {
        let content = ti4_content::ContentStore::embedded();
        let choice = window
            .pending_choice(state, content, ti4_model::POK)
            .unwrap();
        let option = choice
            .options
            .iter()
            .find(|option| option.id == id)
            .unwrap_or_else(|| panic!("{id} not offered: {:?}", choice.ids()))
            .clone();
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
        window.resolve(state, &mut resolving, option).unwrap();
    }

    fn ids(window: &DiplomacyWindow, state: &GameState) -> Vec<String> {
        window
            .pending_choice(state, ti4_content::ContentStore::embedded(), ti4_model::POK)
            .map(|choice| choice.ids().into_iter().map(str::to_owned).collect())
            .unwrap_or_default()
    }

    #[test]
    fn a_deal_is_built_offer_first_then_ask_then_proposed() {
        let mut state = table(3);
        let mut window =
            DiplomacyWindow::open(&mut state, pid("a"), pid("b"), scope(true), vec![]).unwrap();
        answer(&mut window, &mut state, "diplomacy|now|tg");
        assert_eq!(
            ids(&window, &state),
            [
                "diplomacy|amount|1",
                "diplomacy|amount|2",
                "diplomacy|amount|3"
            ]
        );
        answer(&mut window, &mut state, "diplomacy|amount|2");
        answer(&mut window, &mut state, DONE_ID);
        // Now asking: the partner's commodities, not the builder's trade goods.
        let asking = ids(&window, &state);
        assert!(asking.contains(&"diplomacy|now|commodities".to_owned()));
        assert!(!asking.contains(&"diplomacy|now|tg".to_owned()));
        answer(&mut window, &mut state, "diplomacy|now|commodities");
        answer(&mut window, &mut state, "diplomacy|amount|2");
        answer(&mut window, &mut state, DONE_ID);
        answer(&mut window, &mut state, PROPOSE_ID);

        assert_eq!(window.stage, DiplomacyStage::Responding);
        let deal = &state.diplomacy.active_deals[&DealId(1)];
        assert_eq!(
            deal.latest().proposer_terms,
            vec![DealTerm::ImmediateTransfer(TransferAsset::TradeGoods(2))]
        );
        assert_eq!(
            deal.latest().recipient_terms,
            vec![DealTerm::ImmediateTransfer(TransferAsset::Commodities(2))]
        );
        let response = window
            .pending_choice(
                &state,
                ti4_content::ContentStore::embedded(),
                ti4_model::POK,
            )
            .unwrap();
        assert_eq!(response.player, pid("b"));
    }

    #[test]
    fn physical_items_need_a_transaction_partner_but_promises_do_not() {
        let mut state = table(3);
        let window =
            DiplomacyWindow::open(&mut state, pid("a"), pid("b"), scope(false), vec![]).unwrap();
        let offered = ids(&window, &state);
        assert!(offered.iter().all(|id| !id.starts_with("diplomacy|now|")));
        assert!(offered.iter().all(|id| !id.starts_with("diplomacy|note|")));
        assert!(offered.contains(&"diplomacy|later|tg".to_owned()));
        assert!(
            offered
                .iter()
                .any(|id| id.starts_with("diplomacy|promise|"))
        );
    }

    #[test]
    fn one_promissory_note_per_side() {
        let mut state = table(0);
        for note in ["cf:x", "ps:x"] {
            state.promissory_notes.insert(note.to_owned(), pid("a"));
        }
        let mut window =
            DiplomacyWindow::open(&mut state, pid("a"), pid("b"), scope(true), vec![]).unwrap();
        let notes: Vec<String> = ids(&window, &state)
            .into_iter()
            .filter(|id| id.starts_with("diplomacy|note|"))
            .collect();
        assert!(notes.len() >= 2, "several notes to choose from: {notes:?}");
        answer(&mut window, &mut state, &notes[0]);
        assert!(
            ids(&window, &state)
                .iter()
                .all(|id| !id.starts_with("diplomacy|note|")),
            "a second note is never offered on the same side"
        );
    }

    #[test]
    fn a_gift_can_be_proposed_and_two_counters_is_the_limit() {
        let mut state = table(3);
        let mut window =
            DiplomacyWindow::open(&mut state, pid("a"), pid("b"), scope(false), vec![]).unwrap();
        answer(&mut window, &mut state, "diplomacy|later|tg");
        answer(&mut window, &mut state, "diplomacy|amount|1");
        answer(&mut window, &mut state, DONE_ID);
        answer(&mut window, &mut state, DONE_ID);
        answer(&mut window, &mut state, PROPOSE_ID);
        // Nothing asked in return: a gift, allowed.
        assert!(
            state.diplomacy.active_deals[&DealId(1)]
                .latest()
                .recipient_terms
                .is_empty()
        );

        for counter in 0..MAX_COUNTERS {
            assert!(
                ids(&window, &state).contains(&COUNTER_ID.to_owned()),
                "counter {counter}"
            );
            answer(&mut window, &mut state, COUNTER_ID);
            // The counter starts from the terms on the table; propose them back unchanged.
            answer(&mut window, &mut state, DONE_ID);
            answer(&mut window, &mut state, DONE_ID);
            answer(&mut window, &mut state, PROPOSE_ID);
        }
        assert!(!ids(&window, &state).contains(&COUNTER_ID.to_owned()));
        assert_eq!(window.responses, MAX_COUNTERS);
        // One counter per side (operator rule): the recipient counters first, the proposer second.
        let authors: Vec<_> = state.diplomacy.active_deals[&DealId(1)]
            .revisions
            .iter()
            .map(|revision| revision.author.clone())
            .collect();
        assert_eq!(authors, [pid("a"), pid("b"), pid("a")]);
    }

    #[test]
    fn walking_away_from_a_counter_returns_to_the_offer() {
        let mut state = table(3);
        let mut window =
            DiplomacyWindow::open(&mut state, pid("a"), pid("b"), scope(false), vec![]).unwrap();
        answer(&mut window, &mut state, "diplomacy|later|tg");
        answer(&mut window, &mut state, "diplomacy|amount|2");
        answer(&mut window, &mut state, DONE_ID);
        answer(&mut window, &mut state, DONE_ID);
        answer(&mut window, &mut state, PROPOSE_ID);
        answer(&mut window, &mut state, COUNTER_ID);
        answer(&mut window, &mut state, DONE_ID);
        answer(&mut window, &mut state, DONE_ID);
        answer(&mut window, &mut state, CANCEL_ID);
        assert_eq!(window.stage, DiplomacyStage::Responding);
        assert_eq!(window.responses, 0, "an abandoned counter is not a counter");
        assert!(
            !ids(&window, &state).contains(&COUNTER_ID.to_owned()),
            "but the chance to counter is spent, or counter-and-walk-away never ends"
        );
        answer(&mut window, &mut state, ACCEPT_ID);
        assert_eq!(window.stage, DiplomacyStage::Done);
    }

    /// Adding and removing cannot cycle forever: after MAX_BUILD_STEPS operations only "done" is
    /// left, and review no longer offers "edit".
    #[test]
    fn a_revision_has_a_bounded_number_of_edits() {
        let mut state = table(9);
        let mut window =
            DiplomacyWindow::open(&mut state, pid("a"), pid("b"), scope(true), vec![]).unwrap();
        for _ in 0..crate::diplomacy::builder::MAX_BUILD_STEPS / 2 {
            answer(&mut window, &mut state, "diplomacy|now|tg");
            answer(&mut window, &mut state, "diplomacy|amount|1");
            answer(&mut window, &mut state, "diplomacy|remove|0");
        }
        // Only "done" (and, the draft being empty again, "make no offer").
        assert_eq!(
            ids(&window, &state),
            [DONE_ID.to_owned(), CANCEL_ID.to_owned()]
        );
        answer(&mut window, &mut state, DONE_ID);
        assert_eq!(
            ids(&window, &state),
            [DONE_ID.to_owned()],
            "asking is closed too"
        );
        answer(&mut window, &mut state, DONE_ID);
        assert!(!ids(&window, &state).contains(&EDIT_ID.to_owned()));
    }

    /// "Edit" from review is a step: done, done, edit cannot go round forever.
    #[test]
    fn review_and_edit_cannot_cycle_forever() {
        let mut state = table(3);
        let mut window =
            DiplomacyWindow::open(&mut state, pid("a"), pid("b"), scope(false), vec![]).unwrap();
        answer(&mut window, &mut state, "diplomacy|later|tg");
        answer(&mut window, &mut state, "diplomacy|amount|1");
        let mut edits = 0;
        loop {
            answer(&mut window, &mut state, DONE_ID);
            answer(&mut window, &mut state, DONE_ID);
            if !ids(&window, &state).contains(&EDIT_ID.to_owned()) {
                break;
            }
            answer(&mut window, &mut state, EDIT_ID);
            edits += 1;
            assert!(edits < 20, "edit never ran out");
        }
        assert_eq!(edits, 2, "at most two edits per revision");
    }

    #[test]
    fn selecting_a_signal_closes_without_opening_a_deal() {
        let mut state = table(0);
        let mut window = DiplomacyWindow::open(
            &mut state,
            pid("a"),
            pid("b"),
            scope(false),
            vec![SignalStatement::AttackIfYouActivate {
                system: ti4_model::SystemId::new("18"),
            }],
        )
        .unwrap();
        let signal = ids(&window, &state)
            .into_iter()
            .find(|id| id.starts_with(SIGNAL_PREFIX))
            .expect("the warning is offered on the opening screen");
        answer(&mut window, &mut state, &signal);
        assert_eq!(window.stage, DiplomacyStage::Done);
        assert!(state.diplomacy.active_deals.is_empty());
        assert_eq!(state.diplomacy.recent_signals.len(), 1);
    }
}
