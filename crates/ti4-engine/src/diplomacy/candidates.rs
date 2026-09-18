//! Deterministic, bounded whole-bundle generation.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use ti4_content::{ContentStore, galaxy::Galaxy};
use ti4_model::{DealRevision, DealTerm, GameState, PlayerId, TransferAsset};

/// Room for the five promise templates (four variants each) plus the trade bundles below.
pub const MAX_INITIAL_CANDIDATES: usize = 36;
/// Trades are the legacy transaction shapes, capped on their own so they cannot crowd out promises.
pub const MAX_TRADE_CANDIDATES: usize = 12;
pub const MAX_COUNTER_CANDIDATES: usize = 8;
pub const MAX_SIGNAL_CANDIDATES: usize = 6;
pub const OPEN_KIND: &str = "open_diplomacy";
/// The option that ends a seat's negotiation before an agenda vote.
pub const END_TALKS_ID: &str = "diplomacy|end_talks";
const OPEN_PREFIX: &str = "component|diplomacy|seat|";
pub const PAYMENT_KIND: &str = "diplomacy_fulfill_payment";
const PAYMENT_PREFIX: &str = "component|diplomacy-payment|";

#[must_use]
pub fn available_contacts(state: &GameState, actor: &PlayerId) -> Vec<crate::ChoiceOption> {
    if !state.diplomacy.enabled {
        return Vec::new();
    }
    let used = state.diplomacy.initiations_this_turn.get(actor);
    state
        .seating_order
        .iter()
        .enumerate()
        .filter(|(_, target)| *target != actor && used.is_none_or(|used| !used.contains(*target)))
        .filter_map(|(seat_index, target)| {
            let faction = state.player(target)?.faction.as_str();
            Some(crate::ChoiceOption::labelled(
                format!("{OPEN_PREFIX}{seat_index}"),
                OPEN_KIND,
                format!("open diplomatic contact with {faction}"),
            ))
        })
        .collect()
}

#[must_use]
pub fn contact_seat_index(option_id: &str) -> Option<usize> {
    option_id.strip_prefix(OPEN_PREFIX)?.parse().ok()
}

#[must_use]
pub fn contact_target(state: &GameState, option: &crate::ChoiceOption) -> Option<PlayerId> {
    state
        .seating_order
        .get(contact_seat_index(&option.id)?)
        .cloned()
}

#[must_use]
pub fn payment_actions(
    state: &GameState,
    content: &ContentStore,
    galaxy: &Galaxy,
    actor: &PlayerId,
) -> Vec<crate::ChoiceOption> {
    if !state.diplomacy.enabled {
        return Vec::new();
    }
    let mut options = Vec::new();
    for (id, deal) in &state.diplomacy.active_deals {
        if deal.status != ti4_model::DealStatus::Active {
            continue;
        }
        let revision = deal.latest();
        let proposer_count = revision.proposer_terms.len();
        for (side_actor, beneficiary, terms, statuses, offset) in [
            (
                &deal.proposer,
                &deal.recipient,
                &revision.proposer_terms,
                &revision.proposer_statuses,
                0,
            ),
            (
                &deal.recipient,
                &deal.proposer,
                &revision.recipient_terms,
                &revision.recipient_statuses,
                proposer_count,
            ),
        ] {
            if side_actor != actor {
                continue;
            }
            if state.transacted_with(actor).contains(beneficiary)
                || state
                    .diplomacy
                    .initiations_this_turn
                    .get(actor)
                    .is_some_and(|used| used.contains(beneficiary))
            {
                continue;
            }
            for (index, (term, status)) in terms.iter().zip(statuses).enumerate() {
                if *status != ti4_model::PromiseStatus::Pending {
                    continue;
                }
                let DealTerm::FuturePayment { asset, .. } = term else {
                    continue;
                };
                let Ok(given) = super::transfers::one_asset(asset) else {
                    continue;
                };
                let offer = crate::transactions::Offer {
                    proposer: actor.clone(),
                    partner: beneficiary.clone(),
                    given,
                    received: crate::transactions::Terms::default(),
                };
                if crate::transactions::why_illegal(state, content, galaxy, &offer).is_none() {
                    let global = offset + index;
                    options.push(crate::ChoiceOption::labelled(
                        format!("{PAYMENT_PREFIX}{}|{global}", id.0),
                        PAYMENT_KIND,
                        format!("fulfill deal {} payment to {beneficiary}", id.0),
                    ));
                }
            }
        }
    }
    options
}

