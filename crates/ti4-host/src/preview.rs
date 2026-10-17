//! The read-only secondary preview a seated follower can ask for while a strategic action is in
//! progress (`ClientMessage::PreviewSecondary`; see `ti4_engine::secondary_preview` for how the
//! answer is computed). Both hosts answer through this module: [`eligibility`] says whether the
//! request means something at the position, and [`answer`] builds the reply.
//!
//! What a host keeps for itself: whether its game is live right now (the server: not stopped,
//! finished, failed or still replaying; [`PreviewRefusal::Busy`] otherwise), the copy of the
//! position it reads (the server copies it under its lock and computes on the copy), and the
//! seat that asks (the server: the connection's seat; the browser: the seat the page is shown
//! as).
//!
//! What this module guarantees:
//!
//! * **Read only.** The game, its RNG, the decision log and the event log are never touched.
//! * **Private.** The answer is for the asking seat only. It is computed from that seat's own
//!   holdings and the public board.
//! * **Only when it means something.** A seated follower, while the action phase's latest action
//!   is a strategic action by another seat whose card is still unexhausted, and before that seat
//!   has been asked anything of the action. Anything else is refused with a reason.
//!
//! The freshest position a host has while the primary is mid-step is the one published at its
//! last nested offer; before the primary's first question it is the position at the top of the
//! step, i.e. BEFORE the primary. Either way the answer is "as of now": the primary's remaining
//! effects, followers asked earlier in clockwise order and action cards can still change what the
//! real question offers, so a client re-checks its plan against the real question.

use ti4_content::ContentStore;
use ti4_content::galaxy::Galaxy;
use ti4_engine::choice::{Choice, DecisionRecord};
use ti4_engine::secondary_preview::preview_secondary;
use ti4_model::content_types::POK;
use ti4_model::id::{PlayerId, StrategyCardId};
use ti4_model::state::{GameState, Phase};

use crate::protocol::PROTOCOL_VERSION;
use crate::protocol::server::{PreviewRefusal, PreviewResult, SecondaryPreviewMsg};

/// One preview request as the host hands it over (the seat is the host's, never the message's).
#[derive(Debug, Clone)]
pub struct PreviewRequest {
    pub request_id: u64,
    pub card: String,
    pub primary: String,
    pub answers: Vec<String>,
}

/// Why a request is refused, with the detail for the person.
pub type Refusal = (PreviewRefusal, String);

/// The refusal of a host whose game is not live right now.
#[must_use]
pub fn busy() -> Refusal {
    (
        PreviewRefusal::Busy,
        "the game is not live right now".to_owned(),
    )
}

/// The position a request is judged against.
#[derive(Debug, Clone, Copy)]
pub struct Position<'a> {
    /// The newest published position.
    pub state: &'a GameState,
    /// Every published decision.
    pub decision_log: &'a [DecisionRecord],
    /// The question open for a seat right now (the seat, the choice), if any.
    pub asked: Option<(&'a PlayerId, &'a Choice)>,
}

/// Whether `seat` may preview the secondary of `card` that `primary` plays, at `position`.
pub fn eligibility(
    position: &Position<'_>,
    seat: &PlayerId,
    card: &StrategyCardId,
    primary: &PlayerId,
) -> Result<(), Refusal> {
    let refuse = |reason, detail: &str| Err((reason, detail.to_owned()));
    let state = position.state;
    if state.phase != Phase::Action || state.player(seat).is_none() {
        return refuse(
            PreviewRefusal::NoStrategicAction,
            "no strategic action is in progress",
        );
    }
    if seat == primary {
        return refuse(PreviewRefusal::IsPrimary, "you played this card");
    }
    // The action in progress is the newest "action phase" decision, taken by the primary, and
    // the primary is still the active seat (Coup d'Etat passes the turn without exhausting).
    let log = position.decision_log;
    let Some(start) = log
        .iter()
        .rposition(|record| record.prompt == "action phase")
    else {
        return refuse(
            PreviewRefusal::NoStrategicAction,
            "no strategic action is in progress",
        );
    };
    let action = &log[start];
    let named = action.chosen.strip_prefix("strategic|");
    let strategic = action.chosen == ti4_engine::STRATEGIC_ACTION_ID || named.is_some();
    let holds_unexhausted = state.player(primary).is_some_and(|holder| {
        holder.strategy_cards.contains(card) && !holder.exhausted_strategy_cards.contains(card)
    });
    if !strategic
        || &action.player != primary
        || named.is_some_and(|id| id != card.as_str())
        || state.active.as_ref() != Some(primary)
        || !holds_unexhausted
    {
        return refuse(
            PreviewRefusal::NoStrategicAction,
            "that strategic action is not in progress",
        );
    }
    // Every decision the seat answered after the action began is in the log; once there is one
    // the seat has been asked. A question open for the seat right now counts as asked too.
    let asked_now = position.asked.is_some_and(|(asked, choice)| {
        asked == seat
            && choice
                .details
                .get("kind")
                .and_then(serde_json::Value::as_str)
                == Some("strategy_secondary")
    });
    if asked_now || log[start + 1..].iter().any(|record| &record.player == seat) {
        return refuse(PreviewRefusal::AlreadyAsked, "you have already been asked");
    }
    Ok(())
}

/// What a preview is computed from once [`eligibility`] passed.
#[derive(Debug, Clone, Copy)]
pub struct Basis<'a> {
    pub state: &'a GameState,
    pub galaxy: Option<&'a Galaxy>,
    /// How many decisions the game had published.
    pub decisions: usize,
}

/// The reply to `request` of `seat`: the engine's preview on `basis`, or the refusal. `version`
/// is the game version of the position.
#[must_use]
pub fn answer(
    game_id: &str,
    seat: &PlayerId,
    request: &PreviewRequest,
    version: u64,
    basis: Result<Basis<'_>, Refusal>,
) -> SecondaryPreviewMsg {
    let (as_of_decisions, outcome) = match basis {
        Err((reason, detail)) => (0, PreviewResult::Refused { reason, detail }),
        Ok(basis) => {
            let preview = preview_secondary(
                basis.state,
                ContentStore::embedded(),
                POK,
                basis.galaxy,
                &StrategyCardId::new(request.card.clone()),
                &PlayerId::new(request.primary.clone()),
                seat,
                &request.answers,
            );
            (basis.decisions, PreviewResult::Preview { preview })
        }
    };
    SecondaryPreviewMsg {
        protocol_version: PROTOCOL_VERSION,
        game_id: game_id.to_owned(),
        request_id: request.request_id,
        as_of_version: version,
        as_of_decisions,
        card: request.card.clone(),
        outcome,
    }
}
