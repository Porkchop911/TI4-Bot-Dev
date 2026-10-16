//! Keleres flagship (Artemiris), mech (Omniopiares), the three variant heroes and the Keleres
//! Rider. Everything else Keleres is in `keleres.rs`.
//!
//! Card text (content corpus, latest printing):
//!
//! * Artemiris, flagship: "Other players must spend 2 influence to activate the system that
//!   contains this ship."
//! * Omniopiares, mech: "Other players must spend 1 influence to commit ground forces to the planet
//!   that contains this unit."
//! * Kuuasi Aun Jalatai, hero (Keleres-Argent): "At the start of a round of space combat in a system
//!   that contains a planet you control: Place your flagship and up to a total of 2 cruisers and/or
//!   destroyers from your reinforcements in the active system. Then, purge this card."
//! * Odlynn Myrr, hero (Keleres-Xxcha): "After an agenda is revealed: You may cast up to 6 additional
//!   votes on this agenda. Predict aloud an outcome of this agenda. For each player that abstains or
//!   votes for another outcome, gain 1 trade good and 1 command token. Then, purge this card."
//! * Harka Leeds, hero (Keleres-Mentak): "ACTION: Reveal cards from the action card deck until you
//!   reveal 3 action cards that have component actions. Draw those cards and shuffle the rest back
//!   into the action card deck. Then, purge this card." Unlock: "Have 3 scored objectives."
//!   (generic, `leaders::check_unlocks`).
//! * Keleres Rider (`riderm` / `riderx` / `ridera`), promissory note: "After an agenda is revealed:
//!   You cannot vote on this agenda. Predict aloud an outcome of this agenda. If your prediction is
//!   correct, draw 1 action card and gain 2 trade goods. Then, return this card to the Keleres
//!   player."
//!
//! Routes:
//!
//! * Artemiris is a cost of activating, so it is a legality gate and a payment. A system holding
//!   another player's Artemiris is not offered by `tactical::activatable_with` unless the activator
//!   can pay ([`activation_unpayable`]); the payment is a mandatory `SYSTEM_ACTIVATED` window ability
//!   ([`artemiris`]) that the activator pays with `production::pay_seeing`, choosing the planets.
//! * Omniopiares is the same for the commit step: `invasion.rs` leaves out the options that land on
//!   an unpayable tolled planet ([`commit_blocked`]) and takes the payment with the first landing on
//!   the planet ([`pay_commit_toll`]); further landings on the same planet in the same activation
//!   are free (one "commit ground forces to the planet" step).
//! * Kuuasi hangs on `COMBAT_ROUND_STARTED`; Odlynn on `AGENDA_REVEALED` (the votes, through
//!   `vote::add_votes`) and `AGENDA_RESOLVED` (the payout, read from the mirrored ballot); Harka is
//!   an ACTION delivered through [`leader_action`] / [`use_leader`]; the Rider is a `AGENDA_REVEALED`
//!   window that records its prediction in `GameState::agenda_predictions` under the `keleres_rider`
//!   alias, which `action_cards::rider_payoff` pays.

use std::sync::Arc;

use ti4_content::ContentStore;
use ti4_model::content_types::SourceSet;
use ti4_model::id::{LeaderId, PlanetId, PlayerId, SystemId};
use ti4_model::state::{GameState, LeaderStatus};

use super::hooks_combat::CombatHooks;
use crate::choice::{Choice, ChoiceOption, IllegalChoice, Table};
use crate::decision_context::{DecisionContext, DecisionSource};
use crate::event::Event;
use crate::production::Spend;
use crate::timing::{Ability, Relation, TimingContext, TimingError};

const FLAGSHIP: &str = "keleres_flagship";
const MECH: &str = "keleres_mech";
const HARKA: &str = "keleresheroharka";
const ODLYNN: &str = "keleresheroodlynn";
const KUUASI: &str = "keleresherokuuasi";
/// The three Keleres Riders, one per variant.
const RIDERS: [&str; 3] = ["riderm", "riderx", "ridera"];
/// The alias a Keleres Rider's prediction carries in `agenda_predictions` (`"outcome|alias"`).
pub(crate) const RIDER_ALIAS: &str = "keleres_rider";
/// Influence Artemiris charges to activate its system.
const ARTEMIRIS_COST: i64 = 2;
/// Influence Omniopiares charges to commit ground forces to its planet.
const OMNIOPIARES_COST: i64 = 1;
/// Extra votes Odlynn Myrr casts.
const ODLYNN_VOTES: i64 = 6;
/// Cruisers and/or destroyers Kuuasi places besides the flagship.
const KUUASI_ESCORTS: usize = 2;
/// Component-action cards Harka draws.
const HARKA_CARDS: usize = 3;

/// Units claimed by every Keleres variant (flagship, mech).
pub const UNITS: &[&str] = &[FLAGSHIP, MECH];
/// Leaders claimed per variant: the variant's hero (the shared agent and commander are reported by
/// `keleres.rs`). Keleres-Mentak: Harka Leeds.
pub const LEADERS_M: &[&str] = &["keleresagent", HARKA, "kelerescommander"];
/// Keleres–Xxcha leaders: Odlynn Myrr.
pub const LEADERS_X: &[&str] = &["keleresagent", ODLYNN, "kelerescommander"];
/// Keleres–Argent leaders: Kuuasi Aun Jalatai.
pub const LEADERS_A: &[&str] = &["keleresagent", KUUASI, "kelerescommander"];
/// Keleres–Mentak Rider.
pub const PROMISSORY_M: &[&str] = &["riderm"];
/// Keleres–Xxcha Rider.
pub const PROMISSORY_X: &[&str] = &["riderx"];
/// Keleres–Argent Rider.
pub const PROMISSORY_A: &[&str] = &["ridera"];
/// Combat hooks for the units and heroes. None: every window here is a timing ability.
pub const COMBAT_HOOKS: CombatHooks = CombatHooks::NONE;

/// Timing abilities of the units, heroes and Rider, for one seat.
///
/// Every window is registered for every seat; each condition is false unless the seat really has the
/// flagship, the unlocked hero or the note in hand, so a game without Keleres is unchanged.
pub(crate) fn timing_abilities(
    _state: &GameState,
    owner_name: &str,
    seat: &PlayerId,
) -> Vec<Ability> {
    vec![
        artemiris(owner_name, seat),
        kuuasi(owner_name, seat),
        odlynn_votes(owner_name, seat),
        odlynn_payout(owner_name, seat),
        keleres_rider(owner_name, seat),
    ]
}

/// The one question the abilities here ask.
fn ask(
    context: &mut TimingContext<'_>,
    who: &PlayerId,
    ability: &str,
    subtype: &str,
    prompt: String,
    options: Vec<ChoiceOption>,
) -> Result<ChoiceOption, TimingError> {
    let choice = Choice::new(who.clone(), prompt, options).contextualized(DecisionContext::new(
        who.clone(),
        DecisionSource::FactionAbility(ability.to_owned()),
        subtype,
        context.state.phase,
        context.state.round,
    ));
    context
        .ask_seeing(&choice)
        .map_err(TimingError::IllegalChoice)
}

/// Whether `player` holds `leader` unlocked.
fn unlocked(state: &GameState, player: &PlayerId, leader: &str) -> bool {
    state.player(player).is_some_and(|seat| {
        seat.leaders.get(&LeaderId::new(leader)) == Some(&LeaderStatus::Unlocked)
    })
}

/// Restore a state a failed multi-step effect left half changed.
fn fail<T>(
    context: &mut TimingContext<'_>,
    before: GameState,
    error: IllegalChoice,
) -> Result<T, TimingError> {
    *context.state = before;
    Err(TimingError::IllegalChoice(error))
}

// -- Artemiris ----------------------------------------------------------------------------------

/// Influence `player` owes to activate `system`: 2 for the Artemiris of each other player in its
/// space area.
fn activation_toll(state: &GameState, player: &PlayerId, system: &SystemId) -> i64 {
    let Some(board) = state.board.get(system) else {
        return 0;
    };
    let owners: std::collections::BTreeSet<&PlayerId> = board
        .units
        .iter()
        .filter(|unit| {
            unit.owner != *player
                && super::flagship_has_text(state, &unit.owner, unit.type_id.as_str(), FLAGSHIP)
        })
        .map(|unit| &unit.owner)
        .collect();
    i64::try_from(owners.len()).unwrap_or(i64::MAX) * ARTEMIRIS_COST
}