#[must_use]
pub fn payment_action(option: &crate::ChoiceOption) -> Option<(ti4_model::DealId, usize)> {
    let rest = option.id.strip_prefix(PAYMENT_PREFIX)?;
    let (deal, term) = rest.split_once('|')?;
    Some((ti4_model::DealId(deal.parse().ok()?), term.parse().ok()?))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DealTemplate {
    FuturePayment,
    PayForNonAggression,
    PayForAttack,
    CommodityExchangePlusFavor,
    PayForVote,
    /// A transaction: immediate transfers only, one of the shapes the legacy window offers.
    Trade,
    /// The Trade primary's free refresh, against a commodity paid back later.
    RefreshForCommodity,
    /// Trade goods now for the recipient's agent used on the proposer's behalf.
    PayForAgentFavour,
    /// The proposer's agent used on the recipient's behalf, for trade goods now.
    SellAgentFavour,
    /// One of the proposer's own notes now, for the recipient's non-aggression.
    NoteForNonAggression,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CandidateFeatures {
    pub immediate_value_self: f32,
    pub immediate_value_other: f32,
    pub future_value_self: f32,
    pub future_value_other: f32,
    pub target_relationship_effect: f32,
    pub objective_relevance: f32,
    pub military_relevance: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CandidateBundle {
    pub id: String,
    pub template: DealTemplate,
    pub revision: DealRevision,
    pub features: CandidateFeatures,
}

pub struct CandidateContext<'a> {
    pub state: &'a GameState,
    pub content: &'a ContentStore,
    pub galaxy: &'a Galaxy,
    pub proposer: &'a PlayerId,
    pub recipient: &'a PlayerId,
    /// The agenda under negotiation, when the contact is held before its vote.
    pub agenda: Option<&'a str>,
}

#[must_use]
#[expect(
    clippy::too_many_lines,
    reason = "six bounded templates are generated in their fixed canonical order"
)]
pub fn generate_initial_candidates(ctx: &CandidateContext<'_>) -> Vec<CandidateBundle> {
    if !ctx.state.diplomacy.enabled || ctx.proposer == ctx.recipient {
        return Vec::new();
    }
    let proposer_goods = ctx
        .state
        .player(ctx.proposer)
        .map_or(0, |p| bounded_holding(p.trade_goods));
    let recipient_goods = ctx
        .state
        .player(ctx.recipient)
        .map_or(0, |p| bounded_holding(p.trade_goods));
    let mut out = Vec::new();
    // Credit rather than a beg: goods now against more of them next round. This template used to
    // ask one side to pay later and offer nothing in return.
    let mut variants = Vec::new();
    for amount in amounts(proposer_goods) {
        push(
            &mut variants,
            DealTemplate::FuturePayment,
            ctx.proposer.clone(),
            vec![DealTerm::ImmediateTransfer(TransferAsset::TradeGoods(
                amount,
            ))],
            vec![DealTerm::FuturePayment {
                asset: TransferAsset::TradeGoods(amount.saturating_add(1)),
                deadline_round: ctx.state.round.saturating_add(1),
            }],
            ctx.state.round,
        );
    }
    append_template(&mut out, variants);
    let mut variants = Vec::new();
    for amount in amounts(proposer_goods) {
        for deadline in [ctx.state.round, ctx.state.round.saturating_add(1)] {
            push(
                &mut variants,
                DealTemplate::PayForNonAggression,
                ctx.proposer.clone(),
                vec![DealTerm::ImmediateTransfer(TransferAsset::TradeGoods(
                    amount,
                ))],
                vec![DealTerm::DoNotAttack {
                    player: ctx.proposer.clone(),
                    deadline_round: deadline,
                }],
                ctx.state.round,
            );
        }
    }
    append_template(&mut out, variants);

    let mut variants = Vec::new();
    for target in relevant_attack_targets(ctx).into_iter().take(2) {
        for amount in amounts(proposer_goods) {
            for deadline in [ctx.state.round, ctx.state.round.saturating_add(1)] {
                push(
                    &mut variants,
                    DealTemplate::PayForAttack,
                    ctx.proposer.clone(),
                    vec![DealTerm::ImmediateTransfer(TransferAsset::TradeGoods(
                        amount,
                    ))],
                    vec![DealTerm::Attack {
                        player: target.clone(),
                        deadline_round: deadline,
                    }],
                    ctx.state.round,
                );
            }
        }
    }
    append_template(&mut out, variants);

    let proposer_commodities = ctx
        .state
        .player(ctx.proposer)
        .map_or(0, |p| bounded_holding(p.commodities));
    let recipient_commodities = ctx
        .state
        .player(ctx.recipient)
        .map_or(0, |p| bounded_holding(p.commodities));
    let mut variants = Vec::new();
    for amount in amounts(proposer_commodities.min(recipient_commodities)) {
        push(
            &mut variants,
            DealTemplate::CommodityExchangePlusFavor,
            ctx.proposer.clone(),
            vec![DealTerm::ImmediateTransfer(TransferAsset::Commodities(
                amount,
            ))],
            vec![
                DealTerm::ImmediateTransfer(TransferAsset::Commodities(amount)),
                DealTerm::FuturePayment {
                    asset: TransferAsset::TradeGoods(1),
                    deadline_round: ctx.state.round.saturating_add(1),
                },
            ],
            ctx.state.round,
        );
    }
    append_template(&mut out, variants);
    // With diplomacy on, a contact with a transaction partner is also the table's transaction
    // (94): the legacy window is not offered, so every shape it would have offered arrives here.
    out.extend(trade_bundles(ctx));
    out.extend(favour_bundles(ctx, proposer_goods, recipient_goods));
    out.extend(vote_bundles(ctx, proposer_goods));
    for candidate in &mut out {
        enrich_candidate_features(ctx, candidate);
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out.dedup_by(|a, b| a.id == b.id);
    out.retain(|candidate| {
        // Nothing for nothing: a contact offers deals, not gifts, so both sides must hand
        // something over or promise something. The legacy transaction window still offers gifts
        // with diplomacy off, where a gift is a legal transaction (94.3).
        !candidate.revision.proposer_terms.is_empty()
            && !candidate.revision.recipient_terms.is_empty()
            && immediate_components_are_legal(ctx, candidate)
            && !duplicates_active_obligation(ctx, candidate)
    });
    cap_across_templates(out, MAX_INITIAL_CANDIDATES)
}

/// Keep at most `limit` bundles, taking them template by template in turns.
///
/// A plain truncation of the id-sorted list cut whole templates by the alphabetical order of
/// their names, so the last ones (trades, agent favours) vanished once the table grew. The result
/// stays sorted by id.
fn cap_across_templates(bundles: Vec<CandidateBundle>, limit: usize) -> Vec<CandidateBundle> {
    if bundles.len() <= limit {
        return bundles;
    }
    let mut groups: std::collections::BTreeMap<DealTemplate, std::collections::VecDeque<_>> =
        std::collections::BTreeMap::new();
    for candidate in bundles {
        groups
            .entry(candidate.template)
            .or_default()
            .push_back(candidate);
    }
    let mut kept = Vec::with_capacity(limit);
    while kept.len() < limit && groups.values().any(|group| !group.is_empty()) {
        for group in groups.values_mut() {
            if kept.len() == limit {
                break;
            }
            if let Some(candidate) = group.pop_front() {
                kept.push(candidate);
            }
        }
    }
    kept.sort_by(|a, b| a.id.cmp(&b.id));
    kept
}

/// Trade goods now for the recipient's votes on the agenda under negotiation.
fn vote_bundles(ctx: &CandidateContext<'_>, proposer_goods: u8) -> Vec<CandidateBundle> {
    let Some(agenda) = ctx.agenda else {
        return Vec::new();
    };
    // A seat barred from this vote (a prediction, Political Secret) has no votes to sell.
    if ctx.state.agenda_predictions.contains_key(ctx.recipient) {
        return Vec::new();
    }
    let mut variants = Vec::new();
    for outcome in ctx.state.agenda_choices.iter().take(2) {
        for amount in amounts(proposer_goods).into_iter().take(2) {
            push(
                &mut variants,
                DealTemplate::PayForVote,
                ctx.proposer.clone(),
                vec![DealTerm::ImmediateTransfer(TransferAsset::TradeGoods(
                    amount,
                ))],
                vec![DealTerm::Vote {
                    agenda: agenda.to_owned(),
                    outcome: outcome.clone(),
                    deadline_round: ctx.state.round,
                }],
                ctx.state.round,
            );
        }
    }
    let mut out = Vec::new();
    append_template(&mut out, variants);
    out
}

/// Agents whose use can be promised to another seat, and how the engine judges that use.
const FAVOUR_AGENTS: [&str; 2] = ["hacanagent", "l1z1xagent"];

/// Components that are not transfers: an agent used for someone, a secondary left alone, a note
/// handed over for restraint or lent against trade goods.
#[expect(
    clippy::too_many_lines,
    reason = "five bounded favour templates are generated in their fixed canonical order"
)]
fn favour_bundles(
    ctx: &CandidateContext<'_>,
    proposer_goods: u8,
    recipient_goods: u8,
) -> Vec<CandidateBundle> {
    let round = ctx.state.round;
    let mut out = Vec::new();
    let ready_agents = |player: &PlayerId| -> Vec<String> {
        ctx.state.player(player).map_or_else(Vec::new, |seat| {
            seat.leaders
                .iter()
                .filter(|(leader, status)| {
                    FAVOUR_AGENTS.contains(&leader.as_str())
                        && **status == ti4_model::state::LeaderStatus::Readied
                })
                .map(|(leader, _)| leader.as_str().to_owned())
                .collect()
        })
    };

    let mut variants = Vec::new();
    for leader in ready_agents(ctx.recipient) {
        for amount in amounts(proposer_goods).into_iter().take(2) {
            push(
                &mut variants,
                DealTemplate::PayForAgentFavour,
                ctx.proposer.clone(),
                vec![DealTerm::ImmediateTransfer(TransferAsset::TradeGoods(
                    amount,
                ))],
                vec![DealTerm::UseLeaderFor {
                    leader: leader.clone(),
                    beneficiary: ctx.proposer.clone(),
                    deadline_round: round,
                }],
                round,
            );
        }
    }
    append_template(&mut out, variants);

    let mut variants = Vec::new();
    for leader in ready_agents(ctx.proposer) {
        for amount in amounts(recipient_goods).into_iter().take(2) {
            push(
                &mut variants,
                DealTemplate::SellAgentFavour,
                ctx.proposer.clone(),
                vec![DealTerm::UseLeaderFor {
                    leader: leader.clone(),
                    beneficiary: ctx.recipient.clone(),
                    deadline_round: round,
                }],
                vec![DealTerm::ImmediateTransfer(TransferAsset::TradeGoods(
                    amount,
                ))],
                round,
            );
        }
    }
    append_template(&mut out, variants);

    // The Trade primary refreshes other seats at its holder's choice. Unplayed, that is worth
    // selling: the refresh, kept only when the primary actually names that seat, against a
    // commodity paid back later.
    let holds_unplayed_trade = ctx.state.player(ctx.proposer).is_some_and(|seat| {
        seat.strategy_cards.iter().any(|card| {
            !seat.exhausted_strategy_cards.contains(card)
                && crate::strategy_cards::card_name(ctx.content, card.as_str())
                    .is_some_and(|name| name == "Trade")
        })
    });
    let mut variants = Vec::new();
    if holds_unplayed_trade {
        push(
            &mut variants,
            DealTemplate::RefreshForCommodity,
            ctx.proposer.clone(),
            vec![DealTerm::ReplenishFor {
                beneficiary: ctx.recipient.clone(),
                deadline_round: round.saturating_add(1),
            }],
            vec![DealTerm::FuturePayment {
                asset: TransferAsset::Commodities(1),
                deadline_round: round.saturating_add(1),
            }],
            round,
        );
    }
    append_template(&mut out, variants);

    // The proposer's own notes, still in hand: those are the ones it can trade away.
    let own_name = crate::promissory::faction_name(ctx.state, ctx.proposer);
    let own_notes: Vec<String> =
        crate::promissory::available_notes(ctx.state, ctx.content, ctx.proposer)
            .into_iter()
            .filter(|note| crate::promissory::owner_of(note).as_deref() == Some(own_name.as_str()))
            .take(3)
            .collect();
    let mut variants = Vec::new();
    for note in &own_notes {
        push(
            &mut variants,
            DealTemplate::NoteForNonAggression,
            ctx.proposer.clone(),
            vec![DealTerm::ImmediateTransfer(TransferAsset::PromissoryNote(
                note.clone(),
            ))],
            vec![DealTerm::DoNotAttack {
                player: ctx.proposer.clone(),
                deadline_round: round.saturating_add(1),
            }],
            round,
        );
    }
    append_template(&mut out, variants);

    out
}

