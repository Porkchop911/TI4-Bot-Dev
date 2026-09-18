//! Plain-language presentation of structured diplomacy.
//!
//! Everything here reads the engine's own diplomacy state -- directional relationships, deals and
//! their promises, signals, and the append-only journal -- and turns it into short lines a
//! reviewer can scan. The native window, the action summaries and the tests all use these, so a
//! deal reads the same wherever it appears.

use serde::Deserialize as _;
use serde_json::Value;
use ti4_engine::diplomacy::candidates::{CandidateBundle, DealTemplate};
use ti4_model::id::PlayerId;
use ti4_model::state::GameState;
use ti4_model::{
    Deal, DealRevision, DealStatus, DealTerm, DiplomacyEvent, DiplomacyJournalEntry, PromiseStatus,
    Relationship, Signal, SignalStatement, SignalStatus, TerminalDealSummary, TransferAsset,
};

use crate::{DecisionDetail, OptionDetail};

/// A seat as the reviewer names it: `seat2 (xxcha)`.
#[must_use]
pub fn seat(state: &GameState, player: &PlayerId) -> String {
    state
        .player(player)
        .filter(|seat| !seat.faction.as_str().is_empty())
        .map_or_else(
            || player.to_string(),
            |seat| format!("{} ({})", seat.id, seat.faction),
        )
}

fn count(amount: u8, one: &str, many: &str) -> String {
    format!("{amount} {}", if amount == 1 { one } else { many })
}

/// What one transferable asset is, in words.
#[must_use]
pub fn asset_text(asset: &TransferAsset) -> String {
    match asset {
        TransferAsset::TradeGoods(n) => count(*n, "trade good", "trade goods"),
        TransferAsset::Commodities(n) => count(*n, "commodity", "commodities"),
        TransferAsset::CulturalFragments(n) => count(*n, "cultural fragment", "cultural fragments"),
        TransferAsset::HazardousFragments(n) => {
            count(*n, "hazardous fragment", "hazardous fragments")
        }
        TransferAsset::IndustrialFragments(n) => {
            count(*n, "industrial fragment", "industrial fragments")
        }
        TransferAsset::UnknownFragments(n) => count(*n, "unknown fragment", "unknown fragments"),
        TransferAsset::PromissoryNote(id) => format!("promissory note {id}"),
        TransferAsset::ActionCard(id) => format!("action card {id}"),
        TransferAsset::SecretObjective(id) => format!("secret objective {id}"),
    }
}

/// One term of a deal. With a promiser it reads as a statement (`seat1 pays 2 trade goods by the
/// end of round 3`); without one, as what the deciding seat commits to (`pay 2 trade goods ...`).
#[must_use]
pub fn term_text(state: &GameState, promiser: Option<&str>, term: &DealTerm) -> String {
    let verb = |third: &str, base: &str| {
        promiser.map_or_else(|| base.to_owned(), |who| format!("{who} {third}"))
    };
    match term {
        DealTerm::ImmediateTransfer(asset) => {
            format!("{} {} now", verb("gives", "give"), asset_text(asset))
        }
        DealTerm::FuturePayment {
            asset,
            deadline_round,
        } => format!(
            "{} {} by the end of round {deadline_round}",
            verb("pays", "pay"),
            asset_text(asset)
        ),
        DealTerm::DoNotActivate {
            system,
            deadline_round,
        } => format!(
            "{} system {system} through round {deadline_round}",
            verb("does not activate", "do not activate")
        ),
        DealTerm::DoNotAttack {
            player,
            deadline_round,
        } => format!(
            "{} {} through round {deadline_round}",
            verb("does not attack", "do not attack"),
            seat(state, player)
        ),
        DealTerm::Vote {
            agenda,
            outcome,
            deadline_round,
        } => format!(
            "{} {outcome} on {agenda} by round {deadline_round}",
            verb("votes", "vote")
        ),
        DealTerm::Attack {
            player,
            deadline_round,
        } => format!(
            "{} {} by the end of round {deadline_round}",
            verb("attacks", "attack"),
            seat(state, player)
        ),
        DealTerm::ReplenishFor {
            beneficiary,
            deadline_round,
        } => format!(
            "{} {} with the Trade primary by the end of round {deadline_round}",
            verb("replenishes", "replenish"),
            seat(state, beneficiary)
        ),
        DealTerm::UseLeaderFor {
            leader,
            beneficiary,
            deadline_round,
        } => format!(
            "{} agent {leader} for {} by the end of round {deadline_round}",
            verb("uses", "use"),
            seat(state, beneficiary)
        ),
    }
}

