//! Structural strategy-card actions.

use ti4_content::ContentStore;
use ti4_model::content_types::SourceSet;
use ti4_model::id::{PlayerId, StrategyCardId};
use ti4_model::state::{GameState, TokenPool};

use crate::choice::{Choice, ChoiceOption, IllegalChoice, validate};
use crate::draft::strategy_card_label;

/// The historical action id used when a player has exactly one strategic action available.
pub const STRATEGIC_ACTION_ID: &str = "strategic";
/// The choice kind for an ordinary action-phase action.
pub const ACTION_KIND: &str = "action";
/// The id used to follow a strategic action's secondary.
pub const FOLLOW_SECONDARY_ID: &str = "follow";
/// The choice kind for a strategic-action secondary response.
pub const STRATEGY_KIND: &str = "strategy";
/// The subtype of a follower's secondary question (every card but Leadership, whose question is
/// the purchase question `buy_token_with_influence`). Its source is
/// [`DecisionSource::StrategyCard`](crate::decision_context::DecisionSource::StrategyCard) with
/// the card's id and `secondary: true`.
pub const FOLLOW_SUBTYPE: &str = "strategy_secondary";
/// Prefix of an option that follows a secondary without spending a strategy token, offered only
/// when a faction module has a waiver (Winnu Acquiescence). The full id is
/// `follow|waived|<module index>|<waiver id>`.
pub const WAIVED_SECONDARY_PREFIX: &str = "follow|waived|";

/// A strategic action could not be selected from the state that was presented.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StrategyActionError {
    #[error("player {0} has no unused strategy card")]
    NoUnusedStrategyCard(PlayerId),
    #[error("strategy action id {0:?} was malformed")]
    MalformedActionId(String),
    #[error(transparent)]
    IllegalChoice(#[from] IllegalChoice),
}

/// The recorded structural result of one eligible follower's secondary decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecondaryResolution {
    /// The follower chose not to use the secondary.
    Declined,
    /// The follower accepted the secondary. Its shared token cost, when applicable, was paid;
    /// the game driver immediately invokes the content-specific effect.
    Followed,
    /// The follower had no strategy token and was not offered the secondary.
    Ineligible,
}

/// An error while resolving the follower window of a strategic action.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StrategySecondaryError {
    #[error("the strategy-secondary window is complete")]
    Complete,
    #[error("follower {0} is no longer seated")]
    FollowerMissing(PlayerId),
    #[error("follower {0} no longer has a strategy token")]
    NoStrategyToken(PlayerId),
    #[error(transparent)]
    IllegalChoice(#[from] IllegalChoice),
}

/// Why a follower is not asked a card's secondary (see [`StrategySecondaryWindow::blocker`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum SecondaryBlocker {
    /// The secondary costs a strategy token and the seat has none (and no waiver).
    NoStrategyToken,
    /// Leadership: the seat cannot afford one command token.
    CannotAffordInfluence,
    /// Technology: the seat cannot pay the 4 resources.
    CannotPayResources,
    /// Technology: nothing is researchable.
    NothingToResearch,
    /// Diplomacy: no controlled planet is exhausted.
    NoExhaustedPlanet,
    /// Trade: commodities are already at the limit.
    CommoditiesFull,
    /// Warfare: the home system has no production.
    NoProduction,
    Other,
}

/// The follower's secondary question with the facts a client needs to present it: which card
/// was played, by whom, and how many strategy tokens the follower has left.
fn secondary_choice(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    card: &StrategyCardId,
    primary: &PlayerId,
    player: &PlayerId,
    costs_token: bool,
) -> Choice {
    let tokens_left = state.player(player).map_or(0, |seat| seat.strategic_tokens);
    let mut question = secondary_question(content, card, player, costs_token);
    // The source names the card by its id: the follow question of every card is the same
    // question, and a log tells which card a seat followed from it.
    let source = crate::decision_context::DecisionSource::StrategyCard {
        card: card.to_string(),
        secondary: true,
    };
    if crate::strategy_cards::card_name(content, card.as_str()).as_deref() == Some("Leadership") {
        // The window's question is the purchase question itself: typed like the loop's later
        // asks (whose source is the rule, 52.3), and carrying what a client needs to plan every
        // purchase at once.
        question = question.contextualized(crate::decision_context::DecisionContext::new(
            player.clone(),
            source,
            "buy_token_with_influence",
            state.phase,
            state.round,
        ));
        if let Some(purchase) =
            crate::strategy_cards::purchase_hints(state, content, sources, player)
        {
            question = crate::tokens::with_pool_details(question, state, "buy", Some(0))
                .hinted("purchase", &purchase);
        }
    } else {
        question = question.contextualized(
            crate::decision_context::DecisionContext::new(
                player.clone(),
                source,
                FOLLOW_SUBTYPE,
                state.phase,
                state.round,
            )
            .optional(true),
        );
    }
    if crate::strategy_cards::card_name(content, card.as_str()).as_deref() == Some("Diplomacy") {
        question = question.hinted_fields(&crate::strategy_cards::ready_hints(
            state,
            content,
            sources,
            player,
            crate::strategy_cards::DIPLOMACY_READIES,
        ));
    }
    if crate::strategy_cards::card_name(content, card.as_str()).as_deref() == Some("Construction") {
        // Display only: the structure that "yes" places, so that a client can stage it with the
        // window.
        question = question.hinted(
            "structures",
            &crate::strategy_cards::structure_hints(
                state,
                content,
                sources,
                player,
                &crate::strategy_cards::ONE_STRUCTURE,
            ),
        );
    }
    if crate::strategy_cards::card_name(content, card.as_str()).as_deref() == Some("Trade") {
        // Display only: the follower's commodities now and at the limit the replenish fills to.
        question = question.hinted(
            "commodities",
            &crate::strategy_cards::commodity_standing(state, content, player),
        );
    }
    if crate::strategy_cards::card_name(content, card.as_str()).as_deref() == Some("Imperial") {
        // Display only: the secret objectives that the follower holds now, scored ones included,
        // against the limit, as the primary's question says them.
        question = question.hinted_fields(&crate::strategy_cards::imperial_hints(
            state, content, player,
        ));
    }
    question
        .detailed("kind", "strategy_secondary")
        .detailed("card", card.as_str())
        .detailed("played_by", primary.as_str())
        .detailed("tokens_left", tokens_left)
        .detailed("costs_token", costs_token)
}

fn secondary_question(
    content: &ContentStore,
    card: &StrategyCardId,
    player: &PlayerId,
    costs_token: bool,
) -> Choice {
    let name = crate::strategy_cards::card_name(content, card.as_str())
        .unwrap_or_else(|| card.to_string());
    // Leadership has no strategy-token gate and no decline/follow prompt (LRR 52.3):
    // affordability is the only gate, so the window's question *is* the oracle
    // influence-purchase question itself.
    if name == "Leadership" {
        let n = crate::strategy_cards::INFLUENCE_PER_TOKEN;
        return Choice::new(
            player.clone(),
            format!("spend {n} influence for a command token"),
            vec![
                ChoiceOption::labelled(
                    crate::strategy_cards::STOP_BUYING,
                    STRATEGY_KIND,
                    "spend nothing further",
                ),
                ChoiceOption::labelled(
                    crate::strategy_cards::BUY_TOKEN,
                    STRATEGY_KIND,
                    format!("spend {n} influence"),
                ),
            ],
        );
    }
    let contract = match card.as_str() {
        "te4construction" => Some((
            "spend a strategy token to place a structure",
            "decline",
            "place",
        )),
        _ => match name.as_str() {
            "Trade" => Some((
                "spend a strategy token to replenish commodities",
                "decline",
                "replenish",
            )),
            "Construction" => Some((
                "spend a strategy token to build a structure",
                "decline",
                "build",
            )),
            "Warfare" => Some((
                "spend a strategy token to produce at home",
                "decline",
                "produce",
            )),
            "Technology" => Some((
                "spend a strategy token and 4 resources to research",
                "decline",
                "spend",
            )),
            "Imperial" => Some((
                "spend a strategy token to draw a secret objective",
                "decline",
                "draw",
            )),
            "Diplomacy" => Some((
                "spend a strategy token to ready two planets",
                "decline",
                "ready",
            )),
            "Politics" => Some((
                "spend a strategy token to draw two action cards",
                "decline",
                "draw",
            )),
            _ => None,
        },
    };
    if let Some((prompt, no_label, yes_label)) = contract {
        return Choice::new(
            player.clone(),
            prompt,
            vec![
                ChoiceOption::labelled("no", STRATEGY_KIND, no_label),
                ChoiceOption::labelled("yes", STRATEGY_KIND, yes_label),
            ],
        );
    }
    Choice::new(
        player.clone(),
        format!("{card} secondary"),
        vec![
            ChoiceOption::decline(),
            ChoiceOption::labelled(
                FOLLOW_SECONDARY_ID,
                STRATEGY_KIND,
                if costs_token {
                    "spend a strategy token to resolve the secondary"
                } else {
                    "resolve the secondary"
                },
            ),
        ],
    )
}

