//! The Yssaril Tribes (`yssaril`). See `factions/mod.rs` for the contract and
//! `plans/BASE_FACTIONS_PLAN_2026-10-02.md` for scope; the per-item record is
//! `plans/evidence/BF-yssaril.md`.
//!
//! Card texts (latest printing, `crates/ti4-content/content/*.json`):
//!
//! * Stall Tactics: "ACTION: Discard 1 action card from your hand."
//! * Scheming: "When you draw 1 or more action cards: Draw 1 additional action card. Then, choose
//!   and discard 1 action card from your hand."
//! * Crafty: "You can have any number of action cards in your hand. Game effects cannot prevent
//!   you from using this ability."
//! * Transparasteel Plating (`tp`): "During your turn of the action phase, players that have
//!   passed cannot play action cards."
//! * Mageon Implants (`mi`): "ACTION: Exhaust this card to look at another player's hand of
//!   action cards. Choose 1 of those cards and add it to your hand."
//! * Y'sia Y'ssrila (flagship): "This ship can move through systems that contain other player's
//!   ships."
//! * Blackshade Infiltrator (mech): "DEPLOY: After you use your Stall Tactics faction ability, you
//!   may place 1 mech on a planet you control."
//! * Spy Net (`spynet`): "At the start of your turn: Look at the Yssaril player's hand of action
//!   cards. Choose 1 of those cards and add it to your hand. Then, return this card to the
//!   Yssaril player."
//! * So Ata (`yssarilcommander`): "After another player activates a system that contains your
//!   units: You may look at that player's action cards, promissory notes, or secret objectives."
//!   Unlock: "Have 7 action cards."
//! * Kyver, Blade and Key (`yssarilhero`): "ACTION: Each other player shows you 1 action card from
//!   their hand. For each player, you may either take that card or force that player to discard 3
//!   random action cards from their hand. Then, purge this card." Unlock: "Have 3 scored
//!   objectives."
//!
//! Ssruu copies every other seated faction's agent: the ACTION agents through
//! `leaders::borrowed_component_actions`, the timing agents through each faction's borrowed route
//! (census and acceptance audit in `plans/evidence/BF-SSRUU-SEATED-AGENT-CENSUS.md`).
//! Deepgloom Executable (`yssarilbt`) is implemented: consent, borrowed effects and the fixed-pair
//! transaction resolve through the staged-event flush at the beginning of every game step.
//!
//! **Hidden information.** Every look at another player's hidden cards goes through
//! `hooks_cards::reveal*`, which names the one seat that may see them, and is read back only
//! through that seat's `SeatObservation`. This module keeps no secret bookkeeping of its own, so it
//! writes no `private:<player>:` rows.

use std::sync::Arc;

use ti4_content::ContentStore;
use ti4_model::content_types::{POK, SourceSet};
use ti4_model::id::{ActionCardId, LeaderId, PlayerId, SystemId};
use ti4_model::state::{GameState, Phase};

use super::hooks_cards::{self, CardHooks, RevealKind, RevealScope};
use super::hooks_economy::EconomyHooks;
use super::hooks_movement::{MovementHooks, PassSite};
use super::{FactionModule, Hooks};
use crate::choice::{Choice, ChoiceOption, IllegalChoice, Observed, Table};
use crate::decision_context::{DecisionContext, DecisionSource};
use crate::timing::{Ability, Relation, TimingContext, TimingError};

/// The faction alias; also the faction name in promissory note ids (`spynet:yssaril`).
const FACTION: &str = "yssaril";
/// The Spy Net note's id.
const SPY_NET: &str = "spynet:yssaril";
/// Stall Tactics' component-action option id.
const STALL_TACTICS: &str = "faction|yssaril|stall_tactics";
/// Mageon Implants' component-action option ids start with this; the target seat follows.
const MAGEON_PREFIX: &str = "faction|yssaril|mi|";
/// Deepgloom Executable: another player's Stall Tactics, by leave of the Yssaril seat that follows.
const BT_STALL_PREFIX: &str = "faction|yssaril|bt_stall|";

/// What this faction implements.
pub const MODULE: FactionModule = FactionModule {
    alias: FACTION,
    // `scheming`: every draw in a running game goes through `action_cards::draw` or the status
    // phase's draw hooks, Politics Rider included since the f77a0347 review fix.
    abilities: &["stall_tactics", "crafty", "scheming"],
    technologies: &["tp", "mi"],
    units: &["yssaril_flagship", "yssaril_mech"],
    promissory: &["spynet"],
    leaders: &["yssarilagent", "yssarilcommander", "yssarilhero"],
    breakthroughs: &["yssarilbt"],
    hooks: Hooks {
        component_actions: Some(component_actions),
        perform_component: Some(perform_component),
        commander_unlocked: Some(commander_unlocked),
        leader_action: Some(leader_action),
        use_leader: Some(use_leader),
        timing_abilities: Some(timing_abilities),
        cards: CardHooks {
            transaction_reach: Some(deepgloom_reach),
            transaction_limit_exempt: Some(transaction_limit_exempt),
            ..CardHooks::NONE
        },
        economy: EconomyHooks {
            action_card_draw_requested: Some(action_card_draw_requested),
            action_card_draw_bonus: Some(action_card_draw_bonus),
            action_cards_drawn: Some(action_cards_drawn),
            action_card_limit: Some(action_card_limit),
            action_cards_forbidden: Some(action_cards_forbidden),
            ..EconomyHooks::NONE
        },
        movement: MovementHooks {
            may_move_through_ships: Some(may_move_through_ships),
            ..MovementHooks::NONE
        },
        ..Hooks::NONE
    },
};

// -- small readers -------------------------------------------------------------------------------

fn plays_yssaril(state: &GameState, player: &PlayerId) -> bool {
    state
        .player(player)
        .is_some_and(|seat| seat.faction.as_str() == FACTION)
}

fn hand_size(state: &GameState, player: &PlayerId) -> usize {
    state
        .player(player)
        .map_or(0, |seat| seat.action_cards.len())
}

fn technology_ready(state: &GameState, player: &PlayerId, alias: &str) -> bool {
    crate::technology::technology_text_ready(state, player, alias)
}

fn decision(state: &GameState, player: &PlayerId, source: &str, subtype: &str) -> DecisionContext {
    DecisionContext::new(
        player.clone(),
        DecisionSource::FactionAbility(source.to_owned()),
        subtype,
        state.phase,
        state.round,
    )
}

/// One option per distinct printed card in `hand`, id = the alias of its first copy.
fn distinct_hand_options(content: &ContentStore, hand: &[ActionCardId]) -> Vec<ChoiceOption> {
    let mut seen: Vec<String> = Vec::new();
    let mut options = Vec::new();
    for card in hand {
        let name = crate::action_cards::name_of(content, card);
        if seen.contains(&name) {
            continue;
        }
        options.push(ChoiceOption::labelled(
            card.as_str(),
            crate::action_cards::OWN_HAND_KIND,
            name.clone(),
        ));
        seen.push(name);
    }
    options
}

// -- Crafty --------------------------------------------------------------------------------------

/// Crafty: "You can have any number of action cards in your hand. Game effects cannot prevent you
/// from using this ability." Modules run after the law cap (Sanctions), so this beats it.
fn action_card_limit(
    state: &GameState,
    _content: &ContentStore,
    player: &PlayerId,
    limit: usize,
) -> usize {
    if plays_yssaril(state, player) {
        usize::MAX
    } else {
        limit
    }
}

// -- Scheming ------------------------------------------------------------------------------------

fn scheming_key(player: &PlayerId) -> String {
    format!("private:#borrowed_scheming:{player}")
}

fn action_card_draw_requested(
    state: &mut GameState,
    content: &ContentStore,
    table: &mut Table,
    user: &PlayerId,
    requested: usize,
) -> Result<(), IllegalChoice> {
    if requested == 0 || state.action_card_deck.is_empty() || plays_yssaril(state, user) {
        return Ok(());
    }
    let owners = state
        .players
        .iter()
        .filter(|p| p.id != *user && crate::breakthroughs::holds(state, &p.id, "yssarilbt"))
        .map(|p| p.id.clone())
        .collect::<Vec<_>>();
    for owner in owners {
        let choice = Choice::new(
            owner.clone(),
            format!("Deepgloom Executable: allow {user} to use Scheming"),
            vec![
                ChoiceOption::labelled("allow", "breakthrough", "allow Scheming"),
                ChoiceOption::decline(),
            ],
        )
        .contextualized(decision(
            state,
            &owner,
            "yssarilbt",
            "deepgloom_allow_scheming",
        ));
        let answer = table.ask_seeing(
            &choice,
            &Observed::new(state, content, ti4_model::content_types::DEFAULT, None),
        )?;
        if answer.id == "allow" {
            state
                .faction_marks
                .insert(scheming_key(user), owner.to_string());
            break;
        }
    }
    Ok(())
}

fn stage_deepgloom_transaction(state: &mut GameState, owner: &PlayerId, user: &PlayerId) {
    state
        .faction_marks
        .insert(bt_key("exempt", owner, user), state.turn_seq.to_string());
    let payload = [
        (
            "player".to_owned(),
            serde_json::Value::String(owner.to_string()),
        ),
        (
            "other".to_owned(),
            serde_json::Value::String(user.to_string()),
        ),
    ]
    .into_iter()
    .collect();
    crate::supply::stage_event(state, "DEEPGLOOM_TRANSACTION", &payload);
}

/// Scheming: "Draw 1 additional action card."
fn action_card_draw_bonus(
    state: &GameState,
    _content: &ContentStore,
    player: &PlayerId,
    _requested: usize,
) -> usize {
    usize::from(
        plays_yssaril(state, player) || state.faction_marks.contains_key(&scheming_key(player)),
    )
}

