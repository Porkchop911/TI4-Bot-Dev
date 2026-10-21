//! Seat control, pending manual choices, choice fingerprints, replay records, provenance.
//!
//! These are the pieces a human sits between: the engine asks a question, the replayer decides
//! whether the learned policy answers it or a person does, and the answer is recorded so the same
//! line can be rebuilt later. Everything here is a plain value or a small state machine over plain
//! values — no engine call, no I/O, no thread — because the same definitions have to serve the
//! simulation thread, the project file, and the branch tree.
//!
//! Two facts about the engine shape this vocabulary, and are worth knowing before reading it:
//!
//! * One `Game::step()` can raise several decisions. A measured reference session put 670 settled
//!   decisions across 602 engine steps, with one production step consuming eight. So a pending
//!   choice carries an `ask` index within its frame, not just a frame number.
//! * `Game::legal_options()` cannot see every decision a step will ask — the open transaction
//!   window in particular is dispatched before it. The decorator that actually gets asked is
//!   therefore the authoritative pause trigger, and these types never assume otherwise.

// `SeatControl`/`ManualControl`/`ControlError` are the names callers should see; renaming them to
// dodge a naming-shape lint would only make the API harder to read from a call site.
#![allow(clippy::module_name_repetitions)]

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{self, Write as _};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use thiserror::Error;
use ti4_engine::choice::{Choice, ChoiceOption};
use ti4_model::id::{FactionId, PlayerId};

/// Version tag mixed into every choice fingerprint.
///
/// Fingerprints outlive the build that wrote them — they are stored in project files and compared
/// again on import — so the pre-image format is versioned rather than implicitly trusted.
pub const CHOICE_FINGERPRINT_VERSION: &str = "r02-choice-v1";

/// Branches per project, source timeline included.
///
/// Inherited from the R02 plan: a branch is a full replayed timeline, so an unbounded tree is an
/// unbounded replay cost. Refusing is better than evicting a branch somebody is looking at.
pub const MAX_BRANCHES_PER_PROJECT: u32 = 128;

/// Ceiling on the options of one offered choice.
///
/// The widest real decision in the engine is a structured-contact or payment offer, far below
/// this. The cap exists so a corrupt or misbuilt choice cannot allocate an unbounded panel.
pub const MAX_OFFERED_OPTIONS: usize = 1_024;

/// Why a control or replay value could not be built.
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum ControlError {
    #[error("a project holds at most {MAX_BRANCHES_PER_PROJECT} branches")]
    TooManyBranches,
    #[error("a choice offers at most {MAX_OFFERED_OPTIONS} options, this one offers {found}")]
    TooManyOptions {
        /// Options the rejected choice carried.
        found: usize,
    },
    #[error("a manual choice must offer at least one option")]
    EmptyChoice,
    #[error("{chosen:?} was not one of the offered options")]
    OptionNotOffered {
        /// The rejected option id.
        chosen: String,
    },
}

/// A timeline inside a replayer project. Naming is presentation; this index is identity.
///
/// Ids are monotonic per project and never reused, so a child branch cannot be mistaken for a
/// sibling that was replaced.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct BranchId(u32);

impl BranchId {
    /// Identity of the imported source timeline, which every first-level child names as parent.
    pub const SOURCE: Self = Self(0);

    #[must_use]
    pub const fn new(index: u32) -> Self {
        Self(index)
    }

    #[must_use]
    pub const fn index(self) -> u32 {
        self.0
    }
}

impl fmt::Display for BranchId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "branch-{}", self.0)
    }
}

/// Monotonic branch id allocator for one project.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BranchIds {
    next: u32,
}

impl BranchIds {
    /// An allocator whose source timeline already occupies id 0.
    #[must_use]
    pub const fn fresh() -> Self {
        Self { next: 1 }
    }

    /// Rebuild the allocator after loading a project, from one past its highest used id.
    #[must_use]
    pub const fn from_next(next: u32) -> Self {
        Self { next }
    }

    #[must_use]
    pub const fn next(&self) -> u32 {
        self.next
    }

    /// Allocate the next branch id, refusing to exceed the project's branch bound.
    ///
    /// # Errors
    /// [`ControlError::TooManyBranches`] once the project is full. Existing ids stay valid.
    pub fn allocate(&mut self) -> Result<BranchId, ControlError> {
        if self.next >= MAX_BRANCHES_PER_PROJECT {
            return Err(ControlError::TooManyBranches);
        }
        let id = BranchId(self.next);
        self.next += 1;
        Ok(id)
    }
}

/// How a physical seat's next unresolved choice gets answered.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SeatMode {
    /// The learned policy answers, as in `ti4-review`.
    #[default]
    Auto,
    /// Pause before the seat's next choice so a person can answer it.
    Manual,
}

/// Persistent control mode per physical seat, plus queued one-shot delegations.
///
/// A seat is `Auto` until something sets it, which keeps an imported game with no recorded modes
/// behaviourally identical to the reviewer.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SeatControl {
    modes: BTreeMap<PlayerId, SeatMode>,
    delegated: BTreeSet<PlayerId>,
}

impl SeatControl {
    #[must_use]
    pub fn all_auto() -> Self {
        Self::default()
    }

    /// A table from recorded mode changes only; seats not listed remain `Auto`.
    pub fn from_changes<I, S>(changes: I) -> Self
    where
        I: IntoIterator<Item = (S, SeatMode)>,
        S: AsRef<str>,
    {
        Self {
            modes: changes
                .into_iter()
                .map(|(seat, mode)| (PlayerId::new(seat.as_ref()), mode))
                .collect(),
            delegated: BTreeSet::new(),
        }
    }

    #[must_use]
    pub fn mode(&self, seat: &PlayerId) -> SeatMode {
        self.modes.get(seat).copied().unwrap_or_default()
    }

    #[must_use]
    pub fn is_manual(&self, seat: &PlayerId) -> bool {
        self.mode(seat) == SeatMode::Manual
    }

