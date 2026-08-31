//! Action cards: draw, hand limit, discard (LRR 2).
//!
//! Ported from the oracle's `engine/action_cards.py`: `draw`, `_first_of_each`,
//! `enforce_hand_limit`, `discard` and `unimplemented`.

use std::collections::{BTreeMap, BTreeSet};

use ti4_content::ContentStore;
use ti4_content::galaxy::Galaxy;
use ti4_content::units::UnitType;
use ti4_model::content_types::{ContentType, POK, SourceSet};
use ti4_model::id::{ActionCardId, PlayerId, StrategyCardId};
use ti4_model::state::{GameState, TransientFlags};
use ti4_model::units::Unit;

use crate::choice::{Choice, ChoiceOption, IllegalChoice, Observed, Table};

/// 2.4: seven cards in hand at the end of a turn.
pub const HAND_LIMIT: usize = 7;

/// The choice kind for discarding down to the limit.
pub const DISCARD_KIND: &str = "discard";

/// A card's printed name, falling back to its alias.
///
/// The alias is not the card. A card printed four times has four aliases — Morale Boost is
/// `mb1` to `mb4`, and so are Flank Speed, Skilled Retreat and Maneuvering Jets — so two copies
/// in one hand are two *different* aliases carrying identical text.
#[must_use]
pub fn name_of(content: &ContentStore, card: &ActionCardId) -> String {
    content
        .get(ContentType::ActionCards, card.as_str())
        .and_then(|record| record.text("name"))
        .unwrap_or_else(|| card.as_str())
        .to_owned()
}

/// The first index of each distinct *card* in a hand, keyed by printed name.
///
/// Keying on the alias would leave two copies looking distinct, and the hand would offer the
/// same card twice — which is not free, because a sampling decider draws per option.
#[must_use]
pub fn first_of_each(content: &ContentStore, hand: &[ActionCardId]) -> BTreeMap<String, usize> {
    let mut first = BTreeMap::new();
    for (index, card) in hand.iter().enumerate() {
        first.entry(name_of(content, card)).or_insert(index);
    }
    first
}

/// 2.3: take from the top of the deck into hand.
///
/// Returns what was drawn. An empty deck draws nothing rather than reshuffling: this engine
/// tracks no discard pile, so there is nothing to shuffle back, and inventing a fresh deck
/// would hand out cards that are already in someone's hand.
///
/// # Errors
/// [`IllegalChoice`] when a decider answers the hand-limit discard with something not offered.
pub fn draw(
    state: &mut GameState,
    content: &ContentStore,
    table: &mut Table,
    player: &PlayerId,
    count: usize,
) -> Result<Vec<ActionCardId>, IllegalChoice> {
    let mut drawn = Vec::new();
    for _ in 0..count {
        if state.action_card_deck.is_empty() {
            break;
        }
        let top = state.action_card_deck.remove(0);
        if let Some(seat) = state.player_mut(player) {
            seat.action_cards.push(top.clone());
        }
        drawn.push(top);
    }
    enforce_hand_limit(state, content, table, player)?;
    Ok(drawn)
}

/// 2.4: over the limit, the player chooses which to discard.
///
/// # Errors
/// [`IllegalChoice`] when a decider answers with something not offered.
pub fn enforce_hand_limit(
    state: &mut GameState,
    content: &ContentStore,
    table: &mut Table,
    player: &PlayerId,
) -> Result<(), IllegalChoice> {
    loop {
        let hand = state
            .player(player)
            .map(|seat| seat.action_cards.clone())
            .unwrap_or_default();
        // Sanctions caps the hand at three; without it the printed seven applies.
        if hand.len() <= crate::laws::action_card_limit(state, HAND_LIMIT) {
            return Ok(());
        }

        // One option per distinct card. Two copies of Maneuvering Jets are one decision
        // written twice, and the copy is not free: a sampling decider draws per option, so the
        // card held two of was likelier to be discarded than the one held one of, whatever it
        // thought of either.
        let distinct = first_of_each(content, &hand);
        let options: Vec<ChoiceOption> = distinct
            .iter()
            .map(|(name, index)| {
                ChoiceOption::labelled(index.to_string(), DISCARD_KIND, name.clone())
            })
            .collect();
        let choice = Choice::new(
            player.clone(),
            format!("over the hand limit — discard one of {}", hand.len()),
            options,
        );
        let answer = table.ask_seeing(&choice, &Observed::new(state, content, POK, None))?;
        let index = answer.id.parse::<usize>().unwrap_or(0);
        discard(state, player, index);
    }
}

/// Remove one card from a hand by index.
pub fn discard(state: &mut GameState, player: &PlayerId, index: usize) -> Option<ActionCardId> {
    let seat = state.player_mut(player)?;
    if index >= seat.action_cards.len() {
        return None;
    }
    Some(seat.action_cards.remove(index))
}

// -- the component action (22.1) -----------------------------------------------------------------

/// The kind of a component-action option.
pub const ACTION_KIND: &str = "component";

/// The prefix of an option that plays an action card as a turn action.
const PLAY_PREFIX: &str = "action_card|";

/// Whether a card's printed window makes it a component action (22.1).
#[must_use]
pub fn is_component_action(content: &ContentStore, alias: &ActionCardId) -> bool {
    content
        .get(ContentType::ActionCards, alias.as_str())
        .and_then(|record| record.text("window"))
        .is_some_and(|window| window.trim() == "Action")
}

/// 22.3: whether this card could be played right now.
///
/// A card nobody has modelled has no stated requirement, so it stays offered — which is what
/// keeps an unimplemented card countable rather than invisible. The alternative hides the gap
/// behind a card that simply never appears.
#[must_use]
pub fn is_playable(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: Option<&Galaxy>,
    player: &PlayerId,
    alias: &ActionCardId,
) -> bool {
    if crate::laws::action_cards_forbidden(state, player) {
        return false;
    }
    // Oracle parity (engine/action_cards.py): each card's printed text gates itself. Signal
    // Jamming needs a jammable system and an opponent whose token can be stranded. The other
    // cards' eligibility lambdas are not modelled yet (F13 backlog).
    if alias.as_str() == "jamming" {
        return !jamming_systems(state, content, sources, galaxy, player).is_empty()
            && state.players.len() > 1;
    }
    true
}

/// Component actions this player could take with the cards in hand (22.1).
///
/// Indexed by hand position rather than by alias, because a hand may hold two copies of the
/// same card and naming the alias would make them one option — which a sampling decider would
/// then draw half as often as it should.
#[must_use]
pub fn available_actions(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: Option<&Galaxy>,
    player: &PlayerId,
) -> Vec<crate::choice::ChoiceOption> {
    let Some(seat) = state.player(player) else {
        return Vec::new();
    };
    seat.action_cards
        .iter()
        .enumerate()
        .filter(|(_, alias)| is_component_action(content, alias))
        .filter(|(_, alias)| is_playable(state, content, sources, galaxy, player, alias))
        .map(|(index, alias)| {
            crate::choice::ChoiceOption::labelled(
                format!("{PLAY_PREFIX}{index}"),
                ACTION_KIND,
                format!("play {}", name_of(content, alias)),
            )
        })
        .collect()
}

/// Play an action card as a component action.
///
/// Returns `false` when the card was not performed: the option is not a playable component
/// action, or its play was cancelled while announced (e.g. by Sabotage). In both cases the
/// caller must not treat the player's action as used -- 22.3 forbids performing what cannot
/// be resolved, and 22.4 says a cancelled component action does not consume the turn. A
/// cancelled play is still spent: the card is discarded either way.
///
/// # Errors
/// [`crate::timing::TimingError`] when announcing the play cannot be resolved.
pub fn perform(
    context: &mut crate::timing::TimingContext<'_>,
    resolver: &mut crate::timing::Resolver,
    player: &PlayerId,
    option: &crate::choice::ChoiceOption,
) -> Result<bool, crate::timing::TimingError> {
    let Some(index) = option
        .id
        .strip_prefix(PLAY_PREFIX)
        .and_then(|index| index.parse::<usize>().ok())
    else {
        return Ok(false);
    };
    let held = context
        .state
        .player(player)
        .and_then(|seat| seat.action_cards.get(index).cloned());
    let Some(alias) = held else {
        return Ok(false);
    };
    if !is_component_action(context.content, &alias)
        || !is_playable(
            context.state,
            context.content,
            context.sources,
            context.galaxy,
            player,
            &alias,
        )
    {
        return Ok(false);
    }

    // 22.3: the card leaves the hand whether or not its effect is modelled. It was genuinely
    // played, and pretending otherwise would let a bot hold it for ever.
    discard(context.state, player, index);
    // `announce` reports whether the play stood. A cancelled play still discards the card,
    // but returns `false` so the caller keeps the turn instead of advancing it (22.4).
    crate::reactions::announce(context, resolver, player, &alias)
}

// -- effects (M06-016b) --------------------------------------------------------------------------

