//! A read-only preview of what a follower's strategy-card secondary would ask if its window
//! opened right now.
//!
//! A client that wants to prepare a secondary while the primary resolves needs the real
//! questions (which technologies are researchable and how the 4 resources can be paid,
//! which sites a structure may go on, which units the home system may build), not an estimate
//! worked out from public data: skips, waivers, discounts, supply and production limits all live
//! in the engine.
//!
//! The preview is computed by the engine's own flow, so it cannot drift from the real question:
//!
//! * the window question is built by [`StrategySecondaryWindow`] itself (`pending_choice`);
//! * the follow-up questions are produced by running [`crate::strategy_cards::follow`] (the very
//!   function the game driver calls) on a throwaway copy of the position with a decider that
//!   records the first question of the card's own kind and stops the flow there.
//!
//! It is read only by construction: the position is taken by shared reference and copied with
//! [`crate::game::detached_state`] (the copy gets its own turn-redo deck cursor, see the FORK RULE
//! in `AGENTS.md`); nothing here owns a `Game`, a decision log, an event list, a dice or random
//! stream, or storage. The flows that draw cards (Politics, Imperial) or ask nothing further
//! (Trade) are never run: for them the preview is the window question alone.
//!
//! What it cannot know is what happens between now and the moment the window really opens: the
//! primary's own effects, followers asked earlier in clockwise order and action cards. Callers
//! label the result "as of now" and re-check the plan against the real question when it opens.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use serde::{Deserialize, Serialize};
use ti4_content::ContentStore;
use ti4_content::galaxy::Galaxy;
use ti4_model::content_types::SourceSet;
use ti4_model::id::{PlayerId, StrategyCardId};
use ti4_model::state::GameState;

use crate::choice::{Choice, ChoiceOption, Decider, IllegalChoice, Table};
pub use crate::strategy::SecondaryBlocker;
use crate::strategy::StrategySecondaryWindow;

/// Most prompts one preview may auto-decline on its way to the card's own question.
const MAX_SKIPPED_PROMPTS: usize = 8;
/// Most scripted answers one preview accepts. Most cards ask at most two follow-up questions; a
/// Leadership purchase asks a payment question per planet or trade good, a pool and a "again?"
/// question per token, so a whole prepared purchase needs room (the flow stops at the first
/// answer the engine does not offer, so the cost is bounded by the game, not by this number).
pub const MAX_ANSWERS: usize = 48;

/// What the follower would be asked, as of the position given.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
#[allow(
    clippy::large_enum_variant,
    reason = "one short-lived value per request; boxing the choice would only complicate every reader"
)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum SecondaryPreview {
    /// The next question: the window question when `step` is 0, a follow-up after that.
    Question {
        choice: Choice,
        step: usize,
        /// Technology: how the engine would pay the 4 resources once a technology is chosen.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        payment: Option<PaymentPreview>,
        /// Faction prompts the real flow would put first that the preview declined to get here.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        skipped: Vec<SkippedPrompt>,
    },
    /// The window would not open for this seat at all.
    WouldNotBeAsked { blocker: SecondaryBlocker },
    /// The answers given end the secondary: nothing further would be asked.
    Complete {
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        skipped: Vec<SkippedPrompt>,
        /// Scripted answers after the first that the flow never asked for (it ended earlier than
        /// the script assumed: a purchase of more tokens than the seat can afford). Payment
        /// answers for a payment the engine settled by itself (a lone option) do not count.
        #[serde(default, skip_serializing_if = "is_zero")]
        unused_answers: usize,
    },
    /// A scripted answer is not offered by the question the engine asks at that point: the
    /// planned answers do not fit the game as it is now. `choice` is that question.
    Rejected {
        /// Index into the given `answers` of the answer that is not offered.
        at: usize,
        answer: String,
        choice: Box<Choice>,
    },
    /// The preview cannot say (an unexpected question, an answer that is not offered, ...).
    Unavailable { detail: String },
}

const fn is_zero(count: &usize) -> bool {
    *count == 0
}

/// The cheapest way to pay a Technology secondary's resources: the first plan of
/// [`crate::payment::plans`]. The player chooses the payment when the game asks for it
/// (`pay_resources`, one planet or trade good per question); a lone way is taken without asking.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct PaymentPreview {
    /// Resources owed.
    pub cost: i64,
    /// Planets that would be exhausted.
    pub planets: Vec<String>,
    /// Trade goods that would be spent.
    pub trade_goods: i32,
    /// What the plan is worth (at least `cost`).
    pub worth: i64,
}

/// A question outside the card's own that the real flow would ask first, answered "decline" here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct SkippedPrompt {
    pub subtype: String,
    pub prompt: String,
}