    /// Set one seat's mode, returning the mode it had before.
    ///
    /// Choosing `Auto` also drops any queued one-shot delegation for that seat: a delegation
    /// answers one decision *for a manual seat*, and a seat that is no longer manual has nothing
    /// left to delegate. Setting `Manual` deliberately keeps a delegation already queued.
    pub fn set_mode(&mut self, seat: &PlayerId, mode: SeatMode) -> SeatMode {
        if mode == SeatMode::Auto {
            self.delegated.remove(seat);
        }
        self.modes.insert(seat.clone(), mode).unwrap_or_default()
    }

    /// Flip one seat and report the mode it now has. Seats are independent: touching one never
    /// rewrites another entry.
    pub fn toggle(&mut self, seat: &PlayerId) -> SeatMode {
        let next = if self.is_manual(seat) {
            SeatMode::Auto
        } else {
            SeatMode::Manual
        };
        self.set_mode(seat, next);
        next
    }

    /// Record that the human is letting the policy answer this seat's next decision.
    pub fn delegate_once(&mut self, seat: &PlayerId) {
        self.delegated.insert(seat.clone());
    }

    /// Consume a queued delegation, reporting whether one was there.
    ///
    /// One-shot by construction: a delegation never survives the decision it was made for, and the
    /// seat stays `Manual` for the decision after it.
    pub fn take_delegation(&mut self, seat: &PlayerId) -> bool {
        self.delegated.remove(seat)
    }

    #[must_use]
    pub fn has_delegation(&self, seat: &PlayerId) -> bool {
        self.delegated.contains(seat)
    }

    /// Seats currently in `Manual`, in stable seat order.
    pub fn manual_seats(&self) -> impl Iterator<Item = &PlayerId> {
        self.modes
            .iter()
            .filter(|(_, mode)| **mode == SeatMode::Manual)
            .map(|(seat, _)| seat)
    }

    /// Recorded mode changes, for the project file's mode-change log.
    #[must_use]
    pub fn changes(&self) -> Vec<(PlayerId, SeatMode)> {
        self.modes.clone().into_iter().collect()
    }
}

/// A hash of exactly what a human was asked to decide.
///
/// It binds the actor, the prompt, the ordered `(id, kind)` pairs on offer, and the typed context.
/// It deliberately does *not* bind branch or frame identity, labels, payloads, previews, scores or
/// probabilities: those are presentation or provenance around the same decision, and re-scoring an
/// identical choice must not invalidate the answer a person already clicked. Excluding frame
/// identity is what lets a recorded answer still identify the choice a rebuild offers it again.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ChoiceFingerprint(String);

/// One occurrence of an offer, as distinct from every other — including offers that look identical.
///
/// [`ChoiceFingerprint`] says what an offer *is*: actor, prompt, ordered options and typed context.
/// It deliberately says nothing about *which* occurrence, because a rebuild has to match a recorded
/// answer to the offer it was made against. But the engine does ask identical-looking questions in
/// succession: a seat six units over capacity is asked "remove a unit: over capacity in 14" six times
/// in one engine step, each time with the single option `remove|0` and no context. The fingerprint
/// cannot say which of those a click was aimed at, and a click aimed at the first was accepted
/// against the second and consumed it (reproduced in review, 2026-09-30,
/// `examples/astra_stale_submission_20260930.rs`).
///
/// Allocated from a process-wide counter when the offer is constructed and never reused within a
/// process, so a new ask, a new branch and a new table all get fresh ones: a stale click cannot land
/// on a later offer however alike the two look. It is captured by whoever *draws* the offer and
/// carried by the click, and it is checked under the gate's lock. It is not part of the fingerprint
/// and plays no part in replay matching. Zero is never allocated.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct OfferId(u64);

impl OfferId {
    /// The next unused occurrence id in this process.
    fn next() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        Self(NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed))
    }

    /// The raw value, for logs and diagnostics.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

impl fmt::Display for OfferId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "offer#{}", self.0)
    }
}

impl ChoiceFingerprint {
    /// Fingerprint an offer from its identity-bearing parts.
    ///
    /// Every field is length-prefixed, so a prompt that happens to contain an option id cannot
    /// produce the same pre-image as a different choice whose fields happen to line up that way.
    #[must_use]
    pub fn compute(
        actor: &PlayerId,
        prompt: &str,
        options: &[OfferedOption],
        context: Option<&Value>,
    ) -> Self {
        let mut preimage = String::new();
        let _ = writeln!(preimage, "{CHOICE_FINGERPRINT_VERSION}");
        write_field(&mut preimage, actor.as_str());
        write_field(&mut preimage, prompt);
        let _ = writeln!(preimage, "options {}", options.len());
        for option in options {
            write_field(&mut preimage, &option.id);
            write_field(&mut preimage, &option.kind);
        }
        match context {
            None => preimage.push_str("context -\n"),
            // `serde_json` orders object keys and the engine's context types use declaration
            // order plus `BTreeMap`s, so this serialization is stable for a given choice.
            Some(value) => write_field(&mut preimage, &canonical_json(value)),
        }
        Self(hex_sha256(preimage.as_bytes()))
    }

    /// Fingerprint exactly what the engine offered, as a caller that will not rescore it sees it.
    ///
    /// The manual panel and the decider that accepts the answer must key it identically, so both
    /// derive it here: same option order, same context projection, one recipe. A test asserts the
    /// two paths agree.
    #[must_use]
    pub fn from_choice(choice: &Choice) -> Self {
        let options: Vec<OfferedOption> = choice.options.iter().map(OfferedOption::from).collect();
        let context = choice
            .context
            .as_ref()
            // Display-only fields (the reaction trigger) are not part of what a replay binds.
            .and_then(|context| serde_json::to_value(context.without_display_fields()).ok());
        Self::compute(&choice.player, &choice.prompt, &options, context.as_ref())
    }

    #[must_use]
    pub fn as_hex(&self) -> &str {
        &self.0
    }
}

fn write_field(sink: &mut String, value: &str) {
    let _ = writeln!(sink, "{}:{}", value.len(), value);
}

fn canonical_json(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|error| format!("!{error}"))
}

