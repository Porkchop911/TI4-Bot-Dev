//! Action cards: draw, hand limit, discard (LRR 2).
//!
//! Ported from the oracle's `engine/action_cards.py`: `draw`, `_first_of_each`,
//! `enforce_hand_limit`, `discard` and `unimplemented`.

use std::collections::{BTreeMap, BTreeSet};

use ti4_content::ContentStore;
use ti4_content::galaxy::Galaxy;
use ti4_content::units::UnitType;
use ti4_model::content_types::{ContentType, POK, SourceSet};
use ti4_model::id::{ActionCardId, PlayerId};
use ti4_model::state::GameState;

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

/// Play an action card as a component action. Returns `false` for an option that is not one.
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
    crate::reactions::announce(context, resolver, player, &alias)?;
    Ok(true)
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
/// The prediction is read from the event rather than from the game, because the card is played
/// into the `AGENDA_REVEALED` window and the outcomes are what that event carries.
fn imperial_rider(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    let choices: Vec<String> = context.state.agenda_choices.clone();
    let Some(first) = choices.first().cloned() else {
        return; // nothing to predict, so the card cannot resolve (22.3)
    };
    let predicted = if choices.len() == 1 {
        first
    } else {
        let choice = crate::choice::Choice::new(
            player.clone(),
            "Imperial Rider: predict the agenda outcome",
            choices
                .iter()
                .map(|outcome| {
                    crate::choice::ChoiceOption::labelled(
                        outcome.clone(),
                        "prediction",
                        format!("predict {outcome}"),
                    )
                })
                .collect(),
        );
        match context.ask_seeing(&choice) {
            Ok(answer) => answer.id,
            Err(_) => return,
        }
    };
    context
        .state
        .agenda_predictions
        .insert(player.clone(), predicted);
}

/// Pay every correct Imperial Rider once the outcome is known, and clear the predictions.
///
/// Called when a vote closes. Clearing matters: a prediction left behind would pay again on the
/// next agenda, for a card that was spent on this one.
pub fn resolve_predictions(state: &mut GameState, outcome: &str) -> Vec<PlayerId> {
    let predictions = std::mem::take(&mut state.agenda_predictions);
    let mut paid = Vec::new();
    for (player, predicted) in predictions {
        if predicted == outcome {
            if let Some(seat) = state.player_mut(&player) {
                seat.victory_points =
                    (seat.victory_points + 1).min(crate::objectives::VICTORY_TARGET);
            }
            paid.push(player);
        }
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
    if seat.silence_activation == this_activation {
        rules.ignore_enemy_ships_from = seat.silence_system.as_ref().map(ToString::to_string);
    }
}

// -- shared machinery for effects ----------------------------------------------------------------

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
pub fn effect_for(alias: &ActionCardId) -> Option<Effect> {
    match alias.as_str() {
        // Four physical copies each, resolved from the printed name rather than listed by hand:
        // the registry test catches a *wrong* alias, but nothing catches a *missing* one, and a
        // fourth copy left off a list stays unplayable for ever with no symptom.
        "mb1" | "mb2" | "mb3" | "mb4" => Some(morale_boost),
        "fs1" | "fs2" | "fs3" | "fs4" => Some(flank_speed),
        "cripple" => Some(cripple_defenses),
        "f_deployment" => Some(frontline_deployment),
        "f_researched" => Some(focused_research),
        "ghost_ship" => Some(ghost_ship),
        "jamming" => Some(signal_jamming),
        "lucky" => Some(lucky_shot),
        "imp_rider" => Some(imperial_rider),
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
        "s_retreat1" | "s_retreat2" | "s_retreat3" | "s_retreat4" => Some(skilled_retreat),
        "silence_space" => Some(in_the_silence_of_space),
        _ => None,
    }
}

/// Aliases with a registered effect.
#[must_use]
pub fn registered_aliases() -> Vec<&'static str> {
    vec![
        "fs1",
        "fs2",
        "fs3",
        "fs4",
        "cripple",
        "f_deployment",
        "imp_rider",
        "insub",
        "messiah",
        "mining_initiative",
        "mb1",
        "mb2",
        "mb3",
        "mb4",
        "nav_suite",
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
    ]
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
/// Every action card is currently unimplemented; the list is exposed so the gap is queryable
/// rather than implied, in the same way `unregistered_objectives` is.
#[must_use]
pub fn unimplemented(content: &ContentStore) -> Vec<ActionCardId> {
    content
        .records(ContentType::ActionCards)
        .iter()
        .filter_map(|record| record.text("alias"))
        .map(ActionCardId::new)
        .collect()
}

#[cfg(test)]
mod tests {

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

    /// A component action this engine has *no* effect for.
    ///
    /// Chosen by asking rather than by taking the first: the first one gained an effect and a
    /// test naming it silently became a test of something else.
    fn an_unimplemented_component_action() -> ActionCardId {
        ContentStore::embedded()
            .from_sources(
                ti4_model::content_types::ContentType::ActionCards,
                ti4_model::content_types::POK,
            )
            .filter(|record| record.text("window") == Some("Action"))
            .filter_map(|record| record.text("alias").map(ActionCardId::new))
            .find(|alias| effect_for(alias).is_none())
            .expect("some component action is still unported")
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
    fn playing_a_component_action_spends_the_card_even_with_no_effect() {
        // 22.3: it was genuinely played. Leaving an unmodelled card in hand would let a bot
        // hold it for ever and would hide the gap behind a card that never leaves.
        let content = ContentStore::embedded();
        let card = an_unimplemented_component_action();
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
                .any(|line| line.contains("ACTION_CARD_UNRESOLVED")),
            "and said it had no effect: {:?}",
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
                planet
                    .has_trait("hazardous")
                    .then(|| {
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
    fn every_action_card_is_reported_unimplemented() {
        // The gap is queryable rather than implied: nothing plays an action card yet.
        let all = unimplemented(ContentStore::embedded());
        assert!(all.len() > 50, "the corpus has a full deck");
    }
}