/// Scheming: "Then, choose and discard 1 action card from your hand."
///
/// The hook has no timing context, so it asks the choice itself and stages the discard
/// (`hooks_cards::discard_chosen`); the game announces it after the next component or leader
/// action (see the evidence for the draw sites that do not).
fn action_cards_drawn(
    state: &mut GameState,
    content: &ContentStore,
    table: &mut Table,
    player: &PlayerId,
    drawn: &[ActionCardId],
) -> Result<(), IllegalChoice> {
    let borrowed = state.faction_marks.remove(&scheming_key(player));
    if drawn.is_empty() || (!plays_yssaril(state, player) && borrowed.is_none()) {
        return Ok(());
    }
    let hand: Vec<ActionCardId> = state
        .player(player)
        .map(|seat| seat.action_cards.clone())
        .unwrap_or_default();
    if hand.is_empty() {
        return Ok(());
    }
    let hand_before_draw = hand.len().saturating_sub(drawn.len());
    let mut options = distinct_hand_options(content, &hand);
    let card = if options.len() == 1 {
        ActionCardId::new(options.remove(0).id)
    } else {
        let choice = Choice::new(
            player.clone(),
            "Scheming: choose and discard 1 action card from your hand",
            options,
        )
        .contextualized(decision(state, player, "scheming", "scheming_discard"));
        match table.ask_seeing(&choice, &Observed::new(state, content, POK, None)) {
            Ok(answer) => ActionCardId::new(answer.id),
            Err(error) => {
                // The caller has not committed the draw when this hook fails. Restore the exact
                // cards this invocation appended and put them back on top in their original order.
                if let Some(seat) = state.player_mut(player) {
                    seat.action_cards.truncate(hand_before_draw);
                }
                for card in drawn.iter().rev() {
                    state.action_card_deck.insert(0, card.clone());
                }
                return Err(error);
            }
        }
    };
    hooks_cards::discard_chosen(state, player, &card);
    if let Some(owner) = borrowed {
        stage_deepgloom_transaction(state, &PlayerId::new(owner), player);
    }
    Ok(())
}

// -- Transparasteel Plating ----------------------------------------------------------------------

/// Transparasteel Plating: "During your turn of the action phase, players that have passed cannot
/// play action cards." True for `player` when the active player holds the technology in the action
/// phase and `player` has passed.
fn action_cards_forbidden(state: &GameState, player: &PlayerId) -> bool {
    if state.phase != Phase::Action || !state.player(player).is_some_and(|seat| seat.passed) {
        return false;
    }
    state.active.as_ref().is_some_and(|active| {
        active != player && crate::technology::has_technology_text(state, active, "tp")
    })
}

// -- the flagship --------------------------------------------------------------------------------

/// Y'sia Y'ssrila: "This ship can move through systems that contain other player's ships."
fn may_move_through_ships(
    state: &GameState,
    _content: &ContentStore,
    _sources: SourceSet,
    site: &PassSite<'_>,
) -> bool {
    super::flagship_has_text(state, site.player, site.ship_type, "yssaril_flagship")
}

// -- component actions: Stall Tactics and Mageon Implants ----------------------------------------

/// The seats Mageon Implants may look at: other players holding action cards, in seat order.
fn mageon_targets(state: &GameState, player: &PlayerId) -> Vec<PlayerId> {
    if !technology_ready(state, player, "mi") {
        return Vec::new();
    }
    state
        .players
        .iter()
        .filter(|seat| &seat.id != player && !seat.action_cards.is_empty())
        .map(|seat| seat.id.clone())
        .collect()
}

fn component_actions(
    state: &GameState,
    _content: &ContentStore,
    player: &PlayerId,
) -> Vec<ChoiceOption> {
    let mut options = Vec::new();
    if plays_yssaril(state, player) && hand_size(state, player) > 0 {
        options.push(ChoiceOption::labelled(
            STALL_TACTICS,
            crate::faction_abilities::ACTION_KIND,
            "Stall Tactics: discard 1 action card from your hand",
        ));
    }
    for owner in bt_owners(state, player) {
        options.push(ChoiceOption::labelled(
            format!("{BT_STALL_PREFIX}{owner}"),
            crate::faction_abilities::ACTION_KIND,
            format!("Deepgloom Executable: use {owner}'s Stall Tactics (if {owner} allows)"),
        ));
    }
    for target in mageon_targets(state, player) {
        options.push(ChoiceOption::labelled(
            format!("{MAGEON_PREFIX}{target}"),
            crate::faction_abilities::ACTION_KIND,
            format!("Mageon Implants: exhaust to look at {target}'s action cards and take 1"),
        ));
    }
    options
}

fn perform_component(
    context: &mut TimingContext<'_>,
    player: &PlayerId,
    option: &ChoiceOption,
) -> bool {
    if option.id == STALL_TACTICS {
        return stall_tactics(context, player);
    }
    if let Some(owner) = option.id.strip_prefix(BT_STALL_PREFIX) {
        return borrowed_stall_tactics(context, player, &PlayerId::new(owner));
    }
    if let Some(target) = option.id.strip_prefix(MAGEON_PREFIX) {
        return mageon_implants(context, player, &PlayerId::new(target));
    }
    false
}

/// Stall Tactics, then the mech's DEPLOY: "After you use your Stall Tactics faction ability, you
/// may place 1 mech on a planet you control."
fn stall_tactics(context: &mut TimingContext<'_>, player: &PlayerId) -> bool {
    if !plays_yssaril(context.state, player) {
        return false;
    }
    let Ok(Some(card)) = crate::action_cards::choose_from_own_hand(
        context,
        player,
        "stall_tactics",
        "stall_tactics_discard",
        "Stall Tactics: discard 1 action card from your hand",
        false,
    ) else {
        return false;
    };
    if !hooks_cards::discard_chosen(context.state, player, &card) {
        return false;
    }
    // The discard is done; a decider answering outside the options for the optional placement is
    // treated as declining it, since there is nothing left to roll back.
    let _ = crate::action_cards::place_units_choosing(
        context,
        player,
        "mech",
        1,
        crate::action_cards::PlacementTarget::ControlledPlanet,
        None,
        true,
        "yssaril_mech",
        crate::action_cards::PlacementLimits::Respect,
    );
    true
}

/// Mageon Implants: exhaust, look at another player's action cards, take 1.
fn mageon_implants(context: &mut TimingContext<'_>, player: &PlayerId, target: &PlayerId) -> bool {
    if !mageon_targets(context.state, player).contains(target) {
        return false;
    }
    crate::technology::exhaust_technology_text(context.state, player, "mi");
    // A decider answering outside the options leaves the card exhausted, as the look happened.
    let _ = crate::action_cards::look_at_hand_and_take(context, player, target, "mi", false);
    true
}

// -- Deepgloom Executable -------------------------------------------------------------------------

/// Public, per-turn facts about the breakthrough, in `faction_marks` (they are known to the table,
/// so no `private:` key). Value: the `turn_seq` they hold for.
fn bt_key(kind: &str, owner: &PlayerId, user: &PlayerId) -> String {
    format!("yssaril:bt:{kind}:{owner}:{user}")
}

fn bt_mark(state: &GameState, kind: &str, owner: &PlayerId, user: &PlayerId) -> bool {
    state
        .faction_marks
        .get(&bt_key(kind, owner, user))
        .is_some_and(|turn| *turn == state.turn_seq.to_string())
}

/// The Yssaril seats whose Stall Tactics `user` may ask to use this turn: they hold the
/// breakthrough, have not refused `user` this turn, and `user` has a card to discard.
fn bt_owners(state: &GameState, user: &PlayerId) -> Vec<PlayerId> {
    if hand_size(state, user) == 0 {
        return Vec::new();
    }
    state
        .players
        .iter()
        .filter(|seat| {
            &seat.id != user
                && crate::breakthroughs::holds(state, &seat.id, "yssarilbt")
                && !bt_mark(state, "declined", &seat.id, user)
        })
        .map(|seat| seat.id.clone())
        .collect()
}

/// "You can allow other players to use your STALL TACTICS ...; when you do, you may resolve a
/// transaction with that player. During the action phase, that transaction does not count against
/// the once-per-player transaction limit for that turn."
///
/// The owner is asked; a refusal is remembered for the turn so it is not offered again. Opening
/// the transaction itself is the owner's ordinary transaction (the exemption mark is what the
/// transaction limit reads). The Scheming path asks permission inside `action_cards::draw`; its
/// staged transaction is flushed by `Game::step` before the next game decision.
fn borrowed_stall_tactics(
    context: &mut TimingContext<'_>,
    user: &PlayerId,
    owner: &PlayerId,
) -> bool {
    if !bt_owners(context.state, user).contains(owner) {
        return false;
    }
    let choice = Choice::new(
        owner.clone(),
        format!("Deepgloom Executable: allow {user} to use your Stall Tactics"),
        vec![
            ChoiceOption::labelled("allow", "breakthrough", format!("allow {user}")),
            ChoiceOption::decline(),
        ],
    )
    .contextualized(decision(
        context.state,
        owner,
        "yssarilbt",
        "deepgloom_allow_stall_tactics",
    ));
    let Ok(answer) = context.ask_seeing(&choice) else {
        return false;
    };
    let turn = context.state.turn_seq.to_string();
    if answer.id != "allow" {
        context
            .state
            .faction_marks
            .insert(bt_key("declined", owner, user), turn);
        return false;
    }
    let Ok(Some(card)) = crate::action_cards::choose_from_own_hand(
        context,
        user,
        "yssarilbt",
        "deepgloom_stall_tactics_discard",
        "Stall Tactics (allowed by Deepgloom Executable): discard 1 action card",
        false,
    ) else {
        return false;
    };
    if !hooks_cards::discard_chosen(context.state, user, &card) {
        return false;
    }
    stage_deepgloom_transaction(context.state, owner, user);
    true
}

fn deepgloom_reach(state: &GameState, _: &ContentStore, a: &PlayerId, b: &PlayerId) -> bool {
    state
        .faction_marks
        .contains_key(&bt_key("negotiating", a, b))
        || state
            .faction_marks
            .contains_key(&bt_key("negotiating", b, a))
}