const fn promise_mark(status: Option<PromiseStatus>) -> &'static str {
    match status {
        Some(PromiseStatus::Pending) => "…",
        Some(PromiseStatus::Fulfilled) => "✓",
        Some(PromiseStatus::Broken) => "✗",
        Some(PromiseStatus::Expired) => "⌛",
        None => "·",
    }
}

const fn promise_word(status: PromiseStatus) -> &'static str {
    match status {
        PromiseStatus::Pending => "pending",
        PromiseStatus::Fulfilled => "kept",
        PromiseStatus::Broken => "broken",
        PromiseStatus::Expired => "expired",
    }
}

fn status_word(status: DealStatus) -> String {
    format!("{status:?}").to_lowercase()
}

/// Every term of a revision, each marked with its promise status: `✓ seat0 (sol) gives 2 trade
/// goods now`, `… seat1 (hacan) does not attack seat0 (sol) through round 3`.
#[must_use]
pub fn revision_lines(
    state: &GameState,
    proposer: &PlayerId,
    recipient: &PlayerId,
    revision: &DealRevision,
) -> Vec<String> {
    let side = |who: &PlayerId, terms: &[DealTerm], statuses: &[PromiseStatus]| {
        let name = seat(state, who);
        terms
            .iter()
            .enumerate()
            .map(|(index, term)| {
                format!(
                    "{} {}",
                    promise_mark(statuses.get(index).copied()),
                    term_text(state, Some(&name), term)
                )
            })
            .collect::<Vec<_>>()
    };
    let mut lines = side(
        proposer,
        &revision.proposer_terms,
        &revision.proposer_statuses,
    );
    lines.extend(side(
        recipient,
        &revision.recipient_terms,
        &revision.recipient_statuses,
    ));
    lines
}

/// A live deal: a headline, then its latest terms.
#[must_use]
pub fn deal_lines(state: &GameState, deal: &Deal) -> Vec<String> {
    let latest = deal.latest();
    let countered = deal.revisions.len().saturating_sub(1);
    let mut lines = vec![format!(
        "Deal #{}: {} ↔ {} · {} · offered in round {}{}",
        deal.id.0,
        seat(state, &deal.proposer),
        seat(state, &deal.recipient),
        status_word(deal.status),
        deal.created_round,
        if countered == 0 {
            String::new()
        } else {
            format!(
                " · countered {countered}×, latest by {}",
                seat(state, &latest.author)
            )
        }
    )];
    lines.extend(revision_lines(
        state,
        &deal.proposer,
        &deal.recipient,
        latest,
    ));
    lines
}

/// A deal that has ended, as history keeps it.
#[must_use]
pub fn summary_lines(state: &GameState, summary: &TerminalDealSummary) -> Vec<String> {
    let mut lines = vec![format!(
        "Deal #{}: {} ↔ {} · {} · rounds {}–{}",
        summary.id.0,
        seat(state, &summary.proposer),
        seat(state, &summary.recipient),
        status_word(summary.status),
        summary.created_round,
        summary.terminal_round
    )];
    if let Some(text) = &summary.legacy_promise {
        lines.push(format!(
            "legacy promise “{text}”{}",
            summary
                .legacy_status
                .map_or_else(String::new, |status| format!(" · {}", promise_word(status)))
        ));
    }
    if let Some(revision) = &summary.latest_revision {
        lines.extend(revision_lines(
            state,
            &summary.proposer,
            &summary.recipient,
            revision,
        ));
    }
    lines
}

/// What happened to a signal, in words.
#[must_use]
pub const fn signal_status_text(status: SignalStatus) -> &'static str {
    match status {
        SignalStatus::Open => "open",
        SignalStatus::Honoured => "honoured (the assurance was kept)",
        SignalStatus::Broken => "broken (the speaker attacked anyway)",
        SignalStatus::Heeded => "heeded",
        SignalStatus::Ignored => "ignored",
        SignalStatus::Triggered => "triggered: waiting to see whether the speaker acts",
        SignalStatus::CarriedOut => "carried out",
        SignalStatus::Bluffed => "a bluff (never acted on)",
    }
}