/// Whether a revision moves trade goods in both directions, which is never a deal.
///
/// The legacy window enumerates every `give 0..3` against `want 0..3` in trade goods. Equal amounts
/// are a literal no-op ("1 trade good for 1 trade good"), and unequal ones are a one-way gift with
/// extra steps -- neither belongs in a contact.
///
/// Only trade goods. A symmetric swap of anything else is a real deal: a commodity becomes a trade
/// good when it changes hands (21.5), so swapping commodities leaves both sides better off, and
/// swapping Support for the Throne, Ceasefires or Political Secrets trades the cards' effects
/// rather than a currency. Those shapes remain in the legacy window for diplomacy-off games too.
fn is_null_swap(revision: &DealRevision) -> bool {
    let only_goods = |terms: &[DealTerm]| {
        !terms.is_empty()
            && terms.iter().all(|term| {
                matches!(
                    term,
                    DealTerm::ImmediateTransfer(TransferAsset::TradeGoods(_))
                )
            })
    };
    only_goods(&revision.proposer_terms) && only_goods(&revision.recipient_terms)
}

/// Every legacy transaction shape between two partners, as immediate-transfer bundles.
///
/// Built from `transactions::offer_options` through `offer_from`, so notes (the Support swap, note
/// sales and gifts), commodities, trade goods, Arbiters action cards and Black Market goods trade
/// under exactly the legality and pricing the transaction window uses. Only partners who have not
/// transacted this turn trade, and only legal shapes are kept, in the legacy offer order.
fn trade_bundles(ctx: &CandidateContext<'_>) -> Vec<CandidateBundle> {
    let partners = crate::transactions::partners(ctx.state, ctx.content, ctx.galaxy, ctx.proposer);
    // During the agenda phase every other seat is a transaction partner (94).
    let partner =
        ctx.state.phase == ti4_model::state::Phase::Agenda || partners.contains(ctx.recipient);
    if !partner
        || ctx
            .state
            .transacted_with(ctx.proposer)
            .contains(ctx.recipient)
    {
        return Vec::new();
    }
    crate::transactions::offer_options(ctx.state, ctx.content, ctx.proposer, ctx.recipient)
        .iter()
        .filter_map(|option| {
            crate::transactions::offer_from(ctx.state, &option.id, ctx.proposer, ctx.recipient)
        })
        .filter_map(|offer| {
            let given = super::transfers::assets_of(&offer.given)?;
            let received = super::transfers::assets_of(&offer.received)?;
            let mut proposer_terms: Vec<DealTerm> =
                given.into_iter().map(DealTerm::ImmediateTransfer).collect();
            let mut recipient_terms: Vec<DealTerm> = received
                .into_iter()
                .map(DealTerm::ImmediateTransfer)
                .collect();
            // Nothing for nothing. The legacy window offers one-sided shapes -- a note nobody can
            // pay for, a commodity gift -- and those are legal transactions with diplomacy off.
            // Inside a contact the empty side owes a commitment instead, so the deal still happens
            // and somebody is on the hook for it.
            let owed = DealTerm::FuturePayment {
                asset: TransferAsset::Commodities(1),
                deadline_round: ctx.state.round.saturating_add(1),
            };
            if proposer_terms.is_empty() {
                proposer_terms.push(owed);
            } else if recipient_terms.is_empty() {
                recipient_terms.push(owed);
            }
            DealRevision::new(
                0,
                ctx.proposer.clone(),
                proposer_terms,
                recipient_terms,
                ctx.state.round,
            )
            .ok()
        })
        .map(|revision| bundle(DealTemplate::Trade, revision))
        .filter(|candidate| !is_null_swap(&candidate.revision))
        .filter(|candidate| immediate_components_are_legal(ctx, candidate))
        .take(MAX_TRADE_CANDIDATES)
        .collect()
}