/// What playing an action card does.
///
/// A card with no entry here is played, announced, and reports itself unresolved — the registry
/// design used throughout this engine. A card that silently did nothing would be indistinguishable
/// from one that worked.
pub type Effect = fn(&mut crate::timing::TimingContext<'_>, &PlayerId);

/// Morale Boost: "+1 to the result of each of your unit's combat rolls during this combat round."
///
/// Scoped to [`GameState::combat_round_seq`] rather than a flag, so the bonus expires with the
/// round it was played in. A flag would improve every later round of the same combat too.
fn morale_boost(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    let round = context.state.combat_round_seq;
    if let Some(seat) = context.state.player_mut(player) {
        seat.combat_bonus_round = Some(round);
    }
}

/// Flank Speed: "+1 to the move value of each of your ships during this tactical action."
///
/// Scoped to [`GameState::activation_seq`] for the same reason: the card says *this* tactical
/// action, and an unscoped bonus would follow the fleet for the rest of the game.
fn flank_speed(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    let activation = context.state.activation_seq;
    if let Some(seat) = context.state.player_mut(player) {
        seat.move_bonus_activation = Some(activation);
    }
}

/// Solar Flare: "During the 'Movement' step of this tactical action, other players cannot use
/// SPACE CANNON against your ships."
///
/// The engine's cannon step is the one that belongs to the named tactical action, so the marker
/// is activation-scoped like the card's wording. [`crate::combat::space_cannon_offense`] reads
/// it and suppresses the whole step: every gun in that step belongs to another player and fires
/// at this player's ships, which is exactly what the card forbids.
fn solar_flare(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    let activation = context.state.activation_seq;
    if let Some(seat) = context.state.player_mut(player) {
        seat.solar_flare.push(activation);
    }
}

/// Lost Star Chart: "During this tactical action, systems that contain alpha and beta wormholes
/// are adjacent to each other."
///
/// The adjacency itself is a switch on the map, re-derived every step by
/// [`crate::laws::apply_to_galaxy`] from the active player's marker — the same shape as the
/// wormhole laws, so no movement path can consult a map that forgot the card. The marker scopes
/// the effect to the tactical action the card was played in.
///
/// On this map 82b Mallice - Nexus is the only system carrying both an alpha and a beta
/// wormhole, so the card changes no actual adjacency in a base game: a single system has no
/// partner. The switch and the marker are still implemented as printed, and the link rule is
/// pinned by the galaxy's own tests.
fn lost_star(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    let activation = context.state.activation_seq;
    if let Some(seat) = context.state.player_mut(player) {
        seat.lost_star.push(activation);
    }
}

/// Sabotage: "When another player plays an action card other than 'Sabotage': cancel that
/// action card."
///
/// The cancellation itself happens in the reaction slot that owns the triggering
/// `ACTION_CARD_PLAYED` event (`crate::reactions`): the slot is the only place that still
/// holds that event, because an effect signature carries no event and by the time the
/// played card's effect runs, the triggering event is back in its own frame. Playing
/// Sabotage spends the card (the slot discards it first), announces it, and cancels the
/// card that was being played — whose effect never runs and whose spend stands (1.15
/// cancels the event, not the spend). This entry exists so a played Sabotage reports as
/// resolved rather than `ACTION_CARD_UNRESOLVED`; do not move the cancellation here.
fn sabotage(_: &mut crate::timing::TimingContext<'_>, _: &PlayerId) {}

/// Veto (all three copies): "When an agenda is revealed: discard that agenda and reveal 1
/// agenda from the top of the deck. Players vote on this agenda instead."
///
/// The effect runs inside the `AGENDA_REVEALED` window, which the vote driver opens before
/// it builds the vote window — so the card cannot tear down the vote that is about to open.
/// Instead it draws the replacement from the top of the agenda deck (the two agendas revealed
/// this phase are already out of it, so this is the next one behind them) and hands it to
/// the driver via [`GameState::agenda_veto_replacement`]; the driver discards the revealed
/// agenda and opens the vote on the replacement instead.
///
/// An empty agenda deck has nothing to reveal, and the card then does nothing: a corner that
/// cannot arise in a full game, where the 63-card deck sheds two agendas a phase.
fn veto(context: &mut crate::timing::TimingContext<'_>, _: &PlayerId) {
    let Some(replacement) = context.state.agenda_deck.first().cloned() else {
        return;
    };
    context.state.agenda_deck.remove(0);
    context.state.agenda_veto_replacement = Some(replacement);
}

/// Confounding Legal Text: "When another player is elected as the outcome of an agenda: you
/// are the elected player instead."
///
/// The holder becomes the elected player. The redirect is recorded on
/// [`GameState::agenda_elected_override`], which the vote driver reads after the
/// `AGENDA_RESOLVED` window closes and applies the agenda's own effect to this seat instead
/// of the one the ballots elected.
fn confounding(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    context.state.agenda_elected_override = Some(player.clone());
}

/// Confusing Legal Text: "When you are elected as the outcome of an agenda: choose 1 player.
/// That player is the elected player instead."
///
/// The elected player redirects the election. With one other seat there is nothing to choose,
/// so the card takes that seat outright; otherwise the choice is asked through the game's
/// table and the redirect is recorded on [`GameState::agenda_elected_override`] for the vote
/// driver, which applies the agenda's own effect to the chosen player.
fn confusing(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    let others: Vec<PlayerId> = context
        .state
        .players
        .iter()
        .map(|seat| seat.id.clone())
        .filter(|id| id != player)
        .collect();
    if others.is_empty() {
        return; // nobody else to redirect the election to
    }
    let elected = if others.len() == 1 {
        others.into_iter().next().unwrap()
    } else {
        let choice = Choice::new(
            player.clone(),
            "Confusing Legal Text: who is the elected player instead",
            others
                .iter()
                .map(|id| ChoiceOption::labelled(id.to_string(), "elect", format!("elect {id}")))
                .collect(),
        );
        match context.ask_seeing(&choice) {
            Ok(answer) => PlayerId::new(answer.id),
            Err(_) => return,
        }
    };
    context.state.agenda_elected_override = Some(elected);
}

/// Deadly Plot: "During the agenda phase when an outcome would be resolved: If you voted for
/// or predicted another outcome, discard the agenda instead. The agenda is resolved with no
/// effect and it is not replaced. Then, exhaust all of your planets."
///
/// The guard (vote vs prediction vs the outcome about to resolve) is answered by the window
/// machinery before the effect runs, so reaching this point means the discard applies.
/// [`GameState::agenda_outcome_discarded`] tells the vote driver to spend the resolution on
/// nothing: no agenda effect, no prediction payout, no law, no elected feat — but the vote
/// itself still happened, so its occurrence window still opens. "Not replaced" has nothing
/// to suppress in this engine: it never draws a replacement after a resolution.
///
/// Then the holder exhausts every planet they control — the card names "your planets",
/// which are the ones on this player's board, not the planets the agenda would have touched.
fn deadly_plot(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    context
        .state
        .transient_flags
        .set(TransientFlags::AGENDA_DISCARDED);
    let planets: Vec<ti4_model::id::PlanetId> = context
        .state
        .board
        .values()
        .flat_map(|system| system.planet_control.iter())
        .filter_map(|(planet, owner)| (owner == player).then_some(planet.clone()))
        .collect();
    for planet in planets {
        context.state.exhaust_planet(planet);
    }
}

/// Coup d'Etat: "When another player would perform a strategic action: End that player's
/// turn, the strategic action is not resolved and the strategy card is not exhausted."
///
/// The driver fires the typed `STRATEGIC_ACTION_BEGAN` event before it resolves anything, so
/// setting `TransientFlags::STRATEGIC_CANCELLED` here still undoes the action entirely:
/// the card goes back to hand unexhausted, no token is placed, no ability runs, and the
/// victim's turn simply ends.
fn coup(context: &mut crate::timing::TimingContext<'_>, _: &PlayerId) {
    context
        .state
        .transient_flags
        .set(TransientFlags::STRATEGIC_CANCELLED);
}

/// Crisis: "At the end of any player's turn, if there are at least 2 players who have not
/// passed: Skip the next player's turn."
///
/// The guard counts the unpassed seats at the moment a turn ends; the effect arms
/// `TransientFlags::SKIP_NEXT_TURN`, which the turn driver consumes on the very next advance —
/// the seat the turn just landed on is skipped and never acts.
fn crisis(context: &mut crate::timing::TimingContext<'_>, _: &PlayerId) {
    context
        .state
        .transient_flags
        .set(TransientFlags::SKIP_NEXT_TURN);
}

/// Master Plan: "After you perform an action: Perform an additional action."
///
/// The driver fires `ACTION_COMPLETED` when the action is over, and the effect arms
/// `TransientFlags::ADDITIONAL_ACTION`: the next turn advance keeps the same seat — no
/// `turn_seq` bump, no end-of-turn tech, no transaction reset — and the player simply takes
/// another action.
fn master_plan(context: &mut crate::timing::TimingContext<'_>, _: &PlayerId) {
    context
        .state
        .transient_flags
        .set(TransientFlags::ADDITIONAL_ACTION);
}

/// Hack Election: "After an agenda is revealed: During this agenda, you vote last."
///
/// The marker is keyed to `agenda_seq`, which `reveal_agenda` bumps before its window opens,
/// so it binds to the vote that reveal produces — including a Veto replacement, which is
/// voted on in the same cycle — and expires at the next reveal without cleanup. The vote
/// order reads it in `VoteWindow::new` and moves the holder to the last seat, after the
/// speaker.
fn hack_election(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    let agenda = context.state.agenda_seq;
    if let Some(seat) = context.state.player_mut(player) {
        seat.hack_votes_last_agenda = Some(agenda);
    }
}

/// Summit: "At the start of the strategy phase: Gain 2 command tokens."
///
/// The tokens name no pool, so each is placed individually into a pool of the holder's
/// choice (52.4). The window opens when the strategy phase begins and the questions are
/// asked from the card's own timing context.
fn summit(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    let _ = crate::strategy_cards::gain_tokens(
        context.state,
        context.content,
        context.sources,
        context.galaxy,
        context.table,
        player,
        2,
    );
}

/// Political Stability: "When you would return your strategy card(s) during the status
/// phase: Do not return your strategy card(s). You do not choose strategy cards during
/// the next strategy phase."
///
/// The driver fires `STRATEGY_CARDS_WOULD_RETURN` per seat before 81.8 returns the cards,
/// and the effect marks the seat. The marker does the card's two halves: 81.8 keeps the
/// seat's cards (readying any the seat spent), and the draft skips the seat in the
/// strategy phase that follows. It is cleared when that action phase begins, and the
/// retained cards then go back to the mat in the following round's status phase.
fn political_stability(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    if let Some(seat) = context.state.player_mut(player) {
        seat.stability = true;
    }
}

/// Public Disgrace: "When another player chooses a strategy card during the strategy
/// phase: That player must choose a different strategy card instead, if able."
///
/// The driver records the picker and the chosen card in `last_strategy_choice` before
/// firing `STRATEGY_CARD_CHOSEN` — the event payload is consumed by the timing machinery,
/// which the effect cannot see. The chosen card goes back to the mat, the picker is asked
/// to choose again from what the mat now holds, and "if able" means the first choice
/// simply stands when nothing else remains. A failed question restores the first choice
/// exactly: the card goes back to the picker and off the mat again.
fn public_disgrace(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    let Some((picker, first)) = context.state.last_strategy_choice.clone() else {
        return;
    };
    if picker == *player {
        return; // the row's guard says "another player"; the card cannot act on itself
    }
    let Some(position) = context
        .state
        .player(&picker)
        .and_then(|seat| seat.strategy_cards.iter().position(|card| card == &first))
    else {
        return;
    };
    {
        let seat = context.state.player_mut(&picker).expect("just read");
        seat.strategy_cards.remove(position);
    }
    context.state.unclaimed_strategy_cards.push(first.clone());
    let alternatives: Vec<ChoiceOption> = context
        .state
        .unclaimed_strategy_cards
        .iter()
        .filter(|card| **card != first)
        .map(|card| {
            ChoiceOption::labelled(
                card.as_str().to_owned(),
                crate::draft::STRATEGY_CARD_KIND,
                crate::draft::strategy_card_label(context.content, card.as_str()),
            )
        })
        .collect();
    if alternatives.is_empty() {
        // "If able": nothing else is on the mat, so the first choice stands.
        restore_first_choice(context, &picker, &first);
        return;
    }
    let choice = Choice::new(
        picker.clone(),
        "choose a different strategy card",
        alternatives,
    );
    let Ok(answer) = context.ask_seeing(&choice) else {
        restore_first_choice(context, &picker, &first);
        return;
    };
    let picked = StrategyCardId::new(answer.id.clone());
    let Some(at) = context
        .state
        .unclaimed_strategy_cards
        .iter()
        .position(|card| card == &picked)
    else {
        restore_first_choice(context, &picker, &first);
        return;
    };
    context.state.unclaimed_strategy_cards.remove(at);
    let _ = context.state.deal_strategy_card(&picker, picked);
}

/// Put the first choice back with its picker and off the mat — the exact restoration
/// [`public_disgrace`] makes when the re-choice fails or when "if able" offers nothing.
fn restore_first_choice(
    context: &mut crate::timing::TimingContext<'_>,
    picker: &PlayerId,
    first: &StrategyCardId,
) {
    if let Some(at) = context
        .state
        .unclaimed_strategy_cards
        .iter()
        .position(|card| card == first)
    {
        context.state.unclaimed_strategy_cards.remove(at);
    }
    let _ = context.state.deal_strategy_card(picker, first.clone());
}

/// Puppets on a String: "At the end of a player's turn, if you have passed: Perform 1
/// action."
///
/// The row's `actor_is` guard means the window opens only for the seat whose turn ended
/// — the seat that has just passed. The effect arms `TransientFlags::PUPPET_ACTION`, and
/// the turn advance hands that passer a fresh turn — new `turn_seq`, start-of-turn hooks,
/// `TURN_BEGAN` window — in which exactly one action is taken. The seat stays `passed`:
/// the grant is one action, not a return from pass, so the advance after it moves on like
/// any other passed seat.
fn puppets_on_a_string(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    let Some(seat) = context.state.player(player) else {
        return;
    };
    if !seat.passed {
        return; // belt and braces: the row's guard already requires it
    }
    context
        .state
        .transient_flags
        .set(TransientFlags::PUPPET_ACTION);
}

/// Extreme Duress: "At the start of another player's turn, if they have a readied
/// strategy card: If that player's next action is not a strategic action, they discard
/// all of their action cards, give you all of their trade goods, and show you all of
/// their secret objectives."
///
/// The window fires at the start of the target's turn — the row's guard requires the new
/// active seat to hold a readied strategy card — and the effect marks the target with
/// the holder. The punishment is deferred: when the target next takes an action the
/// driver settles the marker. A strategic action lifts the duress quietly, and any other
/// action triggers it (see `Game::settle_extreme_duress`). Passing is not an action, so
/// it neither triggers nor lifts it.
fn extreme_duress(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    let Some(target) = context.state.active.clone() else {
        return;
    };
    if target == *player {
        return;
    }
    if let Some(seat) = context.state.player_mut(&target) {
        seat.duress_by = Some(player.clone());
    }
}

/// Black Market Dealings: "When you are negotiating a transaction: You and the other player
/// may include relics, action cards, and unscored secret objectives as part of the
/// transaction. This card cannot be canceled."
///
/// The play happens inside the window the negotiation's opening opens, and its effect is to
/// mark the negotiation in flight. When the driver asks its first question, the marked table
/// is offered the two extra asset kinds on top of the usual shapes: action cards — the shape
/// otherwise gated behind Arbiters — plus unscored secret objectives and relic fragments
/// ("relics" are the fragments, per the 5th-printing clarification; LRR 73.4 keeps full relics
/// untradeable, and the engine trades only fragments anyway). The marker is cleared when the
/// negotiation closes or the turn ends, so it can never bleed into another player's table. The
/// "cannot be canceled" clause is structural: once the flag is set, nothing else touches it, so
/// no counter-play can strip the terms back away.
fn blackmarketdealings(context: &mut crate::timing::TimingContext<'_>, _player: &PlayerId) {
    context
        .state
        .transient_flags
        .set(ti4_model::state::TransientFlags::BLACK_MARKET);
}

/// Infiltrate: "When you gain control of a planet: Replace each PDS and space dock that is on
/// that planet with a matching unit from your reinforcements."
///
/// The frame names the capture it reacts to in `last_control_gained`, because the card cannot
/// read the event that summoned it. The capture has already destroyed every structure that was
/// not the new controller's, so the only PDS or space dock still standing on the planet is the
/// controller's own, and those are the units the card replaces, one for one. "From your
/// reinforcements" is the box's supply: a replacement arrives only if the game still has that
/// kind of unit to give, otherwise the unit that was there stays.
fn infiltrate(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    let Some((system, planet, _gained, _previous)) = context.state.last_control_gained.clone()
    else {
        return;
    };
    let types = ti4_content::units::catalogue(context.content, context.sources);
    let standing: Vec<ti4_model::units::Unit> = context
        .state
        .system_state(&system)
        .on_planet(&planet)
        .iter()
        .filter(|unit| &unit.owner == player)
        .filter(|unit| {
            types
                .get(unit.type_id.as_str())
                .is_some_and(|kind| kind.base_type() == "pds" || kind.base_type() == "spacedock")
        })
        .cloned()
        .collect();
    for unit in standing {
        if crate::supply::allowed(
            context.state,
            context.content,
            context.sources,
            player,
            &unit.type_id,
            1,
        ) == 0
        {
            continue; // the box holds no more of this kind: nothing to replace it with
        }
        let kind = unit.type_id.clone();
        context.state.system_mut(&system).replace_planet_unit(
            &planet,
            &unit,
            ti4_model::units::Unit::new(kind, player.clone()),
        );
    }
}

/// Reparations: "After another player gains control of a planet you control: Exhaust 1 planet
/// that player controls and ready 1 planet you control."
///
/// The frame hands over who the planet was taken from, because control has already changed by
/// the time the window runs and the board alone answers with the new controller. The new
/// controller chooses which of their planets exhausts — the one just taken is among them — and
/// the holder chooses which of their exhausted planets readies; a single candidate needs no
/// question, and a side with no candidate simply skips its half.
fn reparations(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    let Some((_system, _planet, gainer, previous)) = context.state.last_control_gained.clone()
    else {
        return;
    };
    // "A planet you control" is about the planet's former holder, not its new one.
    if previous.as_ref() != Some(player) || &gainer == player {
        return;
    }
    let controlled_by = |context: &crate::timing::TimingContext<'_>, who: &PlayerId| {
        let mut planets: std::collections::BTreeSet<ti4_model::id::PlanetId> =
            std::collections::BTreeSet::new();
        for board in &context.state.board {
            for (planet, holder) in &board.1.planet_control {
                if holder == who {
                    planets.insert(planet.clone());
                }
            }
        }
        planets
    };
    // Exhaust 1 planet the new controller controls — theirs to choose.
    let gainer_planets = controlled_by(context, &gainer);
    if !gainer_planets.is_empty() {
        let exhaust = if gainer_planets.len() == 1 {
            gainer_planets.into_iter().next().expect("one candidate")
        } else {
            let options = gainer_planets
                .iter()
                .map(|planet| {
                    crate::choice::ChoiceOption::labelled(
                        planet.to_string(),
                        "reparations_exhaust",
                        planet.to_string(),
                    )
                })
                .collect();
            let choice = crate::choice::Choice::new(
                gainer.clone(),
                "exhaust a planet (Reparations)",
                options,
            );
            match context.table.ask_seeing(
                &choice,
                &crate::choice::Observed::new(
                    context.state,
                    context.content,
                    context.sources,
                    context.galaxy,
                ),
            ) {
                Ok(answer) => ti4_model::id::PlanetId::new(&answer.id),
                Err(_) => return,
            }
        };
        context.state.exhaust_planet(exhaust);
    }
    // Ready 1 planet the holder controls — among their exhausted ones, since readying a ready
    // planet is not a thing the board records.
    let own: Vec<_> = controlled_by(context, player)
        .into_iter()
        .filter(|planet| context.state.exhausted_planets.contains(planet))
        .collect();
    if !own.is_empty() {
        let ready = if own.len() == 1 {
            own.into_iter().next().expect("one candidate")
        } else {
            let options = own
                .iter()
                .map(|planet| {
                    crate::choice::ChoiceOption::labelled(
                        planet.to_string(),
                        "reparations_ready",
                        planet.to_string(),
                    )
                })
                .collect();
            let choice =
                crate::choice::Choice::new(player.clone(), "ready a planet (Reparations)", options);
            match context.table.ask_seeing(
                &choice,
                &crate::choice::Observed::new(
                    context.state,
                    context.content,
                    context.sources,
                    context.galaxy,
                ),
            ) {
                Ok(answer) => ti4_model::id::PlanetId::new(&answer.id),
                Err(_) => return,
            }
        };
        context.state.ready_planet(&ready);
    }
}

/// Salvage: "After you win a space combat: Your opponent gives you all of their
/// commodities."
///
/// The frame names the opponents the winner fought — the sides the fight opened with, minus
/// the winner — because the losers' ships are off the board by the time the window runs and
/// the board alone answers with the winner. Each opponent gives up every commodity they hold,
/// and a commodity becomes a trade good the moment it changes hands (21.5), the same handoff
/// the transaction makes, so the goods arrive as goods.
fn salvage(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    let Some((_system, opponents)) = context.state.last_combat_sides.clone() else {
        return;
    };
    let mut goods = 0i32;
    for opponent in &opponents {
        if opponent == player {
            continue;
        }
        if let Some(seat) = context.state.player_mut(opponent) {
            goods += seat.commodities;
            seat.commodities = 0;
        }
    }
    if goods > 0
        && let Some(winner) = context.state.player_mut(player)
    {
        winner.commodities += goods;
    }
}

/// Reverse Engineer: "After another player discards an action card that has a component
/// action: Take that action card from the discard pile."
///
/// The frame names the discarded card in `last_action_discarded` and has already put it in
/// the pile, because the card cannot read the event that summoned it and the pile is where
/// the printed card text sends it. The window's printed qualifier — the discarded card has a
/// component action — is checked here, against the content, and a second holder who acted
/// first leaves the pile empty for the one who acted after: the card is taken, not copied.
fn reverse_engineer(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    let Some((by, alias)) = context.state.last_action_discarded.clone() else {
        return;
    };
    if by == *player {
        return;
    }
    if !is_component_action(context.content, &alias) {
        return;
    }
    let pile = &mut context.state.discarded_action_cards;
    let Some(at) = pile.iter().position(|held| held == &alias) else {
        return; // a faster reverse engineer took it first
    };
    pile.remove(at);
    if let Some(seat) = context.state.player_mut(player) {
        seat.action_cards.push(alias);
    }
}

/// Rout: "your opponent must announce a retreat, if able."
///
/// The card is played in the window that opens when the retreat announcement step begins, so
/// the marker keys off the same `combat_round_seq` the window compares at announce time. The
/// combat window hands the defender a single forced `retreat` option instead of `stay`/`retreat`
/// when the marker matches; a defender that cannot legally retreat simply keeps fighting, which
/// is what "if able" says.
fn rout(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    let round = context.state.combat_round_seq;
    if let Some(seat) = context.state.player_mut(player) {
        seat.rout_round = Some(round);
    }
}

/// Waylay: "before you roll dice for ANTI-FIGHTER BARRAGE: hits from this roll are produced
/// against all ships (not just fighters)."
///
/// The holder plays it against their own upcoming barrage roll, so the marker keys off the
/// round the barrage is rolled in and the combat window routes that side's barrage hits through
/// the ordinary casualty absorption (owner-chosen, sustain-eligible) instead of auto-destroying
/// fighters.
fn waylay(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    let round = context.state.combat_round_seq;
    if let Some(seat) = context.state.player_mut(player) {
        seat.waylay_barrage_round = Some(round);
    }
}

/// Direct Hit (dh1-dh4): "after another player's ship uses SUSTAIN DAMAGE to cancel a hit
/// produced by your units or abilities: destroy that ship."
///
/// The combat window records the just-sustained hit in [`GameState::last_sustain`] before
/// emitting `SUSTAIN_DAMAGE_USED`, because a reacting card cannot read the payload of the event
/// that summoned it. The victim is the sustained ship: the first ship of that type the owner
/// still has in the system, in unit order (unit order is the stable projection the rules use
/// everywhere else, so no hidden tie-break is introduced).
///
/// The destruction is staged in [`GameState::pending_destructions`] and announced by the card's
/// own resolution step, which does hold the game's resolver: the event goes through the timing
/// machinery like any other, opening its WHEN and AFTER windows. A ship destroyed this way is
/// off the board before the announcement, so `last` is read from the position a reacting card
/// would see.
fn direct_hit(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    let Some((system, victim, unit_type, producer)) = context.state.last_sustain.clone() else {
        return; // no sustain was just used; the guard should have kept this window closed
    };
    if &producer != player {
        return; // defence in depth: the window guard already checks this
    }
    let board = context.state.system_mut(&system);
    let index = board
        .units
        .iter()
        .position(|unit| unit.owner == victim && unit.type_id == unit_type);
    let Some(index) = index else {
        return; // the sustained ship left the system in the meantime; nothing to destroy
    };
    board.units.remove(index);
    context
        .state
        .pending_destructions
        .push((system, victim, unit_type));
}

/// Maneuvering Jets, four physical copies: "before you assign hits produced by another
/// player's SPACE CANNON roll: cancel 1 hit."
///
/// The cancellation is a round-scoped grant on the seat (the same mechanism Shields Holding
/// uses), and the cannon step spends it before any hit is assigned — a cancelled hit is one
/// nobody has to absorb. The window fires right before that gunner's hits are absorbed, so a
/// grant made in it cancels hits of that roll rather than of some other absorption.
fn maneuvering_jets(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    crate::combat::grant_hit_cancellation(context.state, player, 1);
}

/// Reflective Shielding: "when one of your ships uses SUSTAIN DAMAGE during combat: produce 2
/// hits against your opponent's ships in the active system."
///
/// The sustained hit is read from the [`GameState::last_sustain`] handoff the sustain step
/// records before emitting the event that opens this window, and the opponent is the sustained
/// hit's producer: in a fleet fight that is the rival, and in a cannon step the firing player —
/// either way, exactly the ship the card text names. The hits are staged in
/// [`GameState::pending_reflective_hits`], which the sustain step drains the moment the window
/// that plays this card has closed, so the opponent's own sustain answers and loss choices
/// still happen through the ordinary question path.
fn reflective(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    let Some((system, victim, _unit_type, producer)) = context.state.last_sustain.clone() else {
        return; // no sustain was just used; the guard should have kept this window closed
    };
    if &victim != player || &producer == player {
        return; // defence in depth: the window guard already checks it was your ship
    }
    context.state.pending_reflective_hits = Some((system, producer, 2));
}

/// Courageous to the End: "after 1 of your ships is destroyed during a space combat: roll 2
/// dice. For each result equal to or higher than that ship's combat value, your opponent must
/// choose and destroy 1 of their ships."
///
/// The destroyed ship is read from the [`GameState::last_ship_destroyed`] handoff. "Your
/// opponent" is the other ship-bearing combatant of the active system, inferred from the board
/// the way Intercept names the declarant — the holder may have lost the ship that was their
/// last one, so the check runs against the board as it is *now*, and a combat with no ships
/// left for anyone ends with no one to choose a loss. The window's guard cannot see "during a
/// space combat", so the effect's own checks are the binding: a destruction staged outside a
/// fight (a Direct Hit during a tactical action) still acts, against whoever else has ships in
/// the system. Each successful die asks that opponent to choose one of their own ships to
/// destroy (the ordinary casualty question), and each loss is staged in
/// [`GameState::pending_destructions`] so the card's resolution step announces it as a
/// first-class `SHIP_DESTROYED` through the game's resolver. A refused or invalid answer stops
/// the card where it is, like every other window effect that asks.
fn courageous(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    let Some((system, victim, unit_type)) = context.state.last_ship_destroyed.clone() else {
        return;
    };
    if &victim != player {
        return; // defence in depth: the window guard already checks this
    }
    let combatants =
        crate::combat::combatants(context.state, context.content, context.sources, &system);
    let Some(opponent) = combatants.iter().find(|combatant| **combatant != *player) else {
        return; // no opponent in the system, so no one to choose a loss
    };
    let unit = Unit::new(unit_type, victim.clone());
    let Some(combat_value) = crate::combat::hits_on(context.content, context.sources, &unit) else {
        return;
    };
    let roll = context
        .dice
        .roll(context.rng, 2, "courageous to the end", None);
    for face in roll.faces {
        if i64::from(face) < combat_value {
            continue;
        }
        let alive = crate::combat::ships_of(
            context.state,
            context.content,
            context.sources,
            opponent,
            &system,
        );
        if alive.is_empty() {
            break;
        }
        let Ok(casualty) = crate::combat::choose_casualty(
            context.state,
            context.content,
            context.sources,
            context.galaxy,
            context.table,
            opponent,
            &alive,
        ) else {
            break; // the decider refused to choose a loss; the card stops where it is
        };
        let board = context.state.system_mut(&system);
        if let Some(index) = board.units.iter().position(|unit| unit == &casualty) {
            board.units.remove(index);
        }
        context.state.pending_destructions.push((
            system.clone(),
            opponent.clone(),
            casualty.type_id,
        ));
    }
}

/// Crash Landing: "when your last ship in the active system is destroyed: place 1 of your
/// ground forces from the space area of the active system onto a planet in that system
/// (other than Mecatol Rex). If the planet contains other players' units, place your ground
/// forces into coexistence."
///
/// The window fires on the `last` fact the combat window recomputes from the board before it
/// emits `SHIP_DESTROYED`, and the system is read from the `last_ship_destroyed` handoff.
/// "Other than Mecatol Rex" is resolved by name in the planet catalogue: both the base-game
/// planet and its expansion variants carry that name. A question with exactly one possible
/// answer is not asked — the card does the only thing it can do; with several, the holder
/// chooses, without decline, because the landing is mandatory.
fn crashlanding(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    let Some((system, victim, _unit_type)) = context.state.last_ship_destroyed.clone() else {
        return;
    };
    if &victim != player {
        return; // defence in depth: the window guard already checked it was your last ship
    }
    let types = ti4_content::units::catalogue(context.content, context.sources);
    // What can land: the holder's ground forces in the system's space area.
    let kinds: std::collections::BTreeSet<String> = context
        .state
        .system_state(&system)
        .units
        .iter()
        .filter(|unit| {
            unit.owner == *player
                && types
                    .get(unit.type_id.as_str())
                    .is_some_and(ti4_content::units::UnitType::is_ground_force)
        })
        .map(|unit| unit.type_id.to_string())
        .collect();
    if kinds.is_empty() {
        return; // nothing to land
    }
    // Where it can land: the system's planets, other than Mecatol Rex.
    let planets: Vec<ti4_model::id::PlanetId> =
        crate::planets::in_system(context.state, context.content, context.sources, &system)
            .into_iter()
            .filter(|planet| {
                context
                    .content
                    .get(ContentType::Planets, planet.as_str())
                    .is_none_or(|record| record.text("name") != Some("Mecatol Rex"))
            })
            .collect();
    if planets.is_empty() {
        return; // nowhere to land
    }
    let Some(kind) = choose_crashlanding_ground(context, player, &system, &kinds) else {
        return;
    };
    let Some(planet_id) = choose_crashlanding_planet(context, player, &system, &planets) else {
        return;
    };
    // Move one unit of the chosen type from the space area onto the chosen planet.
    let board = context.state.system_mut(&system);
    let index = board
        .units
        .iter()
        .position(|unit| unit.owner == *player && unit.type_id.as_str() == kind)
        .expect("checked above");
    let landed = board.units.remove(index);
    let others_there = board
        .planet_units
        .get(&planet_id)
        .is_some_and(|units| units.iter().any(|unit| unit.owner != *player));
    board
        .planet_units
        .entry(planet_id.clone())
        .or_default()
        .push(Unit::new(landed.type_id, player.clone()));
    if others_there {
        board
            .coexisting
            .entry(planet_id)
            .or_default()
            .insert(player.clone());
    }
}

/// Crash Landing's first decision: which of the holder's ground forces in space lands.
/// A lone kind is not a decision, so it is taken without asking.
fn choose_crashlanding_ground(
    context: &mut crate::timing::TimingContext<'_>,
    player: &PlayerId,
    system: &ti4_model::id::SystemId,
    kinds: &std::collections::BTreeSet<String>,
) -> Option<String> {
    if kinds.len() == 1 {
        return kinds.first().cloned();
    }
    let unit_name = |kind: &str| {
        context
            .content
            .get(ContentType::Units, kind)
            .and_then(|record| record.text("name"))
            .unwrap_or(kind)
            .to_owned()
    };
    let options: Vec<crate::choice::ChoiceOption> = kinds
        .iter()
        .map(|kind| {
            crate::choice::ChoiceOption::labelled(
                format!("ground|{kind}"),
                "crashlanding_ground",
                unit_name(kind),
            )
        })
        .collect();
    let choice = crate::choice::Choice::new(
        player.clone(),
        format!("Crash Landing: choose a ground force in {system}"),
        options,
    );
    let Ok(answer) = context.ask_seeing(&choice) else {
        return None;
    };
    answer.id.strip_prefix("ground|").map(str::to_owned)
}

/// Crash Landing's second decision: which planet the ground force lands on. A lone
/// eligible planet is not a decision, so it is taken without asking.
fn choose_crashlanding_planet(
    context: &mut crate::timing::TimingContext<'_>,
    player: &PlayerId,
    system: &ti4_model::id::SystemId,
    planets: &[ti4_model::id::PlanetId],
) -> Option<ti4_model::id::PlanetId> {
    if planets.len() == 1 {
        return Some(planets[0].clone());
    }
    let options: Vec<crate::choice::ChoiceOption> = planets
        .iter()
        .map(|planet| {
            let name =
                ti4_content::galaxy::planet(context.content, planet.as_str(), context.sources)
                    .and_then(|record| record.name())
                    .unwrap_or(planet.as_str())
                    .to_owned();
            crate::choice::ChoiceOption::labelled(
                format!("planet|{planet}"),
                "crashlanding_planet",
                name,
            )
        })
        .collect();
    let choice = crate::choice::Choice::new(
        player.clone(),
        format!("Crash Landing: choose a planet in {system}"),
        options,
    );
    let Ok(answer) = context.ask_seeing(&choice) else {
        return None;
    };
    let id = answer.id.strip_prefix("planet|")?;
    Some(ti4_model::id::PlanetId::new(id))
}

/// Nav Suite: "during the Movement step of this tactical action, ignore the effect of anomalies."
///
/// All of them, including a gravity rift's +1 and its destruction roll. A rift's bonus is as much
/// an effect of an anomaly as a supernova's bar, so the card gives up the one along with the other.
fn nav_suite(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    let activation = context.state.activation_seq;
    if let Some(seat) = context.state.player_mut(player) {
        seat.anomalies_ignored_activation = Some(activation);
    }
}

/// In The Silence Of Space: "choose 1 system; during this tactical action, your ships in the
/// chosen system can move through systems that contain other players' ships."
///
/// The permission is tied to where the ships *start*, not to what they pass through, so the chosen
/// origin is stored and checked against the origin of each route.
fn in_the_silence_of_space(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    let origins: Vec<ti4_model::id::SystemId> = context
        .state
        .systems_with_units_of(player)
        .into_iter()
        .filter(|system| !context.state.ships_of(player, system).is_empty())
        .cloned()
        .collect();
    let Some(first) = origins.first().cloned() else {
        return; // no ships anywhere, so there is nothing to free
    };
    let chosen = if origins.len() == 1 {
        first
    } else {
        let choice = crate::choice::Choice::new(
            player.clone(),
            "In The Silence Of Space: whose ships ignore blockades",
            origins
                .iter()
                .map(|system| {
                    crate::choice::ChoiceOption::labelled(
                        system.to_string(),
                        "silence",
                        format!("ships in {system} may pass blockades"),
                    )
                })
                .collect(),
        );
        match context.ask_seeing(&choice) {
            Ok(answer) => ti4_model::id::SystemId::new(answer.id),
            Err(_) => return,
        }
    };
    let activation = context.state.activation_seq;
    if let Some(seat) = context.state.player_mut(player) {
        seat.silence_activation = Some(activation);
        seat.silence_system = Some(chosen);
    }
}

/// Skilled Retreat: "move all of your ships from the active system into an adjacent system that
/// does not contain another player's ships. The space combat ends in a draw. Then place a command
/// token in that system."
///
/// The movement, the stranding of cargo beyond capacity and the token are exactly a retreat, so
/// [`crate::combat::retreat_to`] does them. What the card changes is *where* it may go and that
/// the result is a draw rather than a win for whoever stayed.
fn skilled_retreat(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    let Some(system) = context.state.active_system.clone() else {
        return;
    };
    let Some(galaxy) = context.galaxy else {
        return; // without a map there is no "adjacent", so the card cannot resolve
    };
    let destinations = crate::combat::skilled_retreat_destinations(
        context.state,
        context.content,
        context.sources,
        galaxy,
        player,
        &system,
    );
    let Some(first) = destinations.first().cloned() else {
        return;
    };
    let chosen = if destinations.len() == 1 {
        first
    } else {
        let choice = crate::choice::Choice::new(
            player.clone(),
            "Skilled Retreat: withdraw to which system",
            destinations
                .iter()
                .map(|system| {
                    crate::choice::ChoiceOption::labelled(
                        system.to_string(),
                        "skilled_retreat",
                        format!("withdraw to {system}"),
                    )
                })
                .collect(),
        );
        match context.ask_seeing(&choice) {
            Ok(answer) => ti4_model::id::SystemId::new(answer.id),
            Err(_) => return,
        }
    };

    crate::combat::retreat_to(
        context.state,
        context.content,
        context.sources,
        player,
        &system,
        &chosen,
    );
    context.state.combat_draw_round = Some(context.state.combat_round_seq);
}

/// Imperial Rider: "predict aloud an outcome of this agenda. If your prediction is correct, gain
/// 1 victory point." The cost is the vote: a player who predicts cannot vote on that agenda.
///
/// The prediction is read from the game rather than from the event, because the card is played
/// into the `AGENDA_REVEALED` window and the outcomes are what that event carries.
///
/// Imperial Rider stores the bare outcome, which [`resolve_predictions`] reads as its legacy
/// payoff (+1 victory point). The other riders store `"outcome|alias"` so the same vote
/// machinery can pay each card's distinct reward.
fn imperial_rider(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    let Some(predicted) = predicted_outcome(
        context,
        player,
        "Imperial Rider: predict the agenda outcome",
    ) else {
        return; // nothing to predict, so the card cannot resolve (22.3)
    };
    context
        .state
        .agenda_predictions
        .insert(player.clone(), predicted);
}

/// Read the agenda's outcomes and ask this player to predict one, or take the only one on offer.
///
/// One is not a decision and none is not a question: asking either would put a line in the
/// decision log that no player ever chose. `None` means the card fizzles (22.3) or the answer
/// was refused.
fn predicted_outcome(
    context: &mut crate::timing::TimingContext<'_>,
    player: &PlayerId,
    prompt: &str,
) -> Option<String> {
    let choices: Vec<String> = context.state.agenda_choices.clone();
    match choices.as_slice() {
        [] => None,
        [only] => Some(only.clone()),
        many => {
            let choice = crate::choice::Choice::new(
                player.clone(),
                prompt,
                many.iter()
                    .map(|outcome| {
                        crate::choice::ChoiceOption::labelled(
                            outcome.clone(),
                            "prediction",
                            format!("predict {outcome}"),
                        )
                    })
                    .collect(),
            );
            context.ask_seeing(&choice).ok().map(|answer| answer.id)
        }
    }
}

/// Construction Rider: "You cannot vote on this agenda. Predict aloud an outcome of this agenda.
/// If your prediction is correct, place 1 space dock from your reinforcements on a planet you
/// control."
fn construction_rider(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    let Some(predicted) = predicted_outcome(
        context,
        player,
        "Construction Rider: predict the agenda outcome",
    ) else {
        return;
    };
    context
        .state
        .agenda_predictions
        .insert(player.clone(), format!("{predicted}|const_rider"));
}

/// Diplomacy Rider: "You cannot vote on this agenda. Predict aloud an outcome of this agenda.
/// If your prediction is correct, choose 1 system that contains a planet you control. Each other
/// player places a command token from their reinforcements in that system."
fn diplomacy_rider(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    let Some(predicted) = predicted_outcome(
        context,
        player,
        "Diplomacy Rider: predict the agenda outcome",
    ) else {
        return;
    };
    context
        .state
        .agenda_predictions
        .insert(player.clone(), format!("{predicted}|diplo_rider"));
}

/// Leadership Rider: "You cannot vote on this agenda. Predict aloud an outcome of this agenda.
/// If your prediction is correct, gain 3 command tokens."
fn leadership_rider(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    let Some(predicted) = predicted_outcome(
        context,
        player,
        "Leadership Rider: predict the agenda outcome",
    ) else {
        return;
    };
    context
        .state
        .agenda_predictions
        .insert(player.clone(), format!("{predicted}|lead_rider"));
}

/// Politics Rider: "You cannot vote on this agenda. Predict aloud an outcome of this agenda.
/// If your prediction is correct, draw 3 action cards and gain the speaker token."
fn politics_rider(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    let Some(predicted) = predicted_outcome(
        context,
        player,
        "Politics Rider: predict the agenda outcome",
    ) else {
        return;
    };
    context
        .state
        .agenda_predictions
        .insert(player.clone(), format!("{predicted}|politic_rider"));
}

/// Technology Rider: "You cannot vote on this agenda. Predict aloud an outcome of this agenda.
/// If your prediction is correct, research 1 technology."
///
/// The prediction and the vote exclusion are honoured; the research payoff needs a content store
/// and a table to pick and pay for the technology, and the vote-close that pays predictions has
/// neither, so a correct Technology Rider is recorded but its research is not performed.
fn technology_rider(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    let Some(predicted) = predicted_outcome(
        context,
        player,
        "Technology Rider: predict the agenda outcome",
    ) else {
        return;
    };
    context
        .state
        .agenda_predictions
        .insert(player.clone(), format!("{predicted}|tech_rider"));
}

/// Trade Rider: "You cannot vote on this agenda. Predict aloud an outcome of this agenda. If
/// your prediction is correct, gain 5 trade goods."
fn trade_rider(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    let Some(predicted) =
        predicted_outcome(context, player, "Trade Rider: predict the agenda outcome")
    else {
        return;
    };
    context
        .state
        .agenda_predictions
        .insert(player.clone(), format!("{predicted}|trade_rider"));
}

/// Warfare Rider: "You cannot vote on this agenda. Predict aloud an outcome of this agenda. If
/// your prediction is correct, place 1 dreadnought from your reinforcements in a system that
/// contains 1 or more of your ships."
fn warfare_rider(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    let Some(predicted) =
        predicted_outcome(context, player, "Warfare Rider: predict the agenda outcome")
    else {
        return;
    };
    context
        .state
        .agenda_predictions
        .insert(player.clone(), format!("{predicted}|war_rider"));
}

/// Sanction: "You cannot vote on this agenda. Predict aloud an outcome of this agenda. If your
/// prediction is correct, each player that voted for that outcome returns 1 command token from
/// their fleet supply to their reinforcements."
///
/// The prediction and the vote exclusion are honoured; the payoff reads the ballot, and the
/// vote-close that pays predictions carries only the outcome, not who voted which way, so a
/// correct Sanction is recorded but its token returns are not performed.
fn sanction(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    let Some(predicted) =
        predicted_outcome(context, player, "Sanction: predict the agenda outcome")
    else {
        return;
    };
    context
        .state
        .agenda_predictions
        .insert(player.clone(), format!("{predicted}|sanction"));
}

/// Assassinate Representative: "Choose 1 player. That player cannot vote on this agenda."
///
/// The vote order is built from `agenda_predictions`: any player who has an entry there is
/// excluded from voting. The entry's value is a sentinel that matches no outcome, so the victim
/// collects nothing either. A victim who already predicted keeps that prediction, not the
/// sentinel: one prediction per agenda, and a rider in hand is worth more than an assassination.
fn assassinate_representative(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    let seating = context.state.seating_order.clone();
    if seating.is_empty() {
        return;
    }
    let options = seating
        .iter()
        .map(|victim| (victim.to_string(), format!("assassinate {victim}")))
        .collect::<Vec<_>>();
    let Some(victim) = pick(
        context,
        player,
        "Assassinate Representative: choose 1 player",
        "player",
        &options,
    ) else {
        return;
    };
    let victim = ti4_model::id::PlayerId::new(&victim);
    if context.state.agenda_predictions.contains_key(&victim) {
        return; // they already predicted; the assassination finds no new grip on them
    }
    context
        .state
        .agenda_predictions
        .insert(victim, "none|assassin".to_owned());
}

/// Insider Information: "Look at the top 3 cards of the agenda deck."
///
/// A pure peek, and this engine's deciders see the whole state through
/// `Table::ask_seeing`, so there is no hidden top of the deck to reveal: the card changes no
/// state, the deck order is untouched, and the information it would add is already held by any
/// decider answering for the player. It is registered rather than left unimplemented because it
/// is not an unmodelled gap, it is a complete effect with no mechanical half.
fn insider_information(_: &mut crate::timing::TimingContext<'_>, _: &PlayerId) {}

/// Ancient Burial Sites: "Choose 1 player. Exhaust each cultural planet owned by that player."
fn ancient_burial_sites(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    let seating = context.state.seating_order.clone();
    if seating.is_empty() {
        return;
    }
    let options = seating
        .iter()
        .map(|victim| (victim.to_string(), format!("target {victim}")))
        .collect::<Vec<_>>();
    let Some(target) = pick(
        context,
        player,
        "Ancient Burial Sites: choose 1 player",
        "player",
        &options,
    ) else {
        return;
    };
    let target = ti4_model::id::PlayerId::new(&target);
    let planets = ti4_content::galaxy::all_planets(context.content, context.sources);
    let spots: Vec<ti4_model::id::PlanetId> = context
        .state
        .controlled_planets(&target)
        .into_iter()
        .map(|(_, planet)| planet.clone())
        .collect();
    for planet in spots {
        let cultural = planets
            .get(planet.as_str())
            .is_some_and(|planet| planet.planet_type() == Some("CULTURAL"));
        if cultural {
            context.state.exhausted_planets.insert(planet);
        }
    }
}

/// Diplomatic Pressure: "Choose another player. That player must give you 1 promissory note from
/// their hand."
///
/// The note given is the holder's to choose: the card says they *give* it, and a forced handover
/// of a note the holder did not pick would invent a second choice the card does not print. If
/// the chosen player holds no notes, the command has nothing to act on and the card fizzles
/// (22.3); it has already left the hand.
fn diplomatic_pressure(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    let seating = context.state.seating_order.clone();
    let options: Vec<(String, String)> = seating
        .iter()
        .filter(|victim| *victim != player)
        .map(|victim| (victim.to_string(), format!("pressure {victim}")))
        .collect();
    let Some(victim) = pick(
        context,
        player,
        "Diplomatic Pressure: choose another player",
        "player",
        &options,
    ) else {
        return;
    };
    let victim = ti4_model::id::PlayerId::new(&victim);
    let notes = crate::promissory::held_by(context.state, &victim);
    let Some(note) = pick(
        context,
        &victim,
        "Diplomatic Pressure: which note do you give",
        "promissory note",
        &notes
            .iter()
            .map(|note| (note.clone(), note.clone()))
            .collect::<Vec<_>>(),
    ) else {
        return; // the chosen player holds nothing to give (22.3)
    };
    crate::promissory::take(context.state, context.content, player, &note);
}
/// The payoff a correct rider owes, from the vote-close, which carries only the state.
///
/// A reward the card's text forces to a single answer (one controlled planet, one system of
/// space holdings) is performed; a reward that needs a second guess (which of several
/// planets, which of several systems) is skipped, and a reward that needs machinery the
/// vote-close does not have (Technology Rider's research, Sanction's token returns) is
/// recorded in the prediction itself and not performed here.
#[allow(clippy::too_many_lines)] // one arm per rider: a table, not a story
fn rider_payoff(state: &mut GameState, player: &PlayerId, card: Option<&str>) {
    match card {
        Some("lead_rider") => {
            // "gain 3 command tokens" is a supply of reinforcements, not a placement: the
            // tokens land in the fleet pool from which command tokens are spent.
            if let Some(seat) = state.player_mut(player) {
                seat.gain_token(ti4_model::state::TokenPool::Fleet, 3);
            }
        }
        Some("trade_rider") => {
            if let Some(seat) = state.player_mut(player) {
                seat.trade_goods += 5;
            }
        }
        Some("politic_rider") => {
            // Three action cards, the hand limit applied later by whoever owns a table (the
            // same idiom the Unconventional Measures arm uses), and the speaker token.
            for _ in 0..3 {
                if state.action_card_deck.is_empty() {
                    break;
                }
                let top = state.action_card_deck.remove(0);
                if let Some(seat) = state.player_mut(player) {
                    seat.action_cards.push(top);
                }
            }
            state.speaker = player.clone();
        }
        Some("const_rider") => {
            // "on a planet you control": with one controlled planet the card forces it. The
            // 79.2 one-dock-per-planet cap is not checked here because the vote-close has no
            // content store, so a planet that already holds a dock takes the card at face
            // value instead of blocking the placement.
            let spots: Vec<(String, String)> = state
                .controlled_planets(player)
                .into_iter()
                .map(|(system, planet)| (system.to_string(), planet.to_string()))
                .collect();
            if spots.len() == 1 {
                let system = ti4_model::id::SystemId::new(&spots[0].0);
                let planet = ti4_model::id::PlanetId::new(&spots[0].1);
                let already_docked = state
                    .system_state(&system)
                    .planet_units
                    .get(&planet)
                    .is_some_and(|units| {
                        units
                            .iter()
                            .any(|unit| unit.type_id.as_str() == "spacedock")
                    });
                if !already_docked {
                    state
                        .system_mut(&system)
                        .planet_units
                        .entry(planet)
                        .or_default()
                        .push(ti4_model::units::Unit::new(
                            ti4_model::id::UnitTypeId::new("spacedock"),
                            player.clone(),
                        ));
                }
            }
        }
        Some("diplo_rider") => {
            // "choose 1 system that contains a planet you control": with one such system the
            // card forces it, and each other player who still holds a command token places
            // one there; a seat that spent them all simply cannot comply.
            let systems: std::collections::BTreeSet<String> = state
                .controlled_planets(player)
                .iter()
                .map(|(system, _)| system.to_string())
                .collect();
            if systems.len() == 1 {
                let system = ti4_model::id::SystemId::new(systems.into_iter().next().unwrap());
                let mut holders: Vec<ti4_model::id::PlayerId> = Vec::new();
                for other in state.seating_order.iter().filter(|other| *other != player) {
                    if state
                        .player(other)
                        .is_some_and(|seat| seat.tokens(ti4_model::state::TokenPool::Fleet) > 0)
                    {
                        holders.push((*other).clone());
                    }
                }
                for other in holders {
                    if let Some(seat) = state.player_mut(&other) {
                        seat.spend_token(ti4_model::state::TokenPool::Fleet);
                    }
                    state.system_mut(&system).command_tokens.insert(other);
                }
            }
        }
        Some("war_rider") => {
            // "in a system that contains 1 or more of your ships": space holdings are ships
            // (structures sit on planets), so one system holding your space units forces the
            // placement.
            let systems: Vec<ti4_model::id::SystemId> = state
                .board
                .iter()
                .filter(|(_, board)| !board.units_of(player).is_empty())
                .map(|(system, _)| system.clone())
                .collect();
            if systems.len() == 1 {
                state
                    .system_mut(&systems[0])
                    .units
                    .push(ti4_model::units::Unit::new(
                        ti4_model::id::UnitTypeId::new("dreadnought"),
                        player.clone(),
                    ));
            }
        }
        // Recorded, not performed at this call site: the payoff needs a content store and a
        // table (research) or the ballot (token returns), and the vote-close carries only
        // the outcome. See the riders' doc comments.
        Some("tech_rider" | "sanction" | "assassin") => {}
        // The bare imperial encoding, and anything unknown a correct prediction is worth the
        // rider that stores a bare outcome: 1 victory point.
        _ => {
            if let Some(seat) = state.player_mut(player) {
                seat.victory_points =
                    (seat.victory_points + 1).min(crate::objectives::VICTORY_TARGET);
            }
        }
    }
}

/// Pay every correct prediction once the outcome is known, and clear the predictions.
///
/// Called when a vote closes. Clearing matters: a prediction left behind would pay again on the
/// next agenda, for a card that was spent on this one.
///
/// A stored prediction is either a bare outcome (Imperial Rider, worth 1 victory point) or
/// `"outcome|alias"`, where the alias names the rider and the payoff that alias owes. The payoffs
/// here run with only the game state in hand: a reward that needs a choice (which planet, which
/// system) is performed when the card's own text forces a single answer, and skipped when it
/// would be a guess. A reward that needs the content store, a table, or the ballot (Technology
/// Rider's research, Sanction's token returns) is likewise recorded but not performed at this
/// call site, which has none of the three.
pub fn resolve_predictions(state: &mut GameState, outcome: &str) -> Vec<PlayerId> {
    let predictions = std::mem::take(&mut state.agenda_predictions);
    let mut paid = Vec::new();
    for (player, predicted) in predictions {
        let (hit, card) = match predicted.split_once('|') {
            Some((result, card)) => (result == outcome, Some(card)),
            None => (predicted == outcome, None),
        };
        if !hit {
            continue;
        }
        rider_payoff(state, &player, card);

        paid.push(player);
    }
    paid
}

/// Apply every movement modifier this player currently owns to a set of rules.
///
/// One door, called wherever rules are built for a real move. Reading the fields at each
/// construction site instead would mean a card that works in the option list and not in the move
/// that follows, which is worse than one that does not work at all.
pub fn apply_movement_effects(
    rules: &mut crate::movement::MovementRules<'_>,
    state: &GameState,
    player: &PlayerId,
) {
    rules.rifts_ignored = crate::relics::ignores_gravity_rifts(state, player);
    let Some(seat) = state.player(player) else {
        return;
    };
    let this_activation = Some(state.activation_seq);
    if seat.anomalies_ignored_activation == this_activation {
        rules.anomalies_ignored = true;
    }
    // The Dominus Orb, purged into this activation: ships may leave systems holding this player's
    // own command tokens. Scoped by activation, like the card effects around it, so a purge in one
    // tactical action cannot loosen the next.
    if seat.dominus_orb.contains(&state.activation_seq) {
        rules.command_tokens_ignored = true;
    }
    if seat.silence_activation == this_activation {
        rules.ignore_enemy_ships_from = seat.silence_system.as_ref().map(ToString::to_string);
    }
}

// -- tactical-action effects ------------------------------------------------------------------

/// Rally: "After you activate a system that contains another player's ships, place 2 command
/// tokens from your reinforcements in your fleet pool."
///
/// A pure gain from the supply; the card's trigger is the window it is played into.
fn rally(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    if let Some(seat) = context.state.player_mut(player) {
        seat.gain_token(ti4_model::state::TokenPool::Fleet, 2);
    }
}

/// Forward Supply Base: "After another player activates a system that contains your units, gain
/// 3 trade goods. Then, choose another player to gain 1 trade good."
fn forward_supply_base(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    if let Some(seat) = context.state.player_mut(player) {
        seat.trade_goods += 3;
    }
    let seating = context.state.seating_order.clone();
    let options: Vec<(String, String)> = seating
        .iter()
        .filter(|other| *other != player)
        .map(|other| (other.to_string(), format!("pay {other}")))
        .collect();
    if options.is_empty() {
        return; // a solo table owes the card nothing further
    }
    let Some(other) = pick(
        context,
        player,
        "Forward Supply Base: choose another player",
        "player",
        &options,
    ) else {
        return;
    };
    if let Some(seat) = context
        .state
        .player_mut(&ti4_model::id::PlayerId::new(&other))
    {
        seat.trade_goods += 1;
    }
}

/// Counterstroke: "After another player activates a system that contains 1 of your command
/// tokens, return that command token to your tactic pool."
///
/// The activated system is `state.active_system`: the driver sets it before the typed
/// `SYSTEM_ACTIVATED` window fires, and a player holds at most one command token in a system
/// (26.1), so "that" token is the one in the set.
fn counterstroke(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    let Some(system) = context.state.active_system.clone() else {
        return;
    };
    if !context
        .state
        .system_state(&system)
        .command_tokens
        .contains(player)
    {
        return; // the window fired, but no token of the player's is there
    }
    context
        .state
        .system_mut(&system)
        .command_tokens
        .remove(player);
    if let Some(seat) = context.state.player_mut(player) {
        seat.gain_token(ti4_model::state::TokenPool::Tactic, 1);
    }
}

/// Distinguished Councilor: "After you cast votes on an outcome of an agenda: cast 5 additional
/// votes for that outcome."
///
/// The bonus is stored against `agenda_seq` and read in `vote::record`, where it is added to the
/// outcome the seat actually chose, so it cannot be honoured on one voting path and forgotten on
/// another. The window fires after the voter's outcome choice but before their vote is banked,
/// so in the normal case (the voter still owes planet exhaustions) the +5 lands on the ballot.
/// Two degenerate cases leave it unbanked, both inherited from `VOTES_CAST` being emitted after
/// `record` (`game.rs`/`vote.rs`): a voter who exhausts no planet is banked in the same step the
/// window opens, and an abstainer casts nothing to add to.
fn distinguished_councilor(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    crate::vote::add_votes(context.state, player, 5);
}

/// Bribery: "After the speaker votes on an agenda: spend any number of trade goods. For each
/// trade good spent, cast 1 additional vote for the outcome on which you voted."
///
/// The extra votes ride on the holder's own recorded vote exactly as for Distinguished
/// Councilor, so they bank only if the holder's vote is not yet banked when the window fires.
/// The printed trigger is the *speaker's* vote, and the table row in `reactions.rs`
/// (`VOTES_CAST`, unguarded) cannot express "the voter is the speaker" — a guard sees the event
/// and the card holder but not the seating — so the engine also offers the card after any
/// voter's vote; playing it then either banks on the holder's still-pending vote or, once the
/// holder's vote is already banked, counts for nothing.
fn bribery(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    let held = context
        .state
        .player(player)
        .map_or(0, |seat| seat.trade_goods.max(0));
    if held == 0 {
        return; // "any number" includes zero, and zero buys nothing
    }
    let options: Vec<(String, String)> = (0..=held)
        .map(|count| (count.to_string(), format!("spend {count} trade goods")))
        .collect();
    let Some(answer) = pick(
        context,
        player,
        "Bribery: how many trade goods to spend?",
        "count",
        &options,
    ) else {
        return;
    };
    let spent = answer.parse::<i32>().unwrap_or(0).clamp(0, held);
    if spent == 0 {
        return;
    }
    if let Some(seat) = context.state.player_mut(player) {
        seat.trade_goods -= spent;
    }
    crate::vote::add_votes(context.state, player, i64::from(spent));
}

/// Shields Holding, four physical copies: "Before you assign hits to your ships during a space
/// combat: cancel up to 2 hits."
///
/// The hits are granted for the current combat round and spent as the hits would land, before
/// the sustain offer: a cancelled hit is one nobody has to absorb. Each copy cancels two, so
/// holding all four cancels eight, and the pool is consumed across the round rather than
/// refreshed per assignment.
fn shields_holding(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    crate::combat::grant_hit_cancellation(context.state, player, 2);
}

/// Intercept: "After your opponent declares a retreat during a space combat: your opponent
/// cannot retreat during this round of space combat."
///
/// Expressed as having nowhere to go: the retreat step already declines to ask a seat with no
/// legal destination (78.4c), so barring the declarant is enough to keep their ships in the
/// fight. The declarant is not on `GameState` — the combat window is on the driver — so the
/// effect infers it from the board: the declarant is a ship-bearing combatant of the active
/// system who is not the card holder. That inference is exact while the system holds only the
/// two combatants' ships; parked third-party ships widen it to a superset, which is inert on
/// any player who never announces a retreat (a bar is only read for the retreatants). A seat
/// with no ships in the system has no opponent in the combat, and the card fizzles for it.
fn intercept(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    let Some(system) = context.state.active_system.clone() else {
        return;
    };
    let combatants =
        crate::combat::combatants(context.state, context.content, context.sources, &system);
    if !combatants.iter().any(|combatant| combatant == player) {
        return;
    }
    for opponent in &combatants {
        if opponent != player {
            crate::combat::bar_retreat(context.state, opponent);
        }
    }
}

/// Fighter Prototype: "Apply +2 to the result of each of your fighters' combat rolls during
/// this combat round."
///
/// The window (start of the first round of a space combat) fires before any round's rolls, so
/// the marker is in place for every round of the combat; scoping it to `combat_round_seq` is
/// what keeps it from improving the next combat. One entry per copy, so two copies in the same
/// round give +4, and the fleet rolls and the anti-fighter barrage both read the marker.
fn fighter_prototype(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    let round = context.state.combat_round_seq;
    if let Some(seat) = context.state.player_mut(player) {
        seat.fighter_bonus_round.push(round);
    }
}

/// Bunker: "During this invasion, apply -4 to the result of each BOMBARDMENT roll against
/// planets you control."
///
/// An invasion belongs to exactly one tactical action, so `activation_seq` is the invasion's
/// identity and the marker cannot leak into a later activation's invasion. The window opens
/// before the invasion window is constructed, and the invasion window bombards on its opening
/// (49.1), so the marker is in place by the time the rolls are made. One entry per copy: two
/// Bunkers give -8.
fn bunker(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    let activation = context.state.activation_seq;
    if let Some(seat) = context.state.player_mut(player) {
        seat.bunker_invasion.push(activation);
    }
}

/// War Machine (all four copies are identical): "Apply +4 to the total PRODUCTION value of
/// your units and reduce the combined cost of the produced units by 1."
///
/// The window opens when the production step is about to happen, so both halves land on this
/// step's budget: the engine pays produced units out of the same faces the units' PRODUCTION
/// value provides, and +4 of value plus -1 of combined cost is five faces on it. The marker
/// is keyed to the activation that played the card; production happens once per tactical
/// action, so the step that spends the bonus is the one the window described. One entry per
/// copy, so two machines add ten faces.
///
/// The WILD WILD Galaxy printing ("reduce the combined cost by 5") is not modelled: the engine
/// plays the base rules.
fn war_machine(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    let activation = context.state.activation_seq;
    if let Some(seat) = context.state.player_mut(player) {
        seat.war_machine_use.push(activation);
    }
}

/// Blitz: "Each of your non-fighter ships in the active system that do not have BOMBARDMENT
/// gain BOMBARDMENT 6 until the end of the invasion."
///
/// The window opens before the invasion window is built, and that window's bombardment plan
/// (49.1) reads the marker, so a non-fighter ship without BOMBARDMENT rolls one die that hits
/// on 6. The marker is keyed to the activation that owns the invasion, the same way Bunker's
/// is, so it cannot leak into a later activation.
fn blitz(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    let activation = context.state.activation_seq;
    if let Some(seat) = context.state.player_mut(player) {
        seat.blitz_invasion.push(activation);
    }
}

/// Disable: "Your opponents' PDS units lose PLANETARY SHIELD and SPACE CANNON during this
/// invasion."
///
/// The window text promises a system holding 1 or more of your opponents' PDS, but the table
/// maps that wording to the same bare invasion-start row as Blitz — per-card narrowing is not
/// modelled in the table — so the effect re-checks the promise before marking. The marker is
/// read by the bombardment plan (the shield) and the space-cannon guns (the cannon); both are
/// keyed to the activation that owns the invasion.
fn disable(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    let Some(system) = context.state.active_system.clone() else {
        return;
    };
    let types = ti4_content::units::catalogue(context.content, context.sources);
    let board = context.state.system_state(&system);
    let has_opponent_pds = board
        .planet_units
        .values()
        .flatten()
        .chain(&board.units)
        .any(|unit| {
            &unit.owner != player
                && types
                    .get(unit.type_id.as_str())
                    .is_some_and(|kind| kind.base_type() == "pds")
        });
    if !has_opponent_pds {
        return;
    }
    let activation = context.state.activation_seq;
    if let Some(seat) = context.state.player_mut(player) {
        seat.disable_invasion.push(activation);
    }
}

/// Parley: "Return the committed units to the space area."
///
/// The commit step records the unit it just placed in `last_committed_unit` before emitting
/// `UNITS_COMMITTED`, so this reads it back and hands the unit to the space area of the same
/// system. The window fires per landing, so the marker always names the unit the window is
/// about; clearing it on the way out lets a later landing re-arm it.
fn parley(context: &mut crate::timing::TimingContext<'_>, _player: &PlayerId) {
    let Some((owner, system, planet, unit)) = context.state.last_committed_unit.take() else {
        return;
    };
    if unit.owner != owner {
        return;
    }
    let board = context.state.system_mut(&system);
    let standing = board
        .planet_units
        .get(&planet)
        .is_some_and(|units| units.contains(&unit));
    if !standing {
        // An earlier window in the same emission took the unit from the planet, so there is
        // nothing left to return.
        return;
    }
    if let Some(units) = board.planet_units.get_mut(&planet)
        && let Some(index) = units.iter().position(|u| *u == unit)
    {
        units.remove(index);
    }
    board.units.push(unit);
}

/// One Ghost Squad selection moves every unit of one ground-force type from one of the
/// player's planets in the system to another; these are the selectable moves, in
/// deterministic order.
fn ghost_squad_moves(
    board: &ti4_model::state::SystemState,
    player: &PlayerId,
    types: &std::collections::BTreeMap<&str, ti4_content::units::UnitType>,
) -> Vec<crate::choice::ChoiceOption> {
    let own: Vec<ti4_model::id::PlanetId> = board
        .planet_control
        .iter()
        .filter(|(_, controller)| **controller == *player)
        .map(|(planet, _)| planet.clone())
        .collect();
    let mut options: Vec<crate::choice::ChoiceOption> = Vec::new();
    for from in &own {
        let on_from: Vec<&Unit> = board
            .on_planet(from)
            .iter()
            .filter(|unit| {
                unit.owner == *player
                    && types
                        .get(unit.type_id.as_str())
                        .is_some_and(ti4_content::units::UnitType::is_ground_force)
            })
            .collect();
        if on_from.is_empty() {
            continue;
        }
        for to in &own {
            if to == from {
                continue;
            }
            for kind in on_from
                .iter()
                .map(|unit| unit.type_id.as_str())
                .collect::<std::collections::BTreeSet<_>>()
            {
                let count = on_from
                    .iter()
                    .filter(|u| u.type_id.as_str() == kind)
                    .count();
                options.push(crate::choice::ChoiceOption::labelled(
                    format!("move|{from}|{to}|{kind}"),
                    "move",
                    format!("move {count} {kind} from {from} to {to}"),
                ));
            }
        }
    }
    options
}

/// Ghost Squad: "Move any number of your ground forces from any planet you control in the
/// active system to any other planet you control in the active system."
///
/// One selection moves every unit of one type from one of your planets to another, and the
/// decider is asked again after each move — "any number" is the sum of the selections, and
/// declining ends the effect. A selection is written `move|<from>|<to>|<type>`; the offer
/// always carries a decline option so a single remaining move cannot move on its own.
fn ghost_squad(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    let Some(system) = context.state.active_system.clone() else {
        return;
    };
    let types = ti4_content::units::catalogue(context.content, context.sources);
    loop {
        let options = ghost_squad_moves(&context.state.system_state(&system), player, &types);
        if options.is_empty() {
            return;
        }
        let mut offered = options.clone();
        offered.push(crate::choice::ChoiceOption::decline());
        let choice = crate::choice::Choice::new(
            player.clone(),
            format!("Ghost Squad: move ground forces in {system}"),
            offered,
        );
        let Ok(answer) = context.ask_seeing(&choice) else {
            return;
        };
        if answer.is_decline() {
            return;
        }
        let Some((from, to, kind)) = answer.id.strip_prefix("move|").and_then(|rest| {
            let mut parts = rest.split('|');
            let (from, to, kind) = (parts.next()?, parts.next()?, parts.next()?);
            (parts.next().is_none()).then_some((from, to, kind))
        }) else {
            return;
        };
        let from = ti4_model::id::PlanetId::new(from);
        let to = ti4_model::id::PlanetId::new(to);
        let moved: Vec<Unit> = context
            .state
            .system_state(&system)
            .on_planet(&from)
            .iter()
            .filter(|unit| {
                unit.owner == *player
                    && unit.type_id.as_str() == kind
                    && types
                        .get(unit.type_id.as_str())
                        .is_some_and(ti4_content::units::UnitType::is_ground_force)
            })
            .cloned()
            .collect();
        if moved.is_empty() {
            return;
        }
        let board = context.state.system_mut(&system);
        if let Some(units) = board.planet_units.get_mut(&from) {
            units.retain(|unit| !moved.iter().any(|m| m == unit));
        }
        board
            .planet_units
            .entry(to.clone())
            .or_default()
            .extend(moved.iter().cloned());
    }
}

/// The player's ground forces on the board, wherever they stand, as `system|planet|index`
/// options. A structure is not a ground force, and a unit whose type the catalogue does not
/// recognise is not one either.
fn collect_ground_units(
    state: &GameState,
    types: &BTreeMap<&str, ti4_content::units::UnitType>,
    player: &PlayerId,
) -> Vec<(String, String, ti4_model::units::Unit)> {
    // The index is into that planet's unit list, counted the way the effect will remove it.
    let mut found: Vec<(String, String, ti4_model::units::Unit)> = Vec::new();
    for (system_id, board) in &state.board {
        for (planet, units) in &board.planet_units {
            for (index, unit) in units.iter().enumerate() {
                if &unit.owner != player
                    || !types
                        .get(unit.type_id.as_str())
                        .is_some_and(ti4_content::units::UnitType::is_ground_force)
                {
                    continue;
                }
                found.push((
                    format!("{system_id}|{planet}|{index}"),
                    format!("{} in {system_id}", unit.type_id),
                    unit.clone(),
                ));
            }
        }
    }
    found
}

/// Decoy Operation: "After another player activates a system that contains 1 or more of your
/// structures, remove up to 2 of your ground forces from the game board and place them on a
/// planet you control in the active system."
///
/// The units the player can pull, as [`collect_ground_units`] found them.
fn pull_and_land(
    context: &mut crate::timing::TimingContext<'_>,
    found: &[(String, String, ti4_model::units::Unit)],
    taken: &[usize],
    system: &ti4_model::id::SystemId,
    planet: &ti4_model::id::PlanetId,
) {
    // Remove back-to-front within each planet so an earlier removal cannot shift a later
    // index, then land what was pulled.
    let mut by_source: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for position in taken {
        let system_part = found[*position].0.split('|').next().expect("a system part");
        by_source
            .entry(system_part.to_owned())
            .or_default()
            .push(*position);
    }
    for (source_system, mut positions) in by_source {
        positions.sort_by(|a, b| {
            let index_of = |position: &usize| -> usize {
                found[*position]
                    .0
                    .rsplit_once('|')
                    .expect("an index")
                    .1
                    .parse()
                    .expect("numeric")
            };
            index_of(b).cmp(&index_of(a))
        });
        for position in positions {
            let planet_part = found[position]
                .0
                .split('|')
                .nth(1)
                .and_then(|rest| rest.split('|').next())
                .expect("system|planet|index");
            let index = found[position]
                .0
                .rsplit_once('|')
                .expect("an index")
                .1
                .parse::<usize>()
                .expect("numeric");
            context
                .state
                .system_mut(&ti4_model::id::SystemId::new(&source_system))
                .planet_units
                .get_mut(&ti4_model::id::PlanetId::new(planet_part))
                .expect("the unit was there")
                .swap_remove(index);
        }
    }
    let moved: Vec<ti4_model::units::Unit> = taken
        .iter()
        .map(|position| found[*position].2.clone())
        .collect();
    context
        .state
        .system_mut(system)
        .planet_units
        .entry(planet.clone())
        .or_default()
        .extend(moved);
}

/// Decoy Operation: "After another player activates a system that contains 1 or more of your
/// structures, remove up to 2 of your ground forces from the game board and place them on a
/// planet you control in the active system."
///
/// The units come from anywhere on the board and return to the box, so removal is not capped
/// by what the box still holds. "Up to 2" is a real choice when more than two units are
/// available: the player may stop after one, and the stop is offered as an option rather than
/// invented as a rule. The destination is fixed by the card (a planet of the player's in the
/// activated system), so if the player controls none there the card fizzles before anything is
/// removed: a failed transition is atomic.
fn decoy_operation(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    let Some(system) = context.state.active_system.clone() else {
        return;
    };
    let types = ti4_content::units::catalogue(context.content, context.sources);
    // Destination first, so a card with nowhere to land removes nothing.
    let destinations: Vec<(String, String)> = context
        .state
        .controlled_planets(player)
        .iter()
        .filter(|(held, _)| held.as_str() == system.as_str())
        .map(|(_, planet)| (planet.to_string(), planet.to_string()))
        .collect();
    if destinations.is_empty() {
        return;
    }
    let found = collect_ground_units(context.state, &types, player);
    if found.is_empty() {
        return;
    }
    let mut taken: Vec<usize> = Vec::new();
    if found.len() <= 2 {
        taken = (0..found.len()).collect();
    } else {
        let options = found
            .iter()
            .map(|(id, label, _)| (id.clone(), label.clone()))
            .collect::<Vec<_>>();
        let Some(first) = pick(
            context,
            player,
            "Decoy Operation: which unit to pull",
            "unit",
            &options,
        ) else {
            return;
        };
        let Some(first_index) = found.iter().position(|(id, _, _)| id == &first) else {
            return; // the answer named nothing on offer
        };
        taken.push(first_index);
        let rest: Vec<(String, String)> = found
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != first_index)
            .map(|(_, (id, label, _))| (id.clone(), label.clone()))
            .chain(std::iter::once((
                "stop".to_owned(),
                "stop after one".to_owned(),
            )))
            .collect();
        let Some(second) = pick(
            context,
            player,
            "Decoy Operation: another unit or stop",
            "unit",
            &rest,
        ) else {
            return;
        };
        if second != "stop" {
            let Some(second_index) = found
                .iter()
                .enumerate()
                .filter(|(i, _)| *i != first_index)
                .find(|(_, (id, _, _))| id == &second)
                .map(|(i, _)| i)
            else {
                return;
            };
            taken.push(second_index);
        }
    }
    // The planet they land on, after the units: one planet is not a decision.
    let Some(planet_id) = pick(
        context,
        player,
        "Decoy Operation: which planet they land on",
        "planet",
        &destinations,
    ) else {
        return;
    };
    let planet = ti4_model::id::PlanetId::new(&planet_id);
    pull_and_land(context, &found, &taken, &system, &planet);
}

/// Emergency Repairs: "at the start or end of a combat round, repair all of your units that
/// have SUSTAIN DAMAGE in the active system."
///
/// A unit's sustained damage is a flag on the unit, and the active system of the combat round
/// is `state.active_system`, so the repair is a scan of both its space and its planets.
fn emergency_repairs(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    let Some(system) = context.state.active_system.clone() else {
        return;
    };
    let board = context.state.system_mut(&system);
    for unit in &mut board.units {
        if &unit.owner == player {
            unit.sustained_damage = false;
        }
    }
    for units in board.planet_units.values_mut() {
        for unit in units.iter_mut() {
            if &unit.owner == player {
                unit.sustained_damage = false;
            }
        }
    }
}

/// Upgrade: "after you activate a system that contains 1 or more of your ships, replace 1 of
/// your cruisers in that system with 1 dreadnought from your reinforcements."
///
/// The cruiser is removed to the box and the dreadnought is a fresh unit from reinforcements,
/// so the 31.4 box limit applies to the dreadnought alone; a seat whose box is empty of them
/// cannot play the card into a state it cannot pay.
fn upgrade_ship(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    let Some(system) = context.state.active_system.clone() else {
        return;
    };
    let types = ti4_content::units::catalogue(context.content, context.sources);
    let dreadnought = ti4_model::id::UnitTypeId::new("dreadnought");
    if crate::supply::allowed(
        context.state,
        context.content,
        context.sources,
        player,
        &dreadnought,
        1,
    ) == 0
    {
        return; // the box holds no more dreadnoughts
    }
    let units = context.state.system_state(&system).units.clone();
    let options: Vec<(String, String)> = units
        .iter()
        .enumerate()
        .filter(|(_, unit)| {
            &unit.owner == player
                && types
                    .get(unit.type_id.as_str())
                    .is_some_and(|kind| kind.base_type() == "cruiser")
        })
        .map(|(index, unit)| (index.to_string(), format!("{}", unit.type_id)))
        .collect();
    if options.is_empty() {
        return; // no cruiser of the player's in the system (22.3)
    }
    let Some(chosen) = pick(
        context,
        player,
        "Upgrade: which cruiser becomes a dreadnought",
        "cruiser",
        &options,
    ) else {
        return;
    };
    let index: usize = chosen.parse().expect("the option was an index");
    let board = context.state.system_mut(&system);
    board.units.remove(index);
    board
        .units
        .push(ti4_model::units::Unit::new(dreadnought, player.clone()));
}

/// Experimental Battlestation: "after the active player moves ships into the active system
/// during a tactical action, choose 1 of your space docks that is either in or adjacent to that
/// system. That space dock uses SPACE CANNON 5(x3) against the active player's ships in the
/// active system."
///
/// The roll is three faces on the ten-sided die, hits on 5 or higher, and the hits go through
/// the combat engine's own absorption, so sustain damage and the owner's casualty choices are
/// honoured exactly as in a real space-cannon exchange.
fn experimental_battlestation(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    let Some(system) = context.state.active_system.clone() else {
        return;
    };
    let types = ti4_content::units::catalogue(context.content, context.sources);
    // A dock is any structure whose type is a space dock, on any planet of any system.
    let mut dock_systems: BTreeSet<String> = BTreeSet::new();
    for (system_id, board) in &context.state.board {
        for units in board.planet_units.values() {
            if units.iter().any(|unit| {
                &unit.owner == player
                    && types
                        .get(unit.type_id.as_str())
                        .is_some_and(|kind| kind.base_type() == "spacedock")
            }) {
                dock_systems.insert(system_id.to_string());
            }
        }
    }
    let eligible: Vec<(String, String)> = dock_systems
        .iter()
        .filter(|here| {
            if **here == system.to_string() {
                return true;
            }
            context
                .galaxy
                .is_some_and(|galaxy| galaxy.adjacent(system.as_str()).contains(here.as_str()))
        })
        .map(|here| (here.clone(), format!("dock in {here}")))
        .collect();
    if eligible.is_empty() {
        return; // no dock in or adjacent to the active system
    }
    let Some(_) = pick(
        context,
        player,
        "Experimental Battlestation: which dock fires",
        "space dock",
        &eligible,
    ) else {
        return;
    };
    let roll = context.dice.roll(
        context.rng,
        3,
        "experimental_battlestation_space_cannon",
        Some(5),
    );
    let hits = roll.hits();
    if hits == 0 {
        return;
    }
    // The active player is the one whose tactical action this is, not the card player.
    let Some(target) = context.state.active.clone() else {
        return;
    };
    // A card effect has no resolver of its own: the hits apply on a stub resolver, so a
    // sustained hit here announces nothing and opens no window.
    let crate::timing::TimingContext {
        state,
        content,
        sources,
        table,
        dice,
        rng,
        galaxy,
        ..
    } = context;
    let mut ctx = crate::choice::Resolving {
        content,
        sources: *sources,
        dice,
        rng,
        table,
        timing: None,
    };
    let _ = crate::combat::absorb_hits_seeing(
        state, content, *sources, *galaxy, &mut ctx, &target, &system, player, hits,
    );
}

/// Reveal Prototype: "at the start of a combat, spend 4 resources to research a unit upgrade
/// technology of the same type as 1 of your units that is participating in this combat."
///
/// The participants of the space combat are the player's space units in the active system at
/// its start. A unit upgrade of the "same type" is one whose line is that of a participating
/// unit: a technology that names its subject in `baseUpgrade` must match that subject's line
/// (or the unit itself), and the base game's unnamed second-generation line (Carrier II and
/// kin) is matched by the technology's own name, so "Carrier II" is of the carrier line.
/// Prerequisites still apply: only a technology the player can research now is offered, and
/// the 4 resources are paid before the research lands, so a seat that cannot pay does not
/// research.
fn reveal_prototype(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    let Some(system) = context.state.active_system.clone() else {
        return;
    };
    let types = ti4_content::units::catalogue(context.content, context.sources);
    let lines: Vec<&str> = context
        .state
        .system_state(&system)
        .units
        .iter()
        .filter(|unit| &unit.owner == player)
        .filter_map(|unit| {
            types
                .get(unit.type_id.as_str())
                .map(ti4_content::units::UnitType::base_type)
        })
        .collect();
    if lines.is_empty() {
        return; // no unit of the player's is in the combat
    }
    let open =
        crate::technology::researchable(context.state, context.content, context.sources, player);
    let options: Vec<(String, String)> = open
        .iter()
        .filter(|alias| crate::technology::is_unit_upgrade(context.content, alias))
        .filter_map(|alias| {
            let record = context
                .content
                .get(
                    ti4_model::content_types::ContentType::Technologies,
                    alias.as_str(),
                )?
                .clone();
            let base = record.text("baseUpgrade").unwrap_or_default().to_owned();
            if base.is_empty() {
                // The unnamed second-generation line: the name names the line.
                let name = record
                    .text("name")
                    .unwrap_or_default()
                    .to_lowercase()
                    .replace(" ii", "")
                    .replace(' ', "_");
                let matches = lines
                    .iter()
                    .any(|line| name == *line || name.ends_with(&format!("_{line}")));
                matches.then(|| {
                    (
                        alias.as_str().to_owned(),
                        record.text("name").unwrap_or(alias.as_str()).to_owned(),
                    )
                })
            } else {
                // A named subject: it must be the unit's own line, or the unit itself.
                let subject_line = types
                    .get(base.as_str())
                    .map(ti4_content::units::UnitType::base_type);
                let matches = lines.iter().any(|line| Some(*line) == subject_line)
                    || subject_line.is_none() && lines.iter().any(|line| **line == base);
                matches.then(|| {
                    (
                        alias.as_str().to_owned(),
                        record.text("name").unwrap_or(alias.as_str()).to_owned(),
                    )
                })
            }
        })
        .collect();
    if options.is_empty() {
        return; // no offered technology is of a line in the combat
    }
    let Some(alias) = pick(
        context,
        player,
        "Reveal Prototype: which prototype to reveal",
        "technology",
        &options,
    ) else {
        return;
    };
    let paid = crate::production::pay(
        context.state,
        context.content,
        context.sources,
        context.table,
        player,
        4,
        crate::production::Spend::Resources,
    );
    let Ok(paid) = paid else {
        return;
    };
    if !paid {
        return; // the four resources could not be found (22.3)
    }
    crate::technology::research(
        context.state,
        context.content,
        context.sources,
        player,
        &ti4_model::TechnologyId::new(&alias),
    );
}

// -- component-action effects ------------------------------------------------------------------
//
// The cards whose printed window is "Action" are played on their owner's own turn (22.1):
// `perform` discards one and announces it, and `announce` runs the effect below. There is no
// window to wait for and no one else to react, so each effect runs straight out of the timing
// context and asks the game's own table when a decision belongs to a player.

/// The player's neighbours in seating order. A two-player table has one.
fn neighbors(state: &GameState, player: &PlayerId) -> Vec<PlayerId> {
    let order = &state.seating_order;
    let Some(index) = order.iter().position(|seat| seat == player) else {
        return Vec::new();
    };
    let before = order[(index + order.len() - 1) % order.len()].clone();
    let after = order[(index + 1) % order.len()].clone();
    let same = before == after;
    let mut out = vec![before];
    if !same {
        out.push(after);
    }
    out
}

/// A system a pirate card may build in: not a homeworld, and no ship of any player's. Neutral
/// ships do not block a pirate fleet (the cards forbid *non-neutral* ships), which is exactly
/// what this tests: a unit owned by someone in `state.players`.
fn pirate_systems(state: &GameState) -> Vec<ti4_model::id::SystemId> {
    state
        .board
        .keys()
        .filter(|system| {
            !state
                .system_state(system)
                .units
                .iter()
                .any(|unit| state.players.iter().any(|seat| seat.id == unit.owner))
        })
        .cloned()
        .collect()
}

/// A system a pirate card may build in that is also off the homeworlds, which the board may
/// hold only as ids.
fn pirate_systems_off_homes(
    state: &GameState,
    content: &ContentStore,
    sources: ti4_model::content_types::SourceSet,
) -> Vec<ti4_model::id::SystemId> {
    let homes = ti4_content::galaxy::home_systems(content, sources);
    pirate_systems(state)
        .into_iter()
        .filter(|system| !homes.contains(system.as_str()))
        .collect()
}

/// Harness Energy: "After you activate an anomaly, replenish your commodities."
///
/// Replenishing is the same computation the strategy card uses: commodities fill back up to
/// the faction's limit.
fn harness_energy(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    let limit = crate::strategy_cards::commodity_limit(context.state, context.content, player);
    if let Some(seat) = context.state.player_mut(player) {
        seat.commodities = limit;
    }
}

/// Economic Initiative: "Ready each cultural planet you control."
fn economic_initiative(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    let owned: Vec<ti4_model::id::PlanetId> = context
        .state
        .controlled_planets(player)
        .into_iter()
        .map(|(_, planet)| planet.clone())
        .collect();
    for planet in owned {
        let cultural =
            ti4_content::galaxy::planet(context.content, planet.as_str(), context.sources)
                .is_some_and(|planet| {
                    planet
                        .planet_type()
                        .is_some_and(|kind| kind.eq_ignore_ascii_case("cultural"))
                });
        if cultural {
            context.state.exhausted_planets.remove(&planet);
        }
    }
}

/// Industrial Initiative: "Gain 1 trade good for each industrial planet you control."
fn industrial_initiative(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    let owned: Vec<ti4_model::id::PlanetId> = context
        .state
        .controlled_planets(player)
        .into_iter()
        .map(|(_, planet)| planet.clone())
        .collect();
    let count = owned
        .iter()
        .filter(|planet| {
            ti4_content::galaxy::planet(context.content, planet.as_str(), context.sources)
                .is_some_and(|planet| {
                    planet
                        .planet_type()
                        .is_some_and(|kind| kind.eq_ignore_ascii_case("industrial"))
                })
        })
        .count();
    if let Some(seat) = context.state.player_mut(player) {
        seat.trade_goods += i32::try_from(count).unwrap_or(i32::MAX);
    }
}

/// Fighter Conscription: "Place 1 fighter from your reinforcements in each system that
/// contains 1 or more of your space docks or units that have capacity. They cannot be placed
/// in systems that contain other players' ships."
///
/// "Units that have capacity" are the cargo rules as printed: any of the player's space units
/// whose type carries cargo space. A space dock is a structure, so it is looked for on the
/// planet's unit list. The card forbids other *players'* ships; neutral units are not players',
/// so they do not block the placement.
fn fighter_conscription(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    let types = ti4_content::units::catalogue(context.content, context.sources);
    let seated: std::collections::BTreeSet<String> = context
        .state
        .players
        .iter()
        .map(|seat| seat.id.to_string())
        .collect();
    let fighter = ti4_model::id::UnitTypeId::new("fighter");
    let mut eligible: Vec<ti4_model::id::SystemId> = Vec::new();
    for (system, board) in &context.state.board {
        let docked = board.planet_units.values().any(|units| {
            units.iter().any(|unit| {
                &unit.owner == player
                    && types
                        .get(unit.type_id.as_str())
                        .is_some_and(|kind| kind.base_type() == "spacedock")
            })
        });
        let has_capacity = board.units.iter().any(|unit| {
            &unit.owner == player
                && types
                    .get(unit.type_id.as_str())
                    .is_some_and(|kind| kind.capacity() > 0)
        });
        if !docked && !has_capacity {
            continue;
        }
        if board
            .units
            .iter()
            .any(|unit| &unit.owner != player && seated.contains(&unit.owner.to_string()))
        {
            continue; // someone else's ship is in the system
        }
        eligible.push(system.clone());
    }
    for system in eligible {
        if crate::supply::allowed(
            context.state,
            context.content,
            context.sources,
            player,
            &fighter,
            1,
        ) == 0
        {
            break; // the box holds no more fighters
        }
        context
            .state
            .system_mut(&system)
            .units
            .push(ti4_model::units::Unit::new(fighter.clone(), player.clone()));
    }
}

fn impersonation(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    let paid = crate::production::pay(
        context.state,
        context.content,
        context.sources,
        context.table,
        player,
        3,
        crate::production::Spend::Influence,
    );
    let Ok(paid) = paid else {
        return;
    };
    if !paid {
        return;
    }
    // A hand-limit answer the decider gives illegally leaves the objective drawn: the draw
    // happened, and the limit question is the game's to refuse, not the card's to undo.
    let _ = crate::secrets::draw(context.state, context.content, context.table, player);
}

/// Plagiarize: "Spend 5 influence and choose a non-faction technology owned by 1 of your
/// neighbors. Gain that technology."
///
/// The five influence go first, for the same reason as Impersonation's. A faction technology
/// is one the corpus tags with a `faction`; everything else a neighbour owns is fair prey, and
/// gaining it leaves it no longer in the neighbour's play area.
fn plagiarize(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    let paid = crate::production::pay(
        context.state,
        context.content,
        context.sources,
        context.table,
        player,
        5,
        crate::production::Spend::Influence,
    );
    let Ok(paid) = paid else {
        return;
    };
    if !paid {
        return;
    }
    let mut options: Vec<(
        String,
        String,
        ti4_model::id::PlayerId,
        ti4_model::TechnologyId,
    )> = Vec::new();
    for neighbour in neighbors(context.state, player) {
        let Some(seat) = context.state.player(&neighbour) else {
            continue;
        };
        for tech in &seat.technologies {
            let non_faction = context
                .content
                .get(
                    ti4_model::content_types::ContentType::Technologies,
                    tech.as_str(),
                )
                .is_some_and(|record| record.text("faction").is_none());
            if !non_faction {
                continue;
            }
            let name = context
                .content
                .get(
                    ti4_model::content_types::ContentType::Technologies,
                    tech.as_str(),
                )
                .and_then(|record| record.text("name"))
                .unwrap_or(tech.as_str());
            options.push((
                format!("{neighbour}|{tech}"),
                format!("{neighbour}'s {name}"),
                neighbour.clone(),
                tech.clone(),
            ));
        }
    }
    if options.is_empty() {
        return;
    }
    let options_only = options
        .iter()
        .map(|(id, label, _, _)| (id.clone(), label.clone()))
        .collect::<Vec<_>>();
    let Some(chosen) = pick(
        context,
        player,
        "Plagiarize: which technology to steal",
        "technology",
        &options_only,
    ) else {
        return;
    };
    let Some((_, _, owner, tech)) = options.iter().find(|(id, _, _, _)| *id == chosen).cloned()
    else {
        return; // the answer named nothing on offer
    };
    crate::technology::grant(context.state, player, &tech);
    if let Some(seat) = context.state.player_mut(&owner) {
        seat.technologies.remove(&tech);
    }
}

/// Archaeological Expedition: "Reveal the top 3 cards of an exploration deck that matches a
/// planet you control; gain any relic fragments that you reveal and discard the rest."
///
/// The card reveals, it does not explore: attachments and instants go straight to the discard
/// with nowhere to attach or fire at, and only fragments are kept. A planet whose trait names
/// no deck in the corpus offers nothing and is not offered.
fn archaeological_expedition(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    let mut decks: Vec<(ti4_model::id::PlanetId, String)> = Vec::new();
    for (_, planet) in context.state.controlled_planets(player) {
        if let Some(deck) = crate::exploration::trait_of(context.content, context.sources, planet) {
            decks.push((planet.clone(), deck));
        }
    }
    if decks.is_empty() {
        return;
    }
    let options = decks
        .iter()
        .map(|(planet, _)| (planet.to_string(), planet.to_string()))
        .collect::<Vec<_>>();
    let Some(planet_id) = pick(
        context,
        player,
        "Archaeological Expedition: which planet's deck",
        "planet",
        &options,
    ) else {
        return;
    };
    let deck = decks
        .iter()
        .find(|(planet, _)| planet.as_str() == planet_id)
        .expect("offered")
        .1
        .clone();
    for _ in 0..3 {
        let Some(card) = crate::exploration::draw(context.state, &deck) else {
            break; // the deck ran out before the third card
        };
        if crate::exploration::resolution(context.content, &card)
            .is_some_and(|kind| kind == "Fragment")
        {
            let trait_name = context
                .content
                .get(ti4_model::content_types::ContentType::Explores, &card)
                .and_then(|record| record.text("type"))
                .unwrap_or(&deck)
                .to_ascii_uppercase();
            crate::exploration::gain_fragment(context.state, player, &trait_name);
        }
    }
}

/// Divert Funding: "Return a non-unit upgrade, non-faction technology that you own to your
/// technology deck. Then, research another technology."
///
/// The engine does not track a technology deck, so the returned card leaves the player's play
/// area and is not put back on any table: the research half still runs, and the deck half is
/// the documented gap. The research itself is the engine's own: prerequisites still apply.
fn divert_funding(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    let mine = context
        .state
        .player(player)
        .map(|seat| seat.technologies.clone())
        .unwrap_or_default();
    let options: Vec<(String, String)> = mine
        .iter()
        .filter(|alias| !crate::technology::is_unit_upgrade(context.content, alias))
        .filter(|alias| {
            context
                .content
                .get(
                    ti4_model::content_types::ContentType::Technologies,
                    alias.as_str(),
                )
                .is_some_and(|record| record.text("faction").is_none())
        })
        .map(|alias| {
            (
                alias.as_str().to_owned(),
                context
                    .content
                    .get(
                        ti4_model::content_types::ContentType::Technologies,
                        alias.as_str(),
                    )
                    .and_then(|record| record.text("name"))
                    .unwrap_or(alias.as_str())
                    .to_owned(),
            )
        })
        .collect();
    if options.is_empty() {
        return;
    }
    let Some(alias) = pick(
        context,
        player,
        "Divert Funding: which technology to return",
        "technology",
        &options,
    ) else {
        return;
    };
    if let Some(seat) = context.state.player_mut(player) {
        seat.technologies
            .remove(&ti4_model::TechnologyId::new(&alias));
    }
    let open =
        crate::technology::researchable(context.state, context.content, context.sources, player);
    if open.is_empty() {
        return;
    }
    let research_options = open
        .iter()
        .map(|tech| {
            (
                tech.as_str().to_owned(),
                context
                    .content
                    .get(
                        ti4_model::content_types::ContentType::Technologies,
                        tech.as_str(),
                    )
                    .and_then(|record| record.text("name"))
                    .unwrap_or(tech.as_str())
                    .to_owned(),
            )
        })
        .collect::<Vec<_>>();
    let Some(alias) = pick(
        context,
        player,
        "Divert Funding: what to research with the funding",
        "technology",
        &research_options,
    ) else {
        return;
    };
    crate::technology::research(
        context.state,
        context.content,
        context.sources,
        player,
        &ti4_model::TechnologyId::new(&alias),
    );
}

/// Exploration Probe: "Explore a frontier token that is in or adjacent to a system that
/// contains 1 or more of your ships."
///
/// The frontier token is removed when its card is drawn; a deck that runs out before the draw
/// keeps its token, because nothing was explored. Adjacency needs a map, so a game without one
/// can only probe tokens in systems its ships already sit in.
fn exploration_probe(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    let mine = systems_with_my_ships(context.state, context.content, context.sources, player);
    let mut eligible: BTreeSet<String> = mine.into_iter().collect();
    if let Some(galaxy) = context.galaxy {
        let mut adjacent = BTreeSet::new();
        for system in &eligible {
            adjacent.extend(galaxy.adjacent(system).into_iter().map(str::to_owned));
        }
        eligible.extend(adjacent);
    }
    let held: BTreeSet<String> = context
        .state
        .frontier_tokens
        .iter()
        .map(ToString::to_string)
        .collect();
    let options: Vec<(String, String)> = eligible
        .intersection(&held)
        .map(|system| (system.clone(), system.clone()))
        .collect();
    if options.is_empty() {
        return;
    }
    let Some(system) = pick(
        context,
        player,
        "Exploration Probe: which frontier token to explore",
        "system",
        &options,
    ) else {
        return;
    };
    let system = ti4_model::id::SystemId::new(&system);
    if !context.state.frontier_tokens.remove(&system) {
        return;
    }
    let mut ctx = crate::choice::Resolving {
        content: context.content,
        sources: context.sources,
        dice: context.dice,
        rng: context.rng,
        table: context.table,
        timing: None,
    };
    let _ = crate::exploration::explore_with(
        context.state,
        &mut ctx,
        player,
        crate::exploration::FRONTIER,
        None,
    );
}

/// Refit Troops: "Choose 1 or 2 of your infantry on the game board. Replace each of those
/// infantry with mechs."
///
/// Infantry only stand on planets, so the search is the planet unit lists. "1 or 2" is a real
/// choice when there is more than one infantry: the second is offered with a stop, the same
/// way Decoy Operation offers its second pull. The mech is a fresh unit from the box, so the
/// box must still hold it.
#[allow(clippy::too_many_lines)]
fn refit_troops(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    let types = ti4_content::units::catalogue(context.content, context.sources);
    let mech = ti4_model::id::UnitTypeId::new("mech");
    if crate::supply::allowed(
        context.state,
        context.content,
        context.sources,
        player,
        &mech,
        1,
    ) == 0
    {
        return; // the box holds no more mechs
    }
    // `system|planet|index`, the index into that planet's unit list.
    let mut found: Vec<(String, String, ti4_model::units::Unit)> = Vec::new();
    for (system, board) in &context.state.board {
        for (planet, units) in &board.planet_units {
            for (index, unit) in units.iter().enumerate() {
                if &unit.owner != player
                    || types
                        .get(unit.type_id.as_str())
                        .is_some_and(|kind| kind.base_type() != "infantry")
                {
                    continue;
                }
                found.push((
                    format!("{system}|{planet}|{index}"),
                    format!("infantry in {system}"),
                    unit.clone(),
                ));
            }
        }
    }
    if found.is_empty() {
        return;
    }
    let mut taken: Vec<usize> = Vec::new();
    if found.len() == 1 {
        taken.push(0);
    } else {
        let options = found
            .iter()
            .map(|(id, label, _)| (id.clone(), label.clone()))
            .collect::<Vec<_>>();
        let Some(first) = pick(
            context,
            player,
            "Refit Troops: which infantry to replace",
            "infantry",
            &options,
        ) else {
            return;
        };
        let Some(first_index) = found.iter().position(|(id, _, _)| id == &first) else {
            return;
        };
        taken.push(first_index);
        let rest: Vec<(String, String)> = found
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != first_index)
            .map(|(_, (id, label, _))| (id.clone(), label.clone()))
            .chain(std::iter::once((
                "stop".to_owned(),
                "stop after one".to_owned(),
            )))
            .collect();
        let Some(second) = pick(
            context,
            player,
            "Refit Troops: another infantry or stop",
            "infantry",
            &rest,
        ) else {
            return;
        };
        if second != "stop" {
            let Some(second_index) = found
                .iter()
                .enumerate()
                .filter(|(i, _)| *i != first_index)
                .find(|(_, (id, _, _))| id == &second)
                .map(|(i, _)| i)
            else {
                return;
            };
            taken.push(second_index);
        }
    }
    let mut by_source: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for position in taken {
        let system_part = found[position].0.split('|').next().expect("a system part");
        by_source
            .entry(system_part.to_owned())
            .or_default()
            .push(position);
    }
    for (source_system, mut positions) in by_source {
        positions.sort_by(|a, b| {
            let index_of = |position: &usize| -> usize {
                found[*position]
                    .0
                    .rsplit_once('|')
                    .expect("an index")
                    .1
                    .parse()
                    .expect("numeric")
            };
            index_of(b).cmp(&index_of(a))
        });
        for position in positions {
            let planet_part = found[position]
                .0
                .split('|')
                .nth(1)
                .and_then(|rest| rest.split('|').next())
                .expect("system|planet|index");
            let index = found[position]
                .0
                .rsplit_once('|')
                .expect("an index")
                .1
                .parse::<usize>()
                .expect("numeric");
            let units = context
                .state
                .system_mut(&ti4_model::id::SystemId::new(&source_system))
                .planet_units
                .get_mut(&ti4_model::id::PlanetId::new(planet_part))
                .expect("the infantry was there");
            units.remove(index);
            units.push(ti4_model::units::Unit::new(mech.clone(), player.clone()));
        }
    }
}