fn deepgloom_transaction(owner_name: &str, seat: &PlayerId) -> Ability {
    let owner = seat.clone();
    let condition = seat.clone();
    Ability::stateful(
        format!("breakthrough:{owner_name}:yssarilbt:DEEPGLOOM_TRANSACTION:after"),
        seat.clone(),
        "DEEPGLOOM_TRANSACTION",
        Relation::After,
        Arc::new(move |event, resolver, context| {
            let Some(user) = event.text("other").map(PlayerId::new) else {
                return Ok(());
            };
            let Some(galaxy) = context.galaxy else {
                context
                    .state
                    .faction_marks
                    .remove(&bt_key("exempt", &owner, &user));
                return Ok(());
            };
            let key = bt_key("negotiating", &owner, &user);
            context
                .state
                .faction_marks
                .insert(key.clone(), "true".to_owned());
            let mut window = crate::transactions::TradeWindow::open_with_content(
                context.state,
                context.content,
                &owner,
                &user,
            );
            while !window.is_complete() {
                let Some(choice) = window.pending_choice(context.state, context.content) else {
                    break;
                };
                // Propagate invalid scripted/user answers through the resolver. The
                // economy hook currently treats timing failures as a failed event,
                // so silently breaking here would make a rejected transaction look
                // like a successful flush to callers.
                let answer = context
                    .ask_seeing(&choice)
                    .map_err(crate::timing::TimingError::IllegalChoice)?;
                let result = window.resolve(context.state, context.content, galaxy, &answer);
                if result == crate::transactions::Traded::Resolved {
                    let payload = crate::transactions::resolved_payload(
                        window.parties().0,
                        window.parties().1,
                    );
                    let event = context
                        .event_sequence
                        .next("TRANSACTION_RESOLVED", payload)
                        .expect("event id");
                    resolver.emit_with_context(context, event, |_, _| {})?;
                }
            }
            context.state.faction_marks.remove(&key);
            context
                .state
                .faction_marks
                .remove(&bt_key("exempt", &owner, &user));
            Ok(())
        }),
    )
    .with_stateful_condition(Arc::new(move |event, _, _| {
        event.text("player") == Some(condition.as_str())
    }))
}

/// A transaction between a Yssaril seat and a player it allowed to use Stall Tactics this turn does
/// not count against the once-per-player limit.
fn transaction_limit_exempt(
    state: &GameState,
    _content: &ContentStore,
    active: &PlayerId,
    other: &PlayerId,
) -> bool {
    state.phase == Phase::Action
        && (bt_mark(state, "exempt", active, other) || bt_mark(state, "exempt", other, active))
}

// -- the commander -------------------------------------------------------------------------------

/// So Ata, unlock: "Have 7 action cards."
fn commander_unlocked(
    state: &GameState,
    _content: &ContentStore,
    _sources: SourceSet,
    _galaxy: Option<&ti4_content::galaxy::Galaxy>,
    player: &PlayerId,
    leader: &LeaderId,
) -> Option<bool> {
    (leader.as_str() == "yssarilcommander").then(|| hand_size(state, player) >= 7)
}

/// The hidden promissory notes `owner` holds (faceup notes are public).
fn hidden_notes(state: &GameState, owner: &PlayerId) -> Vec<String> {
    crate::promissory::held_by(state, owner)
        .into_iter()
        .filter(|note| !state.promissory_faceup.contains(note))
        .collect()
}

/// What So Ata could look at on `owner`: the kinds with something in them.
fn lookable(state: &GameState, owner: &PlayerId) -> Vec<RevealKind> {
    let mut kinds = Vec::new();
    if hand_size(state, owner) > 0 {
        kinds.push(RevealKind::ActionCards);
    }
    if !hidden_notes(state, owner).is_empty() {
        kinds.push(RevealKind::PromissoryNotes);
    }
    if state
        .player(owner)
        .is_some_and(|seat| !seat.secret_objectives.is_empty())
    {
        kinds.push(RevealKind::SecretObjectives);
    }
    kinds
}

/// The player whose activation of a system with the seat's units opens So Ata's window, if the
/// seat has the commander's ability and there is something to look at.
fn commander_target(
    state: &GameState,
    seat: &PlayerId,
    event: &crate::event::Event,
) -> Option<(PlayerId, Vec<RevealKind>)> {
    if !crate::promissory::has_commander_ability(state, seat, "yssarilcommander") {
        return None;
    }
    let actor = PlayerId::new(event.text("player")?);
    if &actor == seat {
        return None;
    }
    let system = SystemId::new(event.text("system")?);
    let board = state.system_state(&system);
    let has_units = board.units.iter().any(|unit| &unit.owner == seat)
        || board
            .planet_units
            .values()
            .flatten()
            .any(|unit| &unit.owner == seat);
    if !has_units {
        return None;
    }
    let kinds = lookable(state, &actor);
    (!kinds.is_empty()).then_some((actor, kinds))
}

fn kind_token(kind: RevealKind) -> (&'static str, &'static str) {
    match kind {
        RevealKind::ActionCards => ("action_cards", "action cards"),
        RevealKind::PromissoryNotes => ("promissory_notes", "promissory notes"),
        RevealKind::SecretObjectives => ("secret_objectives", "secret objectives"),
    }
}

/// So Ata: "After another player activates a system that contains your units: You may look at that
/// player's action cards, promissory notes, or secret objectives."
///
/// The look is a reveal to the owner alone, until the end of the action (`ACTION_COMPLETED`, see
/// [`commander_clear`]).
fn commander_look(owner_name: &str, seat: &PlayerId) -> Ability {
    let owner = seat.clone();
    let condition_owner = seat.clone();
    Ability::stateful(
        format!("leader:{owner_name}:yssarilcommander:SYSTEM_ACTIVATED:after"),
        seat.clone(),
        "SYSTEM_ACTIVATED",
        Relation::After,
        Arc::new(move |event, _resolver, context| {
            let Some((target, kinds)) = commander_target(context.state, &owner, event) else {
                return Ok(());
            };
            let kind = if let [only] = kinds[..] {
                only
            } else {
                let options = kinds
                    .iter()
                    .map(|kind| {
                        let (id, label) = kind_token(*kind);
                        ChoiceOption::labelled(
                            id,
                            "commander",
                            format!("look at {target}'s {label}"),
                        )
                    })
                    .collect();
                let choice = Choice::new(
                    owner.clone(),
                    format!("So Ata: what to look at on {target}"),
                    options,
                )
                .contextualized(decision(
                    context.state,
                    &owner,
                    "yssarilcommander",
                    "so_ata_look",
                ));
                let answer = context
                    .ask_seeing(&choice)
                    .map_err(TimingError::IllegalChoice)?;
                let Some(kind) = kinds
                    .iter()
                    .copied()
                    .find(|kind| kind_token(*kind).0 == answer.id)
                else {
                    return Err(TimingError::IllegalChoice(IllegalChoice::NotOffered {
                        player: owner.clone(),
                        chosen: answer.id.clone(),
                        offered: kinds
                            .iter()
                            .map(|kind| kind_token(*kind).0.to_owned())
                            .collect(),
                    }));
                };
                kind
            };
            let ids: Vec<String> = match kind {
                RevealKind::ActionCards => context
                    .state
                    .player(&target)
                    .map(|seat| {
                        seat.action_cards
                            .iter()
                            .map(|card| card.as_str().to_owned())
                            .collect()
                    })
                    .unwrap_or_default(),
                RevealKind::PromissoryNotes => hidden_notes(context.state, &target),
                RevealKind::SecretObjectives => context
                    .state
                    .player(&target)
                    .map(|seat| {
                        seat.secret_objectives
                            .iter()
                            .map(|card| card.as_str().to_owned())
                            .collect()
                    })
                    .unwrap_or_default(),
            };
            if hooks_cards::reveal(
                context.state,
                &owner,
                &target,
                kind,
                &ids,
                RevealScope::Action,
                "yssarilcommander",
            ) {
                context.rng.observe_hidden_information();
            }
            Ok(())
        }),
    )
    .with_optional(true)
    .with_stateful_condition(Arc::new(move |event, _, context| {
        commander_target(context.state, &condition_owner, event).is_some()
    }))
}

/// Whether a So Ata reveal row is still standing.
fn commander_reveals_stand(state: &GameState, viewer: &PlayerId) -> bool {
    state.faction_marks.keys().any(|key| {
        key.strip_prefix("cards:reveal:").is_some_and(|rest| {
            let mut parts = rest.split('|');
            parts.next();
            parts.next() == Some("yssarilcommander") && parts.next() == Some(viewer.as_str())
        })
    })
}

/// End So Ata's look when the action it was made in is over (the engine's own end-of-action clear
/// of `RevealScope::Action` is a pending request; this makes the module not depend on it).
fn commander_clear(owner_name: &str, seat: &PlayerId) -> Ability {
    let condition_owner = seat.clone();
    Ability::stateful(
        format!("leader:{owner_name}:yssarilcommander_clear:ACTION_COMPLETED:after"),
        seat.clone(),
        "ACTION_COMPLETED",
        Relation::After,
        Arc::new(move |_event, _resolver, context| {
            hooks_cards::clear_reveals_from(context.state, "yssarilcommander");
            Ok(())
        }),
    )
    .with_stateful_condition(Arc::new(move |_event, _, context| {
        commander_reveals_stand(context.state, &condition_owner)
    }))
}

// -- Spy Net -------------------------------------------------------------------------------------

/// The Yssaril seat whose hand `holder` may look at with Spy Net, if `holder` holds the note, is
/// not its owner, and that hand has a card.
fn spy_net_owner(state: &GameState, holder: &PlayerId) -> Option<PlayerId> {
    if state.promissory_notes.get(SPY_NET) != Some(holder) {
        return None;
    }
    let owner = crate::promissory::seat_of(state, FACTION)?;
    (&owner != holder && hand_size(state, &owner) > 0).then_some(owner)
}