/// One signal as the sentence the speaker sent, and what came of it: `seat2 (xxcha) → seat4
/// (jolnar): Warning: if you activate system 18 before round 3 ends, I will attack you · heeded`.
#[must_use]
pub fn signal_text(state: &GameState, signal: &Signal) -> String {
    let until = signal.expires_round;
    let sentence = match &signal.statement {
        SignalStatement::WillVote { agenda, outcome } => {
            format!("Assurance: I will vote {outcome} on {agenda}")
        }
        SignalStatement::AttackIfYouActivate { system } => format!(
            "Warning: if you activate system {system} before round {until} ends, I will attack you"
        ),
    };
    format!(
        "{} → {}: {sentence} · {}",
        seat(state, &signal.speaker),
        seat(state, &signal.target),
        signal_status_text(signal.status)
    )
}

/// What the stance words and letters in the relationship grid mean.
pub const RELATIONSHIP_LEGEND: &str = "T trust and C cooperation run from -100 to 100; Th threat and H hostility from 0 to 100. Stance: hostile = hostility 40+ or trust -40 or lower; wary = hostility 20+, trust -20 or lower, or threat 30+; friendly = trust 20+ with hostility under 20; neutral otherwise. ⚔ attacked recently, ✗ broke a promise recently.";

/// One word for how a seat regards another, derived only from the four public values.
#[must_use]
pub const fn stance(relationship: Relationship) -> &'static str {
    if relationship.hostility >= 40 || relationship.trust <= -40 {
        "hostile"
    } else if relationship.hostility >= 20 || relationship.trust <= -20 || relationship.threat >= 30
    {
        "wary"
    } else if relationship.trust >= 20 {
        "friendly"
    } else {
        "neutral"
    }
}

/// A relationship in one short cell: `T+12 C+5 Th15 H20`.
#[must_use]
pub fn relationship_cell(relationship: Relationship) -> String {
    format!(
        "T{:+} C{:+} Th{} H{}",
        relationship.trust, relationship.cooperation, relationship.threat, relationship.hostility
    )
}

/// How each seat's view of another changed between two positions.
#[must_use]
pub fn relationship_changes(before: &GameState, after: &GameState) -> Vec<String> {
    let mut lines = Vec::new();
    for (observer, row) in &after.diplomacy.relationships {
        for (subject, now) in row {
            let was = before.diplomacy.relationship(observer, subject);
            if was == *now {
                continue;
            }
            let parts: Vec<String> = [
                ("trust", i32::from(now.trust) - i32::from(was.trust)),
                (
                    "cooperation",
                    i32::from(now.cooperation) - i32::from(was.cooperation),
                ),
                ("threat", i32::from(now.threat) - i32::from(was.threat)),
                (
                    "hostility",
                    i32::from(now.hostility) - i32::from(was.hostility),
                ),
            ]
            .into_iter()
            .filter(|(_, delta)| *delta != 0)
            .map(|(name, delta)| format!("{name} {delta:+}"))
            .collect();
            lines.push(format!(
                "{} now regards {}: {}",
                seat(after, observer),
                seat(after, subject),
                parts.join(", ")
            ));
        }
    }
    lines
}

/// The parties and latest terms of a deal as the journal knew them at `upto`.
fn deal_at(
    journal: &[DiplomacyJournalEntry],
    deal: ti4_model::DealId,
    upto: usize,
) -> Option<(&PlayerId, &PlayerId, &DealRevision)> {
    let mut found: Option<(&PlayerId, &PlayerId, &DealRevision)> = None;
    for entry in journal.iter().take(upto + 1) {
        match &entry.event {
            DiplomacyEvent::Offered {
                deal_id,
                proposer,
                recipient,
                revision,
                ..
            } if *deal_id == deal => found = Some((proposer, recipient, revision)),
            DiplomacyEvent::Countered { deal_id, revision } if *deal_id == deal => {
                if let Some((proposer, recipient, _)) = found {
                    found = Some((proposer, recipient, revision));
                }
            }
            _ => {}
        }
    }
    found
}