/// Scuttle: "Choose 1 or 2 of your non-fighter ships on the game board and return them to your
/// reinforcements. Gain trade goods equal to the combined cost of those ships."
///
/// Non-fighter ships are the major ships in space: a fighter is excluded by its line, and
/// structures are not ships at all, so the search is the space unit lists. The trade goods are
/// the printed costs of the ships, which for every non-fighter are whole numbers.
#[allow(clippy::too_many_lines, clippy::cast_possible_truncation)]
fn scuttle(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    let types = ti4_content::units::catalogue(context.content, context.sources);
    let mut found: Vec<(String, String, ti4_model::units::Unit)> = Vec::new();
    for (system, board) in &context.state.board {
        for (index, unit) in board.units.iter().enumerate() {
            if &unit.owner != player
                || types
                    .get(unit.type_id.as_str())
                    .is_some_and(|kind| !kind.is_ship() || kind.is_fighter())
            {
                continue;
            }
            found.push((
                format!("{system}|{index}"),
                format!("{} in {system}", unit.type_id),
                unit.clone(),
            ));
        }
    }
    if found.is_empty() {
        return;
    }
    let mut taken: Vec<usize> = Vec::new();
    if found.len() == 1 {
        taken.push(0);
    } else {
        let options = found
            .iter()
            .map(|(id, label, _)| (id.clone(), label.clone()))
            .collect::<Vec<_>>();
        let Some(first) = pick(
            context,
            player,
            "Scuttle: which ship to scuttle",
            "ship",
            &options,
        ) else {
            return;
        };
        let Some(first_index) = found.iter().position(|(id, _, _)| id == &first) else {
            return;
        };
        taken.push(first_index);
        let rest: Vec<(String, String)> = found
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != first_index)
            .map(|(_, (id, label, _))| (id.clone(), label.clone()))
            .chain(std::iter::once((
                "stop".to_owned(),
                "stop after one".to_owned(),
            )))
            .collect();
        let Some(second) = pick(
            context,
            player,
            "Scuttle: another ship or stop",
            "ship",
            &rest,
        ) else {
            return;
        };
        if second != "stop" {
            let Some(second_index) = found
                .iter()
                .enumerate()
                .filter(|(i, _)| *i != first_index)
                .find(|(_, (id, _, _))| id == &second)
                .map(|(i, _)| i)
            else {
                return;
            };
            taken.push(second_index);
        }
    }
    let mut by_source: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for position in taken {
        let system_part = found[position].0.split('|').next().expect("a system part");
        by_source
            .entry(system_part.to_owned())
            .or_default()
            .push(position);
    }
    let mut goods = 0i32;
    for (source_system, mut positions) in by_source {
        positions.sort_by(|a, b| {
            let index_of = |position: &usize| -> usize {
                found[*position]
                    .0
                    .rsplit_once('|')
                    .expect("an index")
                    .1
                    .parse()
                    .expect("numeric")
            };
            index_of(b).cmp(&index_of(a))
        });
        for position in positions {
            let index = found[position]
                .0
                .rsplit_once('|')
                .expect("an index")
                .1
                .parse::<usize>()
                .expect("numeric");
            goods += types
                .get(found[position].2.type_id.as_str())
                .map_or(0, |kind| kind.cost().round() as i32);
            context
                .state
                .system_mut(&ti4_model::id::SystemId::new(&source_system))
                .units
                .remove(index);
        }
    }
    if let Some(seat) = context.state.player_mut(player) {
        seat.trade_goods += goods;
    }
}