/// Whether `player` cannot activate `system` because it holds another player's Artemiris and they
/// cannot pay its toll. Read by `tactical::activatable_with`, so the option is never offered.
#[must_use]
pub(crate) fn activation_unpayable(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    system: &SystemId,
) -> bool {
    let toll = activation_toll(state, player, system);
    toll > 0 && !crate::payment::affordable(state, content, sources, player, toll, Spend::Influence)
}

/// The activator who owes this Artemiris its toll for `event`.
fn artemiris_payer(
    context: &TimingContext<'_>,
    owner: &PlayerId,
    event: &Event,
) -> Option<PlayerId> {
    let payer = PlayerId::new(event.text("player")?);
    let system = SystemId::new(event.text("system")?);
    if &payer == owner || context.state.player(&payer).is_none() {
        return None;
    }
    let holds = context.state.board.get(&system).is_some_and(|board| {
        board.units.iter().any(|unit| {
            &unit.owner == owner
                && super::flagship_has_text(context.state, owner, unit.type_id.as_str(), FLAGSHIP)
        })
    });
    (holds
        && crate::payment::affordable(
            context.state,
            context.content,
            context.sources,
            &payer,
            ARTEMIRIS_COST,
            Spend::Influence,
        ))
    .then_some(payer)
}

/// Artemiris: the activator pays 2 influence, choosing the planets. Mandatory ("must"); a system the
/// activator could not pay for was never offered, so the payment finds the influence it needs.
fn artemiris(owner_name: &str, seat: &PlayerId) -> Ability {
    let (owner, condition_owner) = (seat.clone(), seat.clone());
    Ability::stateful(
        format!("unit:{owner_name}:{FLAGSHIP}:SYSTEM_ACTIVATED:when"),
        seat.clone(),
        "SYSTEM_ACTIVATED",
        Relation::When,
        Arc::new(move |event, _, context| {
            let Some(payer) = artemiris_payer(context, &owner, event) else {
                return Ok(());
            };
            let before = context.state.clone();
            match crate::production::pay_seeing(
                context.state,
                context.content,
                context.sources,
                context.galaxy,
                context.table,
                &payer,
                ARTEMIRIS_COST,
                Spend::Influence,
            ) {
                Ok(true) => Ok(()),
                Ok(false) => {
                    *context.state = before;
                    Ok(())
                }
                Err(error) => fail(context, before, error),
            }
        }),
    )
    .with_stateful_condition(Arc::new(move |event, _, context| {
        artemiris_payer(context, &condition_owner, event).is_some()
    }))
}

// -- Omniopiares ----------------------------------------------------------------------------------

/// The mark recording that the toll for `planet` is paid in the current activation.
fn toll_mark(state: &GameState, planet: &PlanetId) -> String {
    format!("{TOLL_PREFIX}{}:{planet}", state.activation_seq)
}

const TOLL_PREFIX: &str = "keleres:omniopiares:";

/// Whether `invader` owes the Omniopiares toll to commit ground forces to `planet`: another player's
/// mech stands on it and the toll is not yet paid in this activation.
fn commit_toll_owed(
    state: &GameState,
    invader: &PlayerId,
    system: &SystemId,
    planet: &PlanetId,
) -> bool {
    state.board.get(system).is_some_and(|board| {
        board
            .on_planet(planet)
            .iter()
            .any(|unit| unit.type_id.as_str() == MECH && unit.owner != *invader)
    }) && !state.faction_marks.contains_key(&toll_mark(state, planet))
}

/// Whether landing on `planet` is barred to `invader`: the toll is owed and cannot be paid. Read by
/// `invasion::commit_options`, so such a landing is never offered.
#[must_use]
pub(crate) fn commit_blocked(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    invader: &PlayerId,
    system: &SystemId,
    planet: &PlanetId,
) -> bool {
    commit_toll_owed(state, invader, system, planet)
        && !crate::payment::affordable(
            state,
            content,
            sources,
            invader,
            OMNIOPIARES_COST,
            Spend::Influence,
        )
}

/// Pay the Omniopiares toll for committing to `planet`, if one is owed. `Ok(true)` when nothing is
/// owed or it was paid (the planet is then marked paid for this activation); `Ok(false)`, with
/// nothing changed, when it is owed and cannot be paid.
///
/// # Errors
/// [`IllegalChoice`] when the decider answers the payment with something not offered; nothing has
/// changed.
#[allow(
    clippy::too_many_arguments,
    reason = "one parameter per distinct input of the payment"
)]
pub(crate) fn pay_commit_toll(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: Option<&ti4_content::galaxy::Galaxy>,
    table: &mut Table,
    invader: &PlayerId,
    system: &SystemId,
    planet: &PlanetId,
) -> Result<bool, IllegalChoice> {
    if !commit_toll_owed(state, invader, system, planet) {
        return Ok(true);
    }
    if !crate::production::pay_seeing(
        state,
        content,
        sources,
        galaxy,
        table,
        invader,
        OMNIOPIARES_COST,
        Spend::Influence,
    )? {
        return Ok(false);
    }
    // Marks of earlier activations are history.
    let current = format!("{TOLL_PREFIX}{}:", state.activation_seq);
    state
        .faction_marks
        .retain(|key, _| !key.starts_with(TOLL_PREFIX) || key.starts_with(&current));
    let mark = toll_mark(state, planet);
    state.faction_marks.insert(mark, String::new());
    Ok(true)
}

// -- Kuuasi Aun Jalatai ---------------------------------------------------------------------------

/// The system of a space-combat round `owner` may use Kuuasi in: `owner` fights in it, controls a
/// planet in it, the hero is unlocked, and something can be placed.
fn kuuasi_window(context: &TimingContext<'_>, owner: &PlayerId, event: &Event) -> Option<SystemId> {
    if !unlocked(context.state, owner, KUUASI) {
        return None;
    }
    let fighting = [event.text("attacker"), event.text("defender")]
        .iter()
        .flatten()
        .any(|side| *side == owner.as_str());
    if !fighting {
        return None;
    }
    let system = SystemId::new(event.text("system")?);
    let controls = context
        .state
        .board
        .get(&system)?
        .planet_control
        .values()
        .any(|holder| holder == owner);
    (controls && !kuuasi_options(context, owner).is_empty()).then_some(system)
}

/// Units the box still holds for one base type.
fn in_reinforcements(
    context: &TimingContext<'_>,
    owner: &PlayerId,
    base_type: &str,
    cap: usize,
) -> usize {
    crate::action_cards::placed_unit_id(
        context.state,
        context.content,
        context.sources,
        owner,
        base_type,
    )
    .map_or(0, |id| {
        crate::supply::allowed(
            context.state,
            context.content,
            context.sources,
            owner,
            &id,
            cap,
        )
    })
}

/// The compositions Kuuasi can place: `(flagship, cruisers, destroyers)` with at most 2 escorts,
/// each limited by what the reinforcements hold. The flagship comes whenever it can; "up to" lets the
/// escorts be fewer. Empty when nothing can be placed.
fn kuuasi_options(context: &TimingContext<'_>, owner: &PlayerId) -> Vec<(bool, usize, usize)> {
    let flagship = in_reinforcements(context, owner, "flagship", 1) > 0;
    let cruisers = in_reinforcements(context, owner, "cruiser", KUUASI_ESCORTS);
    let destroyers = in_reinforcements(context, owner, "destroyer", KUUASI_ESCORTS);
    let mut options = Vec::new();
    for c in 0..=cruisers {
        for d in 0..=destroyers {
            if c + d <= KUUASI_ESCORTS && (flagship || c + d > 0) {
                options.push((flagship, c, d));
            }
        }
    }
    options
}

fn kuuasi_id(option: &(bool, usize, usize)) -> String {
    format!("kuuasi|{}|{}", option.1, option.2)
}