fn immediate_components_are_legal(ctx: &CandidateContext<'_>, candidate: &CandidateBundle) -> bool {
    let Ok(given) = super::transfers::immediate_terms(&candidate.revision.proposer_terms) else {
        return false;
    };
    let Ok(received) = super::transfers::immediate_terms(&candidate.revision.recipient_terms)
    else {
        return false;
    };
    if given.is_empty() && received.is_empty() {
        return true;
    }
    crate::transactions::why_illegal(
        ctx.state,
        ctx.content,
        ctx.galaxy,
        &crate::transactions::Offer {
            proposer: ctx.proposer.clone(),
            partner: ctx.recipient.clone(),
            given,
            received,
        },
    )
    .is_none()
}

fn append_template(out: &mut Vec<CandidateBundle>, mut variants: Vec<CandidateBundle>) {
    variants.sort_by(|a, b| a.id.cmp(&b.id));
    variants.dedup_by(|a, b| a.id == b.id);
    variants.truncate(4);
    out.extend(variants);
}

fn relevant_attack_targets(ctx: &CandidateContext<'_>) -> Vec<PlayerId> {
    let recipient_presence = crate::transactions::presence(ctx.state, ctx.recipient);
    ctx.state
        .seating_order
        .iter()
        .filter(|target| *target != ctx.proposer && *target != ctx.recipient)
        .filter(|target| {
            let target_presence = crate::transactions::presence(ctx.state, target);
            recipient_presence
                .intersection(&target_presence)
                .next()
                .is_some()
                || recipient_presence.iter().any(|system| {
                    ctx.galaxy
                        .adjacent(system.as_str())
                        .into_iter()
                        .any(|adjacent| {
                            target_presence.contains(&ti4_model::SystemId::new(adjacent))
                        })
                })
        })
        .cloned()
        .collect()
}

/// The concrete signals a proposer may send the recipient: bounded, and only where they mean
/// something.
///
/// Any seat may assure another that it will not attack. Requests, threats and warnings are offered
/// only between seats whose forces share or border a system, and the systems they name are the
/// proposer's systems that are shared with or next to the recipient's -- asking someone to stay
/// out of a system nobody contests says nothing. At most six, in a fixed order.
#[must_use]
pub fn generate_signal_statements(ctx: &CandidateContext<'_>) -> Vec<ti4_model::SignalStatement> {
    use ti4_model::SignalStatement;

    if !ctx.state.diplomacy.enabled || ctx.proposer == ctx.recipient {
        return Vec::new();
    }
    // A vote assurance exists only where an agenda is on the table: the talks before its vote.
    let mut statements: Vec<SignalStatement> =
        ctx.agenda
            .into_iter()
            .flat_map(|agenda| {
                ctx.state.agenda_choices.iter().take(2).map(move |outcome| {
                    SignalStatement::WillVote {
                        agenda: agenda.to_owned(),
                        outcome: outcome.clone(),
                    }
                })
            })
            .collect();
    let recipient_presence = crate::transactions::presence(ctx.state, ctx.recipient);
    let contested: Vec<ti4_model::SystemId> =
        crate::transactions::presence(ctx.state, ctx.proposer)
            .into_iter()
            .filter(|system| {
                recipient_presence.contains(system)
                    || ctx
                        .galaxy
                        .adjacent(system.as_str())
                        .into_iter()
                        .any(|adjacent| {
                            recipient_presence.contains(&ti4_model::SystemId::new(adjacent))
                        })
            })
            .collect();
    if contested.is_empty() {
        return statements;
    }
    // One statement, about one system both seats can reach: everything else was said to every
    // seat every round and never acted on.
    statements.push(SignalStatement::AttackIfYouActivate {
        system: contested[0].clone(),
    });
    statements.truncate(MAX_SIGNAL_CANDIDATES);
    statements
}