/// Seize Artifact: "Choose 1 of your neighbors that has 1 or more relic fragments. That player
/// must give you 1 relic fragment of your choice."
fn seize_artifact(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    let victims: Vec<(String, String)> = neighbors(context.state, player)
        .iter()
        .filter_map(|neighbour| {
            let seat = context.state.player(neighbour)?;
            if seat.relic_fragments.values().sum::<i32>() > 0 {
                Some((neighbour.to_string(), neighbour.to_string()))
            } else {
                None
            }
        })
        .collect();
    if victims.is_empty() {
        return;
    }
    let Some(victim) = pick(
        context,
        player,
        "Seize Artifact: which neighbor to take from",
        "player",
        &victims,
    ) else {
        return;
    };
    let victim_id = ti4_model::id::PlayerId::new(&victim);
    let traits: Vec<(String, String)> = context
        .state
        .player(&victim_id)
        .expect("the victim was on offer")
        .relic_fragments
        .iter()
        .filter(|(_, count)| **count > 0)
        .map(|(trait_name, _)| (trait_name.clone(), trait_name.clone()))
        .collect();
    if traits.is_empty() {
        return;
    }
    let Some(trait_name) = pick(
        context,
        player,
        "Seize Artifact: which fragment to take",
        "fragment",
        &traits,
    ) else {
        return;
    };
    let Some(seat) = context.state.player_mut(&victim_id) else {
        return;
    };
    let count = seat.relic_fragments.get(&trait_name).copied().unwrap_or(0);
    if count <= 0 {
        return;
    }
    seat.relic_fragments.insert(trait_name.clone(), count - 1);
    if count - 1 == 0 {
        seat.relic_fragments.remove(&trait_name);
    }
    if let Some(mine) = context.state.player_mut(player) {
        *mine.relic_fragments.entry(trait_name).or_insert(0) += 1;
    }
}

/// Exchange Program: "Choose another player. You and that player may agree to place 1 infantry
/// from each of your reinforcements into coexistence on a planet the other player controls
/// that contains their ground forces; if no agreement is reached, you each discard 1 token
/// from your fleet pool."
///
/// The proposer names the planet, so the offer is a planet of the other player's that holds
/// their ground forces, and the other player answers yes or no. Coexistence here means both
/// infantry stand on the planet; the controller's control does not change hands, which is the
/// case the controller-stepping-aside machinery does not model, so the two units on the planet
/// are the whole record.
fn exchange_program(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    let others: Vec<(String, String)> = context
        .state
        .seating_order
        .iter()
        .filter(|other| *other != player)
        .map(|other| (other.to_string(), other.to_string()))
        .collect();
    if others.is_empty() {
        return;
    }
    let Some(other) = pick(
        context,
        player,
        "Exchange Program: which player to ask",
        "player",
        &others,
    ) else {
        return;
    };
    let other = ti4_model::id::PlayerId::new(&other);
    // A planet the other player controls on which their ground forces stand.
    let offers = exchange_offers(context, &other);
    if offers.is_empty() {
        // Nothing to offer is a failed deal: both sides pay the fleet token.
        refuse_exchange(context.state, player, &other);
        return;
    }
    let offers_only = offers
        .iter()
        .map(|(id, label, _, _)| (id.clone(), label.clone()))
        .collect::<Vec<_>>();
    let Some(chosen) = pick(
        context,
        player,
        "Exchange Program: which planet to offer",
        "planet",
        &offers_only,
    ) else {
        return;
    };
    let Some((_, _, system, planet)) = offers.iter().find(|(id, _, _, _)| *id == chosen).cloned()
    else {
        return;
    };
    let decision = crate::choice::Choice::new(
        other.clone(),
        "Exchange Program: accept the exchange?",
        vec![
            crate::choice::ChoiceOption::labelled("yes", "answer", "accept"),
            crate::choice::ChoiceOption::labelled("no", "answer", "refuse"),
        ],
    );
    let answer = context.table.ask_seeing(
        &decision,
        &crate::choice::Observed::new(
            context.state,
            context.content,
            context.sources,
            context.galaxy,
        ),
    );
    let Ok(answer) = answer else {
        return;
    };
    if answer.id != "yes" {
        refuse_exchange(context.state, player, &other);
        return;
    }
    let infantry = ti4_model::id::UnitTypeId::new("infantry");
    let units = context
        .state
        .system_mut(&system)
        .planet_units
        .entry(planet)
        .or_default();
    units.push(ti4_model::units::Unit::new(infantry.clone(), other.clone()));
    units.push(ti4_model::units::Unit::new(infantry, player.clone()));
}

/// Exchange Program's offerable set: planets `other` controls on which their own ground forces
/// stand.
fn exchange_offers(
    context: &crate::timing::TimingContext<'_>,
    other: &PlayerId,
) -> Vec<(
    String,
    String,
    ti4_model::id::SystemId,
    ti4_model::id::PlanetId,
)> {
    let types = ti4_content::units::catalogue(context.content, context.sources);
    let mut offers: Vec<(
        String,
        String,
        ti4_model::id::SystemId,
        ti4_model::id::PlanetId,
    )> = Vec::new();
    for (system, planet) in context.state.controlled_planets(other) {
        if context
            .state
            .system_state(system)
            .planet_units
            .get(planet)
            .is_some_and(|units| {
                units.iter().any(|unit| {
                    unit.owner == *other
                        && types
                            .get(unit.type_id.as_str())
                            .is_some_and(ti4_content::units::UnitType::is_ground_force)
                })
            })
        {
            offers.push((
                format!("{system}|{planet}"),
                format!("{planet} in {system}"),
                system.clone(),
                planet.clone(),
            ));
        }
    }
    offers
}

/// The failed half of Exchange Program: both players discard 1 fleet token. A pool that is
/// already empty simply has nothing to discard.
fn refuse_exchange(state: &mut GameState, player: &PlayerId, other: &PlayerId) {
    if let Some(seat) = state.player_mut(player) {
        seat.spend_token(ti4_model::state::TokenPool::Fleet);
    }
    if let Some(seat) = state.player_mut(other) {
        seat.spend_token(ti4_model::state::TokenPool::Fleet);
    }
}

/// Mercenary Contract: "Spend 2 trade goods to place 2 neutral infantry on any non-home planet
/// that contains no units; if that planet was owned by another player, they return its planet
/// card to the planet card deck."
///
/// The planet itself must be bare: no ground forces, no structures. Ships in the system are
/// not units on the planet. The planet-card half is not modelled: the engine does not track
/// planet cards in hands, so an owned planet keeps its owner and the gap is recorded in the
/// card's doc comment rather than invented state.
fn mercenary_contract(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    if context
        .state
        .player(player)
        .is_some_and(|seat| seat.trade_goods < 2)
    {
        return;
    }
    let homes = ti4_content::galaxy::home_systems(context.content, context.sources);
    let all = ti4_content::galaxy::all_planets(context.content, context.sources);
    let mut options: Vec<(String, String)> = Vec::new();
    for (system, board) in &context.state.board {
        if homes.contains(system.as_str()) {
            continue;
        }
        for (planet, units) in &board.planet_units {
            if !units.is_empty() {
                continue;
            }
            if !all.contains_key(planet.as_str()) {
                continue; // a planet the map does not know about is not on offer
            }
            options.push((
                format!("{system}|{planet}"),
                format!("{planet} in {system}"),
            ));
        }
    }
    if options.is_empty() {
        return;
    }
    let Some(chosen) = pick(
        context,
        player,
        "Mercenary Contract: which planet the infantry land on",
        "planet",
        &options,
    ) else {
        return;
    };
    let (system_part, planet_part) = chosen
        .split_once('|')
        .map(|(s, p)| (s.to_owned(), p.to_owned()))
        .unwrap_or_default();
    let (system, planet) = (
        ti4_model::id::SystemId::new(&system_part),
        ti4_model::id::PlanetId::new(&planet_part),
    );
    let infantry = ti4_model::id::UnitTypeId::new("infantry");
    let neutral = ti4_model::id::PlayerId::new(crate::neutral_units::NEUTRAL);
    let units = context
        .state
        .system_mut(&system)
        .planet_units
        .entry(planet)
        .or_default();
    units.push(ti4_model::units::Unit::new(
        infantry.clone(),
        neutral.clone(),
    ));
    units.push(ti4_model::units::Unit::new(infantry, neutral));
    if let Some(seat) = context.state.player_mut(player) {
        seat.trade_goods -= 2;
    }
}

/// Pirate Fleet: "Spend 3 resources to place 1 neutral carrier, 1 neutral cruiser, 1 neutral
/// destroyer, and 2 neutral fighters in a non-home system that contains no non-neutral ships."
fn pirate_fleet(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    let paid = crate::production::pay(
        context.state,
        context.content,
        context.sources,
        context.table,
        player,
        3,
        crate::production::Spend::Resources,
    );
    let Ok(paid) = paid else {
        return;
    };
    if !paid {
        return;
    }
    let systems = pirate_systems_off_homes(context.state, context.content, context.sources);
    if systems.is_empty() {
        return;
    }
    let options = systems
        .iter()
        .map(|system| (system.to_string(), system.to_string()))
        .collect::<Vec<_>>();
    let Some(system) = pick(
        context,
        player,
        "Pirate Fleet: which system the fleet enters",
        "system",
        &options,
    ) else {
        return;
    };
    let neutral = ti4_model::id::PlayerId::new(crate::neutral_units::NEUTRAL);
    let fleet = ["carrier", "cruiser", "destroyer", "fighter", "fighter"];
    let system = ti4_model::id::SystemId::new(&system);
    let board = context.state.system_mut(&system);
    for kind in fleet {
        board.units.push(ti4_model::units::Unit::new(
            ti4_model::id::UnitTypeId::new(kind),
            neutral.clone(),
        ));
    }
}

/// Pirate Contract: "Place 1 neutral destroyer in a non-home system that contains no
/// non-neutral ships."
fn pirate_contract(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    let systems = pirate_systems_off_homes(context.state, context.content, context.sources);
    if systems.is_empty() {
        return;
    }
    let options = systems
        .iter()
        .map(|system| (system.to_string(), system.to_string()))
        .collect::<Vec<_>>();
    let Some(system) = pick(
        context,
        player,
        "Pirate Contract: which system the destroyer enters",
        "system",
        &options,
    ) else {
        return;
    };
    let neutral = ti4_model::id::PlayerId::new(crate::neutral_units::NEUTRAL);
    context
        .state
        .system_mut(&ti4_model::id::SystemId::new(&system))
        .units
        .push(ti4_model::units::Unit::new(
            ti4_model::id::UnitTypeId::new("destroyer"),
            neutral,
        ));
}

/// Brilliance: "Ready 1 of your planets that has a technology specialty or choose 1 player to
/// gain their breakthrough."
///
/// The corpus does not mark which planets carry a technology specialty, so that half of the
/// card is not offered: guessing which planets qualify would invent a rule the content does
/// not state. The breakthrough half is the whole choice when any other player holds one, and
/// gaining it takes it from them.
fn brilliance(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    let options: Vec<(String, String, ti4_model::id::PlayerId)> = context
        .state
        .seating_order
        .iter()
        .filter(|other| *other != player)
        .filter_map(|other| {
            let seat = context.state.player(other)?;
            seat.breakthrough.is_some().then(|| {
                (
                    other.to_string(),
                    format!("gain {other}'s breakthrough"),
                    other.clone(),
                )
            })
        })
        .collect();
    if options.is_empty() {
        return;
    }
    let options_only = options
        .iter()
        .map(|(id, label, _)| (id.clone(), label.clone()))
        .collect::<Vec<_>>();
    let Some(chosen) = pick(
        context,
        player,
        "Brilliance: which breakthrough to gain",
        "player",
        &options_only,
    ) else {
        return;
    };
    let Some((_, _, owner)) = options.iter().find(|(id, _, _)| *id == chosen).cloned() else {
        return;
    };
    let Some(theirs) = context
        .state
        .player(&owner)
        .and_then(|seat| seat.breakthrough.clone())
    else {
        return;
    };
    if let Some(seat) = context.state.player_mut(&owner) {
        seat.breakthrough = None;
    }
    if let Some(seat) = context.state.player_mut(player) {
        seat.breakthrough = Some(theirs);
    }
}

/// The strategy-card half of Overrule and Strategize: run the chosen card's ability through
/// the engine's own strategy-card resolver, for the card player. The readied cards of the
/// other players keep their owners: performing a card's ability does not move the card.
type StrategyAbility = fn(
    &mut GameState,
    &ContentStore,
    ti4_model::content_types::SourceSet,
    Option<&Galaxy>,
    &mut crate::choice::Table,
    &PlayerId,
    &str,
) -> Result<crate::strategy_cards::Ability, crate::choice::IllegalChoice>;

#[allow(clippy::match_same_arms)]
fn perform_strategy_card(
    context: &mut crate::timing::TimingContext<'_>,
    player: &PlayerId,
    prompt: &str,
    ability: StrategyAbility,
) {
    let mut options: Vec<(String, String)> = Vec::new();
    for other in &context.state.seating_order {
        if other == player {
            continue;
        }
        let Some(seat) = context.state.player(other) else {
            continue;
        };
        for card in &seat.strategy_cards {
            if seat.exhausted_strategy_cards.contains(card) {
                continue; // an exhausted card cannot be performed
            }
            options.push((
                card.to_string(),
                format!(
                    "{}'s {}",
                    other,
                    crate::strategy_cards::card_name(context.content, card.as_str())
                        .unwrap_or_else(|| card.to_string())
                ),
            ));
        }
    }
    for card in &context.state.unclaimed_strategy_cards {
        options.push((
            card.to_string(),
            crate::strategy_cards::card_name(context.content, card.as_str())
                .unwrap_or_else(|| card.to_string()),
        ));
    }
    if options.is_empty() {
        return;
    }
    let Some(alias) = pick(context, player, prompt, "strategy card", &options) else {
        return;
    };
    let result = ability(
        context.state,
        context.content,
        context.sources,
        context.galaxy,
        context.table,
        player,
        &alias,
    );
    match result {
        Ok(crate::strategy_cards::Ability::FreeTactical(system)) => {
            // The ability is a free tactical action. The effect context carries the game's
            // state but not its turn machinery, so the activation is recorded — the player is
            // the active one, the system is the active system — while the move itself and the
            // windows around it belong to the driver.
            context.state.active = Some(player.clone());
            context.state.active_system = Some(system);
        }
        Ok(_) => {}
        Err(_) => {
            // The decider answered the ability's own question in a way the ability does not
            // accept; the card fizzles.
        }
    }
}

/// Overrule: "Perform the primary ability of a readied or unchosen strategy card."
fn overrule(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    perform_strategy_card(
        context,
        player,
        "Overrule: which card's primary ability to perform",
        crate::strategy_cards::primary,
    );
}

/// Strategize: "Perform the secondary ability of any readied or unchosen strategy card."
fn strategize(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    perform_strategy_card(
        context,
        player,
        "Strategize: which card's secondary ability to perform",
        crate::strategy_cards::secondary,
    );
}
/// Ask this player to pick one of several things, or take the only one on offer.
///
/// One is not a decision and none is not a question: asking either would put a line in the
/// decision log that no player ever chose.
fn pick(
    context: &mut crate::timing::TimingContext<'_>,
    player: &PlayerId,
    prompt: &str,
    kind: &str,
    options: &[(String, String)],
) -> Option<String> {
    match options {
        [] => None,
        [(only, _)] => Some(only.clone()),
        many => {
            let choice = crate::choice::Choice::new(
                player.clone(),
                prompt,
                many.iter()
                    .map(|(id, label)| {
                        crate::choice::ChoiceOption::labelled(id.clone(), kind, label.clone())
                    })
                    .collect(),
            );
            context.ask_seeing(&choice).ok().map(|answer| answer.id)
        }
    }
}

/// `system|planet` pairs this player controls.
fn controlled_spots(state: &GameState, player: &PlayerId) -> Vec<(String, String)> {
    state
        .controlled_planets(player)
        .into_iter()
        .map(|(system, planet)| (format!("{system}|{planet}"), planet.to_string()))
        .collect()
}

/// Split a `system|planet` option id.
fn spot(id: &str) -> Option<(ti4_model::id::SystemId, ti4_model::id::PlanetId)> {
    let (system, planet) = id.split_once('|')?;
    Some((
        ti4_model::id::SystemId::new(system),
        ti4_model::id::PlanetId::new(planet),
    ))
}

/// Place `count` units of a base type, capped by what the box still holds (31.4).
pub fn place_units(
    context: &mut crate::timing::TimingContext<'_>,
    player: &PlayerId,
    system: &ti4_model::id::SystemId,
    planet: Option<&ti4_model::id::PlanetId>,
    base_type: &str,
    count: usize,
) {
    let faction = context
        .state
        .player(player)
        .map(|seat| seat.faction.to_string())
        .unwrap_or_default();
    let generic = ti4_content::units::catalogue(context.content, context.sources)
        .get(base_type)
        .map(|unit| unit.id().to_owned());
    let Some(id) =
        ti4_content::units::faction_unit(context.content, &faction, base_type, context.sources)
            .map(|unit| unit.id().to_owned())
            .or(generic)
    else {
        return;
    };
    let type_id = ti4_model::id::UnitTypeId::new(id);
    let count = crate::supply::allowed(
        context.state,
        context.content,
        context.sources,
        player,
        &type_id,
        count,
    );
    for _ in 0..count {
        let unit = ti4_model::units::Unit::new(type_id.clone(), player.clone());
        match planet {
            Some(planet) => context
                .state
                .system_mut(system)
                .planet_units
                .entry(planet.clone())
                .or_default()
                .push(unit),
            None => context.state.system_mut(system).units.push(unit),
        }
    }
}

/// Systems holding at least one ship of this player's.
fn systems_with_my_ships(
    state: &GameState,
    content: &ContentStore,
    sources: ti4_model::content_types::SourceSet,
    player: &PlayerId,
) -> Vec<String> {
    let types = ti4_content::units::catalogue(content, sources);
    state
        .board
        .iter()
        .filter(|(_, board)| {
            board.units_of(player).into_iter().any(|unit| {
                types
                    .get(unit.type_id.as_str())
                    .is_some_and(ti4_content::units::UnitType::is_ship)
            })
        })
        .map(|(id, _)| id.to_string())
        .collect()
}

/// Every `system|planet` holding at least one unit of a base type, whoever owns it.
fn planets_holding(
    state: &GameState,
    content: &ContentStore,
    sources: ti4_model::content_types::SourceSet,
    base_type: &str,
) -> Vec<(String, String)> {
    let types = ti4_content::units::catalogue(content, sources);
    let mut found = Vec::new();
    for (system, board) in &state.board {
        for (planet, units) in &board.planet_units {
            if units.iter().any(|unit| {
                types
                    .get(unit.type_id.as_str())
                    .is_some_and(|kind| kind.base_type() == base_type)
            }) {
                found.push((format!("{system}|{planet}"), planet.to_string()));
            }
        }
    }
    found
}

// -- the cards -----------------------------------------------------------------------------------

/// Rise of a Messiah: one infantry on each planet you control.
fn rise_of_a_messiah(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    for (id, _) in controlled_spots(context.state, player) {
        if let Some((system, planet)) = spot(&id) {
            place_units(context, player, &system, Some(&planet), "infantry", 1);
        }
    }
}

/// Frontline Deployment: three infantry on one planet you control.
fn frontline_deployment(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    let spots = controlled_spots(context.state, player);
    let Some(chosen) = pick(
        context,
        player,
        "Frontline Deployment: onto which planet",
        "planet",
        &spots,
    ) else {
        return;
    };
    if let Some((system, planet)) = spot(&chosen) {
        place_units(context, player, &system, Some(&planet), "infantry", 3);
    }
}

/// Manipulate Investments: five trade goods onto strategy cards, across at least three of them.
///
/// > Place a total of 5 trade goods from the supply on strategy cards of your choice. You must
/// > place these tokens on at least 3 different cards.
///
/// `GameState::strategy_card_goods` already holds this: the strategy phase puts a good on every
/// unpicked card and `game.rs` pays it out when one is taken. Nothing had to be added for the card
/// — the slot was there with no caller.
///
/// The "at least 3 different" clause is enforced by narrowing the offer rather than by validating
/// afterwards. Once the tokens left equal the distinct cards still owed, only unused cards are
/// offered, so a legal placement is the only placement available and the card cannot be played into
/// a state it forbids.
fn manipulate_investments(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    const TOKENS: usize = 5;
    const DISTINCT: usize = 3;

    // Strategy cards key on `id`, not `alias` -- the one content category that does.
    let cards: Vec<String> = context
        .content
        .from_sources(ContentType::StrategyCards, context.sources)
        .filter_map(|record| record.text("id"))
        .map(std::borrow::ToOwned::to_owned)
        .collect();
    if cards.len() < DISTINCT {
        return; // 22.3: a card that cannot fully resolve is not played
    }

    let mut used: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for placed in 0..TOKENS {
        let remaining = TOKENS - placed;
        let owed = DISTINCT.saturating_sub(used.len());
        let offer: Vec<(String, String)> = cards
            .iter()
            .filter(|alias| remaining > owed || !used.contains(*alias))
            .map(|alias| (alias.clone(), format!("place a trade good on {alias}")))
            .collect();
        let Some(chosen) = pick(
            context,
            player,
            "Manipulate Investments: place a trade good on which strategy card",
            "strategy_card",
            &offer,
        ) else {
            return;
        };
        *context
            .state
            .strategy_card_goods
            .entry(ti4_model::id::StrategyCardId::new(chosen.clone()))
            .or_insert(0) += 1;
        used.insert(chosen);
    }
}

/// Lie in Wait: take one action card from each of two neighbours who have traded.
///
/// > Look at each of those players' hands of action cards, then choose and take 1 action card from
/// > each.
///
/// "Those players" are the neighbours whose transaction opened the window, which is a fact about
/// the round rather than about one deal -- so it comes from
/// `transactions::neighbours_who_transacted` rather than from the event payload. A neighbour who
/// traded twice is one player with one hand, and counts once.
///
/// Looking at the hand needs no modelling: nothing in this engine hides a hand from a decider. What
/// the card *does* is the taking, and a neighbour holding nothing is skipped rather than refused --
/// the card takes from each who has one.
fn lie_in_wait(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    let Some(galaxy) = context.galaxy else {
        return; // 22.3: without the map there are no neighbours, so nothing to resolve
    };
    let traders = crate::transactions::neighbours_who_transacted(context.state, galaxy, player);
    for victim in traders.into_iter().take(2) {
        let hand: Vec<(String, String)> = context
            .state
            .player(&victim)
            .map(|seat| seat.action_cards.clone())
            .unwrap_or_default()
            .into_iter()
            .map(|alias| {
                let label = format!("take {alias} from {victim}");
                (alias.to_string(), label)
            })
            .collect();
        if hand.is_empty() {
            continue;
        }
        let Some(taken) = pick(
            context,
            player,
            "Lie in Wait: take which action card",
            "action_card",
            &hand,
        ) else {
            continue;
        };
        let taken = ti4_model::id::ActionCardId::new(taken);
        if let Some(seat) = context.state.player_mut(&victim) {
            if let Some(at) = seat.action_cards.iter().position(|held| *held == taken) {
                seat.action_cards.remove(at);
            } else {
                continue;
            }
        }
        if let Some(seat) = context.state.player_mut(player) {
            seat.action_cards.push(taken);
        }
    }
}

/// Mining Initiative: trade goods equal to the resource value of one planet you hold.
fn mining_initiative(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    let spots = controlled_spots(context.state, player);
    let Some(chosen) = pick(
        context,
        player,
        "Mining Initiative: mine which planet",
        "planet",
        &spots,
    ) else {
        return;
    };
    let Some((_, planet)) = spot(&chosen) else {
        return;
    };
    let worth = crate::production::planet_value(
        context.content,
        context.sources,
        &planet,
        crate::production::Spend::Resources,
    );
    if let Some(seat) = context.state.player_mut(player) {
        seat.trade_goods += i32::try_from(worth).unwrap_or(0);
    }
}

/// War Effort: one cruiser into a system that already holds a ship of yours.
fn war_effort(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    let systems: Vec<(String, String)> =
        systems_with_my_ships(context.state, context.content, context.sources, player)
            .into_iter()
            .map(|system| (system.clone(), format!("cruiser into {system}")))
            .collect();
    let Some(chosen) = pick(
        context,
        player,
        "War Effort: into which system",
        "system",
        &systems,
    ) else {
        return;
    };
    place_units(
        context,
        player,
        &ti4_model::id::SystemId::new(chosen),
        None,
        "cruiser",
        1,
    );
}

/// Cripple Defenses: choose one planet and destroy *each* PDS on it (63).
fn cripple_defenses(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    let spots = planets_holding(context.state, context.content, context.sources, "pds");
    let Some(chosen) = pick(
        context,
        player,
        "Cripple Defenses: which planet",
        "planet",
        &spots,
    ) else {
        return;
    };
    let Some((system, planet)) = spot(&chosen) else {
        return;
    };
    let types = ti4_content::units::catalogue(context.content, context.sources);
    if let Some(units) = context
        .state
        .system_mut(&system)
        .planet_units
        .get_mut(&planet)
    {
        // Each of them, not one: the card says "destroy each PDS on that planet".
        units.retain(|unit| {
            types
                .get(unit.type_id.as_str())
                .is_none_or(|kind| kind.base_type() != "pds")
        });
    }
}

/// Repeal Law: discard one law from play.
fn repeal_law(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    let laws: Vec<(String, String)> = crate::laws::in_play(context.state)
        .into_iter()
        .map(|alias| (alias.clone(), alias))
        .collect();
    let Some(chosen) = pick(context, player, "repeal which law", "repeal", &laws) else {
        return;
    };
    crate::laws::repeal(context.state, &chosen);
}

/// Insubordination: a rival loses one token from their tactic pool (20.4).
fn insubordination(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    let targets: Vec<(String, String)> = context
        .state
        .players
        .iter()
        .filter(|seat| &seat.id != player)
        .filter(|seat| seat.tokens(ti4_model::state::TokenPool::Tactic) > 0)
        .map(|seat| {
            (
                seat.id.to_string(),
                format!("take a tactic token from {}", seat.id),
            )
        })
        .collect();
    let Some(chosen) = pick(
        context,
        player,
        "Insubordination: whose tactic pool",
        "player",
        &targets,
    ) else {
        return;
    };
    if let Some(seat) = context.state.player_mut(&PlayerId::new(chosen)) {
        seat.gain_token(ti4_model::state::TokenPool::Tactic, -1);
    }
}

/// Unexpected Action: lift one of your command tokens off the board.
///
/// It returns to reinforcements rather than to a pool, so nothing is gained — what the card buys
/// is the right to activate that system again (89.1).
fn unexpected_action(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    let held: Vec<(String, String)> = context
        .state
        .systems_with_token(player)
        .into_iter()
        .map(|system| {
            (
                system.to_string(),
                format!("recall your token from {system}"),
            )
        })
        .collect();
    let Some(chosen) = pick(
        context,
        player,
        "Unexpected Action: recall your token from where",
        "recall",
        &held,
    ) else {
        return;
    };
    context
        .state
        .system_mut(&ti4_model::id::SystemId::new(chosen))
        .command_tokens
        .remove(player);
}

/// Destroy up to `limit` units of a base type from a planet, and report how many died.
fn destroy_on_planet(
    context: &mut crate::timing::TimingContext<'_>,
    system: &ti4_model::id::SystemId,
    planet: &ti4_model::id::PlanetId,
    base_type: &str,
    limit: Option<usize>,
) -> usize {
    let types = ti4_content::units::catalogue(context.content, context.sources);
    let Some(units) = context
        .state
        .system_mut(system)
        .planet_units
        .get_mut(planet)
    else {
        return 0;
    };
    let mut destroyed = 0;
    units.retain(|unit| {
        if limit.is_some_and(|cap| destroyed >= cap) {
            return true;
        }
        let hit = types
            .get(unit.type_id.as_str())
            .is_some_and(|kind| kind.base_type() == base_type);
        if hit {
            destroyed += 1;
        }
        !hit
    });
    destroyed
}

/// Every `system|planet` a *rival* controls.
fn rival_planets(state: &GameState, player: &PlayerId) -> Vec<(String, String)> {
    let mut found = Vec::new();
    for (system, board) in &state.board {
        for (planet, owner) in &board.planet_control {
            if owner != player {
                found.push((format!("{system}|{planet}"), planet.to_string()));
            }
        }
    }
    found
}

/// Reactor Meltdown: destroy one space dock in a non-home system (79).
fn reactor_meltdown(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    let docks: Vec<(String, String)> =
        planets_holding(context.state, context.content, context.sources, "spacedock")
            .into_iter()
            .filter(|(id, _)| {
                spot(id).is_some_and(|(system, _)| {
                    !ti4_content::galaxy::is_home_system(
                        context.content,
                        system.as_str(),
                        context.sources,
                    )
                })
            })
            .collect();
    let Some(chosen) = pick(
        context,
        player,
        "Reactor Meltdown: which space dock",
        "planet",
        &docks,
    ) else {
        return;
    };
    if let Some((system, planet)) = spot(&chosen) {
        // One dock, not every dock on the planet: the card says "1 space dock".
        destroy_on_planet(context, &system, &planet, "spacedock", Some(1));
    }
}

/// Unstable Planet: exhaust one hazardous planet and destroy up to three infantry on it.
fn unstable_planet(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    let catalogue = ti4_content::galaxy::all_planets(context.content, context.sources);
    let hazardous: Vec<(String, String)> = context
        .state
        .board
        .iter()
        .flat_map(|(system, board)| {
            board
                .planet_units
                .keys()
                .map(move |planet| (system, planet))
        })
        .filter(|(_, planet)| {
            catalogue
                .get(planet.as_str())
                .is_some_and(|record| record.has_trait("hazardous"))
        })
        .map(|(system, planet)| (format!("{system}|{planet}"), planet.to_string()))
        .collect();
    let Some(chosen) = pick(
        context,
        player,
        "Unstable Planet: which hazardous planet",
        "planet",
        &hazardous,
    ) else {
        return;
    };
    let Some((system, planet)) = spot(&chosen) else {
        return;
    };
    context.state.exhausted_planets.insert(planet.clone());
    destroy_on_planet(context, &system, &planet, "infantry", Some(3));
}

/// Uprising: exhaust a rival's non-home planet and take its resource value in trade goods.
fn uprising(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    let targets: Vec<(String, String)> = rival_planets(context.state, player)
        .into_iter()
        .filter(|(id, _)| {
            spot(id).is_some_and(|(system, _)| {
                !ti4_content::galaxy::is_home_system(
                    context.content,
                    system.as_str(),
                    context.sources,
                )
            })
        })
        .collect();
    let Some(chosen) = pick(
        context,
        player,
        "Uprising: which planet",
        "planet",
        &targets,
    ) else {
        return;
    };
    let Some((_, planet)) = spot(&chosen) else {
        return;
    };
    let worth = crate::production::planet_value(
        context.content,
        context.sources,
        &planet,
        crate::production::Spend::Resources,
    );
    context.state.exhausted_planets.insert(planet);
    if let Some(seat) = context.state.player_mut(player) {
        seat.trade_goods += i32::try_from(worth).unwrap_or(0);
    }
}

