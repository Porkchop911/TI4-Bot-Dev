//! The Mentak Coalition (`mentak`). See `factions/mod.rs` for the contract and
//! `plans/BASE_FACTIONS_PLAN_2026-10-02.md` for scope.
//!
//! Implemented: Ambush, Pillage (on a trade-goods-gained event and on a resolved transaction),
//! Mirror Computing (the only place the trade-good worth is decided), Salvage Operations, Promise
//! of Protection (the Pillage bar and the return on activation), Fourth Moon, Moll Terminus, the
//! Corsair's passage, Suffi An, S'ula Mentarion, Ipswitch and The Table's Grace (the Corsair
//! stands in for Cruiser II; gaining the breakthrough flips the ships already on the board).

use std::sync::Arc;

use ti4_content::ContentStore;
use ti4_model::content_types::SourceSet;
use ti4_model::id::{LeaderId, PlayerId, SystemId};
use ti4_model::state::{GameState, LeaderStatus};

use super::hooks_combat::{CombatHooks, CombatMoment, HitSite, ProducedHits};
use super::hooks_economy::EconomyHooks;
use super::hooks_ground::GroundHooks;
use super::hooks_movement::{MovementHooks, PassSite};
use super::hooks_strategy::StrategyHooks;
use super::{CombatUnit, FactionModule, Hooks};
use crate::choice::{Choice, ChoiceOption, IllegalChoice};
use crate::decision_context::{DecisionContext, DecisionSource};
use crate::event::Event;
use crate::phrases::{Phrase, Subject, Worded};
use crate::production::Spend;
use crate::timing::{Ability, Relation, TimingContext, TimingError};
use ti4_model::id::UnitTypeId;

/// What this faction implements; grows package by package.
pub const MODULE: FactionModule = FactionModule {
    alias: "mentak",
    abilities: &["ambush", "pillage"],
    technologies: &["mc", "so"],
    units: &["mentak_flagship", "mentak_mech", "mentak_cruiser3"],
    promissory: &["pop"],
    leaders: &["mentakagent", "mentakcommander", "mentakhero"],
    breakthroughs: &["mentakbt"],
    hooks: Hooks {
        commander_unlocked: Some(commander_unlocked),
        timing_abilities: Some(timing_abilities),
        combat: CombatHooks {
            produced_hits: Some(produced_hits),
            may_sustain: Some(flagship_allows_sustain),
            ..CombatHooks::NONE
        },
        ground: GroundHooks {
            may_sustain: Some(mech_allows_sustain),
            ..GroundHooks::NONE
        },
        economy: EconomyHooks {
            trade_good_worth: Some(trade_good_worth),
            ..EconomyHooks::NONE
        },
        movement: MovementHooks {
            may_move_through_ships: Some(may_move_through_ships),
            ..MovementHooks::NONE
        },
        strategy: StrategyHooks {
            unit_form_override: Some(unit_form_override),
            ..StrategyHooks::NONE
        },
        ..Hooks::NONE
    },
};

const FLAGSHIP: &str = "mentak_flagship";
const MECH: &str = "mentak_mech";
const CORSAIR: &str = "mentak_cruiser3";
const BREAKTHROUGH: &str = "mentakbt";
const AGENT: &str = "mentakagent";
const COMMANDER: &str = "mentakcommander";
const HERO: &str = "mentakhero";

// -- small helpers -------------------------------------------------------------------------------

fn is_mentak(state: &GameState, player: &PlayerId) -> bool {
    state
        .player(player)
        .is_some_and(|seat| seat.faction.as_str() == "mentak")
}

/// Owns the technology, or the Nekro's Valefar Assimilator carries its text.
fn owns_technology(state: &GameState, player: &PlayerId, technology: &str) -> bool {
    crate::technology::has_technology_text(state, player, technology)
}

fn leader_status(state: &GameState, player: &PlayerId, leader: &str) -> Option<LeaderStatus> {
    crate::leaders::status(state, player, &LeaderId::new(leader))
}

fn illegal(error: IllegalChoice) -> TimingError {
    TimingError::IllegalChoice(error)
}

/// Put a question to `player`, with exactly these options.
fn ask_among(
    context: &mut TimingContext<'_>,
    player: &PlayerId,
    prompt: String,
    ability: &str,
    subtype: &str,
    options: Vec<ChoiceOption>,
) -> Result<ChoiceOption, IllegalChoice> {
    let choice = Choice::new(player.clone(), prompt, options);
    ask_choice(context, player, choice, ability, subtype)
}

/// [`ask_among`] for a question whose labels name seats or notes by id: `prompt` and `worded`
/// say what they name (`details.phrases`).
fn ask_phrased(
    context: &mut TimingContext<'_>,
    player: &PlayerId,
    prompt: &Worded,
    ability: &str,
    subtype: &str,
    kind: &str,
    worded: &[(String, Worded)],
) -> Result<ChoiceOption, IllegalChoice> {
    let choice = Choice::new(
        player.clone(),
        prompt.label(),
        crate::phrases::options(kind, worded),
    )
    .phrased_prompt(prompt)
    .phrased_options(worded);
    ask_choice(context, player, choice, ability, subtype)
}

fn ask_choice(
    context: &mut TimingContext<'_>,
    player: &PlayerId,
    choice: Choice,
    ability: &str,
    subtype: &str,
) -> Result<ChoiceOption, IllegalChoice> {
    let choice = choice.contextualized(DecisionContext::new(
        player.clone(),
        DecisionSource::FactionAbility(ability.to_owned()),
        subtype,
        context.state.phase,
        context.state.round,
    ));
    context.ask_seeing(&choice)
}

/// [`ask_among`] with a decline added.
fn ask_one(
    context: &mut TimingContext<'_>,
    player: &PlayerId,
    prompt: String,
    ability: &str,
    subtype: &str,
    mut options: Vec<ChoiceOption>,
) -> Result<ChoiceOption, IllegalChoice> {
    options.push(ChoiceOption::decline());
    ask_among(context, player, prompt, ability, subtype, options)
}

/// The base type of a unit type id.
fn base_type_of(content: &ContentStore, sources: SourceSet, unit_type: &str) -> Option<String> {
    ti4_content::units::catalogue(content, sources)
        .get(unit_type)
        .map(|kind| kind.base_type().to_owned())
}

/// Whether `system` holds a unit of `unit_type` (space area) owned by anyone but `except`.
fn foreign_ship_in(
    state: &GameState,
    system: &SystemId,
    unit_type: &str,
    except: &PlayerId,
) -> bool {
    state.system_state(system).units.iter().any(|unit| {
        &unit.owner != except
            && super::flagship_has_text(state, &unit.owner, unit.type_id.as_str(), unit_type)
    })
}

// -- Mirror Computing ----------------------------------------------------------------------------

/// Mirror Computing: "When you spend trade goods, each trade good is worth 2 resources or
/// influence instead of 1."
///
/// The only place this is decided: `production::trade_good_worth` and `payment::plans` both read
/// the economy hook. `max` rather than a sum, so it stays "2 instead of 1" if another effect has
/// already raised the worth.
fn trade_good_worth(state: &GameState, player: &PlayerId, worth: i64) -> i64 {
    if owns_technology(state, player, "mc") {
        worth.max(2)
    } else {
        worth
    }
}

// -- Fourth Moon and Moll Terminus ---------------------------------------------------------------

/// Fourth Moon: "Other player's ships in this system cannot use SUSTAIN DAMAGE."
fn flagship_allows_sustain(
    state: &GameState,
    _content: &ContentStore,
    _sources: SourceSet,
    unit: &CombatUnit<'_>,
) -> bool {
    let Some(system) = unit.system else {
        return true;
    };
    !foreign_ship_in(state, system, FLAGSHIP, unit.player)
}

/// Moll Terminus: "Other player's ground forces on this planet cannot use SUSTAIN DAMAGE."
fn mech_allows_sustain(
    state: &GameState,
    _content: &ContentStore,
    _sources: SourceSet,
    unit: &CombatUnit<'_>,
) -> bool {
    let (Some(system), Some(planet)) = (unit.system, unit.planet) else {
        return true;
    };
    !state
        .system_state(system)
        .planet_units
        .get(planet)
        .is_some_and(|units| {
            units
                .iter()
                .any(|other| other.type_id.as_str() == MECH && &other.owner != unit.player)
        })
}

// -- Corsair -------------------------------------------------------------------------------------

/// Corsair: "If the active system contains another player's non-fighter ships, this unit can move
/// through systems that contain other players' ships."
fn may_move_through_ships(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    site: &PassSite<'_>,
) -> bool {
    if site.ship_type != CORSAIR {
        return false;
    }
    let types = ti4_content::units::catalogue(content, sources);
    state.system_state(site.active).units.iter().any(|unit| {
        &unit.owner != site.player
            && types
                .get(unit.type_id.as_str())
                .is_some_and(|kind| kind.is_ship() && !kind.is_fighter())
    })
}

// -- The Table's Grace ---------------------------------------------------------------------------

/// The Table's Grace: "If you have the Cruiser II unit upgrade technology, flip this card and place
/// it on top of cruiser II." The Corsair stands in for Cruiser II for the holder, in production,
/// on research and on the board (`technology::apply_unit_upgrades`).
fn unit_form_override(
    state: &GameState,
    _content: &ContentStore,
    _sources: SourceSet,
    player: &PlayerId,
    base_type: &str,
    chosen_id: &str,
) -> Option<UnitTypeId> {
    (base_type == "cruiser"
        && chosen_id == "cruiser2"
        && is_mentak(state, player)
        && crate::breakthroughs::holds(state, player, BREAKTHROUGH))
    .then(|| UnitTypeId::new(CORSAIR))
}

/// Whether `event` announces that `player` gained The Table's Grace.
fn grace_gained(event: &Event, state: &GameState, player: &PlayerId) -> bool {
    event.text("player") == Some(player.as_str())
        && event.text("breakthrough") == Some(BREAKTHROUGH)
        && is_mentak(state, player)
        && crate::breakthroughs::holds(state, player, BREAKTHROUGH)
}

/// The Table's Grace, on gaining it: "If you have the Cruiser II unit upgrade technology, flip this
/// card and place it on top of cruiser II." The Cruiser IIs already on the board become Corsairs
/// (`technology::apply_unit_upgrades`); later placements and research ask [`unit_form_override`].
/// Announced as `BREAKTHROUGH_GAINED` at the next step (`Game::announce_gains`).
fn tables_grace(owner_name: &str, seat: &PlayerId) -> Ability {
    let owner = seat.clone();
    let condition_owner = seat.clone();
    Ability::stateful(
        format!("breakthrough:{owner_name}:{BREAKTHROUGH}:BREAKTHROUGH_GAINED:after"),
        seat.clone(),
        "BREAKTHROUGH_GAINED",
        Relation::After,
        Arc::new(move |event, _resolver, context| {
            if grace_gained(event, context.state, &owner) {
                crate::technology::apply_unit_upgrades(
                    context.state,
                    context.content,
                    context.sources,
                    &owner,
                );
            }
            Ok(())
        }),
    )
    .with_stateful_condition(Arc::new(move |event, _, context| {
        grace_gained(event, context.state, &condition_owner)
    }))
}

// -- Ambush --------------------------------------------------------------------------------------