fn terms_joined(
    state: &GameState,
    proposer: &PlayerId,
    recipient: &PlayerId,
    revision: &DealRevision,
) -> String {
    let proposer_name = seat(state, proposer);
    let recipient_name = seat(state, recipient);
    let terms: Vec<String> = revision
        .proposer_terms
        .iter()
        .map(|term| term_text(state, Some(&proposer_name), term))
        .chain(
            revision
                .recipient_terms
                .iter()
                .map(|term| term_text(state, Some(&recipient_name), term)),
        )
        .collect();
    terms.join("; ")
}

/// What the diplomacy journal recorded between two positions, one line per event.
#[must_use]
pub fn journal_lines(before: &GameState, after: &GameState) -> Vec<String> {
    let journal = &after.diplomacy.journal;
    let start = before.diplomacy.journal.len();
    if journal.len() <= start {
        return Vec::new();
    }
    let mut lines = Vec::new();
    for (index, entry) in journal.iter().enumerate().skip(start) {
        let line = match &entry.event {
            DiplomacyEvent::Offered {
                deal_id,
                proposer,
                recipient,
                revision,
                ..
            } => Some(format!(
                "Deal #{} offered by {} to {}: {}",
                deal_id.0,
                seat(after, proposer),
                seat(after, recipient),
                terms_joined(after, proposer, recipient, revision)
            )),
            DiplomacyEvent::Countered { deal_id, revision } => deal_at(journal, *deal_id, index)
                .map(|(proposer, recipient, _)| {
                    format!(
                        "Deal #{} countered by {}: {}",
                        deal_id.0,
                        seat(after, &revision.author),
                        terms_joined(after, proposer, recipient, revision)
                    )
                }),
            DiplomacyEvent::Accepted { deal_id, actor, .. } => Some(format!(
                "Deal #{} accepted by {}",
                deal_id.0,
                seat(after, actor)
            )),
            DiplomacyEvent::Declined { deal_id, actor, .. } => Some(format!(
                "Deal #{} declined by {}",
                deal_id.0,
                seat(after, actor)
            )),
            // The accepted terms are already listed; applying their transfers adds nothing new.
            DiplomacyEvent::ImmediateApplied { .. } => None,
            DiplomacyEvent::PromiseSettled {
                deal_id,
                term_index,
                status,
            } => {
                let term =
                    deal_at(journal, *deal_id, index).and_then(
                        |(proposer, recipient, revision)| {
                            let index = usize::from(*term_index);
                            let split = revision.proposer_terms.len();
                            if index < split {
                                revision.proposer_terms.get(index).map(|term| {
                                    term_text(after, Some(&seat(after, proposer)), term)
                                })
                            } else {
                                revision.recipient_terms.get(index - split).map(|term| {
                                    term_text(after, Some(&seat(after, recipient)), term)
                                })
                            }
                        },
                    );
                Some(format!(
                    "Deal #{} promise {}: {}",
                    deal_id.0,
                    promise_word(*status),
                    term.unwrap_or_else(|| format!("term {term_index}"))
                ))
            }
            DiplomacyEvent::DealSettled { deal_id, status } => Some(format!(
                "Deal #{} ended: {}",
                deal_id.0,
                status_word(*status)
            )),
            DiplomacyEvent::SignalEmitted { signal_id } => Some(
                after
                    .diplomacy
                    .recent_signals
                    .iter()
                    .find(|signal| signal.id == *signal_id)
                    .map_or_else(
                        || format!("Signal #{} sent", signal_id.0),
                        |signal| format!("Signal: {}", signal_text(after, signal)),
                    ),
            ),
            DiplomacyEvent::SignalJudged { signal } => {
                Some(format!("Signal outcome: {}", signal_text(after, signal)))
            }
        };
        lines.extend(line);
    }
    lines
}