/// Add any faction waivers to a follower's secondary choice. A follower who must pay a token and
/// has none is offered the waiver instead of a "yes" the window would then refuse: legal options
/// are generated, never rejected late.
#[allow(
    clippy::too_many_arguments,
    reason = "the choice plus the window's identity"
)]
fn with_waivers(
    state: &GameState,
    content: &ContentStore,
    mut choice: Choice,
    player: &PlayerId,
    primary_player: &PlayerId,
    card: &StrategyCardId,
    costs_token: bool,
) -> Choice {
    if !costs_token {
        // Enervate: a puppeted player's Leadership may be followed by performing its primary
        // ability instead of its secondary.
        if crate::factions::firmament_plots::primary_instead_offered(
            state,
            player,
            primary_player,
            crate::strategy_cards::card_name(content, card.as_str()).as_deref(),
        ) {
            choice.options.push(ChoiceOption::labelled(
                crate::factions::firmament_plots::PRIMARY_INSTEAD_ID,
                STRATEGY_KIND,
                "perform the primary ability instead (Enervate)",
            ));
        }
        return choice;
    }
    let waivers = waiver_options(state, content, player, primary_player, card);
    if waivers.is_empty() {
        return choice;
    }
    let tokenless = state
        .player(player)
        .is_none_or(|seat| seat.strategic_tokens <= 0);
    if tokenless {
        choice.options.retain(|option| option.id != "yes");
    }
    let phrased: Vec<(String, crate::phrases::Worded)> = waivers
        .iter()
        .filter_map(|(option, worded)| Some((option.id.clone(), worded.clone()?)))
        .collect();
    choice
        .options
        .extend(waivers.into_iter().map(|(option, _)| option));
    choice.phrased_options(&phrased)
}

/// Token-free ways for `follower` to follow `card` (faction waivers), as offered options, with
/// what each label names by id.
fn waiver_options(
    state: &GameState,
    content: &ContentStore,
    follower: &PlayerId,
    primary: &PlayerId,
    card: &StrategyCardId,
) -> Vec<(ChoiceOption, Option<crate::phrases::Worded>)> {
    let name = crate::strategy_cards::card_name(content, card.as_str())
        .unwrap_or_else(|| card.to_string());
    crate::factions::hooks_strategy::secondary_waivers(state, content, follower, primary, &name)
        .into_iter()
        .map(|(index, waiver)| {
            (
                ChoiceOption::labelled(
                    format!("{WAIVED_SECONDARY_PREFIX}{index}|{}", waiver.id),
                    STRATEGY_KIND,
                    waiver.label,
                ),
                waiver.worded,
            )
        })
        .collect()
}

/// The ordered follower window opened by a strategic action.
///
/// The owner is deliberately absent from `followers`: LRR 82.1 offers a secondary to
/// everyone else, clockwise from the primary player. The window keeps completion data
/// outside `GameState` until the M04 step driver can own open decision windows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StrategySecondaryWindow {
    primary_player: PlayerId,
    card: StrategyCardId,
    followers: Vec<PlayerId>,
    next_follower: usize,
    resolutions: Vec<(PlayerId, SecondaryResolution)>,
    /// Whether finishing the window exhausts the primary player's copy of the card. False for
    /// a card resolved by an effect rather than held (see [`StrategySecondaryWindow::foreign`]).
    exhausts_card: bool,
    /// A disposable one-follower window on a planning fork (see [`StrategySecondaryWindow::preview`]).
    preview: bool,
}

impl StrategySecondaryWindow {
    /// The follower window for a card the primary player does **not** hold, resolved by an effect
    /// rather than a strategic action: `followers` (in the order given) may each follow its
    /// secondary, at the usual cost, and no card is exhausted when the window completes.
    ///
    /// Winnu hero (`winnuhero`, Mathis Mathinus): "ACTION: Perform the primary ability of any
    /// strategy card. Then, choose any number of other players. Those players may perform the
    /// secondary ability of that strategy card. Then, purge this card." The caller resolves the
    /// primary with [`crate::strategy_cards::primary`] (which does not need the card to be held),
    /// chooses the followers, and drives this window as for a strategic action.
    #[must_use]
    pub fn foreign(
        primary_player: PlayerId,
        card: StrategyCardId,
        followers: Vec<PlayerId>,
    ) -> Self {
        Self {
            primary_player,
            card,
            followers,
            next_follower: 0,
            resolutions: Vec::new(),
            exhausts_card: false,
            preview: false,
        }
    }

    /// One follower's secondary in isolation, for a disposable planning fork.
    ///
    /// The window offers `follower` the ordinary question at the ordinary cost. Completing it
    /// exhausts no card, and the driver neither finishes the action nor advances the turn: the
    /// fork only previews this seat's own secondary and is then discarded.
    #[must_use]
    pub fn preview(primary_player: PlayerId, card: StrategyCardId, follower: PlayerId) -> Self {
        Self {
            preview: true,
            ..Self::foreign(primary_player, card, vec![follower])
        }
    }

    /// Whether this is a [`StrategySecondaryWindow::preview`] window.
    #[must_use]
    pub const fn is_preview(&self) -> bool {
        self.preview
    }

    /// Followers not yet recorded, in resolution order.
    #[must_use]
    pub fn unresolved_followers(&self) -> &[PlayerId] {
        &self.followers[self.next_follower..]
    }

    /// A window with `follower` alone in it, for the read-only secondary preview
    /// ([`crate::secondary_preview`]): it asks the same eligibility and builds the same question
    /// the real window would for that seat, without the followers before it.
    #[must_use]
    pub(crate) fn preview_for(
        primary_player: PlayerId,
        card: StrategyCardId,
        follower: PlayerId,
    ) -> Self {
        Self {
            primary_player,
            card,
            followers: vec![follower],
            next_follower: 0,
            resolutions: Vec::new(),
            exhausts_card: false,
            preview: false,
        }
    }

    /// Why `follower` would not be asked this window's question, or `None` when it would be.
    ///
    /// Derived from the same predicates as [`Self::pending_choice`] (`eligible`), so the two
    /// cannot disagree: this returns `Some` exactly when `pending_choice` returns `None` for a
    /// window with this follower first in line. Inspection only.
    #[must_use]
    pub fn blocker(
        &self,
        state: &GameState,
        content: &ContentStore,
        sources: SourceSet,
        follower: &PlayerId,
    ) -> Option<SecondaryBlocker> {
        if self.eligible(state, content, sources, follower) {
            return None;
        }
        let name = crate::strategy_cards::card_name(content, self.card.as_str())
            .unwrap_or_else(|| self.card.to_string());
        if name == "Leadership" {
            return Some(SecondaryBlocker::CannotAffordInfluence);
        }
        let paid = state.player(follower).is_some_and(|seat| {
            !secondary_costs_token(content, &self.card)
                || secondary_is_free(state, content, follower, &self.card)
                || seat.strategic_tokens > 0
        });
        if !paid {
            return Some(SecondaryBlocker::NoStrategyToken);
        }
        Some(match name.as_str() {
            "Technology" => {
                if crate::technology::researchable(state, content, sources, follower).is_empty() {
                    SecondaryBlocker::NothingToResearch
                } else {
                    SecondaryBlocker::CannotPayResources
                }
            }
            "Diplomacy" => SecondaryBlocker::NoExhaustedPlanet,
            "Trade" => SecondaryBlocker::CommoditiesFull,
            "Warfare" => SecondaryBlocker::NoProduction,
            _ => SecondaryBlocker::Other,
        })
    }

    /// The player resolving the primary.
    #[must_use]
    pub const fn primary_player(&self) -> &PlayerId {
        &self.primary_player
    }

    /// The strategy card whose secondary is being offered.
    #[must_use]
    pub const fn card(&self) -> &StrategyCardId {
        &self.card
    }

    /// Follower results in clockwise resolution order.
    #[must_use]
    pub fn resolutions(&self) -> &[(PlayerId, SecondaryResolution)] {
        &self.resolutions
    }

    /// Whether every follower has been recorded and the primary card exhausted.
    #[must_use]
    pub const fn is_complete(&self) -> bool {
        self.next_follower == self.followers.len()
    }