fn duplicates_active_obligation(ctx: &CandidateContext<'_>, candidate: &CandidateBundle) -> bool {
    let proposed = candidate
        .revision
        .proposer_terms
        .iter()
        .filter(|term| !term.is_immediate())
        .map(|term| (ctx.proposer, ctx.recipient, term))
        .chain(
            candidate
                .revision
                .recipient_terms
                .iter()
                .filter(|term| !term.is_immediate())
                .map(|term| (ctx.recipient, ctx.proposer, term)),
        );
    proposed.into_iter().any(|(promiser, beneficiary, term)| {
        ctx.state.diplomacy.active_deals.values().any(|deal| {
            if deal.status != ti4_model::DealStatus::Active {
                return false;
            }
            let revision = deal.latest();
            let existing = if &deal.proposer == promiser && &deal.recipient == beneficiary {
                Some(&revision.proposer_terms)
            } else if &deal.recipient == promiser && &deal.proposer == beneficiary {
                Some(&revision.recipient_terms)
            } else {
                None
            };
            existing.is_some_and(|terms| terms.iter().any(|other| same_subject(term, other)))
        })
    })
}

fn same_subject(a: &DealTerm, b: &DealTerm) -> bool {
    match (a, b) {
        (DealTerm::FuturePayment { asset: a, .. }, DealTerm::FuturePayment { asset: b, .. }) => {
            std::mem::discriminant(a) == std::mem::discriminant(b)
        }
        (DealTerm::DoNotActivate { system: a, .. }, DealTerm::DoNotActivate { system: b, .. }) => {
            a == b
        }
        (DealTerm::DoNotAttack { player: a, .. }, DealTerm::DoNotAttack { player: b, .. })
        | (DealTerm::Attack { player: a, .. }, DealTerm::Attack { player: b, .. }) => a == b,
        (
            DealTerm::Vote {
                agenda: aa,
                outcome: ao,
                ..
            },
            DealTerm::Vote {
                agenda: ba,
                outcome: bo,
                ..
            },
        ) => aa == ba && ao == bo,
        (DealTerm::UseLeaderFor { leader: a, .. }, DealTerm::UseLeaderFor { leader: b, .. }) => {
            a == b
        }
        (
            DealTerm::ReplenishFor { beneficiary: a, .. },
            DealTerm::ReplenishFor { beneficiary: b, .. },
        ) => a == b,
        _ => false,
    }
}

/// Counter bundles to `current`, a deal between `proposer` and `recipient`.
///
/// A counter keeps the parties and moves one amount or one deadline, so the part of transaction
/// legality it can newly break is holdings: a bundle whose immediate terms ask a side for more
/// than it holds is never offered, because accepting it could not resolve.
#[must_use]
pub fn generate_counter_candidates(
    state: &GameState,
    proposer: &PlayerId,
    recipient: &PlayerId,
    current: &CandidateBundle,
    author: &PlayerId,
    round: u32,
) -> Vec<CandidateBundle> {
    let mut variants = Vec::new();
    for delta in [-1_i16, 1] {
        let mut revision = current.revision.clone();
        revision.number = revision.number.saturating_add(1);
        revision.author = author.clone();
        let mut changed = false;
        for term in revision
            .proposer_terms
            .iter_mut()
            .chain(&mut revision.recipient_terms)
        {
            if let DealTerm::ImmediateTransfer(TransferAsset::TradeGoods(amount))
            | DealTerm::FuturePayment {
                asset: TransferAsset::TradeGoods(amount),
                ..
            } = term
            {
                let next = i16::from(*amount) + delta;
                if (1..=3).contains(&next) {
                    *amount = u8::try_from(next).unwrap_or(1);
                    changed = true;
                    break;
                }
            }
        }
        if changed {
            revision.proposer_statuses = revision
                .proposer_terms
                .iter()
                .map(|t| {
                    if t.is_immediate() {
                        ti4_model::PromiseStatus::Fulfilled
                    } else {
                        ti4_model::PromiseStatus::Pending
                    }
                })
                .collect();
            revision.recipient_statuses = revision
                .recipient_terms
                .iter()
                .map(|t| {
                    if t.is_immediate() {
                        ti4_model::PromiseStatus::Fulfilled
                    } else {
                        ti4_model::PromiseStatus::Pending
                    }
                })
                .collect();
            variants.push(counter_bundle(current, revision));
        }
    }
    let mut expiry = current.revision.clone();
    expiry.number = expiry.number.saturating_add(1);
    expiry.author = author.clone();
    let mut changed = false;
    for term in expiry
        .proposer_terms
        .iter_mut()
        .chain(&mut expiry.recipient_terms)
    {
        let Some(deadline) = term.deadline_round() else {
            continue;
        };
        let replacement = if deadline == round {
            round.saturating_add(1)
        } else {
            round
        };
        match term {
            DealTerm::FuturePayment { deadline_round, .. }
            | DealTerm::DoNotActivate { deadline_round, .. }
            | DealTerm::DoNotAttack { deadline_round, .. }
            | DealTerm::Vote { deadline_round, .. }
            | DealTerm::Attack { deadline_round, .. }
            | DealTerm::UseLeaderFor { deadline_round, .. }
            | DealTerm::ReplenishFor { deadline_round, .. } => *deadline_round = replacement,
            DealTerm::ImmediateTransfer(_) => {}
        }
        changed = true;
        break;
    }
    if changed {
        variants.push(counter_bundle(current, expiry));
    }
    // A counter edits amounts, so it can turn a real deal into goods for goods even when the
    // bundle it answers was not one. Same rule as the opening offer.
    variants.retain(|candidate| {
        !is_null_swap(&candidate.revision)
            && immediates_affordable(state, proposer, recipient, &candidate.revision)
    });
    variants.sort_by(|a, b| a.id.cmp(&b.id));
    variants.dedup_by(|a, b| a.id == b.id);
    variants.truncate(MAX_COUNTER_CANDIDATES);
    variants
}

/// Whether each side still holds the trade goods and commodities its immediate terms hand over.
fn immediates_affordable(
    state: &GameState,
    proposer: &PlayerId,
    recipient: &PlayerId,
    revision: &DealRevision,
) -> bool {
    [
        (proposer, &revision.proposer_terms),
        (recipient, &revision.recipient_terms),
    ]
    .into_iter()
    .all(|(player, terms)| {
        let Ok(given) = super::transfers::immediate_terms(terms) else {
            return false;
        };
        state.player(player).is_some_and(|seat| {
            seat.trade_goods >= given.trade_goods && seat.commodities >= given.commodities
        })
    })
}

fn amounts(available: u8) -> Vec<u8> {
    let mut set = BTreeSet::new();
    if available > 0 {
        set.insert(1);
        set.insert(available.min(2));
        set.insert(available.min(3));
    }
    set.into_iter().collect()
}