/// Plague: one die per infantry on a rival planet; each 6 or better destroys one of them.
fn plague(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    let targets = rival_planets(context.state, player);
    let Some(chosen) = pick(context, player, "Plague: which planet", "planet", &targets) else {
        return;
    };
    let Some((system, planet)) = spot(&chosen) else {
        return;
    };
    let types = ti4_content::units::catalogue(context.content, context.sources);
    let infantry = context
        .state
        .system_state(&system)
        .planet_units
        .get(&planet)
        .map_or(0, |units| {
            units
                .iter()
                .filter(|unit| {
                    types
                        .get(unit.type_id.as_str())
                        .is_some_and(|kind| kind.base_type() == "infantry")
                })
                .count()
        });
    if infantry == 0 {
        return;
    }
    // One die each, through the seeded roller: an ambient generator here would break replay.
    let roll = context
        .dice
        .roll(context.rng, infantry, "plague", Some(PLAGUE_KILLS_ON));
    let kills = roll
        .faces
        .iter()
        .filter(|face| **face >= PLAGUE_KILLS_ON)
        .count();
    destroy_on_planet(context, &system, &planet, "infantry", Some(kills));
}

/// Plague destroys an infantry on a six or better.
const PLAGUE_KILLS_ON: u32 = 6;

/// Spy: a chosen player hands you one random action card from their hand (2.5).
fn spy(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    let targets: Vec<(String, String)> = context
        .state
        .players
        .iter()
        .filter(|seat| &seat.id != player)
        .filter(|seat| !seat.action_cards.is_empty())
        .map(|seat| (seat.id.to_string(), format!("take a card from {}", seat.id)))
        .collect();
    let Some(chosen) = pick(context, player, "Spy: rob which player", "player", &targets) else {
        return;
    };
    let victim = PlayerId::new(chosen);
    let held = context
        .state
        .player(&victim)
        .map_or(0, |seat| seat.action_cards.len());
    if held == 0 {
        return;
    }
    // "Random" comes from the seeded roller too, or replay diverges.
    let face = context
        .dice
        .roll(context.rng, 1, "spy", None)
        .faces
        .first()
        .copied()
        .unwrap_or(1) as usize;
    let index = (face - 1) % held;
    let Some(taken) = discard(context.state, &victim, index) else {
        return;
    };
    if let Some(seat) = context.state.player_mut(player) {
        seat.action_cards.push(taken);
    }
}

/// Ghost Ship: one destroyer into a non-home wormhole system free of anyone else's ships.
fn ghost_ship(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    let Some(galaxy) = context.galaxy else {
        return; // without a map there are no wormhole systems to name
    };
    let systems = ti4_content::galaxy::all_systems(context.content, context.sources);
    let types = ti4_content::units::catalogue(context.content, context.sources);
    let open: Vec<(String, String)> = galaxy
        .system_ids()
        .into_iter()
        .filter(|id| {
            systems
                .get(id)
                .is_some_and(|system| !system.wormholes().is_empty())
        })
        .filter(|id| !ti4_content::galaxy::is_home_system(context.content, id, context.sources))
        .filter(|id| {
            !context
                .state
                .system_state(&ti4_model::id::SystemId::new(*id))
                .units
                .iter()
                .any(|unit| {
                    &unit.owner != player
                        && types
                            .get(unit.type_id.as_str())
                            .is_some_and(ti4_content::units::UnitType::is_ship)
                })
        })
        .map(|id| (id.to_owned(), format!("destroyer into {id}")))
        .collect();
    let Some(chosen) = pick(
        context,
        player,
        "Ghost Ship: into which wormhole system",
        "system",
        &open,
    ) else {
        return;
    };
    place_units(
        context,
        player,
        &ti4_model::id::SystemId::new(chosen),
        None,
        "destroyer",
        1,
    );
}

/// Focused Research costs this much.
const FOCUSED_RESEARCH_COST: i32 = 4;

/// Focused Research: spend four trade goods to research one technology.
fn focused_research(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    let held = context
        .state
        .player(player)
        .map_or(0, |seat| seat.trade_goods);
    if held < FOCUSED_RESEARCH_COST {
        return; // 22.3: it cannot resolve, so it does nothing
    }
    let available =
        crate::technology::researchable(context.state, context.content, context.sources, player);
    let options: Vec<(String, String)> = available
        .iter()
        .map(|alias| (alias.to_string(), format!("research {alias}")))
        .collect();
    let Some(chosen) = pick(
        context,
        player,
        "Focused Research: research a technology",
        "technology",
        &options,
    ) else {
        return; // nothing to research, and nothing is charged for it
    };
    if let Some(seat) = context.state.player_mut(player) {
        seat.trade_goods -= FOCUSED_RESEARCH_COST;
    }
    let _ = crate::technology::research(
        context.state,
        context.content,
        context.sources,
        player,
        &ti4_model::id::TechnologyId::new(chosen),
    );
}

/// Tactical Bombardment: exhaust every rival-controlled planet in one system holding your units.
///
/// Nothing here is a bombardment *roll*, so 65's planetary shield does not apply — the card
/// exhausts, it does not destroy.
fn tactical_bombardment(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    let systems: Vec<(String, String)> = context
        .state
        .board
        .iter()
        .filter(|(_, board)| !board.units_of(player).is_empty())
        .filter(|(_, board)| board.planet_control.values().any(|owner| owner != player))
        .map(|(system, _)| (system.to_string(), format!("bombard {system}")))
        .collect();
    let Some(chosen) = pick(
        context,
        player,
        "Tactical Bombardment: which system",
        "system",
        &systems,
    ) else {
        return;
    };
    let system = ti4_model::id::SystemId::new(chosen);
    let hit: Vec<ti4_model::id::PlanetId> = context
        .state
        .system_state(&system)
        .planet_control
        .iter()
        .filter(|(_, owner)| *owner != player)
        .map(|(planet, _)| planet.clone())
        .collect();
    for planet in hit {
        context.state.exhausted_planets.insert(planet);
    }
}

/// The systems Signal Jamming can close off.
///
/// Oracle parity (`engine/action_cards.py` `_jamming_systems`, 6): non-home systems within the
/// effective galaxy that hold one of your ships, or are adjacent to such a system, sorted by id.
/// A ship on your homeworld still counts for adjacency; the home system itself is never offered
/// (88.2). No effective galaxy means nothing can be jammed at all.
#[must_use]
pub fn jamming_systems(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: Option<&Galaxy>,
    player: &PlayerId,
) -> Vec<String> {
    let Some(galaxy) = galaxy else {
        return Vec::new();
    };
    // Own systems restricted to the effective galaxy; only ships count (6).
    let types = ti4_content::units::catalogue(content, sources);
    let mut mine: BTreeSet<&str> = BTreeSet::new();
    for (system, board) in &state.board {
        if galaxy.coord_of(system.as_str()).is_none() {
            continue;
        }
        let has_ship = board.units.iter().any(|unit| {
            &unit.owner == player
                && types
                    .get(unit.type_id.as_str())
                    .is_some_and(UnitType::is_ship)
        });
        if has_ship {
            mine.insert(system.as_str());
        }
    }
    let mut reach: BTreeSet<&str> = mine.clone();
    for system in &mine {
        reach.extend(galaxy.adjacent(system));
    }
    // Home systems are never jammable, even when they hold the player's ships.
    // One corpus pass for the whole set, not one per system in `reach`.
    let homes = ti4_content::galaxy::home_systems(content, sources);
    reach.retain(|system| !homes.contains(*system));
    reach.into_iter().map(str::to_owned).collect()
}

/// Signal Jamming: strand a rival's command token in a system near your ships.
///
/// 20.6: a token placed where that player already has one goes back to their reinforcements
/// instead — which here is a set that already holds it, so the placement is idempotent and the
/// system stays closed to them (89.1).
fn signal_jamming(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    // The oracle's option set is the eligibility itself (`_jamming_systems`): ships only, in
    // the effective galaxy, adjacency expanded, home systems excluded. An empty set means
    // there is nothing to jam and the card fizzles.
    let systems: Vec<(String, String)> = jamming_systems(
        context.state,
        context.content,
        context.sources,
        context.galaxy,
        player,
    )
    .into_iter()
    .map(|system| (system.clone(), format!("jam {system}")))
    .collect();
    let Some(where_to) = pick(
        context,
        player,
        "Signal Jamming: jam which system",
        "system",
        &systems,
    ) else {
        return;
    };
    // engine/action_cards.py:1059–1064 names the rivals by faction (Python player ids are
    // factions) and the prompt carries the chosen system, so the answer maps back to a seat.
    let rivals: Vec<(String, PlayerId)> = context
        .state
        .players
        .iter()
        .filter(|seat| &seat.id != player)
        .map(|seat| {
            (
                crate::promissory::faction_name(context.state, &seat.id),
                seat.id.clone(),
            )
        })
        .collect();
    let victims: Vec<(String, String)> = rivals
        .iter()
        .map(|(name, _)| (name.clone(), format!("{name}'s command token")))
        .collect();
    let Some(victim) = pick(
        context,
        player,
        &format!("Signal Jamming: whose token goes into {where_to}"),
        "player",
        &victims,
    ) else {
        return;
    };
    // Structurally unreachable otherwise: `pick` only returns ids it offered, and those were
    // built from `rivals`.
    let Some((_, seat)) = rivals.iter().find(|(name, _)| *name == victim) else {
        return;
    };
    context
        .state
        .system_mut(&ti4_model::id::SystemId::new(where_to))
        .command_tokens
        .insert(seat.clone());
}

/// Lucky Shot: destroy one dreadnought, cruiser or destroyer in a system you hold a planet in.
fn lucky_shot(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    let types = ti4_content::units::catalogue(context.content, context.sources);
    let mut targets: Vec<(String, String)> = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for (system, board) in &context.state.board {
        if !board.controls_a_planet(player) {
            continue;
        }
        for (index, unit) in board.units.iter().enumerate() {
            if &unit.owner == player {
                continue;
            }
            let Some(kind) = types.get(unit.type_id.as_str()) else {
                continue;
            };
            if !matches!(kind.base_type(), "dreadnought" | "cruiser" | "destroyer") {
                continue;
            }
            // One option per distinguishable kill. Two identical hulls in one system are the
            // same shot written twice, and a sampling decider would draw it twice as often.
            let shape = (
                system.to_string(),
                unit.owner.to_string(),
                unit.type_id.to_string(),
                unit.sustained_damage,
            );
            if !seen.insert(shape) {
                continue;
            }
            targets.push((
                format!("{system}|{index}"),
                format!("destroy {}'s {} in {system}", unit.owner, unit.type_id),
            ));
        }
    }
    let Some(chosen) = pick(
        context,
        player,
        "Lucky Shot: destroy which ship",
        "ship",
        &targets,
    ) else {
        return;
    };
    let Some((system, index)) = chosen.split_once('|').and_then(|(system, index)| {
        Some((
            ti4_model::id::SystemId::new(system),
            index.parse::<usize>().ok()?,
        ))
    }) else {
        return;
    };
    let board = context.state.system_mut(&system);
    if index < board.units.len() {
        board.units.remove(index);
    }
}

/// Rescue: move one of your ships into the active system from anywhere without your token.
///
/// Not a tactical move: no path is traced and no move value is spent, so this ignores movement
/// entirely rather than borrowing rules the card does not ask for. Adjacency is not required
/// either — the card says any system.
fn rescue(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    let Some(destination) = context.state.active_system.clone() else {
        return;
    };
    let types = ti4_content::units::catalogue(context.content, context.sources);
    let origins: Vec<(String, String)> = context
        .state
        .board
        .iter()
        .filter(|(system, _)| **system != destination)
        .filter(|(_, board)| !board.command_tokens.contains(player))
        .filter(|(_, board)| {
            board.units_of(player).into_iter().any(|unit| {
                types
                    .get(unit.type_id.as_str())
                    .is_some_and(ti4_content::units::UnitType::is_ship)
            })
        })
        .map(|(system, _)| (system.to_string(), format!("a ship from {system}")))
        .collect();
    let Some(chosen) = pick(
        context,
        player,
        "Rescue: bring a ship from where",
        "rescue_from",
        &origins,
    ) else {
        return;
    };
    let origin = ti4_model::id::SystemId::new(chosen);
    let taken = {
        let board = context.state.system_mut(&origin);
        let found = board.units.iter().position(|unit| {
            &unit.owner == player
                && types
                    .get(unit.type_id.as_str())
                    .is_some_and(ti4_content::units::UnitType::is_ship)
        });
        found.map(|index| board.units.remove(index))
    };
    if let Some(ship) = taken {
        context.state.system_mut(&destination).units.push(ship);
    }
}

/// The effect registered for a card, if this engine has one.
#[must_use]
#[allow(
    clippy::too_many_lines,
    reason = "one arm per action card, read as a table of what each card does"
)]
pub fn effect_for(alias: &ActionCardId) -> Option<Effect> {
    match alias.as_str() {
        // Four physical copies each, resolved from the printed name rather than listed by hand:
        // the registry test catches a *wrong* alias, but nothing catches a *missing* one, and a
        // fourth copy left off a list stays unplayable for ever with no symptom.
        "mb1" | "mb2" | "mb3" | "mb4" => Some(morale_boost),
        "fs1" | "fs2" | "fs3" | "fs4" => Some(flank_speed),
        "solar_flare" => Some(solar_flare),
        "lost_star" => Some(lost_star),
        "sabo1" | "sabo2" | "sabo3" | "sabo4" => Some(sabotage),
        "veto" | "veto3" | "veto4" => Some(veto),
        "confusing" => Some(confusing),
        "confounding" => Some(confounding),
        "deadly_plot" => Some(deadly_plot),
        "coup" => Some(coup),
        "crisis" => Some(crisis),
        "master_plan" => Some(master_plan),
        "hack" => Some(hack_election),
        "summit" => Some(summit),
        "stability" => Some(political_stability),
        "disgrace" => Some(public_disgrace),
        "puppetsonastring" => Some(puppets_on_a_string),
        "extremeduress" => Some(extreme_duress),
        "salvage" => Some(salvage),
        "reparations" => Some(reparations),
        "infiltrate" => Some(infiltrate),
        "reverse_engineer" => Some(reverse_engineer),
        "blackmarketdealing" => Some(blackmarketdealings),
        "rout" => Some(rout),
        "waylay" => Some(waylay),
        "dh1" | "dh2" | "dh3" | "dh4" => Some(direct_hit),
        "mjets1" | "mjets2" | "mjets3" | "mjets4" => Some(maneuvering_jets),
        "reflective" => Some(reflective),
        "courageous" => Some(courageous),
        "crashlanding" => Some(crashlanding),
        "cripple" => Some(cripple_defenses),
        "f_deployment" => Some(frontline_deployment),
        "investments" => Some(manipulate_investments),
        "lieinwait" => Some(lie_in_wait),
        "f_researched" => Some(focused_research),
        "ghost_ship" => Some(ghost_ship),
        "jamming" => Some(signal_jamming),
        "lucky" => Some(lucky_shot),
        "imp_rider" => Some(imperial_rider),
        "const_rider" => Some(construction_rider),
        "diplo_rider" => Some(diplomacy_rider),
        "lead_rider" => Some(leadership_rider),
        "politic_rider" => Some(politics_rider),
        "tech_rider" => Some(technology_rider),
        "trade_rider" => Some(trade_rider),
        "war_rider" => Some(warfare_rider),
        "sanction" => Some(sanction),
        "assassin" => Some(assassinate_representative),
        "insider" => Some(insider_information),
        "abs" => Some(ancient_burial_sites),
        "dp1" | "dp2" | "dp3" | "dp4" => Some(diplomatic_pressure),
        "insub" => Some(insubordination),
        "meltdown" => Some(reactor_meltdown),
        "messiah" => Some(rise_of_a_messiah),
        "mining_initiative" => Some(mining_initiative),
        "plague" => Some(plague),
        "repeal" => Some(repeal_law),
        "rescue" => Some(rescue),
        "spy" => Some(spy),
        "unexpected" => Some(unexpected_action),
        "tactical" => Some(tactical_bombardment),
        "unstable" => Some(unstable_planet),
        "uprising" => Some(uprising),
        "war_effort" => Some(war_effort),
        "nav_suite" => Some(nav_suite),
        "harness" => Some(harness_energy),
        "economic_initiative" => Some(economic_initiative),
        "industrial_initiative" => Some(industrial_initiative),
        "f_conscription" => Some(fighter_conscription),
        "impersonation" => Some(impersonation),
        "plagiarize" => Some(plagiarize),
        "arch_expedition" => Some(archaeological_expedition),
        "divert_funding" => Some(divert_funding),
        "probe" => Some(exploration_probe),
        "refit" => Some(refit_troops),
        "scuttle" => Some(scuttle),
        "seize" => Some(seize_artifact),
        "exchangeprogram" => Some(exchange_program),
        "mercenarycontract" => Some(mercenary_contract),
        "piratefleet" => Some(pirate_fleet),
        "piratecontract1" | "piratecontract2" | "piratecontract3" | "piratecontract4" => {
            Some(pirate_contract)
        }
        "brilliance" => Some(brilliance),
        "overrule" => Some(overrule),
        "strategize1" | "strategize2" | "strategize3" | "strategize4" => Some(strategize),
        "rally" => Some(rally),
        "fsb" => Some(forward_supply_base),
        "counterstroke" => Some(counterstroke),
        "distinguished" => Some(distinguished_councilor),
        "bribery" => Some(bribery),
        "sh1" | "sh2" | "sh3" | "sh4" => Some(shields_holding),
        "intercept" => Some(intercept),
        "f_prototype" => Some(fighter_prototype),
        "bunker" => Some(bunker),
        "fire_team" => Some(fire_team),
        "scramble" => Some(scramble),
        "war_machine1" | "war_machine2" | "war_machine3" | "war_machine4" => Some(war_machine),
        "blitz" => Some(blitz),
        "disable" => Some(disable),
        "parley" => Some(parley),
        "ghost_squad" => Some(ghost_squad),
        "decoy" => Some(decoy_operation),
        "emergency" => Some(emergency_repairs),
        "upgrade" => Some(upgrade_ship),
        "experimental" => Some(experimental_battlestation),
        "reveal_prototype" => Some(reveal_prototype),
        "s_retreat1" | "s_retreat2" | "s_retreat3" | "s_retreat4" => Some(skilled_retreat),
        "silence_space" => Some(in_the_silence_of_space),
        _ => None,
    }
}

/// Fire Team: "After your ground forces make combat rolls during a round of ground combat:
/// Reroll any number of your dice."
///
/// The window opened at the ground roll staged the roller's dice in
/// `GameState::reroll_staging`; the dice chosen here are re-drawn through the game's roller,
/// and the roll site recomputes the hits from the new faces before anyone is removed.
fn fire_team(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    let Some(set) = context.state.reroll_staging.get(player) else {
        return;
    };
    if set.kind != "ground" {
        return;
    }
    let picks = crate::combat::choose_reroll_dice(
        context.state,
        context.content,
        context.sources,
        context.galaxy,
        context.table,
        player,
    );
    if picks.is_empty() {
        return;
    }
    let set = context
        .state
        .reroll_staging
        .get_mut(player)
        .expect("checked above");
    crate::combat::apply_reroll_dice(context.dice, context.rng, set, &picks, "fire team");
}

/// Scramble Frequency: "After another player makes a BOMBARDMENT, SPACE CANNON, or
/// ANTI-FIGHTER BARRAGE roll: That player rerolls all of their dice."
///
/// Forced, not a choice: every die the roller just made is re-drawn. The reaction guard
/// already excludes the roller themselves; the kind check keeps this from touching a
/// window the card text does not name.
fn scramble(context: &mut crate::timing::TimingContext<'_>, _player: &PlayerId) {
    let Some(roller) = context.state.last_reroll_player.clone() else {
        return;
    };
    let Some(set) = context.state.reroll_staging.get(&roller) else {
        return;
    };
    if !matches!(
        set.kind.as_str(),
        "bombardment" | "space_cannon" | "anti_fighter_barrage"
    ) {
        return;
    }
    let picks: Vec<(usize, usize)> = set
        .rolls
        .iter()
        .enumerate()
        .flat_map(|(unit, entry)| {
            entry
                .faces
                .iter()
                .enumerate()
                .map(move |(die, _)| (unit, die))
        })
        .collect();
    if picks.is_empty() {
        return;
    }
    let set = context
        .state
        .reroll_staging
        .get_mut(&roller)
        .expect("checked above");
    crate::combat::apply_reroll_dice(context.dice, context.rng, set, &picks, "scramble frequency");
}

/// The aliases whose effects are registered in `effect_for`.
const REGISTERED_ALIASES: &[&str] = &[
    "investments",
    "lieinwait",
    "fs1",
    "fs2",
    "fs3",
    "fs4",
    "sabo1",
    "sabo2",
    "sabo3",
    "sabo4",
    "solar_flare",
    "lost_star",
    "veto",
    "veto3",
    "veto4",
    "confusing",
    "confounding",
    "deadly_plot",
    "hack",
    "summit",
    "stability",
    "disgrace",
    "puppetsonastring",
    "extremeduress",
    "salvage",
    "reparations",
    "infiltrate",
    "reverse_engineer",
    "blackmarketdealing",
    "dh1",
    "dh2",
    "dh3",
    "dh4",
    "rout",
    "waylay",
    "mjets1",
    "mjets2",
    "mjets3",
    "mjets4",
    "reflective",
    "courageous",
    "crashlanding",
    "coup",
    "crisis",
    "master_plan",
    "cripple",
    "f_deployment",
    "imp_rider",
    "const_rider",
    "diplo_rider",
    "lead_rider",
    "politic_rider",
    "tech_rider",
    "trade_rider",
    "war_rider",
    "sanction",
    "assassin",
    "insider",
    "abs",
    "dp1",
    "dp2",
    "dp3",
    "dp4",
    "insub",
    "messiah",
    "mining_initiative",
    "mb1",
    "mb2",
    "mb3",
    "mb4",
    "nav_suite",
    "harness",
    "economic_initiative",
    "industrial_initiative",
    "f_conscription",
    "impersonation",
    "plagiarize",
    "arch_expedition",
    "divert_funding",
    "probe",
    "refit",
    "scuttle",
    "seize",
    "exchangeprogram",
    "mercenarycontract",
    "piratefleet",
    "piratecontract1",
    "piratecontract2",
    "piratecontract3",
    "piratecontract4",
    "brilliance",
    "overrule",
    "strategize1",
    "strategize2",
    "strategize3",
    "strategize4",
    "rally",
    "fsb",
    "counterstroke",
    "distinguished",
    "bribery",
    "sh1",
    "sh2",
    "sh3",
    "sh4",
    "intercept",
    "f_prototype",
    "bunker",
    "fire_team",
    "scramble",
    "war_machine1",
    "war_machine2",
    "war_machine3",
    "war_machine4",
    "decoy",
    "emergency",
    "upgrade",
    "experimental",
    "reveal_prototype",
    "s_retreat1",
    "s_retreat2",
    "s_retreat3",
    "s_retreat4",
    "repeal",
    "silence_space",
    "unexpected",
    "ghost_ship",
    "meltdown",
    "plague",
    "spy",
    "unstable",
    "uprising",
    "f_researched",
    "jamming",
    "lucky",
    "rescue",
    "tactical",
    "war_effort",
    "blitz",
    "disable",
    "parley",
    "ghost_squad",
];

/// Aliases with a registered effect.
#[must_use]
pub fn registered_aliases() -> Vec<&'static str> {
    REGISTERED_ALIASES.to_vec()
}

/// This player's ships move one further during `activation`, from Flank Speed.
#[must_use]
pub fn move_bonus(state: &GameState, player: &PlayerId, activation: u32) -> i32 {
    i32::from(
        state
            .player(player)
            .is_some_and(|seat| seat.move_bonus_activation == Some(activation)),
    )
}

/// Cards this engine has no effect for.
///
/// The list is exposed so the gap is queryable rather than implied, in the same way
/// `unregistered_objectives` is.
///
/// It used to return *every* card unconditionally, with a doc comment saying every action card was
/// unimplemented. That was true when written and stopped being true as effects landed, and because
/// nothing consulted [`effect_for`], the coverage it reported never moved: thirty-four implemented
/// aliases were reported missing. A coverage function that cannot improve is worse than none, since
/// it reads as evidence.
#[must_use]
pub fn unimplemented(content: &ContentStore) -> Vec<ActionCardId> {
    content
        .records(ContentType::ActionCards)
        .iter()
        .filter_map(|record| record.text("alias"))
        .map(ActionCardId::new)
        .filter(|alias| effect_for(alias).is_none())
        .collect()
}

#[cfg(test)]
mod tests {
    use crate::choice::Window;

    /// Resolve one card's effect against a state, through a real timing context.
    fn play_effect(state: &mut GameState, alias: &str, player: &PlayerId) {
        let effect = effect_for(&ActionCardId::new(alias)).expect("a registered effect");
        let mut table = crate::choice::Table::new();
        let mut dice = crate::dice::Dice::new();
        let mut rng = crate::rng::GameRng::new(0);
        let mut sequence = crate::event::EventSequence::new();
        let mut context = crate::timing::TimingContext {
            state,
            content: ContentStore::embedded(),
            sources: ti4_model::content_types::POK,
            table: &mut table,
            dice: &mut dice,
            rng: &mut rng,
            event_sequence: &mut sequence,
            galaxy: None,
        };
        effect(&mut context, player);
    }

    #[test]
    fn morale_boost_expires_with_the_round_it_was_played_in() {
        // "During this combat round." Held as the round number rather than a flag, because a
        // flag would improve every later round of the same combat as well.
        let mut state = crate::fixtures::game(&["a"]);
        let player = PlayerId::new("a");
        state.combat_round_seq = 4;

        play_effect(&mut state, "mb1", &player);

        assert_eq!(
            state.player(&player).unwrap().combat_bonus_round,
            Some(4),
            "the bonus belongs to the round it was played in"
        );
        assert_ne!(
            state.player(&player).unwrap().combat_bonus_round,
            Some(5),
            "and not to the next one"
        );
    }

    #[test]
    fn flank_speed_expires_with_the_tactical_action_it_was_played_in() {
        let mut state = crate::fixtures::game(&["a"]);
        let player = PlayerId::new("a");
        state.activation_seq = 3;

        play_effect(&mut state, "fs1", &player);

        assert_eq!(move_bonus(&state, &player, 3), 1, "this activation");
        assert_eq!(move_bonus(&state, &player, 4), 0, "not the next one");
        assert_eq!(
            move_bonus(&state, &PlayerId::new("b"), 3),
            0,
            "and not somebody else's ships"
        );
    }

    #[test]
    fn every_copy_of_a_card_carries_the_same_effect() {
        // Morale Boost is four physical cards and so is Flank Speed. A list written by hand
        // catches a wrong alias but never a missing one, and the only symptom of a fourth copy
        // left off is a slightly lower play rate.
        let content = ContentStore::embedded();
        for name in ["Morale Boost", "Flank Speed", "Skilled Retreat"] {
            let copies: Vec<ActionCardId> = content
                .from_sources(
                    ti4_model::content_types::ContentType::ActionCards,
                    ti4_model::content_types::POK,
                )
                .filter(|record| record.text("name") == Some(name))
                .filter_map(|record| record.text("alias").map(ActionCardId::new))
                .collect();
            assert!(copies.len() > 1, "{name} is printed more than once");
            for alias in copies {
                assert!(
                    effect_for(&alias).is_some(),
                    "{alias} is a copy of {name} and must carry its effect"
                );
            }
        }
    }

    #[test]
    fn a_correct_prediction_is_worth_a_point_and_a_wrong_one_is_not() {
        let mut state = crate::fixtures::game(&["a", "b"]);
        state
            .agenda_predictions
            .insert(PlayerId::new("a"), "for".to_owned());
        state
            .agenda_predictions
            .insert(PlayerId::new("b"), "against".to_owned());

        let paid = resolve_predictions(&mut state, "for");

        assert_eq!(paid, vec![PlayerId::new("a")]);
        assert_eq!(state.player(&PlayerId::new("a")).unwrap().victory_points, 1);
        assert_eq!(state.player(&PlayerId::new("b")).unwrap().victory_points, 0);
    }

    #[test]
    fn predictions_do_not_survive_the_agenda_they_were_made_on() {
        // The card is spent on one agenda. A prediction left behind pays again on the next.
        let mut state = crate::fixtures::game(&["a"]);
        state
            .agenda_predictions
            .insert(PlayerId::new("a"), "for".to_owned());

        resolve_predictions(&mut state, "for");
        assert!(state.agenda_predictions.is_empty());

        let paid_again = resolve_predictions(&mut state, "for");
        assert!(paid_again.is_empty(), "and cannot pay a second time");
        assert_eq!(state.player(&PlayerId::new("a")).unwrap().victory_points, 1);
    }

    #[test]
    fn a_rider_stores_its_outcome_tagged_with_its_own_alias() {
        // The vote order reads the prediction's key; the payoff reads its value. Tagging the
        // value with the alias is what lets one vote pay seven different riders.
        for (alias, tag) in [
            ("const_rider", "const_rider"),
            ("diplo_rider", "diplo_rider"),
            ("lead_rider", "lead_rider"),
            ("politic_rider", "politic_rider"),
            ("tech_rider", "tech_rider"),
            ("trade_rider", "trade_rider"),
            ("war_rider", "war_rider"),
            ("sanction", "sanction"),
        ] {
            let player = PlayerId::new("a");
            let mut state = crate::fixtures::game(&["a", "b"]);
            state.agenda_choices = vec!["FOR".to_owned(), "AGAINST".to_owned()];

            resolve_card(&mut state, alias, &player, &["FOR"]);

            assert_eq!(
                state.agenda_predictions.get(&player).map(String::as_str),
                Some(format!("FOR|{tag}").as_str()),
                "{alias} records its prediction tagged with itself"
            );
        }
    }

    #[test]
    fn a_lone_outcome_is_predicted_without_asking() {
        // One is not a decision: an agenda with a single outcome needs no answer, and the
        // script never sees a question that was never asked.
        let player = PlayerId::new("a");
        let mut state = crate::fixtures::game(&["a", "b"]);
        state.agenda_choices = vec!["AGAINST".to_owned()];

        resolve_card(&mut state, "lead_rider", &player, &[]);

        assert_eq!(
            state.agenda_predictions.get(&player).map(String::as_str),
            Some("AGAINST|lead_rider"),
        );
    }

    #[test]
    fn the_rider_payoffs_pay_what_each_card_promises() {
        let fleet = ti4_model::state::TokenPool::Fleet;
        let mut state = crate::fixtures::game(&["a", "b", "c"]);
        let a_before = state.player(&PlayerId::new("a")).unwrap().tokens(fleet);
        let b_before = state.player(&PlayerId::new("b")).unwrap().trade_goods;
        state
            .agenda_predictions
            .insert(PlayerId::new("a"), "FOR|lead_rider".to_owned());
        state
            .agenda_predictions
            .insert(PlayerId::new("b"), "FOR|trade_rider".to_owned());
        // A correct prediction whose payoff this call site cannot perform: recorded, not paid
        // out in kind, and no state may move for it.
        state
            .agenda_predictions
            .insert(PlayerId::new("c"), "FOR|tech_rider".to_owned());

        let paid = resolve_predictions(&mut state, "FOR");

        assert_eq!(
            paid,
            vec![PlayerId::new("a"), PlayerId::new("b"), PlayerId::new("c")]
        );
        assert_eq!(
            state.player(&PlayerId::new("a")).unwrap().tokens(fleet),
            a_before + 3,
            "three command tokens, on top of what the seat already held"
        );
        assert_eq!(
            state.player(&PlayerId::new("b")).unwrap().trade_goods,
            b_before + 5,
        );
        assert!(state.agenda_predictions.is_empty(), "all spent");
    }

    #[test]
    fn a_wrong_rider_prediction_pays_nothing() {
        let mut state = crate::fixtures::game(&["a"]);
        state
            .agenda_predictions
            .insert(PlayerId::new("a"), "AGAINST|trade_rider".to_owned());

        let paid = resolve_predictions(&mut state, "FOR");

        assert!(paid.is_empty());
        assert_eq!(state.player(&PlayerId::new("a")).unwrap().trade_goods, 0);
        assert!(
            state.agenda_predictions.is_empty(),
            "the card is spent either way"
        );
    }

    #[test]
    fn a_politics_rider_draws_three_and_takes_the_speaker_token() {
        let mut state = crate::fixtures::game(&["a", "b"]);
        state.action_card_deck = vec![
            ActionCardId::new("x1"),
            ActionCardId::new("x2"),
            ActionCardId::new("x3"),
            ActionCardId::new("x4"),
        ];
        state
            .agenda_predictions
            .insert(PlayerId::new("a"), "FOR|politic_rider".to_owned());

        resolve_predictions(&mut state, "FOR");

        let seat = state.player(&PlayerId::new("a")).unwrap();
        assert_eq!(
            seat.action_cards,
            vec![
                ActionCardId::new("x1"),
                ActionCardId::new("x2"),
                ActionCardId::new("x3")
            ]
        );
        assert_eq!(
            state.action_card_deck.len(),
            1,
            "exactly three, not the whole deck"
        );
        assert_eq!(state.speaker, PlayerId::new("a"));
    }

    #[test]
    fn an_assassinated_representative_never_votes_and_never_wins() {
        // The sentinel matches no outcome, so the victim is excluded from the ballot by the
        // key and collects nothing by the value.
        let mut state = crate::fixtures::game(&["a", "b"]);
        state
            .agenda_predictions
            .insert(PlayerId::new("b"), "none|assassin".to_owned());

        for outcome in ["FOR", "AGAINST"] {
            let paid = resolve_predictions(&mut state, outcome);
            assert!(paid.is_empty(), "{outcome} pays the sentinel nothing");
        }

        let player = PlayerId::new("a");
        let mut state = crate::fixtures::game(&["a", "b"]);
        resolve_card(&mut state, "assassin", &player, &["b"]);
        assert_eq!(
            state
                .agenda_predictions
                .get(&PlayerId::new("b"))
                .map(String::as_str),
            Some("none|assassin"),
        );

        // A victim who already predicted keeps the prediction, not the sentinel: a rider in
        // hand is worth more than an assassination, and one prediction per agenda.
        let mut state = crate::fixtures::game(&["a", "b"]);
        state.agenda_choices = vec!["FOR".to_owned(), "AGAINST".to_owned()];
        state
            .agenda_predictions
            .insert(PlayerId::new("b"), "FOR|lead_rider".to_owned());

        resolve_card(&mut state, "assassin", &player, &["b"]);

        assert_eq!(
            state
                .agenda_predictions
                .get(&PlayerId::new("b"))
                .map(String::as_str),
            Some("FOR|lead_rider"),
        );
    }

    #[test]
    fn a_construction_rider_places_a_dock_when_there_is_only_one_choice() {
        // "on a planet you control": one controlled planet forces the placement, and a planet
        // that already holds a dock takes nothing more (the 79.2 cap, so far as this call
        // site can see it).
        let (system, planet) = crate::fixtures::a_placed_planet();
        let player = PlayerId::new("a");
        let mut state = crate::fixtures::game(&["a"]);
        state
            .system_mut(&system)
            .set_control(planet.clone(), player.clone());
        state
            .agenda_predictions
            .insert(player.clone(), "FOR|const_rider".to_owned());

        resolve_predictions(&mut state, "FOR");

        let docked = state
            .system_state(&system)
            .planet_units
            .get(&planet)
            .map_or(0, Vec::len);
        assert_eq!(docked, 1, "one dock on the one planet");

        let mut state = crate::fixtures::game(&["a"]);
        state
            .system_mut(&system)
            .set_control(planet.clone(), player.clone());
        state
            .system_mut(&system)
            .planet_units
            .entry(planet.clone())
            .or_default()
            .push(ti4_model::units::Unit::new(
                ti4_model::id::UnitTypeId::new("spacedock"),
                player.clone(),
            ));
        state
            .agenda_predictions
            .insert(player.clone(), "FOR|const_rider".to_owned());

        resolve_predictions(&mut state, "FOR");

        let docked = state
            .system_state(&system)
            .planet_units
            .get(&planet)
            .map_or(0, Vec::len);
        assert_eq!(docked, 1, "a second dock has nowhere to go");
    }