fn hex_sha256(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// One option as offered to a human: what it is, what it said, and what the policy thought.
///
/// Labels, payloads, previews and scores ride along for the panel and are excluded from the
/// fingerprint. `score`/`probability` are filled by the policy trace wrapper, so a human choice
/// shows the same numbers the bot would have seen.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OfferedOption {
    /// Stable engine option id — the thing a submission names.
    pub id: String,
    /// Engine option kind, fingerprinted with the id because kind changes what taking it means.
    pub kind: String,
    /// Human-readable label from the R01 projection.
    pub label: String,
    /// Structured detail the engine will apply.
    pub payload: BTreeMap<String, Value>,
    /// Serialized outcome preview, when the engine could compute one.
    pub preview: Option<Value>,
    /// Policy score for this option, when a policy scored the choice.
    pub score: Option<f64>,
    /// Sampling probability for this option, when a policy scored the choice.
    pub probability: Option<f64>,
}

impl From<&ChoiceOption> for OfferedOption {
    fn from(option: &ChoiceOption) -> Self {
        Self {
            id: option.id.clone(),
            kind: option.kind.clone(),
            label: option.label.clone(),
            payload: option.payload.clone(),
            preview: option
                .preview
                .as_ref()
                .and_then(|preview| serde_json::to_value(preview).ok()),
            score: None,
            probability: None,
        }
    }
}

impl OfferedOption {
    /// Attach the policy's numbers for this option. Presentation only: the fingerprint is unchanged.
    #[must_use]
    pub fn with_scores(mut self, score: Option<f64>, probability: Option<f64>) -> Self {
        self.score = score;
        self.probability = probability;
        self
    }
}

/// The choice a manual seat is being asked to answer right now.
///
/// `offer` says *which occurrence* this is, and is what a click must carry to be accepted; `branch`,
/// `frame` and `ask` say where the panel belongs, for display; the fingerprint says what the offer
/// is, for replay. The three are kept apart on purpose — see [`OfferId`] and [`ChoiceFingerprint`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PendingManualChoice {
    /// This occurrence, as distinct from every other offer in the process.
    pub offer: OfferId,
    /// Branch whose engine is paused.
    pub branch: BranchId,
    /// Frame the pause happened at — the step boundary the engine is stopped on.
    pub frame: u64,
    /// Which ask within that frame, counting from 0.
    ///
    /// One engine step can raise several choices at once (a production step in the measured
    /// reference session carried eight). They share a frame and differ only here, which is what
    /// lets a nested prompt be told apart from a repeat of the same frame.
    pub ask: u32,
    /// Physical seat being asked.
    pub actor: PlayerId,
    /// Faction for presentation; never used to decide control.
    pub faction: Option<FactionId>,
    pub prompt: String,
    /// Options in the engine's own order. Panel order and the fingerprint both follow it.
    pub options: Vec<OfferedOption>,
    /// Serialized typed decision context, display-only fields stripped: this is what the
    /// fingerprint and the replay record bind.
    pub context: Option<Value>,
    pub fingerprint: ChoiceFingerprint,
    /// The question's display-only facts (`Choice::details`): pools, fleet supply, phrases, offer
    /// card. For the panel only; excluded from the fingerprint and from records.
    #[serde(default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub details: serde_json::Map<String, Value>,
    /// The full decision context, with the reaction-card previews and the pending effect that
    /// `context` strips. For the panel only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display: Option<ti4_engine::decision_context::DecisionContext>,
}

impl PendingManualChoice {
    /// Wrap an engine choice for display and later validation.
    ///
    /// # Errors
    /// [`ControlError::EmptyChoice`] or [`ControlError::TooManyOptions`] for a degenerate offer.
    pub fn new(
        branch: BranchId,
        frame: u64,
        ask: u32,
        faction: Option<FactionId>,
        choice: &Choice,
    ) -> Result<Self, ControlError> {
        if choice.options.is_empty() {
            return Err(ControlError::EmptyChoice);
        }
        if choice.options.len() > MAX_OFFERED_OPTIONS {
            return Err(ControlError::TooManyOptions {
                found: choice.options.len(),
            });
        }
        let options: Vec<OfferedOption> = choice.options.iter().map(OfferedOption::from).collect();
        let context = choice
            .context
            .as_ref()
            // Display-only fields (the reaction trigger) are not part of what a replay binds.
            .and_then(|context| serde_json::to_value(context.without_display_fields()).ok());
        let fingerprint = ChoiceFingerprint::from_choice(choice);
        Ok(Self {
            offer: OfferId::next(),
            branch,
            frame,
            ask,
            actor: choice.player.clone(),
            faction,
            prompt: choice.prompt.clone(),
            options,
            context,
            fingerprint,
            details: choice.details.clone(),
            display: choice.context.clone(),
        })
    }

    /// The display-only lines under this question's prompt: the engine's details (pools, fleet
    /// supply, capacity, offer card, phrased prompt) and what the played card will do when it
    /// resolves. Both panels (local and remote) take them from here, so neither can drop them.
    #[must_use]
    pub fn question_lines(&self, session: &ti4_review::ReviewSession) -> Vec<String> {
        let mut lines = ti4_review::view::details_lines(session, &self.prompt, &self.details);
        if let Some(said) = self
            .pending_effect()
            .and_then(|effect| ti4_review::view::say_effect(session, effect))
        {
            lines.push(format!("Pending effect: {said}"));
        }
        lines
    }

    /// The reaction-card previews of this offer (display only).
    #[must_use]
    pub fn previews(&self) -> &[ti4_engine::decision_context::EffectPreview] {
        self.display.as_ref().map_or(&[], |display| &display.previews)
    }

    /// What the played card this window reacts to will do when it resolves (display only).
    #[must_use]
    pub fn pending_effect(&self) -> Option<&ti4_engine::decision_context::EffectPreview> {
        self.display.as_ref()?.pending_effect.as_ref()
    }

    #[must_use]
    pub fn offers(&self, option_id: &str) -> bool {
        self.options.iter().any(|option| option.id == option_id)
    }

    /// Offered ids in offered order, for diagnostics and rebuild comparison.
    #[must_use]
    pub fn ids(&self) -> Vec<&str> {
        self.options
            .iter()
            .map(|option| option.id.as_str())
            .collect()
    }

