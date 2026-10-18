//! The Yin Brotherhood (`yin`). See `factions/mod.rs` for the contract and
//! `plans/BASE_FACTIONS_PLAN_2026-10-02.md` for scope.
//!
//! Implemented: Indoctrination (with the Moyin's Ashes DEPLOY), Devotion, Impulse Core, Yin
//! Spinner, Van Hauge, Greyfire Mutagen, Brother Milor, Brother Omar, and Dannel of the Tenth.
//! Yin Ascendant grants canonical unused-faction commander rights; unsupported granted effects
//! remain an explicit scope limitation, so its asset is not claimed complete.

use std::sync::Arc;

use ti4_content::ContentStore;
use ti4_model::content_types::{ContentType, DEFAULT, SourceSet};
use ti4_model::id::{LeaderId, PlanetId, PlayerId, SystemId, TechnologyId, UnitTypeId};
use ti4_model::state::{GameState, LeaderStatus};
use ti4_model::units::Unit;

use super::hooks_combat::{CombatHooks, CombatMoment, HitSite, ProducedHits};
use super::hooks_strategy::{ResearchWaiver, ResearchWaiverPayment, StrategyHooks};
use super::{FactionModule, Hooks};
use crate::choice::{Choice, ChoiceOption, IllegalChoice, Resolving, TimingHandle};
use crate::decision_context::{DecisionContext, DecisionSource};
use crate::event::Event;
use crate::production::Spend;
use crate::timing::{Ability, Relation, TimingContext, TimingError};

/// What this faction implements; grows package by package.
pub const MODULE: FactionModule = FactionModule {
    alias: "yin",
    abilities: &["indoctrination", "devotion"],
    technologies: &["yso", "ic"],
    units: &["yin_flagship", "yin_mech"],
    promissory: &["greyfire"],
    leaders: &["yinagent", "yincommander", "yinhero"],
    // Yin Ascendant: every commander in its pool has a live, rights-aware handler
    // (`yinbt_pool_has_a_live_handler_for_every_commander`).
    breakthroughs: &["yinbt"],
    hooks: Hooks {
        commander_unlocked: Some(commander_unlocked),
        leader_action: Some(leader_action),
        use_leader: Some(use_leader),
        use_leader_timed: Some(use_leader_timed),
        strategy: StrategyHooks {
            extra_prerequisite_colours: Some(extra_prerequisite_colours),
            research_waiver_offer: Some(research_waiver_offer),
            research_waiver_paid: Some(research_waiver_paid),
            ..StrategyHooks::NONE
        },
        timing_abilities: Some(timing_abilities),
        combat: CombatHooks {
            produced_hits: Some(produced_hits),
            ..CombatHooks::NONE
        },
        ..Hooks::NONE
    },
};

/// Influence Indoctrination costs.
const INDOCTRINATION_COST: i64 = 2;
/// With Moyin's Ashes: "you may spend 1 additional influence".
const INDOCTRINATION_MECH_COST: i64 = INDOCTRINATION_COST + 1;

// -- small helpers -------------------------------------------------------------------------------

fn is_yin(state: &GameState, player: &PlayerId) -> bool {
    state
        .player(player)
        .is_some_and(|seat| seat.faction.as_str() == "yin")
}

/// Owns the technology, or the Nekro's Valefar Assimilator carries its text.
fn owns_technology(state: &GameState, player: &PlayerId, technology: &str) -> bool {
    crate::technology::has_technology_text(state, player, technology)
}

/// The key recording that `player` has used one of their faction abilities (Brother Omar's
/// unlock: "Use one of your faction abilities"). Both of the faction's abilities set it.
fn ability_mark(player: &PlayerId) -> String {
    format!("yin:ability_used:{player}")
}

fn record_ability_used(state: &mut GameState, player: &PlayerId) {
    state
        .faction_marks
        .insert(ability_mark(player), "1".to_owned());
}

/// The unit type `player` places for a base type (upgrade, else faction unit, else generic).
fn placed_type(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    base_type: &str,
) -> Option<UnitTypeId> {
    let seat = state.player(player)?;
    let faction = seat.faction.to_string();
    let held: Vec<String> = seat
        .technologies
        .iter()
        .map(|tech| tech.as_str().to_owned())
        .collect();
    ti4_content::units::unlocked_upgrade(content, sources, base_type, &faction, &held)
        .or_else(|| ti4_content::units::faction_unit(content, &faction, base_type, sources))
        .or_else(|| {
            ti4_content::units::catalogue(content, sources)
                .get(base_type)
                .copied()
        })
        .map(|kind| UnitTypeId::new(kind.id().to_owned()))
}

/// Whether `player`'s reinforcements hold a unit of this base type (31.4).
fn in_reinforcements(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    base_type: &str,
) -> bool {
    placed_type(state, content, sources, player, base_type)
        .is_some_and(|id| crate::supply::allowed(state, content, sources, player, &id, 1) > 0)
}

/// `player`'s units of a base type on a planet.
fn on_planet(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    system: &SystemId,
    planet: &PlanetId,
    base_type: Option<&str>,
) -> usize {
    let types = ti4_content::units::catalogue(content, sources);
    state
        .board
        .get(system)
        .and_then(|board| board.planet_units.get(planet))
        .map_or(0, |units| {
            units
                .iter()
                .filter(|unit| &unit.owner == player)
                .filter(|unit| {
                    types.get(unit.type_id.as_str()).is_some_and(|kind| {
                        base_type
                            .map_or_else(|| kind.is_ground_force(), |base| kind.base_type() == base)
                    })
                })
                .count()
        })
}

/// The planet and the opponent of `owner` in a `GROUND_COMBAT_STARTED` event, if `owner` is a side.
fn ground_combat_of(event: &Event, owner: &PlayerId) -> Option<(SystemId, PlanetId, PlayerId)> {
    let attacker = event.text("attacker")?;
    let defender = event.text("defender")?;
    let opponent = if owner.as_str() == attacker {
        defender
    } else if owner.as_str() == defender {
        attacker
    } else {
        return None;
    };
    Some((
        SystemId::new(event.text("system")?),
        PlanetId::new(event.text("planet")?),
        PlayerId::new(opponent),
    ))
}

/// Put a question to one player: the given options plus a decline.
fn ask_one(
    context: &mut TimingContext<'_>,
    player: &PlayerId,
    prompt: String,
    ability: &str,
    subtype: &str,
    mut options: Vec<ChoiceOption>,
) -> Result<ChoiceOption, IllegalChoice> {
    options.push(ChoiceOption::decline());
    let choice = Choice::new(player.clone(), prompt, options).contextualized(DecisionContext::new(
        player.clone(),
        DecisionSource::FactionAbility(ability.to_owned()),
        subtype,
        context.state.phase,
        context.state.round,
    ));
    context.ask_seeing(&choice)
}

// -- Devotion and Impulse Core -------------------------------------------------------------------

/// Devotion: "After each space combat round: You may destroy 1 of your cruisers or destroyers in
/// the active system to produce 1 hit and assign it to 1 of your opponent's ships in that system."
///
/// Impulse Core: "At the start of a space combat, you may destroy 1 of your cruisers or destroyers
/// in the active system to produce 1 hit against your opponent's ships; that hit must be assigned
/// by your opponent to 1 of their non-fighters ships if able."
///
/// Both are one sacrifice for one hit, differing only in the moment and in who aims the hit.
fn produced_hits(
    context: &mut TimingContext<'_>,
    site: &HitSite<'_>,
) -> Result<ProducedHits, IllegalChoice> {
    let player = site.player;
    let (ability, subtype) = match site.moment {
        CombatMoment::RoundEnded if is_yin(context.state, player) => {
            ("devotion", "devotion_sacrifice")
        }
        CombatMoment::CombatStart if owns_technology(context.state, player, "ic") => {
            ("ic", "impulse_core_sacrifice")
        }
        _ => return Ok(ProducedHits::NONE),
    };
    let ships = |context: &TimingContext<'_>, owner: &PlayerId| {
        crate::combat::ships_of(
            context.state,
            context.content,
            context.sources,
            owner,
            site.system,
        )
    };
    // Nothing to hit, nothing to offer.
    if ships(context, site.opponent).is_empty() {
        return Ok(ProducedHits::NONE);
    }
    let types = ti4_content::units::catalogue(context.content, context.sources);
    let mut victims: Vec<Unit> = Vec::new();
    for unit in ships(context, player) {
        let base = types
            .get(unit.type_id.as_str())
            .map(ti4_content::units::UnitType::base_type);
        if matches!(base, Some("cruiser" | "destroyer"))
            && !victims.iter().any(|seen| seen.type_id == unit.type_id)
        {
            victims.push(unit);
        }
    }
    if victims.is_empty() {
        return Ok(ProducedHits::NONE);
    }
    let options = victims
        .iter()
        .map(|unit| {
            ChoiceOption::labelled(
                unit.type_id.to_string(),
                "destroy_own_ship",
                format!("destroy 1 {} to produce 1 hit", unit.type_id),
            )
        })
        .collect();
    let answer = ask_one(
        context,
        player,
        format!(
            "{}: destroy a cruiser or destroyer to produce 1 hit",
            if ability == "ic" {
                "Impulse Core"
            } else {
                "Devotion"
            }
        ),
        ability,
        subtype,
        options,
    )?;
    if answer.is_decline() {
        return Ok(ProducedHits::NONE);
    }
    let Some(victim) = victims
        .iter()
        .find(|unit| unit.type_id.as_str() == answer.id)
    else {
        return Ok(ProducedHits::NONE);
    };
    let cause = if ability == "devotion" {
        "faction_ability:devotion"
    } else {
        "technology:impulse_core"
    };
    if crate::combat::destroy_units_with_context(
        context.state,
        context.content,
        context.sources,
        player,
        site.system,
        std::slice::from_ref(victim),
        cause,
        // The sacrifice is paid at one of the combat's own moments ([`CombatMoment`]).
        true,
    ) == 0
    {
        return Ok(ProducedHits::NONE);
    }
    if ability == "devotion" {
        // A faction ability was used (Brother Omar's unlock); Impulse Core is a technology.
        record_ability_used(context.state, player);
        Ok(ProducedHits {
            producer_assigned: 1,
            ..ProducedHits::NONE
        })
    } else {
        Ok(ProducedHits {
            non_fighter: 1,
            ..ProducedHits::NONE
        })
    }
}

// -- Timing abilities ----------------------------------------------------------------------------

fn timing_abilities(state: &GameState, owner_name: &str, seat: &PlayerId) -> Vec<Ability> {
    let mut abilities = vec![
        indoctrination(owner_name, seat),
        greyfire(owner_name, seat),
        yin_spinner(owner_name, seat),
        van_hauge(owner_name, seat),
        brother_milor_ship(owner_name, seat),
        brother_milor_ground(owner_name, seat),
        yin_ascendant_gain(owner_name, seat),
        yin_ascendant_public_score(owner_name, seat),
    ];
    // Ssruu's listener is registered from the static seated roster. Whether a source is live is
    // checked on every event through borrowable_agents, so a Readied or Exhausted Brother Milor
    // can be copied while a locked or purged one cannot.
    if is_yin(state, seat)
        && state
            .player(seat)
            .is_some_and(|source| source.leaders.contains_key(&LeaderId::new("yinagent")))
    {
        for borrower in &state.players {
            if &borrower.id != seat
                && borrower
                    .leaders
                    .contains_key(&LeaderId::new("yssarilagent"))
            {
                abilities.push(borrowed_milor_ship(owner_name, seat, &borrower.id));
                abilities.push(borrowed_milor_ground(owner_name, seat, &borrower.id));
            }
        }
    }
    abilities
}

const YIN_ASCENDANT: &str = "yinbt";
const YIN_ASCENDANT_RNG: &str = "yinbt:unused_commander";

/// Firmament and Obsidian are alternate faces of one faction family. Ghost's legacy
/// `redcreusscommander` is noncanonical; the canonical faction commander is `ghostcommander`.
fn yinbt_faction_family(faction: &str) -> &str {
    match faction {
        "obsidian" => "firmament",
        other => other,
    }
}

/// Official DEFAULT commanders Yin can gain. This is deliberately independent of whether a
/// faction module implements the granted effect: the printed pool is all unused factions.
fn yinbt_canonical_commanders(content: &ContentStore) -> Vec<(String, String)> {
    let mut candidates = std::collections::BTreeMap::<String, String>::new();
    for record in content
        .catalogue(ContentType::Leaders, DEFAULT)
        .into_values()
    {
        if record.text("type") != Some("commander") {
            continue;
        }
        let Some(faction) = record.text("faction") else {
            continue;
        };
        let Some(id) = record.id() else {
            continue;
        };
        // The corpus includes redcreusscommander as a second Ghost commander and Obsidian's
        // secondary Firmament form. Neither is another unused faction.
        if id != format!("{faction}commander") || faction == "obsidian" {
            continue;
        }
        let family = yinbt_faction_family(faction);
        candidates.insert(family.to_owned(), id.to_owned());
    }
    candidates.into_iter().collect()
}

fn yinbt_commander_candidates(
    state: &GameState,
    content: &ContentStore,
    player: &PlayerId,
) -> Vec<String> {
    let used_families: std::collections::BTreeSet<String> = state
        .players
        .iter()
        .map(|seat| yinbt_faction_family(seat.faction.as_str()).to_owned())
        .collect();
    yinbt_canonical_commanders(content)
        .into_iter()
        .filter(|(family, commander)| {
            !used_families.contains(family)
                && !crate::promissory::has_commander_ability(state, player, commander)
        })
        .map(|(_, commander)| commander)
        .collect()
}

fn sample_yinbt_commander(candidates: &[String], rng: &mut crate::rng::GameRng) -> Option<String> {
    let sides = u32::try_from(candidates.len()).ok()?;
    if sides == 0 {
        return None;
    }
    let index = usize::try_from(rng.die(YIN_ASCENDANT_RNG, sides) - 1).ok()?;
    candidates.get(index).cloned()
}

fn grant_yinbt_commander(context: &mut TimingContext<'_>, player: &PlayerId) {
    let candidates = yinbt_commander_candidates(context.state, context.content, player);
    let Some(commander) = sample_yinbt_commander(&candidates, context.rng) else {
        return;
    };
    // The candidate pool excludes all known acquired rights. Keep the helper's guard anyway, so
    // a future mark source cannot turn a repeated or stale event into a second grant.
    let _ = crate::promissory::grant_commander_ability(
        context.state,
        context.content,
        player,
        &commander,
    );
}