    /// Inspect the next offered secondary without changing the game state.
    ///
    /// A game driver needs this to expose legal options without recording tokenless followers
    /// merely because a client looked at a choice. [`Self::next_choice`] remains the mutating
    /// resolver used when a step actually advances the window.
    #[must_use]
    pub fn pending_choice(
        &self,
        state: &GameState,
        content: &ContentStore,
        sources: SourceSet,
    ) -> Option<Choice> {
        self.followers[self.next_follower..]
            .iter()
            .find(|player_id| self.eligible(state, content, sources, player_id))
            .map(|player_id| {
                let costs_token = secondary_costs_token(content, &self.card)
                    && !secondary_is_free(state, content, player_id, &self.card);
                let choice = secondary_choice(
                    state,
                    content,
                    sources,
                    &self.card,
                    &self.primary_player,
                    player_id,
                    costs_token,
                );
                with_waivers(
                    state,
                    content,
                    choice,
                    player_id,
                    &self.primary_player,
                    &self.card,
                    costs_token,
                )
            })
    }

    /// Return the next eligible follower's choice, recording tokenless followers as skipped.
    ///
    /// A content-specific secondary may later impose further eligibility checks. This generic
    /// structural window has only the shared strategy-token gate.
    pub fn next_choice(
        &mut self,
        state: &mut GameState,
        content: &ContentStore,
        sources: SourceSet,
    ) -> Option<Choice> {
        while let Some(player_id) = self.followers.get(self.next_follower).cloned() {
            if self.eligible(state, content, sources, &player_id) {
                let costs_token = secondary_costs_token(content, &self.card)
                    && !secondary_is_free(state, content, &player_id, &self.card);
                let choice = secondary_choice(
                    state,
                    content,
                    sources,
                    &self.card,
                    &self.primary_player,
                    &player_id,
                    costs_token,
                );
                return Some(with_waivers(
                    state,
                    content,
                    choice,
                    &player_id,
                    &self.primary_player,
                    &self.card,
                    costs_token,
                ));
            }
            self.resolutions
                .push((player_id, SecondaryResolution::Ineligible));
            self.next_follower += 1;
        }
        self.exhaust_primary(state);
        None
    }

    /// Resolve the current follower's answer and return its structural result.
    ///
    /// # Errors
    /// [`StrategySecondaryError::IllegalChoice`] if the answer was not offered, or
    /// [`StrategySecondaryError::Complete`] once all followers are resolved.
    pub fn take_choice(
        &mut self,
        state: &mut GameState,
        content: &ContentStore,
        sources: SourceSet,
        answer: ChoiceOption,
    ) -> Result<SecondaryResolution, StrategySecondaryError> {
        let choice = self
            .next_choice(state, content, sources)
            .ok_or(StrategySecondaryError::Complete)?;
        let answer = validate(&choice, answer)?;
        let resolution = if answer.is_decline() || answer.id == "no" {
            SecondaryResolution::Declined
        } else if let Some(waived) = answer.id.strip_prefix(WAIVED_SECONDARY_PREFIX) {
            // A faction waiver: no token. The module pays its own cost. `validate` accepted the id
            // because `waiver_options` offered it, so it is `<index>|<waiver id>`.
            if let Some((index, waiver)) = waived.split_once('|')
                && let Ok(index) = index.parse::<usize>()
            {
                crate::factions::hooks_strategy::secondary_waived(
                    state,
                    content,
                    &choice.player,
                    &self.primary_player,
                    index,
                    waiver,
                );
            }
            SecondaryResolution::Followed
        } else {
            let costs_token = secondary_costs_token(content, &self.card)
                && !secondary_is_free(state, content, &choice.player, &self.card);
            if costs_token {
                let player = state.player_mut(&choice.player).ok_or_else(|| {
                    StrategySecondaryError::FollowerMissing(choice.player.clone())
                })?;
                if !player.spend_token(TokenPool::Strategic) {
                    return Err(StrategySecondaryError::NoStrategyToken(choice.player));
                }
                crate::supply::note_strategy_token_spent(
                    state,
                    &choice.player,
                    "strategy_secondary",
                );
            }
            SecondaryResolution::Followed
        };
        // Enervate: remember whether this follower took the primary instead (and forget a stale
        // one). After every refusal above, so a refused answer changes nothing.
        crate::factions::firmament_plots::record_follow_answer(state, &choice.player, &answer.id);
        self.resolutions.push((choice.player, resolution));
        self.next_follower += 1;
        if self.is_complete() {
            self.exhaust_primary(state);
        }
        Ok(resolution)
    }

    /// [`secondary_eligible`], or a follower without a token who holds a waiver.
    fn eligible(
        &self,
        state: &GameState,
        content: &ContentStore,
        sources: SourceSet,
        player: &PlayerId,
    ) -> bool {
        secondary_eligible(state, content, sources, player, &self.card)
            || crate::factions::firmament_plots::primary_instead_offered(
                state,
                player,
                &self.primary_player,
                crate::strategy_cards::card_name(content, self.card.as_str()).as_deref(),
            )
            || (secondary_costs_token(content, &self.card)
                && crate::strategy_cards::card_name(content, self.card.as_str()).as_deref()
                    != Some("Leadership")
                && !waiver_options(state, content, player, &self.primary_player, &self.card)
                    .is_empty()
                && secondary_can_do_something(state, content, sources, player, &self.card))
    }

    fn exhaust_primary(&self, state: &mut GameState) {
        if self.is_complete() && self.exhausts_card {
            let exhausted = state.exhaust_strategy_card(&self.primary_player, self.card.clone());
            debug_assert!(
                exhausted,
                "the primary holder must retain the selected card"
            );
        }
    }
}

fn secondary_costs_token(content: &ContentStore, card: &StrategyCardId) -> bool {
    crate::strategy_cards::card_name(content, card.as_str()).as_deref() != Some("Leadership")
}

fn secondary_is_free(
    state: &GameState,
    content: &ContentStore,
    player: &PlayerId,
    card: &StrategyCardId,
) -> bool {
    crate::strategy_cards::card_name(content, card.as_str()).is_some_and(|name| {
        crate::faction_abilities::secondary_is_free(state, content, player, &name)
    })
}

fn secondary_eligible(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    card: &StrategyCardId,
) -> bool {
    if crate::strategy_cards::card_name(content, card.as_str()).as_deref() == Some("Leadership") {
        // 52.3 costs influence rather than a strategy token: unaffordable seats make no
        // decision at all (oracle `_leadership_secondary`).
        return crate::strategy_cards::leadership_influence_eligible(
            state, content, sources, player,
        );
    }
    if !state.player(player).is_some_and(|seat| {
        !secondary_costs_token(content, card)
            || secondary_is_free(state, content, player, card)
            || seat.strategic_tokens > 0
    }) {
        return false;
    }
    secondary_can_do_something(state, content, sources, player, card)
}

/// Whether this seat's secondary could actually accomplish anything.
///
/// Three secondaries resolve to nothing under conditions the offer did not check, and every one
/// of them still charges the strategy token:
///
/// * **Technology** costs "1 token **and** 4 resources" (LRR 74.2) -- one price, not two. A seat
///   that cannot pay the resources cannot resolve it, and `paid_research` returned early for want
///   of them. The same early return covers an empty researchable set, so that is checked too.
/// * **Diplomacy** readies exhausted planets; `ready_planets` breaks immediately when the seat has
///   none exhausted.
/// * **Trade** replenishes commodities; `replenish` assigns the limit, which is a no-op for a seat
///   already at it.
/// * **Warfare** produces from the home system; `home_production` no-ops when that system has no
///   production capacity.
///
/// Construction is deliberately *not* gated: a seat essentially always controls a planet to build
/// on, and scuttling a dock to place one further forward is a legitimate choice rather than a
/// wasted token.
///
/// Without these gates the windows opened regardless and the policies accepted them at close to
/// 100%, burning tokens for nothing.
///
/// Jol-Nar is deliberately exempt from the Technology clause: Brilliant substitutes the primary
/// ([`crate::faction_abilities::substitutes_primary`]), which researches free, so the resource
/// price never applies to them and the window stays open however little they hold.
fn secondary_can_do_something(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    card: &StrategyCardId,
) -> bool {
    let Some(name) = crate::strategy_cards::card_name(content, card.as_str()) else {
        return true;
    };
    match name.as_str() {
        "Technology" => {
            if crate::technology::researchable(state, content, sources, player).is_empty() {
                return false;
            }
            crate::faction_abilities::substitutes_primary(state, content, player, &name)
                // Jol-Nar's Specialist Compounds pays with a planet rather than resources, so a
                // seat holding it can research however little it can afford. Without this the
                // window would be closed on the price the card exists to avoid.
                || !crate::breakthroughs::specialty_research_planets(
                    state, content, sources, player,
                )
                .is_empty()
                || crate::payment::affordable(
                    state,
                    content,
                    sources,
                    player,
                    crate::strategy_cards::TECHNOLOGY_SECONDARY_COST,
                    crate::production::Spend::Resources,
                )
                // Xander Alexin Victori III: commodities as trade goods, offered when the paid
                // research opens.
                || crate::factions::keleres::with_agent_granted(state, player, |granted| {
                    crate::payment::affordable(
                        granted,
                        content,
                        sources,
                        player,
                        crate::strategy_cards::TECHNOLOGY_SECONDARY_COST,
                        crate::production::Spend::Resources,
                    )
                })
                .unwrap_or(false)
        }
        "Diplomacy" => state
            .controlled_planets(player)
            .into_iter()
            .any(|(_, planet)| state.exhausted_planets.contains(planet)),
        "Trade" => state.player(player).is_some_and(|seat| {
            seat.commodities < crate::strategy_cards::commodity_limit(state, content, player)
        }),
        // Warfare's secondary produces from the units in the seat's home system; `home_production`
        // no-ops when that system has no production capacity, i.e. no space dock.
        "Warfare" => state
            .player(player)
            .and_then(|seat| seat.home_system.clone())
            .is_some_and(|home| {
                crate::production::capacity(state, content, sources, player, &home) > 0
            }),
        _ => true,
    }
}