    #[test]
    fn a_warfare_rider_places_a_dreadnought_where_its_ships_are() {
        // Space holdings are ships (structures sit on planets), so one system holding space
        // units forces the placement; two such systems are a choice this call site cannot ask.
        let (system, _) = crate::fixtures::a_placed_planet();
        let player = PlayerId::new("a");
        let mut state = crate::fixtures::game(&["a"]);
        crate::fixtures::put(&mut state, &system, "cruiser", &player, 1);
        state
            .agenda_predictions
            .insert(player.clone(), "FOR|war_rider".to_owned());

        resolve_predictions(&mut state, "FOR");

        let dreadnoughts = state
            .system_state(&system)
            .units
            .iter()
            .filter(|unit| unit.type_id.as_str() == "dreadnought")
            .count();
        assert_eq!(dreadnoughts, 1);
    }

    #[test]
    fn a_diplomacy_rider_anchors_the_others_to_the_one_system() {
        // "choose 1 system that contains a planet you control": one such system forces it, and
        // every other seat that still holds a command token places one there.
        let (system, planet) = crate::fixtures::a_placed_planet();
        let player = PlayerId::new("a");
        let mut state = crate::fixtures::game(&["a", "b", "c"]);
        state
            .system_mut(&system)
            .set_control(planet.clone(), player.clone());
        let b_before = state
            .player(&PlayerId::new("b"))
            .unwrap()
            .tokens(ti4_model::state::TokenPool::Fleet);
        let c_before = state
            .player(&PlayerId::new("c"))
            .unwrap()
            .tokens(ti4_model::state::TokenPool::Fleet);
        state
            .player_mut(&PlayerId::new("c"))
            .unwrap()
            .spend_token(ti4_model::state::TokenPool::Fleet);
        state
            .agenda_predictions
            .insert(player, "FOR|diplo_rider".to_owned());

        resolve_predictions(&mut state, "FOR");

        let tokens = state.system_state(&system).command_tokens.clone();
        assert!(
            tokens.contains(&PlayerId::new("b")),
            "b has a token to place"
        );
        assert!(
            tokens.contains(&PlayerId::new("c")),
            "c has a token to place"
        );
        assert_eq!(
            state
                .player(&PlayerId::new("b"))
                .unwrap()
                .tokens(ti4_model::state::TokenPool::Fleet),
            b_before - 1,
        );
        assert_eq!(
            state
                .player(&PlayerId::new("c"))
                .unwrap()
                .tokens(ti4_model::state::TokenPool::Fleet),
            c_before - 2,
            "c spent one before the agenda and one more on the rider",
        );
    }

    #[test]
    fn ancient_burial_sites_exhausts_the_cultural_planets_and_only_those() {
        let store = ContentStore::embedded();
        let planets = ti4_content::galaxy::all_planets(store, ti4_model::content_types::POK);
        let cultural = planets
            .iter()
            .find(|(_, planet)| planet.planet_type() == Some("CULTURAL"))
            .expect("the corpus has cultural planets");
        let (cultural_id, cultural_planet) = cultural;
        let barren = planets
            .iter()
            .find(|(_, planet)| planet.planet_type() == Some("INDUSTRIAL"))
            .expect("the corpus has industrial planets");
        let (barren_id, _) = barren;

        let player = PlayerId::new("a");
        let mut state = crate::fixtures::game(&["a", "b"]);
        let cultural_system = ti4_model::id::SystemId::new(
            cultural_planet
                .system_id()
                .expect("a placed planet has a system"),
        );
        let barren_system = ti4_model::id::SystemId::new(
            ti4_content::galaxy::planet(store, barren_id, ti4_model::content_types::POK)
                .expect("in the galaxy")
                .system_id()
                .expect("a placed planet has a system"),
        );
        state
            .system_mut(&cultural_system)
            .set_control(ti4_model::id::PlanetId::new(*cultural_id), player.clone());
        state
            .system_mut(&barren_system)
            .set_control(ti4_model::id::PlanetId::new(*barren_id), player.clone());

        resolve_card(&mut state, "abs", &player, &["a"]);

        assert!(
            state
                .exhausted_planets
                .contains(&ti4_model::id::PlanetId::new(*cultural_id)),
            "the cultural planet is exhausted"
        );
        assert!(
            !state
                .exhausted_planets
                .contains(&ti4_model::id::PlanetId::new(*barren_id)),
            "an industrial planet is not"
        );
    }

    #[test]
    fn diplomatic_pressure_takes_a_note_the_holder_gives() {
        let note = "cf:Solari";
        let me = PlayerId::new("a");
        let victim = PlayerId::new("b");
        let mut state = crate::fixtures::game(&["a", "b"]);
        state
            .promissory_notes
            .insert(note.to_owned(), victim.clone());

        // Setup already dealt b their own notes, so which note to give is a real question and
        // the script answers it. The victim question is never asked: in a two-player game b is
        // the only other player, and one is not a decision.
        resolve_card(&mut state, "dp1", &me, &[note]);

        assert_eq!(
            state.promissory_notes.get(note),
            Some(&me),
            "the note moves to the card player"
        );

        // A player holding nothing to give: the command fizzles, the card is spent anyway.
        let mut state = crate::fixtures::game(&["a", "b"]);
        state.promissory_notes.clear();
        resolve_card(&mut state, "dp1", &me, &[]);
        assert!(state.promissory_notes.is_empty());
    }

    #[test]
    fn insider_information_changes_nothing_and_the_deck_order_is_untouched() {
        // A peek is not a state change: the deck, in the order it was dealt, is as found.
        let player = PlayerId::new("a");
        let mut state = crate::fixtures::game(&["a"]);
        state.agenda_deck = vec!["one".into(), "two".into(), "three".into()];

        resolve_card(&mut state, "insider", &player, &[]);

        assert_eq!(
            state.agenda_deck,
            vec!["one".to_owned(), "two".to_owned(), "three".to_owned()]
        );
    }

    #[test]
    fn rally_places_two_command_tokens_in_the_fleet_pool() {
        let player = PlayerId::new("a");
        let (system, _) = crate::fixtures::a_placed_planet();
        let mut state = crate::fixtures::game(&["a"]);
        state.active_system = Some(system);
        let before = state
            .player(&player)
            .unwrap()
            .tokens(ti4_model::state::TokenPool::Fleet);

        resolve_card(&mut state, "rally", &player, &[]);

        assert_eq!(
            state
                .player(&player)
                .unwrap()
                .tokens(ti4_model::state::TokenPool::Fleet),
            before + 2,
        );
    }

    #[test]
    fn forward_supply_base_pays_three_and_one_other() {
        let me = PlayerId::new("a");
        let other = PlayerId::new("b");
        let mut state = crate::fixtures::game(&["a", "b"]);
        let me_before = state.player(&me).unwrap().trade_goods;
        let other_before = state.player(&other).unwrap().trade_goods;

        // Two players: the other one is the only choice, and one is not a decision.
        resolve_card(&mut state, "fsb", &me, &[]);

        assert_eq!(state.player(&me).unwrap().trade_goods, me_before + 3);
        assert_eq!(state.player(&other).unwrap().trade_goods, other_before + 1);
    }

    #[test]
    fn counterstroke_returns_the_stranded_token_to_the_tactic_pool() {
        let player = PlayerId::new("a");
        let (system, _) = crate::fixtures::a_placed_planet();
        let mut state = crate::fixtures::game(&["a"]);
        state.active_system = Some(system.clone());
        state
            .system_mut(&system)
            .command_tokens
            .insert(player.clone());
        let before = state
            .player(&player)
            .unwrap()
            .tokens(ti4_model::state::TokenPool::Tactic);

        resolve_card(&mut state, "counterstroke", &player, &[]);

        assert!(
            !state.system_state(&system).command_tokens.contains(&player),
            "the token is off the board"
        );
        assert_eq!(
            state
                .player(&player)
                .unwrap()
                .tokens(ti4_model::state::TokenPool::Tactic),
            before + 1,
        );

        // No token of the player's in the activated system: the card fizzles.
        let mut state = crate::fixtures::game(&["a"]);
        state.active_system = Some(system.clone());
        let before = state
            .player(&player)
            .unwrap()
            .tokens(ti4_model::state::TokenPool::Tactic);

        resolve_card(&mut state, "counterstroke", &player, &[]);

        assert_eq!(
            state
                .player(&player)
                .unwrap()
                .tokens(ti4_model::state::TokenPool::Tactic),
            before,
            "nothing to return, nothing moved"
        );
    }

    #[test]
    fn distinguished_councilor_casts_five_extra_votes_for_the_agenda() {
        let a = PlayerId::new("a");
        let b = PlayerId::new("b");
        let mut state = crate::fixtures::game(&["a", "b"]);
        state.agenda_seq = 3;

        resolve_card(&mut state, "distinguished", &a, &[]);

        assert_eq!(crate::vote::extra_votes(&state, &a), 5);
        assert_eq!(
            crate::vote::extra_votes(&state, &b),
            0,
            "the bonus is the holder's, not the table's"
        );

        // A second copy in hand gets a second window: the bonus accumulates, not overwrites.
        resolve_card(&mut state, "distinguished", &a, &[]);
        assert_eq!(crate::vote::extra_votes(&state, &a), 10);

        // The card says "that outcome": the next reveal leaves the bonus worth nothing.
        state.agenda_seq = 4;
        assert_eq!(crate::vote::extra_votes(&state, &a), 0);
    }

    #[test]
    fn bribery_adds_one_vote_per_trade_good_spent() {
        let a = PlayerId::new("a");
        let b = PlayerId::new("b");
        let mut state = crate::fixtures::game(&["a", "b"]);
        state.agenda_seq = 1;
        state.player_mut(&a).unwrap().trade_goods = 3;
        state.player_mut(&b).unwrap().trade_goods = 1;

        resolve_card(&mut state, "bribery", &a, &["2"]);

        assert_eq!(
            state.player(&a).unwrap().trade_goods,
            1,
            "two goods are gone"
        );
        assert_eq!(crate::vote::extra_votes(&state, &a), 2);

        // "Any number" includes zero: the goods stay and no vote is cast.
        resolve_card(&mut state, "bribery", &b, &["0"]);
        assert_eq!(state.player(&b).unwrap().trade_goods, 1);
        assert_eq!(crate::vote::extra_votes(&state, &b), 0);

        // No goods at all: the card asks nothing and buys nothing.
        let c = PlayerId::new("c");
        let mut state = crate::fixtures::game(&["a", "b", "c"]);
        state.agenda_seq = 1;

        resolve_card(&mut state, "bribery", &c, &[]);

        assert_eq!(crate::vote::extra_votes(&state, &c), 0);
    }

    #[test]
    fn shields_holding_grants_two_cancellations_per_copy_for_the_round() {
        let a = PlayerId::new("a");
        let mut state = crate::fixtures::game(&["a"]);
        state.combat_round_seq = 2;

        for alias in ["sh1", "sh2", "sh3", "sh4"] {
            resolve_card(&mut state, alias, &a, &[]);
        }
        assert_eq!(
            crate::combat::cancellable_hits(&state, &a),
            8,
            "four copies cancel eight, stacked"
        );

        // The card names this combat round: the next round starts with an empty pool.
        state.combat_round_seq = 3;
        assert_eq!(crate::combat::cancellable_hits(&state, &a), 0);
    }

    #[test]
    fn intercept_bars_the_opponents_retreat_for_the_round() {
        let a = PlayerId::new("a");
        let b = PlayerId::new("b");
        let system = ti4_model::id::SystemId::new(crate::fixtures::plain_hub().centre.clone());
        let mut state = crate::fixtures::game(&["a", "b"]);
        state.active_system = Some(system.clone());
        state.combat_round_seq = 5;
        crate::fixtures::put(&mut state, &system, "destroyer", &a, 1);
        crate::fixtures::put(&mut state, &system, "cruiser", &b, 1);

        resolve_card(&mut state, "intercept", &a, &[]);

        assert!(
            crate::combat::retreat_barred(&state, &b),
            "the declarant is barred"
        );
        assert!(
            !crate::combat::retreat_barred(&state, &a),
            "the card holder is not"
        );

        // The bar names this combat round: the next round is unbarred.
        state.combat_round_seq = 6;
        assert!(!crate::combat::retreat_barred(&state, &b));
    }

    #[test]
    fn intercept_fizzles_for_a_seat_outside_the_combat() {
        let a = PlayerId::new("a");
        let b = PlayerId::new("b");
        let c = PlayerId::new("c");
        let system = ti4_model::id::SystemId::new(crate::fixtures::plain_hub().centre.clone());
        let mut state = crate::fixtures::game(&["a", "b", "c"]);
        state.active_system = Some(system.clone());
        state.combat_round_seq = 5;
        crate::fixtures::put(&mut state, &system, "destroyer", &a, 1);
        crate::fixtures::put(&mut state, &system, "cruiser", &b, 1);
        // c has no ships in the system: it is not in the combat, so it has no opponent to bar.

        resolve_card(&mut state, "intercept", &c, &[]);

        assert!(!crate::combat::retreat_barred(&state, &a));
        assert!(!crate::combat::retreat_barred(&state, &b));
    }

    #[test]
    fn fighter_prototype_lifts_the_holders_fighter_thresholds_for_the_round() {
        // Fighter I hits on a 9 with one die. The marker moves the threshold down by 2 per
        // copy, for the combat round it was played in - and for fighters only.
        let a = PlayerId::new("a");
        let b = PlayerId::new("b");
        let system = ti4_model::id::SystemId::new(crate::fixtures::plain_hub().centre.clone());
        let store = ContentStore::embedded();
        let mut rng = crate::rng::GameRng::new(1);

        let arena = || {
            let mut state = crate::fixtures::game(&["a", "b"]);
            state.active_system = Some(system.clone());
            state.combat_round_seq = 3;
            crate::fixtures::put(&mut state, &system, "fighter", &a, 1);
            crate::fixtures::put(&mut state, &system, "destroyer", &b, 1);
            state
        };

        // Baseline: a 6 is below the fighter's 9.
        let mut state = arena();
        let mut dice = crate::dice::Dice::from_faces([6]);
        assert_eq!(
            crate::combat::roll_fleet(&state, store, POK, &mut dice, &mut rng, &a, &system),
            0
        );

        // One copy: a 7 now hits the lowered 7 (the same 7 would have been a miss on the
        // printed 9), while a 6 still falls short.
        state.player_mut(&a).unwrap().fighter_bonus_round = vec![3];
        let mut dice = crate::dice::Dice::from_faces([6]);
        assert_eq!(
            crate::combat::roll_fleet(&state, store, POK, &mut dice, &mut rng, &a, &system),
            0
        );
        let mut dice = crate::dice::Dice::from_faces([7]);
        assert_eq!(
            crate::combat::roll_fleet(&state, store, POK, &mut dice, &mut rng, &a, &system),
            1
        );

        // Two copies: the threshold reaches 5, so a 4 misses and a 5 hits.
        state.player_mut(&a).unwrap().fighter_bonus_round = vec![3, 3];
        let mut dice = crate::dice::Dice::from_faces([4]);
        assert_eq!(
            crate::combat::roll_fleet(&state, store, POK, &mut dice, &mut rng, &a, &system),
            0
        );
        let mut dice = crate::dice::Dice::from_faces([5]);
        assert_eq!(
            crate::combat::roll_fleet(&state, store, POK, &mut dice, &mut rng, &a, &system),
            1
        );

        // A marker for round 3 says nothing in round 4: the combat is over for it.
        state.combat_round_seq = 4;
        let mut dice = crate::dice::Dice::from_faces([8]);
        assert_eq!(
            crate::combat::roll_fleet(&state, store, POK, &mut dice, &mut rng, &a, &system),
            0
        );

        // The bonus is fighter-only. A destroyer holding the marker keeps its printed 9, so
        // the same 8 that the boosted fighter turns into a hit stays a miss for it.
        let mut state = arena();
        state.combat_round_seq = 3;
        state.player_mut(&a).unwrap().fighter_bonus_round = vec![3];
        crate::fixtures::put(&mut state, &system, "destroyer", &a, 1);
        let mut dice = crate::dice::Dice::from_faces([8, 8]);
        assert_eq!(
            crate::combat::roll_fleet(&state, store, POK, &mut dice, &mut rng, &a, &system),
            1
        );
        let mut state = arena();
        state.combat_round_seq = 3;
        state.player_mut(&a).unwrap().fighter_bonus_round = vec![3];
        crate::fixtures::put(&mut state, &system, "destroyer", &a, 1);
        let mut dice = crate::dice::Dice::from_faces([7, 8, 6, 6]);
        let pending = crate::combat::anti_fighter_barrage(
            &mut state, store, POK, &mut dice, &mut rng, &system, &a, &b,
        )
        .unwrap();
        assert_eq!(pending, Vec::<(PlayerId, usize)>::new());

        // The card effect stamps the marker for the round it is played in.
        let mut state = arena();
        state.combat_round_seq = 5;
        play_effect(&mut state, "f_prototype", &a);
        assert_eq!(state.player(&a).unwrap().fighter_bonus_round, vec![5]);
    }

    #[test]
    fn bunker_shields_its_planets_from_bombardment_for_the_invasion() {
        // A dreadnought bombards on one die hitting 5. Bunker raises the threshold by 4 per
        // copy for the invasion it was played in, so a face that used to kill does not.
        let (system, planet) = crate::fixtures::a_placed_planet();
        let a = PlayerId::new("a");
        let b = PlayerId::new("b");
        let store = ContentStore::embedded();
        let mut rng = crate::rng::GameRng::new(2);

        let mut arena = |markers: Vec<u32>, face: u32| {
            let mut state = crate::fixtures::game(&["a", "b"]);
            state.active = Some(a.clone());
            state.active_system = Some(system.clone());
            state.activation_seq = 2;
            state
                .system_mut(&system)
                .set_control(planet.clone(), b.clone());
            crate::fixtures::put_on_planet(&mut state, &system, &planet, "infantry", &b, 2);
            crate::fixtures::put(&mut state, &system, "dreadnought", &a, 1);
            state.player_mut(&b).unwrap().bunker_invasion = markers;
            let mut dice = crate::dice::Dice::from_faces([face]);
            let killed = crate::invasion::bombardment(
                &mut state,
                store,
                POK,
                &mut dice,
                &mut rng,
                &mut crate::choice::Table::new(),
                &system,
                &a,
            )
            .expect("single-owner planet: no choice to refuse");
            (killed, state.system_state(&system).on_planet(&planet).len())
        };

        // Face 8: a bare 5 kills one of the two infantry; one Bunker (threshold 9) saves both.
        assert_eq!(arena(vec![], 8), (1usize, 1));
        assert_eq!(arena(vec![2], 8), (0, 2));

        // Two Bunkers raise it to 13: the 10 that one Bunker still lets through stops there.
        assert_eq!(arena(vec![2], 10), (1, 1));
        assert_eq!(arena(vec![2, 2], 10), (0, 2));

        // A marker from a different activation does not reach this one.
        assert_eq!(arena(vec![3], 8), (1, 1));

        // The card effect stamps the activation it was played in.
        let mut state = crate::fixtures::game(&["a", "b"]);
        state.activation_seq = 7;
        play_effect(&mut state, "bunker", &b);
        assert_eq!(state.player(&b).unwrap().bunker_invasion, vec![7]);
    }

    #[test]
    fn war_machine_grows_the_activated_steps_production_budget() {
        // War Machine is +4 of production value and -1 of combined cost, which on the engine's
        // single production budget is five faces of it - for the activation the card was played
        // in, and only for the budget: the machine does not add payment faces.
        //
        // Production in this engine is the PRODUCTION value of a player's own units (a system
        // with no producing unit produces nothing), so the fixture uses a Hel-Titan I, whose
        // value of 1 is independent of any planet, and trade goods that can pay for anything:
        // the budget, not the wallet, is what binds.
        let (system, _planet) = crate::fixtures::a_placed_planet();
        let a = PlayerId::new("a");
        let store = ContentStore::embedded();

        let arena = |markers: Vec<u32>| {
            let mut state = crate::fixtures::game(&["a"]);
            state.activation_seq = 2;
            crate::fixtures::put(&mut state, &system, "titans_pds", &a, 1);
            state.player_mut(&a).unwrap().trade_goods = 10;
            state.player_mut(&a).unwrap().war_machine_use = markers;
            state
        };

        let bare = arena(vec![]);
        assert_eq!(
            crate::production::capacity(&bare, store, POK, &a, &system),
            1
        );
        let boosted = arena(vec![2]);
        assert_eq!(
            crate::production::capacity(&boosted, store, POK, &a, &system),
            6,
            "+4 of value and -1 of cost is five faces",
        );
        // Both halves land: the value half grows the budget, and the cost half is spent from
        // the same wallet, so the resource wallet gains the same five faces. The influence
        // bill is untouched.
        assert_eq!(
            crate::production::available(
                &bare,
                store,
                POK,
                &a,
                crate::production::Spend::Resources
            ),
            10,
        );
        assert_eq!(
            crate::production::available(
                &boosted,
                store,
                POK,
                &a,
                crate::production::Spend::Resources
            ),
            15,
        );
        assert_eq!(
            crate::production::available(
                &bare,
                store,
                POK,
                &a,
                crate::production::Spend::Influence
            ),
            crate::production::available(
                &boosted,
                store,
                POK,
                &a,
                crate::production::Spend::Influence
            ),
        );

        // A marker for another activation buys nothing here.
        assert_eq!(
            crate::production::capacity(&arena(vec![3]), store, POK, &a, &system),
            1,
        );

        // The window: built before the card, refreshed after. The budget grows from 1 to 6,
        // which turns the lone fighter the bare budget afforded into the full pair; the
        // one-off builds it already afforded survive, and the prompt says so.
        let mut state = arena(vec![]);
        let mut window = crate::production::ProductionWindow::new(&state, store, POK, &a, &system);
        let before = window
            .pending_choice(&state, store, POK)
            .expect("one budget face can still buy a fighter");
        assert!(before.prompt.ends_with("(1 left)"), "{}", before.prompt);
        assert!(before.option("build|fighter|1").is_some());
        assert!(before.option("build|fighter|2").is_none());

        state.player_mut(&a).unwrap().war_machine_use = vec![2];
        window.refresh(&state, store, POK);
        let after = window
            .pending_choice(&state, store, POK)
            .expect("the refreshed budget can still produce");
        assert!(after.prompt.ends_with("(6 left)"), "{}", after.prompt);
        assert!(
            after.option("build|fighter|2").is_some(),
            "six faces now afford the full pair",
        );
        assert!(
            after.option("build|destroyer|1").is_some(),
            "a build the budget already afforded is not lost",
        );

        // The card effect stamps the activation it was played in; all four copies are the same
        // effect.
        for alias in [
            "war_machine1",
            "war_machine2",
            "war_machine3",
            "war_machine4",
        ] {
            let mut state = crate::fixtures::game(&["a"]);
            state.activation_seq = 9;
            play_effect(&mut state, alias, &a);
            assert_eq!(
                state.player(&a).unwrap().war_machine_use,
                vec![9],
                "{alias}"
            );
        }
    }

    #[test]
    fn decoy_operation_pulls_units_from_anywhere_to_the_active_planet() {
        let player = PlayerId::new("a");
        let (active_system, landing) = crate::fixtures::a_placed_planet();
        let store = ContentStore::embedded();
        let all = ti4_content::galaxy::all_planets(store, POK);
        let homes = ti4_content::galaxy::home_systems(store, POK);
        // A system that is neither the active one nor a homeworld and that actually has a
        // planet: some plain systems are empty, and the units need somewhere to stand.
        let foreign = crate::fixtures::plain_systems(40)
            .into_iter()
            .find(|system| {
                system.as_str() != active_system.as_str()
                    && !homes.contains(system.as_str())
                    && all
                        .iter()
                        .any(|(_, planet)| planet.system_id() == Some(system.as_str()))
            })
            .expect("a non-active, non-home, non-empty plain system");
        let foreign_planet = all
            .iter()
            .find(|(_, planet)| planet.system_id() == Some(foreign.as_str()))
            .map(|(id, _)| id.to_owned())
            .expect("the system was chosen for having a planet");
        let foreign_planet = ti4_model::id::PlanetId::new(foreign_planet);
        let foreign_system = ti4_model::id::SystemId::new(&foreign);

        let mut state = crate::fixtures::game(&["a"]);
        state.active_system = Some(active_system.clone());
        state
            .system_mut(&active_system)
            .set_control(landing.clone(), player.clone());
        // Two infantry at home, one abroad: three candidates, so the pull is a real choice.
        state
            .system_mut(&active_system)
            .planet_units
            .entry(landing.clone())
            .or_default()
            .extend([
                ti4_model::units::Unit::new(
                    ti4_model::id::UnitTypeId::new("infantry"),
                    player.clone(),
                ),
                ti4_model::units::Unit::new(
                    ti4_model::id::UnitTypeId::new("infantry"),
                    player.clone(),
                ),
            ]);
        state
            .system_mut(&foreign_system)
            .planet_units
            .entry(foreign_planet.clone())
            .or_default()
            .push(ti4_model::units::Unit::new(
                ti4_model::id::UnitTypeId::new("infantry"),
                player.clone(),
            ));

        // The option ids are `system|planet|index`; the script names the far unit first, stops
        // after one, and lands it on the only planet the player controls in the active system.
        let far_id = format!("{foreign}|{foreign_planet}|0");
        resolve_card(&mut state, "decoy", &player, &[&far_id, "stop"]);

        let landed = state
            .system_state(&active_system)
            .planet_units
            .get(&landing)
            .map_or(0, Vec::len);
        assert_eq!(landed, 3, "the pulled infantry landed beside its two");
        let abroad = state
            .system_state(&foreign_system)
            .planet_units
            .get(&foreign_planet)
            .map_or(0, Vec::len);
        assert_eq!(abroad, 0, "and is gone from where it stood");

        // A card with nowhere to land removes nothing: fizzle before the first removal.
        let mut state = crate::fixtures::game(&["a"]);
        state.active_system = Some(active_system.clone());
        state
            .system_mut(&foreign_system)
            .planet_units
            .entry(foreign_planet.clone())
            .or_default()
            .push(ti4_model::units::Unit::new(
                ti4_model::id::UnitTypeId::new("infantry"),
                player.clone(),
            ));

        resolve_card(&mut state, "decoy", &player, &[]);

        assert_eq!(
            state
                .system_state(&foreign_system)
                .planet_units
                .get(&foreign_planet)
                .map_or(0, Vec::len),
            1,
            "the unit stays put"
        );
    }

    #[test]
    fn emergency_repairs_repairs_the_active_system_and_only_its_units() {
        let player = PlayerId::new("a");
        let rival = PlayerId::new("b");
        let (system, _) = crate::fixtures::a_placed_planet();
        let foreign = crate::fixtures::plain_systems(12)
            .into_iter()
            .find(|candidate| candidate.as_str() != system.as_str())
            .expect("more than one plain system");
        let foreign_system = ti4_model::id::SystemId::new(&foreign);

        let mut state = crate::fixtures::game(&["a", "b"]);
        state.active_system = Some(system.clone());
        crate::fixtures::put(&mut state, &system, "cruiser", &player, 1);
        crate::fixtures::put(&mut state, &foreign_system, "cruiser", &player, 1);
        crate::fixtures::put(&mut state, &system, "cruiser", &rival, 1);
        for (where_is, owner) in [
            (&system, &player),
            (&foreign_system, &player),
            (&system, &rival),
        ] {
            state
                .system_mut(where_is)
                .units
                .iter_mut()
                .filter(|unit| &unit.owner == owner)
                .for_each(|unit| unit.sustained_damage = true);
        }

        resolve_card(&mut state, "emergency", &player, &[]);

        let damaged = |state: &GameState, system: &ti4_model::id::SystemId, owner: &PlayerId| {
            state
                .system_state(system)
                .units
                .iter()
                .filter(|unit| &unit.owner == owner)
                .filter(|unit| unit.sustained_damage)
                .count()
        };
        assert_eq!(
            damaged(&state, &system, &player),
            0,
            "the player's ships there are repaired"
        );
        assert_eq!(
            damaged(&state, &foreign_system, &player),
            1,
            "and not the ones elsewhere"
        );
        assert_eq!(
            damaged(&state, &system, &rival),
            1,
            "nor anyone else's ships"
        );
    }

    #[test]
    fn upgrade_swaps_a_cruiser_for_a_dreadnought() {
        let player = PlayerId::new("a");
        let (system, _) = crate::fixtures::a_placed_planet();
        let mut state = crate::fixtures::game(&["a"]);
        state.active_system = Some(system.clone());
        crate::fixtures::put(&mut state, &system, "cruiser", &player, 1);

        // One cruiser is not a decision; the dreadnought comes from the box.
        resolve_card(&mut state, "upgrade", &player, &[]);

        let board = state.system_state(&system);
        assert_eq!(
            board
                .units
                .iter()
                .filter(|unit| unit.type_id.as_str() == "cruiser")
                .count(),
            0,
            "the cruiser is gone"
        );
        assert_eq!(
            board
                .units
                .iter()
                .filter(|unit| unit.type_id.as_str() == "dreadnought")
                .count(),
            1,
            "the dreadnought is there"
        );
    }

    #[test]
    fn the_experimental_battlestation_fires_its_dock() {
        let card_player = PlayerId::new("a");
        let mover = PlayerId::new("b");
        let (system, planet) = crate::fixtures::a_placed_planet();
        let mut state = crate::fixtures::game(&["a", "b"]);
        state.active_system = Some(system.clone());
        state.active = Some(mover.clone());
        state
            .system_mut(&system)
            .set_control(planet.clone(), card_player.clone());
        // The card player's dock stands on the active system, so it is the only candidate.
        state
            .system_mut(&system)
            .planet_units
            .entry(planet.clone())
            .or_default()
            .push(ti4_model::units::Unit::new(
                ti4_model::id::UnitTypeId::new("spacedock"),
                card_player.clone(),
            ));
        // The mover leaves a cruiser in the system to be shot at.
        crate::fixtures::put(&mut state, &system, "cruiser", &mover, 1);

        // Three faces, two of which are five or higher: two hits on one cruiser, which dies.
        let mut dice = crate::dice::Dice::from_faces([5, 9, 3]);
        resolve_card_loaded(
            &mut state,
            "experimental",
            &card_player,
            &[],
            &mut dice,
            None,
        );

        assert!(
            !state
                .system_state(&system)
                .units
                .iter()
                .any(|unit| unit.owner == mover),
            "the cruiser took the hits and is destroyed"
        );

        // The same card, a roll of nothing but misses, destroys nothing.
        let mut state = crate::fixtures::game(&["a", "b"]);
        state.active_system = Some(system.clone());
        state.active = Some(mover.clone());
        state
            .system_mut(&system)
            .set_control(planet.clone(), card_player.clone());
        state
            .system_mut(&system)
            .planet_units
            .entry(planet.clone())
            .or_default()
            .push(ti4_model::units::Unit::new(
                ti4_model::id::UnitTypeId::new("spacedock"),
                card_player.clone(),
            ));
        crate::fixtures::put(&mut state, &system, "cruiser", &mover, 1);

        let mut dice = crate::dice::Dice::from_faces([1, 2, 4]);
        resolve_card_loaded(
            &mut state,
            "experimental",
            &card_player,
            &[],
            &mut dice,
            None,
        );

        assert_eq!(
            state
                .system_state(&system)
                .units
                .iter()
                .filter(|unit| unit.owner == mover)
                .count(),
            1,
            "no hits, no casualties"
        );
    }

    #[test]
    fn reveal_prototype_researches_the_line_in_the_combat() {
        let player = PlayerId::new("a");
        let (system, _) = crate::fixtures::a_placed_planet();
        let store = ContentStore::embedded();
        let mut state = crate::fixtures::game(&["a"]);
        state.active_system = Some(system.clone());
        state.active = Some(player.clone());
        crate::fixtures::put(&mut state, &system, "cruiser", &player, 1);

        // The effect may only offer a technology the player can research now; find one whose
        // line is the cruiser's and drive the card with it. If none is open, the card fizzles
        // and the test says so rather than passing vacuously.
        let open = crate::technology::researchable(&state, store, POK, &player);
        let candidates: Vec<ti4_model::TechnologyId> = open
            .iter()
            .filter(|alias| crate::technology::is_unit_upgrade(store, alias))
            .filter(|alias| {
                let record = store
                    .get(
                        ti4_model::content_types::ContentType::Technologies,
                        alias.as_str(),
                    )
                    .expect("in the corpus");
                let base = record.text("baseUpgrade").unwrap_or_default().to_owned();
                let types = ti4_content::units::catalogue(store, POK);
                if base.is_empty() {
                    let name = record
                        .text("name")
                        .unwrap_or_default()
                        .to_lowercase()
                        .replace(" ii", "")
                        .replace(' ', "_");
                    name == "cruiser" || name.ends_with("_cruiser")
                } else {
                    types
                        .get(base.as_str())
                        .is_some_and(|kind| kind.base_type() == "cruiser")
                }
            })
            .cloned()
            .collect();
        let Some(chosen) = candidates.first() else {
            // No cruiser line is researchable for this faction's start; the card is correct
            // to fizzle, and the fizzle is the assertion.
            state.player_mut(&player).unwrap().trade_goods = 10;
            let before = state.player(&player).map(|seat| seat.technologies.len());
            resolve_card(&mut state, "reveal_prototype", &player, &[]);
            assert_eq!(
                state.player(&player).map(|seat| seat.technologies.len()),
                before,
                "nothing offered, nothing researched"
            );
            return;
        };
        state.player_mut(&player).unwrap().trade_goods = 10;
        let before = state
            .player(&player)
            .map(|seat| seat.technologies.len())
            .unwrap();
        let tg_before = state.player(&player).unwrap().trade_goods;

        resolve_card(&mut state, "reveal_prototype", &player, &[chosen.as_str()]);

        assert!(
            state.player(&player).unwrap().technologies.contains(chosen),
            "the prototype is revealed"
        );
        assert_eq!(
            state.player(&player).unwrap().technologies.len(),
            before + 1,
        );
        assert_eq!(
            state.player(&player).unwrap().trade_goods,
            tg_before - 4,
            "the four resources are the cost"
        );
    }

    /// A card whose printed window makes it a component action.
    fn a_component_action_card() -> ActionCardId {
        ContentStore::embedded()
            .from_sources(
                ti4_model::content_types::ContentType::ActionCards,
                ti4_model::content_types::POK,
            )
            .find(|record| record.text("window") == Some("Action"))
            .and_then(|record| record.text("alias").map(ActionCardId::new))
            .expect("the corpus has component-action cards")
    }

    /// Any action card in the corpus this engine does not model yet.
    ///
    /// The engine is not done: some cards' windows are still unmapped, and while any of them
    /// remains, this is the card a test can use to exercise the "announced unresolved" path.
    /// When the last unported card lands, this expect is the canary that says the premise is
    /// stale.
    fn an_unimplemented_action_card() -> ActionCardId {
        ContentStore::embedded()
            .from_sources(
                ti4_model::content_types::ContentType::ActionCards,
                ti4_model::content_types::DEFAULT,
            )
            .filter_map(|record| record.text("alias").map(ActionCardId::new))
            .find(|alias| effect_for(alias).is_none())
            // Every printed card is implemented now, so a *real* unported card no longer exists.
            // The path this feeds still has to work -- a card with no effect must announce itself
            // unresolved rather than pass as having done something -- and it is reachable by any
            // alias the registry does not know. Falling back to a synthetic one keeps the test
            // about the path instead of about the corpus being incomplete.
            .unwrap_or_else(|| ActionCardId::new("not_a_printed_card"))
    }