/// Spy Net: "At the start of your turn: Look at the Yssaril player's hand of action cards. Choose
/// 1 of those cards and add it to your hand. Then, return this card to the Yssaril player."
fn spy_net(owner_name: &str, seat: &PlayerId) -> Ability {
    let holder = seat.clone();
    let condition_holder = seat.clone();
    Ability::stateful(
        format!("promissory:{owner_name}:spynet:TURN_BEGAN:after"),
        seat.clone(),
        "TURN_BEGAN",
        Relation::After,
        Arc::new(move |event, _resolver, context| {
            if event.text("player") != Some(holder.as_str()) {
                return Ok(());
            }
            let Some(owner) = spy_net_owner(context.state, &holder) else {
                return Ok(());
            };
            crate::action_cards::look_at_hand_and_take(context, &holder, &owner, "spynet", false)
                .map_err(TimingError::IllegalChoice)?;
            crate::promissory::give_back(context.state, SPY_NET);
            Ok(())
        }),
    )
    .with_optional(true)
    .with_stateful_condition(Arc::new(move |event, _, context| {
        event.text("player") == Some(condition_holder.as_str())
            && spy_net_owner(context.state, &condition_holder).is_some()
    }))
}

fn timing_abilities(_state: &GameState, owner_name: &str, seat: &PlayerId) -> Vec<Ability> {
    vec![
        spy_net(owner_name, seat),
        commander_look(owner_name, seat),
        commander_clear(owner_name, seat),
        deepgloom_transaction(owner_name, seat),
    ]
}

// -- the hero ------------------------------------------------------------------------------------

/// The other seats holding action cards, in seat order.
fn hero_targets(state: &GameState, player: &PlayerId) -> Vec<PlayerId> {
    state
        .players
        .iter()
        .filter(|seat| &seat.id != player && !seat.action_cards.is_empty())
        .map(|seat| seat.id.clone())
        .collect()
}

/// Kyver is offered while she is unlocked and at least one other player has a card to show.
fn leader_action(
    state: &GameState,
    _content: &ContentStore,
    player: &PlayerId,
    leader: &LeaderId,
) -> Option<bool> {
    (leader.as_str() == "yssarilhero").then(|| !hero_targets(state, player).is_empty())
}

fn use_leader(
    context: &mut TimingContext<'_>,
    player: &PlayerId,
    leader: &LeaderId,
) -> Option<bool> {
    if leader.as_str() != "yssarilhero" {
        return None;
    }
    let targets = hero_targets(context.state, player);
    if targets.is_empty() {
        return Some(false);
    }
    // Everyone shows a card first, then Yssaril decides for each player. Every decision is made
    // before any hand changes, so a failed ask leaves the game as it was (reveals are cleared).
    let decided = kyver_decisions(context, player, &targets);
    hooks_cards::clear_reveals_from(context.state, "yssarilhero");
    let Ok(decided) = decided else {
        return Some(false);
    };
    // Applying cannot fail. The takes need their reveal, so they run before it is cleared again.
    for (target, card, outcome) in &decided {
        if *outcome == KyverOutcome::Take {
            hooks_cards::reveal(
                context.state,
                player,
                target,
                RevealKind::ActionCards,
                &[card.as_str().to_owned()],
                RevealScope::Choice,
                "yssarilhero",
            );
            hooks_cards::take_revealed_action_card(context.state, player, target, card);
        }
    }
    hooks_cards::clear_reveals_from(context.state, "yssarilhero");
    for (target, _, outcome) in &decided {
        if *outcome == KyverOutcome::Discard {
            for _ in 0..3 {
                let held = hand_size(context.state, target);
                if held == 0 {
                    break;
                }
                // "Random": uniform over the whole hand, from the game's seeded generator
                // (domain "yssarilhero"), so replay agrees.
                let sides = u32::try_from(held).unwrap_or(u32::MAX);
                let index = context.rng.die("yssarilhero", sides) as usize - 1;
                let picked = context
                    .state
                    .player(target)
                    .and_then(|seat| seat.action_cards.get(index).cloned());
                if let Some(picked) = picked {
                    hooks_cards::discard_chosen(context.state, target, &picked);
                }
            }
        }
    }
    let _ = crate::action_cards::enforce_hand_limit(
        context.state,
        context.content,
        context.table,
        player,
    );
    Some(!decided.is_empty())
}

/// What Kyver does to one player.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KyverOutcome {
    Take,
    Discard,
    Neither,
}