fn yin_ascendant_gain(owner_name: &str, seat: &PlayerId) -> Ability {
    let owner = seat.clone();
    let condition_owner = seat.clone();
    Ability::stateful(
        format!("breakthrough:{owner_name}:{YIN_ASCENDANT}:BREAKTHROUGH_GAINED:after"),
        seat.clone(),
        "BREAKTHROUGH_GAINED",
        Relation::After,
        Arc::new(move |_, _, context| {
            grant_yinbt_commander(context, &owner);
            Ok(())
        }),
    )
    .with_stateful_condition(Arc::new(move |event, _, context| {
        event.text("player") == Some(condition_owner.as_str())
            && event.text("breakthrough") == Some(YIN_ASCENDANT)
            && is_yin(context.state, &condition_owner)
            && crate::breakthroughs::holds(context.state, &condition_owner, YIN_ASCENDANT)
    }))
}

fn yin_ascendant_public_score(owner_name: &str, seat: &PlayerId) -> Ability {
    let owner = seat.clone();
    let condition_owner = seat.clone();
    Ability::stateful(
        format!("breakthrough:{owner_name}:{YIN_ASCENDANT}:PUBLIC_OBJECTIVE_SCORED:after"),
        seat.clone(),
        "PUBLIC_OBJECTIVE_SCORED",
        Relation::After,
        Arc::new(move |_, _, context| {
            grant_yinbt_commander(context, &owner);
            Ok(())
        }),
    )
    .with_stateful_condition(Arc::new(move |event, _, context| {
        event.text("player") == Some(condition_owner.as_str())
            && event.text("objective").is_some()
            && is_yin(context.state, &condition_owner)
            && crate::breakthroughs::holds(context.state, &condition_owner, YIN_ASCENDANT)
    }))
}

fn illegal(error: IllegalChoice) -> TimingError {
    TimingError::IllegalChoice(error)
}

/// Indoctrination: "At the start of a ground combat: You may spend 2 influence to replace 1 of
/// your opponent's participating infantry with 1 infantry from your reinforcements."
///
/// Moyin's Ashes: "DEPLOY: When you use your Indoctrination faction ability, you may spend 1
/// additional influence to replace your opponent's unit with 1 mech instead of 1 infantry." Both
/// forms are one question: infantry (2), mech (3), or neither.
fn indoctrination(owner_name: &str, seat: &PlayerId) -> Ability {
    let owner = seat.clone();
    let condition_owner = seat.clone();
    Ability::stateful(
        format!("ability:{owner_name}:indoctrination:GROUND_COMBAT_STARTED:after"),
        seat.clone(),
        "GROUND_COMBAT_STARTED",
        Relation::After,
        Arc::new(move |event, _resolver, context| indoctrination_effect(event, context, &owner)),
    )
    .with_stateful_condition(Arc::new(move |event, _, context| {
        !indoctrination_forms(
            event,
            context.state,
            context.content,
            context.sources,
            &condition_owner,
        )
        .is_empty()
    }))
}

/// The replacements Indoctrination can make now: `(option id, base type, influence cost)`.
fn indoctrination_forms(
    event: &Event,
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    owner: &PlayerId,
) -> Vec<(&'static str, &'static str, i64)> {
    if !is_yin(state, owner) {
        return Vec::new();
    }
    let Some((system, planet, opponent)) = ground_combat_of(event, owner) else {
        return Vec::new();
    };
    if on_planet(
        state,
        content,
        sources,
        &opponent,
        &system,
        &planet,
        Some("infantry"),
    ) == 0
    {
        return Vec::new();
    }
    [
        ("infantry", "infantry", INDOCTRINATION_COST),
        ("mech", "mech", INDOCTRINATION_MECH_COST),
    ]
    .into_iter()
    .filter(|(_, base, cost)| {
        in_reinforcements(state, content, sources, owner, base)
            && crate::payment::affordable(state, content, sources, owner, *cost, Spend::Influence)
    })
    .collect()
}