    #[test]
    fn only_a_card_that_says_action_is_a_component_action() {
        // 22.1. Offering a reaction card as a turn action would let a player spend a turn on a
        // card whose own text says it is played in somebody else's window.
        let content = ContentStore::embedded();
        assert!(is_component_action(content, &a_component_action_card()));
        assert!(
            !is_component_action(content, &ActionCardId::new("fs1")),
            "Flank Speed is played after an activation, not as a turn"
        );
    }

    #[test]
    fn a_component_action_card_in_hand_is_offered() {
        let content = ContentStore::embedded();
        let card = a_component_action_card();
        let player = PlayerId::new("a");
        let mut state = crate::fixtures::game(&["a"]);
        state.player_mut(&player).unwrap().action_cards =
            vec![ActionCardId::new("fs1"), card.clone()];

        let offered = available_actions(&state, content, POK, None, &player);

        assert_eq!(offered.len(), 1, "one of the two is a turn action");
        assert!(
            offered[0].id.ends_with('1'),
            "offered by hand position, so two copies are two options: {}",
            offered[0].id
        );
    }

    #[test]
    fn two_copies_of_one_card_are_two_options() {
        // Indexed by hand position rather than by alias. Naming the alias would collapse them
        // into one option, which a sampling decider then draws half as often as it should.
        let content = ContentStore::embedded();
        let card = a_component_action_card();
        let player = PlayerId::new("a");
        let mut state = crate::fixtures::game(&["a"]);
        state.player_mut(&player).unwrap().action_cards = vec![card.clone(), card];

        assert_eq!(
            available_actions(&state, content, POK, None, &player).len(),
            2
        );
    }

    #[test]
    fn political_censure_stops_its_owner_playing_action_cards() {
        let content = ContentStore::embedded();
        let player = PlayerId::new("a");
        let mut state = crate::fixtures::game(&["a", "b"]);
        state.player_mut(&player).unwrap().action_cards = vec![a_component_action_card()];
        assert_eq!(
            available_actions(&state, content, POK, None, &player).len(),
            1
        );

        state.enact_law("censure", "a");
        assert!(
            available_actions(&state, content, POK, None, &player).is_empty(),
            "the elected owner cannot play action cards"
        );
        assert_eq!(
            available_actions(&state, content, POK, None, &PlayerId::new("b")).len(),
            0,
            "and b has no cards either way"
        );
    }

    #[test]
    fn playing_a_component_action_spends_the_card() {
        // 22.3: it was genuinely played. Leaving the card in hand would let a bot hold it for
        // ever, and a card whose effect does nothing must spend just like one that works.
        let content = ContentStore::embedded();
        let card = a_component_action_card();
        let player = PlayerId::new("a");
        let mut state = crate::fixtures::game(&["a"]);
        state.player_mut(&player).unwrap().action_cards = vec![card.clone()];

        let mut table = crate::choice::Table::new();
        let mut dice = crate::dice::Dice::new();
        let mut rng = crate::rng::GameRng::new(0);
        let mut sequence = crate::event::EventSequence::new();
        let mut resolver = crate::timing::Resolver::default();
        let option = available_actions(&state, content, POK, None, &player)
            .into_iter()
            .next()
            .expect("it is offered");
        let played = {
            let mut context = crate::timing::TimingContext {
                state: &mut state,
                content,
                sources: ti4_model::content_types::POK,
                table: &mut table,
                dice: &mut dice,
                rng: &mut rng,
                event_sequence: &mut sequence,
                galaxy: None,
            };
            perform(&mut context, &mut resolver, &player, &option).unwrap()
        };

        assert!(played);
        assert!(
            state.player(&player).unwrap().action_cards.is_empty(),
            "the card left the hand"
        );
        assert!(
            resolver
                .log()
                .iter()
                .any(|line| line.contains("ACTION_CARD_PLAYED")),
            "and the play was announced: {:?}",
            resolver.log()
        );
    }

    #[test]
    fn an_unmodelled_card_is_announced_unresolved() {
        // A card with no registered effect is announced unresolved rather than passed off as
        // having done something. That path is what keeps a gap visible on the table.
        let content = ContentStore::embedded();
        let card = an_unimplemented_action_card();
        let player = PlayerId::new("a");
        let mut state = crate::fixtures::game(&["a"]);

        let mut table = crate::choice::Table::new();
        let mut dice = crate::dice::Dice::new();
        let mut rng = crate::rng::GameRng::new(0);
        let mut sequence = crate::event::EventSequence::new();
        let mut resolver = crate::timing::Resolver::default();
        {
            let mut context = crate::timing::TimingContext {
                state: &mut state,
                content,
                sources: ti4_model::content_types::POK,
                table: &mut table,
                dice: &mut dice,
                rng: &mut rng,
                event_sequence: &mut sequence,
                galaxy: None,
            };
            crate::reactions::announce(&mut context, &mut resolver, &player, &card).unwrap();
        }

        assert!(
            resolver
                .log()
                .iter()
                .any(|line| line.contains("ACTION_CARD_UNRESOLVED")),
            "the gap is said out loud: {:?}",
            resolver.log()
        );
    }

    /// Resolve a card's effect with a scripted table, and give back the state it left.
    fn resolve_card(state: &mut GameState, alias: &str, player: &PlayerId, answers: &[&str]) {
        let effect = effect_for(&ActionCardId::new(alias)).expect("a registered effect");
        let mut table = crate::choice::Table::with_default(Box::new(crate::choice::Scripted::new(
            answers.iter().map(|a| (*a).to_owned()),
        )));
        let mut dice = crate::dice::Dice::new();
        let mut rng = crate::rng::GameRng::new(0);
        let mut sequence = crate::event::EventSequence::new();
        let mut context = crate::timing::TimingContext {
            state,
            content: ContentStore::embedded(),
            sources: ti4_model::content_types::POK,
            table: &mut table,
            dice: &mut dice,
            rng: &mut rng,
            event_sequence: &mut sequence,
            galaxy: None,
        };
        effect(&mut context, player);
    }

    /// [`resolve_card`] with the map attached, for cards that read adjacency.
    fn resolve_card_on(
        state: &mut GameState,
        galaxy: &ti4_content::galaxy::Galaxy,
        alias: &str,
        player: &PlayerId,
        answers: &[&str],
    ) {
        let effect = effect_for(&ActionCardId::new(alias)).expect("a registered effect");
        let mut table = crate::choice::Table::with_default(Box::new(crate::choice::Scripted::new(
            answers.iter().map(|a| (*a).to_owned()),
        )));
        let mut dice = crate::dice::Dice::new();
        let mut rng = crate::rng::GameRng::new(0);
        let mut sequence = crate::event::EventSequence::new();
        let mut context = crate::timing::TimingContext {
            state,
            content: ContentStore::embedded(),
            sources: ti4_model::content_types::POK,
            table: &mut table,
            dice: &mut dice,
            rng: &mut rng,
            event_sequence: &mut sequence,
            galaxy: Some(galaxy),
        };
        effect(&mut context, player);
    }

    fn on_planet(state: &GameState, planet: &ti4_model::id::PlanetId) -> usize {
        state
            .board
            .values()
            .filter_map(|board| board.planet_units.get(planet))
            .map(Vec::len)
            .sum()
    }

    #[test]
    fn rise_of_a_messiah_garrisons_every_planet_not_one() {
        let player = PlayerId::new("a");
        let mut state = crate::fixtures::game(&["a"]);
        let planets: Vec<ti4_model::id::PlanetId> = ti4_content::galaxy::all_planets(
            ContentStore::embedded(),
            ti4_model::content_types::POK,
        )
        .into_keys()
        .map(ti4_model::id::PlanetId::new)
        .take(3)
        .collect();
        let (system, _) = crate::fixtures::a_placed_planet();
        for planet in &planets {
            state
                .system_mut(&system)
                .set_control(planet.clone(), player.clone());
        }

        resolve_card(&mut state, "messiah", &player, &[]);

        for planet in &planets {
            assert_eq!(on_planet(&state, planet), 1, "{planet} was garrisoned");
        }
    }

    /// Manipulate Investments places five goods across at least three strategy cards.
    ///
    /// The distinct-card clause is enforced by narrowing the offer, so the test asserts the
    /// *outcome* — five placed, three or more cards touched — rather than that a validator ran.
    #[test]
    fn manipulate_investments_spreads_five_goods_over_three_cards() {
        let player = PlayerId::new("a");
        let mut state = crate::fixtures::game(&["a", "b"]);
        state.strategy_card_goods.clear();

        resolve_card(&mut state, "investments", &player, &[]);

        let placed: i32 = state.strategy_card_goods.values().sum();
        assert_eq!(placed, 5, "five trade goods, no more and no fewer");
        assert!(
            state.strategy_card_goods.len() >= 3,
            "at least three different cards, saw {:?}",
            state.strategy_card_goods
        );
    }

    /// Lie in Wait takes one card from each of two neighbours who traded, and counts a
    /// twice-trading neighbour once.
    #[test]
    fn lie_in_wait_takes_one_card_from_each_trading_neighbour() {
        let hub = crate::fixtures::plain_hub();
        let player = PlayerId::new("a");
        let (b, c) = (PlayerId::new("b"), PlayerId::new("c"));
        let mut state = crate::fixtures::game(&["a", "b", "c"]);

        for (seat, cards) in [(&b, ["mb1", "mb2"]), (&c, ["fs1", "fs2"])] {
            if let Some(held) = state.player_mut(seat) {
                held.action_cards = cards
                    .into_iter()
                    .map(ti4_model::id::ActionCardId::new)
                    .collect();
            }
        }
        // Neighbourship comes from presence on the board (60.1), so all three share the hub's
        // centre. Without units nobody is anybody's neighbour and the card would find nothing --
        // which is what the first version of this test proved.
        let centre = ti4_model::id::SystemId::new(hub.centre.clone());
        for seat in [&player, &b, &c] {
            crate::fixtures::put(&mut state, &centre, "carrier", seat, 1);
        }

        // b trades twice; the card still looks at one hand of b's.
        state.transactions_this_round = vec![(b.clone(), c.clone()), (b.clone(), c.clone())];

        let taken_before = state
            .player(&player)
            .map_or(0, |seat| seat.action_cards.len());
        resolve_card_on(&mut state, &hub.galaxy, "lieinwait", &player, &[]);

        let held = state
            .player(&player)
            .map_or(0, |seat| seat.action_cards.len());
        assert_eq!(
            held,
            taken_before + 2,
            "one card from each of the two neighbours, not one per transaction"
        );
        assert_eq!(
            state.player(&b).map_or(0, |seat| seat.action_cards.len()),
            1,
            "b lost exactly one"
        );
        assert_eq!(
            state.player(&c).map_or(0, |seat| seat.action_cards.len()),
            1,
            "and so did c"
        );
    }

    #[test]
    fn frontline_deployment_puts_three_on_one_planet() {
        let player = PlayerId::new("a");
        let mut state = crate::fixtures::game(&["a"]);
        let (system, planet) = crate::fixtures::a_placed_planet();
        state
            .system_mut(&system)
            .set_control(planet.clone(), player.clone());

        resolve_card(&mut state, "f_deployment", &player, &[]);

        assert_eq!(on_planet(&state, &planet), 3, "three, not one");
    }

    #[test]
    fn mining_initiative_pays_what_the_planet_is_worth() {
        let content = ContentStore::embedded();
        let sources = ti4_model::content_types::POK;
        let player = PlayerId::new("a");
        let mut state = crate::fixtures::game(&["a"]);
        // A planet with resources, so the payment is not trivially zero either way.
        let rich = ti4_content::galaxy::all_planets(content, sources)
            .into_iter()
            .find(|(_, planet)| planet.resources() >= 2)
            .map(|(id, _)| ti4_model::id::PlanetId::new(id))
            .expect("some planet has resources");
        let (system, _) = crate::fixtures::a_placed_planet();
        state
            .system_mut(&system)
            .set_control(rich.clone(), player.clone());
        state.player_mut(&player).unwrap().trade_goods = 0;

        resolve_card(&mut state, "mining_initiative", &player, &[]);

        let worth = crate::production::planet_value(
            content,
            sources,
            &rich,
            crate::production::Spend::Resources,
        );
        assert!(worth >= 2);
        assert_eq!(
            i64::from(state.player(&player).unwrap().trade_goods),
            worth,
            "paid the planet's resource value"
        );
    }

    #[test]
    fn cripple_defenses_destroys_every_pds_on_the_planet() {
        // "Destroy each PDS on that planet" — all of them, not one.
        let player = PlayerId::new("a");
        let mut state = crate::fixtures::game(&["a", "b"]);
        let (system, planet) = crate::fixtures::a_placed_planet();
        crate::fixtures::put_on_planet(&mut state, &system, &planet, "pds", &PlayerId::new("b"), 2);
        crate::fixtures::put_on_planet(
            &mut state,
            &system,
            &planet,
            "infantry",
            &PlayerId::new("b"),
            1,
        );

        resolve_card(&mut state, "cripple", &player, &[]);

        assert_eq!(
            on_planet(&state, &planet),
            1,
            "both PDS went, the infantry stayed"
        );
    }

    #[test]
    fn repeal_law_discards_the_law_it_named() {
        let player = PlayerId::new("a");
        let mut state = crate::fixtures::game(&["a"]);
        state.enact_law("regulations", "for");
        state.enact_law("sanctions", "for");

        resolve_card(&mut state, "repeal", &player, &["sanctions"]);

        assert!(!state.laws.contains_key("sanctions"));
        assert!(state.laws.contains_key("regulations"), "and only that one");
    }

    #[test]
    fn insubordination_takes_a_token_from_a_rival_not_from_you() {
        let player = PlayerId::new("a");
        let mut state = crate::fixtures::game(&["a", "b"]);
        let before = state
            .player(&player)
            .unwrap()
            .tokens(ti4_model::state::TokenPool::Tactic);
        state
            .player_mut(&PlayerId::new("b"))
            .unwrap()
            .gain_token(ti4_model::state::TokenPool::Tactic, 2);
        let theirs = state
            .player(&PlayerId::new("b"))
            .unwrap()
            .tokens(ti4_model::state::TokenPool::Tactic);

        resolve_card(&mut state, "insub", &player, &[]);

        assert_eq!(
            state
                .player(&PlayerId::new("b"))
                .unwrap()
                .tokens(ti4_model::state::TokenPool::Tactic),
            theirs - 1
        );
        assert_eq!(
            state
                .player(&player)
                .unwrap()
                .tokens(ti4_model::state::TokenPool::Tactic),
            before,
            "your own pool is untouched"
        );
    }

    #[test]
    fn unexpected_action_lifts_your_token_off_the_board() {
        let player = PlayerId::new("a");
        let mut state = crate::fixtures::game(&["a"]);
        let (system, _) = crate::fixtures::a_placed_planet();
        state
            .system_mut(&system)
            .command_tokens
            .insert(player.clone());

        resolve_card(&mut state, "unexpected", &player, &[]);

        assert!(
            !state.system_state(&system).command_tokens.contains(&player),
            "the system may be activated again"
        );
    }

    #[test]
    fn war_effort_needs_a_system_you_already_hold() {
        let player = PlayerId::new("a");
        let mut state = crate::fixtures::game(&["a"]);
        let (system, _) = crate::fixtures::a_placed_planet();

        // Nowhere to put it: the card places a cruiser beside your own ships.
        resolve_card(&mut state, "war_effort", &player, &[]);
        assert_eq!(state.system_state(&system).units.len(), 0);

        crate::fixtures::put(&mut state, &system, "carrier", &player, 1);
        resolve_card(&mut state, "war_effort", &player, &[]);
        assert_eq!(
            state.system_state(&system).units.len(),
            2,
            "the cruiser joined the carrier"
        );
    }

    #[test]
    fn a_card_cannot_place_more_plastic_than_the_box_holds() {
        // 31.4 through the shared placement helper, so a new card gets the rule by using the
        // helper rather than by remembering it.
        let player = PlayerId::new("a");
        let mut state = crate::fixtures::game(&["a"]);
        let (system, _) = crate::fixtures::a_placed_planet();
        crate::fixtures::put(&mut state, &system, "carrier", &player, 1);
        crate::fixtures::put(&mut state, &system, "cruiser", &player, 8);

        resolve_card(&mut state, "war_effort", &player, &[]);

        let cruisers = state
            .system_state(&system)
            .units
            .iter()
            .filter(|unit| unit.type_id.as_str() == "cruiser")
            .count();
        assert_eq!(cruisers, 8, "eight cruisers are every cruiser in the box");
    }

    /// Resolve a card with a forced die sequence, for the two that roll.
    fn resolve_with_dice(
        state: &mut GameState,
        alias: &str,
        player: &PlayerId,
        faces: &[u32],
        answers: &[&str],
    ) {
        let effect = effect_for(&ActionCardId::new(alias)).expect("a registered effect");
        let mut table = crate::choice::Table::with_default(Box::new(crate::choice::Scripted::new(
            answers.iter().map(|a| (*a).to_owned()),
        )));
        let mut dice = crate::dice::Dice::from_faces(faces.to_vec());
        let mut rng = crate::rng::GameRng::new(0);
        let mut sequence = crate::event::EventSequence::new();
        let mut context = crate::timing::TimingContext {
            state,
            content: ContentStore::embedded(),
            sources: ti4_model::content_types::POK,
            table: &mut table,
            dice: &mut dice,
            rng: &mut rng,
            event_sequence: &mut sequence,
            galaxy: None,
        };
        effect(&mut context, player);
    }

    /// A planet in a system that is not anybody's home.
    fn a_neutral_spot() -> (ti4_model::id::SystemId, ti4_model::id::PlanetId) {
        let content = ContentStore::embedded();
        let sources = ti4_model::content_types::POK;
        ti4_content::galaxy::all_planets(content, sources)
            .iter()
            .find_map(|(id, planet)| {
                let system = planet.system_id()?;
                (!ti4_content::galaxy::is_home_system(content, system, sources)
                    && !planet.is_placed_during_play())
                .then(|| {
                    (
                        ti4_model::id::SystemId::new(system),
                        ti4_model::id::PlanetId::new(*id),
                    )
                })
            })
            .expect("the corpus has a neutral planet")
    }

    #[test]
    fn reactor_meltdown_takes_one_dock_not_every_dock() {
        let player = PlayerId::new("a");
        let (system, planet) = a_neutral_spot();
        let mut state = crate::fixtures::game(&["a", "b"]);
        crate::fixtures::put_on_planet(
            &mut state,
            &system,
            &planet,
            "spacedock",
            &PlayerId::new("b"),
            2,
        );

        resolve_card(&mut state, "meltdown", &player, &[]);

        assert_eq!(on_planet(&state, &planet), 1, "the card says 1 space dock");
    }

    #[test]
    fn uprising_exhausts_the_planet_and_pays_its_resources() {
        let content = ContentStore::embedded();
        let sources = ti4_model::content_types::POK;
        let player = PlayerId::new("a");
        let (system, planet) = a_neutral_spot();
        let mut state = crate::fixtures::game(&["a", "b"]);
        state
            .system_mut(&system)
            .set_control(planet.clone(), PlayerId::new("b"));
        state.player_mut(&player).unwrap().trade_goods = 0;

        resolve_card(&mut state, "uprising", &player, &[]);

        let worth = crate::production::planet_value(
            content,
            sources,
            &planet,
            crate::production::Spend::Resources,
        );
        assert!(state.exhausted_planets.contains(&planet), "exhausted");
        assert_eq!(
            i64::from(state.player(&player).unwrap().trade_goods),
            worth,
            "and paid its resource value"
        );
    }

    #[test]
    fn uprising_does_not_target_your_own_planet() {
        let player = PlayerId::new("a");
        let (system, planet) = a_neutral_spot();
        let mut state = crate::fixtures::game(&["a", "b"]);
        state
            .system_mut(&system)
            .set_control(planet.clone(), player.clone());
        state.player_mut(&player).unwrap().trade_goods = 0;

        resolve_card(&mut state, "uprising", &player, &[]);

        assert!(!state.exhausted_planets.contains(&planet));
        assert_eq!(state.player(&player).unwrap().trade_goods, 0);
    }

    #[test]
    fn plague_kills_one_infantry_per_six() {
        // One die each, and only a six or better kills. Forced, because a test that took
        // whatever the stream gave would assert on a number it did not choose.
        let player = PlayerId::new("a");
        let (system, planet) = a_neutral_spot();
        let mut state = crate::fixtures::game(&["a", "b"]);
        state
            .system_mut(&system)
            .set_control(planet.clone(), PlayerId::new("b"));
        crate::fixtures::put_on_planet(
            &mut state,
            &system,
            &planet,
            "infantry",
            &PlayerId::new("b"),
            4,
        );

        resolve_with_dice(&mut state, "plague", &player, &[10, 1, 6, 2], &[]);

        assert_eq!(
            on_planet(&state, &planet),
            2,
            "a ten and a six killed one each; a one and a two did not"
        );
    }

    #[test]
    fn plague_on_an_empty_planet_rolls_nothing() {
        let player = PlayerId::new("a");
        let (system, planet) = a_neutral_spot();
        let mut state = crate::fixtures::game(&["a", "b"]);
        state
            .system_mut(&system)
            .set_control(planet.clone(), PlayerId::new("b"));

        resolve_card(&mut state, "plague", &player, &[]);

        assert_eq!(on_planet(&state, &planet), 0);
    }

    #[test]
    fn spy_moves_a_card_from_their_hand_to_yours() {
        let player = PlayerId::new("a");
        let victim = PlayerId::new("b");
        let mut state = crate::fixtures::game(&["a", "b"]);
        state.player_mut(&victim).unwrap().action_cards = (0..3)
            .map(|n| ActionCardId::new(format!("card{n}")))
            .collect();
        state.player_mut(&player).unwrap().action_cards.clear();

        // A two picks the second card, so the theft is not merely "the first one".
        resolve_with_dice(&mut state, "spy", &player, &[2], &[]);

        assert_eq!(state.player(&victim).unwrap().action_cards.len(), 2);
        assert_eq!(
            state.player(&player).unwrap().action_cards,
            vec![ActionCardId::new("card1")],
            "the die chose which"
        );
    }

    #[test]
    fn unstable_planet_destabilises_at_most_three() {
        let content = ContentStore::embedded();
        let sources = ti4_model::content_types::POK;
        let player = PlayerId::new("a");
        let hazardous = ti4_content::galaxy::all_planets(content, sources)
            .iter()
            .find_map(|(id, planet)| {
                let system = planet.system_id()?;
                planet.has_trait("hazardous").then(|| {
                    (
                        ti4_model::id::SystemId::new(system),
                        ti4_model::id::PlanetId::new(*id),
                    )
                })
            });
        let Some((system, planet)) = hazardous else {
            return; // no hazardous planet in this corpus
        };
        let mut state = crate::fixtures::game(&["a", "b"]);
        crate::fixtures::put_on_planet(
            &mut state,
            &system,
            &planet,
            "infantry",
            &PlayerId::new("b"),
            5,
        );

        resolve_card(&mut state, "unstable", &player, &[]);

        assert!(state.exhausted_planets.contains(&planet), "exhausted");
        assert_eq!(on_planet(&state, &planet), 2, "three of the five died");
    }

    #[test]
    fn focused_research_charges_four_and_researches_one() {
        let player = PlayerId::new("a");
        let mut state = crate::fixtures::game(&["a"]);
        state.player_mut(&player).unwrap().trade_goods = 6;
        let before = state.player(&player).unwrap().technologies.len();

        resolve_card(&mut state, "f_researched", &player, &[]);

        assert_eq!(state.player(&player).unwrap().trade_goods, 2, "four spent");
        assert_eq!(
            state.player(&player).unwrap().technologies.len(),
            before + 1,
            "and one gained"
        );
    }

    #[test]
    fn focused_research_charges_nothing_when_it_cannot_pay() {
        // 22.3: a card that cannot resolve does nothing, and must not take the money anyway.
        let player = PlayerId::new("a");
        let mut state = crate::fixtures::game(&["a"]);
        state.player_mut(&player).unwrap().trade_goods = 3;
        let before = state.player(&player).unwrap().technologies.len();

        resolve_card(&mut state, "f_researched", &player, &[]);

        assert_eq!(state.player(&player).unwrap().trade_goods, 3, "untouched");
        assert_eq!(state.player(&player).unwrap().technologies.len(), before);
    }

    #[test]
    fn tactical_bombardment_exhausts_rival_planets_only() {
        let player = PlayerId::new("a");
        let rival = PlayerId::new("b");
        let mut state = crate::fixtures::game(&["a", "b"]);
        // A system with two planets, so "rival planets only" is a distinction this can see.
        // A one-planet system offers no rival target at all and the test would hold either way.
        let system = ti4_content::galaxy::all_systems(
            ContentStore::embedded(),
            ti4_model::content_types::POK,
        )
        .iter()
        .find(|(_, record)| record.planets().len() >= 2)
        .map(|(id, _)| ti4_model::id::SystemId::new(*id))
        .expect("the corpus has a two-planet system");
        let planets: Vec<ti4_model::id::PlanetId> = ti4_content::galaxy::planets_in(
            ContentStore::embedded(),
            system.as_str(),
            ti4_model::content_types::POK,
        )
        .into_iter()
        .map(|planet| ti4_model::id::PlanetId::new(planet.id()))
        .collect();
        let mine = planets[0].clone();
        crate::fixtures::put(&mut state, &system, "cruiser", &player, 1);
        state
            .system_mut(&system)
            .set_control(mine.clone(), player.clone());
        for planet in planets.iter().skip(1) {
            state
                .system_mut(&system)
                .set_control(planet.clone(), rival.clone());
        }

        resolve_card(&mut state, "tactical", &player, &[]);

        assert!(
            !state.exhausted_planets.contains(&mine),
            "your own planet is not bombarded"
        );
        for planet in planets.iter().skip(1) {
            assert!(
                state.exhausted_planets.contains(planet),
                "{planet} exhausted"
            );
        }
    }