/// Ambush: "At the start of a space combat: You may roll 1 die for each of up to 2 of your
/// cruisers or destroyers in the system. For each result equal to or greater than that ship's
/// combat value, produce 1 hit; your opponent must assign it to 1 of their ships."
///
/// One question: which (up to 2) ships roll, offered as every combination of the distinct types
/// present. The die is rolled per ship against its printed combat value.
fn produced_hits(
    context: &mut TimingContext<'_>,
    site: &HitSite<'_>,
) -> Result<ProducedHits, IllegalChoice> {
    let player = site.player;
    if site.moment != CombatMoment::CombatStart || !is_mentak(context.state, player) {
        return Ok(ProducedHits::NONE);
    }
    let types = ti4_content::units::catalogue(context.content, context.sources);
    // Cruisers and destroyers by type, in type-id order, with how many of each.
    let mut kinds: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    for unit in crate::combat::ships_of(
        context.state,
        context.content,
        context.sources,
        player,
        site.system,
    ) {
        let eligible = types.get(unit.type_id.as_str()).is_some_and(|kind| {
            matches!(kind.base_type(), "cruiser" | "destroyer") && kind.combat_hits_on().is_some()
        });
        if eligible {
            *kinds.entry(unit.type_id.to_string()).or_insert(0) += 1;
        }
    }
    if kinds.is_empty() {
        return Ok(ProducedHits::NONE);
    }
    let kinds: Vec<(String, usize)> = kinds.into_iter().collect();
    let mut groups: Vec<Vec<String>> = Vec::new();
    for (i, (first, first_count)) in kinds.iter().enumerate() {
        groups.push(vec![first.clone()]);
        if *first_count >= 2 {
            groups.push(vec![first.clone(), first.clone()]);
        }
        for (second, _) in &kinds[i + 1..] {
            groups.push(vec![first.clone(), second.clone()]);
        }
    }
    let options = groups
        .iter()
        .map(|group| {
            ChoiceOption::labelled(
                group.join("+"),
                "ambush_roll",
                format!("Ambush: roll for {}", group.join(" and ")),
            )
        })
        .collect();
    let answer = ask_one(
        context,
        player,
        "Ambush: choose up to 2 cruisers or destroyers to roll".to_owned(),
        "ambush",
        "ambush_roll",
        options,
    )?;
    if answer.is_decline() {
        return Ok(ProducedHits::NONE);
    }
    let Some(group) = groups.iter().find(|group| group.join("+") == answer.id) else {
        return Ok(ProducedHits::NONE);
    };
    let mut hits = 0;
    for kind in group {
        let value = types
            .get(kind.as_str())
            .and_then(ti4_content::units::UnitType::combat_hits_on)
            .and_then(|value| u32::try_from(value).ok())
            .unwrap_or(u32::MAX);
        let roll = context
            .dice
            .roll_by(context.rng, 1, "ambush", Some(value), player);
        hits += roll.hits();
    }
    Ok(ProducedHits {
        any_ship: hits,
        ..ProducedHits::NONE
    })
}

// -- Timing abilities ----------------------------------------------------------------------------

fn timing_abilities(state: &GameState, owner_name: &str, seat: &PlayerId) -> Vec<Ability> {
    let mut abilities = vec![
        pillage(owner_name, seat, "TRADE_GOODS_GAINED"),
        pillage(owner_name, seat, "TRANSACTION_RESOLVED"),
        native_suffi_an_window(owner_name, seat),
        tables_grace(owner_name, seat),
        salvage_trade_good(owner_name, seat),
        salvage_ship(owner_name, seat),
        promise_of_protection(owner_name, seat),
        commander(owner_name, seat),
        hero_start(owner_name, seat),
        hero_destroyed(owner_name, seat),
        hero_end(owner_name, seat),
    ];
    if state
        .player(seat)
        .is_some_and(|source| source.leaders.contains_key(&LeaderId::new(AGENT)))
    {
        for candidate in &state.players {
            if candidate.id != *seat
                && candidate
                    .leaders
                    .contains_key(&LeaderId::new("yssarilagent"))
            {
                abilities.push(borrowed_suffi_an(owner_name, seat, &candidate.id));
            }
        }
    }
    abilities
}

// -- Pillage and Suffi An ------------------------------------------------------------------------

/// Whether `target`'s faceup Promise of Protection bars `owner` from pillaging them.
fn protected(state: &GameState, owner: &PlayerId, target: &PlayerId) -> bool {
    let note = crate::promissory::note_id("pop", &crate::promissory::faction_name(state, owner));
    state.promissory_notes.get(&note) == Some(target) && state.promissory_faceup.contains(&note)
}

/// Prefix of the rows `supply::stage_event` keeps (private to `supply.rs`; a test below fails if it
/// drifts).
const STAGED_EVENT_PREFIX: &str = "private:#staged:event:";

/// Whether a `TRADE_GOODS_GAINED` from a transaction is staged for `player` and not yet announced.
///
/// A resolved transaction stages one such event per party that gained goods (21.5), and
/// `TRANSACTION_RESOLVED` is emitted before they are flushed. The two are one moment, so
/// `TRANSACTION_RESOLVED` leaves a party with a pending gain to that event and offers Pillage once.
fn transaction_gain_pending(state: &GameState, player: &PlayerId) -> bool {
    state
        .faction_marks
        .range(STAGED_EVENT_PREFIX.to_owned()..)
        .take_while(|(key, _)| key.starts_with(STAGED_EVENT_PREFIX))
        .filter_map(|(_, text)| serde_json::from_str::<serde_json::Value>(text).ok())
        .any(|record| {
            record.get("type").and_then(serde_json::Value::as_str) == Some("TRADE_GOODS_GAINED")
                && record
                    .pointer("/payload/player")
                    .and_then(serde_json::Value::as_str)
                    == Some(player.as_str())
                && record
                    .pointer("/payload/source")
                    .and_then(serde_json::Value::as_str)
                    == Some("transaction")
        })
}

/// Neighbours `owner` may pillage now, out of those the event concerns.
///
/// `TRADE_GOODS_GAINED` names the player who gained; `TRANSACTION_RESOLVED` names `proposer` and
/// `partner`.
fn pillage_targets(
    event: &Event,
    state: &GameState,
    galaxy: Option<&ti4_content::galaxy::Galaxy>,
    owner: &PlayerId,
) -> Vec<PlayerId> {
    if !is_mentak(state, owner) {
        return Vec::new();
    }
    let concerned: Vec<PlayerId> = match event.event_type.as_str() {
        "TRADE_GOODS_GAINED" => event
            .text("player")
            .map(PlayerId::new)
            .into_iter()
            .collect(),
        "TRANSACTION_RESOLVED" => ["proposer", "partner"]
            .into_iter()
            .filter_map(|key| event.text(key).map(PlayerId::new))
            .collect(),
        _ => Vec::new(),
    };
    let Some(galaxy) = galaxy else {
        return Vec::new();
    };
    concerned
        .into_iter()
        .filter(|target| target != owner)
        .filter(|target| {
            event.event_type != "TRANSACTION_RESOLVED" || !transaction_gain_pending(state, target)
        })
        .filter(|target| crate::transactions::are_neighbours(state, galaxy, owner, target))
        .filter(|target| {
            state
                .player(target)
                .is_some_and(|seat| seat.trade_goods >= 3)
        })
        .filter(|target| !protected(state, owner, target))
        .collect()
}

/// Pillage: "After 1 of your neighbors gains trade goods or resolves a transaction: If they have 3
/// or more trade goods, you may take 1 of their trade goods or commodities."
///
/// The taken commodity becomes a trade good in the Mentak player's hands (21.5: a commodity
/// becomes a trade good when it changes hands). Suffi An's window is "after the Pillage faction
/// ability is used against another player"; no event marks that, so the agent is offered here,
/// right after the take.
fn pillage(owner_name: &str, seat: &PlayerId, event_type: &str) -> Ability {
    let owner = seat.clone();
    let condition_owner = seat.clone();
    Ability::stateful(
        format!("ability:{owner_name}:pillage:{event_type}:after"),
        seat.clone(),
        event_type,
        Relation::After,
        Arc::new(move |event, resolver, context| pillage_effect(event, resolver, context, &owner)),
    )
    .with_optional(true)
    .with_stateful_condition(Arc::new(move |event, _, context| {
        !pillage_targets(event, context.state, context.galaxy, &condition_owner).is_empty()
    }))
}

fn pillage_effect(
    event: &Event,
    resolver: &mut crate::timing::Resolver,
    context: &mut TimingContext<'_>,
    owner: &PlayerId,
) -> Result<(), TimingError> {
    let before_log = context.table.log.clone();
    let targets = pillage_targets(event, context.state, context.galaxy, owner);
    let mut worded = Vec::new();
    for target in &targets {
        let from =
            |template: &str| Phrase::of(template).with("target", target, Subject::seat(target));
        worded.push((
            format!("trade_good|{target}"),
            from("take 1 trade good from {target}"),
        ));
        if context
            .state
            .player(target)
            .is_some_and(|seat| seat.commodities > 0)
        {
            worded.push((
                format!("commodity|{target}"),
                from("take 1 commodity from {target}"),
            ));
        }
    }
    let mut options = crate::phrases::options("pillage", &worded);
    let chosen = match options.len() {
        0 => return Ok(()),
        1 => options.remove(0),
        _ => ask_phrased(
            context,
            owner,
            &Phrase::of("Pillage: take 1 trade good or commodity"),
            "pillage",
            "pillage_take",
            "pillage",
            &worded,
        )
        .map_err(illegal)?,
    };
    let Some((kind, target)) = chosen.id.split_once('|') else {
        return Ok(());
    };
    let target = PlayerId::new(target);
    if !targets.contains(&target) {
        return Ok(());
    }
    let before = context.state.clone();
    let before_timing = resolver.checkpoint();
    let before_sequence = context.event_sequence.clone();
    let before_dice = context.dice.clone();
    let before_rng = context.rng.clone();
    let taken = {
        let Some(seat) = context.state.player_mut(&target) else {
            return Ok(());
        };
        if kind == "commodity" && seat.commodities > 0 {
            seat.commodities -= 1;
            true
        } else if kind == "trade_good" && seat.trade_goods > 0 {
            seat.trade_goods -= 1;
            true
        } else {
            false
        }
    };
    if !taken {
        return Ok(());
    }
    crate::supply::gain_trade_goods_staged(context.state, owner, 1, "pillage");
    let result = (|| {
        // Native games without a legal Ssruu copy retain their existing event stream.
        let has_copy = context.state.players.iter().any(|seat| {
            super::hooks_cards::borrowable_agents(context.state, context.content, &seat.id)
                .iter()
                .any(|(_, agent)| agent.as_str() == AGENT)
        });
        if has_copy {
            let event = context.event_sequence.next(
                "PILLAGE_USED",
                std::collections::BTreeMap::from([
                    ("player".to_owned(), owner.to_string().into()),
                    ("target".to_owned(), target.to_string().into()),
                ]),
            )?;
            resolver.emit_with_context(context, event, |_, _| {})?;
        } else {
            suffi_an(context, owner, &target)?;
        }
        Ok(())
    })();
    if let Err(error) = result {
        *context.state = before;
        *context.event_sequence = before_sequence;
        *context.dice = before_dice;
        *context.rng = before_rng;
        resolver.restore(before_timing);
        context.table.log = before_log;
        return Err(error);
    }
    Ok(())
}

/// Suffi An: "After the Pillage faction ability is used against another player: You may exhaust
/// this card: if you do, you and that player each draw 1 action card."
fn suffi_an(
    context: &mut TimingContext<'_>,
    owner: &PlayerId,
    target: &PlayerId,
) -> Result<(), TimingError> {
    if leader_status(context.state, owner, AGENT) != Some(LeaderStatus::Readied)
        || context.state.action_card_deck.is_empty()
    {
        return Ok(());
    }
    let answer = ask_one(
        context,
        owner,
        format!("Suffi An: exhaust to draw 1 action card each with {target}"),
        "mentakagent",
        "suffi_an",
        vec![ChoiceOption::labelled(
            "exhaust",
            "agent",
            "exhaust Suffi An: you and the pillaged player each draw 1 action card",
        )],
    )
    .map_err(illegal)?;
    if answer.is_decline() || !crate::leaders::exhaust(context.state, owner, &LeaderId::new(AGENT))
    {
        return Ok(());
    }
    for player in [owner, target] {
        crate::action_cards::draw(context.state, context.content, context.table, player, 1)
            .map_err(illegal)?;
    }
    Ok(())
}