/// Which decision subtypes are the card's own follow-up questions (the ones a client prepares).
/// Empty for the cards whose follow-up is not previewed.
#[must_use]
pub fn follow_up_subtypes(card_name: &str) -> &'static [&'static str] {
    match card_name {
        // Technology: the research, then its payment, one planet or trade good per question.
        "Technology" => &["research_technology", "pay_resources"],
        "Diplomacy" => &["ready_planet"],
        "Construction" => &["place_structure"],
        // Production: the build list, then (after a scripted build) the payment or placement
        // question the engine asks next, if it asks one at all.
        "Warfare" => &["produce_unit", "pay_resources", "place_unit"],
        // Leadership: the payment of each token is the player's choice, one planet or trade good
        // per question (`pay_influence`; the engine has no automatic rule for it), then the
        // token's pool, then the "again?" question. A client scripts the whole purchase to have
        // the engine check it. Trade, Politics and Imperial resolve without a question (or draw
        // cards, which a preview must not run).
        "Leadership" => &[
            "pay_influence",
            "gain_command_token",
            "buy_token_with_influence",
        ],
        _ => &[],
    }
}

#[derive(Default)]
struct Captured {
    question: Option<Choice>,
    /// `(index into the full answer list, answer, the question that does not offer it)`.
    rejected: Option<(usize, String, Choice)>,
    /// Scripted answers consumed or skipped so far.
    consumed: usize,
    skipped: Vec<SkippedPrompt>,
    failure: Option<String>,
}

/// Answers the scripted follow-up answers, records the first unscripted question of the card's
/// own kind and stops the flow there, and declines anything else.
struct Previewer {
    scripted: VecDeque<String>,
    /// Index (in the full answer list) of the first scripted answer.
    offset: usize,
    own: &'static [&'static str],
    out: Rc<RefCell<Captured>>,
}

/// Whether a scripted answer pays a bill (a planet exhausted, a trade good or commodity spent).
fn is_payment_answer(id: &str) -> bool {
    id.starts_with("exhaust|") || id == "trade_good" || id == "commodity"
}

impl Previewer {
    fn stop(choice: &Choice, reason: &str) -> IllegalChoice {
        IllegalChoice::DeciderFailed {
            player: choice.player.clone(),
            prompt: choice.prompt.clone(),
            reason: reason.to_owned(),
        }
    }
}

impl Decider for Previewer {
    fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
        let subtype = choice
            .context
            .as_ref()
            .map_or("", |context| context.subtype.as_str());
        let mut out = self.out.borrow_mut();
        if self.own.contains(&subtype) {
            // The engine takes a lone payment option without asking (the real batch skips those
            // steps the same way): payment answers are dropped once the flow moves past payments.
            if !subtype.starts_with("pay_") {
                while self
                    .scripted
                    .front()
                    .is_some_and(|id| is_payment_answer(id))
                {
                    self.scripted.pop_front();
                    out.consumed += 1;
                }
            }
            if let Some(wanted) = self.scripted.pop_front() {
                let at = self.offset + out.consumed;
                out.consumed += 1;
                return match choice.options.iter().find(|option| option.id == wanted) {
                    Some(option) => Ok(option.clone()),
                    None => {
                        out.rejected = Some((at, wanted, choice.clone()));
                        Err(Self::stop(choice, "scripted answer not offered"))
                    }
                };
            }
            out.question = Some(choice.clone());
            return Err(Self::stop(choice, "preview boundary"));
        }
        if out.skipped.len() < MAX_SKIPPED_PROMPTS
            && let Some(decline) = choice.options.iter().find(|option| option.is_decline())
        {
            out.skipped.push(SkippedPrompt {
                subtype: subtype.to_owned(),
                prompt: choice.prompt.clone(),
            });
            return Ok(decline.clone());
        }
        out.failure = Some(format!(
            "unexpected question {subtype:?}: {}",
            choice.prompt
        ));
        Err(Self::stop(choice, "unexpected question"))
    }
}

/// What `follower` would be asked for `card` (played by `primary`) if the window opened in
/// `state`, after the `answers` given so far (the first answers the window question, the rest
/// the follow-up questions in order). Pure with respect to its arguments.
#[must_use]
#[allow(
    clippy::too_many_arguments,
    reason = "the position plus the question's identity"
)]
pub fn preview_secondary(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: Option<&Galaxy>,
    card: &StrategyCardId,
    primary: &PlayerId,
    follower: &PlayerId,
    answers: &[String],
) -> SecondaryPreview {
    let run = std::panic::AssertUnwindSafe(|| {
        preview_inner(
            state, content, sources, galaxy, card, primary, follower, answers,
        )
    });
    std::panic::catch_unwind(run).unwrap_or_else(|_| SecondaryPreview::Unavailable {
        detail: "the preview failed internally".to_owned(),
    })
}