fn indoctrination_effect(
    event: &Event,
    context: &mut TimingContext<'_>,
    owner: &PlayerId,
) -> Result<(), TimingError> {
    let forms = indoctrination_forms(
        event,
        context.state,
        context.content,
        context.sources,
        owner,
    );
    let Some((system, planet, opponent)) = ground_combat_of(event, owner) else {
        return Ok(());
    };
    if forms.is_empty() {
        return Ok(());
    }
    let options = forms
        .iter()
        .map(|(id, base, cost)| {
            ChoiceOption::labelled(
                (*id).to_owned(),
                "indoctrination",
                format!("spend {cost} influence to replace 1 infantry of {opponent} with a {base}"),
            )
            .with("cost", *cost)
        })
        .collect();
    let answer = ask_one(
        context,
        owner,
        format!("Indoctrination on {planet}: replace an enemy infantry"),
        "indoctrination",
        "indoctrination_replace",
        options,
    )
    .map_err(illegal)?;
    let Some((_, base, cost)) = forms.iter().find(|(id, _, _)| *id == answer.id) else {
        return Ok(());
    };
    let before = context.state.clone();
    let paid = crate::production::pay_seeing(
        context.state,
        context.content,
        context.sources,
        context.galaxy,
        context.table,
        owner,
        *cost,
        Spend::Influence,
    );
    match paid {
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
    if crate::action_cards::replace_unit(
        context,
        owner,
        &opponent,
        &system,
        Some(&planet),
        "infantry",
        base,
    ) {
        record_ability_used(context.state, owner);
    } else {
        // Checked before asking, so this is unreachable; if it ever happens, refund by restoring.
        *context.state = before;
    }
    Ok(())
}

/// Greyfire Mutagen: "At the start of a ground combat against 2 or more ground forces that are not
/// controlled by the Yin player: Replace 1 of your opponent's infantry with 1 infantry from your
/// reinforcements. Then, return this card to the Yin player."
///
/// The holder is the seat this ability belongs to; the opponent must not be the Yin player.
fn greyfire(owner_name: &str, seat: &PlayerId) -> Ability {
    let owner = seat.clone();
    let condition_owner = seat.clone();
    Ability::stateful(
        format!("promissory:{owner_name}:greyfire:GROUND_COMBAT_STARTED:after"),
        seat.clone(),
        "GROUND_COMBAT_STARTED",
        Relation::After,
        Arc::new(move |event, _resolver, context| {
            if !greyfire_ready(
                event,
                context.state,
                context.content,
                context.sources,
                &owner,
            ) {
                return Ok(());
            }
            let Some((system, planet, opponent)) = ground_combat_of(event, &owner) else {
                return Ok(());
            };
            if crate::action_cards::replace_unit(
                context,
                &owner,
                &opponent,
                &system,
                Some(&planet),
                "infantry",
                "infantry",
            ) {
                crate::promissory::give_back(
                    context.state,
                    &crate::promissory::note_id("greyfire", "yin"),
                );
            }
            Ok(())
        }),
    )
    .with_optional(true)
    .with_stateful_condition(Arc::new(move |event, _, context| {
        greyfire_ready(
            event,
            context.state,
            context.content,
            context.sources,
            &condition_owner,
        )
    }))
}

fn greyfire_ready(
    event: &Event,
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    holder: &PlayerId,
) -> bool {
    let note = crate::promissory::note_id("greyfire", "yin");
    if state.promissory_notes.get(&note) != Some(holder) || is_yin(state, holder) {
        return false;
    }
    let Some((system, planet, opponent)) = ground_combat_of(event, holder) else {
        return false;
    };
    !is_yin(state, &opponent)
        && on_planet(state, content, sources, &opponent, &system, &planet, None) >= 2
        && on_planet(
            state,
            content,
            sources,
            &opponent,
            &system,
            &planet,
            Some("infantry"),
        ) > 0
        && in_reinforcements(state, content, sources, holder, "infantry")
}

/// Yin Spinner: "After you produce units, place up to 2 infantry from your reinforcements on any
/// planet you control or in any space area that contains 1 or more of your ships."
fn yin_spinner(owner_name: &str, seat: &PlayerId) -> Ability {
    let owner = seat.clone();
    let condition_owner = seat.clone();
    Ability::stateful(
        format!("technology:{owner_name}:yso:UNITS_PRODUCED:after"),
        seat.clone(),
        "UNITS_PRODUCED",
        Relation::After,
        Arc::new(move |_event, _resolver, context| {
            // The ability's own decline already asked "use it?"; the spot is the only question.
            crate::action_cards::place_units_choosing(
                context,
                &owner,
                "infantry",
                2,
                crate::action_cards::PlacementTarget::ControlledPlanetOrShipSpace,
                None,
                false,
                "yso",
                crate::action_cards::PlacementLimits::Respect,
            )
            .map(|_| ())
            .map_err(illegal)
        }),
    )
    .with_optional(true)
    .with_stateful_condition(Arc::new(move |event, _, context| {
        event.text("player") == Some(condition_owner.as_str())
            && owns_technology(context.state, &condition_owner, "yso")
            && !crate::action_cards::placement_spots(
                context.state,
                context.content,
                context.sources,
                &condition_owner,
                crate::action_cards::PlacementTarget::ControlledPlanetOrShipSpace,
                None,
            )
            .is_empty()
            && in_reinforcements(
                context.state,
                context.content,
                context.sources,
                &condition_owner,
                "infantry",
            )
    }))
}

/// Van Hauge (flagship): "When this ship is destroyed, destroy all ships in this system."
///
/// Every ship still in the system goes, whoever owns it (neutral ships included). Each is staged
/// as a destruction; the emitter announces them (`combat::announce_staged_destructions`), so a
/// chain resolves.
fn van_hauge(owner_name: &str, seat: &PlayerId) -> Ability {
    let owner = seat.clone();
    let condition_owner = seat.clone();
    Ability::stateful(
        format!("unit:{owner_name}:yin_flagship:SHIP_DESTROYED:after"),
        seat.clone(),
        "SHIP_DESTROYED",
        Relation::After,
        Arc::new(move |event, _resolver, context| {
            let _ = &owner;
            let Some(system) = event.text("system").map(SystemId::new) else {
                return Ok(());
            };
            let mut owners: Vec<PlayerId> = Vec::new();
            if let Some(board) = context.state.board.get(&system) {
                for unit in &board.units {
                    if !owners.contains(&unit.owner) {
                        owners.push(unit.owner.clone());
                    }
                }
            }
            // Van Hauge is the source of every chained loss; only the enclosing combat fact is
            // inherited from the destruction that opened its window.
            let cause = "unit_ability:yin_flagship";
            let during_space_combat = event.boolean("during_space_combat") == Some(true);
            for player in owners {
                let ships = crate::combat::ships_of(
                    context.state,
                    context.content,
                    context.sources,
                    &player,
                    &system,
                );
                if !ships.is_empty() {
                    crate::combat::destroy_units_with_context(
                        context.state,
                        context.content,
                        context.sources,
                        &player,
                        &system,
                        &ships,
                        cause,
                        during_space_combat,
                    );
                }
            }
            Ok(())
        }),
    )
    .with_stateful_condition(Arc::new(move |event, _, context| {
        event.text("unit").is_some_and(|unit| {
            super::flagship_has_text(context.state, &condition_owner, unit, "yin_flagship")
        }) && event.text("player") == Some(condition_owner.as_str())
    }))
}

/// Whether Brother Milor is readied in `owner`'s hand.
fn agent_ready(state: &GameState, owner: &PlayerId) -> bool {
    state.player(owner).is_some_and(|seat| {
        seat.leaders.get(&LeaderId::new("yinagent")) == Some(&LeaderStatus::Readied)
    })
}

fn exhaust_agent(state: &mut GameState, owner: &PlayerId) {
    // Through `leaders::exhaust` so a watching Nomad's Temporal Command Suite hears it.
    crate::leaders::exhaust(state, owner, &LeaderId::new("yinagent"));
}

/// Brother Milor, a ship: "After a player's unit is destroyed during combat: You may exhaust this
/// card to allow that player to place 2 fighters in the destroyed unit's system if it was a ship,
/// or 2 infantry on its planet if it was a ground force."
///
/// "During combat" is read from the event's own `during_space_combat` fact,
/// the same way the ground branch reads it: the shared destruction route announces a combat casualty
/// and a Nova Seed loss identically, and the board cannot tell them apart once the ship is gone.
/// The fighters are placed without a capacity check: the card says "place 2", and 16.3 removes any
/// excess at the end of the turn.
fn brother_milor_ship(owner_name: &str, seat: &PlayerId) -> Ability {
    let owner = seat.clone();
    let condition_owner = seat.clone();
    let counts = |event: &Event| crate::combat::destroyed_during_combat(event);
    Ability::stateful(
        format!("leader:{owner_name}:yinagent:SHIP_DESTROYED:after"),
        seat.clone(),
        "SHIP_DESTROYED",
        Relation::After,
        Arc::new(move |event, _resolver, context| {
            let (Some(system), Some(player)) = (
                event.text("system").map(SystemId::new),
                event.text("player").map(PlayerId::new),
            ) else {
                return Ok(());
            };
            if !counts(event)
                || !agent_ready(context.state, &owner)
                || !in_reinforcements(
                    context.state,
                    context.content,
                    context.sources,
                    &player,
                    "fighter",
                )
            {
                return Ok(());
            }
            exhaust_agent(context.state, &owner);
            crate::action_cards::place_units_counted(context, &player, &system, None, "fighter", 2);
            Ok(())
        }),
    )
    .with_optional(true)
    .with_stateful_condition(Arc::new(move |event, _, context| {
        counts(event)
            && agent_ready(context.state, &condition_owner)
            && event.text("player").is_some_and(|player| {
                in_reinforcements(
                    context.state,
                    context.content,
                    context.sources,
                    &PlayerId::new(player),
                    "fighter",
                )
            })
    }))
}

/// Brother Milor, a ground force. Only ground forces destroyed in a ground combat round (or by
/// Harrow, which is part of it) count as "during combat": bombardment and space cannon defense
/// are not combat.
fn brother_milor_ground(owner_name: &str, seat: &PlayerId) -> Ability {
    let owner = seat.clone();
    let condition_owner = seat.clone();
    let counts = |event: &Event| {
        matches!(event.text("cause"), Some("ground_combat" | "harrow"))
            && event.text("unit").is_some()
    };
    Ability::stateful(
        format!("leader:{owner_name}:yinagent:GROUND_FORCE_DESTROYED:after"),
        seat.clone(),
        "GROUND_FORCE_DESTROYED",
        Relation::After,
        Arc::new(move |event, _resolver, context| {
            let (Some(system), Some(planet), Some(player)) = (
                event.text("system").map(SystemId::new),
                event.text("planet").map(PlanetId::new),
                event.text("player").map(PlayerId::new),
            ) else {
                return Ok(());
            };
            if !counts(event)
                || !agent_ready(context.state, &owner)
                || !in_reinforcements(
                    context.state,
                    context.content,
                    context.sources,
                    &player,
                    "infantry",
                )
            {
                return Ok(());
            }
            exhaust_agent(context.state, &owner);
            crate::action_cards::place_units_counted(
                context,
                &player,
                &system,
                Some(&planet),
                "infantry",
                2,
            );
            Ok(())
        }),
    )
    .with_optional(true)
    .with_stateful_condition(Arc::new(move |event, _, context| {
        counts(event)
            && agent_ready(context.state, &condition_owner)
            && event.text("player").is_some_and(|player| {
                in_reinforcements(
                    context.state,
                    context.content,
                    context.sources,
                    &PlayerId::new(player),
                    "infantry",
                )
            })
    }))
}

/// Ssruu copying Brother Milor's ship branch. The destroyed unit's owner receives the fighters;
/// only Ssruu is exhausted, while the source Milor is deliberately left untouched.
///
/// The copy carries the printed trigger, including "during combat": the same provenance the native
/// route reads, so a borrowed ability is not broader than the one it borrows.
fn borrowed_milor_ship(owner_name: &str, source: &PlayerId, borrower: &PlayerId) -> Ability {
    let (condition_source, condition_borrower) = (source.clone(), borrower.clone());
    let (effect_source, effect_borrower) = (source.clone(), borrower.clone());
    let counts = |event: &Event| crate::combat::destroyed_during_combat(event);
    Ability::stateful(
        format!("leader:{owner_name}:{source}:yssarilagent:yinagent:SHIP_DESTROYED:after"),
        borrower.clone(),
        "SHIP_DESTROYED",
        Relation::After,
        Arc::new(move |event, _, context| {
            if !counts(event)
                || !can_copy_milor(
                    context.state,
                    context.content,
                    &effect_source,
                    &effect_borrower,
                )
            {
                return Ok(());
            }
            let (Some(system), Some(player)) = (
                event.text("system").map(SystemId::new),
                event.text("player").map(PlayerId::new),
            ) else {
                return Ok(());
            };
            if !in_reinforcements(
                context.state,
                context.content,
                context.sources,
                &player,
                "fighter",
            ) || !crate::leaders::exhaust(
                context.state,
                &effect_borrower,
                &LeaderId::new("yssarilagent"),
            ) {
                return Ok(());
            }
            crate::action_cards::place_units_counted(context, &player, &system, None, "fighter", 2);
            super::hooks_cards::borrowed_agent_used(
                context,
                &effect_borrower,
                &LeaderId::new("yinagent"),
            );
            Ok(())
        }),
    )
    .with_optional(true)
    .with_stateful_condition(Arc::new(move |event, _, context| {
        counts(event)
            && can_copy_milor(
                context.state,
                context.content,
                &condition_source,
                &condition_borrower,
            )
            && event.text("player").is_some_and(|player| {
                in_reinforcements(
                    context.state,
                    context.content,
                    context.sources,
                    &PlayerId::new(player),
                    "fighter",
                )
            })
    }))
}

/// Ssruu copying Brother Milor's ground-force branch. Only combat casualties count, matching the
/// latest printed timing and the native route's filtering of bombardment and space cannon.
fn borrowed_milor_ground(owner_name: &str, source: &PlayerId, borrower: &PlayerId) -> Ability {
    let (condition_source, condition_borrower) = (source.clone(), borrower.clone());
    let (effect_source, effect_borrower) = (source.clone(), borrower.clone());
    let counts = |event: &Event| {
        matches!(event.text("cause"), Some("ground_combat" | "harrow"))
            && event.text("unit").is_some()
    };
    Ability::stateful(
        format!("leader:{owner_name}:{source}:yssarilagent:yinagent:GROUND_FORCE_DESTROYED:after"),
        borrower.clone(),
        "GROUND_FORCE_DESTROYED",
        Relation::After,
        Arc::new(move |event, _, context| {
            if !can_copy_milor(
                context.state,
                context.content,
                &effect_source,
                &effect_borrower,
            ) || !counts(event)
            {
                return Ok(());
            }
            let (Some(system), Some(planet), Some(player)) = (
                event.text("system").map(SystemId::new),
                event.text("planet").map(PlanetId::new),
                event.text("player").map(PlayerId::new),
            ) else {
                return Ok(());
            };
            if !in_reinforcements(
                context.state,
                context.content,
                context.sources,
                &player,
                "infantry",
            ) || !crate::leaders::exhaust(
                context.state,
                &effect_borrower,
                &LeaderId::new("yssarilagent"),
            ) {
                return Ok(());
            }
            crate::action_cards::place_units_counted(
                context,
                &player,
                &system,
                Some(&planet),
                "infantry",
                2,
            );
            super::hooks_cards::borrowed_agent_used(
                context,
                &effect_borrower,
                &LeaderId::new("yinagent"),
            );
            Ok(())
        }),
    )
    .with_optional(true)
    .with_stateful_condition(Arc::new(move |event, _, context| {
        counts(event)
            && can_copy_milor(
                context.state,
                context.content,
                &condition_source,
                &condition_borrower,
            )
            && event.text("player").is_some_and(|player| {
                in_reinforcements(
                    context.state,
                    context.content,
                    context.sources,
                    &PlayerId::new(player),
                    "infantry",
                )
            })
    }))
}

fn can_copy_milor(
    state: &GameState,
    content: &ContentStore,
    source: &PlayerId,
    borrower: &PlayerId,
) -> bool {
    super::hooks_cards::borrowable_agents(state, content, borrower)
        .iter()
        .any(|(owner, agent)| owner == source && agent.as_str() == "yinagent")
}

// -- Brother Omar's unlock -----------------------------------------------------------------------

/// Brother Omar, unlock: "Use one of your faction abilities." Recorded by Indoctrination and
/// Devotion, the faction's two abilities, when they resolve. The commander's own text is not
/// implemented (no route), so the leader is not claimed in [`MODULE`].
fn commander_unlocked(
    state: &GameState,
    _content: &ContentStore,
    _sources: SourceSet,
    _galaxy: Option<&ti4_content::galaxy::Galaxy>,
    player: &PlayerId,
    leader: &LeaderId,
) -> Option<bool> {
    (leader.as_str() == "yincommander")
        .then(|| state.faction_marks.contains_key(&ability_mark(player)))
}

// -- Brother Omar's effect -----------------------------------------------------------------------

/// Whether `player` currently has Brother Omar's commander ability.
fn commander_active(state: &GameState, player: &PlayerId) -> bool {
    crate::promissory::has_commander_ability(state, player, "yincommander")
}

/// Brother Omar: "This card satisfies a green technology prerequisite."
fn extra_prerequisite_colours(
    state: &GameState,
    _content: &ContentStore,
    player: &PlayerId,
) -> Vec<(String, usize)> {
    if commander_active(state, player) {
        vec![("green".to_owned(), 1)]
    } else {
        Vec::new()
    }
}

/// Every location where `player` may return an infantry to reinforcements, in board order.
fn infantry_payments(
    state: &GameState,
    content: &ContentStore,
    player: &PlayerId,
) -> Vec<(ResearchWaiverPayment, SystemId, Option<PlanetId>)> {
    let types = ti4_content::units::catalogue(content, DEFAULT);
    let is_mine = |unit: &Unit| {
        &unit.owner == player
            && types
                .get(unit.type_id.as_str())
                .is_some_and(|kind| kind.base_type() == "infantry")
    };
    let mut payments = Vec::new();
    for (system, board) in &state.board {
        for (planet, units) in &board.planet_units {
            if units.iter().any(is_mine) {
                payments.push((
                    ResearchWaiverPayment {
                        id: format!("ground|{system}|{planet}"),
                        label: format!("return 1 infantry from {planet} in {system}"),
                    },
                    system.clone(),
                    Some(planet.clone()),
                ));
            }
        }
        if board.units.iter().any(is_mine) {
            payments.push((
                ResearchWaiverPayment {
                    id: format!("space|{system}"),
                    label: format!("return 1 infantry from space in {system}"),
                },
                system.clone(),
                None,
            ));
        }
    }
    payments
}

/// Brother Omar: "When you research a tech owned by another player, you may return 1 of your
/// infantry to reinforcements to ignore its prerequisites."
fn research_waiver_offer(
    state: &GameState,
    content: &ContentStore,
    player: &PlayerId,
    tech: &TechnologyId,
) -> Option<ResearchWaiver> {
    let owned_elsewhere = state
        .players
        .iter()
        .any(|seat| &seat.id != player && seat.technologies.contains(tech));
    (commander_active(state, player)
        && owned_elsewhere
        && !infantry_payments(state, content, player).is_empty())
    .then(|| ResearchWaiver {
        id: "yin_infantry".to_owned(),
        label: "return 1 infantry to reinforcements to ignore its prerequisites".to_owned(),
        payments: infantry_payments(state, content, player)
            .into_iter()
            .map(|(payment, _, _)| payment)
            .collect(),
    })
}

/// Pay the player-selected infantry. The payment target is recomputed and revalidated before any
/// mutation, so a stale choice fails without granting the technology.
fn research_waiver_paid(
    state: &mut GameState,
    content: &ContentStore,
    player: &PlayerId,
    _tech: &TechnologyId,
    payment: &str,
) -> bool {
    let Some((_, system, planet)) = infantry_payments(state, content, player)
        .into_iter()
        .find(|(choice, _, _)| choice.id == payment)
    else {
        return false;
    };
    let types = ti4_content::units::catalogue(content, DEFAULT);
    let board = state.system_mut(&system);
    let units = match &planet {
        Some(planet) => board.planet_units.get_mut(planet),
        None => Some(&mut board.units),
    };
    if let Some(units) = units
        && let Some(index) = units.iter().position(|unit| {
            &unit.owner == player
                && types
                    .get(unit.type_id.as_str())
                    .is_some_and(|kind| kind.base_type() == "infantry")
        })
    {
        units.remove(index);
        return true;
    }
    false
}

// -- Dannel of the Tenth -------------------------------------------------------------------------

/// Every non-home planet on the map, in the map's system order. Without a map there are none.
fn hero_spots(
    content: &ContentStore,
    galaxy: Option<&ti4_content::galaxy::Galaxy>,
) -> Vec<(SystemId, PlanetId)> {
    let Some(galaxy) = galaxy else {
        return Vec::new();
    };
    let homes = ti4_content::galaxy::home_systems(content, DEFAULT);
    let mut spots = Vec::new();
    for system in galaxy.system_ids() {
        if homes.contains(system) {
            continue;
        }
        for planet in ti4_content::galaxy::planets_in(content, system, DEFAULT) {
            spots.push((SystemId::new(system), PlanetId::new(planet.id())));
        }
    }
    spots
}

fn leader_action(
    state: &GameState,
    content: &ContentStore,
    player: &PlayerId,
    leader: &LeaderId,
) -> Option<bool> {
    (leader.as_str() == "yinhero").then(|| {
        // The map is not in reach here; `use_leader` refuses (nothing changes) without one.
        in_reinforcements(state, content, DEFAULT, player, "infantry")
    })
}

/// Dannel's resolver-less compatibility hook. The Game path uses [`use_leader_timed`] so its
/// ground combats emit the normal timing events. This direct hook retains its historical tests;
/// it cannot run Indoctrination, Greyfire or Brother Milor timing effects.
fn use_leader(
    context: &mut TimingContext<'_>,
    player: &PlayerId,
    leader: &LeaderId,
) -> Option<bool> {
    if leader.as_str() != "yinhero" || !is_yin(context.state, player) {
        return None;
    }
    let spots = hero_spots(context.content, context.galaxy);
    let id = placed_type(
        context.state,
        context.content,
        context.sources,
        player,
        "infantry",
    )?;
    let stock = crate::supply::allowed(
        context.state,
        context.content,
        context.sources,
        player,
        &id,
        3,
    );
    if stock == 0 || spots.is_empty() {
        return Some(false);
    }
    let mut chosen: Vec<(SystemId, PlanetId)> = Vec::new();
    while chosen.len() < stock {
        let options = spots
            .iter()
            .map(|(system, planet)| {
                ChoiceOption::labelled(
                    format!("{system}|{planet}"),
                    "commit_infantry",
                    format!("commit infantry to {planet} in {system}"),
                )
            })
            .collect();
        let answer = ask_one(
            context,
            player,
            format!(
                "Quantum Dissemination: commit infantry {} of {stock}",
                chosen.len() + 1
            ),
            "yinhero",
            "yin_hero_commit",
            options,
        )
        .ok()?;
        if answer.is_decline() {
            break;
        }
        let spot = spots
            .iter()
            .find(|(system, planet)| format!("{system}|{planet}") == answer.id)?;
        chosen.push(spot.clone());
    }
    if chosen.is_empty() {
        return Some(false);
    }
    Some(land_and_fight(context, player, &chosen))
}

/// Dannel's Game path. Commit first, then resolve each selected planet through the ordinary
/// eventful ground-combat window, which deliberately performs no bombardment or space-cannon
/// defense for this action. `leaders::use_leader_timed` owns the purge after a successful return.
fn use_leader_timed(
    context: &mut TimingContext<'_>,
    resolver: &mut crate::timing::Resolver,
    player: &PlayerId,
    leader: &LeaderId,
) -> Result<Option<bool>, TimingError> {
    if leader.as_str() != "yinhero" || !is_yin(context.state, player) {
        return Ok(None);
    }
    let spots = hero_spots(context.content, context.galaxy);
    let stock = crate::supply::allowed(
        context.state,
        context.content,
        context.sources,
        player,
        &placed_type(
            context.state,
            context.content,
            context.sources,
            player,
            "infantry",
        )
        .ok_or_else(|| TimingError::StatefulContextRequired("yinhero infantry form".to_owned()))?,
        3,
    );
    if stock == 0 || spots.is_empty() {
        return Ok(Some(false));
    }
    let mut chosen = Vec::new();
    while chosen.len() < stock {
        let options = spots
            .iter()
            .map(|(system, planet)| {
                ChoiceOption::labelled(
                    format!("{system}|{planet}"),
                    "commit_infantry",
                    format!("commit infantry to {planet} in {system}"),
                )
            })
            .collect();
        let answer = match ask_one(
            context,
            player,
            format!(
                "Quantum Dissemination: commit infantry {} of {stock}",
                chosen.len() + 1
            ),
            "yinhero",
            "yin_hero_commit",
            options,
        ) {
            Ok(answer) => answer,
            Err(error) => return Err(illegal(error)),
        };
        if answer.is_decline() {
            break;
        }
        let Some(spot) = spots
            .iter()
            .find(|(system, planet)| format!("{system}|{planet}") == answer.id)
        else {
            return Ok(Some(false));
        };
        chosen.push(spot.clone());
    }
    if chosen.is_empty() {
        return Ok(Some(false));
    }

    for (system, planet) in &chosen {
        crate::action_cards::place_units_counted(
            context,
            player,
            system,
            Some(planet),
            "infantry",
            1,
        );
    }
    let mut order = Vec::new();
    for spot in &chosen {
        if !order.contains(spot) {
            order.push(spot.clone());
        }
    }
    {
        let crate::timing::TimingContext {
            state,
            content,
            sources,
            table,
            dice,
            rng,
            event_sequence,
            galaxy,
        } = context;
        let mut resolving = Resolving {
            content,
            sources: *sources,
            dice,
            rng,
            table,
            timing: Some(TimingHandle {
                resolver,
                sequence: event_sequence,
                galaxy: *galaxy,
            }),
        };
        for (system, planet) in &order {
            crate::invasion::InvasionWindow::fight_committed_planet(
                state,
                &mut resolving,
                player,
                system,
                planet,
            )?;
            if state.finished {
                break;
            }
        }
    }
    for (system, planet) in &order {
        crate::invasion::establish_control(
            context.state,
            context.content,
            context.sources,
            system,
            player,
            std::slice::from_ref(planet),
        );
    }
    Ok(Some(true))
}

/// Land the infantry, fight every planet to its end, then establish control. `false` (state
/// restored) if a decider gave an illegal answer mid-combat.
fn land_and_fight(
    context: &mut TimingContext<'_>,
    player: &PlayerId,
    chosen: &[(SystemId, PlanetId)],
) -> bool {
    let before = context.state.clone();
    for (system, planet) in chosen {
        crate::action_cards::place_units_counted(
            context,
            player,
            system,
            Some(planet),
            "infantry",
            1,
        );
    }
    let mut order: Vec<(SystemId, PlanetId)> = Vec::new();
    for spot in chosen {
        if !order.contains(spot) {
            order.push(spot.clone());
        }
    }
    for (system, planet) in &order {
        // `ground_combat` selects one rival owner. Dannel's text resolves combats on the chosen
        // planet, so keep resolving while Yin still has a ground force and another rival remains.
        while has_rival_ground_force(
            context.state,
            context.content,
            context.sources,
            system,
            planet,
            player,
        ) {
            if crate::invasion::ground_combat(
                context.state,
                context.content,
                context.sources,
                context.table,
                context.dice,
                context.rng,
                system,
                planet,
                player,
            )
            .is_err()
            {
                *context.state = before;
                return false;
            }
            if !has_ground_force(
                context.state,
                context.content,
                context.sources,
                system,
                planet,
                player,
            ) {
                break;
            }
        }
    }
    for (system, planet) in &order {
        crate::invasion::establish_control(
            context.state,
            context.content,
            context.sources,
            system,
            player,
            std::slice::from_ref(planet),
        );
    }
    true
}

/// Whether `player` has a surviving ground force on this planet.
fn has_ground_force(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    system: &SystemId,
    planet: &PlanetId,
    player: &PlayerId,
) -> bool {
    let types = ti4_content::units::catalogue(content, sources);
    state
        .system_state(system)
        .on_planet(planet)
        .iter()
        .any(|unit| {
            unit.owner == *player
                && types
                    .get(unit.type_id.as_str())
                    .is_some_and(ti4_content::units::UnitType::is_ground_force)
        })
}

/// Whether anyone other than `player` has a ground force on this planet.
fn has_rival_ground_force(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    system: &SystemId,
    planet: &PlanetId,
    player: &PlayerId,
) -> bool {
    let types = ti4_content::units::catalogue(content, sources);
    state
        .system_state(system)
        .on_planet(planet)
        .iter()
        .any(|unit| {
            unit.owner != *player
                && types
                    .get(unit.type_id.as_str())
                    .is_some_and(ti4_content::units::UnitType::is_ground_force)
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::choice::{Decider, Scripted, Table};
    use crate::fixtures::{armed_resolver, put, put_on_planet, seated_game, with_context};
    use ti4_model::content_types::DEFAULT;
    use ti4_model::id::TechnologyId;

    fn a() -> PlayerId {
        PlayerId::new("a")
    }
    fn b() -> PlayerId {
        PlayerId::new("b")
    }
    fn c() -> PlayerId {
        PlayerId::new("c")
    }
    fn arena() -> (GameState, SystemId) {
        (
            seated_game(&[("a", "yin"), ("b", "sol")], DEFAULT),
            SystemId::new("18"),
        )
    }
    fn scripted(answers: &[&str]) -> Table {
        Table::with_default(Box::new(Scripted::new(answers.iter().copied())))
    }
    struct SelectTimingAbility {
        id: String,
    }

    impl Decider for SelectTimingAbility {
        fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
            if let Some(selected) = choice.option(&self.id) {
                return Ok(selected.clone());
            }
            choice
                .options
                .iter()
                .find(|option| option.is_decline())
                .cloned()
                .ok_or_else(|| IllegalChoice::ScriptDiverged {
                    player: choice.player.clone(),
                    wanted: format!("{} or decline", self.id),
                    offered: choice.ids().into_iter().map(str::to_owned).collect(),
                })
        }
    }

    fn selecting_timing_ability(id: &str) -> Table {
        Table::with_default(Box::new(SelectTimingAbility { id: id.to_owned() }))
    }
    fn site<'a>(
        a: &'a PlayerId,
        b: &'a PlayerId,
        system: &'a SystemId,
        moment: CombatMoment,
    ) -> HitSite<'a> {
        HitSite {
            player: a,
            opponent: b,
            system,
            round: 1,
            moment,
        }
    }
    fn hits(
        state: &mut GameState,
        answers: &[&str],
        system: &SystemId,
        moment: CombatMoment,
    ) -> ProducedHits {
        let mut table = scripted(answers);
        let (a, b) = (a(), b());
        with_context(state, DEFAULT, None, &mut table, |ctx| {
            produced_hits(ctx, &site(&a, &b, system, moment))
        })
        .expect("legal")
    }
    fn count(state: &GameState, system: &SystemId, kind: &str) -> usize {
        state.board.get(system).map_or(0, |board| {
            board
                .units
                .iter()
                .filter(|unit| unit.type_id.as_str() == kind)
                .count()
        })
    }
    fn emit(
        state: &mut GameState,
        answers: &[&str],
        event_type: &str,
        payload: &[(&str, serde_json::Value)],
    ) {
        let mut resolver = armed_resolver(state);
        let mut table = scripted(answers);
        with_context(state, DEFAULT, None, &mut table, |ctx| {
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
                .expect("window resolves");
        });
    }

    fn grant_marks(state: &GameState, player: &PlayerId) -> Vec<String> {
        let prefix = format!("{}{}:", crate::promissory::COMMANDER_ABILITY_PREFIX, player);
        state
            .faction_marks
            .keys()
            .filter_map(|key| key.strip_prefix(&prefix).map(ToOwned::to_owned))
            .collect()
    }

    fn yinbt_game(with_breakthrough: bool, seed: u64) -> crate::game::Game<'static> {
        let mut state = seated_game(&[("a", "yin"), ("b", "sol")], DEFAULT);
        if with_breakthrough {
            state.player_mut(&a()).unwrap().breakthrough =
                Some(ti4_model::id::BreakthroughId::new(YIN_ASCENDANT));
        }
        state.rng_seed = seed;
        crate::game::Game::with_table(
            state,
            ContentStore::embedded(),
            Table::with_default(Box::new(crate::choice::AlwaysDecline)),
        )
        .with_sources(DEFAULT)
    }

    #[test]
    fn yinbt_pool_is_the_canonical_default_faction_roster() {
        let content = ContentStore::embedded();
        let canonical = yinbt_canonical_commanders(content);
        let ids: Vec<&str> = canonical.iter().map(|(_, id)| id.as_str()).collect();
        assert_eq!(
            ids.len(),
            30,
            "one canonical commander per official faction family"
        );
        assert!(ids.contains(&"kelerescommander"));
        assert!(
            ids.contains(&"crimsoncommander"),
            "unsupported handlers do not narrow the pool"
        );
        assert!(ids.contains(&"cabalcommander"));
        assert!(
            !ids.contains(&"redcreusscommander"),
            "Ghost has one canonical commander"
        );
        assert!(
            !ids.contains(&"obsidiancommander"),
            "Obsidian is Firmament's alternate face"
        );

        let state = seated_game(&[("a", "yin"), ("b", "sol"), ("c", "firmament")], DEFAULT);
        let candidates = yinbt_commander_candidates(&state, content, &a());
        assert!(!candidates.contains(&"solcommander".to_owned()));
        assert!(!candidates.contains(&"firmamentcommander".to_owned()));
        assert!(!candidates.contains(&"obsidiancommander".to_owned()));
        assert!(candidates.contains(&"crimsoncommander".to_owned()));
    }

    /// Commanders whose effect has a live route that reads `promissory::has_commander_ability`
    /// (BF-ALLIANCE-ROUTES-*, BF-BORROWED-*, BF-COMMANDERS-ALLIANCE-OCT5). A new faction family in
    /// the pool fails this test until its commander joins the list with a handler.
    const HANDLED_COMMANDERS: [&str; 30] = [
        "arboreccommander",
        "argentcommander",
        "ghostcommander",
        "hacancommander",
        "jolnarcommander",
        "l1z1xcommander",
        "letnevcommander",
        "mentakcommander",
        "muaatcommander",
        "naalucommander",
        "naazcommander",
        "saarcommander",
        "sardakkcommander",
        "solcommander",
        "winnucommander",
        "xxchacommander",
        "yincommander",
        "yssarilcommander",
        "cabalcommander",
        "empyreancommander",
        "kelerescommander",
        "mahactcommander",
        "nekrocommander",
        "nomadcommander",
        "titanscommander",
        "bastioncommander",
        "deepwroughtcommander",
        "crimsoncommander",
        "ralnelcommander",
        "firmamentcommander",
    ];

    #[test]
    fn yinbt_pool_has_a_live_handler_for_every_commander() {
        for (_, id) in yinbt_canonical_commanders(ContentStore::embedded()) {
            assert!(
                HANDLED_COMMANDERS.contains(&id.as_str()),
                "{id} can be granted by Yin Ascendant but has no handler"
            );
        }
    }

    #[test]
    fn yinbt_sampler_is_uniformly_indexed_and_seed_deterministic() {
        let candidates = yinbt_canonical_commanders(ContentStore::embedded())
            .into_iter()
            .map(|(_, id)| id)
            .collect::<Vec<_>>();
        let mut first_rng = crate::rng::GameRng::new(82);
        let mut replay_rng = crate::rng::GameRng::new(82);
        let first = sample_yinbt_commander(&candidates, &mut first_rng).unwrap();
        let replay = sample_yinbt_commander(&candidates, &mut replay_rng).unwrap();
        assert_eq!(first, replay);
        assert!(candidates.contains(&first));
        assert!(sample_yinbt_commander(&[], &mut first_rng).is_none());
    }

    #[test]
    fn gaining_yinbt_grants_one_random_unused_commander_through_game_delivery() {
        let content = ContentStore::embedded();
        let mut game = yinbt_game(false, 91);
        let eligible = yinbt_commander_candidates(&game.state, content, &a());
        game.state.player_mut(&a()).unwrap().breakthrough =
            Some(ti4_model::id::BreakthroughId::new(YIN_ASCENDANT));

        let result = game.step();

        assert!(
            result.error.is_none(),
            "gain event resolves: {:?}",
            result.error
        );
        let grants = grant_marks(&game.state, &a());
        assert_eq!(grants.len(), 1);
        assert!(eligible.contains(&grants[0]));
        assert!(crate::promissory::has_commander_ability(
            &game.state,
            &a(),
            &grants[0]
        ));
    }

    #[test]
    fn successful_public_scores_grant_distinct_commander_rights_via_game_flush() {
        let content = ContentStore::embedded();
        let mut game = yinbt_game(true, 177);
        let mut expected = yinbt_commander_candidates(&game.state, content, &a());
        for objective in ["expand_borders", "research_outposts"] {
            let before = grant_marks(&game.state, &a());
            crate::objectives::award(
                &mut game.state,
                content,
                DEFAULT,
                &a(),
                &ti4_model::id::ObjectiveId::new(objective),
            )
            .expect("public objective awards successfully");
            assert!(
                crate::supply::staged_event_types(&game.state)
                    .iter()
                    .any(|kind| kind == "PUBLIC_OBJECTIVE_SCORED")
            );
            let result = game.step();
            assert!(
                result.error.is_none(),
                "score event resolves: {:?}",
                result.error
            );
            let grants = grant_marks(&game.state, &a());
            assert_eq!(
                grants.len(),
                if objective == "expand_borders" { 1 } else { 2 }
            );
            let newly_granted = grants
                .iter()
                .find(|commander| !before.contains(commander))
                .expect("this score grants one new commander");
            assert!(expected.contains(newly_granted));
            expected.retain(|candidate| candidate != newly_granted);
        }
        let grants = grant_marks(&game.state, &a());
        assert_ne!(
            grants[0], grants[1],
            "a previously acquired right is no longer unused"
        );
    }

    #[test]
    fn yinbt_requires_the_seated_yin_owner_and_a_public_score_event() {
        let content = ContentStore::embedded();
        let mut no_breakthrough = yinbt_game(false, 12);
        crate::objectives::award(
            &mut no_breakthrough.state,
            content,
            DEFAULT,
            &a(),
            &ti4_model::id::ObjectiveId::new("expand_borders"),
        )
        .unwrap();
        assert!(
            !crate::supply::staged_event_types(&no_breakthrough.state)
                .iter()
                .any(|kind| kind == "PUBLIC_OBJECTIVE_SCORED")
        );
        assert!(no_breakthrough.step().error.is_none());
        assert!(grant_marks(&no_breakthrough.state, &a()).is_empty());

        let mut secret_score = yinbt_game(true, 13);
        secret_score
            .state
            .player_mut(&a())
            .unwrap()
            .secret_objectives
            .push(ti4_model::id::SecretObjectiveId::new("sar"));
        crate::secrets::award(
            &mut secret_score.state,
            content,
            &a(),
            &ti4_model::id::SecretObjectiveId::new("sar"),
        )
        .expect("held secret scores");
        assert!(
            !crate::supply::staged_event_types(&secret_score.state)
                .iter()
                .any(|kind| kind == "PUBLIC_OBJECTIVE_SCORED")
        );
        assert!(secret_score.step().error.is_none());
        assert!(grant_marks(&secret_score.state, &a()).is_empty());

        let mut non_yin = yinbt_game(false, 14);
        non_yin.state.player_mut(&b()).unwrap().breakthrough =
            Some(ti4_model::id::BreakthroughId::new(YIN_ASCENDANT));
        crate::objectives::award(
            &mut non_yin.state,
            content,
            DEFAULT,
            &b(),
            &ti4_model::id::ObjectiveId::new("expand_borders"),
        )
        .unwrap();
        assert!(
            !crate::supply::staged_event_types(&non_yin.state)
                .iter()
                .any(|kind| kind == "PUBLIC_OBJECTIVE_SCORED")
        );
        assert!(non_yin.step().error.is_none());
        assert!(grant_marks(&non_yin.state, &b()).is_empty());
    }

    // -- Devotion --------------------------------------------------------------------------------

    #[test]
    fn devotion_destroys_a_cruiser_for_a_producer_assigned_hit() {
        let (mut state, system) = arena();
        put(&mut state, &system, "cruiser", &a(), 1);
        put(&mut state, &system, "destroyer", &a(), 1);
        put(&mut state, &system, "dreadnought", &b(), 1);
        let got = hits(&mut state, &["cruiser"], &system, CombatMoment::RoundEnded);
        assert_eq!(
            got,
            ProducedHits {
                producer_assigned: 1,
                ..ProducedHits::NONE
            }
        );
        assert_eq!(count(&state, &system, "cruiser"), 0);
        assert_eq!(
            count(&state, &system, "destroyer"),
            1,
            "only the chosen one"
        );
        assert_eq!(state.pending_destructions.len(), 1, "staged for announcing");
        assert!(state.faction_marks.contains_key(&ability_mark(&a())));
    }

    #[test]
    fn devotion_declined_or_unavailable_changes_nothing() {
        let (mut state, system) = arena();
        put(&mut state, &system, "cruiser", &a(), 1);
        put(&mut state, &system, "dreadnought", &b(), 1);
        let declined = hits(&mut state, &["decline"], &system, CombatMoment::RoundEnded);
        assert_eq!(declined, ProducedHits::NONE);
        assert_eq!(count(&state, &system, "cruiser"), 1);
        assert!(state.faction_marks.is_empty());
        // Only the start moment belongs to Impulse Core, which this seat does not own.
        let wrong = hits(&mut state, &["cruiser"], &system, CombatMoment::CombatStart);
        assert_eq!(wrong, ProducedHits::NONE);
        // No cruiser or destroyer to give: nothing is offered.
        let (mut bare, system) = arena();
        put(&mut bare, &system, "dreadnought", &a(), 1);
        put(&mut bare, &system, "dreadnought", &b(), 1);
        assert_eq!(
            hits(&mut bare, &["decline"], &system, CombatMoment::RoundEnded),
            ProducedHits::NONE
        );
        // Not Yin: no ability.
        let mut other = seated_game(&[("a", "sol"), ("b", "yin")], DEFAULT);
        put(&mut other, &system, "cruiser", &a(), 1);
        put(&mut other, &system, "dreadnought", &b(), 1);
        assert_eq!(
            hits(&mut other, &["cruiser"], &system, CombatMoment::RoundEnded),
            ProducedHits::NONE
        );
        assert_eq!(count(&other, &system, "cruiser"), 1);
    }

    // -- Impulse Core ----------------------------------------------------------------------------

    #[test]
    fn impulse_core_forces_the_hit_onto_a_non_fighter() {
        let (mut state, system) = arena();
        state
            .player_mut(&a())
            .unwrap()
            .technologies
            .insert(TechnologyId::new("ic"));
        put(&mut state, &system, "destroyer", &a(), 1);
        put(&mut state, &system, "fighter", &b(), 2);
        let got = hits(
            &mut state,
            &["destroyer"],
            &system,
            CombatMoment::CombatStart,
        );
        assert_eq!(
            got,
            ProducedHits {
                non_fighter: 1,
                ..ProducedHits::NONE
            }
        );
        assert_eq!(count(&state, &system, "destroyer"), 0);
        assert!(
            state.faction_marks.is_empty(),
            "a technology is not a faction ability"
        );
    }

    #[test]
    fn impulse_core_needs_the_technology_and_a_target() {
        let (mut state, system) = arena();
        put(&mut state, &system, "destroyer", &a(), 1);
        put(&mut state, &system, "fighter", &b(), 1);
        assert_eq!(
            hits(
                &mut state,
                &["destroyer"],
                &system,
                CombatMoment::CombatStart
            ),
            ProducedHits::NONE,
            "no technology"
        );
        state
            .player_mut(&a())
            .unwrap()
            .technologies
            .insert(TechnologyId::new("ic"));
        assert_eq!(
            hits(&mut state, &["decline"], &system, CombatMoment::CombatStart),
            ProducedHits::NONE
        );
        assert_eq!(count(&state, &system, "destroyer"), 1);
        let (mut empty, system) = arena();
        empty
            .player_mut(&a())
            .unwrap()
            .technologies
            .insert(TechnologyId::new("ic"));
        put(&mut empty, &system, "destroyer", &a(), 1);
        assert_eq!(
            hits(
                &mut empty,
                &["destroyer"],
                &system,
                CombatMoment::CombatStart
            ),
            ProducedHits::NONE,
            "the opponent has no ship"
        );
        assert_eq!(count(&empty, &system, "destroyer"), 1);
    }

    // -- Wiring through a real combat ------------------------------------------------------------

    /// Fight the combat in `system` through the combat window, armed as the game arms it, and
    /// return the `(player, unit)` of every `SHIP_DESTROYED`, in order.
    fn fought(mut state: GameState, system: &SystemId, answers: &[&str]) -> Vec<(String, String)> {
        use crate::choice::{Resolving, TimingHandle, Window};
        use std::sync::Mutex;
        let content = ContentStore::embedded();
        let seen: Arc<Mutex<Vec<(String, String)>>> = Arc::default();
        let mut resolver = armed_resolver(&state);
        let log = Arc::clone(&seen);
        resolver.register([Ability::new(
            "probe:SHIP_DESTROYED",
            a(),
            "SHIP_DESTROYED",
            Relation::After,
            Arc::new(move |event, _| {
                log.lock().unwrap().push((
                    event.text("player").unwrap_or_default().to_owned(),
                    event.text("unit").unwrap_or_default().to_owned(),
                ));
                Ok(())
            }),
        )]);
        let mut sequence = crate::event::EventSequence::new();
        let mut table = scripted(answers);
        let mut dice = crate::dice::Dice::new();
        let mut rng = crate::rng::GameRng::new(5);
        let mut window = crate::combat::CombatWindow::new(&state, content, DEFAULT, system);
        let mut ctx = Resolving {
            content,
            sources: DEFAULT,
            dice: &mut dice,
            rng: &mut rng,
            table: &mut table,
            timing: Some(TimingHandle {
                resolver: &mut resolver,
                sequence: &mut sequence,
                galaxy: None,
            }),
        };
        window.settle_open(&mut state, &mut ctx).expect("opens");
        while window.outcome().is_none() {
            window.drive(&mut state, &mut ctx).expect("drives");
            if window.outcome().is_some() {
                break;
            }
            let _ = window.take_scoring_occurrence();
            window.settle_open(&mut state, &mut ctx).expect("settles");
        }
        let guard = seen.lock().unwrap();
        guard.clone()
    }

    #[test]
    fn impulse_core_and_devotion_run_inside_a_real_combat() {
        // Impulse Core: the destroyer is sacrificed at the very start, before any die.
        let (mut state, system) = arena();
        state
            .player_mut(&a())
            .unwrap()
            .technologies
            .insert(TechnologyId::new("ic"));
        put(&mut state, &system, "destroyer", &a(), 1);
        put(&mut state, &system, "dreadnought", &b(), 1);
        let destroyed = fought(state, &system, &["destroyer"]);
        assert_eq!(
            destroyed.first(),
            Some(&("a".to_owned(), "destroyer".to_owned())),
            "the sacrifice is announced first: {destroyed:?}"
        );
        // The same fight without the technology never offers it: the destroyer is not the
        // first casualty by choice (the script would otherwise fail on the unused answer).
        let (mut bare, system) = arena();
        put(&mut bare, &system, "destroyer", &a(), 1);
        put(&mut bare, &system, "dreadnought", &b(), 4);
        let plain = fought(bare, &system, &[]);
        assert!(plain.iter().all(|(player, _)| player == "a"), "{plain:?}");
    }

    // -- Van Hauge -------------------------------------------------------------------------------

    fn destroyed(
        system: &SystemId,
        player: &str,
        unit: &str,
    ) -> Vec<(&'static str, serde_json::Value)> {
        vec![
            ("system", system.to_string().into()),
            ("player", player.into()),
            ("unit", unit.into()),
            ("last", false.into()),
            // These fixtures stand for a ship lost while a space combat was resolving; see
            // `destroyed_outside_combat` for the other provenance.
            ("cause", crate::combat::SHIP_CAUSE_COMBAT.into()),
            ("during_space_combat", true.into()),
        ]
    }

    /// The same destruction, caused by an effect that is not a space combat.
    fn destroyed_outside_combat(
        system: &SystemId,
        player: &str,
        unit: &str,
        cause: &str,
    ) -> Vec<(&'static str, serde_json::Value)> {
        let mut payload = destroyed(system, player, unit);
        payload.retain(|(key, _)| !matches!(*key, "cause" | "during_space_combat"));
        payload.push(("cause", cause.into()));
        payload.push(("during_space_combat", false.into()));
        payload
    }

    /// Brother Milor would answer the same destruction with fighters; these tests are about Van Hauge.
    fn exhaust_milor(state: &mut GameState) {
        exhaust_agent(state, &a());
    }

    #[test]
    fn van_hauge_takes_every_ship_in_the_system_with_it() {
        let (mut state, system) = arena();
        exhaust_milor(&mut state);
        put(&mut state, &system, "cruiser", &a(), 2);
        put(&mut state, &system, "dreadnought", &b(), 1);
        put(&mut state, &system, "fighter", &b(), 3);
        emit(
            &mut state,
            &[],
            "SHIP_DESTROYED",
            &destroyed(&system, "a", "yin_flagship"),
        );
        assert!(
            state
                .board
                .get(&system)
                .is_none_or(|board| board.units.is_empty()),
            "all ships destroyed, both sides'"
        );
        assert_eq!(state.pending_destructions.len(), 6, "each one staged");
    }

    #[test]
    fn van_hauge_does_not_fire_for_other_ships_or_other_systems() {
        let (mut state, system) = arena();
        exhaust_milor(&mut state);
        put(&mut state, &system, "cruiser", &a(), 1);
        put(&mut state, &system, "dreadnought", &b(), 1);
        emit(
            &mut state,
            &[],
            "SHIP_DESTROYED",
            &destroyed(&system, "a", "cruiser"),
        );
        assert_eq!(
            count(&state, &system, "cruiser"),
            1,
            "any other ship is just destroyed"
        );
        assert_eq!(count(&state, &system, "dreadnought"), 1);
        // Another system's ships are untouched.
        let other = SystemId::new("19");
        put(&mut state, &other, "cruiser", &b(), 1);
        emit(
            &mut state,
            &[],
            "SHIP_DESTROYED",
            &destroyed(&system, "a", "yin_flagship"),
        );
        assert_eq!(count(&state, &other, "cruiser"), 1);
        assert_eq!(count(&state, &system, "dreadnought"), 0);
    }

    // -- Yin Spinner -----------------------------------------------------------------------------

    fn produced(player: &str) -> Vec<(&'static str, serde_json::Value)> {
        vec![
            ("player", player.into()),
            ("system", "03".into()),
            ("source", "production".into()),
            ("count", 1.into()),
        ]
    }

    fn home_infantry(state: &GameState) -> usize {
        state
            .board
            .get(&SystemId::new("03"))
            .and_then(|board| board.planet_units.get(&PlanetId::new("darien")))
            .map_or(0, |units| {
                units
                    .iter()
                    .filter(|unit| unit.type_id.as_str() == "infantry")
                    .count()
            })
    }

    #[test]
    fn yin_spinner_places_two_infantry_where_the_owner_chooses() {
        let mut state = seated_game(&[("a", "yin"), ("b", "sol")], DEFAULT);
        state
            .player_mut(&a())
            .unwrap()
            .technologies
            .insert(TechnologyId::new("yso"));
        let before = home_infantry(&state);
        emit(
            &mut state,
            &["technology:yin:yso:UNITS_PRODUCED:after", "03|darien"],
            "UNITS_PRODUCED",
            &produced("a"),
        );
        assert_eq!(home_infantry(&state), before + 2);
    }

    #[test]
    fn yin_spinner_is_not_offered_without_the_technology_or_for_another_producer() {
        let mut state = seated_game(&[("a", "yin"), ("b", "sol")], DEFAULT);
        let before = home_infantry(&state);
        emit(&mut state, &[], "UNITS_PRODUCED", &produced("a"));
        assert_eq!(home_infantry(&state), before, "no technology");
        state
            .player_mut(&a())
            .unwrap()
            .technologies
            .insert(TechnologyId::new("yso"));
        emit(&mut state, &[], "UNITS_PRODUCED", &produced("b"));
        assert_eq!(home_infantry(&state), before, "someone else produced");
        emit(&mut state, &["decline"], "UNITS_PRODUCED", &produced("a"));
        assert_eq!(home_infantry(&state), before, "declined");
    }

    // -- Ground combat: Indoctrination and Greyfire ----------------------------------------------

    fn ground_started(system: &str, planet: &str) -> Vec<(&'static str, serde_json::Value)> {
        vec![
            ("system", system.into()),
            ("planet", planet.into()),
            ("attacker", "a".into()),
            ("defender", "b".into()),
        ]
    }

    /// `a` (Yin) attacks `b` on a planet where `b` has `defenders` infantry.
    fn ground_arena(defenders: usize) -> (GameState, SystemId, PlanetId) {
        let mut state = seated_game(&[("a", "yin"), ("b", "sol")], DEFAULT);
        let (system, planet) = crate::fixtures::a_placed_planet();
        put_on_planet(&mut state, &system, &planet, "infantry", &a(), 1);
        put_on_planet(&mut state, &system, &planet, "infantry", &b(), defenders);
        // Two influence of Darien's worth is enough; give the seat a trade good as well.
        state.player_mut(&a()).unwrap().trade_goods = 5;
        (state, system, planet)
    }

    fn infantry_of(
        state: &GameState,
        system: &SystemId,
        planet: &PlanetId,
        owner: &PlayerId,
        kind: &str,
    ) -> usize {
        state
            .board
            .get(system)
            .and_then(|board| board.planet_units.get(planet))
            .map_or(0, |units| {
                units
                    .iter()
                    .filter(|unit| &unit.owner == owner && unit.type_id.as_str().ends_with(kind))
                    .count()
            })
    }

    #[test]
    fn indoctrination_pays_two_influence_and_swaps_one_infantry() {
        let (mut state, system, planet) = ground_arena(2);
        let goods = state.player(&a()).unwrap().trade_goods;
        emit(
            &mut state,
            &["infantry"],
            "GROUND_COMBAT_STARTED",
            &ground_started(system.as_str(), planet.as_str()),
        );
        assert_eq!(infantry_of(&state, &system, &planet, &b(), "infantry"), 1);
        assert_eq!(infantry_of(&state, &system, &planet, &a(), "infantry"), 2);
        let seat = state.player(&a()).unwrap();
        assert!(
            seat.trade_goods < goods || !state.exhausted_planets.is_empty(),
            "influence was spent"
        );
        assert!(state.faction_marks.contains_key(&ability_mark(&a())));
    }

    #[test]
    fn indoctrination_with_a_mech_costs_one_more_and_places_a_mech() {
        let (mut state, system, planet) = ground_arena(1);
        let goods = state.player(&a()).unwrap().trade_goods;
        emit(
            &mut state,
            &["mech"],
            "GROUND_COMBAT_STARTED",
            &ground_started(system.as_str(), planet.as_str()),
        );
        assert_eq!(infantry_of(&state, &system, &planet, &b(), "infantry"), 0);
        assert_eq!(infantry_of(&state, &system, &planet, &a(), "yin_mech"), 1);
        let spent = (goods - state.player(&a()).unwrap().trade_goods)
            + i32::try_from(state.exhausted_planets.len()).unwrap();
        assert!(spent >= 1);
    }

    #[test]
    fn indoctrination_declined_or_unaffordable_changes_nothing() {
        let (mut state, system, planet) = ground_arena(1);
        emit(
            &mut state,
            &["decline"],
            "GROUND_COMBAT_STARTED",
            &ground_started(system.as_str(), planet.as_str()),
        );
        assert_eq!(infantry_of(&state, &system, &planet, &b(), "infantry"), 1);
        assert!(state.faction_marks.is_empty());
        // Nothing to spend: the ability is not offered (the script would otherwise fail).
        let (mut poor, system, planet) = ground_arena(1);
        poor.player_mut(&a()).unwrap().trade_goods = 0;
        let controlled: Vec<PlanetId> = poor
            .board
            .values()
            .flat_map(|board| board.planet_control.keys().cloned())
            .collect();
        for planet in controlled {
            poor.exhaust_planet(planet);
        }
        emit(
            &mut poor,
            &["infantry"],
            "GROUND_COMBAT_STARTED",
            &ground_started(system.as_str(), planet.as_str()),
        );
        assert_eq!(infantry_of(&poor, &system, &planet, &b(), "infantry"), 1);
    }

    #[test]
    fn indoctrination_belongs_to_the_yin_player_only() {
        let mut state = seated_game(&[("a", "sol"), ("b", "yin")], DEFAULT);
        let (system, planet) = crate::fixtures::a_placed_planet();
        put_on_planet(&mut state, &system, &planet, "infantry", &a(), 2);
        put_on_planet(&mut state, &system, &planet, "infantry", &b(), 1);
        state.player_mut(&a()).unwrap().trade_goods = 5;
        // `a` (Sol) attacks the Yin player: neither side's Indoctrination applies to Sol, and the
        // Yin defender has no infantry of Sol's to take... it has: b is Yin and may indoctrinate a.
        state.player_mut(&b()).unwrap().trade_goods = 5;
        emit(
            &mut state,
            &["infantry"],
            "GROUND_COMBAT_STARTED",
            &ground_started(system.as_str(), planet.as_str()),
        );
        assert_eq!(
            infantry_of(&state, &system, &planet, &a(), "infantry"),
            1,
            "the Yin defender used it on the attacker"
        );
    }

    fn lend_greyfire(state: &mut GameState, to: &PlayerId) {
        let note = crate::promissory::note_id("greyfire", "yin");
        state.promissory_notes.insert(note, to.clone());
    }

    #[test]
    fn greyfire_replaces_an_infantry_and_returns_to_the_yin_player() {
        let mut state = seated_game(&[("a", "sol"), ("b", "hacan"), ("c", "yin")], DEFAULT);
        let (system, planet) = crate::fixtures::a_placed_planet();
        put_on_planet(&mut state, &system, &planet, "infantry", &a(), 1);
        put_on_planet(&mut state, &system, &planet, "infantry", &b(), 2);
        lend_greyfire(&mut state, &a());
        let note = crate::promissory::note_id("greyfire", "yin");
        emit(
            &mut state,
            &["promissory:sol:greyfire:GROUND_COMBAT_STARTED:after"],
            "GROUND_COMBAT_STARTED",
            &ground_started(system.as_str(), planet.as_str()),
        );
        assert_eq!(infantry_of(&state, &system, &planet, &b(), "infantry"), 1);
        assert_eq!(infantry_of(&state, &system, &planet, &a(), "infantry"), 2);
        assert_eq!(
            state.promissory_notes.get(&note),
            Some(&PlayerId::new("c")),
            "returned to the Yin player"
        );
    }

    #[test]
    fn greyfire_needs_two_ground_forces_and_a_non_yin_opponent() {
        let mut state = seated_game(&[("a", "sol"), ("b", "hacan"), ("c", "yin")], DEFAULT);
        let (system, planet) = crate::fixtures::a_placed_planet();
        put_on_planet(&mut state, &system, &planet, "infantry", &a(), 1);
        put_on_planet(&mut state, &system, &planet, "infantry", &b(), 1);
        lend_greyfire(&mut state, &a());
        let note = crate::promissory::note_id("greyfire", "yin");
        emit(
            &mut state,
            &[],
            "GROUND_COMBAT_STARTED",
            &ground_started(system.as_str(), planet.as_str()),
        );
        assert_eq!(
            infantry_of(&state, &system, &planet, &b(), "infantry"),
            1,
            "one ground force is not enough"
        );
        assert_eq!(state.promissory_notes.get(&note), Some(&a()), "still held");
        // Declining keeps the card too.
        put_on_planet(&mut state, &system, &planet, "infantry", &b(), 1);
        emit(
            &mut state,
            &["decline"],
            "GROUND_COMBAT_STARTED",
            &ground_started(system.as_str(), planet.as_str()),
        );
        assert_eq!(infantry_of(&state, &system, &planet, &b(), "infantry"), 2);
        // The Yin player's own forces are excluded.
        let mut own = seated_game(&[("a", "sol"), ("b", "yin")], DEFAULT);
        put_on_planet(&mut own, &system, &planet, "infantry", &a(), 1);
        put_on_planet(&mut own, &system, &planet, "infantry", &b(), 2);
        lend_greyfire(&mut own, &a());
        emit(
            &mut own,
            &[],
            "GROUND_COMBAT_STARTED",
            &ground_started(system.as_str(), planet.as_str()),
        );
        // (The Yin defender may use Indoctrination here; the question is only the card.)
        assert_eq!(
            own.promissory_notes
                .get(&crate::promissory::note_id("greyfire", "yin")),
            Some(&a()),
            "the note is not played against the Yin player's own forces"
        );
    }

    // -- Brother Milor ---------------------------------------------------------------------------

    fn fighters_in(state: &GameState, system: &SystemId, owner: &PlayerId) -> usize {
        state.board.get(system).map_or(0, |board| {
            board
                .units
                .iter()
                .filter(|unit| &unit.owner == owner && unit.type_id.as_str() == "fighter")
                .count()
        })
    }

    #[test]
    fn brother_milor_exhausting_is_heard_by_a_watching_nomad() {
        let (mut state, system) = arena();
        state.player_mut(&b()).unwrap().faction = ti4_model::id::FactionId::new("nomad");
        state
            .player_mut(&b())
            .unwrap()
            .technologies
            .insert(ti4_model::id::TechnologyId::new("tcs"));
        emit(
            &mut state,
            &["leader:yin:yinagent:SHIP_DESTROYED:after"],
            "SHIP_DESTROYED",
            &destroyed(&system, "b", "cruiser"),
        );
        assert_eq!(
            crate::supply::staged_event_types(&state),
            ["AGENT_EXHAUSTED"]
        );
    }

    #[test]
    fn brother_milor_gives_two_fighters_for_a_destroyed_ship() {
        let (mut state, system) = arena();
        emit(
            &mut state,
            &["leader:yin:yinagent:SHIP_DESTROYED:after"],
            "SHIP_DESTROYED",
            &destroyed(&system, "b", "cruiser"),
        );
        assert_eq!(
            fighters_in(&state, &system, &b()),
            2,
            "the destroyed ship's owner places"
        );
        assert_eq!(
            state
                .player(&a())
                .unwrap()
                .leaders
                .get(&LeaderId::new("yinagent")),
            Some(&LeaderStatus::Exhausted)
        );
        // Exhausted now: a second destruction offers nothing.
        emit(
            &mut state,
            &[],
            "SHIP_DESTROYED",
            &destroyed(&system, "b", "cruiser"),
        );
        assert_eq!(fighters_in(&state, &system, &b()), 2);
    }

    #[test]
    fn brother_milor_counts_a_courageous_loss_during_space_combat() {
        let (mut state, system) = arena();
        let mut payload = destroyed(&system, "b", "cruiser");
        payload.retain(|(key, _)| *key != "cause");
        payload.push(("cause", "action_card:courageous".into()));
        emit(
            &mut state,
            &["leader:yin:yinagent:SHIP_DESTROYED:after"],
            "SHIP_DESTROYED",
            &payload,
        );
        assert_eq!(
            fighters_in(&state, &system, &b()),
            2,
            "the card remains the cause while the combat-window fact opens Milor"
        );
    }

    /// The printed trigger is "after a player's unit is destroyed **during combat**". A loss a
    /// non-combat effect caused -- Nova Seed destroying another player's ships, an ordinary
    /// component or leader effect -- is not that trigger, whatever the shared route announced.
    #[test]
    fn brother_milor_ignores_a_ship_destroyed_outside_combat() {
        let (mut state, system) = arena();
        emit(
            &mut state,
            &[],
            "SHIP_DESTROYED",
            &destroyed_outside_combat(&system, "b", "cruiser", "breakthrough:nova_seed"),
        );
        assert_eq!(
            fighters_in(&state, &system, &b()),
            0,
            "a Nova Seed loss is not a combat loss"
        );
        assert_eq!(
            state
                .player(&a())
                .unwrap()
                .leaders
                .get(&LeaderId::new("yinagent")),
            Some(&LeaderStatus::Readied),
            "an unmet trigger does not exhaust the agent"
        );
        // A component effect that names itself is refused for the same reason.
        emit(
            &mut state,
            &[],
            "SHIP_DESTROYED",
            &destroyed_outside_combat(&system, "b", "cruiser", "component:orbital_ships"),
        );
        assert_eq!(fighters_in(&state, &system, &b()), 0);
    }

    #[test]
    fn ssruu_copies_milor_ship_after_actual_destroyed_event_and_actor_gets_fighters() {
        let mut state = seated_game(&[("a", "yin"), ("b", "yssaril"), ("c", "sol")], DEFAULT);
        let system = SystemId::new("18");
        put(&mut state, &system, "cruiser", &c(), 1);
        let source = LeaderId::new("yinagent");
        let borrower = LeaderId::new("yssarilagent");
        let copied = "leader:yin:a:yssarilagent:yinagent:SHIP_DESTROYED:after";
        let native = "leader:yin:yinagent:SHIP_DESTROYED:after";

        // Destroy a real board unit, then announce the pending casualty through the same Resolver
        // route as combat. The source owner's native optional is declined independently.
        let content = ContentStore::embedded();
        let victim = crate::combat::ships_of(&state, content, DEFAULT, &c(), &system)
            .into_iter()
            .find(|unit| unit.type_id.as_str() == "cruiser")
            .expect("the actor's cruiser is in the system");
        assert_eq!(
            crate::combat::destroy_units(
                &mut state,
                content,
                DEFAULT,
                &c(),
                &system,
                std::slice::from_ref(&victim),
                crate::combat::SHIP_CAUSE_COMBAT,
            ),
            1
        );
        let mut resolver = armed_resolver(&state);
        // Resolver order can revisit declined players after another seat resolves. Select the
        // copied stable ID whenever it is offered and decline native Milor and every other
        // reaction window, independent of how many asks precede it.
        let mut table = selecting_timing_ability(copied);
        let mut dice = crate::dice::Dice::new();
        let mut rng = crate::rng::GameRng::new(5);
        let mut sequence = crate::event::EventSequence::new();
        let mut resolving = Resolving {
            content,
            sources: DEFAULT,
            dice: &mut dice,
            rng: &mut rng,
            table: &mut table,
            timing: Some(TimingHandle {
                resolver: &mut resolver,
                sequence: &mut sequence,
                galaxy: None,
            }),
        };
        crate::combat::announce_staged_destructions(&mut state, &mut resolving);
        assert!(state.pending_destructions.is_empty());

        assert_eq!(
            fighters_in(&state, &system, &c()),
            2,
            "the destroyed unit owner places"
        );
        assert_eq!(
            state.player(&a()).unwrap().leaders.get(&source),
            Some(&LeaderStatus::Readied),
            "copying leaves Brother Milor's status unchanged"
        );
        assert_eq!(
            state.player(&b()).unwrap().leaders.get(&borrower),
            Some(&LeaderStatus::Exhausted),
            "only Ssruu is exhausted"
        );
        assert_ne!(copied, native, "the copied option has a distinct stable ID");
    }

    /// The copy carries the printed trigger too, not a looser one: a destruction the shared
    /// route staged as a Nova Seed loss does not open the copied window.
    #[test]
    fn ssruu_copies_milor_only_for_a_destruction_during_combat() {
        let mut state = seated_game(&[("a", "yin"), ("b", "yssaril"), ("c", "sol")], DEFAULT);
        let system = SystemId::new("18");
        put(&mut state, &system, "cruiser", &c(), 1);
        let content = ContentStore::embedded();
        let victim = crate::combat::ships_of(&state, content, DEFAULT, &c(), &system)
            .into_iter()
            .find(|unit| unit.type_id.as_str() == "cruiser")
            .expect("the actor's cruiser is in the system");
        // The same shared route as the passing fixture, with the provenance Nova Seed states.
        assert_eq!(
            crate::combat::destroy_units(
                &mut state,
                content,
                DEFAULT,
                &c(),
                &system,
                std::slice::from_ref(&victim),
                "breakthrough:nova_seed",
            ),
            1
        );
        let copied = "leader:yin:a:yssarilagent:yinagent:SHIP_DESTROYED:after";
        let mut resolver = armed_resolver(&state);
        let mut table = selecting_timing_ability(copied);
        let mut dice = crate::dice::Dice::new();
        let mut rng = crate::rng::GameRng::new(5);
        let mut sequence = crate::event::EventSequence::new();
        let mut resolving = Resolving {
            content,
            sources: DEFAULT,
            dice: &mut dice,
            rng: &mut rng,
            table: &mut table,
            timing: Some(TimingHandle {
                resolver: &mut resolver,
                sequence: &mut sequence,
                galaxy: None,
            }),
        };
        crate::combat::announce_staged_destructions(&mut state, &mut resolving);
        assert!(state.pending_destructions.is_empty());
        assert_eq!(
            fighters_in(&state, &system, &c()),
            0,
            "the copied trigger is the printed one"
        );
        assert_eq!(
            state
                .player(&b())
                .unwrap()
                .leaders
                .get(&LeaderId::new("yssarilagent")),
            Some(&LeaderStatus::Readied),
            "an unmet trigger does not spend Ssruu"
        );
    }

    #[test]
    fn ssruu_copies_exhausted_milor_ground_branch_for_the_event_actor() {
        let mut state = seated_game(&[("a", "yin"), ("b", "yssaril"), ("c", "sol")], DEFAULT);
        let (system, planet) = crate::fixtures::a_placed_planet();
        let source = LeaderId::new("yinagent");
        let borrower = LeaderId::new("yssarilagent");
        state
            .player_mut(&a())
            .unwrap()
            .leaders
            .insert(source.clone(), LeaderStatus::Exhausted);
        emit(
            &mut state,
            &["leader:yin:a:yssarilagent:yinagent:GROUND_FORCE_DESTROYED:after"],
            "GROUND_FORCE_DESTROYED",
            &ground_destroyed_for(&system, &planet, "c", "ground_combat"),
        );

        assert_eq!(
            infantry_of(&state, &system, &planet, &c(), "infantry"),
            2,
            "the destroyed unit owner receives the infantry"
        );
        assert_eq!(
            state.player(&a()).unwrap().leaders.get(&source),
            Some(&LeaderStatus::Exhausted),
            "an already exhausted source remains exhausted"
        );
        assert_eq!(
            state.player(&b()).unwrap().leaders.get(&borrower),
            Some(&LeaderStatus::Exhausted),
            "the borrower pays the use"
        );
    }

    #[test]
    fn ssruu_milor_copy_tracks_source_and_borrower_availability_live() {
        let (system, planet) = crate::fixtures::a_placed_planet();
        let source = LeaderId::new("yinagent");
        let borrower = LeaderId::new("yssarilagent");

        // A locked source is present in the static roster but is not a borrowable agent.
        let mut locked_source =
            seated_game(&[("a", "yin"), ("b", "yssaril"), ("c", "sol")], DEFAULT);
        locked_source
            .player_mut(&a())
            .unwrap()
            .leaders
            .insert(source.clone(), LeaderStatus::Locked);
        emit(
            &mut locked_source,
            &[],
            "GROUND_FORCE_DESTROYED",
            &ground_destroyed(&system, &planet, "ground_combat"),
        );
        assert_eq!(
            locked_source.player(&b()).unwrap().leaders.get(&borrower),
            Some(&LeaderStatus::Readied)
        );

        // A source that changes after registration but before dispatch is rechecked by the
        // callback condition. It does not rely on the status captured while the Resolver was armed.
        let mut changes_after_arm =
            seated_game(&[("a", "yin"), ("b", "yssaril"), ("c", "sol")], DEFAULT);
        let mut resolver = armed_resolver(&changes_after_arm);
        changes_after_arm
            .player_mut(&a())
            .unwrap()
            .leaders
            .insert(source.clone(), LeaderStatus::Locked);
        let mut table = scripted(&[]);
        with_context(
            &mut changes_after_arm,
            DEFAULT,
            None,
            &mut table,
            |context| {
                let event = context
                    .event_sequence
                    .next(
                        "SHIP_DESTROYED",
                        destroyed(&system, "c", "cruiser")
                            .into_iter()
                            .map(|(key, value)| (key.to_owned(), value))
                            .collect(),
                    )
                    .expect("event id");
                resolver
                    .emit_with_context(context, event, |_, _| {})
                    .expect("locked source skips without a choice");
            },
        );
        assert_eq!(fighters_in(&changes_after_arm, &system, &c()), 0);
        assert_eq!(
            changes_after_arm
                .player(&b())
                .unwrap()
                .leaders
                .get(&borrower),
            Some(&LeaderStatus::Readied)
        );

        // An exhausted Ssruu cannot copy a still-readied source.
        let mut spent_borrower =
            seated_game(&[("a", "yin"), ("b", "yssaril"), ("c", "sol")], DEFAULT);
        spent_borrower
            .player_mut(&b())
            .unwrap()
            .leaders
            .insert(borrower.clone(), LeaderStatus::Exhausted);
        emit(
            &mut spent_borrower,
            &["decline"],
            "SHIP_DESTROYED",
            &destroyed(&system, "c", "cruiser"),
        );
        assert_eq!(fighters_in(&spent_borrower, &system, &c()), 0);
        assert_eq!(
            spent_borrower.player(&a()).unwrap().leaders.get(&source),
            Some(&LeaderStatus::Readied)
        );

        // Native Milor keeps its original option ID and source-owned exhaustion behavior.
        let mut native_compatible =
            seated_game(&[("a", "yin"), ("b", "yssaril"), ("c", "sol")], DEFAULT);
        let mut resolver = armed_resolver(&native_compatible);
        let mut table = selecting_timing_ability("leader:yin:yinagent:SHIP_DESTROYED:after");
        with_context(
            &mut native_compatible,
            DEFAULT,
            None,
            &mut table,
            |context| {
                let event = context
                    .event_sequence
                    .next(
                        "SHIP_DESTROYED",
                        destroyed(&system, "c", "cruiser")
                            .into_iter()
                            .map(|(key, value)| (key.to_owned(), value))
                            .collect(),
                    )
                    .expect("event id");
                resolver
                    .emit_with_context(context, event, |_, _| {})
                    .expect("window resolves");
            },
        );
        assert_eq!(fighters_in(&native_compatible, &system, &c()), 2);
        assert_eq!(
            native_compatible.player(&a()).unwrap().leaders.get(&source),
            Some(&LeaderStatus::Exhausted)
        );
        assert_eq!(
            native_compatible
                .player(&b())
                .unwrap()
                .leaders
                .get(&borrower),
            Some(&LeaderStatus::Readied),
            "the copied listener can be declined without spending Ssruu"
        );
    }

    #[test]
    fn brother_milor_declined_leaves_the_agent_ready() {
        let (mut state, system) = arena();
        emit(
            &mut state,
            &["decline"],
            "SHIP_DESTROYED",
            &destroyed(&system, "a", "cruiser"),
        );
        assert_eq!(fighters_in(&state, &system, &a()), 0);
        assert_eq!(
            state
                .player(&a())
                .unwrap()
                .leaders
                .get(&LeaderId::new("yinagent")),
            Some(&LeaderStatus::Readied)
        );
    }

    fn ground_destroyed(
        system: &SystemId,
        planet: &PlanetId,
        cause: &str,
    ) -> Vec<(&'static str, serde_json::Value)> {
        ground_destroyed_for(system, planet, "b", cause)
    }

    fn ground_destroyed_for(
        system: &SystemId,
        planet: &PlanetId,
        player: &str,
        cause: &str,
    ) -> Vec<(&'static str, serde_json::Value)> {
        vec![
            ("system", system.to_string().into()),
            ("planet", planet.to_string().into()),
            ("player", player.into()),
            ("unit", "infantry".into()),
            ("damaged", false.into()),
            ("cause", cause.into()),
        ]
    }

    #[test]
    fn brother_milor_gives_two_infantry_for_a_ground_force_lost_in_combat_only() {
        let (mut state, system, planet) = ground_arena(0);
        // Bombardment is not combat: nothing is offered.
        emit(
            &mut state,
            &[],
            "GROUND_FORCE_DESTROYED",
            &ground_destroyed(&system, &planet, "bombardment"),
        );
        assert_eq!(infantry_of(&state, &system, &planet, &b(), "infantry"), 0);
        emit(
            &mut state,
            &["leader:yin:yinagent:GROUND_FORCE_DESTROYED:after"],
            "GROUND_FORCE_DESTROYED",
            &ground_destroyed(&system, &planet, "ground_combat"),
        );
        assert_eq!(infantry_of(&state, &system, &planet, &b(), "infantry"), 2);
    }

    // -- Brother Omar ----------------------------------------------------------------------------

    #[test]
    fn brother_omar_unlocks_once_a_faction_ability_was_used() {
        let (mut state, system) = arena();
        let content = ContentStore::embedded();
        let omar = LeaderId::new("yincommander");
        let check =
            |state: &GameState| commander_unlocked(state, content, DEFAULT, None, &a(), &omar);
        assert_eq!(check(&state), Some(false));
        assert_eq!(
            commander_unlocked(
                &state,
                content,
                DEFAULT,
                None,
                &a(),
                &LeaderId::new("solcommander")
            ),
            None,
            "not this module's commander"
        );
        put(&mut state, &system, "cruiser", &a(), 1);
        put(&mut state, &system, "dreadnought", &b(), 1);
        hits(&mut state, &["cruiser"], &system, CombatMoment::RoundEnded);
        assert_eq!(check(&state), Some(true));
        assert_eq!(
            commander_unlocked(&state, content, DEFAULT, None, &b(), &omar),
            Some(false),
            "another seat's use does not count"
        );
    }

    fn unlock_omar(state: &mut GameState) {
        state
            .player_mut(&a())
            .unwrap()
            .leaders
            .insert(LeaderId::new("yincommander"), LeaderStatus::Unlocked);
    }

    #[test]
    fn brother_omar_supplies_green_and_waives_prerequisites_for_an_infantry() {
        let content = ContentStore::embedded();
        let (mut state, _) = arena();
        // A technology someone else owns, that Yin cannot otherwise research.
        let tech = TechnologyId::new("ws");
        state
            .player_mut(&b())
            .unwrap()
            .technologies
            .insert(tech.clone());
        assert!(extra_prerequisite_colours(&state, content, &a()).is_empty());
        assert!(
            research_waiver_offer(&state, content, &a(), &tech).is_none(),
            "locked"
        );
        assert!(!crate::technology::can_research(
            &state,
            content,
            DEFAULT,
            &a(),
            &tech
        ));
        unlock_omar(&mut state);
        assert_eq!(
            extra_prerequisite_colours(&state, content, &a()),
            vec![("green".to_owned(), 1)]
        );
        let waiver = research_waiver_offer(&state, content, &a(), &tech).expect("commander waiver");
        assert!(
            !waiver.payments.is_empty(),
            "each legal infantry location is a player choice"
        );
        assert!(
            research_waiver_offer(&state, content, &a(), &TechnologyId::new("gd")).is_none(),
            "nobody else owns it"
        );
        assert!(crate::technology::can_research(
            &state,
            content,
            DEFAULT,
            &a(),
            &tech
        ));
        let payment = waiver.payments[0].id.clone();
        let before = state.clone();
        assert!(
            !crate::technology::research_with_waiver(
                &mut state,
                content,
                DEFAULT,
                &a(),
                &tech,
                0,
                "not-an-infantry-site"
            ),
            "an illegal payment target changes nothing"
        );
        assert_eq!(state, before);
        assert!(crate::technology::research_with_waiver(
            &mut state,
            content,
            DEFAULT,
            &a(),
            &tech,
            0,
            &payment
        ));
        assert!(state.player(&a()).unwrap().technologies.contains(&tech));
        // Another seat's commander does nothing for Yin.
        assert!(extra_prerequisite_colours(&state, content, &b()).is_empty());
    }

    #[test]
    fn brother_omar_prerequisite_and_waiver_follow_alliance_rights_for_any_faction() {
        let content = ContentStore::embedded();
        let mut state = seated_game(&[("a", "sol"), ("b", "hacan")], DEFAULT);
        let (system, planet) = crate::fixtures::a_placed_planet();
        crate::fixtures::put_on_planet(&mut state, &system, &planet, "infantry", &a(), 1);
        let tech = TechnologyId::new("ws");
        state
            .player_mut(&b())
            .unwrap()
            .technologies
            .insert(tech.clone());

        assert!(extra_prerequisite_colours(&state, content, &a()).is_empty());
        assert!(research_waiver_offer(&state, content, &a(), &tech).is_none());
        assert!(crate::promissory::grant_commander_ability(
            &mut state,
            content,
            &a(),
            "yincommander"
        ));
        assert_eq!(
            extra_prerequisite_colours(&state, content, &a()),
            vec![("green".to_owned(), 1)]
        );
        assert!(research_waiver_offer(&state, content, &a(), &tech).is_some());
        assert!(extra_prerequisite_colours(&state, content, &b()).is_empty());

        state
            .faction_marks
            .remove("commander_ability:a:yincommander");
        assert!(extra_prerequisite_colours(&state, content, &a()).is_empty());
        assert!(research_waiver_offer(&state, content, &a(), &tech).is_none());
    }

    #[test]
    fn brother_omar_green_pays_a_single_green_prerequisite() {
        let content = ContentStore::embedded();
        let (mut state, _) = arena();
        let green = content
            .records(ti4_model::content_types::ContentType::Technologies)
            .iter()
            .filter(|r| r.text("requirements") == Some("G") && r.text("faction").is_none())
            .filter_map(|r| r.text("alias"))
            .map(TechnologyId::new)
            .find(|t| !crate::technology::is_unit_upgrade(content, t))
            .expect("a single-green technology");
        let seat = state.player_mut(&a()).unwrap();
        seat.technologies.clear();
        assert!(!crate::technology::can_research(
            &state,
            content,
            DEFAULT,
            &a(),
            &green
        ));
        unlock_omar(&mut state);
        assert!(crate::technology::can_research(
            &state,
            content,
            DEFAULT,
            &a(),
            &green
        ));
    }

    fn hero_state() -> (GameState, SystemId, PlanetId, ti4_content::galaxy::Galaxy) {
        let mut state = seated_game(&[("a", "yin"), ("b", "sol")], DEFAULT);
        // Sol's own agent has a ground-combat window since rules version 16; these scripts do not
        // answer it.
        state
            .player_mut(&b())
            .unwrap()
            .leaders
            .insert(LeaderId::new("solagent"), LeaderStatus::Exhausted);
        let centre = ti4_content::galaxy::all_planets(ContentStore::embedded(), DEFAULT)
            .iter()
            .find(|(_, planet)| {
                planet.homeworld_of().is_none()
                    && !planet.is_placed_during_play()
                    && planet.system_id().is_some()
            })
            .and_then(|(_, planet)| planet.system_id().map(ToOwned::to_owned))
            .expect("a non-home planet");
        let hub = crate::fixtures::hub_with_centre(&centre);
        let (system, planet) = hero_spots(ContentStore::embedded(), Some(&hub.galaxy))
            .into_iter()
            .next()
            .expect("a non-home planet");
        (state, system, planet, hub.galaxy)
    }

    fn use_hero(
        state: &mut GameState,
        galaxy: &ti4_content::galaxy::Galaxy,
        answers: &[&str],
    ) -> Option<bool> {
        let mut table = scripted(answers);
        with_context(state, DEFAULT, Some(galaxy), &mut table, |ctx| {
            use_leader(ctx, &a(), &LeaderId::new("yinhero"))
        })
    }

    fn use_hero_timed(
        state: &mut GameState,
        galaxy: &ti4_content::galaxy::Galaxy,
        answers: &[&str],
        faces: &[u32],
    ) -> (bool, crate::dice::Dice) {
        let (result, dice) = use_hero_timed_result(state, galaxy, answers, faces);
        (result.expect("legal hero timing choices"), dice)
    }

    fn use_hero_timed_result(
        state: &mut GameState,
        galaxy: &ti4_content::galaxy::Galaxy,
        answers: &[&str],
        faces: &[u32],
    ) -> (Result<bool, TimingError>, crate::dice::Dice) {
        let mut resolver = armed_resolver(state);
        let mut table = scripted(answers);
        let mut dice = crate::dice::Dice::from_faces(faces.iter().copied());
        let mut rng = crate::rng::GameRng::new(5);
        let mut event_sequence = crate::event::EventSequence::new();
        let mut context = TimingContext {
            state,
            content: ContentStore::embedded(),
            sources: DEFAULT,
            table: &mut table,
            dice: &mut dice,
            rng: &mut rng,
            event_sequence: &mut event_sequence,
            galaxy: Some(galaxy),
        };
        let done = crate::leaders::use_leader_timed(
            &mut context,
            &mut resolver,
            &a(),
            &LeaderId::new("yinhero"),
        );
        (done, dice)
    }

    fn unlock_hero(state: &mut GameState) {
        state
            .player_mut(&a())
            .unwrap()
            .leaders
            .insert(LeaderId::new("yinhero"), LeaderStatus::Unlocked);
    }

    #[test]
    fn dannel_lands_up_to_three_infantry_and_takes_an_empty_planet() {
        let (mut state, system, planet, galaxy) = hero_state();
        let content = ContentStore::embedded();
        assert_eq!(
            leader_action(&state, content, &a(), &LeaderId::new("yinhero")),
            Some(true)
        );
        assert!(
            hero_spots(content, Some(&galaxy))
                .iter()
                .all(|(s, _)| !ti4_content::galaxy::is_home_system(content, s.as_str(), DEFAULT)),
            "no home planet is offered"
        );
        let spot = format!("{system}|{planet}");
        let done = use_hero(&mut state, &galaxy, &[&spot, &spot, &spot, &spot]);
        assert_eq!(done, Some(true));
        assert_eq!(
            infantry_of(&state, &system, &planet, &a(), "infantry"),
            3,
            "never more than 3"
        );
        assert_eq!(
            state.system_state(&system).planet_control.get(&planet),
            Some(&a()),
            "uncontested, so taken"
        );
    }

    #[test]
    fn dannel_fights_the_defenders_to_the_end_and_declining_changes_nothing() {
        let (mut state, system, planet, galaxy) = hero_state();
        put_on_planet(&mut state, &system, &planet, "infantry", &b(), 1);
        let before = state.clone();
        assert_eq!(use_hero(&mut state, &galaxy, &["decline"]), Some(false));
        assert_eq!(
            state, before,
            "nothing asked beyond the choice, nothing changed"
        );
        let spot = format!("{system}|{planet}");
        assert_eq!(
            use_hero(&mut state, &galaxy, &[&spot, &spot, "decline"]),
            Some(true)
        );
        let (mine, theirs) = (
            infantry_of(&state, &system, &planet, &a(), "infantry"),
            infantry_of(&state, &system, &planet, &b(), "infantry"),
        );
        assert!(
            mine == 0 || theirs == 0,
            "the combat ran to its end: {mine} v {theirs}"
        );
        let holder = state
            .system_state(&system)
            .planet_control
            .get(&planet)
            .cloned();
        assert_eq!(holder == Some(a()), mine > 0);
        // Not this module's leader.
        let mut table = scripted(&[]);
        assert_eq!(
            with_context(&mut state, DEFAULT, None, &mut table, |ctx| use_leader(
                ctx,
                &a(),
                &LeaderId::new("yinagent")
            )),
            None
        );
    }

    #[test]
    fn dannel_continues_against_every_rival_ground_force() {
        let (_, system, planet, galaxy) = hero_state();
        let mut state = seated_game(&[("a", "yin"), ("b", "sol"), ("c", "hacan")], DEFAULT);
        put_on_planet(&mut state, &system, &planet, "infantry", &b(), 1);
        put_on_planet(&mut state, &system, &planet, "infantry", &c(), 1);
        let spot = format!("{system}|{planet}");

        assert_eq!(
            use_hero(&mut state, &galaxy, &[&spot, &spot, &spot]),
            Some(true)
        );
        let mine = infantry_of(&state, &system, &planet, &a(), "infantry");
        if mine > 0 {
            assert_eq!(
                infantry_of(&state, &system, &planet, &b(), "infantry")
                    + infantry_of(&state, &system, &planet, &c(), "infantry"),
                0,
                "a surviving Yin force fights every rival on the planet"
            );
        }
    }

    #[test]
    fn timed_dannel_runs_indoctrination_milor_and_skips_space_cannon() {
        let (mut state, system, planet, galaxy) = hero_state();
        unlock_hero(&mut state);
        state.player_mut(&a()).unwrap().trade_goods = 5;
        state
            .player_mut(&a())
            .unwrap()
            .leaders
            .insert(LeaderId::new("yinagent"), LeaderStatus::Readied);
        put_on_planet(&mut state, &system, &planet, "pds", &b(), 1);
        put_on_planet(&mut state, &system, &planet, "infantry", &b(), 2);
        let spot = format!("{system}|{planet}");
        let (done, dice) = use_hero_timed(
            &mut state,
            &galaxy,
            &[
                &spot,
                "decline",
                "infantry",
                "trade_good",
                "trade_good",
                "leader:yin:yinagent:GROUND_FORCE_DESTROYED:after",
            ],
            &[10, 10, 1, 10, 10, 1, 1],
        );
        assert!(done);
        assert!(dice.rolled("space cannon defense").is_empty());
        assert_eq!(state.player(&a()).unwrap().trade_goods, 3);
        assert!(state.player(&a()).unwrap().faction.as_str() == "yin");
        assert_eq!(
            state
                .player(&a())
                .unwrap()
                .leaders
                .get(&LeaderId::new("yinagent")),
            Some(&LeaderStatus::Exhausted),
            "Brother Milor saw the ground-force casualty window",
        );
        assert!(
            state
                .player(&a())
                .unwrap()
                .leaders
                .get(&LeaderId::new("yinhero"))
                == Some(&LeaderStatus::Purged),
            "the hero is purged once after resolution",
        );
        assert_eq!(infantry_of(&state, &system, &planet, &b(), "infantry"), 0);
    }

    #[test]
    fn timed_dannel_fights_every_rival_and_invalid_placement_is_atomic() {
        let (_, system, planet, galaxy) = hero_state();
        let mut state = seated_game(&[("a", "yin"), ("b", "sol"), ("c", "hacan")], DEFAULT);
        state
            .player_mut(&b())
            .unwrap()
            .leaders
            .insert(LeaderId::new("solagent"), LeaderStatus::Exhausted);
        unlock_hero(&mut state);
        put_on_planet(&mut state, &system, &planet, "infantry", &b(), 1);
        put_on_planet(&mut state, &system, &planet, "infantry", &c(), 1);
        let before = state.clone();
        let (result, _) =
            use_hero_timed_result(&mut state, &galaxy, &["bad-target"], &[10, 1, 10, 1]);
        assert!(result.is_err());
        assert_eq!(state, before, "a bad placement answer changes nothing");

        let spot = format!("{system}|{planet}");
        let (done, _) = use_hero_timed(
            &mut state,
            &galaxy,
            &[&spot, "decline", "decline", "decline"],
            &[10, 1, 10, 1],
        );
        assert!(done);
        assert_eq!(infantry_of(&state, &system, &planet, &b(), "infantry"), 0);
        assert_eq!(infantry_of(&state, &system, &planet, &c(), "infantry"), 0);
    }

    #[test]
    fn every_claim_is_on_the_sheet_and_the_module_is_registered() {
        assert!(crate::factions::module("yin").is_some());
        let missing = crate::factions::missing(ContentStore::embedded(), DEFAULT, "yin");
        let ids: Vec<&str> = missing.iter().map(|asset| asset.id.as_str()).collect();
        assert!(ids.is_empty(), "every Yin asset is claimed: {ids:?}");
    }
    #[test]
    fn timed_dannel_opens_combat_secret_scoring_before_continuing() {
        let (mut state, system, planet, galaxy) = hero_state();
        unlock_hero(&mut state);
        state
            .player_mut(&a())
            .unwrap()
            .leaders
            .insert(LeaderId::new("yinagent"), LeaderStatus::Exhausted);
        state.player_mut(&a()).unwrap().secret_objectives =
            vec![ti4_model::id::SecretObjectiveId::new("sar")];
        state.player_mut(&b()).unwrap().victory_points = 3;
        state.player_mut(&a()).unwrap().trade_goods = 0;
        let held: Vec<_> = state
            .controlled_planets(&a())
            .into_iter()
            .map(|(_, planet)| planet.clone())
            .collect();
        state.exhausted_planets.extend(held);
        put_on_planet(&mut state, &system, &planet, "infantry", &b(), 1);
        let spot = format!("{system}|{planet}");
        let before = state.player(&a()).unwrap().victory_points;
        assert!(use_hero_timed(&mut state, &galaxy, &[&spot, "decline", "sar"], &[10, 1]).0);
        assert_eq!(state.player(&a()).unwrap().victory_points, before + 1);
        assert!(state.player(&a()).unwrap().secret_objectives.is_empty());
    }

    #[test]
    fn timed_dannel_propagates_an_invalid_combat_reaction_without_spending_the_hero() {
        let (mut state, system, planet, galaxy) = hero_state();
        unlock_hero(&mut state);
        state.player_mut(&a()).unwrap().trade_goods = 2;
        put_on_planet(&mut state, &system, &planet, "infantry", &b(), 1);
        let before = state.clone();
        let spot = format!("{system}|{planet}");
        let (result, _) = use_hero_timed_result(
            &mut state,
            &galaxy,
            &[&spot, "decline", "not-an-indoctrination-option"],
            &[10, 1],
        );
        assert!(
            result.is_err(),
            "an illegal timing reaction must reach the Game caller"
        );
        assert_eq!(state.board, before.board);
        assert_eq!(state.players, before.players);
        assert_eq!(state.faction_marks, before.faction_marks);
    }

    #[test]
    fn a_nekro_flagship_with_the_yin_z_token_destroys_every_ship_in_its_system() {
        let run = |lent: &[&str]| {
            let mut state = crate::fixtures::nekro_with_z(&[("a", "nekro"), ("b", "sol")], lent);
            let system = SystemId::new("18");
            put(&mut state, &system, "cruiser", &b(), 2);
            emit(
                &mut state,
                &[],
                "SHIP_DESTROYED",
                &destroyed(&system, "a", "nekro_flagship"),
            );
            count(&state, &system, "cruiser")
        };
        assert_eq!(run(&[]), 2, "off by default");
        assert_eq!(run(&["yin"]), 0);
    }
}