/// In games with a copy, native and copied text share the same optional resolver window.
fn native_suffi_an_window(owner_name: &str, seat: &PlayerId) -> Ability {
    let (owner, condition_owner) = (seat.clone(), seat.clone());
    Ability::stateful(
        format!("leader:{owner_name}:mentakagent:PILLAGE_USED:after"),
        seat.clone(),
        "PILLAGE_USED",
        Relation::After,
        Arc::new(move |event, _, context| {
            let Some(target) = event.text("target").map(PlayerId::new) else {
                return Ok(());
            };
            suffi_an(context, &owner, &target)
        }),
    )
    .with_stateful_condition(Arc::new(move |event, _, context| {
        leader_status(context.state, &condition_owner, AGENT) == Some(LeaderStatus::Readied)
            && event.text("player") == Some(condition_owner.as_str())
            && event.text("target").is_some()
            && !context.state.action_card_deck.is_empty()
    }))
}

/// Ssruu's copied Suffi An reacts to a successful Pillage, not merely a goods gain.
fn borrowed_suffi_an(owner_name: &str, source: &PlayerId, borrower: &PlayerId) -> Ability {
    let (cs, cb) = (source.clone(), borrower.clone());
    let (es, eb) = (source.clone(), borrower.clone());
    Ability::stateful(
        format!("leader:{owner_name}:{source}:yssarilagent:mentakagent:PILLAGE_USED:after"),
        borrower.clone(),
        "PILLAGE_USED",
        Relation::After,
        Arc::new(move |event, _, context| {
            let Some(target) = event.text("target").map(PlayerId::new) else {
                return Ok(());
            };
            if !can_copy_suffi_an(context.state, context.content, &es, &eb) {
                return Ok(());
            }
            if !crate::leaders::exhaust(context.state, &eb, &LeaderId::new("yssarilagent")) {
                return Ok(());
            }
            for player in [&eb, &target] {
                crate::action_cards::draw(context.state, context.content, context.table, player, 1)
                    .map_err(illegal)?;
            }
            super::hooks_cards::borrowed_agent_used(context, &eb, &LeaderId::new(AGENT));
            Ok(())
        }),
    )
    .with_optional(true)
    .with_stateful_condition(Arc::new(move |event, _, context| {
        event
            .text("target")
            .is_some_and(|target| context.state.player(&PlayerId::new(target)).is_some())
            && !context.state.action_card_deck.is_empty()
            && can_copy_suffi_an(context.state, context.content, &cs, &cb)
    }))
}

fn can_copy_suffi_an(
    state: &GameState,
    content: &ContentStore,
    source: &PlayerId,
    borrower: &PlayerId,
) -> bool {
    super::hooks_cards::borrowable_agents(state, content, borrower)
        .iter()
        .any(|(owner, agent)| owner == source && agent.as_str() == AGENT)
}

// -- Salvage Operations --------------------------------------------------------------------------

/// The player's destroyed-ship base types from a `SPACE_COMBAT_ENDED` event, in sorted order.
fn destroyed_ship_types(event: &Event, content: &ContentStore, sources: SourceSet) -> Vec<String> {
    let types = ti4_content::units::catalogue(content, sources);
    let mut found = std::collections::BTreeSet::new();
    if let Some(list) = event
        .payload
        .get("destroyed")
        .and_then(serde_json::Value::as_array)
    {
        for entry in list {
            if let Some(kind) = entry
                .get("unit")
                .and_then(serde_json::Value::as_str)
                .and_then(|id| types.get(id))
                && kind.is_ship()
            {
                found.insert(kind.base_type().to_owned());
            }
        }
    }
    found.into_iter().collect()
}

fn took_part(event: &Event, player: &PlayerId) -> bool {
    event.text("attacker") == Some(player.as_str())
        || event.text("defender") == Some(player.as_str())
}

/// Salvage Operations, the mandatory half: "After you win or lose a space combat, gain 1 trade
/// good". A draw (no winner) is neither a win nor a loss.
fn salvage_trade_good(owner_name: &str, seat: &PlayerId) -> Ability {
    let owner = seat.clone();
    let condition_owner = seat.clone();
    Ability::stateful(
        format!("technology:{owner_name}:so:SPACE_COMBAT_ENDED:after"),
        seat.clone(),
        "SPACE_COMBAT_ENDED",
        Relation::After,
        Arc::new(move |_event, _resolver, context| {
            crate::supply::gain_trade_goods_staged(context.state, &owner, 1, "mentak");
            Ok(())
        }),
    )
    .with_stateful_condition(Arc::new(move |event, _, context| {
        owns_technology(context.state, &condition_owner, "so")
            && took_part(event, &condition_owner)
            && event.text("winner").is_some()
    }))
}

/// The ship base types Salvage Operations could produce now, with their unit ids and costs.
fn salvage_options(
    event: &Event,
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    owner: &PlayerId,
) -> Vec<(String, String, i64)> {
    if !owns_technology(state, owner, "so")
        || event.text("winner") != Some(owner.as_str())
        || !took_part(event, owner)
    {
        return Vec::new();
    }
    let destroyed = destroyed_ship_types(event, content, sources);
    let types = ti4_content::units::catalogue(content, sources);
    crate::production::buildable_for(state, content, sources, owner)
        .into_iter()
        .filter_map(|id| {
            let kind = types.get(id.as_str())?;
            let base = kind.base_type().to_owned();
            // Resources: a fighter's half cost still takes 1 to make (67.2).
            #[allow(
                clippy::cast_possible_truncation,
                reason = "unit costs are small non-negative integers or halves"
            )]
            let cost = kind.cost().ceil() as i64;
            (kind.is_ship()
                && destroyed.contains(&base)
                && crate::supply::allowed(
                    state,
                    content,
                    sources,
                    owner,
                    &ti4_model::id::UnitTypeId::new(id.clone()),
                    1,
                ) > 0
                && crate::payment::affordable(
                    state,
                    content,
                    sources,
                    owner,
                    cost,
                    Spend::Resources,
                ))
            .then_some((base, id, cost))
        })
        .collect()
}

/// Salvage Operations, the optional half: "if you won the combat, you may also produce 1 ship in
/// that system of any ship type that was destroyed during the combat."
///
/// Produced here means the unit's cost is paid in resources and the ship placed in the system; the
/// system's PRODUCTION value is not read (the card names no producer). See the evidence.
fn salvage_ship(owner_name: &str, seat: &PlayerId) -> Ability {
    let owner = seat.clone();
    let condition_owner = seat.clone();
    Ability::stateful(
        format!("technology:{owner_name}:so:SPACE_COMBAT_ENDED:produce"),
        seat.clone(),
        "SPACE_COMBAT_ENDED",
        Relation::After,
        Arc::new(move |event, _resolver, context| salvage_effect(event, context, &owner)),
    )
    .with_optional(true)
    .with_stateful_condition(Arc::new(move |event, _, context| {
        !salvage_options(
            event,
            context.state,
            context.content,
            context.sources,
            &condition_owner,
        )
        .is_empty()
    }))
}

fn salvage_effect(
    event: &Event,
    context: &mut TimingContext<'_>,
    owner: &PlayerId,
) -> Result<(), TimingError> {
    let found = salvage_options(
        event,
        context.state,
        context.content,
        context.sources,
        owner,
    );
    let Some(system) = event.text("system").map(SystemId::new) else {
        return Ok(());
    };
    if found.is_empty() {
        return Ok(());
    }
    let options = found
        .iter()
        .map(|(base, id, cost)| {
            ChoiceOption::labelled(
                base.clone(),
                "salvage",
                format!("produce 1 {id} for {cost} resources"),
            )
            .with("cost", *cost)
        })
        .collect();
    let answer = ask_one(
        context,
        owner,
        "Salvage Operations: produce 1 ship of a type destroyed in the combat".to_owned(),
        "so",
        "salvage_produce",
        options,
    )
    .map_err(illegal)?;
    let Some((base, _, cost)) = found.iter().find(|(base, _, _)| *base == answer.id) else {
        return Ok(());
    };
    let before = context.state.clone();
    match crate::production::pay_seeing(
        context.state,
        context.content,
        context.sources,
        context.galaxy,
        context.table,
        owner,
        *cost,
        Spend::Resources,
    ) {
        Ok(true) => {}
        Ok(false) => {
            *context.state = before;
            return Ok(());
        }
        Err(error) => {
            *context.state = before;
            return Err(illegal(error));
        }
    }
    if crate::action_cards::place_units_counted(context, owner, &system, None, base, 1) == 0 {
        *context.state = before;
    } else {
        // `place_units_counted` ignores the unit-form override; bring the new ship to its form.
        crate::technology::apply_unit_upgrades(
            context.state,
            context.content,
            context.sources,
            owner,
        );
    }
    Ok(())
}

// -- Promise of Protection -----------------------------------------------------------------------

/// Promise of Protection, the return: "If you activate a system that contains 1 or more of the
/// Mentak player's units, return this card to the Mentak player." The bar on Pillage is in
/// [`pillage_targets`].
///
/// The note reaches a play area the moment it changes hands (`promissory::take`, 69.3), so the
/// printed "ACTION: Place this card faceup" step is not a separate action here (see the evidence).
fn pop_to_return(event: &Event, state: &GameState, holder: &PlayerId) -> Vec<String> {
    if event.text("player") != Some(holder.as_str()) {
        return Vec::new();
    }
    let Some(system) = event.text("system").map(SystemId::new) else {
        return Vec::new();
    };
    let board = state.system_state(&system);
    state
        .promissory_notes
        .iter()
        .filter(|(note, who)| *who == holder && crate::promissory::alias_of(note) == "pop")
        .filter(|(note, _)| state.promissory_faceup.contains(*note))
        .filter(|(note, _)| {
            crate::promissory::owner_of(note)
                .and_then(|name| crate::promissory::seat_of(state, &name))
                .is_some_and(|mentak| {
                    mentak != *holder
                        && (board.units.iter().any(|unit| unit.owner == mentak)
                            || board
                                .planet_units
                                .values()
                                .flatten()
                                .any(|unit| unit.owner == mentak))
                })
        })
        .map(|(note, _)| note.clone())
        .collect()
}

fn promise_of_protection(owner_name: &str, seat: &PlayerId) -> Ability {
    let holder = seat.clone();
    let condition_holder = seat.clone();
    Ability::stateful(
        format!("promissory:{owner_name}:pop:SYSTEM_ACTIVATED:after"),
        seat.clone(),
        "SYSTEM_ACTIVATED",
        Relation::After,
        Arc::new(move |event, _resolver, context| {
            for note in pop_to_return(event, context.state, &holder) {
                crate::promissory::give_back(context.state, &note);
            }
            Ok(())
        }),
    )
    .with_stateful_condition(Arc::new(move |event, _, context| {
        !pop_to_return(event, context.state, &condition_holder).is_empty()
    }))
}

// -- S'ula Mentarion -----------------------------------------------------------------------------

/// S'ula Mentarion, unlock: "Have 4 cruisers on the game board." A Corsair is a cruiser.
fn commander_unlocked(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    _galaxy: Option<&ti4_content::galaxy::Galaxy>,
    player: &PlayerId,
    leader: &LeaderId,
) -> Option<bool> {
    if leader.as_str() != COMMANDER {
        return None;
    }
    let types = ti4_content::units::catalogue(content, sources);
    let cruisers = state
        .board
        .values()
        .flat_map(|board| board.units.iter())
        .filter(|unit| &unit.owner == player)
        .filter(|unit| {
            types
                .get(unit.type_id.as_str())
                .is_some_and(|kind| kind.base_type() == "cruiser")
        })
        .count();
    Some(cruisers >= 4)
}