/// The diplomacy part of an action summary: contacts opened, offers not made, everything the
/// journal recorded, and how relationships moved.
pub(crate) fn append_action_lines(
    details: &mut Vec<String>,
    before: &GameState,
    after: &GameState,
    decisions: &[DecisionDetail],
) {
    if !after.diplomacy.enabled {
        return;
    }
    for decision in decisions {
        let Some(option) = crate::selected_option(decision) else {
            continue;
        };
        let actor = seat(after, &PlayerId::new(decision.player.as_str()));
        if option.id == ti4_engine::diplomacy::candidates::END_TALKS_ID {
            details.push(format!("{actor} ended negotiations before the vote"));
        } else if option.kind == ti4_engine::diplomacy::candidates::OPEN_KIND {
            // The option carries the target's stable seating-order index.
            let target = ti4_engine::diplomacy::candidates::contact_seat_index(&option.id)
                .and_then(|index| after.seating_order.get(index))
                .and_then(|id| after.player(id))
                .map_or_else(|| option.label.clone(), |player| seat(after, &player.id));
            details.push(format!("{actor} opened diplomatic contact with {target}"));
        } else if option.id == ti4_engine::diplomacy::window::DECLINE_ID
            && decision
                .context
                .as_ref()
                .and_then(|context| context.get("subtype"))
                .and_then(Value::as_str)
                == Some("diplomacy_offer")
        {
            details.push(format!("{actor} made no offer in that contact"));
        }
    }
    details.extend(journal_lines(before, after));
    details.extend(relationship_changes(before, after));
}

const fn template_text(template: DealTemplate) -> &'static str {
    match template {
        DealTemplate::FuturePayment => "future payment",
        DealTemplate::PayForNonAggression => "pay for non-aggression",
        DealTemplate::PayForAttack => "pay for an attack",
        DealTemplate::CommodityExchangePlusFavor => "commodity exchange plus a favour",
        DealTemplate::PayForVote => "pay for a vote",
        DealTemplate::Trade => "trade",
        DealTemplate::RefreshForCommodity => "a commodity refresh, repaid later",
        DealTemplate::PayForAgentFavour => "pay for an agent's help",
        DealTemplate::SellAgentFavour => "sell an agent's help",
        DealTemplate::NoteForNonAggression => "promissory note for non-aggression",
    }
}

