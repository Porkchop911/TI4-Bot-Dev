//! Data transfer objects for the read-only history splice dry run (H7, phase 1).
//!
//! Nothing here changes a game. A [`SplicePreview`] answers "if the host removed (or changed)
//! decision N and replayed the rest from the same seed, how many later decisions would still fit,
//! and where would the first one that does not fit be?".

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The edit to preview. Cursors are indexes into the live decision log (0 is the first decision).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SpliceEdit {
    /// Take decision `cursor` out of the history.
    Remove { cursor: usize },
    /// Answer decision `cursor` with another option that was on offer at the time.
    Replace { cursor: usize, option_id: String },
}

impl SpliceEdit {
    #[must_use]
    pub const fn cursor(&self) -> usize {
        match self {
            Self::Remove { cursor } | Self::Replace { cursor, .. } => *cursor,
        }
    }
}

/// Why a later decision no longer fits. The first four are the strict matcher's identity checks
/// (actor, prompt, typed context, chosen option on offer); the rest are softer or terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictKind {
    /// The engine asks a different seat than the recorded decision's seat. Hard.
    Actor,
    /// The engine asks a different question (prompt text differs). Hard.
    Prompt,
    /// Same prompt, but the structured context (source, subtype, phase, round, target, ...) differs. Hard.
    Context,
    /// The recorded answer is not among the options now offered. Hard.
    ChosenNotOffered,
    /// The recorded answer is still on offer but the menu is a different set. Soft: the earlier
    /// intent was made against another menu.
    OptionsChanged,
    /// Identical question and menu, but the quantities still owed or available differ. Soft.
    QuantityChanged,
    /// The game finished (or cannot continue) while recorded decisions remain. Hard.
    EngineEnded,
    /// The engine failed while replaying. Hard.
    EngineError,
    /// Turn redo only: a card the original timeline drew for a recorded decision is no longer in
    /// the deck, so that draw cannot give the seat the same card. Hard.
    #[serde(alias = "deck_cursor")]
    ReservedCard,
}

impl ConflictKind {
    #[must_use]
    pub const fn is_soft(self) -> bool {
        matches!(self, Self::OptionsChanged | Self::QuantityChanged)
    }
}

/// The first recorded decision that does not fit the edited history.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpliceConflict {
    /// Index of the recorded decision in the ORIGINAL log.
    pub cursor: usize,
    pub kind: ConflictKind,
    pub soft: bool,
    /// The seat the recorded decision belongs to.
    pub seat: String,
    /// The prompt recorded for that decision.
    pub prompt: String,
    /// What the history had: the recorded choice.
    pub expected_chosen: String,
    /// The options that were on offer when it was recorded.
    pub expected_offered: Vec<String>,
    /// The seat the engine asked instead (when the engine asked anything).
    pub found_seat: Option<String>,
    /// The prompt the engine asked instead.
    pub found_prompt: Option<String>,
    /// The options the engine offers instead.
    pub found_offered: Option<Vec<String>>,
    /// For `options_changed`: ids now on offer that were not, and the reverse.
    pub added: Vec<String>,
    pub removed: Vec<String>,
    pub detail: String,
    /// For a removal: the engine is asking exactly the removed question again (same seat, prompt
    /// and menu), which is why the next recorded decision does not fit. Removing a decision
    /// the engine still generates cannot make it go away.
    pub asks_removed_decision: bool,
}

/// A question the engine settled without asking (single option, never logged) that appears or
/// vanishes because of the edit. These shift the alignment between the log and the engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlignmentChange {
    /// Auto-resolved in the edited run but not in the original.
    Appeared,
    /// Auto-resolved in the original but not in the edited run.
    Vanished,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AlignmentNote {
    pub change: AlignmentChange,
    /// Original cursor: the number of recorded decisions that precede this question.
    pub cursor: usize,
    pub seat: String,
    pub prompt: String,
    pub option_id: String,
}

/// Whether the edit moved the engine's random consumption. Computed in memory only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RngStatus {
    /// Every compared point matches the original run: same dice faces drawn, same deck lengths.
    Neutral,
    /// The edit changed dice faces drawn or deck lengths (see the counters).
    NotNeutral,
    /// Nothing could be compared (for example a conflict before any later decision).
    Unknown,
}

/// What the observable randomness consumers looked like at one point (or a difference of two).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RngCounters {
    /// Entries in the dice roll history.
    pub dice_rolls: i64,
    /// Individual die faces drawn from the dice stream (a reroll counts the dice it replaced).
    pub dice_faces: i64,
    /// Remaining cards per deck (objectives, relics, agendas, action cards, secrets, exploration decks).
    pub decks: BTreeMap<String, i64>,
}

impl RngCounters {
    #[must_use]
    pub fn is_zero(&self) -> bool {
        self.dice_rolls == 0 && self.dice_faces == 0 && self.decks.values().all(|v| *v == 0)
    }
}

/// An informational neutrality indicator. This is NOT the P2 guarantee: it reads only public
/// engine surfaces (dice history, deck lengths), not per-domain stream positions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RngIndicator {
    pub status: RngStatus,
    /// How many points (decisions after the edit) were compared against the original run.
    pub compared: usize,
    /// For a removal: what the removed decision consumed in the original run (a delta).
    pub removed_consumed: Option<RngCounters>,
    /// Original cursor of the first compared point whose counters differ.
    pub first_divergence_cursor: Option<usize>,
    /// The counter difference there: edited run minus original run.
    pub first_divergence: Option<RngCounters>,
}

/// The dry-run report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SplicePreview {
    pub edit: SpliceEdit,
    /// Decisions in the original (live) log.
    pub original_decisions: usize,
    /// Decisions after the edited one in the original log.
    pub later_decisions: usize,
    /// How many of those were replayed exactly and still fit.
    pub kept_later: usize,
    /// `later_decisions - kept_later`: what a commit would drop from the end.
    pub dropped_later: usize,
    /// True when every later decision survived and nothing was dropped.
    pub survives_to_end: bool,
    /// Original cursors whose menu has the same options in another order (kept, noted).
    pub kept_reordered: Vec<usize>,
    pub first_conflict: Option<SpliceConflict>,
    /// Single-option questions that appeared or vanished (cursor alignment).
    pub alignment: Vec<AlignmentNote>,
    pub rng: RngIndicator,
}

/// Request body for `POST /api/games/{id}/history/splice-preview`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SplicePreviewRequest {
    pub edit: SpliceEdit,
}