/// Legal structural strategic actions for one player.
///
/// A one-card holding preserves the oracle's bare `strategic` id for compatibility. A
/// multi-card holding names the selected card in `strategic|<card-id>` so the selected
/// strategic action is a real decision rather than an arbitrary fallback.
#[must_use]
pub fn strategic_action_options(
    state: &GameState,
    content: &ContentStore,
    player_id: &PlayerId,
) -> Option<Choice> {
    let player = state.player(player_id)?;
    let unused = player.unused_strategy_cards();
    if unused.is_empty() {
        return None;
    }
    let options = if unused.len() == 1 {
        vec![ChoiceOption::labelled(
            STRATEGIC_ACTION_ID,
            ACTION_KIND,
            "take your strategic action",
        )]
    } else {
        unused
            .into_iter()
            .map(|card| {
                ChoiceOption::labelled(
                    format!("{STRATEGIC_ACTION_ID}|{}", card.as_str()),
                    ACTION_KIND,
                    format!(
                        "take the strategic action of {}",
                        strategy_card_label(content, card.as_str())
                    ),
                )
            })
            .collect()
    };
    Some(Choice::new(player_id.clone(), "action phase", options))
}

/// Resolve the structural part of a selected strategic action.
///
/// Card-specific effects live in [`crate::strategy_cards`]. This operation retains the older
/// structural convenience API; the driven [`crate::game::Game`] uses
/// [`begin_strategic_action`] and invokes those effects.
///
/// # Errors
/// [`StrategyActionError::IllegalChoice`] if `answer` was not offered, or
/// [`StrategyActionError::NoUnusedStrategyCard`] if the player has no available card.
pub fn take_strategic_action(
    state: &mut GameState,
    content: &ContentStore,
    player_id: &PlayerId,
    answer: ChoiceOption,
) -> Result<StrategyCardId, StrategyActionError> {
    let card = selected_strategic_card(state, content, player_id, answer)?;

    // This convenience operation is retained for the M04-008 structural-primary API. New
    // action drivers must use `begin_strategic_action` so the primary stays ready while the
    // secondary window is open.
    let exhausted = state.exhaust_strategy_card(player_id, card.clone());
    debug_assert!(exhausted, "the checked player must hold the checked card");
    Ok(card)
}

/// Begin a strategic action and open its ordered generic-secondary window.
///
/// The caller applies the primary before driving the returned follower window. The selected card
/// is exhausted only when every follower, including tokenless skipped followers, is recorded.
///
/// # Errors
/// [`StrategyActionError::IllegalChoice`] if `answer` was not offered, or
/// [`StrategyActionError::NoUnusedStrategyCard`] if the player has no available card.
pub fn begin_strategic_action(
    state: &mut GameState,
    content: &ContentStore,
    player_id: &PlayerId,
    answer: ChoiceOption,
) -> Result<StrategySecondaryWindow, StrategyActionError> {
    let card = selected_strategic_card(state, content, player_id, answer)?;
    Ok(StrategySecondaryWindow {
        primary_player: player_id.clone(),
        card,
        followers: state
            .clockwise_from(player_id)
            .into_iter()
            .skip(1)
            .collect(),
        next_follower: 0,
        resolutions: Vec::new(),
        exhausts_card: true,
        preview: false,
    })
}

fn selected_strategic_card(
    state: &GameState,
    content: &ContentStore,
    player_id: &PlayerId,
    answer: ChoiceOption,
) -> Result<StrategyCardId, StrategyActionError> {
    let choice = strategic_action_options(state, content, player_id)
        .ok_or_else(|| StrategyActionError::NoUnusedStrategyCard(player_id.clone()))?;
    let answer = validate(&choice, answer)?;
    let player = state
        .player(player_id)
        .ok_or_else(|| StrategyActionError::NoUnusedStrategyCard(player_id.clone()))?;
    let unused = player.unused_strategy_cards();
    let card = if answer.id == STRATEGIC_ACTION_ID {
        unused
            .first()
            .copied()
            .cloned()
            .ok_or_else(|| StrategyActionError::NoUnusedStrategyCard(player_id.clone()))?
    } else {
        let (_, named) = answer
            .id
            .split_once('|')
            .ok_or_else(|| StrategyActionError::MalformedActionId(answer.id.clone()))?;
        unused
            .into_iter()
            .find(|card| card.as_str() == named)
            .cloned()
            .ok_or_else(|| StrategyActionError::MalformedActionId(answer.id.clone()))?
    };

    Ok(card)
}

// -- initiative overrides and card exchange (BF-00d-strategy) ---------------------------------------

/// From now until [`clear_initiative_overrides`], `player` counts as having initiative `value`
/// instead of the number printed on their strategy card(s): `0` puts them first.
///
/// Naalu Telepathic ("Place the Naalu '0' token on your strategy card; you are first in
/// initiative order") and Gift of Prescience (the holder places that token on their card instead)
/// write this from `hooks_strategy::StrategyHooks::strategy_phase_ended`. Returns `false` and
/// changes nothing for a player who is not seated. Ties (two overrides of `0`) still break by
/// seating order, as for printed numbers.
///
/// Neutral when unused: [`GameState::initiative_order`] reads this map only when it is non-empty.
pub fn set_initiative_override(state: &mut GameState, player: &PlayerId, value: i32) -> bool {
    if state.player(player).is_none() {
        return false;
    }
    state.initiative_overrides.insert(player.clone(), value);
    true
}

/// Remove one player's initiative override.
pub fn clear_initiative_override(state: &mut GameState, player: &PlayerId) -> bool {
    state.initiative_overrides.remove(player).is_some()
}

/// Remove every initiative override. Called from `phase::begin_next_round`: the "0" token lasts
/// the game round (Gift of Prescience is returned "at the end of the status phase").
pub fn clear_initiative_overrides(state: &mut GameState) {
    state.initiative_overrides.clear();
}