#[allow(clippy::too_many_arguments, reason = "see preview_secondary")]
fn preview_inner(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: Option<&Galaxy>,
    card: &StrategyCardId,
    primary: &PlayerId,
    follower: &PlayerId,
    answers: &[String],
) -> SecondaryPreview {
    let unavailable = |detail: &str| SecondaryPreview::Unavailable {
        detail: detail.to_owned(),
    };
    if answers.len() > MAX_ANSWERS {
        return unavailable("too many answers");
    }
    if primary == follower || state.player(follower).is_none() || state.player(primary).is_none() {
        return unavailable("not a follower of this action");
    }
    let Some(name) = crate::strategy_cards::card_name(content, card.as_str()) else {
        return unavailable("unknown strategy card");
    };
    // The throwaway position every step below may change; the caller's is only read.
    let mut scratch = crate::game::detached_state(state);
    let mut window =
        StrategySecondaryWindow::preview_for(primary.clone(), card.clone(), follower.clone());
    let Some(question) = window.pending_choice(&scratch, content, sources) else {
        return match window.blocker(&scratch, content, sources, follower) {
            Some(blocker) => SecondaryPreview::WouldNotBeAsked { blocker },
            None => unavailable("the window question could not be built"),
        };
    };
    let Some((first, rest)) = answers.split_first() else {
        return SecondaryPreview::Question {
            choice: question,
            step: 0,
            payment: None,
            skipped: Vec::new(),
        };
    };
    let Some(answer) = question.options.iter().find(|option| &option.id == first) else {
        return unavailable("that answer is not offered by the window question");
    };
    if answer.is_decline() || answer.id == "no" {
        return SecondaryPreview::Complete {
            skipped: Vec::new(),
            unused_answers: rest.len(),
        };
    }
    let own = follow_up_subtypes(&name);
    if own.is_empty() {
        return if rest.is_empty() {
            SecondaryPreview::Complete {
                skipped: Vec::new(),
                unused_answers: 0,
            }
        } else {
            unavailable("this card has no follow-up question to preview")
        };
    }
    // Follow: the window charges its token (or applies a waiver) on the scratch position.
    if let Err(error) = window.take_choice(&mut scratch, content, sources, answer.clone()) {
        return SecondaryPreview::Unavailable {
            detail: format!("the window refused the answer: {error}"),
        };
    }
    let out = Rc::new(RefCell::new(Captured::default()));
    let mut table = Table::with_default(Box::new(Previewer {
        scripted: rest.iter().cloned().collect(),
        offset: 1,
        own,
        out: out.clone(),
    }));
    let outcome = crate::strategy_cards::follow(
        &mut scratch,
        content,
        sources,
        galaxy,
        &mut table,
        follower,
        card.as_str(),
    );
    let captured = std::mem::take(&mut *out.borrow_mut());
    if let Some(choice) = captured.question {
        let payment = payment_preview(&scratch, content, sources, follower, &choice);
        return SecondaryPreview::Question {
            choice,
            step: answers.len(),
            payment,
            skipped: captured.skipped,
        };
    }
    if let Some((at, answer, choice)) = captured.rejected {
        return SecondaryPreview::Rejected {
            at,
            answer,
            choice: Box::new(choice),
        };
    }
    if let Some(detail) = captured.failure {
        return SecondaryPreview::Unavailable { detail };
    }
    match outcome {
        Ok(_) => SecondaryPreview::Complete {
            skipped: captured.skipped,
            unused_answers: rest.len().saturating_sub(captured.consumed),
        },
        Err(error) => SecondaryPreview::Unavailable {
            detail: format!("the flow stopped: {error}"),
        },
    }
}

/// Technology: the cheapest plan that pays this question's cost.
fn payment_preview(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    follower: &PlayerId,
    choice: &Choice,
) -> Option<PaymentPreview> {
    let context = choice.context.as_ref()?;
    if context.subtype != "research_technology" {
        return None;
    }
    let cost = choice
        .options
        .iter()
        .filter(|option| !option.is_decline())
        .find_map(|option| {
            option
                .payload
                .get("cost")
                .and_then(serde_json::Value::as_i64)
        })?;
    if cost <= 0 {
        return None;
    }
    let spend = crate::production::Spend::Resources;
    let plan = crate::payment::plans(state, content, sources, follower, cost, spend)
        .into_iter()
        .next()?;
    Some(PaymentPreview {
        cost,
        worth: plan.worth_for(state, content, sources, follower, spend),
        planets: plan.planets.iter().map(ToString::to_string).collect(),
        trade_goods: plan.trade_goods,
    })
}

#[cfg(test)]
#[path = "secondary_preview_tests.rs"]
mod tests;