fn push(
    out: &mut Vec<CandidateBundle>,
    template: DealTemplate,
    author: PlayerId,
    proposer_terms: Vec<DealTerm>,
    recipient_terms: Vec<DealTerm>,
    round: u32,
) {
    if let Ok(revision) = DealRevision::new(0, author, proposer_terms, recipient_terms, round) {
        out.push(bundle(template, revision));
    }
}

fn bundle(template: DealTemplate, revision: DealRevision) -> CandidateBundle {
    let encoded = serde_json::to_string(&(
        &template,
        &revision.proposer_terms,
        &revision.recipient_terms,
    ))
    .expect("serializable canonical deal");
    let id = format!(
        "diplomacy|{:?}|{}",
        template,
        stable_hex(encoded.as_bytes())
    );
    let (immediate_self, future_self) = values(&revision.proposer_terms);
    let (immediate_other, future_other) = values(&revision.recipient_terms);
    CandidateBundle {
        id,
        template,
        revision,
        features: CandidateFeatures {
            immediate_value_self: immediate_self,
            immediate_value_other: immediate_other,
            future_value_self: future_self,
            future_value_other: future_other,
            target_relationship_effect: 0.0,
            objective_relevance: 0.0,
            military_relevance: 0.0,
        },
    }
}

fn counter_bundle(current: &CandidateBundle, revision: DealRevision) -> CandidateBundle {
    let mut candidate = bundle(current.template, revision);
    candidate.features.target_relationship_effect = current.features.target_relationship_effect;
    candidate.features.objective_relevance = current.features.objective_relevance;
    candidate.features.military_relevance = current.features.military_relevance;
    candidate
}

fn enrich_candidate_features(ctx: &CandidateContext<'_>, candidate: &mut CandidateBundle) {
    let target = candidate
        .revision
        .proposer_terms
        .iter()
        .chain(&candidate.revision.recipient_terms)
        .find_map(|term| match term {
            DealTerm::Attack { player, .. } => Some(player),
            _ => None,
        });
    let Some(target) = target else { return };
    let relationship = ctx.state.diplomacy.relationship(ctx.recipient, target);
    candidate.features.target_relationship_effect = (f32::from(relationship.threat)
        + f32::from(relationship.hostility)
        - f32::from(relationship.trust)
        - f32::from(relationship.cooperation))
        / 200.0;

    let attacker = crate::transactions::presence(ctx.state, ctx.recipient);
    let victim = crate::transactions::presence(ctx.state, target);
    candidate.features.military_relevance = if attacker.intersection(&victim).next().is_some() {
        1.0
    } else if attacker.iter().any(|system| {
        ctx.galaxy
            .adjacent(system.as_str())
            .into_iter()
            .any(|adjacent| victim.contains(&ti4_model::SystemId::new(adjacent)))
    }) {
        0.5
    } else {
        0.0
    };
}

fn values(terms: &[DealTerm]) -> (f32, f32) {
    terms.iter().fold((0.0, 0.0), |(immediate, future), term| {
        let value = match term {
            DealTerm::ImmediateTransfer(a) | DealTerm::FuturePayment { asset: a, .. } => {
                asset_value(a)
            }
            _ => 0.0,
        };
        if term.is_immediate() {
            (immediate + value, future)
        } else {
            (immediate, future + value)
        }
    })
}

/// What a side's terms are worth to whoever receives them.
///
/// Deliberately the receiver's side of the oracle's two-sided pricing: these numbers describe what
/// each side delivers, and the cost of parting with it is priced where the deciding seat is known
/// (`ti4-policy` features). A commodity delivers a full trade good either way (21.5).
fn asset_value(asset: &TransferAsset) -> f32 {
    match asset {
        TransferAsset::TradeGoods(n)
        | TransferAsset::Commodities(n)
        | TransferAsset::CulturalFragments(n)
        | TransferAsset::HazardousFragments(n)
        | TransferAsset::IndustrialFragments(n)
        | TransferAsset::UnknownFragments(n) => f32::from(*n),
        TransferAsset::PromissoryNote(_)
        | TransferAsset::ActionCard(_)
        | TransferAsset::SecretObjective(_) => 1.0,
    }
}

fn stable_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    use std::fmt::Write as _;
    let digest = Sha256::digest(bytes);
    digest[..8]
        .iter()
        .fold(String::with_capacity(16), |mut encoded, byte| {
            write!(encoded, "{byte:02x}").expect("writing to a String cannot fail");
            encoded
        })
}