/// Kuuasi Aun Jalatai: the Keleres places its flagship and up to 2 cruisers and/or destroyers from
/// reinforcements in the active system, then the card is purged.
///
/// Offered as a window ability, so choosing it is the consent; the composition is then asked
/// (skipped when only one is possible). Everything is checked before the first placement, and an
/// error restores the state.
///
/// Rules question (evidence file): the text does not say the Keleres must be in the combat, but
/// ships placed by a player who is not a combatant cannot fight, so the window is offered to the
/// combatants only.
fn kuuasi(owner_name: &str, seat: &PlayerId) -> Ability {
    let (owner, condition_owner) = (seat.clone(), seat.clone());
    Ability::stateful(
        format!("leader:{owner_name}:{KUUASI}:COMBAT_ROUND_STARTED:after"),
        seat.clone(),
        "COMBAT_ROUND_STARTED",
        Relation::After,
        Arc::new(move |event, _, context| {
            let Some(system) = kuuasi_window(context, &owner, event) else {
                return Ok(());
            };
            let options = kuuasi_options(context, &owner);
            let chosen = if let [only] = options.as_slice() {
                *only
            } else {
                let choices = options
                    .iter()
                    .map(|option| {
                        ChoiceOption::labelled(
                            kuuasi_id(option),
                            "kuuasi",
                            format!(
                                "place the flagship, {} cruisers and {} destroyers",
                                option.1, option.2
                            ),
                        )
                    })
                    .collect();
                let answer = ask(
                    context,
                    &owner,
                    KUUASI,
                    "kuuasi_place",
                    "Kuuasi Aun Jalatai: which ships to place".to_owned(),
                    choices,
                )?;
                let Some(found) = options.iter().find(|option| kuuasi_id(option) == answer.id)
                else {
                    return Ok(());
                };
                *found
            };
            let (flagship, cruisers, destroyers) = chosen;
            if flagship {
                crate::action_cards::place_units_counted(
                    context, &owner, &system, None, "flagship", 1,
                );
            }
            crate::action_cards::place_units_counted(
                context, &owner, &system, None, "cruiser", cruisers,
            );
            crate::action_cards::place_units_counted(
                context,
                &owner,
                &system,
                None,
                "destroyer",
                destroyers,
            );
            crate::leaders::purge(context.state, &owner, &LeaderId::new(KUUASI));
            Ok(())
        }),
    )
    .with_optional(true)
    .with_stateful_condition(Arc::new(move |event, _, context| {
        kuuasi_window(context, &condition_owner, event).is_some()
    }))
}

// -- Odlynn Myrr ------------------------------------------------------------------------------------

/// Mark holding the outcome Odlynn predicted for the agenda numbered `agenda_seq`.
fn odlynn_mark(owner: &PlayerId, agenda_seq: u32) -> String {
    format!("keleres:odlynn:{owner}:{agenda_seq}")
}

/// Whether `owner` could vote on the agenda just revealed with Odlynn unlocked.
fn odlynn_ready(state: &GameState, owner: &PlayerId) -> bool {
    unlocked(state, owner, ODLYNN)
        && !state.agenda_choices.is_empty()
        && !state.agenda_predictions.contains_key(owner)
}

/// Odlynn Myrr, after an agenda is revealed: 6 additional votes on this agenda (banked with
/// `vote::add_votes`, which `vote.rs` adds to the votes the Keleres actually casts), a prediction
/// of an outcome, and the card is purged. The payout is [`odlynn_payout`].
fn odlynn_votes(owner_name: &str, seat: &PlayerId) -> Ability {
    let (owner, condition_owner) = (seat.clone(), seat.clone());
    Ability::stateful(
        format!("leader:{owner_name}:{ODLYNN}:AGENDA_REVEALED:after"),
        seat.clone(),
        "AGENDA_REVEALED",
        Relation::After,
        Arc::new(move |_, _, context| {
            if !odlynn_ready(context.state, &owner) {
                return Ok(());
            }
            let Some(predicted) = crate::action_cards::predicted_outcome(
                context,
                &owner,
                "Odlynn Myrr: predict the agenda outcome",
            ) else {
                return Ok(()); // nothing to predict: the card cannot resolve, and is not spent
            };
            crate::vote::add_votes(context.state, &owner, ODLYNN_VOTES);
            let mark = odlynn_mark(&owner, context.state.agenda_seq);
            context.state.faction_marks.insert(mark, predicted);
            crate::leaders::purge(context.state, &owner, &LeaderId::new(ODLYNN));
            Ok(())
        }),
    )
    .with_optional(true)
    .with_stateful_condition(Arc::new(move |_, _, context| {
        odlynn_ready(context.state, &condition_owner)
    }))
}

/// Odlynn Myrr's payout, when the agenda resolves: for each other player that abstained or voted for
/// an outcome other than the predicted one, 1 trade good and 1 command token.
///
/// The ballot is the one `game.rs` mirrors on `agenda_votes` for this window. A player barred from
/// voting abstains. Rules question (evidence file): "each player" is read as each other player.
fn odlynn_payout(owner_name: &str, seat: &PlayerId) -> Ability {
    let (owner, condition_owner) = (seat.clone(), seat.clone());
    Ability::stateful(
        format!("leader:{owner_name}:{ODLYNN}:AGENDA_RESOLVED:after"),
        seat.clone(),
        "AGENDA_RESOLVED",
        Relation::After,
        Arc::new(move |_, _, context| {
            let mark = odlynn_mark(&owner, context.state.agenda_seq);
            let Some(predicted) = context.state.faction_marks.get(&mark).cloned() else {
                return Ok(());
            };
            let others = context
                .state
                .players
                .iter()
                .filter(|seat| seat.id != owner)
                .filter(|seat| {
                    context.state.agenda_votes.get(&seat.id).map(String::as_str)
                        != Some(predicted.as_str())
                })
                .count();
            let before = context.state.clone();
            context.state.faction_marks.remove(&mark);
            let goods = i32::try_from(others).unwrap_or(i32::MAX);
            crate::supply::gain_trade_goods_staged(context.state, &owner, goods, ODLYNN);
            let tokens = u32::try_from(others).unwrap_or(u32::MAX);
            if let Err(error) = crate::strategy_cards::gain_tokens(
                context.state,
                context.content,
                context.sources,
                context.galaxy,
                context.table,
                &owner,
                tokens,
            ) {
                return fail(context, before, error);
            }
            Ok(())
        }),
    )
    .with_stateful_condition(Arc::new(move |_, _, context| {
        context
            .state
            .faction_marks
            .contains_key(&odlynn_mark(&condition_owner, context.state.agenda_seq))
    }))
}

// -- Harka Leeds ------------------------------------------------------------------------------------

/// Whether `leader` is Harka Leeds and `player` could use the ACTION now: the card is unlocked and
/// the deck holds at least one card with a component action (the hero purges, so it is never offered
/// to burn it for nothing). `None` for any other leader.
pub(crate) fn leader_action(
    state: &GameState,
    content: &ContentStore,
    player: &PlayerId,
    leader: &LeaderId,
) -> Option<bool> {
    (leader.as_str() == HARKA).then(|| {
        unlocked(state, player, HARKA)
            && state
                .action_card_deck
                .iter()
                .any(|card| crate::action_cards::is_component_action(content, card))
    })
}

/// Harka Leeds: reveal cards from the top of the action card deck until 3 with component actions are
/// revealed (or the deck runs out), draw those, and shuffle everything else back with the game's
/// RNG. `None` for any other leader; `Some(false)`, with nothing changed, when it cannot resolve.
/// The shared code purges the card after `Some(true)`.
pub(crate) fn use_leader(
    context: &mut TimingContext<'_>,
    player: &PlayerId,
    leader: &LeaderId,
) -> Option<bool> {
    if leader.as_str() != HARKA {
        return None;
    }
    if leader_action(context.state, context.content, player, leader) != Some(true) {
        return Some(false);
    }
    let before = context.state.clone();
    let before_rng = context.rng.clone();
    let mut revealed_rest = Vec::new();
    let mut found = Vec::new();
    while found.len() < HARKA_CARDS {
        let Some(card) = ti4_model::deck_reserve::take_top(
            &mut context.state.deck_reserve,
            "action_card",
            &mut context.state.action_card_deck,
            Some(player.as_str()),
        ) else {
            break;
        };
        if crate::action_cards::is_component_action(context.content, &card) {
            found.push(card);
        } else {
            revealed_rest.push(card);
        }
    }
    context.state.action_card_deck.append(&mut revealed_rest);
    context.rng.shuffle(
        crate::rng::domain::ACTION_CARDS,
        &mut context.state.action_card_deck,
    );
    if let Some(seat) = context.state.player_mut(player) {
        seat.action_cards.append(&mut found);
    }
    if crate::action_cards::enforce_hand_limit(
        context.state,
        context.content,
        context.table,
        player,
    )
    .is_err()
    {
        *context.state = before;
        *context.rng = before_rng;
        return Some(false);
    }
    Some(true)
}