    /// Check a submission against this exact offer: the same occurrence and the same shape, then an
    /// offered id.
    ///
    /// The occurrence is what matters. A click aimed at an earlier offer is stale even when that
    /// offer looked exactly like this one — which is precisely the case the fingerprint cannot see.
    fn validate(&self, submission: &ManualSubmission) -> SubmitOutcome {
        if submission.offer != self.offer || submission.fingerprint != self.fingerprint {
            return SubmitOutcome::Stale {
                current: self.fingerprint.clone(),
            };
        }
        if self.offers(&submission.option_id) {
            SubmitOutcome::Accepted {
                option_id: submission.option_id.clone(),
            }
        } else {
            SubmitOutcome::NotOffered {
                offered: self.ids().into_iter().map(str::to_owned).collect(),
            }
        }
    }
}

/// A human's answer, bound to the choice it was aimed at.
///
/// Build it with [`ManualSubmission::to`] from the offer that was *drawn*. Capturing the occurrence
/// when the panel is shown, rather than reading whatever is pending when the click arrives, is the
/// whole point: by then a newer, identical-looking offer may be on screen, and binding to it would
/// answer a question the person never saw.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ManualSubmission {
    /// The occurrence the click was made on.
    pub offer: OfferId,
    pub fingerprint: ChoiceFingerprint,
    pub option_id: String,
}

impl ManualSubmission {
    /// A click on `option_id` of the offer on screen, bound to that exact occurrence.
    #[must_use]
    pub fn to(offer: &PendingManualChoice, option_id: impl Into<String>) -> Self {
        Self {
            offer: offer.offer,
            fingerprint: offer.fingerprint.clone(),
            option_id: option_id.into(),
        }
    }
}

/// What happened to a submission. Every refusal leaves the pending choice in place.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SubmitOutcome {
    /// The exact offered option for the choice on screen.
    Accepted {
        /// Option id to hand to the engine's own validation.
        option_id: String,
    },
    /// Nothing is waiting for a human.
    NoPendingChoice,
    /// The panel it aimed at is gone — the choice changed, or this was an old click.
    Stale {
        /// Fingerprint of the choice actually pending, so the panel can redraw.
        current: ChoiceFingerprint,
    },
    /// Right choice, invented option.
    NotOffered {
        /// What was actually on offer, in order.
        offered: Vec<String>,
    },
    /// Already answered; the first answer consumed the pending choice.
    Duplicate,
}

/// Control state of one live branch: seat modes, the pending choice, and what was last answered.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ManualControl {
    seats: SeatControl,
    pending: Option<PendingManualChoice>,
    /// The last occurrence answered. Keyed on [`OfferId`], not the fingerprint: identical-looking
    /// offers in succession are separate questions, and must not read as already answered.
    answered: Option<OfferId>,
}

impl ManualControl {
    pub fn new(seats: SeatControl) -> Self {
        Self {
            seats,
            pending: None,
            answered: None,
        }
    }

    #[must_use]
    pub const fn seats(&self) -> &SeatControl {
        &self.seats
    }

    #[must_use]
    pub const fn pending(&self) -> Option<&PendingManualChoice> {
        self.pending.as_ref()
    }

    #[must_use]
    pub const fn requires_human(&self) -> bool {
        self.pending.is_some()
    }

    /// Show a choice to the human, replacing any panel still on screen.
    ///
    /// Replacing rather than queueing: there is one engine and one panel, so a superseded offer is
    /// not also kept around to be clicked later.
    pub fn publish(&mut self, choice: PendingManualChoice) {
        self.pending = Some(choice);
    }

    /// Accept a human answer, returning the accepted option id for the engine to validate.
    ///
    /// While an offer is up, a click for any other occurrence is [`SubmitOutcome::Stale`] and consumes
    /// nothing — including a click aimed at an earlier offer that looked exactly like the current one.
    /// `Stale` is preferred to `Duplicate` whenever something is pending because it names the current
    /// offer, which is what lets a panel that drew an old question redraw the new one. `Duplicate` is
    /// for the case with nothing up: a second click on the occurrence just answered.
    pub fn submit(&mut self, submission: &ManualSubmission) -> SubmitOutcome {
        let Some(pending) = self.pending.as_ref() else {
            return if self.answered == Some(submission.offer) {
                SubmitOutcome::Duplicate
            } else {
                SubmitOutcome::NoPendingChoice
            };
        };
        let outcome = pending.validate(submission);
        if let SubmitOutcome::Accepted { .. } = outcome {
            self.answered = Some(pending.offer);
            self.pending = None;
        }
        outcome
    }

    /// Answer the pending choice with the policy instead, keeping the seat manual.
    ///
    /// Only for the occurrence named: a delegation made on one panel must not land on a later one
    /// that happened to replace it. Returns the seat whose panel just closed, or `None` when that
    /// offer is not the one waiting — nothing is delegated and the pending offer stays up.
    pub fn delegate_pending_once(&mut self, offer: OfferId) -> Option<PlayerId> {
        if self.pending.as_ref().map(|pending| pending.offer) != Some(offer) {
            return None;
        }
        let actor = self.pending.take().map(|pending| pending.actor)?;
        self.seats.delegate_once(&actor);
        Some(actor)
    }

    /// Set a seat's mode, reporting whether that let its own waiting choice go to the bot.
    ///
    /// Only the pending actor's own seat can release a panel: changing any other seat leaves it up
    /// unchanged.
    pub fn set_mode(&mut self, seat: &PlayerId, mode: SeatMode) -> ModeEffect {
        let previous = self.seats.set_mode(seat, mode);
        let released = if mode == SeatMode::Auto
            && self
                .pending
                .as_ref()
                .is_some_and(|pending| &pending.actor == seat)
        {
            self.pending.take().map(|pending| pending.fingerprint)
        } else {
            None
        };
        match released {
            Some(fingerprint) => ModeEffect::ReleasedBot {
                previous,
                choice: fingerprint,
            },
            None if previous == mode => ModeEffect::Unchanged,
            None => ModeEffect::Changed { previous },
        }
    }

    /// Drop a pending panel without answering it (cancel at a step boundary).
    pub fn clear_pending(&mut self) -> Option<PendingManualChoice> {
        self.pending.take()
    }