/// Promissory notes in `player`'s hand: held and not faceup in a play area, plus their own
/// Support for the Throne while it is still home (it lives in `support_holders`, not the note map).
///
/// Only notes `receiver` may be given (Mahact's Hubris refuses an Alliance).
fn notes_in_hand(state: &GameState, player: &PlayerId, receiver: &PlayerId) -> Vec<String> {
    let mut notes: Vec<String> = crate::promissory::held_by(state, player)
        .into_iter()
        .filter(|note| !state.promissory_faceup.contains(note))
        .filter(|note| crate::promissory::may_receive(state, receiver, note))
        .collect();
    notes.extend(crate::promissory::available_support(state, player));
    notes
}

/// Opponents of the winner (from the `SPACE_COMBAT_WON` event) with a note in hand.
fn commander_targets(event: &Event, state: &GameState, owner: &PlayerId) -> Vec<PlayerId> {
    if event.text("player") != Some(owner.as_str())
        || !crate::promissory::has_commander_ability(state, owner, COMMANDER)
    {
        return Vec::new();
    }
    event
        .payload
        .get("opponents")
        .and_then(serde_json::Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(serde_json::Value::as_str)
                .map(PlayerId::new)
                .filter(|opponent| {
                    opponent != owner && !notes_in_hand(state, opponent, owner).is_empty()
                })
                .collect()
        })
        .unwrap_or_default()
}

/// S'ula Mentarion: "After you win a space combat: You may force your opponent to give you 1
/// promissory note from their hand." The opponent chooses which note; with several opponents the
/// commander's owner chooses which of them gives. The take is not announced: it would show which
/// note moved.
fn commander(owner_name: &str, seat: &PlayerId) -> Ability {
    let owner = seat.clone();
    let condition_owner = seat.clone();
    Ability::stateful(
        format!("leader:{owner_name}:{COMMANDER}:SPACE_COMBAT_WON:after"),
        seat.clone(),
        "SPACE_COMBAT_WON",
        Relation::After,
        Arc::new(move |event, _resolver, context| {
            let targets = commander_targets(event, context.state, &owner);
            let target =
                match targets.len() {
                    0 => return Ok(()),
                    1 => targets[0].clone(),
                    _ => {
                        let worded: Vec<(String, Worded)> =
                            targets
                                .iter()
                                .map(|who| {
                                    (
                                        who.to_string(),
                                        Phrase::of("force {who} to give you a promissory note")
                                            .with("who", who, Subject::seat(who)),
                                    )
                                })
                                .collect();
                        let answer = ask_phrased(
                            context,
                            &owner,
                            &Phrase::of("S'ula Mentarion: choose an opponent"),
                            COMMANDER,
                            "commander_opponent",
                            "commander",
                            &worded,
                        )
                        .map_err(illegal)?;
                        PlayerId::new(answer.id)
                    }
                };
            if !targets.contains(&target) {
                return Ok(());
            }
            let hand = notes_in_hand(context.state, &target, &owner);
            let worded: Vec<(String, Worded)> = hand
                .iter()
                .map(|note| {
                    (
                        note.clone(),
                        Phrase::of("{note}").with(
                            "note",
                            note,
                            Subject::note(context.content, note),
                        ),
                    )
                })
                .collect();
            let answer = ask_phrased(
                context,
                &target,
                &Phrase::of("Give {owner} 1 promissory note from your hand").with(
                    "owner",
                    &owner,
                    Subject::seat(&owner),
                ),
                COMMANDER,
                "commander_give_note",
                "give_note",
                &worded,
            )
            .map_err(illegal)?;
            if hand.contains(&answer.id) {
                if answer.id.starts_with(crate::promissory::SUPPORT_PREFIX) {
                    // Support scores a point on arrival: the same path transactions use.
                    crate::promissory::receive(context.state, &owner, &answer.id);
                } else {
                    crate::promissory::take(context.state, context.content, &owner, &answer.id);
                }
            }
            Ok(())
        }),
    )
    .with_optional(true)
    .with_stateful_condition(Arc::new(move |event, _, context| {
        !commander_targets(event, context.state, &condition_owner).is_empty()
    }))
}

// -- Ipswitch, Loose Cannon ----------------------------------------------------------------------

fn hero_mark(system: &SystemId) -> String {
    format!("mentak:hero:{system}")
}

/// Ipswitch, the purge: "At the start of a space combat that you are participating in: You may
/// purge this card. If you do, for each other player's ship that is destroyed during this combat,
/// place 1 ship of that type from your reinforcements in the active system."
///
/// Purging records the combat's system in `faction_marks`; [`hero_destroyed`] places the ships as
/// they are destroyed and [`hero_end`] clears the record when the combat ends.
fn hero_start(owner_name: &str, seat: &PlayerId) -> Ability {
    let owner = seat.clone();
    let condition_owner = seat.clone();
    let ready = move |event: &Event, state: &GameState, who: &PlayerId| {
        took_part(event, who)
            && leader_status(state, who, HERO) == Some(LeaderStatus::Unlocked)
            && event.text("system").is_some()
    };
    Ability::stateful(
        format!("leader:{owner_name}:{HERO}:SPACE_COMBAT_STARTED:after"),
        seat.clone(),
        "SPACE_COMBAT_STARTED",
        Relation::After,
        Arc::new(move |event, _resolver, context| {
            if !ready(event, context.state, &owner) {
                return Ok(());
            }
            let Some(system) = event.text("system").map(SystemId::new) else {
                return Ok(());
            };
            if crate::leaders::purge(context.state, &owner, &LeaderId::new(HERO)) {
                context
                    .state
                    .faction_marks
                    .insert(hero_mark(&system), owner.to_string());
            }
            Ok(())
        }),
    )
    .with_optional(true)
    .with_stateful_condition(Arc::new(move |event, _, context| {
        ready(event, context.state, &condition_owner)
    }))
}

/// Ipswitch, the effect: one ship of the destroyed type per other player's ship destroyed.
fn hero_destroyed(owner_name: &str, seat: &PlayerId) -> Ability {
    let owner = seat.clone();
    let condition_owner = seat.clone();
    let live = |event: &Event, state: &GameState, who: &PlayerId| {
        event
            .text("player")
            .is_some_and(|loser| loser != who.as_str())
            && event.text("system").is_some_and(|system| {
                state
                    .faction_marks
                    .get(&hero_mark(&SystemId::new(system)))
                    .is_some_and(|holder| holder == who.as_str())
            })
    };
    Ability::stateful(
        format!("leader:{owner_name}:{HERO}:SHIP_DESTROYED:after"),
        seat.clone(),
        "SHIP_DESTROYED",
        Relation::After,
        Arc::new(move |event, _resolver, context| {
            if !live(event, context.state, &owner) {
                return Ok(());
            }
            let (Some(system), Some(unit)) = (event.text("system"), event.text("unit")) else {
                return Ok(());
            };
            let Some(base) = base_type_of(context.content, context.sources, unit) else {
                return Ok(());
            };
            crate::action_cards::place_units_counted(
                context,
                &owner,
                &SystemId::new(system),
                None,
                &base,
                1,
            );
            // `place_units_counted` ignores the unit-form override; bring the new ship to its form.
            crate::technology::apply_unit_upgrades(
                context.state,
                context.content,
                context.sources,
                &owner,
            );
            Ok(())
        }),
    )
    .with_stateful_condition(Arc::new(move |event, _, context| {
        live(event, context.state, &condition_owner)
    }))
}