    /// A test-only galaxy from a stable set of system ids, leaked once per process.
    fn galaxy_of(systems: &[&str]) -> &'static Galaxy {
        Box::leak(Box::new(
            Galaxy::build(ContentStore::embedded(), systems, POK, 1)
                .expect("one ring holds the tiles"),
        ))
    }

    /// As `resolve_card`, but with an effective galaxy for cards that need board structure.
    fn resolve_card_galaxed(
        state: &mut GameState,
        alias: &str,
        player: &PlayerId,
        answers: &[&str],
        galaxy: Option<&Galaxy>,
    ) {
        let effect = effect_for(&ActionCardId::new(alias)).expect("a registered effect");
        let mut table = crate::choice::Table::with_default(Box::new(crate::choice::Scripted::new(
            answers.iter().map(|a| (*a).to_owned()),
        )));
        let mut dice = crate::dice::Dice::new();
        let mut rng = crate::rng::GameRng::new(0);
        let mut sequence = crate::event::EventSequence::new();
        let mut context = crate::timing::TimingContext {
            state,
            content: ContentStore::embedded(),
            sources: POK,
            table: &mut table,
            dice: &mut dice,
            rng: &mut rng,
            event_sequence: &mut sequence,
            galaxy,
        };
        effect(&mut context, player);
    }

    /// The same as [`resolve_card`], but with a dice the test controls, so an effect that rolls
    /// can be driven into a known branch (e.g. a space cannon that hits, or one that misses).
    fn resolve_card_loaded(
        state: &mut GameState,
        alias: &str,
        player: &PlayerId,
        answers: &[&str],
        dice: &mut crate::dice::Dice,
        galaxy: Option<&Galaxy>,
    ) {
        let effect = effect_for(&ActionCardId::new(alias)).expect("a registered effect");
        let mut table = crate::choice::Table::with_default(Box::new(crate::choice::Scripted::new(
            answers.iter().map(|a| (*a).to_owned()),
        )));
        let mut rng = crate::rng::GameRng::new(0);
        let mut sequence = crate::event::EventSequence::new();
        let mut context = crate::timing::TimingContext {
            state,
            content: ContentStore::embedded(),
            sources: POK,
            table: &mut table,
            dice,
            rng: &mut rng,
            event_sequence: &mut sequence,
            galaxy,
        };
        effect(&mut context, player);
    }

    #[test]
    fn signal_jamming_closes_a_system_to_a_rival() {
        let store = ContentStore::embedded();
        // A non-home planet's system inside an effective galaxy: without a map the oracle offers
        // no jammable systems at all (engine/action_cards.py `_jamming_systems`).
        let planet = crate::fixtures::non_home_planets(1)[0].clone();
        let system_str = ti4_content::galaxy::all_planets(store, POK)
            .get(planet.as_str())
            .and_then(ti4_content::Planet::system_id)
            .expect("a placed planet has a system");
        let galaxy = galaxy_of(&[system_str]);
        let system = ti4_model::id::SystemId::new(system_str);

        let player = PlayerId::new("a");
        let rival = PlayerId::new("b");
        let mut state = crate::fixtures::game(&["a", "b"]);
        crate::fixtures::put(&mut state, &system, "cruiser", &player, 1);

        resolve_card_galaxed(&mut state, "jamming", &player, &[], Some(galaxy));

        assert!(
            state.system_state(&system).command_tokens.contains(&rival),
            "b cannot activate it again (89.1)"
        );
    }

    #[test]
    #[allow(
        clippy::too_many_lines,
        reason = "one scenario walks every oracle rule of the jammable set"
    )]
    fn signal_jamming_offers_ship_adjacent_reach_minus_homes() {
        // Oracle `_jamming_systems`: ships only, in the effective galaxy, adjacency expanded,
        // home systems excluded from the offered set (though they still count for reach).
        let store = ContentStore::embedded();
        // The first dozen systems of the corpus are faction homeworlds, so scan past them.
        let plain = crate::fixtures::plain_systems(60);
        let mut picked: Vec<&str> = Vec::new();
        for system in &plain {
            if !ti4_content::galaxy::is_home_system(store, system, POK) {
                picked.push(system);
            }
            if picked.len() == 2 {
                break;
            }
        }
        assert_eq!(picked.len(), 2, "two non-home plain systems in the corpus");
        let (s0, s1) = (picked[0], picked[1]);
        // s0 is the galaxy centre and s1 its first ring cell: adjacent by construction.
        let galaxy = galaxy_of(&[s0, s1]);
        assert!(
            galaxy.are_adjacent(s0, s1),
            "spiral placement puts them next to each other"
        );

        let player = PlayerId::new("a");
        let mut state = crate::fixtures::game(&["a", "b"]);
        // A ship in s0; the same player's infantry and a rival's cruiser sit in adjacent s1, so
        // only the ship counts for reach.
        crate::fixtures::put(
            &mut state,
            &ti4_model::id::SystemId::new(s0),
            "fighter",
            &player,
            1,
        );
        crate::fixtures::put(
            &mut state,
            &ti4_model::id::SystemId::new(s1),
            "infantry",
            &player,
            1,
        );
        crate::fixtures::put(
            &mut state,
            &ti4_model::id::SystemId::new(s1),
            "cruiser",
            &PlayerId::new("b"),
            1,
        );

        let mut expected = vec![s0.to_owned(), s1.to_owned()];
        expected.sort();
        assert_eq!(
            jamming_systems(&state, store, POK, Some(galaxy), &player),
            expected,
            "reach is ship systems plus their neighbours"
        );

        // No ship in the effective galaxy, or no map at all: nothing can be jammed.
        let bare = crate::fixtures::game(&["a", "b"]);
        assert!(jamming_systems(&bare, store, POK, Some(galaxy), &player).is_empty());
        assert!(jamming_systems(&state, store, POK, None, &player).is_empty());

        // A ship on a homeworld system still counts for reach, but the home is never offered:
        // with only that system in the galaxy the jammable set is empty.
        let home = ti4_content::galaxy::all_planets(store, POK)
            .values()
            .find(|p| p.homeworld_of().is_some())
            .and_then(ti4_content::Planet::system_id)
            .expect("the corpus has a homeworld");
        crate::fixtures::put(
            &mut state,
            &ti4_model::id::SystemId::new(home),
            "fighter",
            &player,
            1,
        );
        assert!(
            jamming_systems(&state, store, POK, Some(galaxy_of(&[home])), &player).is_empty(),
            "the home system is never jammable (88.2)"
        );

        // Eligibility: unplayable without a ship or an opponent, offered with both.
        let alias = ActionCardId::new("jamming");
        assert!(!is_playable(
            &bare,
            store,
            POK,
            Some(galaxy),
            &player,
            &alias
        ));
        let mut solo = crate::fixtures::game(&["a"]);
        crate::fixtures::put(
            &mut solo,
            &ti4_model::id::SystemId::new(s0),
            "fighter",
            &player,
            1,
        );
        assert!(
            !is_playable(&solo, store, POK, Some(galaxy), &player, &alias),
            "one player has nobody to jam"
        );
        state.player_mut(&player).unwrap().action_cards = vec![alias.clone()];
        assert!(is_playable(
            &state,
            store,
            POK,
            Some(galaxy),
            &player,
            &alias
        ));
        let offered = available_actions(&state, store, POK, Some(galaxy), &player);
        assert_eq!(
            offered.len(),
            1,
            "the held jamming card is the one turn action"
        );
    }

    // P1-e: signal jamming victim surface aligned to the oracle
    // (engine/action_cards.py:1059–1064 @ 37061c5).

    /// One recorded option surface: id, kind and label.
    type OptionSurface = (String, String, String);
    /// One recorded choice: prompt plus its offered options in order.
    type RecordedAsk = (String, Vec<OptionSurface>);

    /// A decider that records every choice it is asked to answer, answering from a queue of ids.
    struct JammingRecording {
        wanted: std::collections::VecDeque<String>,
        seen: std::rc::Rc<std::cell::RefCell<Vec<RecordedAsk>>>,
    }

    impl JammingRecording {
        fn new(wanted: &[&str]) -> (Self, std::rc::Rc<std::cell::RefCell<Vec<RecordedAsk>>>) {
            let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
            (
                Self {
                    wanted: wanted.iter().map(|id| (*id).to_owned()).collect(),
                    seen: seen.clone(),
                },
                seen,
            )
        }
    }

    impl crate::choice::Decider for JammingRecording {
        fn choose(
            &mut self,
            choice: &crate::choice::Choice,
        ) -> Result<crate::choice::ChoiceOption, crate::choice::IllegalChoice> {
            let recorded = (
                choice.prompt.clone(),
                choice
                    .options
                    .iter()
                    .map(|option| (option.id.clone(), option.kind.clone(), option.label.clone()))
                    .collect::<Vec<OptionSurface>>(),
            );
            self.seen.borrow_mut().push(recorded);
            let Some(wanted) = self.wanted.pop_front() else {
                return Err(crate::choice::IllegalChoice::ScriptDiverged {
                    player: choice.player.clone(),
                    wanted: "<script exhausted>".to_owned(),
                    offered: choice.ids().into_iter().map(str::to_owned).collect(),
                });
            };
            choice.option(&wanted).cloned().ok_or_else(|| {
                crate::choice::IllegalChoice::ScriptDiverged {
                    player: choice.player.clone(),
                    wanted,
                    offered: choice.ids().into_iter().map(str::to_owned).collect(),
                }
            })
        }
    }

    #[test]
    fn signal_jamming_names_rivals_by_faction() {
        // The victim prompt names the chosen system, and each rival option is their faction name
        // labelled "{faction}'s command token"; Rust presented seat ids under "whose token".
        let player = PlayerId::new("a");
        let rival = PlayerId::new("b");
        // Three seats so the victim pick offers more than one option: a single offered
        // candidate is auto-picked without an ask (the recorded F5 family).
        let mut state = crate::fixtures::game(&["a", "b", "c"]);
        state.player_mut(&player).unwrap().faction = ti4_model::id::FactionId::new("sol");
        state.player_mut(&rival).unwrap().faction = ti4_model::id::FactionId::new("hacan");
        state.player_mut(&PlayerId::new("c")).unwrap().faction =
            ti4_model::id::FactionId::new("jolnar");
        // A non-home planet's system inside an effective galaxy: without a map the oracle offers
        // no jammable systems at all (engine/action_cards.py `_jamming_systems`).
        let store = ContentStore::embedded();
        let planet = crate::fixtures::non_home_planets(1)[0].clone();
        let system_str = ti4_content::galaxy::all_planets(store, POK)
            .get(planet.as_str())
            .and_then(ti4_content::Planet::system_id)
            .expect("a placed planet has a system");
        let galaxy = galaxy_of(&[system_str]);
        let system = ti4_model::id::SystemId::new(system_str);
        crate::fixtures::put(&mut state, &system, "cruiser", &player, 1);

        // The system pick is a single candidate and auto-picks without an ask; the script
        // therefore starts at the victim pick.
        let (recorder, seen) = JammingRecording::new(&["hacan"]);
        let mut table = crate::choice::Table::with_default(Box::new(recorder));
        let mut dice = crate::dice::Dice::new();
        let mut rng = crate::rng::GameRng::new(0);
        let mut sequence = crate::event::EventSequence::new();
        let mut context = crate::timing::TimingContext {
            state: &mut state,
            content: ContentStore::embedded(),
            sources: ti4_model::content_types::POK,
            table: &mut table,
            dice: &mut dice,
            rng: &mut rng,
            event_sequence: &mut sequence,
            galaxy: Some(galaxy),
        };
        signal_jamming(&mut context, &player);

        let asks = seen.borrow();
        assert_eq!(asks.len(), 1, "only the victim pick is asked");
        assert_eq!(
            asks[0].0,
            format!("Signal Jamming: whose token goes into {system}")
        );
        assert_eq!(
            asks[0].1,
            vec![
                (
                    "hacan".to_owned(),
                    "player".to_owned(),
                    "hacan's command token".to_owned()
                ),
                (
                    "jolnar".to_owned(),
                    "player".to_owned(),
                    "jolnar's command token".to_owned()
                )
            ]
        );
        // The chosen name maps back to the seat that plays it.
        assert!(state.system_state(&system).command_tokens.contains(&rival));
    }

    #[test]
    fn lucky_shot_destroys_one_hull_of_the_three_it_names() {
        let player = PlayerId::new("a");
        let rival = PlayerId::new("b");
        let mut state = crate::fixtures::game(&["a", "b"]);
        let (system, planet) = crate::fixtures::a_placed_planet();
        state
            .system_mut(&system)
            .set_control(planet, player.clone());
        // A carrier alone is no target: the card names three hulls and a carrier is not one.
        crate::fixtures::put(&mut state, &system, "carrier", &rival, 1);
        resolve_card(&mut state, "lucky", &player, &[]);
        assert_eq!(
            state.system_state(&system).units.len(),
            1,
            "nothing it names is here, so nothing is destroyed"
        );

        crate::fixtures::put(&mut state, &system, "cruiser", &rival, 1);
        resolve_card(&mut state, "lucky", &player, &[]);

        let left: Vec<String> = state
            .system_state(&system)
            .units
            .iter()
            .map(|unit| unit.type_id.to_string())
            .collect();
        assert_eq!(
            left,
            vec!["carrier".to_owned()],
            "the cruiser went, the carrier stayed"
        );
    }

    #[test]
    fn rescue_ignores_adjacency_and_move_values() {
        // "Any system that does not contain one of your command tokens." No path, no move value.
        let player = PlayerId::new("a");
        let mut state = crate::fixtures::game(&["a"]);
        let hub = crate::fixtures::plain_hub();
        let far = ti4_model::id::SystemId::new(hub.across(&hub.outer[0]));
        let active = ti4_model::id::SystemId::new(hub.outer[0].clone());
        crate::fixtures::put(&mut state, &far, "cruiser", &player, 1);
        state.active_system = Some(active.clone());

        resolve_card(&mut state, "rescue", &player, &[]);

        assert_eq!(state.system_state(&active).units.len(), 1, "it arrived");
        assert_eq!(state.system_state(&far).units.len(), 0, "and left");
    }

    #[test]
    fn rescue_will_not_lift_a_ship_from_a_system_you_have_activated() {
        let player = PlayerId::new("a");
        let mut state = crate::fixtures::game(&["a"]);
        let hub = crate::fixtures::plain_hub();
        let far = ti4_model::id::SystemId::new(hub.across(&hub.outer[0]));
        let active = ti4_model::id::SystemId::new(hub.outer[0].clone());
        crate::fixtures::put(&mut state, &far, "cruiser", &player, 1);
        state.system_mut(&far).command_tokens.insert(player.clone());
        state.active_system = Some(active.clone());

        resolve_card(&mut state, "rescue", &player, &[]);

        assert_eq!(
            state.system_state(&far).units.len(),
            1,
            "your token holds it"
        );
        assert_eq!(state.system_state(&active).units.len(), 0);
    }

    #[test]
    fn the_alias_list_and_the_effect_table_agree() {
        // Two places record the same fact: `effect_for` decides behaviour and
        // `registered_aliases` feeds the coverage ledger. They drifted the first time a card was
        // added — the effect worked and the ledger under-reported by four — so this ties them.
        let content = ContentStore::embedded();
        let listed: std::collections::BTreeSet<&str> = registered_aliases().into_iter().collect();

        for record in content.from_sources(
            ti4_model::content_types::ContentType::ActionCards,
            ti4_model::content_types::POK,
        ) {
            let Some(alias) = record.text("alias") else {
                continue;
            };
            let has_effect = effect_for(&ActionCardId::new(alias)).is_some();
            assert_eq!(
                has_effect,
                listed.contains(alias),
                "{alias}: effect_for says {has_effect}, registered_aliases says {}",
                listed.contains(alias)
            );
        }
    }

    #[test]
    fn every_registered_alias_is_a_real_action_card() {
        for alias in registered_aliases() {
            assert!(
                ContentStore::embedded()
                    .get(ti4_model::content_types::ContentType::ActionCards, alias)
                    .is_some(),
                "{alias} is not an action card the corpus knows"
            );
        }
    }

    use super::*;
    use crate::fixtures::game;

    fn player() -> PlayerId {
        PlayerId::new("a")
    }

    fn hand(state: &GameState) -> Vec<ActionCardId> {
        state.player(&player()).unwrap().action_cards.clone()
    }

    fn set_hand(state: &mut GameState, cards: &[&str]) {
        state.player_mut(&player()).unwrap().action_cards =
            cards.iter().map(|c| ActionCardId::new(*c)).collect();
    }

    /// Two aliases of the same printed card, if the corpus has such a pair.
    fn a_duplicated_card() -> Option<(String, String)> {
        let mut by_name: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for record in ContentStore::embedded().records(ContentType::ActionCards) {
            if let (Some(name), Some(alias)) = (record.text("name"), record.text("alias")) {
                by_name
                    .entry(name.to_owned())
                    .or_default()
                    .push(alias.to_owned());
            }
        }
        by_name
            .into_values()
            .find(|aliases| aliases.len() >= 2)
            .map(|aliases| (aliases[0].clone(), aliases[1].clone()))
    }

    #[test]
    fn drawing_takes_from_the_top() {
        let mut state = game(&["a"]);
        set_hand(&mut state, &[]);
        state.action_card_deck = vec![ActionCardId::new("c1"), ActionCardId::new("c2")];
        let mut table = Table::new();

        let drawn = draw(
            &mut state,
            ContentStore::embedded(),
            &mut table,
            &player(),
            2,
        )
        .unwrap();

        assert_eq!(drawn.len(), 2);
        assert!(state.action_card_deck.is_empty());
        assert_eq!(hand(&state).len(), 2);
    }

    #[test]
    fn an_empty_deck_draws_nothing() {
        // No discard pile is tracked, so inventing a fresh deck would deal out cards that are
        // already in somebody's hand.
        let mut state = game(&["a"]);
        set_hand(&mut state, &[]);
        state.action_card_deck.clear();
        let mut table = Table::new();

        let drawn = draw(
            &mut state,
            ContentStore::embedded(),
            &mut table,
            &player(),
            3,
        )
        .unwrap();

        assert!(drawn.is_empty());
        assert!(hand(&state).is_empty());
    }

    #[test]
    fn sanctions_tightens_the_hand_limit_to_three() {
        // A law that is enacted but not enforced is a list nothing reads; this one bites.
        let mut state = game(&["a"]);
        set_hand(&mut state, &[]);
        state.enact_law("sanctions", "for");
        state.action_card_deck = (0..10)
            .map(|n| ActionCardId::new(format!("c{n}")))
            .collect();
        let mut table = Table::new();

        draw(
            &mut state,
            ContentStore::embedded(),
            &mut table,
            &player(),
            10,
        )
        .unwrap();

        assert_eq!(hand(&state).len(), 3, "not the printed seven");
    }

    #[test]
    fn a_hand_over_seven_discards_down() {
        // 2.4.
        let mut state = game(&["a"]);
        set_hand(&mut state, &[]);
        state.action_card_deck = (0..10)
            .map(|n| ActionCardId::new(format!("c{n}")))
            .collect();
        let mut table = Table::new();

        draw(
            &mut state,
            ContentStore::embedded(),
            &mut table,
            &player(),
            10,
        )
        .unwrap();

        assert_eq!(hand(&state).len(), HAND_LIMIT);
    }

    #[test]
    fn two_copies_of_one_card_are_one_discard_decision() {
        // The alias is not the card. Keying on the alias leaves two copies looking distinct,
        // and the hand offers the same card twice — which is not free, because a sampling
        // decider draws per option and would discard the card it held two of more often.
        let Some((first, second)) = a_duplicated_card() else {
            return;
        };
        let mut state = game(&["a"]);
        set_hand(&mut state, &[&first, &second]);

        let distinct = first_of_each(ContentStore::embedded(), &hand(&state));
        assert_eq!(distinct.len(), 1, "two aliases, one printed card");
    }

    #[test]
    fn distinct_cards_are_offered_separately() {
        let mut state = game(&["a"]);
        let two: Vec<String> = ContentStore::embedded()
            .records(ContentType::ActionCards)
            .iter()
            .filter_map(|record| record.text("alias"))
            .map(ToOwned::to_owned)
            .take(2)
            .collect();
        set_hand(&mut state, &[&two[0], &two[1]]);

        let distinct = first_of_each(ContentStore::embedded(), &hand(&state));
        let names: std::collections::BTreeSet<String> = hand(&state)
            .iter()
            .map(|card| name_of(ContentStore::embedded(), card))
            .collect();
        assert_eq!(distinct.len(), names.len());
    }

    #[test]
    fn discarding_out_of_range_changes_nothing() {
        let mut state = game(&["a"]);
        set_hand(&mut state, &["c1"]);
        assert_eq!(discard(&mut state, &player(), 5), None);
        assert_eq!(hand(&state).len(), 1);
    }

    #[test]
    fn an_unknown_card_falls_back_to_its_alias() {
        // A missing display name is not a reason to lose the card.
        assert_eq!(
            name_of(ContentStore::embedded(), &ActionCardId::new("not_a_card")),
            "not_a_card"
        );
    }

    #[test]
    fn the_unimplemented_action_cards_are_the_ones_with_no_effect() {
        // The invariant, not a size. The previous version asserted only `len() > 50`, which held
        // whether the function consulted `effect_for` or ignored it entirely -- and it ignored it,
        // so thirty-four implemented cards were reported missing and the number never moved.
        let content = ContentStore::embedded();
        let missing: std::collections::BTreeSet<String> = unimplemented(content)
            .into_iter()
            .map(|alias| alias.as_str().to_owned())
            .collect();
        let every: std::collections::BTreeSet<String> = content
            .records(ContentType::ActionCards)
            .iter()
            .filter_map(|record| record.text("alias"))
            .map(std::borrow::ToOwned::to_owned)
            .collect();

        for alias in &every {
            let has_effect = effect_for(&ActionCardId::new(alias.clone())).is_some();
            assert_eq!(
                !missing.contains(alias),
                has_effect,
                "{alias}: reported implemented = {}, has an effect = {has_effect}",
                !missing.contains(alias)
            );
        }
        assert!(
            missing.is_empty(),
            "every printed action card is implemented; still missing: {missing:?}"
        );
    }
    /// Build a `SystemId` from a `&str` or `String` the same way every test below does.
    fn sys(id: &str) -> ti4_model::id::SystemId {
        ti4_model::id::SystemId::new(id)
    }

    fn planet(id: &str) -> ti4_model::id::PlanetId {
        ti4_model::id::PlanetId::new(id)
    }

    /// The system a placed planet sits in.
    fn system_of(store: &ContentStore, planet_id: &str) -> ti4_model::id::SystemId {
        ti4_content::galaxy::planet(store, planet_id, POK)
            .expect("a placed planet")
            .system_id()
            .map(sys)
            .expect("in a system")
    }

    #[test]
    fn harness_replenishes_commodities_to_the_faction_limit() {
        let player = PlayerId::new("a");
        let store = ContentStore::embedded();
        let mut state = crate::fixtures::game(&["a"]);
        state.player_mut(&player).unwrap().faction = ti4_model::id::FactionId::new("arborec");
        state.player_mut(&player).unwrap().commodities = 0;

        resolve_card(&mut state, "harness", &player, &[]);

        let limit = crate::strategy_cards::commodity_limit(&state, store, &player);
        assert!(limit > 0, "the fixture's faction holds commodities");
        assert_eq!(
            state.player(&player).unwrap().commodities,
            limit,
            "commodities are back to the faction's full complement"
        );
    }

    #[test]
    fn economic_initiative_readies_the_cultural_planets() {
        let player = PlayerId::new("a");
        let store = ContentStore::embedded();
        let cultural = ti4_content::galaxy::all_planets(store, POK)
            .iter()
            .find(|(_, p)| {
                p.planet_type()
                    .is_some_and(|kind| kind.eq_ignore_ascii_case("cultural"))
            })
            .map(|(id, _)| id.to_string())
            .expect("the corpus has a cultural planet");
        let planet = planet(&cultural);
        let system = system_of(store, &cultural);

        let mut state = crate::fixtures::game(&["a"]);
        state
            .system_mut(&system)
            .set_control(planet.clone(), player.clone());
        state.exhausted_planets.insert(planet.clone());

        resolve_card(&mut state, "economic_initiative", &player, &[]);

        assert!(
            !state.exhausted_planets.contains(&planet),
            "the cultural planet is ready again"
        );
    }

    #[test]
    fn industrial_initiative_pays_one_good_per_industrial_planet() {
        let player = PlayerId::new("a");
        let store = ContentStore::embedded();
        let industrial: Vec<String> = ti4_content::galaxy::all_planets(store, POK)
            .iter()
            .filter(|(_, p)| {
                p.planet_type()
                    .is_some_and(|kind| kind.eq_ignore_ascii_case("industrial"))
            })
            .map(|(id, _)| id.to_string())
            .take(2)
            .collect();
        let mut state = crate::fixtures::game(&["a"]);
        for planet_id in &industrial {
            let system = system_of(store, planet_id);
            state
                .system_mut(&system)
                .set_control(planet(planet_id), player.clone());
        }
        let before = state.player(&player).unwrap().trade_goods;

        resolve_card(&mut state, "industrial_initiative", &player, &[]);

        assert_eq!(
            state.player(&player).unwrap().trade_goods,
            before + 2,
            "one good per industrial planet"
        );
    }

    #[test]
    fn fighter_conscription_fills_the_systems_that_call_for_it() {
        let player = PlayerId::new("a");
        let rival = PlayerId::new("b");
        let (docked, docked_planet) = crate::fixtures::a_placed_planet();
        let capacity_str = crate::fixtures::plain_systems(12)
            .into_iter()
            .find(|system| system.as_str() != docked.as_str())
            .expect("more than one plain system");
        let capacity = sys(&capacity_str);
        let blocked_str = crate::fixtures::plain_systems(24)
            .into_iter()
            .find(|system| {
                system.as_str() != docked.as_str() && system.as_str() != capacity.as_str()
            })
            .expect("three distinct plain systems");
        let blocked = sys(&blocked_str);

        let mut state = crate::fixtures::game(&["a", "b"]);
        // A dock on the player's planet in `docked`; a carrier (which has capacity) in
        // `capacity`; a rival cruiser in `blocked`.
        state
            .system_mut(&docked)
            .set_control(docked_planet.clone(), player.clone());
        state
            .system_mut(&docked)
            .planet_units
            .entry(docked_planet.clone())
            .or_default()
            .push(ti4_model::units::Unit::new(
                ti4_model::id::UnitTypeId::new("spacedock"),
                player.clone(),
            ));
        state
            .system_mut(&capacity)
            .units
            .push(ti4_model::units::Unit::new(
                ti4_model::id::UnitTypeId::new("carrier"),
                player.clone(),
            ));
        state
            .system_mut(&blocked)
            .units
            .push(ti4_model::units::Unit::new(
                ti4_model::id::UnitTypeId::new("cruiser"),
                rival.clone(),
            ));
        let fighters = |state: &GameState, system: &ti4_model::id::SystemId| {
            state
                .system_state(system)
                .units
                .iter()
                .filter(|unit| unit.type_id.as_str() == "fighter")
                .count()
        };

        resolve_card(&mut state, "f_conscription", &player, &[]);

        assert_eq!(
            fighters(&state, &docked),
            1,
            "the dock's system takes a fighter"
        );
        assert_eq!(
            fighters(&state, &capacity),
            1,
            "the carrier's system takes a fighter"
        );
        assert_eq!(
            fighters(&state, &blocked),
            0,
            "the rival's ship keeps its system fighter-free"
        );
    }

    #[test]
    fn impersonation_spends_influence_and_draws_a_secret() {
        let player = PlayerId::new("a");
        let store = ContentStore::embedded();
        let influence = ti4_content::galaxy::all_planets(store, POK)
            .iter()
            .find(|(_, p)| p.influence() >= 3)
            .map(|(id, _)| id.to_string())
            .expect("the corpus has an influence planet of 3 or more");
        let planet = planet(&influence);
        let system = system_of(store, &influence);

        let mut state = crate::fixtures::game(&["a"]);
        state
            .system_mut(&system)
            .set_control(planet.clone(), player.clone());
        // The influence bill has exactly one source (the planet): no trade goods, no
        // promissory notes, so the payment asks no question.
        state.player_mut(&player).unwrap().trade_goods = 0;
        state.promissory_notes.retain(|_, owner| *owner != player);
        let drawn_before = state.player(&player).map(|s| s.secret_objectives.len());
        let deck_before = state.secret_deck.len();

        resolve_card(&mut state, "impersonation", &player, &[]);

        assert_eq!(
            state.player(&player).map(|s| s.secret_objectives.len()),
            drawn_before.map(|n| n + 1),
            "a secret objective is drawn"
        );
        assert_eq!(state.secret_deck.len(), deck_before.saturating_sub(1),);
    }

    #[test]
    fn plagiarize_steals_the_neighbor_non_faction_technology() {
        let me = PlayerId::new("a");
        let neighbor = PlayerId::new("b");
        let store = ContentStore::embedded();
        let tech = store
            .records(ti4_model::content_types::ContentType::Technologies)
            .iter()
            .find(|record| record.text("faction").is_none() && record.id().is_some())
            .and_then(|record| record.id().map(ti4_model::TechnologyId::new))
            .expect("the corpus has a non-faction technology");

        let mut state = crate::fixtures::game(&["a", "b"]);
        state.player_mut(&me).unwrap().trade_goods = 5;
        state.promissory_notes.retain(|_, owner| *owner != me);
        state
            .player_mut(&neighbor)
            .unwrap()
            .technologies
            .insert(tech.clone());

        resolve_card(&mut state, "plagiarize", &me, &[]);

        assert!(
            state.player(&me).unwrap().technologies.contains(&tech),
            "the technology is gained"
        );
        assert!(
            !state
                .player(&neighbor)
                .unwrap()
                .technologies
                .contains(&tech),
            "and it is gone from the neighbor"
        );
    }

    #[test]
    fn the_archaeological_expedition_reveals_three_and_keeps_the_fragments() {
        let player = PlayerId::new("a");
        let store = ContentStore::embedded();
        let trait_planet = ti4_content::galaxy::all_planets(store, POK)
            .iter()
            .find(|(id, _)| {
                let id: &str = id;

                crate::exploration::trait_of(store, POK, &planet(id)).is_some()
            })
            .map(|(id, _)| id.to_string())
            .expect("the corpus has a planet with an exploration trait");
        let planet = planet(&trait_planet);
        let system = system_of(store, &trait_planet);
        let deck = crate::exploration::trait_of(store, POK, &planet).expect("a deck for the trait");

        let mut state = crate::fixtures::game(&["a"]);
        state
            .system_mut(&system)
            .set_control(planet.clone(), player.clone());
        let deck_before = state
            .exploration_decks
            .get(deck.as_str())
            .map_or(0, Vec::len);
        let fragments_before: i32 = state
            .player(&player)
            .map_or(0, |s| s.relic_fragments.values().sum::<i32>());

        resolve_card(&mut state, "arch_expedition", &player, &[]);

        let deck_after = state
            .exploration_decks
            .get(deck.as_str())
            .map_or(0, Vec::len);
        assert!(
            deck_before.saturating_sub(3) == deck_after || deck_before < 3,
            "three cards are revealed (or the deck ran out)"
        );
        let fragments_after: i32 = state
            .player(&player)
            .map_or(0, |s| s.relic_fragments.values().sum::<i32>());
        assert!(
            fragments_after >= fragments_before,
            "every fragment revealed is kept"
        );
    }

    #[test]
    fn divert_funding_returns_a_technology_and_researches_another() {
        let player = PlayerId::new("a");
        let store = ContentStore::embedded();
        let give_back = ti4_model::TechnologyId::new("amd");
        let mut state = crate::fixtures::game(&["a"]);
        crate::technology::grant(&mut state, &player, &give_back);
        let research = crate::technology::researchable(&state, store, POK, &player)
            .into_iter()
            .find(|tech| *tech != give_back)
            .expect("something else is researchable");

        // A single returnable technology is not a question, so the only answer the card
        // asks is which research to take.
        resolve_card(&mut state, "divert_funding", &player, &[research.as_str()]);

        assert!(
            !state
                .player(&player)
                .unwrap()
                .technologies
                .contains(&give_back),
            "the technology is returned"
        );
        assert!(
            state
                .player(&player)
                .unwrap()
                .technologies
                .contains(&research),
            "the other technology is researched"
        );
    }

    #[test]
    fn the_exploration_probe_explores_a_frontier_token_by_ship() {
        let player = PlayerId::new("a");
        let (system, _) = crate::fixtures::a_placed_planet();
        let mut state = crate::fixtures::game(&["a"]);
        state
            .system_mut(&system)
            .units
            .push(ti4_model::units::Unit::new(
                ti4_model::id::UnitTypeId::new("destroyer"),
                player.clone(),
            ));
        let deck_before = state
            .exploration_decks
            .get(crate::exploration::FRONTIER)
            .map_or(0, Vec::len);
        assert!(deck_before > 0, "the frontier deck is dealt");
        state.frontier_tokens.insert(system.clone());

        resolve_card(&mut state, "probe", &player, &[]);

        assert!(
            !state.frontier_tokens.contains(&system),
            "the token is gone"
        );
        let deck_after = state
            .exploration_decks
            .get(crate::exploration::FRONTIER)
            .map_or(0, Vec::len);
        assert_eq!(deck_before.saturating_sub(1), deck_after);
    }

    #[test]
    fn refit_troops_trades_infantry_for_mechs() {
        let player = PlayerId::new("a");
        let (system, planet) = crate::fixtures::a_placed_planet();
        let infantry = ti4_model::id::UnitTypeId::new("infantry");
        let mut state = crate::fixtures::game(&["a"]);
        state
            .system_mut(&system)
            .set_control(planet.clone(), player.clone());
        let units = state
            .system_mut(&system)
            .planet_units
            .entry(planet.clone())
            .or_default();
        units.push(ti4_model::units::Unit::new(
            infantry.clone(),
            player.clone(),
        ));
        units.push(ti4_model::units::Unit::new(infantry, player.clone()));
        let mechs_before = state
            .system_state(&system)
            .planet_units
            .get(&planet)
            .map_or(0, |units| {
                units
                    .iter()
                    .filter(|unit| unit.type_id.as_str() == "mech")
                    .count()
            });

        // Two infantry: choose the first, stop after one.
        let first = format!("{system}|{planet}|0");
        resolve_card(&mut state, "refit", &player, &[&first, "stop"]);

        let board = state.system_state(&system);
        let units = board.planet_units.get(&planet).expect("the planet");
        let mechs = units
            .iter()
            .filter(|unit| unit.type_id.as_str() == "mech")
            .count();
        let infantry_left = units
            .iter()
            .filter(|unit| unit.type_id.as_str() == "infantry")
            .count();
        assert_eq!(
            mechs,
            mechs_before + 1,
            "one mech replaces the chosen infantry"
        );
        assert_eq!(infantry_left, 1, "the other infantry is untouched");
    }

    #[test]
    fn scuttle_returns_ships_and_pays_their_cost() {
        let player = PlayerId::new("a");
        let store = ContentStore::embedded();
        let (system, _) = crate::fixtures::a_placed_planet();
        let destroyer = ti4_model::id::UnitTypeId::new("destroyer");
        let cruiser = ti4_model::id::UnitTypeId::new("cruiser");
        let types = ti4_content::units::catalogue(store, POK);
        let cost_destroyer = {
            let cost = types
                .get("destroyer")
                .expect("the destroyer")
                .cost()
                .round();
            #[allow(clippy::cast_possible_truncation)]
            let cost_i32 = cost as i32;
            cost_i32
        };
        let mut state = crate::fixtures::game(&["a"]);
        let board = state.system_mut(&system);
        board
            .units
            .push(ti4_model::units::Unit::new(destroyer, player.clone()));
        board
            .units
            .push(ti4_model::units::Unit::new(cruiser, player.clone()));
        let before = state.player(&player).unwrap().trade_goods;

        // Two ships: the first is chosen, then the card stops.
        resolve_card(
            &mut state,
            "scuttle",
            &player,
            &[&format!("{system}|0"), "stop"],
        );

        let left = state
            .system_state(&system)
            .units
            .iter()
            .filter(|unit| {
                unit.type_id.as_str() == "destroyer" || unit.type_id.as_str() == "cruiser"
            })
            .count();
        assert_eq!(left, 1, "exactly one ship was scuttled");
        assert_eq!(
            state.player(&player).unwrap().trade_goods,
            before + cost_destroyer,
            "the destroyer's cost is paid back in goods"
        );
    }

    #[test]
    fn seize_artifact_takes_the_chosen_fragment() {
        let me = PlayerId::new("a");
        let victim = PlayerId::new("b");
        let mut state = crate::fixtures::game(&["a", "b"]);
        state
            .player_mut(&victim)
            .unwrap()
            .relic_fragments
            .insert("four".to_owned(), 2);

        resolve_card(&mut state, "seize", &me, &["b", "four"]);

        assert_eq!(
            state
                .player(&victim)
                .unwrap()
                .relic_fragments
                .get("four")
                .copied()
                .unwrap_or(0),
            1,
            "the victim loses the one fragment"
        );
        assert_eq!(
            state
                .player(&me)
                .unwrap()
                .relic_fragments
                .get("four")
                .copied()
                .unwrap_or(0),
            1,
            "and gains it"
        );
    }

    #[test]
    fn the_exchange_program_places_both_infantry_on_agreement() {
        let me = PlayerId::new("a");
        let other = PlayerId::new("b");
        let (system, planet) = crate::fixtures::a_placed_planet();
        let mut state = crate::fixtures::game(&["a", "b"]);
        state
            .system_mut(&system)
            .set_control(planet.clone(), other.clone());
        state
            .system_mut(&system)
            .planet_units
            .entry(planet.clone())
            .or_default()
            .push(ti4_model::units::Unit::new(
                ti4_model::id::UnitTypeId::new("infantry"),
                other.clone(),
            ));

        // Two players: the who question is not asked; the planet is the only offer; b agrees.
        resolve_card(&mut state, "exchangeprogram", &me, &["yes"]);

        let board = state.system_state(&system);
        let units = board.planet_units.get(&planet).expect("the planet");
        assert_eq!(
            units
                .iter()
                .filter(|unit| unit.type_id.as_str() == "infantry")
                .count(),
            3,
            "the original plus one from each side"
        );
        assert_eq!(
            state
                .system_state(&system)
                .planet_control
                .get(&planet)
                .map(ToString::to_string)
                .as_deref(),
            Some("b"),
            "control does not change hands"
        );
    }

    #[test]
    fn the_exchange_program_costs_a_fleet_token_when_refused() {
        let me = PlayerId::new("a");
        let other = PlayerId::new("b");
        let (system, planet) = crate::fixtures::a_placed_planet();
        let mut state = crate::fixtures::game(&["a", "b"]);
        state
            .system_mut(&system)
            .set_control(planet.clone(), other.clone());
        state
            .system_mut(&system)
            .planet_units
            .entry(planet.clone())
            .or_default()
            .push(ti4_model::units::Unit::new(
                ti4_model::id::UnitTypeId::new("infantry"),
                other.clone(),
            ));
        let fleet = |state: &GameState, player: &PlayerId| {
            state
                .player(player)
                .map(|seat| seat.tokens(ti4_model::state::TokenPool::Fleet))
        };
        let mine_before = fleet(&state, &me).unwrap_or(0);
        let theirs_before = fleet(&state, &other).unwrap_or(0);

        resolve_card(&mut state, "exchangeprogram", &me, &["no"]);

        assert_eq!(fleet(&state, &me), Some(mine_before.saturating_sub(1)));
        assert_eq!(fleet(&state, &other), Some(theirs_before.saturating_sub(1)));
    }

    #[test]
    fn the_mercenary_contract_lands_two_neutral_infantry() {
        let player = PlayerId::new("a");
        let store = ContentStore::embedded();
        let homes = ti4_content::galaxy::home_systems(store, POK);
        let mut state = crate::fixtures::game(&["a"]);
        state.player_mut(&player).unwrap().trade_goods = 5;
        // A non-home planet that is bare on the board, exactly the way the card's own
        // eligibility check sees it.
        let planet_id = ti4_content::galaxy::all_planets(store, POK)
            .iter()
            .find(|(id, pl)| {
                if homes.contains(pl.system_id().unwrap_or("no system")) {
                    return false;
                }
                let system = sys(pl.system_id().unwrap_or("no system"));
                state
                    .system_state(&system)
                    .planet_units
                    .get(&planet(id))
                    .is_none_or(Vec::is_empty)
            })
            .map(|(id, _)| id.to_string())
            .expect("a bare, non-home planet in the map");
        let planet = planet(&planet_id);
        let system = system_of(store, &planet_id);
        // The card only offers planets the board already lists; register this one, empty.
        state
            .system_mut(&system)
            .planet_units
            .entry(planet.clone())
            .or_default();

        resolve_card(
            &mut state,
            "mercenarycontract",
            &player,
            &[&format!("{system}|{planet}")],
        );

        let board = state.system_state(&system);
        let units = board.planet_units.get(&planet).expect("the planet");
        let neutral = crate::neutral_units::NEUTRAL;
        assert_eq!(
            units
                .iter()
                .filter(|unit| {
                    unit.type_id.as_str() == "infantry" && unit.owner.as_str() == neutral
                })
                .count(),
            2,
            "the two neutral infantry are on the planet"
        );
        assert_eq!(
            state.player(&player).unwrap().trade_goods,
            3,
            "the two goods are spent"
        );
    }

    #[test]
    fn the_pirate_fleet_builds_its_crew_in_the_chosen_system() {
        let player = PlayerId::new("a");
        let store = ContentStore::embedded();
        let homes = ti4_content::galaxy::home_systems(store, POK);
        let mut state = crate::fixtures::game(&["a"]);
        state.player_mut(&player).unwrap().trade_goods = 5;
        // The setup board holds only homeworlds; open a plain system for the fleet to enter.
        let target = crate::fixtures::plain_systems(40)
            .into_iter()
            .find(|system| !homes.contains(system.as_str()))
            .map(|system| sys(&system))
            .expect("a plain, non-home system in the map");
        state.system_mut(&target);

        resolve_card(&mut state, "piratefleet", &player, &[&target.to_string()]);

        let board = state.system_state(&target);
        let neutral = crate::neutral_units::NEUTRAL;
        let count = |kind: &str| {
            board
                .units
                .iter()
                .filter(|unit| unit.type_id.as_str() == kind && unit.owner.as_str() == neutral)
                .count()
        };
        assert_eq!(count("carrier"), 1);
        assert_eq!(count("cruiser"), 1);
        assert_eq!(count("destroyer"), 1);
        assert_eq!(count("fighter"), 2);
    }

    #[test]
    fn the_pirate_contract_drops_one_neutral_destroyer() {
        let player = PlayerId::new("a");
        let store = ContentStore::embedded();
        let homes = ti4_content::galaxy::home_systems(store, POK);
        let mut state = crate::fixtures::game(&["a"]);
        let target = crate::fixtures::plain_systems(40)
            .into_iter()
            .find(|system| !homes.contains(system.as_str()))
            .map(|system| sys(&system))
            .expect("a plain, non-home system in the map");
        state.system_mut(&target);

        resolve_card(
            &mut state,
            "piratecontract1",
            &player,
            &[&target.to_string()],
        );

        let board = state.system_state(&target);
        assert_eq!(
            board
                .units
                .iter()
                .filter(|unit| {
                    unit.type_id.as_str() == "destroyer"
                        && unit.owner.as_str() == crate::neutral_units::NEUTRAL
                })
                .count(),
            1
        );
    }

    #[test]
    fn brilliance_takes_the_other_players_breakthrough() {
        let me = PlayerId::new("a");
        let other = PlayerId::new("b");
        let mut state = crate::fixtures::game(&["a", "b"]);
        let breakthrough = ti4_model::BreakthroughId::new("test_breakthrough");
        state.player_mut(&other).unwrap().breakthrough = Some(breakthrough.clone());

        resolve_card(&mut state, "brilliance", &me, &[]);

        assert_eq!(
            state.player(&me).unwrap().breakthrough.as_ref(),
            Some(&breakthrough),
            "the breakthrough is gained"
        );
        assert!(
            state.player(&other).unwrap().breakthrough.is_none(),
            "and is taken from its owner"
        );
    }

    #[test]
    fn overrule_performs_the_chosen_cards_primary_ability() {
        let me = PlayerId::new("a");
        let other = PlayerId::new("b");
        let mut state = crate::fixtures::game(&["a", "b"]);
        state
            .player_mut(&other)
            .unwrap()
            .strategy_cards
            .push(ti4_model::StrategyCardId::new("pok5trade"));
        let goods_before = state.player(&me).unwrap().trade_goods;

        resolve_card(&mut state, "overrule", &me, &["pok5trade"]);

        assert_eq!(
            state.player(&me).unwrap().trade_goods,
            goods_before + 3,
            "the Trade primary pays its three goods to the card player"
        );
        assert!(
            state
                .player(&other)
                .unwrap()
                .strategy_cards
                .contains(&ti4_model::StrategyCardId::new("pok5trade")),
            "the card stays in its owner's hand"
        );
    }

    #[test]
    fn strategize_performs_the_chosen_cards_secondary_ability() {
        let me = PlayerId::new("a");
        let other = PlayerId::new("b");
        let store = ContentStore::embedded();
        let mut state = crate::fixtures::game(&["a", "b"]);
        state
            .player_mut(&other)
            .unwrap()
            .strategy_cards
            .push(ti4_model::StrategyCardId::new("pok5trade"));
        state.player_mut(&me).unwrap().faction = ti4_model::id::FactionId::new("arborec");
        state.player_mut(&me).unwrap().commodities = 0;

        resolve_card(&mut state, "strategize1", &me, &["pok5trade"]);

        let limit = crate::strategy_cards::commodity_limit(&state, store, &me);
        assert!(limit > 0, "the fixture's faction holds commodities");
        assert_eq!(
            state.player(&me).unwrap().commodities,
            limit,
            "the Trade secondary replenishes commodities"
        );
    }
}