/// Exchange one strategy card of `a` for one of `b`, atomically.
///
/// Both must hold the card named for them and be different players holding different cards;
/// otherwise nothing changes and `false` is returned. The rest of each holding is preserved and
/// stays in initiative order; each card keeps **its own** exhausted state (an exhausted card
/// stays exhausted wherever it goes). Public information: initiative follows the cards.
///
/// Winnu's Acquiescence is *not* a swap (its text is a free secondary; see
/// `plans/evidence/BF-00d-strategy.md`); this exists for any effect that does exchange cards.
pub fn exchange_strategy_cards(
    state: &mut GameState,
    a: &PlayerId,
    card_a: &StrategyCardId,
    b: &PlayerId,
    card_b: &StrategyCardId,
) -> bool {
    if a == b || card_a == card_b {
        return false;
    }
    let (Some(seat_a), Some(seat_b)) = (state.player(a), state.player(b)) else {
        return false;
    };
    if !seat_a.strategy_cards.contains(card_a) || !seat_b.strategy_cards.contains(card_b) {
        return false;
    }
    let spent_a = seat_a.exhausted_strategy_cards.contains(card_a);
    let spent_b = seat_b.exhausted_strategy_cards.contains(card_b);
    state.swap_strategy_card(a, card_a, card_b.clone());
    state.swap_strategy_card(b, card_b, card_a.clone());
    // `swap_strategy_card` moves exhaustion with the *slot*; here it must follow the card.
    for (player, gave, got, got_spent) in
        [(a, card_a, card_b, spent_b), (b, card_b, card_a, spent_a)]
    {
        if let Some(seat) = state.player_mut(player) {
            seat.exhausted_strategy_cards.remove(gave);
            seat.exhausted_strategy_cards.remove(got);
            if got_spent {
                seat.exhausted_strategy_cards.insert(got.clone());
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use ti4_content::ContentStore;
    use ti4_model::content_types::POK;
    use ti4_model::id::PlayerId;

    use super::*;
    use crate::draft::{strategy_options, take_strategy_card};
    use crate::setup::start_game;

    fn drafted_three_player_game() -> GameState {
        let players = [PlayerId::new("a"), PlayerId::new("b"), PlayerId::new("c")];
        let mut state = start_game(ContentStore::embedded(), &players, POK, None).unwrap();
        while let Some(choice) = strategy_options(&state, ContentStore::embedded()) {
            take_strategy_card(
                &mut state,
                ContentStore::embedded(),
                choice.options[0].clone(),
            )
            .unwrap();
        }
        state
    }

    /// The Technology card id in the dealt deck, whichever variant the sources carry.
    fn technology_card(state: &GameState) -> StrategyCardId {
        state
            .players
            .iter()
            .flat_map(|seat| seat.strategy_cards.iter())
            .chain(state.unclaimed_strategy_cards.iter())
            .find(|card: &&StrategyCardId| {
                crate::strategy_cards::card_name(ContentStore::embedded(), card.as_str()).as_deref()
                    == Some("Technology")
            })
            .cloned()
            .unwrap_or_else(|| StrategyCardId::new("pok7technology"))
    }

    #[test]
    fn technology_secondary_is_withheld_from_a_seat_that_cannot_pay_the_resources() {
        let mut state = drafted_three_player_game();
        let player = PlayerId::new("a");
        let card = technology_card(&state);
        // The fixture seats hold no planets, so trade goods are the whole resource pool and the
        // affordability clause is the only thing that can change the answer.
        if let Some(seat) = state.player_mut(&player) {
            seat.strategic_tokens = 3;
            seat.trade_goods = 0;
        }
        assert!(
            !secondary_eligible(&state, ContentStore::embedded(), POK, &player, &card),
            "a seat with a token but no resources cannot resolve 'spend 1 token AND 4 resources'"
        );
    }

    #[test]
    fn a_seat_that_can_pay_is_still_offered_the_technology_secondary() {
        let mut state = drafted_three_player_game();
        let player = PlayerId::new("a");
        let card = technology_card(&state);
        if let Some(seat) = state.player_mut(&player) {
            seat.strategic_tokens = 3;
            seat.trade_goods =
                i32::try_from(crate::strategy_cards::TECHNOLOGY_SECONDARY_COST).unwrap_or(i32::MAX);
        }
        assert!(
            secondary_eligible(&state, ContentStore::embedded(), POK, &player, &card),
            "four trade goods cover the four resources"
        );
    }

    /// A drafted three-player game in which the first picker takes a named card.
    ///
    /// The generic follower-window tests need a secondary that is always offered. Politics,
    /// Imperial and Construction qualify; Technology, Diplomacy, Trade and Warfare are
    /// conditionally withheld by `secondary_can_do_something`, and taking whatever happened to be
    /// first would couple those tests to the draft order.
    fn drafted_with_first_pick(wanted: &str) -> (GameState, StrategyCardId) {
        let players = [PlayerId::new("a"), PlayerId::new("b"), PlayerId::new("c")];
        let mut state = start_game(ContentStore::embedded(), &players, POK, None).unwrap();
        let mut taken = None;
        while let Some(choice) = strategy_options(&state, ContentStore::embedded()) {
            let pick = choice
                .options
                .iter()
                .find(|option| {
                    taken.is_none()
                        && crate::strategy_cards::card_name(
                            ContentStore::embedded(),
                            option.id.as_str(),
                        )
                        .as_deref()
                            == Some(wanted)
                })
                .unwrap_or(&choice.options[0])
                .clone();
            if taken.is_none()
                && crate::strategy_cards::card_name(ContentStore::embedded(), pick.id.as_str())
                    .as_deref()
                    == Some(wanted)
            {
                taken = Some(StrategyCardId::new(pick.id.clone()));
            }
            take_strategy_card(&mut state, ContentStore::embedded(), pick).unwrap();
        }
        let card = taken.expect("the wanted card was available to the first picker");
        (state, card)
    }

    fn card_named(state: &GameState, wanted: &str) -> StrategyCardId {
        state
            .players
            .iter()
            .flat_map(|seat| seat.strategy_cards.iter())
            .chain(state.unclaimed_strategy_cards.iter())
            .find(|card: &&StrategyCardId| {
                crate::strategy_cards::card_name(ContentStore::embedded(), card.as_str()).as_deref()
                    == Some(wanted)
            })
            .cloned()
            .expect("the dealt deck carries this card")
    }

    #[test]
    fn a_secondary_offer_says_which_card_was_played_by_whom_and_how_many_tokens_are_left() {
        let (mut state, card) = drafted_with_first_pick("Politics");
        if let Some(seat) = state.player_mut(&PlayerId::new("b")) {
            seat.strategic_tokens = 3;
        }
        let window = StrategySecondaryWindow {
            primary_player: PlayerId::new("a"),
            card: card.clone(),
            followers: vec![PlayerId::new("b"), PlayerId::new("c")],
            next_follower: 0,
            resolutions: Vec::new(),
            exhausts_card: false,
            preview: false,
        };
        let choice = window
            .pending_choice(&state, ContentStore::embedded(), POK)
            .expect("the first follower is offered the secondary");
        assert_eq!(choice.player, PlayerId::new("b"));
        assert_eq!(choice.details["kind"], "strategy_secondary");
        assert_eq!(choice.details["card"], card.as_str());
        assert_eq!(choice.details["played_by"], "a");
        assert_eq!(choice.details["tokens_left"], 3);
        assert_eq!(choice.details["costs_token"], true);
        // The context says which card's secondary this is, so that a log can tell the follow.
        let context = choice
            .context
            .as_ref()
            .expect("a follow question has a context");
        assert_eq!(context.subtype, FOLLOW_SUBTYPE);
        assert_eq!(
            context.source,
            crate::decision_context::DecisionSource::StrategyCard {
                card: card.to_string(),
                secondary: true,
            }
        );
        assert_eq!(context.actor, PlayerId::new("b"));
        assert!(context.optional);
    }

    #[test]
    fn the_trade_secondary_states_the_followers_commodities() {
        let (mut state, card) = drafted_with_first_pick("Trade");
        if let Some(seat) = state.player_mut(&PlayerId::new("b")) {
            seat.faction = ti4_model::id::FactionId::new("sol");
            seat.strategic_tokens = 2;
            seat.commodities = 1;
            seat.trade_goods = 2;
        }
        let limit = crate::strategy_cards::commodity_limit(
            &state,
            ContentStore::embedded(),
            &PlayerId::new("b"),
        );
        let window = StrategySecondaryWindow {
            primary_player: PlayerId::new("a"),
            card,
            followers: vec![PlayerId::new("b")],
            next_follower: 0,
            resolutions: Vec::new(),
            exhausts_card: false,
            preview: false,
        };
        let choice = window
            .pending_choice(&state, ContentStore::embedded(), POK)
            .expect("the follower is offered the secondary");
        assert_eq!(
            choice.hint::<crate::strategy_cards::CommodityStanding>("commodities"),
            Some(crate::strategy_cards::CommodityStanding {
                have: 1,
                max: limit,
                trade_goods: 2,
            })
        );
        // The JSON keys that the clients read.
        assert_eq!(choice.details["commodities"]["have"], 1);
        assert_eq!(choice.details["commodities"]["max"], limit);
        assert_eq!(choice.details["commodities"]["trade_goods"], 2);
    }

    #[test]
    fn the_imperial_secondary_states_the_followers_secret_objectives() {
        let (mut state, card) = drafted_with_first_pick("Imperial");
        if let Some(seat) = state.player_mut(&PlayerId::new("b")) {
            seat.strategic_tokens = 2;
            seat.secret_objectives = vec![ti4_model::id::SecretObjectiveId::new("dhw")];
        }
        let window = StrategySecondaryWindow {
            primary_player: PlayerId::new("a"),
            card,
            followers: vec![PlayerId::new("b")],
            next_follower: 0,
            resolutions: Vec::new(),
            exhausts_card: false,
            preview: false,
        };
        let choice = window
            .pending_choice(&state, ContentStore::embedded(), POK)
            .expect("the follower is offered the secondary");
        assert_eq!(
            choice.hint_fields::<crate::strategy_cards::ImperialHints>(),
            Some(crate::strategy_cards::ImperialHints {
                secrets_held: 1,
                secrets_max: crate::secrets::HAND_LIMIT,
            })
        );
        // The JSON keys that the clients read.
        assert_eq!(choice.details["secrets_held"], 1);
        assert_eq!(choice.details["secrets_max"], crate::secrets::HAND_LIMIT);
    }

    #[test]
    fn display_details_never_change_which_decision_is_recorded() {
        let (state, card) = drafted_with_first_pick("Politics");
        let window = StrategySecondaryWindow {
            primary_player: PlayerId::new("a"),
            card,
            followers: vec![PlayerId::new("b")],
            next_follower: 0,
            resolutions: Vec::new(),
            exhausts_card: false,
            preview: false,
        };
        let with = window
            .pending_choice(&state, ContentStore::embedded(), POK)
            .expect("offered");
        let mut bare = with.clone();
        bare.details.clear();
        assert_eq!(with.options, bare.options);
        assert_eq!(with.prompt, bare.prompt);
        assert_eq!(with.context, bare.context);
        // Empty details are not serialised, so an old reader sees exactly the old bytes.
        assert!(!serde_json::to_string(&bare).unwrap().contains("details"));
        assert!(serde_json::to_string(&with).unwrap().contains("details"));
    }

    #[test]
    fn diplomacy_secondary_is_withheld_when_nothing_is_exhausted() {
        let mut state = drafted_three_player_game();
        let player = PlayerId::new("a");
        let card = card_named(&state, "Diplomacy");
        if let Some(seat) = state.player_mut(&player) {
            seat.strategic_tokens = 3;
        }
        assert!(
            !secondary_eligible(&state, ContentStore::embedded(), POK, &player, &card),
            "readying two planets does nothing when none are exhausted"
        );
    }

    #[test]
    fn trade_secondary_is_withheld_when_commodities_are_already_full() {
        let mut state = drafted_three_player_game();
        let player = PlayerId::new("a");
        let card = card_named(&state, "Trade");
        // The bare three-player fixture seats no faction, so the commodity limit resolves to zero
        // and the gate would pass for the wrong reason. Name one with commodities to spend.
        if let Some(seat) = state.player_mut(&player) {
            seat.faction = ti4_model::id::FactionId::new("hacan");
        }
        let limit =
            crate::strategy_cards::commodity_limit(&state, ContentStore::embedded(), &player);
        assert!(limit > 0, "hacan has a commodity value");
        if let Some(seat) = state.player_mut(&player) {
            seat.strategic_tokens = 3;
            seat.commodities = limit;
        }
        assert!(
            !secondary_eligible(&state, ContentStore::embedded(), POK, &player, &card),
            "replenishing to the limit does nothing when already at it"
        );
        if let Some(seat) = state.player_mut(&player) {
            seat.commodities = limit - 1;
        }
        assert!(
            secondary_eligible(&state, ContentStore::embedded(), POK, &player, &card),
            "one commodity short, so there is something to replenish"
        );
    }

    #[test]
    fn warfare_secondary_is_withheld_without_home_production() {
        let mut state = drafted_three_player_game();
        let player = PlayerId::new("a");
        let card = card_named(&state, "Warfare");
        if let Some(seat) = state.player_mut(&player) {
            seat.strategic_tokens = 3;
        }
        // The bare fixture seats no home system, so there is nothing to produce from and the
        // window must stay shut.
        assert!(
            state
                .player(&player)
                .is_some_and(|seat| seat.home_system.is_none()),
            "the fixture seat has no home system"
        );
        assert!(
            !secondary_eligible(&state, ContentStore::embedded(), POK, &player, &card),
            "producing at home does nothing without a home system to produce in"
        );
    }

    #[test]
    fn a_player_with_one_unused_card_keeps_the_legacy_bare_action_id() {
        let players = [PlayerId::new("a"), PlayerId::new("b")];
        let mut state = start_game(ContentStore::embedded(), &players, POK, None).unwrap();
        for _ in 0..2 {
            let choice = strategy_options(&state, ContentStore::embedded()).unwrap();
            take_strategy_card(
                &mut state,
                ContentStore::embedded(),
                choice.options[0].clone(),
            )
            .unwrap();
        }

        let choice =
            strategic_action_options(&state, ContentStore::embedded(), &PlayerId::new("a"))
                .unwrap();

        assert_eq!(choice.ids(), vec!["strategic"]);
    }

    #[test]
    fn each_unused_card_is_a_distinct_action_in_a_multi_card_holding() {
        let state = drafted_three_player_game();
        let choice =
            strategic_action_options(&state, ContentStore::embedded(), &PlayerId::new("a"))
                .unwrap();
        let held = &state.player(&PlayerId::new("a")).unwrap().strategy_cards;

        assert_eq!(
            choice.ids(),
            held.iter()
                .map(|card| format!("strategic|{card}"))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn spending_one_of_two_cards_leaves_the_other_unspent() {
        let mut state = drafted_three_player_game();
        let player = PlayerId::new("a");
        let first = state.player(&player).unwrap().strategy_cards[0].clone();
        let second = state.player(&player).unwrap().strategy_cards[1].clone();

        assert_eq!(
            take_strategic_action(
                &mut state,
                ContentStore::embedded(),
                &player,
                ChoiceOption::new(format!("strategic|{first}"), ACTION_KIND),
            )
            .unwrap(),
            first
        );
        let seat = state.player(&player).unwrap();
        assert!(seat.exhausted_strategy_cards.contains(&first));
        assert!(!seat.exhausted_strategy_cards.contains(&second));
        assert_eq!(
            strategic_action_options(&state, ContentStore::embedded(), &player)
                .unwrap()
                .ids(),
            vec![STRATEGIC_ACTION_ID]
        );
    }

    #[test]
    fn followers_resolve_clockwise_pay_or_decline_then_exhaust_the_primary_card() {
        let (mut state, card) = drafted_with_first_pick("Politics");
        let primary = PlayerId::new("a");
        let first_follower = PlayerId::new("b");
        let second_follower = PlayerId::new("c");
        let first_tokens = state.player(&first_follower).unwrap().strategic_tokens;

        let mut window = begin_strategic_action(
            &mut state,
            ContentStore::embedded(),
            &primary,
            ChoiceOption::new(format!("strategic|{card}"), ACTION_KIND),
        )
        .unwrap();

        assert!(
            !state
                .player(&primary)
                .unwrap()
                .exhausted_strategy_cards
                .contains(&card),
            "the card remains ready while followers decide"
        );
        let choice = window
            .next_choice(&mut state, ContentStore::embedded(), POK)
            .unwrap();
        assert_eq!(choice.player, first_follower);
        assert_eq!(choice.ids(), vec!["no", "yes"]);
        assert_eq!(
            window
                .take_choice(
                    &mut state,
                    ContentStore::embedded(),
                    POK,
                    ChoiceOption::new("yes", "strategy"),
                )
                .unwrap(),
            SecondaryResolution::Followed
        );
        assert_eq!(
            state.player(&first_follower).unwrap().strategic_tokens,
            first_tokens - 1
        );

        let choice = window
            .next_choice(&mut state, ContentStore::embedded(), POK)
            .unwrap();
        assert_eq!(choice.player, second_follower);
        assert_eq!(
            window
                .take_choice(
                    &mut state,
                    ContentStore::embedded(),
                    POK,
                    ChoiceOption::new("no", "strategy"),
                )
                .unwrap(),
            SecondaryResolution::Declined
        );

        assert!(window.is_complete());
        assert!(
            state
                .player(&primary)
                .unwrap()
                .exhausted_strategy_cards
                .contains(&card),
            "the primary card exhausts after every follower has completed"
        );
    }

    #[test]
    fn tokenless_followers_are_recorded_ineligible_and_close_the_window() {
        let mut state = drafted_three_player_game();
        let primary = PlayerId::new("a");
        let card = state
            .player(&primary)
            .unwrap()
            .strategy_cards
            .iter()
            .find(|card| {
                crate::strategy_cards::card_name(ContentStore::embedded(), card.as_str()).as_deref()
                    != Some("Leadership")
            })
            .expect("a three-player hand includes a token-costing card")
            .clone();
        state
            .player_mut(&PlayerId::new("b"))
            .unwrap()
            .strategic_tokens = 0;
        state
            .player_mut(&PlayerId::new("c"))
            .unwrap()
            .strategic_tokens = 0;
        let mut window = begin_strategic_action(
            &mut state,
            ContentStore::embedded(),
            &primary,
            ChoiceOption::new(format!("strategic|{card}"), ACTION_KIND),
        )
        .unwrap();

        assert!(
            window
                .next_choice(&mut state, ContentStore::embedded(), POK)
                .is_none()
        );
        assert_eq!(
            window.resolutions(),
            &[
                (PlayerId::new("b"), SecondaryResolution::Ineligible),
                (PlayerId::new("c"), SecondaryResolution::Ineligible),
            ]
        );
        assert!(window.is_complete());
        assert!(
            state
                .player(&primary)
                .unwrap()
                .exhausted_strategy_cards
                .contains(&card)
        );
    }

    #[test]
    fn an_invented_secondary_response_is_atomic() {
        // A token-costing card with an unconditional secondary: the invented answer must be
        // rejected against a real prompt.
        let (mut state, card) = drafted_with_first_pick("Politics");
        let primary = PlayerId::new("a");
        let mut window = begin_strategic_action(
            &mut state,
            ContentStore::embedded(),
            &primary,
            ChoiceOption::new(format!("strategic|{card}"), ACTION_KIND),
        )
        .unwrap();
        let before = state.clone();

        let error = window
            .take_choice(
                &mut state,
                ContentStore::embedded(),
                POK,
                ChoiceOption::new("invented", STRATEGY_KIND),
            )
            .unwrap_err();

        assert!(matches!(error, StrategySecondaryError::IllegalChoice(_)));
        assert!(state.identical(&before));
        assert!(window.resolutions().is_empty());
        assert!(!window.is_complete());
    }

    #[test]
    fn leadership_secondaries_are_gated_on_influence_affordability() {
        // LRR 52.3 / oracle `_leadership_secondary`: affordability is the only gate — an
        // unaffordable follower makes zero decisions, and an affordable one receives the
        // influence-purchase question itself, not a decline/follow prompt.
        let mut state = drafted_three_player_game();
        let primary = PlayerId::new("a");
        let cards = &state.player(&primary).unwrap().strategy_cards;
        let card = cards
            .iter()
            .find(|card| {
                crate::strategy_cards::card_name(ContentStore::embedded(), card.as_str()).as_deref()
                    == Some("Leadership")
            })
            .cloned()
            .expect("the deterministic draft gives the primary Leadership");
        let affordable = PlayerId::new("b");
        state.player_mut(&affordable).unwrap().trade_goods = 3;

        let mut window = begin_strategic_action(
            &mut state,
            ContentStore::embedded(),
            &primary,
            ChoiceOption::new(format!("strategic|{card}"), ACTION_KIND),
        )
        .unwrap();

        let choice = window
            .next_choice(&mut state, ContentStore::embedded(), POK)
            .unwrap();
        assert_eq!(choice.player, affordable);
        assert_eq!(choice.prompt, "spend 3 influence for a command token");
        assert_eq!(choice.ids(), vec!["no", "yes"]);
        // The window's question is typed like the loop's asks and carries the purchase facts,
        // so a client can plan every purchase and its pool from this one decision.
        assert_eq!(
            choice.context.as_ref().map(|c| c.subtype.as_str()),
            Some("buy_token_with_influence")
        );
        // Its source is the card's secondary, which tells it from the loop's later asks.
        assert_eq!(
            choice.context.as_ref().map(|c| c.source.clone()),
            Some(crate::decision_context::DecisionSource::StrategyCard {
                card: card.to_string(),
                secondary: true,
            })
        );
        assert_eq!(choice.details["kind"], "strategy_secondary");
        assert_eq!(choice.details["mode"], "buy");
        assert_eq!(choice.details["tokens_to_place"], 0);
        let purchase: crate::strategy_cards::PurchaseHints =
            choice.hint("purchase").expect("typed purchase");
        assert_eq!(purchase.influence_available, 3);
        assert_eq!(purchase.max, 1);
        assert_eq!(purchase.payment.cost, 3);

        let resolution = window
            .take_choice(
                &mut state,
                ContentStore::embedded(),
                POK,
                ChoiceOption::new("yes", STRATEGY_KIND),
            )
            .unwrap();
        assert_eq!(resolution, SecondaryResolution::Followed);

        // "c" has no influence: automatically Ineligible with no decision recorded.
        assert!(
            window
                .next_choice(&mut state, ContentStore::embedded(), POK)
                .is_none()
        );
        assert!(window.is_complete());
        assert_eq!(
            window.resolutions(),
            &[
                (affordable.clone(), SecondaryResolution::Followed),
                (PlayerId::new("c"), SecondaryResolution::Ineligible)
            ]
        );
    }

    #[test]
    fn an_invented_strategic_action_is_atomic() {
        let mut state = drafted_three_player_game();
        let before = state.clone();
        let player = PlayerId::new("a");

        let error = take_strategic_action(
            &mut state,
            ContentStore::embedded(),
            &player,
            ChoiceOption::new("strategic|invented", ACTION_KIND),
        )
        .unwrap_err();

        assert!(matches!(error, StrategyActionError::IllegalChoice(_)));
        assert!(state.identical(&before));
    }
}

#[cfg(test)]
mod bf00d_tests {
    use ti4_content::ContentStore;
    use ti4_model::content_types::POK;

    use super::*;
    use crate::factions::hooks_strategy::{SecondaryWaiver, StrategyHooks, with_test_hooks};
    use crate::setup::start_game;

    fn pid(id: &str) -> PlayerId {
        PlayerId::new(id)
    }

    /// Three seats, no cards dealt: every card on the mat.
    fn undealt() -> GameState {
        let players = [pid("a"), pid("b"), pid("c")];
        start_game(ContentStore::embedded(), &players, POK, None).unwrap()
    }

    fn named(state: &GameState, wanted: &str) -> StrategyCardId {
        state
            .unclaimed_strategy_cards
            .iter()
            .find(|card| {
                crate::strategy_cards::card_name(ContentStore::embedded(), card.as_str()).as_deref()
                    == Some(wanted)
            })
            .cloned()
            .expect("the deck carries this card")
    }

    /// Deal a named card from the mat to a seat.
    fn deal(state: &mut GameState, to: &PlayerId, wanted: &str) -> StrategyCardId {
        let card = named(state, wanted);
        state.unclaimed_strategy_cards.retain(|c| c != &card);
        assert!(state.deal_strategy_card(to, card.clone()));
        card
    }

    // -- item 1: initiative override --------------------------------------------------------------

    #[test]
    fn an_initiative_override_puts_a_player_first_until_cleared() {
        let mut state = undealt();
        deal(&mut state, &pid("a"), "Leadership");
        deal(&mut state, &pid("b"), "Diplomacy");
        deal(&mut state, &pid("c"), "Imperial");
        assert_eq!(state.initiative_order(), vec![pid("a"), pid("b"), pid("c")]);

        // The "0" token on the Imperial card: first, whatever the card says.
        assert!(set_initiative_override(&mut state, &pid("c"), 0));
        assert_eq!(state.initiative_order(), vec![pid("c"), pid("a"), pid("b")]);

        // Gift of Prescience hands the token on: the holder takes it, the owner loses it.
        assert!(clear_initiative_override(&mut state, &pid("c")));
        assert!(set_initiative_override(&mut state, &pid("b"), 0));
        assert_eq!(state.initiative_order(), vec![pid("b"), pid("a"), pid("c")]);

        clear_initiative_overrides(&mut state);
        assert_eq!(state.initiative_order(), vec![pid("a"), pid("b"), pid("c")]);
        assert!(!set_initiative_override(&mut state, &pid("nobody"), 0));
        assert!(
            state.initiative_overrides.is_empty(),
            "an unseated id is refused"
        );
    }

    #[test]
    fn two_zero_tokens_tie_by_seating() {
        let mut state = undealt();
        deal(&mut state, &pid("a"), "Leadership");
        deal(&mut state, &pid("b"), "Diplomacy");
        deal(&mut state, &pid("c"), "Imperial");
        set_initiative_override(&mut state, &pid("c"), 0);
        set_initiative_override(&mut state, &pid("b"), 0);
        assert_eq!(state.initiative_order()[..2], [pid("b"), pid("c")]);
    }

    #[test]
    fn the_override_is_invisible_in_serialization_until_used() {
        let mut state = undealt();
        let plain = serde_json::to_value(&state).unwrap();
        assert!(plain.get("initiative_overrides").is_none());
        set_initiative_override(&mut state, &pid("a"), 0);
        let marked = serde_json::to_value(&state).unwrap();
        assert!(marked.get("initiative_overrides").is_some());
        // A snapshot from before the field existed still loads, with no override.
        let back: GameState = serde_json::from_value(plain).unwrap();
        assert!(back.initiative_overrides.is_empty());
        let again: GameState = serde_json::from_value(marked).unwrap();
        assert_eq!(again.initiative_overrides.get(&pid("a")), Some(&0));
    }

    #[test]
    fn the_strategy_phase_ends_through_the_hook_and_the_round_clears_the_token() {
        let mut state = undealt();
        deal(&mut state, &pid("a"), "Leadership");
        deal(&mut state, &pid("b"), "Diplomacy");
        deal(&mut state, &pid("c"), "Imperial");
        state.phase = ti4_model::state::Phase::Strategy;
        let hooks = StrategyHooks {
            strategy_phase_ended: Some(|state| {
                set_initiative_override(state, &PlayerId::new("c"), 0);
            }),
            ..StrategyHooks::NONE
        };
        let outcome = with_test_hooks(hooks, || crate::phase::advance_phase(&mut state));
        assert_eq!(
            outcome,
            crate::phase::PhaseOutcome::ActionBegan(pid("c")),
            "the action phase opens with the player holding the token"
        );
        assert_eq!(state.active, Some(pid("c")));

        state.phase = ti4_model::state::Phase::Status;
        let cards = state.card_initiative.keys().cloned().collect();
        crate::phase::begin_next_round(&mut state, cards);
        assert!(state.initiative_overrides.is_empty());
    }

    // -- item 2: strategy cards -----------------------------------------------------------------

    #[test]
    fn exchanging_cards_moves_each_with_its_own_exhaustion() {
        let mut state = undealt();
        let lead = deal(&mut state, &pid("a"), "Leadership");
        let other = deal(&mut state, &pid("a"), "Warfare");
        let dip = deal(&mut state, &pid("b"), "Diplomacy");
        state.exhaust_strategy_card(&pid("a"), lead.clone());
        state.exhaust_strategy_card(&pid("a"), other.clone());
        assert!(exchange_strategy_cards(
            &mut state,
            &pid("a"),
            &lead,
            &pid("b"),
            &dip
        ));
        let (a, b) = (
            state.player(&pid("a")).unwrap(),
            state.player(&pid("b")).unwrap(),
        );
        assert!(a.strategy_cards.contains(&dip) && !a.strategy_cards.contains(&lead));
        assert!(
            a.strategy_cards.contains(&other),
            "the rest of the holding is kept"
        );
        assert!(b.strategy_cards.contains(&lead));
        assert!(
            !a.exhausted_strategy_cards.contains(&dip),
            "the ready card stays ready"
        );
        assert!(a.exhausted_strategy_cards.contains(&other));
        assert!(
            b.exhausted_strategy_cards.contains(&lead),
            "the exhausted card stays exhausted with its new holder"
        );
    }

    #[test]
    fn a_bad_exchange_changes_nothing() {
        let mut state = undealt();
        let lead = deal(&mut state, &pid("a"), "Leadership");
        let dip = deal(&mut state, &pid("b"), "Diplomacy");
        let imp = named(&state, "Imperial");
        let before = serde_json::to_value(&state).unwrap();
        assert!(!exchange_strategy_cards(
            &mut state,
            &pid("a"),
            &imp,
            &pid("b"),
            &dip
        ));
        assert!(!exchange_strategy_cards(
            &mut state,
            &pid("a"),
            &lead,
            &pid("b"),
            &imp
        ));
        assert!(!exchange_strategy_cards(
            &mut state,
            &pid("a"),
            &lead,
            &pid("a"),
            &lead
        ));
        assert!(!exchange_strategy_cards(
            &mut state,
            &pid("a"),
            &lead,
            &pid("zed"),
            &dip
        ));
        assert_eq!(serde_json::to_value(&state).unwrap(), before);
    }

    #[test]
    fn a_primary_resolves_for_a_card_nobody_holds_and_followers_may_follow_it() {
        let content = ContentStore::embedded();
        let mut state = undealt();
        let lead = named(&state, "Leadership");
        let tokens = |state: &GameState, who: &str| {
            let seat = state.player(&pid(who)).unwrap();
            seat.tactic_tokens + seat.fleet_tokens + seat.strategic_tokens
        };
        let before = tokens(&state, "a");
        let mut table = crate::choice::Table::new();
        let done = crate::strategy_cards::primary(
            &mut state,
            content,
            POK,
            None,
            &mut table,
            &pid("a"),
            lead.as_str(),
        )
        .unwrap();
        assert_eq!(done, crate::strategy_cards::Ability::Resolved);
        assert_eq!(
            tokens(&state, "a"),
            before + i32::try_from(crate::strategy_cards::LEADERSHIP_TOKENS).unwrap(),
            "Winnu's hero resolves the primary of a card Winnu does not hold"
        );

        // The follower window for it exhausts nothing (no one holds it) and does not panic.
        let imp = named(&state, "Imperial");
        let mut window = StrategySecondaryWindow::foreign(pid("a"), imp, vec![pid("b")]);
        state.player_mut(&pid("b")).unwrap().strategic_tokens = 2;
        let choice = window.next_choice(&mut state, content, POK).unwrap();
        assert_eq!(choice.player, pid("b"));
        window
            .take_choice(
                &mut state,
                content,
                POK,
                ChoiceOption::new("yes", STRATEGY_KIND),
            )
            .unwrap();
        assert!(window.is_complete());
        assert_eq!(
            state.player(&pid("b")).unwrap().strategic_tokens,
            1,
            "the usual cost"
        );
        assert!(
            state
                .player(&pid("a"))
                .unwrap()
                .exhausted_strategy_cards
                .is_empty()
        );
    }

    #[test]
    fn a_waiver_lets_a_tokenless_follower_follow_for_free_and_is_paid_by_its_module() {
        let content = ContentStore::embedded();
        let mut state = undealt();
        deal(&mut state, &pid("a"), "Imperial");
        state.player_mut(&pid("b")).unwrap().strategic_tokens = 0;
        state.player_mut(&pid("c")).unwrap().strategic_tokens = 2;
        let strategic = ChoiceOption::new(STRATEGIC_ACTION_ID, ACTION_KIND);

        // No module: the tokenless follower is skipped; the other sees the plain options.
        let mut copy = state.clone();
        let mut plain =
            begin_strategic_action(&mut copy, content, &pid("a"), strategic.clone()).unwrap();
        let choice = plain.next_choice(&mut copy, content, POK).unwrap();
        assert_eq!(choice.player, pid("c"));
        assert_eq!(
            choice.ids(),
            vec!["no", "yes"],
            "neutral without a waiver hook"
        );

        let hooks = StrategyHooks {
            secondary_waivers: Some(|_, _, follower, primary, card| {
                if follower.as_str() == "b" && primary.as_str() == "a" && card == "Imperial" {
                    let worded = crate::phrases::Phrase::of("follow {primary} using Acquiescence")
                        .with("primary", primary, crate::phrases::Subject::seat(primary));
                    vec![SecondaryWaiver {
                        id: "acq".to_owned(),
                        label: worded.label(),
                        worded: Some(worded),
                    }]
                } else {
                    Vec::new()
                }
            }),
            secondary_waived: Some(|state, _, follower, primary, waiver| {
                state.faction_marks.insert(
                    format!("test:waived:{follower}:{primary}"),
                    waiver.to_owned(),
                );
            }),
            ..StrategyHooks::NONE
        };
        with_test_hooks(hooks, || {
            let mut window =
                begin_strategic_action(&mut state, content, &pid("a"), strategic.clone()).unwrap();
            let pending = window.pending_choice(&state, content, POK).unwrap();
            assert_eq!(pending.player, pid("b"), "tokenless but waived: offered");
            // The waiver id carries its module's position, which depends on how many real faction
            // modules also answer this hook; find the test hook's option rather than assume it.
            let ids = pending.ids();
            assert_eq!(ids.first().copied(), Some("no"));
            assert!(
                !ids.contains(&"yes"),
                "tokenless: the waiver replaces a \"yes\" that could not be paid"
            );
            let waived = ids
                .iter()
                .find(|id| id.starts_with(WAIVED_SECONDARY_PREFIX) && id.ends_with("|acq"))
                .map(|id| (*id).to_owned())
                .expect("the waiver is offered");
            // What its label names rides with the question (`details.phrases`).
            let phrase = &pending.phrases().options[&waived];
            assert_eq!(
                pending.option(&waived).map(|option| option.label.as_str()),
                Some("follow a using Acquiescence")
            );
            assert_eq!(
                phrase.refs["primary"],
                crate::phrases::Subject::seat(&pid("a"))
            );
            let resolved = window
                .take_choice(
                    &mut state,
                    content,
                    POK,
                    ChoiceOption::new(&waived, STRATEGY_KIND),
                )
                .unwrap();
            assert_eq!(resolved, SecondaryResolution::Followed);
            assert_eq!(state.player(&pid("b")).unwrap().strategic_tokens, 0);
            assert_eq!(
                state
                    .faction_marks
                    .get("test:waived:b:a")
                    .map(String::as_str),
                Some("acq")
            );
            // The next follower has no waiver: plain options, pays as usual.
            let next = window.pending_choice(&state, content, POK).unwrap();
            assert_eq!(next.player, pid("c"));
            assert_eq!(next.ids(), vec!["no", "yes"]);
        });
    }

    #[test]
    fn an_unoffered_waiver_is_refused_and_changes_nothing() {
        let content = ContentStore::embedded();
        let mut state = undealt();
        deal(&mut state, &pid("a"), "Imperial");
        state.player_mut(&pid("b")).unwrap().strategic_tokens = 2;
        let mut window = begin_strategic_action(
            &mut state,
            content,
            &pid("a"),
            ChoiceOption::new(STRATEGIC_ACTION_ID, ACTION_KIND),
        )
        .unwrap();
        let before = serde_json::to_value(&state).unwrap();
        let refused = window.take_choice(
            &mut state,
            content,
            POK,
            ChoiceOption::new("follow|waived|0|acq", STRATEGY_KIND),
        );
        assert!(refused.is_err());
        assert_eq!(serde_json::to_value(&state).unwrap(), before);
    }
}