    /// Consume a one-shot delegation for a seat, if it has one.
    pub fn take_delegation(&mut self, seat: &PlayerId) -> bool {
        self.seats.take_delegation(seat)
    }

    /// Queue a one-shot delegation for a seat *without* closing a panel.
    ///
    /// [`Self::delegate_pending_once`] is the normal "let the bot have this one" click, made while a
    /// panel is open. This is the same decision made a moment early — the human pressed the button
    /// before the engine reached the ask — and it must not be lost, nor may it survive the decision
    /// it was made for.
    pub fn delegate_seat_once(&mut self, seat: &PlayerId) {
        self.seats.delegate_once(seat);
    }
}

/// What a seat-mode change did to a waiting choice.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ModeEffect {
    /// The seat already had that mode.
    Unchanged,
    /// The mode changed and no panel was affected.
    Changed {
        /// Mode the seat had before.
        previous: SeatMode,
    },
    /// The mode changed and this seat's own panel went to the bot.
    ReleasedBot {
        /// Mode the seat had before.
        previous: SeatMode,
        /// Fingerprint of the choice the bot now answers.
        choice: ChoiceFingerprint,
    },
}

/// Who answered a decision, recorded per choice and shown in the timeline.
#[derive(
    Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum Provenance {
    /// The learned policy answered.
    #[default]
    Policy,
    /// A person clicked an offered option.
    Human,
    /// The panel offered a choice and the human let the policy answer it.
    DelegatedToPolicy,
    /// A recorded answer replayed while rebuilding a branch to its target frame.
    ReplayPrefix,
}

impl Provenance {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Policy => "policy",
            Self::Human => "human",
            Self::DelegatedToPolicy => "delegated_to_policy",
            Self::ReplayPrefix => "replay_prefix",
        }
    }
}

/// One settled decision: what was offered, what was chosen, and who chose it.
///
/// This is the authoritative replay record. It is not the reviewer's decision trace, which also
/// carries synthetic rows (fleet packages, plan-stop notices) that were never engine decisions and
/// must never be replayed as if they had been.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ReplayRecord {
    pub branch: BranchId,
    pub frame: u64,
    pub ask: u32,
    pub actor: PlayerId,
    pub faction: Option<FactionId>,
    pub prompt: String,
    /// Offered ids in engine order.
    pub offered: Vec<String>,
    pub chosen: String,
    pub context: Option<Value>,
    pub fingerprint: ChoiceFingerprint,
    pub provenance: Provenance,
}

impl ReplayRecord {
    /// Record the answer given to a pending choice.
    ///
    /// # Errors
    /// [`ControlError::OptionNotOffered`] when `chosen` was not on offer. A record of an answer the
    /// engine would have refused is worse than no record, because rebuilding from it would look
    /// like a legal prefix.
    pub fn record(
        pending: &PendingManualChoice,
        chosen: &str,
        provenance: Provenance,
    ) -> Result<Self, ControlError> {
        if !pending.offers(chosen) {
            return Err(ControlError::OptionNotOffered {
                chosen: chosen.to_owned(),
            });
        }
        Ok(Self {
            branch: pending.branch,
            frame: pending.frame,
            ask: pending.ask,
            actor: pending.actor.clone(),
            faction: pending.faction.clone(),
            prompt: pending.prompt.clone(),
            offered: pending.ids().into_iter().map(str::to_owned).collect(),
            chosen: chosen.to_owned(),
            context: pending.context.clone(),
            fingerprint: pending.fingerprint.clone(),
            provenance,
        })
    }