/// A decision option that carries a whole bundle, read from the deciding seat's side.
///
/// Empty for every other option.
#[must_use]
pub fn option_lines(state: &GameState, option: &OptionDetail) -> Vec<String> {
    let Some(bundle) = option
        .payload
        .get("bundle")
        .and_then(|value| CandidateBundle::deserialize(value).ok())
    else {
        return Vec::new();
    };
    let actor_is_proposer = option
        .payload
        .get("actor_is_proposer")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let revision = &bundle.revision;
    let (mine, theirs) = if actor_is_proposer {
        (&revision.proposer_terms, &revision.recipient_terms)
    } else {
        (&revision.recipient_terms, &revision.proposer_terms)
    };
    let commit = |terms: &[DealTerm]| {
        if terms.is_empty() {
            "nothing".to_owned()
        } else {
            terms
                .iter()
                .map(|term| term_text(state, None, term))
                .collect::<Vec<_>>()
                .join("; ")
        }
    };
    vec![
        format!(
            "Deal: {}{}",
            template_text(bundle.template),
            if revision.number == 0 {
                String::new()
            } else {
                format!(" · counter {}", revision.number)
            }
        ),
        format!("You commit to: {}", commit(mine)),
        format!("They commit to: {}", commit(theirs)),
    ]
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use ti4_engine::diplomacy::candidates::CandidateFeatures;
    use ti4_model::DiplomacyState;

    use super::*;

    fn table() -> GameState {
        let players: Vec<PlayerId> = (0..3)
            .map(|index| PlayerId::new(format!("seat{index}")))
            .collect();
        let mut state = GameState::new(&players, &[], BTreeMap::new(), None, 1);
        state.diplomacy = DiplomacyState::for_players(&players, true);
        state
    }

    #[test]
    fn a_broken_promise_reads_as_offer_acceptance_breach_and_lost_trust() {
        let before = table();
        let mut after = before.clone();
        let proposer = PlayerId::new("seat0");
        let recipient = PlayerId::new("seat1");
        let revision = DealRevision::new(
            0,
            proposer.clone(),
            vec![],
            vec![DealTerm::FuturePayment {
                asset: TransferAsset::TradeGoods(2),
                deadline_round: 1,
            }],
            1,
        )
        .unwrap();
        let deal = after
            .diplomacy
            .create_deal(proposer, recipient.clone(), 1, revision)
            .unwrap();
        after.diplomacy.active_deals.get_mut(&deal).unwrap().status = DealStatus::Active;
        after.diplomacy.journal.push(DiplomacyJournalEntry {
            round: 1,
            event: DiplomacyEvent::Accepted {
                deal_id: deal,
                actor: recipient,
                revision: 0,
            },
        });
        ti4_engine::diplomacy::settle_deadlines(&mut after, 1).unwrap();

        let journal = journal_lines(&before, &after).join("\n");
        assert!(journal.contains("Deal #1 offered by seat0"), "{journal}");
        assert!(
            journal.contains("pays 2 trade goods by the end of round 1"),
            "{journal}"
        );
        assert!(journal.contains("Deal #1 accepted by seat1"), "{journal}");
        assert!(journal.contains("Deal #1 promise broken"), "{journal}");
        assert!(journal.contains("Deal #1 ended: broken"), "{journal}");

        let changes = relationship_changes(&before, &after).join("\n");
        assert!(changes.contains("now regards"), "{changes}");
        assert!(
            changes.contains("trust -40, threat +15, hostility +25"),
            "{changes}"
        );

        let history = summary_lines(&after, &after.diplomacy.history[0]).join("\n");
        assert!(history.contains("· broken ·"), "{history}");
        assert!(history.contains("✗"), "{history}");
    }

    #[test]
    fn a_bundle_option_reads_from_the_deciding_seats_side() {
        let state = table();
        let revision = DealRevision::new(
            0,
            PlayerId::new("seat0"),
            vec![DealTerm::ImmediateTransfer(TransferAsset::TradeGoods(2))],
            vec![DealTerm::DoNotAttack {
                player: PlayerId::new("seat0"),
                deadline_round: 1,
            }],
            1,
        )
        .unwrap();
        let bundle = CandidateBundle {
            id: "diplomacy|PayForNonAggression|x".to_owned(),
            template: DealTemplate::PayForNonAggression,
            revision,
            features: CandidateFeatures {
                immediate_value_self: 0.0,
                immediate_value_other: 0.0,
                future_value_self: 0.0,
                future_value_other: 0.0,
                target_relationship_effect: 0.0,
                objective_relevance: 0.0,
                military_relevance: 0.0,
            },
        };
        let option = OptionDetail {
            id: "diplomacy|accept".to_owned(),
            kind: "diplomacy_response".to_owned(),
            label: "Accept".to_owned(),
            score: None,
            probability: None,
            features: vec![],
            payload: BTreeMap::from([
                ("bundle".to_owned(), serde_json::to_value(&bundle).unwrap()),
                ("actor_is_proposer".to_owned(), Value::Bool(false)),
            ]),
            preview: None,
        };

        let lines = option_lines(&state, &option).join("\n");
        assert!(lines.contains("Deal: pay for non-aggression"), "{lines}");
        assert!(
            lines.contains("You commit to: do not attack seat0"),
            "{lines}"
        );
        assert!(
            lines.contains("They commit to: give 2 trade goods now"),
            "{lines}"
        );
    }

    #[test]
    fn an_agent_favour_reads_as_what_the_promiser_will_do() {
        let state = table();
        let agent = term_text(
            &state,
            None,
            &DealTerm::UseLeaderFor {
                leader: "hacanagent".to_owned(),
                beneficiary: PlayerId::new("seat1"),
                deadline_round: 2,
            },
        );
        assert_eq!(
            agent,
            "use agent hacanagent for seat1 (generic) by the end of round 2"
        );
    }

    #[test]
    fn a_signal_reads_as_the_sentence_sent_and_what_came_of_it() {
        let state = table();
        let signal = Signal {
            id: ti4_model::SignalId(4),
            speaker: PlayerId::new("seat2"),
            target: PlayerId::new("seat1"),
            kind: ti4_model::SignalKind::Warning,
            statement: SignalStatement::AttackIfYouActivate {
                system: ti4_model::SystemId::new("18"),
            },
            status: SignalStatus::Triggered,
            created_round: 1,
            expires_round: 2,
        };
        let text = signal_text(&state, &signal);
        assert!(text.starts_with("seat2"), "{text}");
        assert!(
            text.contains(
                "Warning: if you activate system 18 before round 2 ends, I will attack you"
            ),
            "{text}"
        );
        assert!(
            text.ends_with("triggered: waiting to see whether the speaker acts"),
            "{text}"
        );
    }
}