// -- Keleres Rider ------------------------------------------------------------------------------------

/// The Keleres Rider `holder` holds that is not their own, if any.
fn rider_in_hand(state: &GameState, holder: &PlayerId) -> Option<String> {
    let own = crate::promissory::faction_name(state, holder);
    state
        .promissory_notes
        .iter()
        .find(|(note, who)| {
            *who == holder
                && RIDERS.contains(&crate::promissory::alias_of(note))
                && crate::promissory::owner_of(note).is_some_and(|name| name != own)
        })
        .map(|(note, _)| note.clone())
}

/// Whether `holder` can play a Rider into the window now: they hold one, the agenda has outcomes, and
/// they have not already predicted (one prediction per agenda).
fn rider_ready(state: &GameState, holder: &PlayerId) -> bool {
    !state.agenda_choices.is_empty()
        && !state.agenda_predictions.contains_key(holder)
        && rider_in_hand(state, holder).is_some()
}

/// Keleres Rider: the holder gives up their vote, predicts an outcome and, if right, draws 1 action
/// card and gains 2 trade goods (`action_cards::rider_payoff`). "Then, return this card to the Keleres
/// player" is done at once: the prediction no longer depends on the card, and a prediction that is
/// cleared unpaid (a discarded agenda) must not strand the note with the holder.
fn keleres_rider(owner_name: &str, seat: &PlayerId) -> Ability {
    let (holder, condition_holder) = (seat.clone(), seat.clone());
    Ability::stateful(
        format!("note:{owner_name}:keleres_rider:AGENDA_REVEALED:after"),
        seat.clone(),
        "AGENDA_REVEALED",
        Relation::After,
        Arc::new(move |_, _, context| {
            if !rider_ready(context.state, &holder) {
                return Ok(());
            }
            let Some(note) = rider_in_hand(context.state, &holder) else {
                return Ok(());
            };
            let Some(predicted) = crate::action_cards::predicted_outcome(
                context,
                &holder,
                "Keleres Rider: predict the agenda outcome",
            ) else {
                return Ok(());
            };
            context
                .state
                .agenda_predictions
                .insert(holder.clone(), format!("{predicted}|{RIDER_ALIAS}"));
            crate::promissory::give_back(context.state, &note);
            Ok(())
        }),
    )
    .with_optional(true)
    .with_stateful_condition(Arc::new(move |_, _, context| {
        rider_ready(context.state, &condition_holder)
    }))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::{Arc as Shared, Mutex};

    use super::*;
    use crate::choice::{Decider, Scripted};
    use crate::event::EventSequence;
    use serde_json::Value;
    use ti4_content::galaxy::Galaxy;
    use ti4_model::content_types::DEFAULT;

    fn a() -> PlayerId {
        PlayerId::new("a")
    }
    fn b() -> PlayerId {
        PlayerId::new("b")
    }
    fn game(keleres: &str) -> GameState {
        crate::fixtures::seated_game(&[("a", keleres), ("b", "sol")], DEFAULT)
    }
    fn scripted(answers: &[&str]) -> Table {
        Table::with_default(Box::new(Scripted::new(answers.iter().copied())))
    }
    fn payload(pairs: &[(&str, Value)]) -> BTreeMap<String, Value> {
        pairs
            .iter()
            .map(|(key, value)| ((*key).to_owned(), value.clone()))
            .collect()
    }

    type Asked = Shared<Mutex<Vec<(String, Vec<String>)>>>;

    /// Records every question (who, option ids), then answers from a script.
    struct Recording(Scripted, Asked);

    impl Decider for Recording {
        fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
            self.1.lock().unwrap().push((
                choice.player.to_string(),
                choice.options.iter().map(|o| o.id.clone()).collect(),
            ));
            self.0.choose(choice)
        }
    }

    fn recording(answers: &[&str]) -> (Table, Asked) {
        let asked = Asked::default();
        let table = Table::with_default(Box::new(Recording(
            Scripted::new(answers.iter().copied()),
            Shared::clone(&asked),
        )));
        (table, asked)
    }

    fn offered(asked: &Asked, wanted: &str) -> bool {
        asked
            .lock()
            .unwrap()
            .iter()
            .any(|(_, options)| options.iter().any(|id| id.contains(wanted)))
    }

    /// Emit one typed event through a resolver armed as the game arms one (`reactions::arm`, so the
    /// module's hook wiring is part of what is tested). Returns whether it was cancelled.
    fn emit(
        state: &mut GameState,
        galaxy: Option<&Galaxy>,
        table: &mut Table,
        kind: &str,
        pairs: &[(&str, Value)],
    ) -> bool {
        let mut resolver = crate::fixtures::armed_resolver(state);
        crate::fixtures::with_context(state, DEFAULT, galaxy, table, |ctx| {
            let event = EventSequence::new().next(kind, payload(pairs)).unwrap();
            resolver
                .emit_with_context(ctx, event, |_, _| {})
                .expect("emits")
                .cancelled
        })
    }

    fn influence(state: &GameState, player: &PlayerId) -> i64 {
        crate::production::available(
            state,
            ContentStore::embedded(),
            DEFAULT,
            player,
            Spend::Influence,
        )
    }

    /// Exhaust every planet `player` controls and empty their trade goods.
    fn broke(state: &mut GameState, player: &PlayerId) {
        for planet in state
            .controlled_planets(player)
            .into_iter()
            .map(|(_, planet)| planet.clone())
            .collect::<Vec<_>>()
        {
            state.exhaust_planet(planet);
        }
        state.player_mut(player).unwrap().trade_goods = 0;
    }

    fn count(state: &GameState, system: &SystemId, player: &PlayerId, kind: &str) -> usize {
        state
            .system_state(system)
            .units
            .iter()
            .filter(|unit| &unit.owner == player && unit.type_id.as_str() == kind)
            .count()
    }

    fn unlock(state: &mut GameState, player: &PlayerId, hero: &str) {
        state
            .player_mut(player)
            .unwrap()
            .leaders
            .insert(LeaderId::new(hero), LeaderStatus::Unlocked);
    }

    fn lock(state: &mut GameState, player: &PlayerId, hero: &str) {
        state
            .player_mut(player)
            .unwrap()
            .leaders
            .insert(LeaderId::new(hero), LeaderStatus::Locked);
    }

    fn status(state: &GameState, player: &PlayerId, hero: &str) -> Option<LeaderStatus> {
        state
            .player(player)
            .unwrap()
            .leaders
            .get(&LeaderId::new(hero))
            .copied()
    }

    fn exhausted_count(state: &GameState) -> usize {
        state.exhausted_planets.len()
    }

    // -- claims and neutrality ------------------------------------------------------------------

    #[test]
    fn the_claims_are_the_real_assets_of_each_variant() {
        let content = ContentStore::embedded();
        for (alias, leaders, notes) in [
            ("keleresm", LEADERS_M, PROMISSORY_M),
            ("keleresx", LEADERS_X, PROMISSORY_X),
            ("keleresa", LEADERS_A, PROMISSORY_A),
        ] {
            let sheet = super::super::assets(content, DEFAULT, alias);
            for id in UNITS.iter().chain(leaders).chain(notes) {
                assert!(sheet.iter().any(|a| a.id == *id), "{alias} {id}");
            }
        }
        // The heroes are each variant's own.
        assert_eq!(
            LEADERS_M,
            ["keleresagent", "keleresheroharka", "kelerescommander"]
        );
        assert_eq!(
            LEADERS_X,
            ["keleresagent", "keleresheroodlynn", "kelerescommander"]
        );
        assert_eq!(
            LEADERS_A,
            ["keleresagent", "keleresherokuuasi", "kelerescommander"]
        );
    }

    #[test]
    fn the_abilities_are_wired_through_the_modules_hook() {
        let state = game("keleresm");
        let ids: Vec<String> = crate::factions::timing_abilities(&state, "keleresm", &a())
            .into_iter()
            .map(|ability| ability.id)
            .collect();
        for wanted in [
            "unit:keleresm:keleres_flagship:SYSTEM_ACTIVATED:when",
            "leader:keleresm:keleresherokuuasi:COMBAT_ROUND_STARTED:after",
            "leader:keleresm:keleresheroodlynn:AGENDA_REVEALED:after",
            "leader:keleresm:keleresheroodlynn:AGENDA_RESOLVED:after",
            "note:keleresm:keleres_rider:AGENDA_REVEALED:after",
        ] {
            assert!(ids.iter().any(|id| id == wanted), "{wanted}");
        }
    }

    #[test]
    fn games_without_keleres_see_nothing_of_the_units_heroes_or_riders() {
        let mut state = crate::fixtures::seated_game(&[("a", "sol"), ("b", "hacan")], DEFAULT);
        state.agenda_choices = vec!["for".to_owned(), "against".to_owned()];
        let before = state.clone();
        let (mut table, asked) = recording(&[]);
        let system = state
            .player(&a())
            .unwrap()
            .home_system
            .clone()
            .unwrap()
            .to_string();
        for (kind, pairs) in [
            (
                "SYSTEM_ACTIVATED",
                vec![
                    ("player", Value::from("b")),
                    ("system", system.clone().into()),
                ],
            ),
            (
                "COMBAT_ROUND_STARTED",
                vec![
                    ("system", Value::from(system.clone())),
                    ("attacker", "a".into()),
                    ("defender", "b".into()),
                ],
            ),
            ("AGENDA_REVEALED", vec![("agenda", Value::from("x"))]),
            (
                "AGENDA_RESOLVED",
                vec![("agenda", Value::from("x")), ("player", "for".into())],
            ),
        ] {
            assert!(!emit(&mut state, None, &mut table, kind, &pairs), "{kind}");
        }
        assert!(asked.lock().unwrap().is_empty(), "nothing offered");
        assert_eq!(state, before, "nothing changed");
        let content = ContentStore::embedded();
        let planet = PlanetId::new("x");
        for system in state.board.keys() {
            assert!(!activation_unpayable(
                &state,
                content,
                DEFAULT,
                &b(),
                system
            ));
            assert!(!commit_blocked(
                &state,
                content,
                DEFAULT,
                &b(),
                system,
                &planet
            ));
        }
        // Nobody's leader here is this file's.
        let leader = LeaderId::new("solhero");
        assert_eq!(leader_action(&state, content, &a(), &leader), None);
        crate::fixtures::with_context(&mut state, DEFAULT, None, &mut table, |ctx| {
            assert_eq!(use_leader(ctx, &a(), &leader), None);
        });
        assert!(rider_in_hand(&state, &a()).is_none());
    }

    // -- Artemiris ------------------------------------------------------------------------------

    /// The Keleres flagship on a ring system of the hub, and Sol as the would-be activator.
    fn artemiris_game() -> (crate::fixtures::Hub, GameState, SystemId) {
        let hub = crate::fixtures::plain_hub();
        let mut state = game("keleresm");
        let ring = SystemId::new(&hub.outer[0]);
        crate::fixtures::put(&mut state, &ring, FLAGSHIP, &a(), 1);
        (hub, state, ring)
    }

    fn activatable(state: &GameState, hub: &crate::fixtures::Hub, who: &PlayerId) -> Vec<SystemId> {
        crate::tactical::activatable_with(
            state,
            ContentStore::embedded(),
            DEFAULT,
            &hub.galaxy,
            who,
        )
    }

    #[test]
    fn artemiris_makes_another_player_pay_two_influence_to_activate_its_system() {
        let (hub, mut state, ring) = artemiris_game();
        assert!(activatable(&state, &hub, &b()).contains(&ring), "payable");
        let before = influence(&state, &b());
        assert!(before >= 2, "the home planets pay for it");
        let exhausted = exhausted_count(&state);
        // The activation itself (`tactical::activate`) is the game's; the window it opens is ours.
        crate::tactical::activate(&mut state, &b(), &ring).unwrap();
        emit(
            &mut state,
            Some(&hub.galaxy),
            &mut scripted(&[]),
            "SYSTEM_ACTIVATED",
            &[("player", "b".into()), ("system", ring.to_string().into())],
        );
        assert!(
            influence(&state, &b()) <= before - ARTEMIRIS_COST,
            "two spent"
        );
        assert!(
            exhausted_count(&state) > exhausted,
            "planets were exhausted"
        );
        assert!(
            state.system_state(&ring).command_tokens.contains(&b()),
            "and the system was activated"
        );
    }

    #[test]
    fn artemiris_is_offered_exactly_when_two_influence_are_on_hand() {
        let (hub, state, ring) = artemiris_game();
        // Rich: offered. Broke: not offered, though every other system still is.
        let mut poor = state.clone();
        broke(&mut poor, &b());
        let options = activatable(&poor, &hub, &b());
        assert!(!options.contains(&ring), "cannot pay, not offered");
        assert!(options.contains(&SystemId::new(&hub.outer[1])), "elsewhere");
        // The owner pays nothing.
        assert!(activatable(&poor, &hub, &a()).contains(&ring));
        // One trade good is one influence: still short. Two are enough.
        poor.player_mut(&b()).unwrap().trade_goods = 1;
        assert_eq!(influence(&poor, &b()), 1);
        assert!(!activatable(&poor, &hub, &b()).contains(&ring), "one short");
        poor.player_mut(&b()).unwrap().trade_goods = 2;
        assert_eq!(influence(&poor, &b()), 2);
        assert!(
            activatable(&poor, &hub, &b()).contains(&ring),
            "exactly two"
        );
    }

    #[test]
    fn artemiris_charges_nothing_for_its_owners_own_activation_or_without_the_flagship() {
        let (hub, mut state, ring) = artemiris_game();
        let before = state.clone();
        emit(
            &mut state,
            Some(&hub.galaxy),
            &mut scripted(&[]),
            "SYSTEM_ACTIVATED",
            &[("player", "a".into()), ("system", ring.to_string().into())],
        );
        assert_eq!(state, before, "the owner activates freely");
        let elsewhere = SystemId::new(&hub.outer[1]);
        emit(
            &mut state,
            Some(&hub.galaxy),
            &mut scripted(&[]),
            "SYSTEM_ACTIVATED",
            &[
                ("player", "b".into()),
                ("system", elsewhere.to_string().into()),
            ],
        );
        assert_eq!(state, before, "no flagship there");
    }

    // -- Omniopiares ----------------------------------------------------------------------------

    /// A planet held by the Keleres with its mech, and Sol infantry in the space area above it.
    fn omniopiares_game() -> (GameState, SystemId, PlanetId) {
        let mut state = game("keleresm");
        let (system, planet) = crate::fixtures::a_placed_planet();
        let board = state.system_mut(&system);
        board.units.clear();
        board.planet_units.clear();
        board.planet_control.clear();
        board.planet_control.insert(planet.clone(), a());
        crate::fixtures::put_on_planet(&mut state, &system, &planet, MECH, &a(), 1);
        crate::fixtures::put(&mut state, &system, "infantry", &b(), 2);
        (state, system, planet)
    }

    fn landed(state: &GameState, system: &SystemId, planet: &PlanetId) -> usize {
        state
            .system_state(system)
            .on_planet_of(planet, &b())
            .iter()
            .filter(|unit| unit.type_id.as_str() == "infantry")
            .count()
    }

    #[test]
    fn omniopiares_makes_the_first_landing_cost_one_influence_and_the_rest_free() {
        let (mut state, system, planet) = omniopiares_game();
        let before = exhausted_count(&state);
        let land = format!("commit|0|{planet}");
        let mut table = scripted(&[&land, &land]);
        let planets = crate::invasion::commit_ground_forces(
            &mut state,
            ContentStore::embedded(),
            DEFAULT,
            &mut table,
            &b(),
            &system,
        )
        .expect("commits");
        assert_eq!(planets, std::slice::from_ref(&planet));
        assert_eq!(landed(&state, &system, &planet), 2, "both landed");
        assert_eq!(
            exhausted_count(&state),
            before + 1,
            "one planet paid, once for the whole step"
        );
    }

    #[test]
    fn omniopiares_is_not_offered_when_the_toll_cannot_be_paid_and_a_refusal_changes_nothing() {
        let (mut state, system, planet) = omniopiares_game();
        let content = ContentStore::embedded();
        assert!(!commit_blocked(
            &state,
            content,
            DEFAULT,
            &b(),
            &system,
            &planet
        ));
        broke(&mut state, &b());
        assert!(commit_blocked(
            &state,
            content,
            DEFAULT,
            &b(),
            &system,
            &planet
        ));
        let before = state.clone();
        let mut table = scripted(&[&format!("commit|0|{planet}")]);
        let result = crate::invasion::commit_ground_forces(
            &mut state,
            content,
            DEFAULT,
            &mut table,
            &b(),
            &system,
        );
        assert!(
            matches!(result, Err(IllegalChoice::ScriptDiverged { .. })),
            "the landing was never offered: {result:?}"
        );
        assert_eq!(state, before);
        // And the payment helper itself changes nothing when it cannot pay.
        let mut table = scripted(&[]);
        assert_eq!(
            pay_commit_toll(
                &mut state,
                content,
                DEFAULT,
                None,
                &mut table,
                &b(),
                &system,
                &planet
            ),
            Ok(false)
        );
        assert_eq!(state, before);
    }

    #[test]
    fn omniopiares_charges_neither_its_owner_nor_a_planet_without_it() {
        let (mut state, system, planet) = omniopiares_game();
        let content = ContentStore::embedded();
        let mut table = scripted(&[]);
        let before = state.clone();
        // The owner lands on its own planet for free.
        assert_eq!(
            pay_commit_toll(
                &mut state,
                content,
                DEFAULT,
                None,
                &mut table,
                &a(),
                &system,
                &planet
            ),
            Ok(true)
        );
        // A planet with no mech on it is free for everyone.
        let empty = PlanetId::new("nowhere");
        assert_eq!(
            pay_commit_toll(
                &mut state,
                content,
                DEFAULT,
                None,
                &mut table,
                &b(),
                &system,
                &empty
            ),
            Ok(true)
        );
        assert_eq!(state, before);
    }

    #[test]
    fn omniopiares_is_paid_through_the_whole_invasion_window() {
        let (mut state, system, planet) = omniopiares_game();
        let before = exhausted_count(&state);
        let mut table = scripted(&[&format!("commit|0|{planet}")]);
        let mut dice = crate::dice::Dice::new();
        let mut rng = crate::rng::GameRng::new(5);
        crate::invasion::resolve(
            &mut state,
            ContentStore::embedded(),
            DEFAULT,
            &mut table,
            &mut dice,
            &mut rng,
            &system,
            &b(),
        )
        .expect("the invasion resolves");
        // Taking a planet exhausts it, so the planet itself is not the payment.
        let payment = state
            .exhausted_planets
            .iter()
            .filter(|exhausted| **exhausted != planet)
            .count();
        assert!(payment > before, "the toll was paid");
    }

    // -- Kuuasi Aun Jalatai -----------------------------------------------------------------------

    const KUUASI_ABILITY: &str = "leader:keleresa:keleresherokuuasi:COMBAT_ROUND_STARTED:after";

    /// A combat at a system the Keleres-Argent holds a planet in, with Sol as the attacker.
    fn kuuasi_game() -> (GameState, SystemId) {
        let mut state = game("keleresa");
        let (system, planet) = crate::fixtures::a_placed_planet();
        let board = state.system_mut(&system);
        board.units.clear();
        board.planet_units.clear();
        board.planet_control.clear();
        board.planet_control.insert(planet, a());
        crate::fixtures::put(&mut state, &system, "cruiser", &a(), 1);
        crate::fixtures::put(&mut state, &system, "cruiser", &b(), 1);
        unlock(&mut state, &a(), KUUASI);
        (state, system)
    }

    fn round_started(system: &SystemId) -> Vec<(&'static str, Value)> {
        vec![
            ("system", system.to_string().into()),
            ("attacker", "b".into()),
            ("defender", "a".into()),
            ("round", 1.into()),
        ]
    }

    #[test]
    fn kuuasi_places_the_flagship_and_two_escorts_and_is_purged() {
        let (mut state, system) = kuuasi_game();
        let (cruisers, destroyers) = (
            count(&state, &system, &a(), "cruiser"),
            count(&state, &system, &a(), "destroyer"),
        );
        emit(
            &mut state,
            None,
            &mut scripted(&[KUUASI_ABILITY, "kuuasi|1|1"]),
            "COMBAT_ROUND_STARTED",
            &round_started(&system),
        );
        assert_eq!(count(&state, &system, &a(), FLAGSHIP), 1, "the flagship");
        assert_eq!(count(&state, &system, &a(), "cruiser"), cruisers + 1);
        assert_eq!(count(&state, &system, &a(), "destroyer"), destroyers + 1);
        assert_eq!(
            status(&state, &a(), KUUASI),
            Some(LeaderStatus::Purged),
            "then purge this card"
        );
        assert_eq!(
            count(&state, &system, &b(), "cruiser"),
            1,
            "the enemy is untouched"
        );
        // The placed ships are in the space area the combat rolls from.
        let fleet =
            crate::combat::ships_of(&state, ContentStore::embedded(), DEFAULT, &a(), &system);
        assert_eq!(fleet.len(), cruisers + destroyers + 3);
    }

    #[test]
    fn kuuasi_places_only_the_escorts_when_the_flagship_is_already_on_the_board() {
        let (mut state, system) = kuuasi_game();
        let elsewhere = SystemId::new("18");
        crate::fixtures::put(&mut state, &elsewhere, FLAGSHIP, &a(), 1);
        emit(
            &mut state,
            None,
            &mut scripted(&[KUUASI_ABILITY, "kuuasi|0|2"]),
            "COMBAT_ROUND_STARTED",
            &round_started(&system),
        );
        assert_eq!(
            count(&state, &system, &a(), FLAGSHIP),
            0,
            "not in reinforcements"
        );
        assert_eq!(count(&state, &system, &a(), "destroyer"), 2);
        assert_eq!(status(&state, &a(), KUUASI), Some(LeaderStatus::Purged));
    }

    #[test]
    fn kuuasi_is_not_offered_locked_without_a_planet_to_non_combatants_and_declining_keeps_it() {
        let (state, system) = kuuasi_game();
        let offers = |state: &GameState, pairs: &[(&str, Value)]| {
            let mut state = state.clone();
            let (mut table, asked) = recording(&[KUUASI_ABILITY]);
            emit(&mut state, None, &mut table, "COMBAT_ROUND_STARTED", pairs);
            offered(&asked, KUUASI_ABILITY)
        };
        assert!(offers(&state, &round_started(&system)), "baseline");
        let mut locked = state.clone();
        lock(&mut locked, &a(), KUUASI);
        assert!(!offers(&locked, &round_started(&system)), "locked");
        let mut landless = state.clone();
        landless.system_mut(&system).planet_control.clear();
        assert!(
            !offers(&landless, &round_started(&system)),
            "no planet of theirs"
        );
        // A combat between two other players: the Keleres is not in it.
        let bystander = vec![
            ("system", Value::from(system.to_string())),
            ("attacker", "b".into()),
            ("defender", "b".into()),
            ("round", 1.into()),
        ];
        assert!(!offers(&state, &bystander), "not a combatant");
        // Declined: nothing changes.
        let mut kept = state.clone();
        emit(
            &mut kept,
            None,
            &mut scripted(&["decline"]),
            "COMBAT_ROUND_STARTED",
            &round_started(&system),
        );
        assert_eq!(kept, state);
    }

    // -- Odlynn Myrr ------------------------------------------------------------------------------

    const ODLYNN_ABILITY: &str = "leader:keleresx:keleresheroodlynn:AGENDA_REVEALED:after";

    fn odlynn_game() -> GameState {
        let mut state = game("keleresx");
        unlock(&mut state, &a(), ODLYNN);
        state.agenda_choices = vec!["for".to_owned(), "against".to_owned()];
        state.agenda_seq = 1;
        state
    }

    fn tokens(state: &GameState, who: &PlayerId) -> i32 {
        let seat = state.player(who).unwrap();
        seat.tactic_tokens + seat.fleet_tokens + seat.strategic_tokens
    }

    fn reveal_with_odlynn(state: &mut GameState) {
        emit(
            state,
            None,
            &mut scripted(&[ODLYNN_ABILITY, "for"]),
            "AGENDA_REVEALED",
            &[("agenda", "x".into())],
        );
    }

    #[test]
    fn odlynn_casts_six_more_votes_predicts_and_is_purged() {
        let mut state = odlynn_game();
        reveal_with_odlynn(&mut state);
        assert_eq!(crate::vote::extra_votes(&state, &a()), 6);
        assert_eq!(
            state
                .faction_marks
                .get(&odlynn_mark(&a(), 1))
                .map(String::as_str),
            Some("for")
        );
        assert_eq!(status(&state, &a(), ODLYNN), Some(LeaderStatus::Purged));
        assert_eq!(
            crate::vote::extra_votes(&state, &b()),
            0,
            "only the Keleres"
        );
    }

    #[test]
    fn odlynns_votes_are_counted_in_the_vote_for_the_outcome_cast() {
        let content = ContentStore::embedded();
        let run = |with_hero: bool| {
            let mut state = odlynn_game();
            if with_hero {
                reveal_with_odlynn(&mut state);
            }
            state.speaker = b();
            let mut window = crate::vote::VoteWindow::new(
                &state,
                "for_against_dummy",
                vec!["for".to_owned(), "against".to_owned()],
            );
            for who in window.order().to_vec() {
                let choice = window.pending_choice(&state, content, DEFAULT).unwrap();
                let answer = if who == a() {
                    choice.option("for").cloned().unwrap()
                } else {
                    choice.option("decline").cloned().unwrap()
                };
                window
                    .resolve(&mut state, content, DEFAULT, answer)
                    .unwrap();
                while let Some(choice) = window.pending_choice(&state, content, DEFAULT) {
                    if choice.player != who {
                        break;
                    }
                    let next = choice
                        .options
                        .iter()
                        .find(|o| !o.is_decline())
                        .cloned()
                        .unwrap_or_else(|| choice.option("decline").cloned().unwrap());
                    window.resolve(&mut state, content, DEFAULT, next).unwrap();
                }
            }
            window.ballot().counts.get("for").copied().unwrap_or(0)
        };
        assert_eq!(run(true), run(false) + 6);
    }

    #[test]
    fn odlynn_pays_a_trade_good_and_a_command_token_per_player_who_abstained_or_voted_otherwise() {
        let paid = |votes: &[(&str, &str)]| {
            let mut state = odlynn_game();
            reveal_with_odlynn(&mut state);
            let (goods, pool) = (
                state.player(&a()).unwrap().trade_goods,
                tokens(&state, &a()),
            );
            state.agenda_votes = votes
                .iter()
                .map(|(who, what)| (PlayerId::new(*who), (*what).to_owned()))
                .collect();
            emit(
                &mut state,
                None,
                &mut scripted(&["tactic_tokens", "tactic_tokens"]),
                "AGENDA_RESOLVED",
                &[("agenda", "x".into()), ("player", "for".into())],
            );
            assert!(
                !state.faction_marks.contains_key(&odlynn_mark(&a(), 1)),
                "the prediction is spent"
            );
            (
                state.player(&a()).unwrap().trade_goods - goods,
                tokens(&state, &a()) - pool,
            )
        };
        assert_eq!(
            paid(&[("a", "for"), ("b", "against")]),
            (1, 1),
            "voted another outcome"
        );
        assert_eq!(paid(&[("a", "for")]), (1, 1), "abstained");
        assert_eq!(
            paid(&[("a", "for"), ("b", "for")]),
            (0, 0),
            "voted the prediction"
        );
    }

    #[test]
    fn odlynn_is_not_offered_locked_after_a_prediction_or_with_nothing_to_predict() {
        let state = odlynn_game();
        let offers = |state: &GameState| {
            let mut state = state.clone();
            let (mut table, asked) = recording(&[ODLYNN_ABILITY, "for"]);
            emit(
                &mut state,
                None,
                &mut table,
                "AGENDA_REVEALED",
                &[("agenda", "x".into())],
            );
            offered(&asked, ODLYNN_ABILITY)
        };
        assert!(offers(&state), "baseline");
        let mut locked = state.clone();
        lock(&mut locked, &a(), ODLYNN);
        assert!(!offers(&locked));
        let mut predicted = state.clone();
        predicted.agenda_predictions.insert(a(), "for".to_owned());
        assert!(!offers(&predicted), "cannot vote, so no votes to cast");
        let mut none = state.clone();
        none.agenda_choices.clear();
        assert!(!offers(&none));
        // Nothing resolves without the hero having been used.
        let mut quiet = state;
        let before = quiet.clone();
        emit(
            &mut quiet,
            None,
            &mut scripted(&["tactic_tokens"]),
            "AGENDA_RESOLVED",
            &[("agenda", "x".into()), ("player", "for".into())],
        );
        assert_eq!(quiet, before);
    }

    // -- Harka Leeds ------------------------------------------------------------------------------

    fn cards(component: bool, how_many: usize) -> Vec<ti4_model::id::ActionCardId> {
        let content = ContentStore::embedded();
        content
            .from_sources(ti4_model::content_types::ContentType::ActionCards, DEFAULT)
            .filter_map(|record| record.text("alias").or_else(|| record.text("id")))
            .map(ti4_model::id::ActionCardId::new)
            .filter(|card| crate::action_cards::is_component_action(content, card) == component)
            .take(how_many)
            .collect()
    }

    fn harka_game(deck: Vec<ti4_model::id::ActionCardId>) -> GameState {
        let mut state = game("keleresm");
        unlock(&mut state, &a(), HARKA);
        state.player_mut(&a()).unwrap().action_cards.clear();
        state.action_card_deck = deck;
        state
    }

    fn harka() -> LeaderId {
        LeaderId::new(HARKA)
    }

    #[test]
    fn harka_draws_the_first_three_component_action_cards_and_shuffles_the_rest_back() {
        let (plain, action) = (cards(false, 5), cards(true, 4));
        assert!(
            plain.len() == 5 && action.len() == 4,
            "the corpus has both kinds"
        );
        let deck = vec![
            plain[0].clone(),
            action[0].clone(),
            plain[1].clone(),
            plain[2].clone(),
            action[1].clone(),
            action[2].clone(),
            plain[3].clone(),
            action[3].clone(),
            plain[4].clone(),
        ];
        let mut state = harka_game(deck.clone());
        let content = ContentStore::embedded();
        assert_eq!(leader_action(&state, content, &a(), &harka()), Some(true));
        let mut table = scripted(&[]);
        let used = crate::fixtures::with_context(&mut state, DEFAULT, None, &mut table, |ctx| {
            use_leader(ctx, &a(), &harka())
        });
        assert_eq!(used, Some(true));
        let hand = state.player(&a()).unwrap().action_cards.clone();
        assert_eq!(
            hand,
            [action[0].clone(), action[1].clone(), action[2].clone()]
        );
        // Everything but the three drawn is the deck again (the unrevealed card and the revealed
        // others, reshuffled), in some order.
        let mut left = state.action_card_deck.clone();
        left.sort();
        let mut expected: Vec<_> = deck
            .into_iter()
            .filter(|card| !hand.contains(card))
            .collect();
        expected.sort();
        assert_eq!(left, expected);
    }

    #[test]
    fn harka_shuffles_with_the_game_rng_deterministically() {
        let (plain, action) = (cards(false, 6), cards(true, 3));
        let deck: Vec<_> = plain.iter().chain(&action).cloned().collect();
        let run = || {
            let mut state = harka_game(deck.clone());
            let mut table = scripted(&[]);
            crate::fixtures::with_context(&mut state, DEFAULT, None, &mut table, |ctx| {
                use_leader(ctx, &a(), &harka());
            });
            state.action_card_deck
        };
        let once = run();
        assert_eq!(once, run());
        assert_eq!(once.len(), 6);
    }

    /// The shared leader path (`leaders::component_actions` / `use_leader`) reaches Harka once
    /// `keleres.rs` sets `leader_action` and `use_leader` in its `HOOKS`. Remove the `ignore` then.
    #[test]
    fn harka_is_offered_and_resolved_and_purged_through_the_shared_leader_path() {
        let content = ContentStore::embedded();
        let (plain, action) = (cards(false, 2), cards(true, 3));
        let mut state = harka_game(plain.iter().chain(&action).cloned().collect());
        let offered = crate::leaders::component_actions(&state, content, &a());
        assert!(
            offered
                .iter()
                .any(|option| option.id == "component|leader|keleresheroharka")
        );
        let mut table = scripted(&[]);
        let used = crate::fixtures::with_context(&mut state, DEFAULT, None, &mut table, |ctx| {
            crate::leaders::use_leader(ctx, &a(), &harka())
        });
        assert!(used);
        assert_eq!(state.player(&a()).unwrap().action_cards.len(), 3);
        assert_eq!(status(&state, &a(), HARKA), Some(LeaderStatus::Purged));
        assert!(
            crate::leaders::component_actions(&state, content, &a())
                .iter()
                .all(|option| option.id != "component|leader|keleresheroharka")
        );
    }

    #[test]
    fn harka_is_not_offered_locked_or_with_no_component_action_in_the_deck() {
        let content = ContentStore::embedded();
        let state = harka_game(cards(false, 3));
        assert_eq!(
            leader_action(&state, content, &a(), &harka()),
            Some(false),
            "nothing to find: the hero would burn for nothing"
        );
        let mut table = scripted(&[]);
        let mut spent = state.clone();
        let used = crate::fixtures::with_context(&mut spent, DEFAULT, None, &mut table, |ctx| {
            use_leader(ctx, &a(), &harka())
        });
        assert_eq!(used, Some(false));
        assert_eq!(spent, state, "nothing changed");
        let mut locked = harka_game(cards(true, 3));
        lock(&mut locked, &a(), HARKA);
        assert_eq!(leader_action(&locked, content, &a(), &harka()), Some(false));
    }

    // -- Keleres Rider ------------------------------------------------------------------------------

    fn rider_game(variant: &str, alias: &str) -> (GameState, String) {
        let mut state = game(variant);
        let note = crate::promissory::note_id(alias, variant);
        crate::promissory::take(&mut state, ContentStore::embedded(), &b(), &note);
        state.agenda_choices = vec!["for".to_owned(), "against".to_owned()];
        state.agenda_seq = 1;
        (state, note)
    }

    const RIDER_ABILITY: &str = "note:sol:keleres_rider:AGENDA_REVEALED:after";

    fn reveal_with_rider(state: &mut GameState) {
        emit(
            state,
            None,
            &mut scripted(&[RIDER_ABILITY, "for"]),
            "AGENDA_REVEALED",
            &[("agenda", "x".into())],
        );
    }

    #[test]
    fn a_rider_predicts_gives_up_the_vote_returns_home_and_pays_two_goods_and_a_card_when_right() {
        for (variant, alias) in [
            ("keleresm", "riderm"),
            ("keleresx", "riderx"),
            ("keleresa", "ridera"),
        ] {
            let (mut state, note) = rider_game(variant, alias);
            reveal_with_rider(&mut state);
            assert_eq!(
                state.agenda_predictions.get(&b()).map(String::as_str),
                Some("for|keleres_rider"),
                "{variant}"
            );
            assert_eq!(
                state.promissory_notes.get(&note),
                Some(&a()),
                "returned to the Keleres"
            );
            // The vote order leaves the predictor out.
            let window = crate::vote::VoteWindow::new(
                &state,
                "for_against_dummy",
                vec!["for".to_owned(), "against".to_owned()],
            );
            assert!(!window.order().contains(&b()), "cannot vote on this agenda");
            // Right: 1 card and 2 trade goods.
            let (goods, hand) = (
                state.player(&b()).unwrap().trade_goods,
                state.player(&b()).unwrap().action_cards.len(),
            );
            let mut table = scripted(&[]);
            let paid = crate::action_cards::resolve_predictions_with(
                &mut state,
                ContentStore::embedded(),
                &mut table,
                "for",
            );
            assert_eq!(paid, [b()]);
            assert_eq!(
                state.player(&b()).unwrap().trade_goods,
                goods + 2,
                "{variant}"
            );
            assert_eq!(state.player(&b()).unwrap().action_cards.len(), hand + 1);
            assert!(state.agenda_predictions.is_empty(), "spent");
        }
    }

    #[test]
    fn a_wrong_rider_prediction_pays_nothing_and_the_note_is_home_all_the_same() {
        let (mut state, note) = rider_game("keleresm", "riderm");
        reveal_with_rider(&mut state);
        let (goods, hand) = (
            state.player(&b()).unwrap().trade_goods,
            state.player(&b()).unwrap().action_cards.len(),
        );
        let mut table = scripted(&[]);
        let paid = crate::action_cards::resolve_predictions_with(
            &mut state,
            ContentStore::embedded(),
            &mut table,
            "against",
        );
        assert!(paid.is_empty());
        assert_eq!(state.player(&b()).unwrap().trade_goods, goods);
        assert_eq!(state.player(&b()).unwrap().action_cards.len(), hand);
        assert_eq!(state.promissory_notes.get(&note), Some(&a()));
        // Cleared without paying (a discarded agenda) the note is still home.
        let (mut state, note) = rider_game("keleresm", "riderm");
        reveal_with_rider(&mut state);
        state.agenda_predictions.clear();
        assert_eq!(state.promissory_notes.get(&note), Some(&a()));
    }

    #[test]
    fn a_rider_is_not_offered_to_its_owner_after_a_prediction_or_without_outcomes() {
        let (state, _) = rider_game("keleresm", "riderm");
        let offers = |state: &GameState| {
            let mut state = state.clone();
            let (mut table, asked) = recording(&[RIDER_ABILITY, "for"]);
            emit(
                &mut state,
                None,
                &mut table,
                "AGENDA_REVEALED",
                &[("agenda", "x".into())],
            );
            offered(&asked, "keleres_rider")
        };
        assert!(offers(&state), "baseline");
        // The Keleres holding its own copy does not play it.
        let (mut own, note) = rider_game("keleresm", "riderm");
        own.promissory_notes.insert(note, a());
        assert!(!offers(&own), "nobody plays a card they own");
        let mut predicted = state.clone();
        predicted
            .agenda_predictions
            .insert(b(), "against".to_owned());
        assert!(!offers(&predicted), "one prediction per agenda");
        let mut none = state.clone();
        none.agenda_choices.clear();
        assert!(!offers(&none), "no outcomes to predict");
        // Declined: the card stays in hand.
        let mut kept = state.clone();
        emit(
            &mut kept,
            None,
            &mut scripted(&["decline"]),
            "AGENDA_REVEALED",
            &[("agenda", "x".into())],
        );
        assert_eq!(kept, state);
    }

    #[test]
    fn a_nekro_flagship_with_the_keleres_z_token_charges_two_influence_to_activate() {
        let hub = crate::fixtures::plain_hub();
        let ring = SystemId::new(&hub.outer[0]);
        let paid = |lent: &[&str]| {
            let mut state = crate::fixtures::nekro_with_z(&[("a", "nekro"), ("b", "sol")], lent);
            crate::fixtures::put(&mut state, &ring, "nekro_flagship", &a(), 1);
            let before = influence(&state, &b());
            crate::tactical::activate(&mut state, &b(), &ring).unwrap();
            emit(
                &mut state,
                Some(&hub.galaxy),
                &mut scripted(&[]),
                "SYSTEM_ACTIVATED",
                &[("player", "b".into()), ("system", ring.to_string().into())],
            );
            before - influence(&state, &b())
        };
        assert_eq!(paid(&[]), 0, "off by default");
        assert!(paid(&["keleresm"]) >= ARTEMIRIS_COST);
    }
}
