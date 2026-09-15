//! Deterministic, bounded whole-bundle generation.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use ti4_content::{ContentStore, galaxy::Galaxy};
use ti4_model::{DealRevision, DealTerm, GameState, PlayerId, TransferAsset};

pub const MAX_INITIAL_CANDIDATES: usize = 24;
pub const MAX_COUNTER_CANDIDATES: usize = 8;
pub const OPEN_KIND: &str = "open_diplomacy";
const OPEN_PREFIX: &str = "component|diplomacy|";
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
        .filter(|target| *target != actor && used.is_none_or(|used| !used.contains(*target)))
        .filter_map(|target| {
            let faction = state.player(target)?.faction.as_str();
            Some(crate::ChoiceOption::labelled(
                format!("{OPEN_PREFIX}{faction}"),
                OPEN_KIND,
                format!("open diplomatic contact with {faction}"),
            ))
        })
        .collect()
}

#[must_use]
pub fn contact_target(state: &GameState, option: &crate::ChoiceOption) -> Option<PlayerId> {
    let faction = option.id.strip_prefix(OPEN_PREFIX)?;
    state
        .seating_order
        .iter()
        .find(|id| {
            state
                .player(id)
                .is_some_and(|seat| seat.faction.as_str() == faction)
        })
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
    NonAggressionSwap,
    PayForAttack,
    CommodityExchangePlusFavor,
    PayForVote,
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
    let mut variants = Vec::new();
    for amount in amounts(recipient_goods) {
        for deadline in [ctx.state.round, ctx.state.round.saturating_add(1)] {
            push(
                &mut variants,
                DealTemplate::FuturePayment,
                ctx.proposer.clone(),
                vec![],
                vec![DealTerm::FuturePayment {
                    asset: TransferAsset::TradeGoods(amount),
                    deadline_round: deadline,
                }],
                ctx.state.round,
            );
        }
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
    for deadline in [ctx.state.round, ctx.state.round.saturating_add(1)] {
        push(
            &mut variants,
            DealTemplate::NonAggressionSwap,
            ctx.proposer.clone(),
            vec![DealTerm::DoNotAttack {
                player: ctx.recipient.clone(),
                deadline_round: deadline,
            }],
            vec![DealTerm::DoNotAttack {
                player: ctx.proposer.clone(),
                deadline_round: deadline,
            }],
            ctx.state.round,
        );
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
    for candidate in &mut out {
        enrich_candidate_features(ctx, candidate);
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out.dedup_by(|a, b| a.id == b.id);
    out.retain(|candidate| {
        immediate_components_are_legal(ctx, candidate)
            && !duplicates_active_obligation(ctx, candidate)
    });
    out.truncate(MAX_INITIAL_CANDIDATES);
    out
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
            | DealTerm::Attack { deadline_round, .. } => *deadline_round = replacement,
            DealTerm::ImmediateTransfer(_) => {}
        }
        changed = true;
        break;
    }
    if changed {
        variants.push(counter_bundle(current, expiry));
    }
    variants
        .retain(|candidate| immediates_affordable(state, proposer, recipient, &candidate.revision));
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
            DealTemplate::NonAggressionSwap,
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
        assert_eq!(attack.features.military_relevance, 1.0);
        assert_eq!(attack.features.target_relationship_effect, 0.0);

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
        assert_eq!(hostile_attack.features.target_relationship_effect, 0.5);
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
                && counter.features.immediate_value_self == 1.0
        }));
    }
}