/// Each target shows a card, then Yssaril picks an outcome per player. Mutates only reveals.
fn kyver_decisions(
    context: &mut TimingContext<'_>,
    player: &PlayerId,
    targets: &[PlayerId],
) -> Result<Vec<(PlayerId, ActionCardId, KyverOutcome)>, IllegalChoice> {
    let mut shown: Vec<(PlayerId, ActionCardId)> = Vec::new();
    for target in targets {
        if let Some(card) = crate::action_cards::show_action_card(
            context,
            target,
            player,
            RevealScope::Choice,
            "yssarilhero",
        )? {
            shown.push((target.clone(), card));
        }
    }
    let mut decided = Vec::new();
    for (target, card) in shown {
        let name = crate::action_cards::name_of(context.content, &card);
        let choice = Choice::new(
            player.clone(),
            format!("Kyver: {target} showed {name}"),
            vec![
                ChoiceOption::labelled("take", "hero", format!("take {name} from {target}")),
                ChoiceOption::labelled(
                    "discard",
                    "hero",
                    format!("force {target} to discard 3 random action cards"),
                ),
                ChoiceOption::decline(),
            ],
        )
        .contextualized(decision(
            context.state,
            player,
            "yssarilhero",
            "kyver_take_or_discard",
        ));
        let outcome = match context.ask_seeing(&choice)?.id.as_str() {
            "take" => KyverOutcome::Take,
            "discard" => KyverOutcome::Discard,
            _ => KyverOutcome::Neither,
        };
        decided.push((target, card, outcome));
    }
    Ok(decided)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ti4_model::id::TechnologyId;
    use ti4_model::state::LeaderStatus;

    fn leader_status(state: &GameState, player: &PlayerId, leader: &str) -> Option<LeaderStatus> {
        crate::leaders::status(state, player, &LeaderId::new(leader))
    }

    use std::collections::BTreeMap;
    use ti4_model::content_types::DEFAULT;
    use ti4_model::id::SecretObjectiveId;

    fn a() -> PlayerId {
        PlayerId::new("a")
    }
    fn b() -> PlayerId {
        PlayerId::new("b")
    }
    fn c() -> PlayerId {
        PlayerId::new("c")
    }

    fn game() -> GameState {
        crate::fixtures::seated_game(&[("a", FACTION), ("b", "sol")], DEFAULT)
    }

    fn three() -> GameState {
        crate::fixtures::seated_game(&[("a", FACTION), ("b", "sol"), ("c", "hacan")], DEFAULT)
    }

    fn deal(state: &mut GameState, who: &str, cards: &[&str]) {
        state.player_mut(&PlayerId::new(who)).unwrap().action_cards =
            cards.iter().map(|name| ActionCardId::new(*name)).collect();
    }

    fn make_cc_swap_available(state: &mut GameState) {
        state.player_mut(&a()).unwrap().commodities = 1;
        state.player_mut(&b()).unwrap().commodities = 1;
    }

    fn hand(state: &GameState, who: &str) -> Vec<String> {
        state
            .player(&PlayerId::new(who))
            .unwrap()
            .action_cards
            .iter()
            .map(|card| card.as_str().to_owned())
            .collect()
    }

    fn scripted(answers: &[&str]) -> Table {
        Table::with_default(Box::new(crate::choice::Scripted::new(
            answers.iter().map(|answer| (*answer).to_owned()),
        )))
    }

    fn payload(pairs: &[(&str, &str)]) -> BTreeMap<String, serde_json::Value> {
        pairs
            .iter()
            .map(|(key, value)| ((*key).to_owned(), serde_json::Value::from(*value)))
            .collect()
    }

    fn emit(state: &mut GameState, table: &mut Table, event_type: &str, pairs: &[(&str, &str)]) {
        let mut resolver = crate::fixtures::armed_resolver(state);
        crate::fixtures::with_context(state, DEFAULT, None, table, |ctx| {
            let event = ctx
                .event_sequence
                .next(event_type, payload(pairs))
                .expect("an event id");
            resolver
                .emit_with_context(ctx, event, |_, _| {})
                .expect("the window resolves");
        });
    }

    fn home_of(state: &GameState, who: &PlayerId) -> SystemId {
        state.player(who).unwrap().home_system.clone().unwrap()
    }

    fn give_tech(state: &mut GameState, who: &PlayerId, alias: &str) {
        state
            .player_mut(who)
            .unwrap()
            .technologies
            .insert(TechnologyId::new(alias));
    }

    fn set_leader(state: &mut GameState, who: &PlayerId, leader: &str, status: LeaderStatus) {
        state
            .player_mut(who)
            .unwrap()
            .leaders
            .insert(LeaderId::new(leader), status);
    }

    fn options(state: &GameState, who: &PlayerId) -> Vec<String> {
        component_actions(state, ContentStore::embedded(), who)
            .into_iter()
            .map(|option| option.id)
            .collect()
    }

    fn perform(state: &mut GameState, table: &mut Table, who: &PlayerId, id: &str) -> bool {
        let option = ChoiceOption::labelled(id, crate::faction_abilities::ACTION_KIND, id);
        crate::fixtures::with_context(state, DEFAULT, None, table, |ctx| {
            perform_component(ctx, who, &option)
        })
    }

    // -- Crafty ----------------------------------------------------------------------------------

    #[test]
    fn crafty_lifts_the_hand_limit_for_its_owner_only() {
        let content = ContentStore::embedded();
        let mut state = game();
        let ten = ["sabotage"; 10];
        deal(&mut state, "a", &ten);
        deal(&mut state, "b", &ten);
        let mut table = scripted(&[]);
        crate::action_cards::enforce_hand_limit(&mut state, content, &mut table, &a())
            .expect("no question for Yssaril");
        assert_eq!(hand(&state, "a").len(), 10, "any number of cards");
        // The Solar hand is over the printed limit of seven, so it is asked to discard.
        crate::action_cards::enforce_hand_limit(
            &mut state,
            content,
            &mut crate::choice::Table::default(),
            &b(),
        )
        .unwrap();
        assert_eq!(hand(&state, "b").len(), 7);
    }

    #[test]
    fn crafty_beats_a_law_that_caps_the_hand() {
        let content = ContentStore::embedded();
        let mut state = game();
        state.laws.insert("sanctions".to_owned(), String::new());
        let capped = crate::laws::action_card_limit(&state, 7);
        assert_eq!(capped, 3, "the law is in force");
        deal(&mut state, "a", &["sabotage"; 8]);
        crate::action_cards::enforce_hand_limit(&mut state, content, &mut scripted(&[]), &a())
            .unwrap();
        assert_eq!(
            hand(&state, "a").len(),
            8,
            "Sanctions ({capped}) cannot stop Crafty"
        );
    }

    // -- Scheming --------------------------------------------------------------------------------

    #[test]
    fn scheming_draws_one_extra_then_discards_one_chosen() {
        let content = ContentStore::embedded();
        let mut state = game();
        deal(&mut state, "a", &["sabotage"]);
        state.action_card_deck = ["bribery", "flank_speed", "skilled_retreat"]
            .iter()
            .map(|name| ActionCardId::new(*name))
            .collect();
        let mut table = scripted(&["bribery"]);
        let drawn = crate::action_cards::draw(&mut state, content, &mut table, &a(), 1).unwrap();
        assert_eq!(drawn.len(), 2, "1 requested + 1 additional");
        let held = hand(&state, "a");
        assert_eq!(held.len(), 2, "3 cards, 1 discarded");
        assert!(
            !held.contains(&"bribery".to_owned()),
            "the chosen card went"
        );
        assert!(hooks_cards::has_staged(&state), "its discard is staged");
        assert_eq!(state.action_card_deck.len(), 1);
    }

    #[test]
    fn a_politics_rider_paid_with_a_table_draws_through_scheming() {
        let content = ContentStore::embedded();
        let mut state = game();
        deal(&mut state, "a", &[]);
        state.action_card_deck = ["bribery", "flank_speed", "skilled_retreat", "sabotage"]
            .iter()
            .map(|name| ActionCardId::new(*name))
            .collect();
        state
            .agenda_predictions
            .insert(a(), "for|politic_rider".to_owned());
        let mut table = scripted(&["bribery"]);
        let paid =
            crate::action_cards::resolve_predictions_with(&mut state, content, &mut table, "for");
        assert_eq!(paid, [a()]);
        let held = hand(&state, "a");
        assert_eq!(held.len(), 3, "3 + 1 additional, 1 discarded");
        assert!(!held.contains(&"bribery".to_owned()));
        assert!(state.action_card_deck.is_empty());
        assert_eq!(state.speaker, a());
    }

    #[test]
    fn scheming_applies_to_nobody_else() {
        let content = ContentStore::embedded();
        let mut state = game();
        deal(&mut state, "b", &[]);
        state.action_card_deck = ["bribery", "flank_speed"]
            .iter()
            .map(|name| ActionCardId::new(*name))
            .collect();
        let (decider, seen) = crate::choice::Capturing::new(Box::new(crate::choice::FirstOption));
        let mut table = Table::with_default(Box::new(decider));
        let drawn = crate::action_cards::draw(&mut state, content, &mut table, &b(), 1).unwrap();
        assert_eq!(drawn.len(), 1);
        assert_eq!(hand(&state, "b").len(), 1);
        assert!(seen.borrow().is_empty(), "no question");
        assert!(!hooks_cards::has_staged(&state));
    }

    // -- Transparasteel Plating --------------------------------------------------------------------

    #[test]
    fn transparasteel_stops_passed_players_only_on_its_owners_action_turn() {
        let mut state = game();
        give_tech(&mut state, &a(), "tp");
        state.phase = Phase::Action;
        state.active = Some(a());
        state.player_mut(&b()).unwrap().passed = true;
        assert!(crate::laws::action_cards_forbidden(&state, &b()));
        assert!(!crate::laws::action_cards_forbidden(&state, &a()));
        // Negative: not the owner's turn, or the player has not passed.
        state.active = Some(b());
        assert!(!crate::laws::action_cards_forbidden(&state, &b()));
        state.active = Some(a());
        state.player_mut(&b()).unwrap().passed = false;
        assert!(!crate::laws::action_cards_forbidden(&state, &b()));
        // Negative: without the technology.
        state.player_mut(&b()).unwrap().passed = true;
        state
            .player_mut(&a())
            .unwrap()
            .technologies
            .remove(&TechnologyId::new("tp"));
        assert!(!crate::laws::action_cards_forbidden(&state, &b()));
    }

    // -- the flagship ----------------------------------------------------------------------------

    #[test]
    fn the_flagship_alone_passes_through_other_players_ships() {
        let state = game();
        let content = ContentStore::embedded();
        let active = home_of(&state, &a());
        let player = a();
        for (ship, expected) in [
            ("yssaril_flagship", true),
            ("yssaril_cruiser", false),
            ("sol_flagship", false),
        ] {
            let site = PassSite {
                player: &player,
                active: &active,
                ship_type: ship,
            };
            assert_eq!(
                crate::factions::hooks_movement::may_move_through_ships(
                    &state, content, DEFAULT, &site
                ),
                expected,
                "{ship}"
            );
        }
    }

    // -- Stall Tactics and the mech --------------------------------------------------------------

    #[test]
    fn stall_tactics_is_offered_with_a_card_and_only_to_yssaril() {
        let mut state = game();
        deal(&mut state, "a", &[]);
        deal(&mut state, "b", &["sabotage"]);
        assert!(options(&state, &a()).is_empty(), "no card, no action");
        assert!(options(&state, &b()).is_empty(), "not Sol's ability");
        deal(&mut state, "a", &["sabotage"]);
        assert_eq!(options(&state, &a()), [STALL_TACTICS]);
    }

    fn mechs_on_planets(state: &GameState, who: &PlayerId) -> usize {
        state
            .board
            .values()
            .flat_map(|board| board.planet_units.values().flatten())
            .filter(|unit| &unit.owner == who && unit.type_id.as_str() == "yssaril_mech")
            .count()
    }

    #[test]
    fn stall_tactics_discards_a_chosen_card_and_the_mech_deploys_after() {
        let mut state = game();
        deal(&mut state, "a", &["sabotage", "bribery"]);
        let before = mechs_on_planets(&state, &a());
        let home = home_of(&state, &a());
        let planet = state
            .controlled_planets(&a())
            .into_iter()
            .find(|(system, _)| **system == home)
            .map(|(_, planet)| planet.clone())
            .expect("a home planet");
        let spot = format!("{home}|{planet}");
        let mut table = scripted(&["bribery", &spot]);
        assert!(perform(&mut state, &mut table, &a(), STALL_TACTICS));
        assert_eq!(hand(&state, "a"), ["sabotage"], "the chosen card left");
        assert!(hooks_cards::has_staged(&state));
        assert_eq!(mechs_on_planets(&state, &a()), before + 1, "DEPLOY");
    }

    #[test]
    fn the_mech_may_be_declined_and_stall_tactics_refuses_an_empty_hand() {
        let mut state = game();
        deal(&mut state, "a", &["sabotage"]);
        let before = mechs_on_planets(&state, &a());
        let mut table = scripted(&["decline"]);
        assert!(perform(&mut state, &mut table, &a(), STALL_TACTICS));
        assert!(hand(&state, "a").is_empty());
        assert_eq!(mechs_on_planets(&state, &a()), before, "declined");
        // Atomic refusal with nothing to discard.
        let after = state.clone();
        assert!(!perform(
            &mut state,
            &mut scripted(&[]),
            &a(),
            STALL_TACTICS
        ));
        assert_eq!(state, after);
        // And never for another faction.
        deal(&mut state, "b", &["bribery"]);
        let snapshot = state.clone();
        assert!(!perform(
            &mut state,
            &mut scripted(&[]),
            &b(),
            STALL_TACTICS
        ));
        assert_eq!(state, snapshot);
    }

    #[test]
    fn a_staged_stall_tactics_discard_reaches_the_pile_when_announced() {
        let mut state = game();
        deal(&mut state, "a", &["sabotage"]);
        assert!(perform(
            &mut state,
            &mut scripted(&["decline"]),
            &a(),
            STALL_TACTICS
        ));
        let mut resolver = crate::fixtures::armed_resolver(&state);
        let announced = crate::fixtures::with_context(
            &mut state,
            DEFAULT,
            None,
            &mut crate::choice::Table::default(),
            |ctx| crate::reactions::announce_staged_card_events(ctx, &mut resolver).unwrap(),
        );
        assert_eq!(announced, 1);
        assert!(
            state
                .discarded_action_cards
                .iter()
                .any(|card| card.as_str() == "sabotage")
        );
    }

    // -- Mageon Implants -------------------------------------------------------------------------

    #[test]
    fn mageon_implants_looks_takes_one_and_exhausts() {
        let mut state = game();
        give_tech(&mut state, &a(), "mi");
        deal(&mut state, "a", &[]);
        deal(&mut state, "b", &["sabotage", "bribery"]);
        let id = format!("{MAGEON_PREFIX}b");
        assert!(options(&state, &a()).contains(&id));
        assert!(perform(&mut state, &mut scripted(&["bribery"]), &a(), &id));
        assert_eq!(hand(&state, "a"), ["bribery"]);
        assert_eq!(hand(&state, "b"), ["sabotage"]);
        assert!(
            state
                .player(&a())
                .unwrap()
                .exhausted_technologies
                .contains(&TechnologyId::new("mi"))
        );
        assert!(
            hooks_cards::revealed_to(&state, &a()).is_empty(),
            "the look ended with the choice"
        );
        assert!(!options(&state, &a()).contains(&id), "exhausted");
    }

    #[test]
    fn mageon_implants_needs_the_card_a_ready_technology_and_a_hand_to_look_at() {
        let mut state = game();
        deal(&mut state, "b", &["sabotage"]);
        let id = format!("{MAGEON_PREFIX}b");
        assert!(
            !options(&state, &a()).contains(&id),
            "without the technology"
        );
        let before = state.clone();
        assert!(!perform(&mut state, &mut scripted(&[]), &a(), &id));
        assert_eq!(state, before, "refusal is atomic");
        give_tech(&mut state, &a(), "mi");
        deal(&mut state, "b", &[]);
        assert!(!options(&state, &a()).contains(&id), "nothing to look at");
        deal(&mut state, "b", &["sabotage"]);
        assert!(options(&state, &a()).contains(&id));
        assert!(
            !options(&state, &b())
                .iter()
                .any(|id| id.contains("yssaril")),
            "only the holder"
        );
    }

    // -- Spy Net ---------------------------------------------------------------------------------

    #[test]
    fn spy_net_takes_a_card_at_the_start_of_the_holders_turn_and_returns() {
        let mut state = game();
        deal(&mut state, "a", &["sabotage", "bribery"]);
        deal(&mut state, "b", &[]);
        crate::promissory::take(&mut state, ContentStore::embedded(), &b(), SPY_NET);
        let ability = "promissory:sol:spynet:TURN_BEGAN:after";
        let mut table = scripted(&[ability, "bribery"]);
        emit(&mut state, &mut table, "TURN_BEGAN", &[("player", "b")]);
        assert_eq!(hand(&state, "b"), ["bribery"]);
        assert_eq!(hand(&state, "a"), ["sabotage"]);
        assert_eq!(state.promissory_notes.get(SPY_NET), Some(&a()), "returned");
    }

    #[test]
    fn spy_net_is_not_offered_to_a_player_without_it_or_on_another_turn() {
        let (decider, seen) = crate::choice::Capturing::new(Box::new(crate::choice::FirstOption));
        let mut table = Table::with_default(Box::new(decider));
        let mut state = game();
        deal(&mut state, "a", &["sabotage"]);
        // b does not hold the note.
        emit(&mut state, &mut table, "TURN_BEGAN", &[("player", "b")]);
        // b holds it, but it is a's turn.
        crate::promissory::take(&mut state, ContentStore::embedded(), &b(), SPY_NET);
        emit(&mut state, &mut table, "TURN_BEGAN", &[("player", "a")]);
        // b holds it and Yssaril has no hand.
        deal(&mut state, "a", &[]);
        emit(&mut state, &mut table, "TURN_BEGAN", &[("player", "b")]);
        assert!(seen.borrow().is_empty(), "{:?}", seen.borrow());
        assert_eq!(state.promissory_notes.get(SPY_NET), Some(&b()));
    }

    // -- the commander ---------------------------------------------------------------------------

    #[test]
    fn the_commander_unlocks_with_seven_action_cards() {
        let mut state = game();
        let content = ContentStore::embedded();
        let leader = LeaderId::new("yssarilcommander");
        deal(&mut state, "a", &["sabotage"; 6]);
        let check = |state: &GameState| {
            crate::leaders::commander_unlocked(state, content, DEFAULT, None, &a(), &leader)
        };
        assert_eq!(check(&state), Some(false));
        deal(&mut state, "a", &["sabotage"; 7]);
        assert_eq!(check(&state), Some(true));
        assert_eq!(
            commander_unlocked(&state, content, DEFAULT, None, &a(), &LeaderId::new("x")),
            None,
            "only its own commander"
        );
    }

    fn activate(state: &mut GameState, table: &mut Table, who: &str, system: &SystemId) {
        emit(
            state,
            table,
            "SYSTEM_ACTIVATED",
            &[("player", who), ("system", system.as_str())],
        );
    }

    #[test]
    fn the_commander_looks_at_the_activating_players_cards_and_only_then() {
        let mut state = three();
        set_leader(&mut state, &a(), "yssarilcommander", LeaderStatus::Unlocked);
        deal(&mut state, "b", &["sabotage", "bribery"]);
        deal(&mut state, "c", &["flank_speed"]);
        let home = home_of(&state, &a());
        let ability = "leader:yssaril:yssarilcommander:SYSTEM_ACTIVATED:after";
        let mut table = scripted(&[ability]);
        activate(&mut state, &mut table, "b", &home);
        let seen = hooks_cards::revealed_to(&state, &a());
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].owner, b());
        assert_eq!(seen[0].kind, RevealKind::ActionCards);
        assert_eq!(seen[0].ids, ["sabotage", "bribery"]);
        // Nobody else was shown anything, through the typed observation.
        let content = ContentStore::embedded();
        let observed = Observed::new(&state, content, DEFAULT, None);
        for seat in [b(), c()] {
            assert!(
                crate::choice::SeatObservation::bind(&observed, seat.clone())
                    .revealed_cards()
                    .is_empty(),
                "{seat} was shown nothing"
            );
        }
        let viewer = crate::choice::SeatObservation::bind(&observed, a());
        assert_eq!(viewer.revealed_action_cards()[0].1.len(), 2);
        // ... and the raw state a non-owner would be handed hides the row.
        assert!(
            !ti4_model::view::leaks(&ti4_model::view::view_for(&state, &c()), &c())
                .iter()
                .any(|row| row.contains("cards:reveal")),
            "the redacted view hides it"
        );
        // The action ends: the look ends.
        emit(
            &mut state,
            &mut scripted(&[]),
            "ACTION_COMPLETED",
            &[("player", "b")],
        );
        assert!(hooks_cards::revealed_to(&state, &a()).is_empty());
        assert!(state.faction_marks.is_empty());
    }

    #[test]
    fn ownerless_alliance_recipient_gets_only_its_so_ata_activation_reveal() {
        let content = ContentStore::embedded();
        let mut state = crate::fixtures::seated_game(&[("a", "sol"), ("b", "hacan")], DEFAULT);
        assert!(crate::promissory::grant_commander_ability(
            &mut state,
            content,
            &a(),
            "yssarilcommander"
        ));
        deal(&mut state, "b", &["sabotage"]);
        let system = home_of(&state, &a());
        crate::fixtures::put(&mut state, &system, "cruiser", &a(), 1);
        let ability = "leader:sol:yssarilcommander:SYSTEM_ACTIVATED:after";
        activate(&mut state, &mut scripted(&[ability]), "b", &system);

        let seen = hooks_cards::revealed_to(&state, &a());
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].owner, b());
        assert_eq!(seen[0].kind, RevealKind::ActionCards);
        assert_eq!(seen[0].ids, ["sabotage"]);
        assert!(hooks_cards::revealed_to(&state, &b()).is_empty());
        emit(
            &mut state,
            &mut scripted(&[]),
            "ACTION_COMPLETED",
            &[("player", "b")],
        );
        assert!(hooks_cards::revealed_to(&state, &a()).is_empty());
    }

    #[test]
    fn the_commander_may_look_at_notes_or_secrets_instead() {
        let mut state = game();
        set_leader(&mut state, &a(), "yssarilcommander", LeaderStatus::Unlocked);
        deal(&mut state, "b", &["sabotage"]);
        state.player_mut(&b()).unwrap().secret_objectives = vec![SecretObjectiveId::new("otf")];
        let home = home_of(&state, &a());
        let ability = "leader:yssaril:yssarilcommander:SYSTEM_ACTIVATED:after";
        let mut table = scripted(&[ability, "secret_objectives"]);
        activate(&mut state, &mut table, "b", &home);
        let seen = hooks_cards::revealed_to(&state, &a());
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].kind, RevealKind::SecretObjectives);
        assert_eq!(seen[0].ids, ["otf"]);
    }

    #[test]
    fn the_commander_needs_to_be_unlocked_the_units_and_another_player() {
        let (decider, seen) = crate::choice::Capturing::new(Box::new(crate::choice::FirstOption));
        let mut table = Table::with_default(Box::new(decider));
        let mut state = game();
        deal(&mut state, "b", &["sabotage"]);
        let home = home_of(&state, &a());
        let elsewhere = home_of(&state, &b());
        // Locked.
        activate(&mut state, &mut table, "b", &home);
        set_leader(&mut state, &a(), "yssarilcommander", LeaderStatus::Unlocked);
        // No Yssaril units in the system.
        activate(&mut state, &mut table, "b", &elsewhere);
        // Its own activation.
        activate(&mut state, &mut table, "a", &home);
        assert!(seen.borrow().is_empty(), "{:?}", seen.borrow());
        assert!(state.faction_marks.is_empty());
    }

    // -- the hero --------------------------------------------------------------------------------

    #[test]
    fn kyver_takes_one_card_and_forces_discards_on_another_then_is_purged() {
        let mut state = three();
        set_leader(&mut state, &a(), "yssarilhero", LeaderStatus::Unlocked);
        deal(&mut state, "a", &[]);
        deal(&mut state, "b", &["sabotage", "bribery"]);
        deal(
            &mut state,
            "c",
            &["flank_speed", "skilled_retreat", "rally", "plague"],
        );
        let hero = LeaderId::new("yssarilhero");
        assert_eq!(
            leader_action(&state, ContentStore::embedded(), &a(), &hero),
            Some(true)
        );
        // b shows bribery and is robbed of it; c shows rally and loses 3 random cards.
        let mut table = scripted(&["bribery", "rally", "take", "discard"]);
        let used = crate::fixtures::with_context(&mut state, DEFAULT, None, &mut table, |ctx| {
            crate::leaders::use_leader(ctx, &a(), &hero)
        });
        assert!(used);
        assert_eq!(hand(&state, "a"), ["bribery"]);
        assert_eq!(hand(&state, "b"), ["sabotage"]);
        assert_eq!(hand(&state, "c").len(), 1, "3 of 4 discarded");
        assert_eq!(
            leader_status(&state, &a(), "yssarilhero"),
            Some(LeaderStatus::Purged)
        );
        assert!(hooks_cards::revealed_to(&state, &a()).is_empty());
        assert!(
            state
                .faction_marks
                .keys()
                .all(|k| k.starts_with("cards:staged:"))
        );
    }

    #[test]
    fn kyver_may_decline_each_player_and_needs_a_hand_to_look_at() {
        let mut state = game();
        set_leader(&mut state, &a(), "yssarilhero", LeaderStatus::Unlocked);
        let hero = LeaderId::new("yssarilhero");
        deal(&mut state, "b", &[]);
        assert_eq!(
            leader_action(&state, ContentStore::embedded(), &a(), &hero),
            Some(false),
            "nobody has a card to show"
        );
        assert_eq!(
            leader_action(
                &state,
                ContentStore::embedded(),
                &a(),
                &LeaderId::new("sol")
            ),
            None
        );
        let before = state.clone();
        let used =
            crate::fixtures::with_context(&mut state, DEFAULT, None, &mut scripted(&[]), |ctx| {
                use_leader(ctx, &a(), &hero)
            });
        assert_eq!(used, Some(false));
        assert_eq!(state, before, "nothing changed");
        deal(&mut state, "b", &["sabotage"]);
        let used = crate::fixtures::with_context(
            &mut state,
            DEFAULT,
            None,
            &mut scripted(&["decline"]),
            |ctx| use_leader(ctx, &a(), &hero),
        );
        assert_eq!(used, Some(true), "she was used even if she did nothing");
        assert_eq!(hand(&state, "b"), ["sabotage"]);
        assert!(hand(&state, "a").is_empty());
    }

    #[test]
    fn kyvers_random_discard_reaches_every_index_of_a_large_hand() {
        let mut reached = std::collections::BTreeSet::new();
        for seed in 0..400u64 {
            let mut rng = crate::rng::GameRng::new(seed);
            reached.insert(rng.die("yssarilhero", 12) as usize - 1);
        }
        assert_eq!(
            reached.len(),
            12,
            "indices 10 and 11 are reachable: {reached:?}"
        );
    }

    #[test]
    fn kyver_changes_nothing_when_a_decision_fails() {
        let mut state = game();
        set_leader(&mut state, &a(), "yssarilhero", LeaderStatus::Unlocked);
        deal(&mut state, "b", &["sabotage"]);
        let before = state.clone();
        let hero = LeaderId::new("yssarilhero");
        let used = crate::fixtures::with_context(
            &mut state,
            DEFAULT,
            None,
            &mut scripted(&["bogus-answer"]),
            |ctx| use_leader(ctx, &a(), &hero),
        );
        assert_eq!(used, Some(false));
        assert_eq!(state, before);
    }

    // -- Ssruu -----------------------------------------------------------------------------------

    fn ssruu_game() -> GameState {
        let mut state = crate::fixtures::seated_game(&[("a", FACTION), ("b", "hacan")], DEFAULT);
        let seat = state.player_mut(&b()).unwrap();
        seat.commodities = 0;
        seat.leaders
            .insert(LeaderId::new("hacanagent"), LeaderStatus::Exhausted);
        state
    }

    #[test]
    fn ssruu_borrows_an_action_agent_even_exhausted_and_exhausts_itself() {
        let mut state = ssruu_game();
        let content = ContentStore::embedded();
        let id = "component|leader|yssarilagent|hacanagent";
        let offered: Vec<String> = crate::leaders::component_actions(&state, content, &a())
            .into_iter()
            .map(|option| option.id)
            .collect();
        assert!(offered.iter().any(|offered| offered == id), "{offered:?}");
        let used = crate::fixtures::with_context(
            &mut state,
            DEFAULT,
            None,
            &mut scripted(&["b"]),
            |ctx| crate::leaders::use_leader(ctx, &a(), &LeaderId::new("yssarilagent|hacanagent")),
        );
        assert!(used);
        assert!(
            state.player(&b()).unwrap().commodities > 0,
            "Carth's text ran"
        );
        assert_eq!(
            leader_status(&state, &b(), "hacanagent"),
            Some(LeaderStatus::Exhausted),
            "the source agent is untouched"
        );
        assert_eq!(
            leader_status(&state, &a(), "yssarilagent"),
            Some(LeaderStatus::Exhausted)
        );
    }

    #[test]
    fn ssruu_is_not_offered_to_a_seat_without_it() {
        let state = ssruu_game();
        let offered = crate::leaders::component_actions(&state, ContentStore::embedded(), &b());
        assert!(
            offered
                .iter()
                .all(|option| !option.id.contains("yssarilagent"))
        );
    }

    // -- Deepgloom Executable -------------------------------------------------------------------------

    fn with_bt(state: &mut GameState) {
        state.player_mut(&a()).unwrap().breakthrough =
            Some(ti4_model::id::BreakthroughId::new("yssarilbt"));
    }

    #[test]
    fn an_allowed_stall_tactics_discards_for_the_user_and_exempts_their_transaction() {
        let mut state = game();
        with_bt(&mut state);
        state.phase = Phase::Action;
        deal(&mut state, "b", &["sabotage"]);
        let id = format!("{BT_STALL_PREFIX}a");
        assert!(options(&state, &b()).contains(&id));
        let content = ContentStore::embedded();
        assert!(!transaction_limit_exempt(&state, content, &b(), &a()));
        assert!(perform(&mut state, &mut scripted(&["allow"]), &b(), &id));
        assert!(hand(&state, "b").is_empty());
        assert!(hooks_cards::has_staged(&state));
        assert!(transaction_limit_exempt(&state, content, &b(), &a()));
        assert!(
            crate::transactions::may_open_again(
                &{
                    let mut s = state.clone();
                    s.record_transaction(&b(), &a());
                    s
                },
                content,
                &b(),
                &a()
            ),
            "the limit no longer applies to the pair"
        );
        state.turn_seq += 1;
        assert!(
            !transaction_limit_exempt(&state, content, &b(), &a()),
            "this turn only"
        );
    }

    #[test]
    fn a_refused_stall_tactics_changes_nothing_but_is_not_offered_again() {
        let mut state = game();
        with_bt(&mut state);
        deal(&mut state, "b", &["sabotage"]);
        let id = format!("{BT_STALL_PREFIX}a");
        assert!(!perform(&mut state, &mut scripted(&["decline"]), &b(), &id));
        assert_eq!(hand(&state, "b"), ["sabotage"]);
        assert!(!transaction_limit_exempt(
            &state,
            ContentStore::embedded(),
            &b(),
            &a()
        ));
        assert!(
            !options(&state, &b()).contains(&id),
            "not offered again this turn"
        );
    }

    #[test]
    fn deepgloom_needs_the_breakthrough_and_a_card() {
        let mut state = game();
        deal(&mut state, "b", &["sabotage"]);
        assert!(
            !options(&state, &b())
                .iter()
                .any(|id| id.contains("bt_stall"))
        );
        with_bt(&mut state);
        deal(&mut state, "b", &[]);
        assert!(
            !options(&state, &b())
                .iter()
                .any(|id| id.contains("bt_stall"))
        );
    }

    #[test]
    fn borrowed_stall_tactics_is_atomic_when_the_users_discard_answer_is_invalid() {
        let mut state = game();
        with_bt(&mut state);
        deal(&mut state, "b", &["sabotage", "bribery"]);
        let before = state.clone();
        let id = format!("{BT_STALL_PREFIX}a");
        assert!(!perform(
            &mut state,
            &mut scripted(&["allow", "not-a-card"]),
            &b(),
            &id
        ));
        assert_eq!(
            state, before,
            "neither consent nor the invalid choice mutates state"
        );
    }

    #[test]
    fn borrowed_scheming_restores_the_draw_when_its_discard_answer_is_invalid() {
        let content = ContentStore::embedded();
        let mut state = game();
        with_bt(&mut state);
        deal(&mut state, "b", &["sabotage"]);
        state.action_card_deck = vec![ActionCardId::new("bribery"), ActionCardId::new("upgrade")];
        let before = state.clone();
        let mut table = scripted(&["allow", "not-a-card"]);

        let result = crate::action_cards::draw(&mut state, content, &mut table, &b(), 1);

        assert!(result.is_err(), "the invalid mandatory discard is rejected");
        assert_eq!(
            state, before,
            "the consent, deck draw, and partial hand change roll back"
        );
    }

    #[test]
    fn borrowed_scheming_draws_extra_discards_one_and_stages_the_transaction() {
        let content = ContentStore::embedded();
        let mut state = game();
        with_bt(&mut state);
        deal(&mut state, "b", &["sabotage"]);
        state.action_card_deck = vec![ActionCardId::new("bribery"), ActionCardId::new("upgrade")];
        let mut table = scripted(&["allow", "sabotage"]);

        let drawn = crate::action_cards::draw(&mut state, content, &mut table, &b(), 1).unwrap();

        assert_eq!(
            drawn,
            [ActionCardId::new("bribery"), ActionCardId::new("upgrade")]
        );
        assert_eq!(hand(&state, "b"), ["bribery", "upgrade"]);
        assert!(state.action_card_deck.is_empty());
        assert_eq!(
            crate::supply::staged_event_types(&state),
            ["DEEPGLOOM_TRANSACTION"]
        );
        assert!(hooks_cards::has_staged(&state));
        assert!(!state.faction_marks.contains_key(&scheming_key(&b())));
    }

    #[test]
    fn game_step_flushes_the_borrowed_scheming_transaction() {
        let content = ContentStore::embedded();
        let hub = crate::fixtures::plain_hub();
        let mut state = game();
        with_bt(&mut state);
        state.phase = Phase::Action;
        state.finished = true;
        // This pair already used its ordinary transaction this turn. Deepgloom must permit
        // the borrowed deal without reopening an ordinary deal after its exemption expires.
        state.record_transaction(&a(), &b());
        let turn_seq = state.turn_seq;
        make_cc_swap_available(&mut state);
        deal(&mut state, "b", &["sabotage"]);
        state.action_card_deck = vec![ActionCardId::new("bribery"), ActionCardId::new("upgrade")];
        let (decider, seen) =
            crate::choice::Capturing::new(Box::new(crate::choice::Scripted::new([
                "allow", "sabotage", "cc1", "accept",
            ])));
        let table = Table::with_default(Box::new(decider));
        let mut driver =
            crate::game::Game::with_table(state, content, table).with_galaxy(hub.galaxy);

        let drawn =
            crate::action_cards::draw(&mut driver.state, content, &mut driver.table, &b(), 1)
                .unwrap();
        assert_eq!(drawn.len(), 2);
        assert_eq!(
            crate::supply::staged_event_types(&driver.state),
            ["DEEPGLOOM_TRANSACTION"]
        );
        assert!(transaction_limit_exempt(&driver.state, content, &a(), &b()));
        assert!(crate::transactions::may_open_again(
            &driver.state,
            content,
            &a(),
            &b()
        ));

        let result = driver.step();

        assert_eq!(result.error, None);
        assert!(result.finished);
        assert_eq!(
            driver.state.turn_seq, turn_seq,
            "finished step did not advance the turn"
        );
        assert_eq!(driver.state.transactions_this_round, vec![(a(), b())]);
        assert_eq!(driver.state.player(&a()).unwrap().commodities, 0);
        assert_eq!(driver.state.player(&b()).unwrap().commodities, 0);
        assert!(!crate::transactions::may_open_again(
            &driver.state,
            content,
            &a(),
            &b()
        ));
        assert!(driver.state.transacted_with(&a()).contains(&b()));
        assert!(crate::supply::staged_event_types(&driver.state).is_empty());
        assert!(!transaction_limit_exempt(
            &driver.state,
            content,
            &a(),
            &b()
        ));
        let transaction_choices = seen
            .borrow()
            .iter()
            .filter(|choice| {
                choice.context.as_ref().is_some_and(|context| {
                    context.subtype == "propose_transaction"
                        || context.subtype == "answer_transaction"
                })
            })
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(transaction_choices.len(), 2);
        assert_eq!(transaction_choices[0].player, a());
        assert_eq!(transaction_choices[1].player, b());
    }
    #[test]
    fn borrowed_scheming_refusal_leaves_the_draw_unmodified_by_yssaril() {
        let content = ContentStore::embedded();
        let mut state = game();
        with_bt(&mut state);
        deal(&mut state, "b", &["sabotage"]);
        state.action_card_deck = vec![ActionCardId::new("bribery"), ActionCardId::new("upgrade")];
        let mut table = scripted(&["decline"]);

        let drawn = crate::action_cards::draw(&mut state, content, &mut table, &b(), 1).unwrap();

        assert_eq!(drawn, [ActionCardId::new("bribery")]);
        assert_eq!(hand(&state, "b"), ["sabotage", "bribery"]);
        assert_eq!(state.action_card_deck, [ActionCardId::new("upgrade")]);
        assert!(!hooks_cards::has_staged(&state));
        assert!(crate::supply::staged_event_types(&state).is_empty());
    }

    #[test]
    fn scheming_does_not_discard_when_no_card_was_drawn() {
        let content = ContentStore::embedded();
        let mut state = game();
        deal(&mut state, "a", &["sabotage", "bribery"]);
        state.action_card_deck.clear();
        let before = state.clone();
        let mut table = scripted(&[]);

        let drawn = crate::action_cards::draw(&mut state, content, &mut table, &a(), 1).unwrap();

        assert!(drawn.is_empty());
        assert_eq!(state, before, "Scheming only triggers after an actual draw");
    }

    #[test]
    fn borrowed_stall_tactics_opens_one_forced_pair_transaction_and_expires() {
        let content = ContentStore::embedded();
        let hub = crate::fixtures::plain_hub();
        let mut state = game();
        with_bt(&mut state);
        state.phase = Phase::Action;
        // A prior ordinary deal has spent this pair's allowance; the borrowed trade is the
        // exempt extra transaction, and cleanup must restore the spent-limit result.
        state.record_transaction(&a(), &b());
        make_cc_swap_available(&mut state);
        deal(&mut state, "b", &["sabotage"]);
        let id = format!("{BT_STALL_PREFIX}a");
        let (decider, seen) =
            crate::choice::Capturing::new(Box::new(crate::choice::Scripted::new([
                "allow", "cc1", "accept",
            ])));
        let mut table = Table::with_default(Box::new(decider));

        assert!(crate::fixtures::with_context(
            &mut state,
            DEFAULT,
            Some(&hub.galaxy),
            &mut table,
            |context| perform_component(
                context,
                &b(),
                &ChoiceOption::labelled(&id, crate::faction_abilities::ACTION_KIND, &id),
            ),
        ));
        assert_eq!(
            crate::supply::staged_event_types(&state),
            ["DEEPGLOOM_TRANSACTION"]
        );
        assert!(transaction_limit_exempt(&state, content, &a(), &b()));
        assert!(crate::transactions::may_open_again(
            &state,
            content,
            &a(),
            &b()
        ));
        assert!(
            hooks_cards::has_staged(&state),
            "the chosen discard is staged too"
        );

        let mut resolver = crate::fixtures::armed_resolver(&state);
        let mut dice = crate::dice::Dice::new();
        let mut rng = crate::rng::GameRng::new(0);
        let mut sequence = crate::event::EventSequence::new();
        let mut resolving = crate::choice::Resolving {
            content,
            sources: DEFAULT,
            dice: &mut dice,
            rng: &mut rng,
            table: &mut table,
            timing: Some(crate::choice::TimingHandle {
                resolver: &mut resolver,
                sequence: &mut sequence,
                galaxy: Some(&hub.galaxy),
            }),
        };
        let flushed = crate::supply::flush_staged_events(&mut state, &mut resolving);
        drop(resolving);
        assert_eq!(
            flushed,
            3,
            "one borrowed transaction should emit its event and both trade gains; resolver log: {:?}; scripted choices: {:?}",
            resolver.log(),
            seen.borrow()
        );

        let choices = seen.borrow();
        let transactions: Vec<_> = choices
            .iter()
            .filter(|choice| {
                choice.context.as_ref().is_some_and(|context| {
                    context.subtype == "propose_transaction"
                        || context.subtype == "answer_transaction"
                })
            })
            .collect();
        assert_eq!(transactions.len(), 2, "one offer and one answer window");
        assert_eq!(
            transactions[0].player,
            a(),
            "the Yssaril owner is the proposer"
        );
        assert!(transactions[0].ids().contains(&"cc1"));
        assert_eq!(
            transactions[1].player,
            b(),
            "the user is the forced counterparty"
        );
        assert!(transactions[1].ids().contains(&"accept"));
        assert_eq!(state.transactions_this_round, vec![(a(), b())]);
        assert!(!transaction_limit_exempt(&state, content, &a(), &b()));
        assert!(!crate::transactions::may_open_again(
            &state,
            content,
            &a(),
            &b()
        ));
        assert!(state.transacted_with(&a()).contains(&b()));
        assert_eq!(
            crate::supply::staged_events(&state),
            0,
            "transaction events drained"
        );
        assert!(
            hooks_cards::has_staged(&state),
            "the card discard still awaits its own event flush"
        );
    }

    // -- no Yssaril, no questions ----------------------------------------------------------------

    #[test]
    fn a_game_without_yssaril_is_never_offered_a_yssaril_ability() {
        let (decider, seen) = crate::choice::Capturing::new(Box::new(crate::choice::FirstOption));
        let mut table = Table::with_default(Box::new(decider));
        let mut state = crate::fixtures::seated_game(&[("a", "hacan"), ("b", "sol")], DEFAULT);
        deal(&mut state, "a", &["sabotage"; 9]);
        deal(&mut state, "b", &["bribery"]);
        let home = home_of(&state, &b());
        for (event, pairs) in [
            ("TURN_BEGAN", vec![("player", "a")]),
            ("TURN_BEGAN", vec![("player", "b")]),
            (
                "SYSTEM_ACTIVATED",
                vec![("player", "a"), ("system", home.as_str())],
            ),
            ("ACTION_COMPLETED", vec![("player", "a")]),
        ] {
            emit(&mut state, &mut table, event, &pairs);
        }
        let asked: Vec<String> = seen
            .borrow()
            .iter()
            .filter(|choice| choice.ids().iter().any(|id| id.contains("yssaril")))
            .map(|choice| choice.prompt.clone())
            .collect();
        assert!(asked.is_empty(), "{asked:?}");
        for who in [a(), b()] {
            assert!(options(&state, &who).is_empty());
            assert!(!action_cards_forbidden(&state, &who));
        }
        assert_eq!(
            action_card_limit(&state, ContentStore::embedded(), &a(), 7),
            7
        );
        assert_eq!(
            action_card_draw_bonus(&state, ContentStore::embedded(), &a(), 1),
            0
        );
        assert!(state.faction_marks.is_empty(), "no bookkeeping written");
    }

    #[test]
    fn a_nekro_flagship_with_the_yssaril_z_token_passes_through_other_players_ships() {
        let content = ContentStore::embedded();
        let passes = |lent: &[&str], ship: &str| {
            let state = crate::fixtures::nekro_with_z(&[("a", "nekro"), ("b", "sol")], lent);
            let active = home_of(&state, &a());
            let player = a();
            let site = PassSite {
                player: &player,
                active: &active,
                ship_type: ship,
            };
            crate::factions::hooks_movement::may_move_through_ships(&state, content, DEFAULT, &site)
        };
        assert!(!passes(&[], "nekro_flagship"), "off by default");
        assert!(passes(&["yssaril"], "nekro_flagship"));
        assert!(!passes(&["yssaril"], "cruiser"), "only the flagship");
    }
}