    /// Whether a freshly offered choice is the one this record answers.
    ///
    /// Rebuild compares identity rather than trusting position: an engine that offers the same
    /// decision again matches, and anything else fails at the first difference.
    #[must_use]
    pub fn matches(&self, pending: &PendingManualChoice) -> bool {
        self.fingerprint == pending.fingerprint
            && self.actor == pending.actor
            && self.offered == pending.ids()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ti4_engine::decision_context::{DecisionContext, DecisionSource};
    use ti4_model::state::Phase;

    fn seat(name: &str) -> PlayerId {
        PlayerId::new(name)
    }

    #[test]
    fn a_reaction_trigger_never_changes_a_choice_fingerprint() {
        // The trigger is display metadata derived from the event: an old replay of the same
        // window, written before triggers existed, must keep matching.
        let context = || {
            DecisionContext::new(
                seat("b"),
                DecisionSource::Reaction("ACTION_CARD_PLAYED".to_owned()),
                "reaction_when_ACTION_CARD_PLAYED",
                Phase::Action,
                1,
            )
        };
        let event = ti4_engine::event::Event::new(
            3,
            "ACTION_CARD_PLAYED",
            [("player".to_owned(), "a".into())].into_iter().collect(),
        );
        let plain = Choice::new(seat("b"), "when X", offered(&[("decline", "decline")]))
            .contextualized(context());
        let with = Choice::new(seat("b"), "when X", offered(&[("decline", "decline")]))
            .contextualized(context().with_trigger(
                ti4_engine::decision_context::DecisionTrigger::from_event(&event, "when", &[]),
            ));
        assert_eq!(
            ChoiceFingerprint::from_choice(&plain),
            ChoiceFingerprint::from_choice(&with)
        );
    }

    fn offered(ids: &[(&str, &str)]) -> Vec<ChoiceOption> {
        ids.iter()
            .map(|(id, kind)| ChoiceOption::labelled(*id, *kind, format!("label {id}")))
            .collect()
    }

    fn choice(actor: &str, prompt: &str, ids: &[(&str, &str)]) -> Choice {
        Choice::new(seat(actor), prompt, offered(ids))
    }

    /// A choice with more options than the offer bound allows.
    fn owned_choice(options: usize) -> Choice {
        Choice {
            player: seat("seat0"),
            prompt: "choose".to_owned(),
            options: (0..options)
                .map(|index| ChoiceOption::new(format!("opt{index}"), "action"))
                .collect(),
            context: None,
            details: serde_json::Map::new(),
            free_form: None,
        }
    }

    fn pending(actor: &str, ids: &[(&str, &str)]) -> PendingManualChoice {
        PendingManualChoice::new(
            BranchId::SOURCE,
            1,
            0,
            None,
            &choice(actor, "choose an action", ids),
        )
        .expect("test choices are well formed")
    }

    fn submission(pending: &PendingManualChoice, option: &str) -> ManualSubmission {
        ManualSubmission {
            offer: pending.offer,
            fingerprint: pending.fingerprint.clone(),
            option_id: option.to_owned(),
        }
    }

    #[test]
    fn fingerprint_is_stable_across_rebuilds_of_the_same_offer() {
        let ids = [("tactical", "action"), ("pass", "action")];
        let first = pending("seat0", &ids);
        let second = pending("seat0", &ids);
        assert_eq!(first.fingerprint, second.fingerprint);
        assert_eq!(first.fingerprint.as_hex().len(), 64);
        // And the complement, which is the whole reason `OfferId` exists: the same shape twice is two
        // occurrences. Replay matches on the first property; a click is bound by the second.
        assert_ne!(
            first.offer, second.offer,
            "identical offers are still distinct occurrences"
        );
    }

    // ---- Occurrence identity (review 2026-09-30, P1). Every case below uses two offers that are
    // identical in shape - same actor, prompt, options and context, so the same fingerprint - which
    // is exactly what the engine produces for a seat over capacity asked "remove a unit" repeatedly.

    /// Regressions 1-3: the second of two identical offers is answerable; replaying the first click
    /// against it is refused and consumes nothing; a fresh click on it is accepted.
    #[test]
    fn an_old_click_does_not_answer_the_next_identical_offer() {
        let ids = [("remove|0", "remove")];
        let first = pending("seat0", &ids);
        let second = pending("seat0", &ids);
        assert_eq!(
            first.fingerprint, second.fingerprint,
            "the collision is real"
        );
        let old_click = submission(&first, "remove|0");

        let mut control = ManualControl::new(SeatControl::all_auto());
        control.publish(first);
        assert_eq!(
            control.submit(&old_click),
            SubmitOutcome::Accepted {
                option_id: "remove|0".to_owned()
            }
        );

        control.publish(second.clone());
        let replayed = control.submit(&old_click);
        assert!(
            matches!(
                replayed,
                SubmitOutcome::Duplicate | SubmitOutcome::Stale { .. }
            ),
            "a click made on the first offer must not answer the second: {replayed:?}"
        );
        assert_eq!(
            control.pending().map(|pending| pending.offer),
            Some(second.offer),
            "and the refused click consumed nothing - the second offer is still up"
        );

        assert_eq!(
            control.submit(&submission(&second, "remove|0")),
            SubmitOutcome::Accepted {
                option_id: "remove|0".to_owned()
            },
            "a click made on the second offer answers it"
        );
        assert!(control.pending().is_none());
    }

    /// A click for an offer answered *two* questions ago is not the last answered, so it is not a
    /// duplicate — it has to be refused as stale instead, still without consuming the current one.
    #[test]
    fn an_older_click_is_stale_and_consumes_nothing() {
        let ids = [("remove|0", "remove")];
        let (first, second, third) = (
            pending("seat0", &ids),
            pending("seat0", &ids),
            pending("seat0", &ids),
        );
        let first_click = submission(&first, "remove|0");
        let mut control = ManualControl::new(SeatControl::all_auto());
        for (offer, click) in [
            (first, first_click.clone()),
            (second.clone(), submission(&second, "remove|0")),
        ] {
            control.publish(offer);
            assert!(matches!(
                control.submit(&click),
                SubmitOutcome::Accepted { .. }
            ));
        }
        control.publish(third.clone());
        assert!(
            matches!(control.submit(&first_click), SubmitOutcome::Stale { .. }),
            "two offers late, the first click is stale"
        );
        assert_eq!(
            control.pending().map(|pending| pending.offer),
            Some(third.offer)
        );
    }

    /// Regression 4: a click from an earlier table cannot land on a new table's offer, even when the
    /// new table asks exactly the same question. A branch id alone would not be enough here: a new
    /// table starts its branch numbering again.
    #[test]
    fn a_click_from_a_replaced_table_is_refused() {
        let ids = [("tactical", "action"), ("pass", "action")];
        let old_table_offer = pending("seat0", &ids);
        let old_click = submission(&old_table_offer, "pass");

        let mut new_table = ManualControl::new(SeatControl::all_auto());
        let new_offer = pending("seat0", &ids);
        assert_eq!(
            new_offer.branch, old_table_offer.branch,
            "same branch numbering"
        );
        assert_eq!(
            new_offer.fingerprint, old_table_offer.fingerprint,
            "same shape"
        );
        new_table.publish(new_offer.clone());

        assert!(
            matches!(new_table.submit(&old_click), SubmitOutcome::Stale { .. }),
            "the old table's click is refused"
        );
        assert_eq!(
            new_table.pending().map(|pending| pending.offer),
            Some(new_offer.offer)
        );
    }

    /// Delegation is bound to the occurrence too: the policy answers the offer the button was drawn
    /// on, and a delegation aimed at an earlier offer does not take over a later identical one.
    #[test]
    fn a_delegation_for_an_old_offer_does_not_delegate_the_new_one() {
        let actor = seat("seat3");
        let ids = [("remove|0", "remove")];
        let first = pending("seat3", &ids);
        let second = pending("seat3", &ids);
        let mut control =
            ManualControl::new(SeatControl::from_changes([("seat3", SeatMode::Manual)]));

        control.publish(first.clone());
        assert!(matches!(
            control.submit(&submission(&first, "remove|0")),
            SubmitOutcome::Accepted { .. }
        ));
        control.publish(second.clone());

        assert_eq!(
            control.delegate_pending_once(first.offer),
            None,
            "a delegation drawn on the first offer is refused"
        );
        assert_eq!(
            control.pending().map(|pending| pending.offer),
            Some(second.offer),
            "and nothing was taken"
        );
        assert!(
            !control.seats().has_delegation(&actor),
            "and no one-shot delegation was left queued behind it"
        );

        assert_eq!(control.delegate_pending_once(second.offer), Some(actor));
        assert!(control.pending().is_none());
    }

    #[test]
    fn fingerprint_is_insensitive_to_labels_scores_and_frames() {
        let ids = [("tactical", "action")];
        let base = pending("seat0", &ids);
        let mut relabelled = base.clone();
        for option in &mut relabelled.options {
            option.label = "something else".to_owned();
            option.score = Some(1.5);
            option.probability = Some(0.25);
            option.payload.insert("extra".to_owned(), Value::from(7));
        }
        relabelled.branch = BranchId::new(9);
        relabelled.frame = 4_242;
        relabelled.ask = 3;
        assert_eq!(base.fingerprint, relabelled.fingerprint);
    }

    #[test]
    fn fingerprint_changes_with_actor_prompt_option_order_option_kind_and_context() {
        let wide = [("tactical", "action"), ("pass", "action")];
        let base = pending("seat0", &wide);

        let other_actor = PendingManualChoice::new(
            BranchId::SOURCE,
            1,
            0,
            None,
            &choice("seat1", "choose an action", &wide),
        )
        .expect("well formed");
        assert_ne!(base.fingerprint, other_actor.fingerprint);

        let other_prompt = PendingManualChoice::new(
            BranchId::SOURCE,
            1,
            0,
            None,
            &choice("seat0", "choose another action", &wide),
        )
        .expect("well formed");
        assert_ne!(base.fingerprint, other_prompt.fingerprint);

        let reordered = pending("seat0", &[("pass", "action"), ("tactical", "action")]);
        assert_ne!(base.fingerprint, reordered.fingerprint);

        let rekinds = pending("seat0", &[("tactical", "ability"), ("pass", "action")]);
        assert_ne!(base.fingerprint, rekinds.fingerprint);

        let with_context =
            choice("seat0", "choose an action", &wide).contextualized(DecisionContext::new(
                seat("seat0"),
                DecisionSource::Rule("52.4".to_owned()),
                "gain_command_token",
                Phase::Action,
                1,
            ));
        let contextual =
            PendingManualChoice::new(BranchId::SOURCE, 1, 0, None, &with_context).expect("ok");
        assert_ne!(base.fingerprint, contextual.fingerprint);

        // Same context again, rebuilt from a different `Choice` value: identity, not pointer.
        let rebuilt =
            PendingManualChoice::new(BranchId::new(7), 99, 4, None, &with_context).expect("ok");
        assert_eq!(contextual.fingerprint, rebuilt.fingerprint);
    }

    #[test]
    fn field_boundaries_cannot_be_spliced_into_the_same_fingerprint() {
        // Same characters overall, different split between prompt and option id.
        let left = PendingManualChoice::new(
            BranchId::SOURCE,
            1,
            0,
            None,
            &choice("seat0", "a|decline", &[("x", "action")]),
        )
        .expect("well formed");
        let right = PendingManualChoice::new(
            BranchId::SOURCE,
            1,
            0,
            None,
            &choice("seat0", "a", &[("decline|x", "action")]),
        )
        .expect("well formed");
        assert_ne!(left.fingerprint, right.fingerprint);
    }

    #[test]
    fn an_exact_offered_option_is_accepted_once() {
        let wait = pending("seat0", &[("tactical", "action"), ("pass", "action")]);
        let mut control =
            ManualControl::new(SeatControl::from_changes([("seat0", SeatMode::Manual)]));
        control.publish(wait.clone());

        let outcome = control.submit(&submission(&wait, "tactical"));
        assert_eq!(
            outcome,
            SubmitOutcome::Accepted {
                option_id: "tactical".to_owned()
            }
        );
        assert!(!control.requires_human());
    }

    #[test]
    fn a_stale_fingerprint_is_refused_without_consuming_the_choice() {
        let old = pending("seat0", &[("tactical", "action")]);
        let fresh = pending("seat0", &[("pass", "action")]);
        let mut control = ManualControl::new(SeatControl::all_auto());
        control.publish(fresh.clone());

        let outcome = control.submit(&submission(&old, "tactical"));
        assert_eq!(
            outcome,
            SubmitOutcome::Stale {
                current: fresh.fingerprint.clone()
            }
        );
        assert!(control.requires_human());
        // The live choice is still answerable after the refused click.
        assert_eq!(
            control.submit(&submission(&fresh, "pass")),
            SubmitOutcome::Accepted {
                option_id: "pass".to_owned()
            }
        );
    }

    #[test]
    fn an_invented_option_is_refused_and_a_second_click_is_a_duplicate() {
        let wait = pending("seat0", &[("tactical", "action"), ("pass", "action")]);
        let mut control = ManualControl::new(SeatControl::all_auto());
        control.publish(wait.clone());

        assert_eq!(
            control.submit(&submission(&wait, "destroy_everything")),
            SubmitOutcome::NotOffered {
                offered: vec!["tactical".to_owned(), "pass".to_owned()]
            }
        );
        assert!(control.requires_human());
        assert_eq!(
            control.submit(&submission(&wait, "pass")),
            SubmitOutcome::Accepted {
                option_id: "pass".to_owned()
            }
        );
        assert_eq!(
            control.submit(&submission(&wait, "pass")),
            SubmitOutcome::Duplicate
        );
        assert_eq!(
            control.submit(&ManualSubmission {
                // An occurrence that was never published anywhere.
                offer: OfferId::next(),
                fingerprint: ChoiceFingerprint::compute(&seat("seat9"), "never offered", &[], None),
                option_id: "pass".to_owned(),
            }),
            SubmitOutcome::NoPendingChoice
        );
    }

    #[test]
    fn six_seats_toggle_independently() {
        let seats: Vec<PlayerId> = (0..6).map(|index| seat(&format!("seat{index}"))).collect();
        let mut control = SeatControl::all_auto();
        for seat in &seats {
            assert_eq!(control.mode(seat), SeatMode::Auto);
        }
        for seat in &seats {
            assert_eq!(control.toggle(seat), SeatMode::Manual);
        }
        assert_eq!(
            control.manual_seats().cloned().collect::<Vec<_>>(),
            seats.clone()
        );

        // One toggle on one seat flips exactly that seat back.
        assert_eq!(control.toggle(&seats[2]), SeatMode::Auto);
        let expected = vec![
            seats[0].clone(),
            seats[1].clone(),
            seats[3].clone(),
            seats[4].clone(),
            seats[5].clone(),
        ];
        assert_eq!(
            control.manual_seats().cloned().collect::<Vec<_>>(),
            expected
        );

        // Toggling every seat twice returns each to where it was and disturbs nobody.
        for seat in &seats {
            control.toggle(seat);
            control.toggle(seat);
        }
        assert_eq!(
            control.manual_seats().cloned().collect::<Vec<_>>(),
            expected
        );
        assert_eq!(control.mode(&seats[2]), SeatMode::Auto);
        assert_eq!(control.mode(&seats[0]), SeatMode::Manual);
    }

    #[test]
    fn delegation_is_one_shot_and_leaves_the_seat_manual() {
        let actor = seat("seat3");
        let wait = pending("seat3", &[("tactical", "action")]);
        let mut control =
            ManualControl::new(SeatControl::from_changes([("seat3", SeatMode::Manual)]));
        let occurrence = wait.offer;
        control.publish(wait);

        assert_eq!(
            control.delegate_pending_once(occurrence),
            Some(actor.clone())
        );
        assert!(!control.requires_human());
        assert_eq!(control.seats().mode(&actor), SeatMode::Manual);
        assert!(control.take_delegation(&actor));
        assert!(!control.take_delegation(&actor));
        assert_eq!(control.seats().mode(&actor), SeatMode::Manual);

        // Setting the seat to Auto clears a delegation it never got to use.
        control.seats.delegate_once(&actor);
        control.seats.set_mode(&actor, SeatMode::Auto);
        assert!(!control.seats().has_delegation(&actor));
    }

    #[test]
    fn switching_the_waiting_seat_to_auto_lets_its_bot_answer() {
        let actor = seat("seat1");
        let wait = pending("seat1", &[("tactical", "action")]);
        let fingerprint = wait.fingerprint.clone();
        let mut control =
            ManualControl::new(SeatControl::from_changes([("seat1", SeatMode::Manual)]));
        control.publish(wait);

        assert_eq!(
            control.set_mode(&actor, SeatMode::Auto),
            ModeEffect::ReleasedBot {
                previous: SeatMode::Manual,
                choice: fingerprint,
            }
        );
        assert!(!control.requires_human());
        assert_eq!(
            control.set_mode(&actor, SeatMode::Auto),
            ModeEffect::Unchanged
        );
    }

    #[test]
    fn changing_another_seat_never_disturbs_a_pending_choice() {
        let wait = pending("seat1", &[("tactical", "action"), ("pass", "action")]);
        let mut control =
            ManualControl::new(SeatControl::from_changes([("seat1", SeatMode::Manual)]));
        control.publish(wait.clone());

        assert_eq!(
            control.set_mode(&seat("seat4"), SeatMode::Manual),
            ModeEffect::Changed {
                previous: SeatMode::Auto,
            }
        );
        assert_eq!(
            control.set_mode(&seat("seat4"), SeatMode::Auto),
            ModeEffect::Changed {
                previous: SeatMode::Manual,
            }
        );
        assert_eq!(control.pending(), Some(&wait));
        assert_eq!(
            control.submit(&submission(&wait, "pass")),
            SubmitOutcome::Accepted {
                option_id: "pass".to_owned()
            }
        );
    }

    #[test]
    fn project_and_offer_bounds_are_refused_not_truncated() {
        let mut ids = BranchIds::fresh();
        assert_eq!(ids.next(), 1);
        for expected in 1..MAX_BRANCHES_PER_PROJECT {
            assert_eq!(ids.allocate(), Ok(BranchId::new(expected)));
        }
        assert_eq!(ids.allocate(), Err(ControlError::TooManyBranches));
        assert_eq!(ids.next(), MAX_BRANCHES_PER_PROJECT);
        assert_eq!(
            BranchIds::from_next(0).allocate(),
            Ok(BranchId::new(0)),
            "a project with no source timeline may still allocate from zero"
        );

        let empty = choice("seat0", "choose", &[]);
        assert_eq!(
            PendingManualChoice::new(BranchId::SOURCE, 0, 0, None, &empty),
            Err(ControlError::EmptyChoice)
        );

        let huge = owned_choice(MAX_OFFERED_OPTIONS + 1);
        assert_eq!(
            PendingManualChoice::new(BranchId::SOURCE, 0, 0, None, &huge),
            Err(ControlError::TooManyOptions {
                found: MAX_OFFERED_OPTIONS + 1
            })
        );
        let at_bound = owned_choice(MAX_OFFERED_OPTIONS);
        assert!(PendingManualChoice::new(BranchId::SOURCE, 0, 0, None, &at_bound).is_ok());
    }

    #[test]
    fn replay_records_only_offered_answers_and_matches_by_identity() {
        let wait = pending("seat0", &[("tactical", "action"), ("pass", "action")]);
        assert_eq!(
            ReplayRecord::record(&wait, "produce", Provenance::Human),
            Err(ControlError::OptionNotOffered {
                chosen: "produce".to_owned()
            })
        );

        let record =
            ReplayRecord::record(&wait, "tactical", Provenance::Human).expect("offered answer");
        assert_eq!(record.provenance, Provenance::Human);
        assert_eq!(record.offered, vec!["tactical", "pass"]);
        assert!(record.matches(&wait));

        let mut different_offer = wait.clone();
        different_offer.options.pop();
        assert!(!record.matches(&different_offer));

        // Rebuild identity survives the presentation fields the fingerprint excludes.
        let mut reshown = wait.clone();
        reshown.frame = 9_001;
        reshown.ask = 2;
        for option in &mut reshown.options {
            option.label = String::new();
        }
        assert!(record.matches(&reshown));

        for (provenance, text) in [
            (Provenance::Policy, "policy"),
            (Provenance::Human, "human"),
            (Provenance::DelegatedToPolicy, "delegated_to_policy"),
            (Provenance::ReplayPrefix, "replay_prefix"),
        ] {
            assert_eq!(provenance.as_str(), text);
            assert_eq!(
                serde_json::to_value(provenance).expect("serializable"),
                Value::from(text)
            );
        }
    }
}
