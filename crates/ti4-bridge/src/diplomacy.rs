//! Untrusted human-language adapter contracts for structured diplomacy.
//!
//! This module intentionally contains no parser or renderer implementation. Adapters may propose
//! structured intent, but only an option ID from the engine's current [`Choice`] can cross the
//! authority boundary.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use ti4_engine::choice::Choice;
use ti4_model::{
    Deal, DealTerm, PlayerId, Relationship, Signal, SignalCondition, SignalKind, SignalSubject,
    SystemId,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DealDraft {
    pub target: PlayerId,
    pub proposer_terms: Vec<DealTerm>,
    pub recipient_terms: Vec<DealTerm>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignalDraft {
    pub target: PlayerId,
    pub kind: SignalKind,
    pub subject: SignalSubject,
    pub condition: Option<SignalCondition>,
    pub expires_round: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiplomacyIntent {
    Offer(DealDraft),
    Signal(SignalDraft),
}

/// The only adapter result that may be submitted to the engine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanonicalIntent {
    pub option_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IntentValidation {
    Canonical(CanonicalIntent),
    Unsupported { reason: String },
    Ambiguous { reason: String },
    Illegal { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiplomacyPromptOption {
    pub id: String,
    pub kind: String,
    pub label: String,
}

/// Public, read-only input suitable for a future parser or renderer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiplomacyPromptContext {
    pub actor: PlayerId,
    pub round: u32,
    pub players: Vec<PlayerId>,
    pub systems: Vec<SystemId>,
    pub agenda_outcomes: BTreeMap<String, Vec<String>>,
    pub relationships: BTreeMap<PlayerId, BTreeMap<PlayerId, Relationship>>,
    pub active_deals: Vec<Deal>,
    pub recent_signals: Vec<Signal>,
    pub legal_options: Vec<DiplomacyPromptOption>,
}

impl DiplomacyPromptContext {
    #[must_use]
    pub fn with_choice(mut self, choice: &Choice) -> Self {
        self.legal_options = choice
            .options
            .iter()
            .map(|option| DiplomacyPromptOption {
                id: option.id.clone(),
                kind: option.kind.clone(),
                label: option.label.clone(),
            })
            .collect();
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StructuredDiplomacyResponse {
    pub selected: Option<CanonicalIntent>,
    pub validation_errors: Vec<String>,
}

/// Future text adapters are translators only: neither method receives game state or transition
/// functions, and bot-to-bot play never needs an implementation of this trait.
pub trait DiplomacyAdapter {
    /// Parse human input into an untrusted structured draft.
    ///
    /// # Errors
    /// Returns a parser-specific message when the input cannot be represented unambiguously.
    fn parse(
        &self,
        context: &DiplomacyPromptContext,
        input: &str,
    ) -> Result<DiplomacyIntent, String>;

    fn render(
        &self,
        context: &DiplomacyPromptContext,
        response: &StructuredDiplomacyResponse,
    ) -> String;
}

/// Validate a requested selection against the engine-generated choice.
///
/// Payload supplied by an adapter is deliberately absent from this API. The engine applies the
/// candidate it stored for this ID inside its resumable window.
#[must_use]
pub fn canonicalize_selected_option(choice: &Choice, option_id: &str) -> IntentValidation {
    if choice.option(option_id).is_some() {
        IntentValidation::Canonical(CanonicalIntent {
            option_id: option_id.to_owned(),
        })
    } else {
        IntentValidation::Illegal {
            reason: "the option id is not present in the current engine choice".to_owned(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ti4_engine::choice::ChoiceOption;

    #[test]
    fn only_an_engine_offered_id_becomes_canonical() {
        let choice = Choice::new(
            PlayerId::new("a"),
            "respond",
            vec![ChoiceOption::new("accept:deal-1:r0", "diplomacy_accept")],
        );
        assert!(matches!(
            canonicalize_selected_option(&choice, "accept:deal-1:r0"),
            IntentValidation::Canonical(CanonicalIntent { option_id })
                if option_id == "accept:deal-1:r0"
        ));
        assert!(matches!(
            canonicalize_selected_option(&choice, "accept:deal-2:r0"),
            IntentValidation::Illegal { .. }
        ));
    }

    #[test]
    fn oversized_or_vague_drafts_have_no_transition_api() {
        fn accepts_untrusted_intent(_intent: DiplomacyIntent) {}
        accepts_untrusted_intent(DiplomacyIntent::Offer(DealDraft {
            target: PlayerId::new("b"),
            proposer_terms: Vec::new(),
            recipient_terms: Vec::new(),
        }));
        // The only state-crossing function in this module requires an engine Choice and ID.
        let gate: fn(&Choice, &str) -> IntentValidation = canonicalize_selected_option;
        let _ = gate;
    }
}