/// Ends Ipswitch's effect with the combat.
fn hero_end(owner_name: &str, seat: &PlayerId) -> Ability {
    let owner = seat.clone();
    let condition_owner = seat.clone();
    let live = |event: &Event, state: &GameState, who: &PlayerId| {
        event.text("system").is_some_and(|system| {
            state
                .faction_marks
                .get(&hero_mark(&SystemId::new(system)))
                .is_some_and(|holder| holder == who.as_str())
        })
    };
    Ability::stateful(
        format!("leader:{owner_name}:{HERO}:SPACE_COMBAT_ENDED:after"),
        seat.clone(),
        "SPACE_COMBAT_ENDED",
        Relation::After,
        Arc::new(move |event, _resolver, context| {
            if let Some(system) = event.text("system")
                && live(event, context.state, &owner)
            {
                context
                    .state
                    .faction_marks
                    .remove(&hero_mark(&SystemId::new(system)));
            }
            Ok(())
        }),
    )
    .with_stateful_condition(Arc::new(move |event, _, context| {
        live(event, context.state, &condition_owner)
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::choice::{Scripted, Table};
    use crate::fixtures::{armed_resolver, put, put_on_planet, seated_game, with_context};
    use ti4_model::content_types::DEFAULT;
    use ti4_model::id::TechnologyId;

    fn a() -> PlayerId {
        PlayerId::new("a")
    }
    fn b() -> PlayerId {
        PlayerId::new("b")
    }
    fn arena() -> (GameState, SystemId) {
        (
            seated_game(&[("a", "mentak"), ("b", "sol")], DEFAULT),
            SystemId::new("18"),
        )
    }
    fn scripted(answers: &[&str]) -> Table {
        Table::with_default(Box::new(Scripted::new(answers.iter().copied())))
    }
    fn give_tech(state: &mut GameState, player: &PlayerId, tech: &str) {
        state
            .player_mut(player)
            .unwrap()
            .technologies
            .insert(TechnologyId::new(tech));
    }
    fn count(state: &GameState, system: &SystemId, kind: &str, owner: &PlayerId) -> usize {
        state
            .system_state(system)
            .units
            .iter()
            .filter(|u| u.type_id.as_str() == kind && &u.owner == owner)
            .count()
    }
    fn set_status(state: &mut GameState, player: &PlayerId, leader: &str, status: LeaderStatus) {
        state
            .player_mut(player)
            .unwrap()
            .leaders
            .insert(LeaderId::new(leader), status);
    }
    /// Emit one typed event through an armed resolver; the error (e.g. an unscripted question)
    /// is returned so a "not offered" test can see none was asked.
    fn emit(
        state: &mut GameState,
        answers: &[&str],
        event_type: &str,
        payload: &[(&str, serde_json::Value)],
    ) -> Result<(), String> {
        emit_on(state, &mut scripted(answers), event_type, payload)
    }

    fn emit_on(
        state: &mut GameState,
        table: &mut crate::choice::Table,
        event_type: &str,
        payload: &[(&str, serde_json::Value)],
    ) -> Result<(), String> {
        let mut resolver = armed_resolver(state);
        let galaxy = crate::fixtures::plain_hub().galaxy;
        with_context(state, DEFAULT, Some(&galaxy), table, |ctx| {
            let payload = payload
                .iter()
                .map(|(key, value)| ((*key).to_owned(), value.clone()))
                .collect();
            let event = ctx
                .event_sequence
                .next(event_type, payload)
                .expect("event id");
            resolver
                .emit_with_context(ctx, event, |_, _| {})
                .map(|_| ())
                .map_err(|error| format!("{error:?}"))
        })
    }
    fn gained(who: &str) -> Vec<(&'static str, serde_json::Value)> {
        vec![
            ("player", who.into()),
            ("amount", 1.into()),
            ("source", "test".into()),
        ]
    }

    // -- Mirror Computing ------------------------------------------------------------------------

    #[test]
    fn mirror_computing_doubles_trade_goods_through_both_payment_paths() {
        let (mut state, _) = arena();
        let content = ContentStore::embedded();
        assert_eq!(crate::production::trade_good_worth(&state, &a()), 1);
        give_tech(&mut state, &a(), "mc");
        assert_eq!(crate::production::trade_good_worth(&state, &a()), 2);
        assert_eq!(
            crate::production::trade_good_worth(&state, &b()),
            1,
            "only the owner"
        );
        // The votes / objective-cost path reads the same hook.
        state.player_mut(&a()).unwrap().trade_goods = 2;
        let plans = crate::payment::plans(&state, content, DEFAULT, &a(), 3, Spend::Influence);
        assert!(plans.iter().all(|plan| plan.trade_goods <= 2));
    }

    // -- Fourth Moon and Moll Terminus -----------------------------------------------------------

    fn space_unit<'a>(player: &'a PlayerId, system: &'a SystemId) -> CombatUnit<'a> {
        CombatUnit {
            player,
            system: Some(system),
            planet: None,
            unit_type: "dreadnought",
            context: "space",
        }
    }

    #[test]
    fn fourth_moon_stops_other_players_ships_sustaining() {
        let (mut state, system) = arena();
        let content = ContentStore::embedded();
        let (a, b) = (a(), b());
        let sustain = |state: &GameState, who: &PlayerId, system: &SystemId| {
            super::super::hooks_combat::may_sustain(
                state,
                content,
                DEFAULT,
                &space_unit(who, system),
            )
        };
        assert!(sustain(&state, &b, &system), "no flagship, no bar");
        put(&mut state, &system, FLAGSHIP, &a, 1);
        assert!(
            !sustain(&state, &b, &system),
            "the other player's ships cannot"
        );
        assert!(sustain(&state, &a, &system), "its owner can");
        assert!(
            sustain(&state, &b, &SystemId::new("19")),
            "another system's ships are untouched"
        );
    }

    #[test]
    fn moll_terminus_stops_other_ground_forces_on_its_planet_sustaining() {
        let (mut state, _) = arena();
        let content = ContentStore::embedded();
        let (system, planet) = crate::fixtures::a_placed_planet();
        let (a, b) = (a(), b());
        let ask = |state: &GameState, who: &PlayerId, planet: &ti4_model::id::PlanetId| {
            super::super::hooks_ground::may_sustain(
                state,
                content,
                DEFAULT,
                &CombatUnit {
                    player: who,
                    system: Some(&system),
                    planet: Some(planet),
                    unit_type: "mech",
                    context: "ground",
                },
            )
        };
        assert!(ask(&state, &b, &planet), "no mech, no bar");
        put_on_planet(&mut state, &system, &planet, MECH, &a, 1);
        assert!(
            !ask(&state, &b, &planet),
            "the other player's ground forces cannot"
        );
        assert!(ask(&state, &a, &planet), "its owner's can");
        assert!(
            ask(&state, &b, &ti4_model::id::PlanetId::new("elsewhere")),
            "a mech reaches only its own planet"
        );
    }

    // -- Corsair ---------------------------------------------------------------------------------

    #[test]
    fn the_corsair_passes_blockades_only_towards_other_players_non_fighter_ships() {
        use crate::movement::{Board, MovementRules};
        let hub = crate::fixtures::plain_hub();
        let (near_a, near_b) = (hub.outer[0].clone(), hub.across(&hub.outer[0]));
        let mut state = seated_game(&[("a", "mentak"), ("b", "sol")], DEFAULT);
        let content = ContentStore::embedded();
        let centre = SystemId::new(hub.centre.as_str());
        let (origin, active) = (
            SystemId::new(near_a.as_str()),
            SystemId::new(near_b.as_str()),
        );
        put(&mut state, &origin, CORSAIR, &a(), 1);
        put(&mut state, &centre, "destroyer", &b(), 1);
        let route = |state: &GameState, ship: Option<&str>| {
            let board = Board::for_player(state, content, DEFAULT, &a());
            MovementRules::with_laws(&hub.galaxy, content, DEFAULT, &near_b, board, Some(state))
                .path_from_ship(&near_a, 2, ship)
        };
        assert!(
            route(&state, Some(CORSAIR)).is_none(),
            "the active system holds no other player's ship"
        );
        put(&mut state, &active, "fighter", &b(), 2);
        assert!(
            route(&state, Some(CORSAIR)).is_none(),
            "fighters do not count"
        );
        put(&mut state, &active, "destroyer", &b(), 1);
        assert!(route(&state, Some(CORSAIR)).is_some(), "now it passes");
        assert!(
            route(&state, Some("cruiser")).is_none(),
            "an ordinary cruiser does not"
        );
    }

    // -- The Table's Grace ----------------------------------------------------------------------

    fn grace_game() -> GameState {
        let (mut state, _) = arena();
        give_tech(&mut state, &a(), "cr2");
        state.player_mut(&a()).unwrap().breakthrough =
            Some(ti4_model::id::BreakthroughId::new("mentakbt"));
        state
    }

    #[test]
    fn the_tables_grace_makes_the_corsair_the_cruiser_ii() {
        let content = ContentStore::embedded();
        let mut state = grace_game();
        let built = crate::production::buildable_for(&state, content, DEFAULT, &a());
        assert!(built.iter().any(|id| id == CORSAIR), "{built:?}");
        assert!(!built.iter().any(|id| id == "cruiser2"));
        // Ships already on the board are flipped with it.
        let system = SystemId::new("18");
        put(&mut state, &system, "cruiser2", &a(), 1);
        put(&mut state, &system, "cruiser2", &b(), 1);
        crate::technology::apply_unit_upgrades(&mut state, content, DEFAULT, &a());
        assert_eq!(count(&state, &system, CORSAIR, &a()), 1);
        assert_eq!(
            count(&state, &system, "cruiser2", &b()),
            1,
            "others keep theirs"
        );
    }

    #[test]
    fn gaining_the_tables_grace_flips_the_cruiser_iis_already_on_the_board() {
        let (mut state, system) = arena();
        give_tech(&mut state, &a(), "cr2");
        put(&mut state, &system, "cruiser2", &a(), 2);
        put(&mut state, &system, "cruiser2", &b(), 1);
        let gained = |who: &str| vec![("player", who.into()), ("breakthrough", "mentakbt".into())];
        // Announced before the seat holds it, or for another seat: nothing changes.
        emit(&mut state, &[], "BREAKTHROUGH_GAINED", &gained("a")).unwrap();
        assert_eq!(count(&state, &system, "cruiser2", &a()), 2);
        state.player_mut(&a()).unwrap().breakthrough =
            Some(ti4_model::id::BreakthroughId::new("mentakbt"));
        emit(&mut state, &[], "BREAKTHROUGH_GAINED", &gained("b")).unwrap();
        assert_eq!(count(&state, &system, "cruiser2", &a()), 2);
        emit(&mut state, &[], "BREAKTHROUGH_GAINED", &gained("a")).unwrap();
        assert_eq!(count(&state, &system, CORSAIR, &a()), 2);
        assert_eq!(count(&state, &system, "cruiser2", &a()), 0);
        assert_eq!(count(&state, &system, "cruiser2", &b()), 1);
    }

    #[test]
    fn the_tables_grace_does_nothing_without_cruiser_ii() {
        let (mut state, system) = arena();
        put(&mut state, &system, "cruiser", &a(), 1);
        state.player_mut(&a()).unwrap().breakthrough =
            Some(ti4_model::id::BreakthroughId::new("mentakbt"));
        let before = state.clone();
        emit(
            &mut state,
            &[],
            "BREAKTHROUGH_GAINED",
            &[("player", "a".into()), ("breakthrough", "mentakbt".into())],
        )
        .unwrap();
        assert_eq!(state, before);
    }

    #[test]
    fn the_tables_grace_needs_the_breakthrough_the_upgrade_and_a_mentak_seat() {
        let content = ContentStore::embedded();
        let override_for = |state: &GameState, who: &PlayerId| {
            super::super::hooks_strategy::unit_form_override(
                state, content, DEFAULT, who, "cruiser", "cruiser2",
            )
        };
        let state = grace_game();
        assert_eq!(override_for(&state, &a()), Some(UnitTypeId::new(CORSAIR)));
        assert_eq!(override_for(&state, &b()), None, "not another seat");
        let mut no_bt = grace_game();
        no_bt.player_mut(&a()).unwrap().breakthrough = None;
        assert_eq!(override_for(&no_bt, &a()), None);
        assert!(
            !crate::production::buildable_for(&no_bt, content, DEFAULT, &a())
                .iter()
                .any(|id| id == CORSAIR)
        );
        // Without Cruiser II the chosen unit is the plain cruiser: nothing to replace.
        let (mut no_cr2, _) = arena();
        no_cr2.player_mut(&a()).unwrap().breakthrough =
            Some(ti4_model::id::BreakthroughId::new("mentakbt"));
        assert!(
            !crate::production::buildable_for(&no_cr2, content, DEFAULT, &a())
                .iter()
                .any(|id| id == CORSAIR)
        );
    }

    // -- Ambush ----------------------------------------------------------------------------------

    fn ambush(
        state: &mut GameState,
        system: &SystemId,
        answers: &[&str],
        faces: &[u32],
    ) -> ProducedHits {
        let mut table = scripted(answers);
        let (a, b) = (a(), b());
        let site = HitSite {
            player: &a,
            opponent: &b,
            system,
            round: 1,
            moment: CombatMoment::CombatStart,
        };
        let mut dice = crate::dice::Dice::from_faces(faces.iter().copied());
        let mut rng = crate::rng::GameRng::new(0);
        let mut sequence = crate::event::EventSequence::new();
        let mut context = TimingContext {
            state,
            content: ContentStore::embedded(),
            sources: DEFAULT,
            table: &mut table,
            dice: &mut dice,
            rng: &mut rng,
            event_sequence: &mut sequence,
            galaxy: None,
        };
        super::super::hooks_combat::produced_hits(&mut context, &site).expect("legal")
    }

    #[test]
    fn ambush_rolls_each_chosen_ship_against_its_own_combat_value() {
        let (mut state, system) = arena();
        put(&mut state, &system, "cruiser", &a(), 1);
        put(&mut state, &system, "destroyer", &a(), 2);
        put(&mut state, &system, "dreadnought", &a(), 1);
        put(&mut state, &system, "carrier", &b(), 1);
        // Cruiser hits on 7, destroyer on 9: a 7 hits the cruiser's die and misses the destroyer's.
        let got = ambush(&mut state, &system, &["cruiser+destroyer"], &[7, 8]);
        assert_eq!(got.any_ship, 1);
        assert_eq!(got.total(), 1);
        let got = ambush(&mut state, &system, &["destroyer+destroyer"], &[9, 10]);
        assert_eq!(got.any_ship, 2);
        let got = ambush(&mut state, &system, &["cruiser"], &[6]);
        assert_eq!(got, ProducedHits::NONE, "a 6 misses a 7");
    }

    #[test]
    fn ambush_declined_unavailable_or_not_mentak_changes_nothing() {
        let (mut state, system) = arena();
        // No cruiser or destroyer: nothing is asked (an unscripted ask would be an error).
        put(&mut state, &system, "dreadnought", &a(), 1);
        assert_eq!(ambush(&mut state, &system, &[], &[]), ProducedHits::NONE);
        put(&mut state, &system, "cruiser", &a(), 1);
        assert_eq!(
            ambush(&mut state, &system, &["decline"], &[10]),
            ProducedHits::NONE
        );
        // Another faction's cruiser is not offered Ambush.
        let mut other = seated_game(&[("a", "sol"), ("b", "hacan")], DEFAULT);
        put(&mut other, &system, "cruiser", &a(), 1);
        assert_eq!(ambush(&mut other, &system, &[], &[]), ProducedHits::NONE);
    }

    // -- Pillage and Suffi An --------------------------------------------------------------------

    /// Mentak (a) and Sol (b) share a system, so they are neighbours.
    fn neighbours() -> (GameState, SystemId) {
        let (mut state, system) = arena();
        put(&mut state, &system, "destroyer", &a(), 1);
        put(&mut state, &system, "destroyer", &b(), 1);
        (state, system)
    }
    const PILLAGE_GAIN: &str = "ability:mentak:pillage:TRADE_GOODS_GAINED:after";
    const PILLAGE_TRADE: &str = "ability:mentak:pillage:TRANSACTION_RESOLVED:after";

    #[test]
    fn pillage_takes_a_trade_good_from_a_neighbour_with_three_or_more() {
        let (mut state, _) = neighbours();
        state.player_mut(&b()).unwrap().trade_goods = 3;
        state.player_mut(&a()).unwrap().trade_goods = 0;
        // No commodities to take: the only option is a trade good, so only the use/decline is asked.
        state.player_mut(&b()).unwrap().commodities = 0;
        emit(
            &mut state,
            &[PILLAGE_GAIN],
            "TRADE_GOODS_GAINED",
            &gained("b"),
        )
        .unwrap();
        assert_eq!(state.player(&b()).unwrap().trade_goods, 2);
        assert_eq!(state.player(&a()).unwrap().trade_goods, 1);
    }

    #[test]
    fn pillage_may_take_a_commodity_instead_and_it_becomes_a_trade_good() {
        let (mut state, _) = neighbours();
        state.player_mut(&b()).unwrap().trade_goods = 4;
        state.player_mut(&b()).unwrap().commodities = 2;
        emit(
            &mut state,
            &[PILLAGE_GAIN, "commodity|b"],
            "TRADE_GOODS_GAINED",
            &gained("b"),
        )
        .unwrap();
        assert_eq!(state.player(&b()).unwrap().commodities, 1);
        assert_eq!(state.player(&b()).unwrap().trade_goods, 4);
        assert_eq!(state.player(&a()).unwrap().trade_goods, 1);
        assert_eq!(state.player(&a()).unwrap().commodities, 0);
    }

    #[test]
    fn pillage_also_follows_a_resolved_transaction() {
        let (mut state, _) = neighbours();
        state.player_mut(&b()).unwrap().trade_goods = 3;
        state.player_mut(&b()).unwrap().commodities = 0;
        let parties = [("proposer", "b".into()), ("partner", "c".into())];
        emit(
            &mut state,
            &[PILLAGE_TRADE],
            "TRANSACTION_RESOLVED",
            &parties,
        )
        .unwrap();
        assert_eq!(state.player(&b()).unwrap().trade_goods, 2);
        assert_eq!(state.player(&a()).unwrap().trade_goods, 1);
    }

    #[test]
    fn pillage_needs_three_goods_a_neighbour_and_no_promise_of_protection() {
        // Fewer than 3 trade goods: not offered (an unscripted question would be an error).
        let (mut state, _) = neighbours();
        state.player_mut(&b()).unwrap().trade_goods = 2;
        emit(&mut state, &[], "TRADE_GOODS_GAINED", &gained("b")).unwrap();
        assert_eq!(state.player(&b()).unwrap().trade_goods, 2);
        // Two systems apart (opposite sides of a hub) are not neighbours; adjacent ones are.
        let hub = crate::fixtures::plain_hub();
        let place = |state: &mut GameState, b_at: &str| {
            state.board.clear();
            put(
                state,
                &SystemId::new(hub.outer[0].as_str()),
                "destroyer",
                &a(),
                1,
            );
            put(state, &SystemId::new(b_at), "destroyer", &b(), 1);
            state.player_mut(&b()).unwrap().trade_goods = 5;
            state.player_mut(&b()).unwrap().commodities = 0;
        };
        let (mut far, _) = arena();
        place(&mut far, &hub.across(&hub.outer[0]));
        emit(&mut far, &[], "TRADE_GOODS_GAINED", &gained("b")).unwrap();
        assert_eq!(far.player(&b()).unwrap().trade_goods, 5);
        let (mut near, _) = arena();
        place(&mut near, &hub.centre);
        emit(
            &mut near,
            &[PILLAGE_GAIN],
            "TRADE_GOODS_GAINED",
            &gained("b"),
        )
        .unwrap();
        assert_eq!(
            near.player(&b()).unwrap().trade_goods,
            4,
            "adjacent systems"
        );
        // The Mentak player's own gain is not a neighbour's.
        let (mut own, _) = neighbours();
        own.player_mut(&a()).unwrap().trade_goods = 5;
        emit(&mut own, &[], "TRADE_GOODS_GAINED", &gained("a")).unwrap();
        assert_eq!(own.player(&a()).unwrap().trade_goods, 5);
        // A faceup Promise of Protection bars it.
        let (mut state, _) = neighbours();
        state.player_mut(&b()).unwrap().trade_goods = 5;
        let note = crate::promissory::note_id("pop", "mentak");
        crate::promissory::take(&mut state, ContentStore::embedded(), &b(), &note);
        assert!(state.promissory_faceup.contains(&note), "a play-area note");
        emit(&mut state, &[], "TRADE_GOODS_GAINED", &gained("b")).unwrap();
        assert_eq!(state.player(&b()).unwrap().trade_goods, 5);
    }

    fn from_transaction(who: &str) -> Vec<(&'static str, serde_json::Value)> {
        vec![
            ("player", who.into()),
            ("amount", 1.into()),
            ("source", "transaction".into()),
        ]
    }

    #[test]
    fn the_staged_event_prefix_matches_supply() {
        let (mut state, _) = neighbours();
        crate::supply::note_trade_goods_gained(&mut state, &b(), 1, "transaction");
        assert!(transaction_gain_pending(&state, &b()));
        assert!(!transaction_gain_pending(&state, &a()));
        crate::supply::note_trade_goods_gained(&mut state, &a(), 1, "relic");
        assert!(!transaction_gain_pending(&state, &a()), "other sources");
    }

    #[test]
    fn a_transaction_that_moves_goods_offers_pillage_once() {
        let (mut state, _) = neighbours();
        state.player_mut(&b()).unwrap().trade_goods = 5;
        state.player_mut(&b()).unwrap().commodities = 0;
        // The deal stages the party's gain before TRANSACTION_RESOLVED is emitted.
        crate::supply::note_trade_goods_gained(&mut state, &b(), 1, "transaction");
        let parties = [("proposer", "b".into()), ("partner", "c".into())];
        // Not offered here (an unscripted question would be an error); the gain event offers it.
        emit(&mut state, &[], "TRANSACTION_RESOLVED", &parties).unwrap();
        assert_eq!(state.player(&b()).unwrap().trade_goods, 5);
        emit(
            &mut state,
            &[PILLAGE_GAIN],
            "TRADE_GOODS_GAINED",
            &from_transaction("b"),
        )
        .unwrap();
        assert_eq!(state.player(&b()).unwrap().trade_goods, 4);
        assert_eq!(state.player(&a()).unwrap().trade_goods, 1);
    }

    #[test]
    fn the_flush_after_a_transaction_resolves_pillage_exactly_once() {
        let (mut state, _) = neighbours();
        state.player_mut(&b()).unwrap().trade_goods = 5;
        state.player_mut(&b()).unwrap().commodities = 0;
        state.player_mut(&a()).unwrap().trade_goods = 0;
        crate::supply::note_trade_goods_gained(&mut state, &b(), 1, "transaction");
        assert_eq!(crate::supply::staged_events(&state), 1);
        let mut resolver = armed_resolver(&state);
        let galaxy = crate::fixtures::plain_hub().galaxy;
        let content = ContentStore::embedded();
        let mut sequence = crate::event::EventSequence::new();
        let mut dice = crate::dice::Dice::new();
        let mut rng = crate::rng::GameRng::new(0);
        // Only one answer is scripted: a second offer would fail the call.
        let mut table = scripted(&[PILLAGE_GAIN]);
        let mut ctx = crate::choice::Resolving {
            content,
            sources: DEFAULT,
            dice: &mut dice,
            rng: &mut rng,
            table: &mut table,
            timing: Some(crate::choice::TimingHandle {
                resolver: &mut resolver,
                sequence: &mut sequence,
                galaxy: Some(&galaxy),
            }),
        };
        // First the resolution (no staged flush yet), then the flush at the next step.
        crate::factions::hooks_economy::emit(
            &mut ctx,
            &mut state,
            "TRANSACTION_RESOLVED",
            crate::transactions::resolved_payload(&b(), &PlayerId::new("c")),
        );
        assert_eq!(state.player(&b()).unwrap().trade_goods, 5, "deferred");
        // The transaction's gain, then the Mentak player's own pillaged good (staged by the take).
        assert_eq!(crate::supply::flush_staged_events(&mut state, &mut ctx), 2);
        assert_eq!(crate::supply::staged_events(&state), 0);
        assert_eq!(state.player(&b()).unwrap().trade_goods, 4);
        assert_eq!(state.player(&a()).unwrap().trade_goods, 1);
    }

    struct CopySuffiDecider;
    impl crate::choice::Decider for CopySuffiDecider {
        fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
            if choice.prompt.starts_with("Suffi An:")
                || choice
                    .options
                    .iter()
                    .any(|o| o.id == "leader:mentak:mentakagent:PILLAGE_USED:after")
            {
                return Ok(choice
                    .options
                    .iter()
                    .find(|o| o.is_decline())
                    .unwrap()
                    .clone());
            }
            if let Some(option) = choice.options.iter().find(|o| {
                o.id.contains(":yssarilagent:mentakagent:PILLAGE_USED:after")
            }) {
                assert_eq!(choice.player, PlayerId::new("c"));
                return Ok(option.clone());
            }
            Ok(choice
                .options
                .iter()
                .find(|o| !o.is_decline())
                .unwrap()
                .clone())
        }
    }
    fn copied_pillage_game() -> GameState {
        let mut state = seated_game(&[("a", "mentak"), ("b", "sol"), ("c", "yssaril")], DEFAULT);
        let system = SystemId::new("18");
        put(&mut state, &system, "destroyer", &a(), 1);
        put(&mut state, &system, "destroyer", &b(), 1);
        state.player_mut(&b()).unwrap().trade_goods = 3;
        state.player_mut(&b()).unwrap().commodities = 0;
        state
    }
    fn emit_copied_pillage(
        state: &mut GameState,
        resolver: &mut crate::timing::Resolver,
    ) -> Result<(), TimingError> {
        let mut table = Table::with_default(Box::new(CopySuffiDecider));
        emit_copied_pillage_with_table(state, resolver, &mut table)
    }
    fn emit_copied_pillage_with_table(
        state: &mut GameState,
        resolver: &mut crate::timing::Resolver,
        table: &mut Table,
    ) -> Result<(), TimingError> {
        let galaxy = crate::fixtures::plain_hub().galaxy;
        with_context(state, DEFAULT, Some(&galaxy), table, |ctx| {
            let event = ctx
                .event_sequence
                .next(
                    "TRADE_GOODS_GAINED",
                    gained("b")
                        .into_iter()
                        .map(|(k, v)| (k.to_owned(), v))
                        .collect(),
                )
                .unwrap();
            resolver
                .emit_with_context(ctx, event, |_, _| {})
                .map(|_| ())
        })
    }
    #[test]
    fn native_and_copied_suffi_an_compete_in_normal_player_order() {
        let mut state = copied_pillage_game();
        state.action_card_deck.truncate(2);
        let borrower = PlayerId::new("c");
        let before_a = state.player(&a()).unwrap().action_cards.len();
        let before_c = state.player(&borrower).unwrap().action_cards.len();
        let mut resolver = armed_resolver(&state);
        resolver.set_active_player(Some(borrower.clone()));
        emit_copied_pillage_with_table(&mut state, &mut resolver, &mut Table::new()).unwrap();
        assert_eq!(state.player(&a()).unwrap().action_cards.len(), before_a);
        assert_eq!(
            state.player(&borrower).unwrap().action_cards.len(),
            before_c + 1
        );
        assert_eq!(
            leader_status(&state, &a(), AGENT),
            Some(LeaderStatus::Readied)
        );
        assert_eq!(
            leader_status(&state, &borrower, "yssarilagent"),
            Some(LeaderStatus::Exhausted)
        );
    }
    struct FailCopiedDiscard;
    impl crate::choice::Decider for FailCopiedDiscard {
        fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
            if choice.player == PlayerId::new("c")
                && choice.prompt.to_lowercase().contains("discard")
            {
                return Err(IllegalChoice::NotOffered {
                    player: choice.player.clone(),
                    chosen: "invalid-discard".to_owned(),
                    offered: choice.options.iter().map(|o| o.id.clone()).collect(),
                });
            }
            Ok(choice
                .options
                .iter()
                .find(|o| !o.is_decline())
                .unwrap()
                .clone())
        }
    }
    #[test]
    fn failed_copied_suffi_draw_restores_pillage_state_and_decision_log() {
        let mut state = copied_pillage_game();
        set_status(&mut state, &a(), AGENT, LeaderStatus::Exhausted);
        let before = state.clone();
        let mut resolver = armed_resolver(&state);
        let mut table = Table::with_default(Box::new(FailCopiedDiscard));
        let log_before = table.log.clone();
        assert!(emit_copied_pillage_with_table(&mut state, &mut resolver, &mut table).is_err());
        assert_eq!(
            state, before,
            "copied draw failure restores Pillage and both hands/cards"
        );
        // The outer Pillage selection happened before the callback transaction. It may remain;
        // every nested native/copy/draw selection must be removed.
        assert!(table.log.len() <= log_before.len() + 1);
        assert!(
            !table
                .log
                .records
                .iter()
                .any(|entry| format!("{entry:?}").contains("yssarilagent:mentakagent"))
        );
    }

    #[test]
    fn ssruu_copies_suffi_an_only_after_actual_pillage_for_both_source_statuses() {
        for status in [LeaderStatus::Readied, LeaderStatus::Exhausted] {
            let mut state = copied_pillage_game();
            set_status(&mut state, &a(), AGENT, status);
            let hand_a = state.player(&a()).unwrap().action_cards.len();
            let hand_b = state.player(&b()).unwrap().action_cards.len();
            let hand_c = state
                .player(&PlayerId::new("c"))
                .unwrap()
                .action_cards
                .len();
            let mut resolver = armed_resolver(&state);
            emit_copied_pillage(&mut state, &mut resolver).unwrap();
            assert_eq!(state.player(&b()).unwrap().trade_goods, 2);
            assert_eq!(state.player(&a()).unwrap().action_cards.len(), hand_a);
            assert_eq!(state.player(&b()).unwrap().action_cards.len(), hand_b + 1);
            assert_eq!(
                state
                    .player(&PlayerId::new("c"))
                    .unwrap()
                    .action_cards
                    .len(),
                hand_c + 1
            );
            assert_eq!(leader_status(&state, &a(), AGENT), Some(status));
            assert_eq!(
                leader_status(&state, &PlayerId::new("c"), "yssarilagent"),
                Some(LeaderStatus::Exhausted)
            );
        }
    }
    #[test]
    fn copied_suffi_an_has_live_readiness_and_does_not_trigger_without_pillage() {
        let mut state = copied_pillage_game();
        let borrower = PlayerId::new("c");
        set_status(&mut state, &a(), AGENT, LeaderStatus::Exhausted);
        set_status(
            &mut state,
            &borrower,
            "yssarilagent",
            LeaderStatus::Exhausted,
        );
        let mut resolver = armed_resolver(&state);
        set_status(&mut state, &borrower, "yssarilagent", LeaderStatus::Readied);
        state.player_mut(&b()).unwrap().trade_goods = 2;
        let before = state.clone();
        emit_copied_pillage(&mut state, &mut resolver).unwrap();
        assert_eq!(
            state, before,
            "a goods gain without legal Pillage must not draw cards"
        );
        state.player_mut(&b()).unwrap().trade_goods = 3;
        emit_copied_pillage(&mut state, &mut resolver).unwrap();
        assert_eq!(
            leader_status(&state, &borrower, "yssarilagent"),
            Some(LeaderStatus::Exhausted)
        );
    }

    #[test]
    fn suffi_an_draws_an_action_card_each_after_pillage() {
        let (mut state, _) = neighbours();
        state.player_mut(&b()).unwrap().trade_goods = 3;
        state.player_mut(&b()).unwrap().commodities = 0;
        let (hand_a, hand_b) = (
            state.player(&a()).unwrap().action_cards.len(),
            state.player(&b()).unwrap().action_cards.len(),
        );
        emit(
            &mut state,
            &[PILLAGE_GAIN, "exhaust"],
            "TRADE_GOODS_GAINED",
            &gained("b"),
        )
        .unwrap();
        assert_eq!(
            state
                .player(&a())
                .unwrap()
                .leaders
                .get(&LeaderId::new(AGENT)),
            Some(&LeaderStatus::Exhausted)
        );
        assert_eq!(state.player(&a()).unwrap().action_cards.len(), hand_a + 1);
        assert_eq!(state.player(&b()).unwrap().action_cards.len(), hand_b + 1);
        // Declined, the agent stays ready and nobody draws.
        let (mut state, _) = neighbours();
        state.player_mut(&b()).unwrap().trade_goods = 3;
        state.player_mut(&b()).unwrap().commodities = 0;
        emit(
            &mut state,
            &[PILLAGE_GAIN, "decline"],
            "TRADE_GOODS_GAINED",
            &gained("b"),
        )
        .unwrap();
        assert_eq!(
            state
                .player(&a())
                .unwrap()
                .leaders
                .get(&LeaderId::new(AGENT)),
            Some(&LeaderStatus::Readied)
        );
        assert_eq!(state.player(&a()).unwrap().action_cards.len(), hand_a);
    }

    // -- Salvage Operations ----------------------------------------------------------------------

    fn ended(system: &SystemId, winner: Option<&str>) -> Vec<(&'static str, serde_json::Value)> {
        let mut payload = vec![
            ("system", system.to_string().into()),
            ("attacker", "a".into()),
            ("defender", "b".into()),
            (
                "destroyed",
                serde_json::json!([{"player": "b", "unit": "destroyer"}, {"player": "a", "unit": "cruiser"}]),
            ),
        ];
        if let Some(winner) = winner {
            payload.push(("winner", winner.into()));
        }
        payload
    }

    #[test]
    fn salvage_gains_a_trade_good_for_winning_or_losing_and_may_rebuild_when_winning() {
        let (mut state, system) = arena();
        give_tech(&mut state, &a(), "so");
        state.player_mut(&a()).unwrap().trade_goods = 1;
        let cruisers_before = count(&state, &system, "cruiser", &a());
        emit(
            &mut state,
            &[
                "technology:mentak:so:SPACE_COMBAT_ENDED:produce",
                "cruiser",
                "trade_good",
                "trade_good",
            ],
            "SPACE_COMBAT_ENDED",
            &ended(&system, Some("a")),
        )
        .unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(count(&state, &system, "cruiser", &a()), cruisers_before + 1);
        // 1 held + 1 gained would be 2; the cruiser's cost was paid, in part from trade goods.
        assert!(state.player(&a()).unwrap().trade_goods < 2);

        // Losing still gains the trade good but offers no ship.
        let (mut lost, system) = arena();
        give_tech(&mut lost, &a(), "so");
        lost.player_mut(&a()).unwrap().trade_goods = 0;
        emit(
            &mut lost,
            &[],
            "SPACE_COMBAT_ENDED",
            &ended(&system, Some("b")),
        )
        .unwrap();
        assert_eq!(lost.player(&a()).unwrap().trade_goods, 1);
        assert_eq!(count(&lost, &system, "cruiser", &a()), 0);
    }

    #[test]
    fn salvage_does_nothing_without_the_technology_or_after_a_draw() {
        let (mut state, system) = arena();
        state.player_mut(&a()).unwrap().trade_goods = 0;
        emit(
            &mut state,
            &[],
            "SPACE_COMBAT_ENDED",
            &ended(&system, Some("a")),
        )
        .unwrap();
        assert_eq!(state.player(&a()).unwrap().trade_goods, 0, "no technology");
        give_tech(&mut state, &a(), "so");
        emit(&mut state, &[], "SPACE_COMBAT_ENDED", &ended(&system, None)).unwrap();
        assert_eq!(
            state.player(&a()).unwrap().trade_goods,
            0,
            "a draw is neither"
        );
    }

    // -- Promise of Protection -------------------------------------------------------------------

    #[test]
    fn promise_of_protection_returns_when_its_holder_activates_a_mentak_system() {
        let (mut state, system) = arena();
        let note = crate::promissory::note_id("pop", "mentak");
        crate::promissory::take(&mut state, ContentStore::embedded(), &b(), &note);
        let activate = |system: &SystemId| {
            vec![
                ("player", "b".into()),
                ("system", system.to_string().into()),
            ]
        };
        // A system with none of the Mentak player's units: the note stays.
        emit(&mut state, &[], "SYSTEM_ACTIVATED", &activate(&system)).unwrap();
        assert_eq!(state.promissory_notes.get(&note), Some(&b()));
        // The Mentak player's units in that system: it goes home.
        put(&mut state, &system, "destroyer", &a(), 1);
        emit(&mut state, &[], "SYSTEM_ACTIVATED", &activate(&system)).unwrap();
        assert_eq!(state.promissory_notes.get(&note), Some(&a()));
        assert!(!state.promissory_faceup.contains(&note));
        // The Mentak player activating their own system never returns anything.
        crate::promissory::take(&mut state, ContentStore::embedded(), &b(), &note);
        let own = vec![
            ("player", "a".into()),
            ("system", system.to_string().into()),
        ];
        emit(&mut state, &[], "SYSTEM_ACTIVATED", &own).unwrap();
        assert_eq!(state.promissory_notes.get(&note), Some(&b()));
    }

    // -- S'ula Mentarion -------------------------------------------------------------------------

    #[test]
    fn the_commander_unlocks_with_four_cruisers_on_the_board() {
        let (mut state, system) = arena();
        let content = ContentStore::embedded();
        let leader = LeaderId::new(COMMANDER);
        let check =
            |state: &GameState| commander_unlocked(state, content, DEFAULT, None, &a(), &leader);
        // Start from an empty board so the count is the test's own.
        state.board.clear();
        assert_eq!(check(&state), Some(false));
        put(&mut state, &system, "cruiser", &a(), 3);
        assert_eq!(check(&state), Some(false), "three is not four");
        put(&mut state, &system, "cruiser", &b(), 4);
        assert_eq!(
            check(&state),
            Some(false),
            "other players' cruisers do not count"
        );
        put(&mut state, &SystemId::new("19"), CORSAIR, &a(), 1);
        assert_eq!(
            check(&state),
            Some(true),
            "a Corsair in another system is a cruiser"
        );
        assert_eq!(
            commander_unlocked(
                &state,
                content,
                DEFAULT,
                None,
                &a(),
                &LeaderId::new("solcommander")
            ),
            None
        );
    }

    fn won(system: &SystemId) -> Vec<(&'static str, serde_json::Value)> {
        vec![
            ("player", "a".into()),
            ("system", system.to_string().into()),
            ("opponents", serde_json::json!(["b"])),
        ]
    }
    const COMMANDER_WINDOW: &str = "leader:mentak:mentakcommander:SPACE_COMBAT_WON:after";

    #[test]
    fn the_commander_takes_a_note_the_opponent_chooses_from_their_hand() {
        let (mut state, system) = arena();
        crate::promissory::deal(&mut state, ContentStore::embedded(), DEFAULT);
        set_status(&mut state, &a(), COMMANDER, LeaderStatus::Unlocked);
        let note = crate::promissory::note_id("ps", "sol");
        assert_eq!(state.promissory_notes.get(&note), Some(&b()));
        emit(
            &mut state,
            &[COMMANDER_WINDOW, &note],
            "SPACE_COMBAT_WON",
            &won(&system),
        )
        .unwrap();
        assert_eq!(state.promissory_notes.get(&note), Some(&a()));
    }

    /// The opponent's question names each note and the seat that takes it (`details.phrases`).
    #[test]
    fn the_commanders_question_names_the_notes_and_the_taker() {
        let (mut state, system) = arena();
        let content = ContentStore::embedded();
        crate::promissory::deal(&mut state, content, DEFAULT);
        set_status(&mut state, &a(), COMMANDER, LeaderStatus::Unlocked);
        let (mut table, seen) = crate::phrases::capturing(&[COMMANDER_WINDOW]);
        emit_on(&mut state, &mut table, "SPACE_COMBAT_WON", &won(&system)).unwrap();
        let seen = seen.borrow();
        let choice = seen
            .iter()
            .find(|choice| choice.prompt.starts_with("Give "))
            .expect("asked");
        let hints = crate::phrases::phrased(choice);
        let prompt = hints.prompt.expect("the prompt names the taker");
        assert_eq!(prompt.refs["owner"], crate::phrases::Subject::seat(&a()));
        let note = crate::promissory::note_id("ps", "sol");
        assert_eq!(hints.options[&note].text, "{note}");
        assert_eq!(
            hints.options[&note].refs["note"],
            crate::phrases::Subject::note(content, &note)
        );
    }

    /// Each option of Pillage names the seat that it takes from.
    #[test]
    fn pillage_names_the_seat_of_each_option() {
        let (mut state, _) = neighbours();
        state.player_mut(&b()).unwrap().trade_goods = 3;
        state.player_mut(&b()).unwrap().commodities = 2;
        let (mut table, seen) = crate::phrases::capturing(&[PILLAGE_GAIN]);
        emit_on(&mut state, &mut table, "TRADE_GOODS_GAINED", &gained("b")).unwrap();
        let seen = seen.borrow();
        let choice = seen
            .iter()
            .find(|choice| choice.prompt.starts_with("Pillage:"))
            .expect("asked");
        let hints = crate::phrases::phrased(choice);
        for id in ["trade_good|b", "commodity|b"] {
            assert_eq!(
                hints.options[id].refs["target"],
                crate::phrases::Subject::seat(&b())
            );
        }
    }

    #[test]
    fn ownerless_commander_grant_transfers_the_losers_chosen_note() {
        let (mut state, system) = arena();
        crate::promissory::deal(&mut state, ContentStore::embedded(), DEFAULT);
        state.player_mut(&a()).unwrap().faction = ti4_model::id::FactionId::new("yin");
        state
            .player_mut(&a())
            .unwrap()
            .leaders
            .remove(&LeaderId::new(COMMANDER));
        assert!(crate::promissory::grant_commander_ability(
            &mut state,
            ContentStore::embedded(),
            &a(),
            COMMANDER
        ));
        let note = crate::promissory::note_id("ps", "sol");
        assert_eq!(state.promissory_notes.get(&note), Some(&b()));
        emit(
            &mut state,
            &["leader:yin:mentakcommander:SPACE_COMBAT_WON:after", &note],
            "SPACE_COMBAT_WON",
            &won(&system),
        )
        .unwrap();
        assert_eq!(state.promissory_notes.get(&note), Some(&a()));
        assert_eq!(leader_status(&state, &a(), COMMANDER), None);
    }

    #[test]
    fn the_commander_can_take_support_for_the_throne_and_scores_it() {
        let (mut state, system) = arena();
        crate::promissory::deal(&mut state, ContentStore::embedded(), DEFAULT);
        for note in crate::promissory::held_by(&state, &b()) {
            state.promissory_faceup.insert(note);
        }
        set_status(&mut state, &a(), COMMANDER, LeaderStatus::Unlocked);
        let support = crate::promissory::support("sol");
        let vp = state.player(&a()).unwrap().victory_points;
        emit(
            &mut state,
            &[COMMANDER_WINDOW, &support],
            "SPACE_COMBAT_WON",
            &won(&system),
        )
        .unwrap();
        assert_eq!(state.support_holders.get(&b()), Some(&a()));
        assert_eq!(state.player(&a()).unwrap().victory_points, vp + 1);
    }

    #[test]
    fn the_commander_is_not_offered_locked_or_when_the_opponent_has_no_note_in_hand() {
        let (mut state, system) = arena();
        // Still locked.
        emit(&mut state, &[], "SPACE_COMBAT_WON", &won(&system)).unwrap();
        // Unlocked, but every note b holds is faceup in a play area.
        crate::promissory::deal(&mut state, ContentStore::embedded(), DEFAULT);
        set_status(&mut state, &a(), COMMANDER, LeaderStatus::Unlocked);
        for note in crate::promissory::held_by(&state, &b()) {
            state.promissory_faceup.insert(note);
        }
        // Their Support is lent out too.
        state.support_holders.insert(b(), PlayerId::new("c"));
        let before = state.clone();
        emit(&mut state, &[], "SPACE_COMBAT_WON", &won(&system)).unwrap();
        assert_eq!(state, before);
    }

    // -- Ipswitch --------------------------------------------------------------------------------

    fn started(system: &SystemId) -> Vec<(&'static str, serde_json::Value)> {
        vec![
            ("system", system.to_string().into()),
            ("attacker", "a".into()),
            ("defender", "b".into()),
            ("player", "a".into()),
        ]
    }
    fn destroyed(
        system: &SystemId,
        owner: &str,
        unit: &str,
    ) -> Vec<(&'static str, serde_json::Value)> {
        vec![
            ("system", system.to_string().into()),
            ("player", owner.into()),
            ("unit", unit.into()),
            ("last", false.into()),
        ]
    }

    #[test]
    fn ipswitch_replaces_each_other_players_destroyed_ship_for_the_combat() {
        let (mut state, system) = arena();
        set_status(&mut state, &a(), HERO, LeaderStatus::Unlocked);
        emit(
            &mut state,
            &["leader:mentak:mentakhero:SPACE_COMBAT_STARTED:after"],
            "SPACE_COMBAT_STARTED",
            &started(&system),
        )
        .unwrap();
        assert_eq!(
            state
                .player(&a())
                .unwrap()
                .leaders
                .get(&LeaderId::new(HERO)),
            Some(&LeaderStatus::Purged)
        );
        emit(
            &mut state,
            &[],
            "SHIP_DESTROYED",
            &destroyed(&system, "b", "destroyer"),
        )
        .unwrap();
        assert_eq!(
            count(&state, &system, "destroyer", &a()),
            1,
            "one for theirs"
        );
        // Mentak's own loss is not replaced.
        emit(
            &mut state,
            &[],
            "SHIP_DESTROYED",
            &destroyed(&system, "a", "cruiser"),
        )
        .unwrap();
        assert_eq!(count(&state, &system, "cruiser", &a()), 0);
        // The combat ends: later destructions are not replaced.
        emit(
            &mut state,
            &[],
            "SPACE_COMBAT_ENDED",
            &ended(&system, Some("a")),
        )
        .unwrap();
        assert!(!state.faction_marks.contains_key(&hero_mark(&system)));
        emit(
            &mut state,
            &[],
            "SHIP_DESTROYED",
            &destroyed(&system, "b", "destroyer"),
        )
        .unwrap();
        assert_eq!(count(&state, &system, "destroyer", &a()), 1);
    }

    #[test]
    fn ipswitch_is_not_offered_locked_declined_or_to_a_non_participant() {
        let (mut state, system) = arena();
        // Locked.
        emit(&mut state, &[], "SPACE_COMBAT_STARTED", &started(&system)).unwrap();
        set_status(&mut state, &a(), HERO, LeaderStatus::Unlocked);
        // Declined: not purged, nothing replaced afterwards.
        emit(
            &mut state,
            &["decline"],
            "SPACE_COMBAT_STARTED",
            &started(&system),
        )
        .unwrap();
        assert_eq!(
            state
                .player(&a())
                .unwrap()
                .leaders
                .get(&LeaderId::new(HERO)),
            Some(&LeaderStatus::Unlocked)
        );
        emit(
            &mut state,
            &[],
            "SHIP_DESTROYED",
            &destroyed(&system, "b", "destroyer"),
        )
        .unwrap();
        assert_eq!(count(&state, &system, "destroyer", &a()), 0);
        // Not a participant.
        let mut elsewhere = vec![("system", system.to_string().into())];
        elsewhere.push(("attacker", "b".into()));
        elsewhere.push(("defender", "c".into()));
        emit(&mut state, &[], "SPACE_COMBAT_STARTED", &elsewhere).unwrap();
    }

    // -- No Mentak seat --------------------------------------------------------------------------

    #[test]
    fn a_game_without_a_mentak_seat_is_offered_no_mentak_ability() {
        let mut state = seated_game(&[("a", "sol"), ("b", "hacan")], DEFAULT);
        let system = SystemId::new("18");
        put(&mut state, &system, "destroyer", &a(), 1);
        put(&mut state, &system, "destroyer", &b(), 1);
        state.player_mut(&b()).unwrap().trade_goods = 5;
        // Even a stray "mc" or leader row on a non-Mentak seat changes nothing but mc itself.
        let before = state.clone();
        let events: Vec<(&str, Vec<(&str, serde_json::Value)>)> = vec![
            ("TRADE_GOODS_GAINED", gained("b")),
            ("TRANSACTION_RESOLVED", vec![]),
            (
                "BREAKTHROUGH_GAINED",
                vec![("player", "a".into()), ("breakthrough", "mentakbt".into())],
            ),
            ("SPACE_COMBAT_STARTED", started(&system)),
            ("SPACE_COMBAT_ENDED", ended(&system, Some("a"))),
            ("SPACE_COMBAT_WON", won(&system)),
            (
                "SYSTEM_ACTIVATED",
                vec![("player", "a".into()), ("system", "18".into())],
            ),
            ("SHIP_DESTROYED", destroyed(&system, "b", "destroyer")),
        ];
        for (event, payload) in events {
            // An unscripted question would be an error, so Ok(()) shows nothing was asked.
            emit(&mut state, &[], event, &payload)
                .unwrap_or_else(|error| panic!("{event}: {error}"));
        }
        assert_eq!(state, before);
        let content = ContentStore::embedded();
        assert_eq!(crate::production::trade_good_worth(&state, &a()), 1);
        assert!(super::super::hooks_combat::may_sustain(
            &state,
            content,
            DEFAULT,
            &space_unit(&b(), &system)
        ));
    }

    #[test]
    fn a_nekro_flagship_with_the_mentak_z_token_stops_other_players_ships_sustaining() {
        let content = ContentStore::embedded();
        let (_, system) = arena();
        let barred = |lent: &[&str]| {
            let mut state = crate::fixtures::nekro_with_z(&[("a", "nekro"), ("b", "sol")], lent);
            put(&mut state, &system, "nekro_flagship", &a(), 1);
            !super::super::hooks_combat::may_sustain(
                &state,
                content,
                DEFAULT,
                &space_unit(&b(), &system),
            )
        };
        assert!(!barred(&[]), "off by default");
        assert!(barred(&["mentak"]));
    }
}