fn bounded_holding(value: i32) -> u8 {
    u8::try_from(value.clamp(0, i32::from(u8::MAX))).expect("clamped holding fits u8")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeMap, BTreeSet};
    use ti4_model::{
        DealStatus, DiplomacyState, StrategyCardId, SystemId, SystemState, Unit, UnitTypeId,
    };

    fn pid(id: &str) -> PlayerId {
        PlayerId::new(id)
    }

    fn fixture() -> (GameState, Galaxy) {
        let players = [pid("a"), pid("b"), pid("c")];
        let mut state =
            GameState::new(&players, &[] as &[StrategyCardId], BTreeMap::new(), None, 0);
        state.diplomacy = DiplomacyState::for_players(&players, true);
        for player in &players {
            let seat = state.player_mut(player).unwrap();
            seat.trade_goods = 3;
            seat.commodities = 3;
        }
        let mut system = SystemState::default();
        for player in players {
            system
                .units
                .push(Unit::new(UnitTypeId::new("fighter"), player));
        }
        state.board.insert(SystemId::new("18"), system);
        let galaxy = Galaxy::build(ContentStore::embedded(), &["18"], ti4_model::POK, 0).unwrap();
        (state, galaxy)
    }

    #[test]
    fn initial_candidates_are_stable_bounded_and_template_capped() {
        let (state, galaxy) = fixture();
        let content = ContentStore::embedded();
        let context = CandidateContext {
            state: &state,
            content,
            galaxy: &galaxy,
            proposer: &pid("a"),
            recipient: &pid("b"),
            agenda: None,
        };
        let first = generate_initial_candidates(&context);
        let second = generate_initial_candidates(&context);
        assert_eq!(first, second);
        assert!(first.len() <= MAX_INITIAL_CANDIDATES);
        assert_eq!(
            first
                .iter()
                .map(|candidate| &candidate.id)
                .collect::<BTreeSet<_>>()
                .len(),
            first.len()
        );
        for template in [
            DealTemplate::FuturePayment,
            DealTemplate::PayForNonAggression,
            DealTemplate::PayForAttack,
            DealTemplate::CommodityExchangePlusFavor,
        ] {
            let count = first
                .iter()
                .filter(|candidate| candidate.template == template)
                .count();
            assert!(count > 0, "missing {template:?}");
            assert!(count <= 4, "too many {template:?}: {count}");
        }
        let attack = first
            .iter()
            .find(|candidate| candidate.template == DealTemplate::PayForAttack)
            .expect("fixture has a reachable third party");
        assert!((attack.features.military_relevance - 1.0).abs() < f32::EPSILON);
        assert!(attack.features.target_relationship_effect.abs() < f32::EPSILON);

        let mut hostile = state.clone();
        hostile
            .diplomacy
            .relationship_mut(&pid("b"), &pid("c"))
            .unwrap()
            .hostility = 100;
        let hostile_context = CandidateContext {
            state: &hostile,
            ..context
        };
        let hostile_attack = generate_initial_candidates(&hostile_context)
            .into_iter()
            .find(|candidate| candidate.id == attack.id)
            .expect("relationships do not change candidate identity");
        assert!((hostile_attack.features.target_relationship_effect - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn payment_actions_offer_only_currently_legal_atomic_transfers() {
        let (mut state, galaxy) = fixture();
        let content = ContentStore::embedded();
        let revision = DealRevision::new(
            0,
            pid("a"),
            vec![DealTerm::FuturePayment {
                asset: TransferAsset::TradeGoods(2),
                deadline_round: state.round,
            }],
            vec![],
            state.round,
        )
        .unwrap();
        let id = state
            .diplomacy
            .create_deal(pid("a"), pid("b"), state.round, revision)
            .unwrap();
        state.diplomacy.active_deals.get_mut(&id).unwrap().status = DealStatus::Active;
        assert_eq!(
            payment_actions(&state, content, &galaxy, &pid("a")).len(),
            1
        );
        state.player_mut(&pid("a")).unwrap().trade_goods = 1;
        assert!(payment_actions(&state, content, &galaxy, &pid("a")).is_empty());
    }

    #[test]
    fn signals_are_concrete_and_name_only_contested_systems() {
        let (state, galaxy) = fixture();
        let content = ContentStore::embedded();
        let context = CandidateContext {
            state: &state,
            content,
            galaxy: &galaxy,
            proposer: &pid("a"),
            recipient: &pid("b"),
            agenda: None,
        };
        let statements = generate_signal_statements(&context);
        assert_eq!(
            statements,
            vec![ti4_model::SignalStatement::AttackIfYouActivate {
                system: SystemId::new("18")
            }],
            "one warning about one contested system, and nothing else"
        );
        assert_eq!(statements, generate_signal_statements(&context));
    }

    #[test]
    fn with_diplomacy_on_trading_happens_inside_the_contact() {
        let (mut state, galaxy) = fixture();
        let content = ContentStore::embedded();
        assert!(
            crate::transactions::available_actions(&state, content, &galaxy, &pid("a")).is_empty(),
            "no separate transaction window while diplomacy is on"
        );
        let context = CandidateContext {
            state: &state,
            content,
            galaxy: &galaxy,
            proposer: &pid("a"),
            recipient: &pid("b"),
            agenda: None,
        };
        let trades: Vec<CandidateBundle> = generate_initial_candidates(&context)
            .into_iter()
            .filter(|candidate| candidate.template == DealTemplate::Trade)
            .collect();
        assert!(!trades.is_empty(), "partners trade inside the contact");
        assert!(trades.len() <= MAX_TRADE_CANDIDATES);
        assert!(
            trades.iter().all(|candidate| {
                !candidate.revision.proposer_terms.is_empty()
                    && !candidate.revision.recipient_terms.is_empty()
            }),
            "nothing for nothing: both sides carry something"
        );
        assert!(
            trades.iter().all(|candidate| {
                candidate
                    .revision
                    .proposer_terms
                    .iter()
                    .chain(&candidate.revision.recipient_terms)
                    .any(DealTerm::is_immediate)
            }),
            "a trade still moves something now"
        );
        assert!(
            trades.iter().all(|candidate| {
                candidate
                    .revision
                    .proposer_terms
                    .iter()
                    .chain(&candidate.revision.recipient_terms)
                    .all(|term| {
                        term.is_immediate() || matches!(term, DealTerm::FuturePayment { .. })
                    })
            }),
            "the only promise a trade carries is the commitment owed for a one-sided shape"
        );

        state.diplomacy.enabled = false;
        assert!(
            !crate::transactions::available_actions(&state, content, &galaxy, &pid("a")).is_empty(),
            "with diplomacy off the legacy window is unchanged"
        );
    }

    #[test]
    fn favours_and_votes_are_offered_only_when_their_components_exist() {
        let (mut state, galaxy) = fixture();
        let content = ContentStore::embedded();
        let has = |bundles: &[CandidateBundle], template| {
            bundles
                .iter()
                .any(|candidate| candidate.template == template)
        };
        let plain = generate_initial_candidates(&CandidateContext {
            state: &state,
            content,
            galaxy: &galaxy,
            proposer: &pid("a"),
            recipient: &pid("b"),
            agenda: None,
        });
        assert!(!has(&plain, DealTemplate::PayForAgentFavour));

        state.player_mut(&pid("b")).unwrap().leaders.insert(
            ti4_model::id::LeaderId::new("hacanagent"),
            ti4_model::state::LeaderStatus::Readied,
        );
        let favours = generate_initial_candidates(&CandidateContext {
            state: &state,
            content,
            galaxy: &galaxy,
            proposer: &pid("a"),
            recipient: &pid("b"),
            agenda: None,
        });
        assert!(favours.len() <= MAX_INITIAL_CANDIDATES);
        assert!(has(&favours, DealTemplate::PayForAgentFavour));
        assert!(
            has(&favours, DealTemplate::Trade),
            "the cap takes templates in turns, so trades survive a crowded table"
        );
        assert!(
            !has(&favours, DealTemplate::PayForVote),
            "votes are bought only in the talks before one"
        );

        state.phase = ti4_model::state::Phase::Agenda;
        state.agenda_choices = vec!["for".to_owned(), "against".to_owned()];
        let talks = generate_initial_candidates(&CandidateContext {
            state: &state,
            content,
            galaxy: &galaxy,
            proposer: &pid("a"),
            recipient: &pid("b"),
            agenda: Some("minister_of_war"),
        });
        let vote = talks
            .iter()
            .find(|candidate| candidate.template == DealTemplate::PayForVote)
            .expect("a contact before a vote can buy votes");
        assert!(matches!(
            &vote.revision.recipient_terms[0],
            DealTerm::Vote { agenda, .. } if agenda == "minister_of_war"
        ));
        let signals = generate_signal_statements(&CandidateContext {
            state: &state,
            content,
            galaxy: &galaxy,
            proposer: &pid("a"),
            recipient: &pid("b"),
            agenda: Some("minister_of_war"),
        });
        assert!(
            signals.iter().any(|signal| matches!(
                signal,
                ti4_model::SignalStatement::WillVote { agenda, outcome }
                    if agenda == "minister_of_war" && state.agenda_choices.contains(outcome)
            )),
            "the talks before a vote offer a vote assurance: {signals:?}"
        );
    }

    #[test]
    fn contact_ids_name_seats_even_when_factions_are_duplicated() {
        let (mut state, _) = fixture();
        for (seat, faction) in state.players.iter_mut().zip(["sol", "hacan", "hacan"]) {
            seat.faction = ti4_model::id::FactionId::new(faction);
        }
        let contacts = available_contacts(&state, &pid("a"));
        assert_eq!(contacts.len(), 2, "both Hacan seats remain addressable");
        assert_eq!(
            contacts
                .iter()
                .filter_map(|option| contact_target(&state, option))
                .collect::<Vec<_>>(),
            vec![pid("b"), pid("c")]
        );
        let from_b: Vec<_> = available_contacts(&state, &pid("b"))
            .iter()
            .filter_map(|option| contact_target(&state, option))
            .collect();
        assert_eq!(from_b, vec![pid("a"), pid("c")]);
    }

    #[test]
    fn a_contact_never_offers_trade_goods_for_trade_goods() {
        let (state, galaxy) = fixture();
        let content = ContentStore::embedded();
        let offered = generate_initial_candidates(&CandidateContext {
            state: &state,
            content,
            galaxy: &galaxy,
            proposer: &pid("a"),
            recipient: &pid("b"),
            agenda: None,
        });
        let goods_only = |terms: &[DealTerm]| {
            !terms.is_empty()
                && terms.iter().all(|term| {
                    matches!(
                        term,
                        DealTerm::ImmediateTransfer(TransferAsset::TradeGoods(_))
                    )
                })
        };
        assert!(
            !offered.iter().any(|candidate| {
                goods_only(&candidate.revision.proposer_terms)
                    && goods_only(&candidate.revision.recipient_terms)
            }),
            "a trade good for a trade good is not a deal"
        );
        // A symmetric swap of anything but trade goods is a real deal and must survive: each
        // commodity becomes a trade good in the other player's hands.
        let commodities_only = |terms: &[DealTerm]| {
            !terms.is_empty()
                && terms.iter().all(|term| {
                    matches!(
                        term,
                        DealTerm::ImmediateTransfer(TransferAsset::Commodities(_))
                    )
                })
        };
        assert!(
            offered.iter().any(|candidate| {
                commodities_only(&candidate.revision.proposer_terms)
                    && commodities_only(&candidate.revision.recipient_terms)
            }),
            "swapping commodities leaves both sides better off"
        );
    }

    #[test]
    fn an_unplayed_trade_card_is_worth_selling_as_a_refresh() {
        let (mut state, galaxy) = fixture();
        let content = ContentStore::embedded();
        let has_refresh = |state: &GameState| {
            generate_initial_candidates(&CandidateContext {
                state,
                content,
                galaxy: &galaxy,
                proposer: &pid("a"),
                recipient: &pid("b"),
                agenda: None,
            })
            .into_iter()
            .find(|candidate| candidate.template == DealTemplate::RefreshForCommodity)
        };
        assert!(
            has_refresh(&state).is_none(),
            "no Trade card, no refresh to sell"
        );

        let trade = StrategyCardId::new("pok5trade");
        state
            .player_mut(&pid("a"))
            .unwrap()
            .strategy_cards
            .push(trade.clone());
        let offered = has_refresh(&state).expect("an unplayed Trade card is worth selling");
        assert_eq!(
            offered.revision.proposer_terms,
            vec![DealTerm::ReplenishFor {
                beneficiary: pid("b"),
                deadline_round: state.round.saturating_add(1),
            }]
        );
        assert!(matches!(
            offered.revision.recipient_terms.as_slice(),
            [DealTerm::FuturePayment {
                asset: TransferAsset::Commodities(1),
                ..
            }]
        ));

        state
            .player_mut(&pid("a"))
            .unwrap()
            .exhausted_strategy_cards
            .insert(trade);
        assert!(
            has_refresh(&state).is_none(),
            "a played Trade card has no refresh left to promise"
        );
    }

    #[test]
    fn counters_never_ask_a_side_for_more_trade_goods_than_it_holds() {
        let (mut state, _) = fixture();
        state.player_mut(&pid("a")).unwrap().trade_goods = 2;
        let revision = DealRevision::new(
            0,
            pid("a"),
            vec![DealTerm::ImmediateTransfer(TransferAsset::TradeGoods(2))],
            vec![DealTerm::DoNotAttack {
                player: pid("a"),
                deadline_round: state.round,
            }],
            state.round,
        )
        .unwrap();
        let current = bundle(DealTemplate::PayForNonAggression, revision);

        let counters = generate_counter_candidates(
            &state,
            &pid("a"),
            &pid("b"),
            &current,
            &pid("b"),
            state.round,
        );

        let paid: Vec<_> = counters
            .iter()
            .map(|counter| counter.revision.proposer_terms[0].clone())
            .collect();
        assert!(paid.contains(&DealTerm::ImmediateTransfer(TransferAsset::TradeGoods(1))));
        assert!(!paid.contains(&DealTerm::ImmediateTransfer(TransferAsset::TradeGoods(3))));
        assert!(counters.iter().any(|counter| {
            counter.revision.proposer_terms[0]
                == DealTerm::ImmediateTransfer(TransferAsset::TradeGoods(1))
                && (counter.features.immediate_value_self - 1.0).abs() < f32::EPSILON
        }));
    }
}
