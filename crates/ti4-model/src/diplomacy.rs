//! Match-scoped structured diplomacy state.
//!
//! This module contains data and structural validation only. Gameplay legality and
//! valuation belong to `ti4-engine`.

use crate::{ActionCardId, PlanetId, PlayerId, SystemId};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;

pub const DIPLOMACY_RULES_V1: &str = "diplomacy-rules-v1";
pub const MAX_TERMS_PER_SIDE: usize = 3;
pub const MAX_COUNTERS: u8 = 2;
pub const MAX_TERMINAL_HISTORY: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DealId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SignalId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Relationship {
    pub trust: i16,
    pub threat: u8,
    pub cooperation: i16,
    pub hostility: u8,
}

impl Relationship {
    #[must_use]
    pub fn adjusted(self, trust: i16, threat: i16, cooperation: i16, hostility: i16) -> Self {
        Self {
            trust: (self.trust + trust).clamp(-100, 100),
            threat: u8::try_from((i16::from(self.threat) + threat).clamp(0, 100))
                .unwrap_or_default(),
            cooperation: (self.cooperation + cooperation).clamp(-100, 100),
            hostility: u8::try_from((i16::from(self.hostility) + hostility).clamp(0, 100))
                .unwrap_or_default(),
        }
    }

    pub fn decay(&mut self) {
        self.trust = self.trust * 90 / 100;
        self.cooperation = self.cooperation * 90 / 100;
        self.threat = u8::try_from(u16::from(self.threat) * 80 / 100).unwrap_or_default();
        self.hostility = u8::try_from(u16::from(self.hostility) * 80 / 100).unwrap_or_default();
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransferAsset {
    TradeGoods(u8),
    Commodities(u8),
    CulturalFragments(u8),
    HazardousFragments(u8),
    IndustrialFragments(u8),
    UnknownFragments(u8),
    PromissoryNote(String),
    ActionCard(ActionCardId),
    SecretObjective(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DealTerm {
    ImmediateTransfer(TransferAsset),
    FuturePayment {
        asset: TransferAsset,
        deadline_round: u32,
    },
    DoNotActivate {
        system: SystemId,
        deadline_round: u32,
    },
    DoNotAttack {
        player: PlayerId,
        deadline_round: u32,
    },
    Vote {
        agenda: String,
        outcome: String,
        deadline_round: u32,
    },
    Attack {
        player: PlayerId,
        deadline_round: u32,
    },
}

impl DealTerm {
    #[must_use]
    pub const fn is_immediate(&self) -> bool {
        matches!(self, Self::ImmediateTransfer(_))
    }

    #[must_use]
    pub const fn deadline_round(&self) -> Option<u32> {
        match self {
            Self::ImmediateTransfer(_) => None,
            Self::FuturePayment { deadline_round, .. }
            | Self::DoNotActivate { deadline_round, .. }
            | Self::DoNotAttack { deadline_round, .. }
            | Self::Vote { deadline_round, .. }
            | Self::Attack { deadline_round, .. } => Some(*deadline_round),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PromiseStatus {
    Pending,
    Fulfilled,
    Broken,
    Expired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DealRevision {
    pub number: u8,
    pub author: PlayerId,
    pub proposer_terms: Vec<DealTerm>,
    pub recipient_terms: Vec<DealTerm>,
    #[serde(default)]
    pub proposer_statuses: Vec<PromiseStatus>,
    #[serde(default)]
    pub recipient_statuses: Vec<PromiseStatus>,
}

impl DealRevision {
    /// Construct and structurally validate one complete revision.
    ///
    /// # Errors
    /// Returns [`DiplomacyError`] when term counts, assets, or deadlines are invalid.
    pub fn new(
        number: u8,
        author: PlayerId,
        proposer_terms: Vec<DealTerm>,
        recipient_terms: Vec<DealTerm>,
        current_round: u32,
    ) -> Result<Self, DiplomacyError> {
        if proposer_terms.len() > MAX_TERMS_PER_SIDE || recipient_terms.len() > MAX_TERMS_PER_SIDE {
            return Err(DiplomacyError::TooManyTerms);
        }
        if proposer_terms.is_empty() && recipient_terms.is_empty() {
            return Err(DiplomacyError::EmptyDeal);
        }
        if proposer_terms
            .iter()
            .chain(&recipient_terms)
            .filter_map(DealTerm::deadline_round)
            .any(|deadline| deadline < current_round || deadline > current_round.saturating_add(1))
        {
            return Err(DiplomacyError::InvalidDeadline);
        }
        validate_assets(proposer_terms.iter().chain(&recipient_terms))?;
        let proposer_statuses = statuses_for(&proposer_terms);
        let recipient_statuses = statuses_for(&recipient_terms);
        Ok(Self {
            number,
            author,
            proposer_terms,
            recipient_terms,
            proposer_statuses,
            recipient_statuses,
        })
    }
}

fn statuses_for(terms: &[DealTerm]) -> Vec<PromiseStatus> {
    terms
        .iter()
        .map(|term| {
            if term.is_immediate() {
                PromiseStatus::Fulfilled
            } else {
                PromiseStatus::Pending
            }
        })
        .collect()
}

fn validate_assets<'a>(terms: impl Iterator<Item = &'a DealTerm>) -> Result<(), DiplomacyError> {
    for asset in terms.filter_map(|term| match term {
        DealTerm::ImmediateTransfer(asset) | DealTerm::FuturePayment { asset, .. } => Some(asset),
        _ => None,
    }) {
        let valid = match asset {
            TransferAsset::TradeGoods(n)
            | TransferAsset::Commodities(n)
            | TransferAsset::CulturalFragments(n)
            | TransferAsset::HazardousFragments(n)
            | TransferAsset::IndustrialFragments(n)
            | TransferAsset::UnknownFragments(n) => *n > 0,
            TransferAsset::PromissoryNote(id) | TransferAsset::SecretObjective(id) => {
                !id.is_empty()
            }
            TransferAsset::ActionCard(_) => true,
        };
        if !valid {
            return Err(DiplomacyError::InvalidAsset);
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DealStatus {
    Proposed,
    Accepted,
    Active,
    Declined,
    Fulfilled,
    Broken,
    Expired,
    /// A migrated free-text record. It is historical and never executable.
    Legacy,
}

impl DealStatus {
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Declined | Self::Fulfilled | Self::Broken | Self::Expired | Self::Legacy
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Deal {
    pub id: DealId,
    pub proposer: PlayerId,
    pub recipient: PlayerId,
    pub created_round: u32,
    pub revisions: Vec<DealRevision>,
    pub status: DealStatus,
}

impl Deal {
    #[must_use]
    /// # Panics
    /// Only an invalid, manually-constructed deal with no revisions can panic.
    pub fn latest(&self) -> &DealRevision {
        self.revisions
            .last()
            .expect("a validated deal has a revision")
    }

    /// # Errors
    /// Returns [`DiplomacyError`] unless this is the next legal bounded revision.
    pub fn add_counter(&mut self, revision: DealRevision) -> Result<(), DiplomacyError> {
        if self.status != DealStatus::Proposed {
            return Err(DiplomacyError::TerminalDeal);
        }
        if self.revisions.len().saturating_sub(1) >= usize::from(MAX_COUNTERS) {
            return Err(DiplomacyError::TooManyCounters);
        }
        let expected = u8::try_from(self.revisions.len()).unwrap_or(u8::MAX);
        if revision.number != expected
            || (revision.author != self.proposer && revision.author != self.recipient)
        {
            return Err(DiplomacyError::InvalidRevision);
        }
        self.revisions.push(revision);
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminalDealSummary {
    pub id: DealId,
    pub proposer: PlayerId,
    pub recipient: PlayerId,
    pub created_round: u32,
    pub terminal_round: u32,
    pub status: DealStatus,
    pub latest_revision: Option<DealRevision>,
    pub legacy_promise: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub legacy_status: Option<PromiseStatus>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SignalKind {
    Request,
    Threat,
    Assurance,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SignalSubject {
    Player(PlayerId),
    System(SystemId),
    Planet(PlanetId),
    AgendaOutcome { agenda: String, outcome: String },
    Predicate(DealTerm),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignalCondition {
    pub predicate: DealTerm,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Signal {
    pub id: SignalId,
    pub speaker: PlayerId,
    pub target: PlayerId,
    pub kind: SignalKind,
    pub subject: SignalSubject,
    pub condition: Option<SignalCondition>,
    pub created_round: u32,
    pub expires_round: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiplomacyEvent {
    Offered {
        deal_id: DealId,
        proposer: PlayerId,
        recipient: PlayerId,
        relationship_snapshot: DealRelationshipSnapshot,
        revision: DealRevision,
    },
    Countered {
        deal_id: DealId,
        revision: DealRevision,
    },
    Accepted {
        deal_id: DealId,
        actor: PlayerId,
        revision: u8,
    },
    Declined {
        deal_id: DealId,
        actor: PlayerId,
        revision: u8,
    },
    ImmediateApplied {
        deal_id: DealId,
        term_index: u8,
    },
    PromiseSettled {
        deal_id: DealId,
        term_index: u8,
        status: PromiseStatus,
    },
    DealSettled {
        deal_id: DealId,
        status: DealStatus,
    },
    SignalEmitted {
        signal_id: SignalId,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DealRelationshipSnapshot {
    pub proposer_to_recipient: Relationship,
    pub recipient_to_proposer: Relationship,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DealResponseKind {
    Counter,
    Accept,
    Decline,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DealResponseRecord {
    pub round: u32,
    pub actor: PlayerId,
    pub revision: u8,
    pub response: DealResponseKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiplomacyJournalEntry {
    pub round: u32,
    pub event: DiplomacyEvent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiplomacyLogRecord {
    pub schema: String,
    pub rules: String,
    pub game_id: String,
    pub deal_id: DealId,
    pub created_round: u32,
    pub proposer: PlayerId,
    pub recipient: PlayerId,
    pub relationship_snapshot: DealRelationshipSnapshot,
    pub original_bundle: DealRevision,
    pub counters: Vec<DealRevision>,
    pub responses: Vec<DealResponseRecord>,
    pub final_status: DealStatus,
    pub terminal_round: u32,
    pub events: Vec<DiplomacyJournalEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiplomacyState {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "rules_v1")]
    pub rules_version: String,
    #[serde(default)]
    pub relationships: BTreeMap<PlayerId, BTreeMap<PlayerId, Relationship>>,
    #[serde(default)]
    pub active_deals: BTreeMap<DealId, Deal>,
    #[serde(default)]
    pub history: Vec<TerminalDealSummary>,
    #[serde(default)]
    pub recent_signals: Vec<Signal>,
    #[serde(default)]
    pub journal: Vec<DiplomacyJournalEntry>,
    #[serde(default = "first_id")]
    pub next_deal_id: u64,
    #[serde(default = "first_id")]
    pub next_signal_id: u64,
    #[serde(default)]
    pub initiations_this_turn: BTreeMap<PlayerId, BTreeSet<PlayerId>>,
    #[serde(default)]
    pub last_attacks: BTreeMap<PlayerId, BTreeMap<PlayerId, u32>>,
    #[serde(default)]
    pub last_breaches: BTreeMap<PlayerId, BTreeMap<PlayerId, u32>>,
    #[serde(default)]
    pub judged_attacks: BTreeSet<(u32, PlayerId, PlayerId)>,
}

fn rules_v1() -> String {
    DIPLOMACY_RULES_V1.to_owned()
}
const fn first_id() -> u64 {
    1
}

impl Default for DiplomacyState {
    fn default() -> Self {
        Self {
            enabled: false,
            rules_version: rules_v1(),
            relationships: BTreeMap::new(),
            active_deals: BTreeMap::new(),
            history: Vec::new(),
            recent_signals: Vec::new(),
            journal: Vec::new(),
            next_deal_id: 1,
            next_signal_id: 1,
            initiations_this_turn: BTreeMap::new(),
            last_attacks: BTreeMap::new(),
            last_breaches: BTreeMap::new(),
            judged_attacks: BTreeSet::new(),
        }
    }
}

impl DiplomacyState {
    #[must_use]
    pub fn for_players(players: &[PlayerId], enabled: bool) -> Self {
        let mut state = Self {
            enabled,
            ..Self::default()
        };
        for observer in players {
            let subjects = players
                .iter()
                .filter(|subject| *subject != observer)
                .cloned()
                .map(|subject| (subject, Relationship::default()))
                .collect();
            state.relationships.insert(observer.clone(), subjects);
        }
        state
    }

    #[must_use]
    pub fn relationship(&self, observer: &PlayerId, subject: &PlayerId) -> Relationship {
        self.relationships
            .get(observer)
            .and_then(|row| row.get(subject))
            .copied()
            .unwrap_or_default()
    }

    /// # Errors
    /// Returns [`DiplomacyError::SelfPair`] for a self-directed relationship.
    pub fn relationship_mut(
        &mut self,
        observer: &PlayerId,
        subject: &PlayerId,
    ) -> Result<&mut Relationship, DiplomacyError> {
        if observer == subject {
            return Err(DiplomacyError::SelfPair);
        }
        Ok(self
            .relationships
            .entry(observer.clone())
            .or_default()
            .entry(subject.clone())
            .or_default())
    }

    pub fn consume_initiation(&mut self, actor: &PlayerId, target: &PlayerId) -> bool {
        actor != target
            && self
                .initiations_this_turn
                .entry(actor.clone())
                .or_default()
                .insert(target.clone())
    }

    pub fn clear_turn_initiations(&mut self) {
        self.initiations_this_turn.clear();
    }

    /// # Errors
    /// Returns [`DiplomacyError`] for invalid parties, IDs, or revisions.
    pub fn create_deal(
        &mut self,
        proposer: PlayerId,
        recipient: PlayerId,
        round: u32,
        revision: DealRevision,
    ) -> Result<DealId, DiplomacyError> {
        if proposer == recipient {
            return Err(DiplomacyError::SelfPair);
        }
        if !self.relationships.contains_key(&proposer)
            || !self.relationships.contains_key(&recipient)
        {
            return Err(DiplomacyError::PlayerMissing);
        }
        if revision.author != proposer || revision.number != 0 {
            return Err(DiplomacyError::InvalidRevision);
        }
        let id = DealId(self.next_deal_id);
        self.next_deal_id = self
            .next_deal_id
            .checked_add(1)
            .ok_or(DiplomacyError::IdExhausted)?;
        let relationship_snapshot = DealRelationshipSnapshot {
            proposer_to_recipient: self.relationship(&proposer, &recipient),
            recipient_to_proposer: self.relationship(&recipient, &proposer),
        };
        self.active_deals.insert(
            id,
            Deal {
                id,
                proposer: proposer.clone(),
                recipient: recipient.clone(),
                created_round: round,
                revisions: vec![revision.clone()],
                status: DealStatus::Proposed,
            },
        );
        self.journal.push(DiplomacyJournalEntry {
            round,
            event: DiplomacyEvent::Offered {
                deal_id: id,
                proposer,
                recipient,
                relationship_snapshot,
                revision,
            },
        });
        Ok(id)
    }

    /// # Errors
    /// Returns [`DiplomacyError`] if the deal is unknown or not terminal.
    pub fn archive_deal(&mut self, id: DealId, round: u32) -> Result<(), DiplomacyError> {
        let deal = self
            .active_deals
            .get(&id)
            .ok_or(DiplomacyError::UnknownDeal)?;
        if !deal.status.is_terminal() {
            return Err(DiplomacyError::TerminalDeal);
        }
        let deal = self
            .active_deals
            .remove(&id)
            .ok_or(DiplomacyError::UnknownDeal)?;
        self.history.push(TerminalDealSummary {
            id,
            proposer: deal.proposer,
            recipient: deal.recipient,
            created_round: deal.created_round,
            terminal_round: round,
            status: deal.status,
            latest_revision: deal.revisions.last().cloned(),
            legacy_promise: None,
            legacy_status: None,
        });
        if self.history.len() > MAX_TERMINAL_HISTORY {
            self.history.remove(0);
        }
        Ok(())
    }

    pub fn prune_signals(&mut self, round: u32) {
        self.recent_signals
            .retain(|signal| signal.expires_round >= round);
    }

    /// Validate a decoded match-scoped state before it is admitted by the compatibility loader.
    /// # Errors
    /// Returns [`DiplomacyError`] when decoded state violates the v1 structural contract.
    pub fn validate(&self, players: &[PlayerId], current_round: u32) -> Result<(), DiplomacyError> {
        let seated: BTreeSet<_> = players.iter().cloned().collect();
        if seated.len() != players.len() {
            return Err(DiplomacyError::InvalidState);
        }
        if self.rules_version != DIPLOMACY_RULES_V1 {
            return Err(DiplomacyError::UnsupportedRules);
        }
        if self.history.len() > MAX_TERMINAL_HISTORY
            || self.next_deal_id == 0
            || self.next_signal_id == 0
        {
            return Err(DiplomacyError::InvalidState);
        }
        if self.enabled {
            for observer in &seated {
                let row = self
                    .relationships
                    .get(observer)
                    .ok_or(DiplomacyError::PlayerMissing)?;
                if row.len() != seated.len().saturating_sub(1)
                    || row.contains_key(observer)
                    || row.keys().any(|subject| !seated.contains(subject))
                    || row.values().any(|relationship| {
                        !(-100..=100).contains(&relationship.trust)
                            || !(-100..=100).contains(&relationship.cooperation)
                            || relationship.threat > 100
                            || relationship.hostility > 100
                    })
                {
                    return Err(DiplomacyError::InvalidState);
                }
            }
            if self
                .relationships
                .keys()
                .any(|player| !seated.contains(player))
            {
                return Err(DiplomacyError::PlayerMissing);
            }
        }

        let mut seen_deals = BTreeSet::new();
        for (key, deal) in &self.active_deals {
            if key != &deal.id
                || !seen_deals.insert(deal.id)
                || !seated.contains(&deal.proposer)
                || !seated.contains(&deal.recipient)
                || deal.proposer == deal.recipient
                || deal.revisions.is_empty()
                || deal.revisions.len() > usize::from(MAX_COUNTERS) + 1
                || deal.status.is_terminal()
            {
                return Err(DiplomacyError::InvalidState);
            }
            validate_revisions(deal)?;
        }
        for summary in &self.history {
            if !seen_deals.insert(summary.id)
                || !summary.status.is_terminal()
                || !seated.contains(&summary.proposer)
                || !seated.contains(&summary.recipient)
                || summary.proposer == summary.recipient
            {
                return Err(DiplomacyError::InvalidState);
            }
            if let Some(revision) = &summary.latest_revision {
                if revision.number > MAX_COUNTERS
                    || (revision.author != summary.proposer && revision.author != summary.recipient)
                {
                    return Err(DiplomacyError::InvalidRevision);
                }
                validate_revision_shape(revision, summary.created_round)?;
            }
        }
        if seen_deals.iter().any(|id| id.0 >= self.next_deal_id) {
            return Err(DiplomacyError::InvalidState);
        }

        let mut seen_signals = BTreeSet::new();
        for signal in &self.recent_signals {
            if !seen_signals.insert(signal.id)
                || signal.id.0 >= self.next_signal_id
                || signal.speaker == signal.target
                || !seated.contains(&signal.speaker)
                || !seated.contains(&signal.target)
                || signal.created_round > current_round
                || signal.expires_round < signal.created_round
                || signal.expires_round > signal.created_round.saturating_add(1)
            {
                return Err(DiplomacyError::InvalidState);
            }
        }
        if self.initiations_this_turn.iter().any(|(actor, targets)| {
            !seated.contains(actor)
                || targets
                    .iter()
                    .any(|target| target == actor || !seated.contains(target))
        }) {
            return Err(DiplomacyError::InvalidState);
        }
        Ok(())
    }
}

fn validate_revisions(deal: &Deal) -> Result<(), DiplomacyError> {
    for (index, revision) in deal.revisions.iter().enumerate() {
        if usize::from(revision.number) != index
            || (revision.author != deal.proposer && revision.author != deal.recipient)
            || (index == 0 && revision.author != deal.proposer)
        {
            return Err(DiplomacyError::InvalidRevision);
        }
        validate_revision_shape(revision, deal.created_round)?;
    }
    Ok(())
}

fn validate_revision_shape(
    revision: &DealRevision,
    created_round: u32,
) -> Result<(), DiplomacyError> {
    let rebuilt = DealRevision::new(
        revision.number,
        revision.author.clone(),
        revision.proposer_terms.clone(),
        revision.recipient_terms.clone(),
        created_round,
    )?;
    if rebuilt.proposer_statuses.len() != revision.proposer_statuses.len()
        || rebuilt.recipient_statuses.len() != revision.recipient_statuses.len()
    {
        return Err(DiplomacyError::InvalidRevision);
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DiplomacyError {
    #[error("diplomacy participants must be distinct")]
    SelfPair,
    #[error("a diplomacy participant is not seated")]
    PlayerMissing,
    #[error("a deal contains too many terms")]
    TooManyTerms,
    #[error("a deal must contain at least one term")]
    EmptyDeal,
    #[error("a deadline must be this round or the next round")]
    InvalidDeadline,
    #[error("a transfer amount or identifier is invalid")]
    InvalidAsset,
    #[error("a deal may contain at most two counters")]
    TooManyCounters,
    #[error("the deal revision is invalid")]
    InvalidRevision,
    #[error("the deal is not in a mutable state")]
    TerminalDeal,
    #[error("the deal does not exist")]
    UnknownDeal,
    #[error("the match-scoped identifier space is exhausted")]
    IdExhausted,
    #[error("the diplomacy state is structurally invalid")]
    InvalidState,
    #[error("the diplomacy rules version is unsupported")]
    UnsupportedRules,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pid(id: &str) -> PlayerId {
        PlayerId::new(id)
    }

    #[test]
    fn relationships_are_neutral_directional_and_clamped() {
        let a = pid("a");
        let b = pid("b");
        let mut state = DiplomacyState::for_players(&[a.clone(), b.clone()], true);
        *state.relationship_mut(&a, &b).unwrap() =
            Relationship::default().adjusted(150, 150, -150, -1);
        assert_eq!(
            state.relationship(&a, &b),
            Relationship {
                trust: 100,
                threat: 100,
                cooperation: -100,
                hostility: 0
            }
        );
        assert_eq!(state.relationship(&b, &a), Relationship::default());
        assert!(state.relationships.get(&a).unwrap().get(&a).is_none());
    }

    #[test]
    fn decay_moves_negative_values_toward_zero() {
        let mut relation = Relationship {
            trust: -9,
            threat: 9,
            cooperation: -19,
            hostility: 19,
        };
        relation.decay();
        assert_eq!(
            relation,
            Relationship {
                trust: -8,
                threat: 7,
                cooperation: -17,
                hostility: 15
            }
        );
    }

    #[test]
    fn ids_round_trip_and_remain_monotonic() {
        let a = pid("a");
        let b = pid("b");
        let mut state = DiplomacyState::for_players(&[a.clone(), b.clone()], true);
        let revision = DealRevision::new(
            0,
            a.clone(),
            vec![DealTerm::FuturePayment {
                asset: TransferAsset::TradeGoods(1),
                deadline_round: 2,
            }],
            vec![],
            1,
        )
        .unwrap();
        assert_eq!(state.create_deal(a, b, 1, revision).unwrap(), DealId(1));
        let mut back: DiplomacyState =
            serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap();
        let revision = DealRevision::new(
            0,
            pid("a"),
            vec![DealTerm::FuturePayment {
                asset: TransferAsset::TradeGoods(1),
                deadline_round: 2,
            }],
            vec![],
            1,
        )
        .unwrap();
        assert_eq!(
            back.create_deal(pid("a"), pid("b"), 1, revision).unwrap(),
            DealId(2)
        );
    }

    #[test]
    fn structural_bounds_are_atomic() {
        let term = DealTerm::FuturePayment {
            asset: TransferAsset::TradeGoods(1),
            deadline_round: 1,
        };
        assert_eq!(
            DealRevision::new(0, pid("a"), vec![term.clone(); 4], vec![], 1),
            Err(DiplomacyError::TooManyTerms)
        );
        assert_eq!(
            DealRevision::new(
                0,
                pid("a"),
                vec![DealTerm::FuturePayment {
                    asset: TransferAsset::TradeGoods(0),
                    deadline_round: 1
                }],
                vec![],
                1
            ),
            Err(DiplomacyError::InvalidAsset)
        );
    }

    #[test]
    fn decoded_state_validation_rejects_unseated_and_malformed_records() {
        let a = pid("a");
        let b = pid("b");
        let mut state = DiplomacyState::for_players(&[a.clone(), b.clone()], true);
        let revision = DealRevision::new(
            0,
            a.clone(),
            vec![DealTerm::FuturePayment {
                asset: TransferAsset::TradeGoods(1),
                deadline_round: 2,
            }],
            vec![],
            1,
        )
        .unwrap();
        assert!(matches!(
            state.create_deal(a.clone(), pid("missing"), 1, revision.clone()),
            Err(DiplomacyError::PlayerMissing)
        ));
        let id = state.create_deal(a, b, 1, revision).unwrap();
        state.active_deals.get_mut(&id).unwrap().revisions[0]
            .proposer_statuses
            .clear();
        assert!(matches!(
            state.validate(&[pid("a"), pid("b")], 1),
            Err(DiplomacyError::InvalidRevision)
        ));
    }
}
