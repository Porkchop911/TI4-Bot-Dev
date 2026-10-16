//! Ssruu copies of the two combat-round agents that select one unit for one extra die.
//!
//! This is deliberately a standalone registration package. The coordinator registers
//! [`abilities`] and invokes [`extra_die_for`] from per-unit roll sites.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use ti4_content::ContentStore;
use ti4_model::content_types::SourceSet;
use ti4_model::id::{LeaderId, PlanetId, PlayerId, SystemId, UnitTypeId};
use ti4_model::state::GameState;
use ti4_model::units::Unit;

use crate::choice::{Choice, ChoiceOption};
use crate::decision_context::{DecisionContext, DecisionSource};
use crate::event::Event;
use crate::timing::{Ability, Relation, TimingContext, TimingError};

const LETNEV_AGENT: &str = "letnevagent";
const SOL_AGENT: &str = "solagent";
const MARK_PREFIX: &str = "yssaril:ssruu-round-agent:";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct RoundBonus {
    borrower: PlayerId,
    source_owner: PlayerId,
    source_agent: String,
    system: SystemId,
    /// Ground-combat planet named by the triggering event (the actual battle).
    combat_planet: Option<PlanetId>,
    /// Location of the selected unit. This can differ from `combat_planet` for Sol.
    unit_planet: Option<PlanetId>,
    unit_owner: PlayerId,
    unit_type: UnitTypeId,
    sustained_damage: bool,
    galvanized: bool,
    /// Ordinal in the owner's complete eligible roll roster at this location.
    unit_index: usize,
    round_seq: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CopyPlan {
    system: SystemId,
    combat_planet: Option<PlanetId>,
    round_seq: u32,
    candidates: Vec<UnitTarget>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct UnitTarget {
    owner: PlayerId,
    unit_type: UnitTypeId,
    /// Location of a ground force; ships have no planet location.
    planet: Option<PlanetId>,
    sustained_damage: bool,
    galvanized: bool,
    /// Ordinal in this owner's complete eligible roll roster at this location.
    index: usize,
}

/// Fixed AFTER slots for every seated Yssaril borrower and each Sol/Letnev source seat.
///
/// Exhaustion and source rights are checked dynamically, so this roster remains stable when
/// either agent changes status during the game.
#[must_use]
pub fn abilities(state: &GameState) -> Vec<Ability> {
    let borrowers: Vec<_> = state
        .players
        .iter()
        .filter(|seat| seat.faction.as_str() == "yssaril")
        .map(|seat| seat.id.clone())
        .collect();
    let sources: Vec<_> = state
        .players
        .iter()
        .filter(|seat| matches!(seat.faction.as_str(), "sol" | "letnev"))
        .map(|seat| (seat.id.clone(), seat.faction.to_string()))
        .collect();

    borrowers
        .into_iter()
        .flat_map(|borrower| {
            let borrower_for_filter = borrower.clone();
            sources
                .iter()
                .filter(move |(owner, _)| owner != &borrower_for_filter)
                .map(move |(source_owner, faction)| {
                    let source_owner = source_owner.clone();
                    let borrower = borrower.clone();
                    let agent = if faction == "sol" {
                        SOL_AGENT
                    } else {
                        LETNEV_AGENT
                    }
                    .to_owned();
                    let event_type = event_for(&agent).to_owned();
                    let effect_borrower = borrower.clone();
                    let effect_source = source_owner.clone();
                    let effect_agent = agent.clone();
                    let condition_borrower = borrower.clone();
                    let condition_source = source_owner.clone();
                    let condition_agent = agent.clone();

                    Ability::stateful(
                        format!(
                            "leader:ssruu-copy:{borrower}:{agent}:{source_owner}:{event_type}:after"
                        ),
                        borrower,
                        event_type,
                        Relation::After,
                        Arc::new(move |event, _, context| {
                            resolve_copy(
                                event,
                                context,
                                &effect_borrower,
                                &effect_source,
                                &effect_agent,
                            )
                        }),
                    )
                    .with_optional(true)
                    .with_stateful_condition(Arc::new(
                        move |event, _, context| {
                            copy_plan(
                                context.state,
                                context.content,
                                context.sources,
                                event,
                                &condition_borrower,
                                &condition_source,
                                &condition_agent,
                            )
                            .is_some()
                        },
                    ))
                })
        })
        .collect()
}

/// Whether any Yssaril can currently borrow this exact agent, for event-producer gating.
#[must_use]
pub fn has_round_copy(state: &GameState, content: &ContentStore, source_agent: &str) -> bool {
    state.players.iter().any(|seat| {
        seat.faction.as_str() == "yssaril"
            && crate::factions::hooks_cards::borrowable_agents(state, content, &seat.id)
                .iter()
                .any(|(_, id)| id.as_str() == source_agent)
    })
}

/// Read copied per-unit combat dice bonuses. The root calls this once for each roll candidate.
///
/// `unit_index` is the ordinal in the exact eligible roll roster: `combat::ships_of` for space or
/// `invasion::is_ground_force_here` for ground. A live unit signature check makes removals or
/// reorderings inert instead of transferring the bonus to another type, owner, location, or
/// differently marked unit. An old, malformed, or differently scoped mark returns 0.
#[must_use]
pub fn extra_die_for(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    system: &SystemId,
    planet: Option<&PlanetId>,
    player: &PlayerId,
    unit_type: &str,
    unit_index: usize,
    round_seq: u32,
) -> i64 {
    if state.combat_round_seq != round_seq {
        return 0;
    }
    let Some(target) = live_target(state, content, sources, system, planet, player, unit_index)
    else {
        return 0;
    };
    if target.type_id.as_str() != unit_type {
        return 0;
    }
    state
        .faction_marks
        .iter()
        .filter(|(key, _)| key.starts_with(MARK_PREFIX))
        .filter_map(|(_, value)| serde_json::from_str::<RoundBonus>(value).ok())
        .filter(|mark| {
            mark.system == *system
                && mark.combat_planet.as_ref() == planet
                && mark.unit_planet.as_ref() == planet
                && mark.unit_owner == *player
                && mark.unit_type.as_str() == unit_type
                && mark.unit_index == unit_index
                && mark.round_seq == round_seq
                && target.sustained_damage == mark.sustained_damage
                && target.galvanized == mark.galvanized
        })
        .count() as i64
}

fn event_for(agent: &str) -> &'static str {
    if agent == SOL_AGENT {
        "GROUND_COMBAT_ROUND_STARTED"
    } else {
        "COMBAT_ROUND_STARTED"
    }
}

fn source_is_borrowable(
    state: &GameState,
    content: &ContentStore,
    borrower: &PlayerId,
    source_owner: &PlayerId,
    source_agent: &str,
) -> bool {
    crate::factions::hooks_cards::borrowable_agents(state, content, borrower)
        .iter()
        .any(|(owner, agent)| owner == source_owner && agent.as_str() == source_agent)
}

fn copy_plan(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    event: &Event,
    borrower: &PlayerId,
    source_owner: &PlayerId,
    source_agent: &str,
) -> Option<CopyPlan> {
    if event.event_type != event_for(source_agent)
        || !source_is_borrowable(state, content, borrower, source_owner, source_agent)
    {
        return None;
    }
    let system = SystemId::new(event.text("system")?);
    if state.board.get(&system).is_none() {
        return None;
    }
    let round_seq = u32::try_from(event.integer("round_seq")?).ok()?;
    if round_seq != state.combat_round_seq {
        return None;
    }
    let attacker = PlayerId::new(event.text("attacker")?);
    let defender = PlayerId::new(event.text("defender")?);
    if attacker == defender
        || state.player(&attacker).is_none()
        || state.player(&defender).is_none()
    {
        return None;
    }

    let combat_planet = if source_agent == SOL_AGENT {
        let planet = PlanetId::new(event.text("planet")?);
        if !state.board.get(&system)?.planet_units.contains_key(&planet) {
            return None;
        }
        Some(planet)
    } else {
        None
    };
    let mut candidates = Vec::new();
    for seat in &state.players {
        if combat_planet.is_some() {
            // Sol's text allows any ground force in the active system, even on a planet
            // outside the current battle. Keep each planet's roll ordinal independent.
            let board = state.board.get(&system)?;
            for (unit_planet, units) in &board.planet_units {
                let roster: Vec<_> = units
                    .iter()
                    .filter(|unit| unit.owner == seat.id)
                    .filter(|unit| {
                        crate::invasion::is_ground_force_here(
                            state,
                            content,
                            sources,
                            &system,
                            unit_planet,
                            unit,
                        )
                    })
                    .collect();
                candidates.extend(roster.into_iter().enumerate().map(|(roster_index, unit)| {
                    target_from_unit(unit, Some(unit_planet.clone()), roster_index)
                }));
            }
        } else {
            let roster = crate::combat::ships_of(state, content, sources, &seat.id, &system);
            candidates.extend(
                roster
                    .iter()
                    .enumerate()
                    .map(|(roster_index, unit)| target_from_unit(unit, None, roster_index)),
            );
        }
    }
    (!candidates.is_empty()).then_some(CopyPlan {
        system,
        combat_planet,
        round_seq,
        candidates,
    })
}

fn resolve_copy(
    event: &Event,
    context: &mut TimingContext<'_>,
    borrower: &PlayerId,
    source_owner: &PlayerId,
    source_agent: &str,
) -> Result<(), TimingError> {
    let Some(plan) = copy_plan(
        context.state,
        context.content,
        context.sources,
        event,
        borrower,
        source_owner,
        source_agent,
    ) else {
        return Ok(());
    };
    // Display only: each candidate by unit and place, with whose it is.
    let captions: Vec<(String, serde_json::Value)> = plan
        .candidates
        .iter()
        .map(|target| {
            let place = target.planet.as_ref().map_or_else(
                || "In the fleet".to_owned(),
                |planet| {
                    let name = ti4_content::galaxy::planet(
                        context.content,
                        planet.as_str(),
                        context.sources,
                    )
                    .and_then(|record| record.name())
                    .unwrap_or(planet.as_str());
                    format!("On {name}")
                },
            );
            (
                target_id(target),
                crate::choice::offer_caption_for_seat(
                    &format!("{} · {place}", target.unit_type),
                    Some(&format!("Unit {} of that kind there", target.index + 1)),
                    target.owner.as_str(),
                ),
            )
        })
        .collect();
    let options = plan
        .candidates
        .iter()
        .map(|target| {
            let id = target_id(target);
            ChoiceOption::labelled(
                id,
                "leader_ssruu_round_agent_unit",
                format!(
                    "{}'s {}{} (unit {})",
                    target.owner,
                    target.unit_type,
                    target
                        .planet
                        .as_ref()
                        .map(|planet| format!(" on {planet}"))
                        .unwrap_or_default(),
                    target.index + 1
                ),
            )
        })
        .collect();
    let choice = Choice::new(
        borrower.clone(),
        format!("Ssruu copying {source_agent}: choose one unit for +1 combat die"),
        options,
    )
    .offered(
        crate::strategy_cards::leader_card(
            context.content,
            source_agent,
            "agent (copied by Ssruu)",
        ),
        Vec::new(),
        &captions
            .iter()
            .map(|(id, caption)| (id.as_str(), caption.clone()))
            .collect::<Vec<_>>(),
    )
    .contextualized(DecisionContext::new(
        borrower.clone(),
        DecisionSource::Content(source_agent.to_owned()),
        "leader_ssruu_round_agent_unit",
        context.state.phase,
        context.state.round,
    ));
    let answer = context
        .ask_seeing(&choice)
        .map_err(TimingError::IllegalChoice)?;
    let Some(target) = plan
        .candidates
        .into_iter()
        .find(|target| target_id(target) == answer.id)
    else {
        return Ok(());
    };

    // Recheck live rights and the selected unit immediately before recording the effect.
    let Some(current_plan) = copy_plan(
        context.state,
        context.content,
        context.sources,
        event,
        borrower,
        source_owner,
        source_agent,
    ) else {
        return Ok(());
    };
    if !current_plan.candidates.contains(&target) {
        return Ok(());
    }

    let mark = RoundBonus {
        borrower: borrower.clone(),
        source_owner: source_owner.clone(),
        source_agent: source_agent.to_owned(),
        system: plan.system,
        combat_planet: plan.combat_planet,
        unit_planet: target.planet.clone(),
        unit_owner: target.owner.clone(),
        unit_type: target.unit_type.clone(),
        sustained_damage: target.sustained_damage,
        galvanized: target.galvanized,
        unit_index: target.index,
        round_seq: plan.round_seq,
    };
    let key = format!(
        "{MARK_PREFIX}{}:{}:{}",
        borrower, source_agent, mark.round_seq
    );
    let Ok(value) = serde_json::to_string(&mark) else {
        return Ok(());
    };
    let borrower_prefix = format!("{MARK_PREFIX}{borrower}:");
    context
        .state
        .faction_marks
        .retain(|existing, _| !existing.starts_with(&borrower_prefix));
    context.state.faction_marks.insert(key, value);
    // Through `leaders::exhaust` so a watching Nomad's Temporal Command Suite hears it.
    crate::leaders::exhaust(context.state, borrower, &LeaderId::new("yssarilagent"));
    if borrower != &target.owner {
        crate::diplomacy::evaluate_event(
            context.state,
            &crate::diplomacy::DiplomacyEventContext::LeaderUsedFor {
                user: borrower.clone(),
                leader: source_agent.to_owned(),
                beneficiary: target.owner,
            },
        )
        .expect("validated diplomacy promises settle deterministically");
    }
    crate::factions::hooks_cards::borrowed_agent_used(
        context,
        borrower,
        &LeaderId::new(source_agent),
    );
    Ok(())
}

fn target_id(target: &UnitTarget) -> String {
    format!(
        "{}|{}|{}|{}|{}",
        target.owner,
        target.planet.as_ref().map_or("space", PlanetId::as_str),
        target.unit_type,
        target.index,
        u8::from(target.sustained_damage) * 2 + u8::from(target.galvanized)
    )
}

fn target_from_unit(unit: &Unit, planet: Option<PlanetId>, index: usize) -> UnitTarget {
    UnitTarget {
        owner: unit.owner.clone(),
        unit_type: unit.type_id.clone(),
        planet,
        sustained_damage: unit.sustained_damage,
        galvanized: unit.galvanized,
        index,
    }
}

/// Current unit occupying one exact eligible roll-roster ordinal.
fn live_target(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    system: &SystemId,
    planet: Option<&PlanetId>,
    player: &PlayerId,
    unit_index: usize,
) -> Option<Unit> {
    if let Some(planet) = planet {
        state
            .board
            .get(system)?
            .planet_units
            .get(planet)?
            .iter()
            .filter(|unit| unit.owner == *player)
            .filter(|unit| {
                crate::invasion::is_ground_force_here(state, content, sources, system, planet, unit)
            })
            .nth(unit_index)
            .cloned()
    } else {
        crate::combat::ships_of(state, content, sources, player, system)
            .into_iter()
            .nth(unit_index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ti4_model::content_types::POK;
    use ti4_model::state::LeaderStatus;

    fn test_state() -> (GameState, SystemId, PlanetId) {
        let mut state = crate::fixtures::game(&["a", "b", "c"]);
        let borrower = PlayerId::new("a");
        let source = PlayerId::new("b");
        state.player_mut(&borrower).unwrap().faction = ti4_model::id::FactionId::new("yssaril");
        state.player_mut(&source).unwrap().faction = ti4_model::id::FactionId::new("letnev");
        state
            .player_mut(&borrower)
            .unwrap()
            .leaders
            .insert(LeaderId::new("yssarilagent"), LeaderStatus::Readied);
        state
            .player_mut(&source)
            .unwrap()
            .leaders
            .insert(LeaderId::new(LETNEV_AGENT), LeaderStatus::Exhausted);
        let (system, planet) = crate::fixtures::a_placed_planet();
        state.combat_round_seq = 4;
        crate::fixtures::put(&mut state, &system, "cruiser", &PlayerId::new("c"), 2);
        crate::fixtures::put_on_planet(
            &mut state,
            &system,
            &planet,
            "infantry",
            &PlayerId::new("c"),
            1,
        );
        (state, system, planet)
    }

    fn read_bonus(
        state: &GameState,
        system: &SystemId,
        planet: Option<&PlanetId>,
        player: &PlayerId,
        unit_type: &str,
        unit_index: usize,
        round_seq: u32,
    ) -> i64 {
        extra_die_for(
            state,
            ContentStore::embedded(),
            POK,
            system,
            planet,
            player,
            unit_type,
            unit_index,
            round_seq,
        )
    }

    fn emit_copy(
        state: &mut GameState,
        agent: &str,
        system: &SystemId,
        planet: &PlanetId,
        answers: &[String],
    ) -> Result<Event, TimingError> {
        let event = event(agent, system, planet, state.combat_round_seq);
        let abilities = abilities(state);
        let mut resolver = crate::timing::Resolver::new(
            vec![PlayerId::new("a"), PlayerId::new("b"), PlayerId::new("c")],
            Some(PlayerId::new("a")),
            crate::choice::Table::new(),
        );
        resolver.register(abilities);
        let mut table = crate::choice::Table::with_default(Box::new(crate::choice::Scripted::new(
            answers.to_vec(),
        )));
        let mut dice = crate::dice::Dice::new();
        let mut rng = crate::rng::GameRng::new(0);
        let mut event_sequence = crate::event::EventSequence::new();
        let mut context = TimingContext {
            state,
            content: ContentStore::embedded(),
            sources: POK,
            table: &mut table,
            dice: &mut dice,
            rng: &mut rng,
            event_sequence: &mut event_sequence,
            galaxy: None,
        };
        resolver.emit_with_context(&mut context, event, |_, _| {})
    }

    fn event(agent: &str, system: &SystemId, planet: &PlanetId, round: u32) -> Event {
        let mut payload = std::collections::BTreeMap::new();
        payload.insert("system".to_owned(), system.to_string().into());
        payload.insert("attacker".to_owned(), "a".into());
        payload.insert("defender".to_owned(), "c".into());
        payload.insert("round_seq".to_owned(), i64::from(round).into());
        if agent == SOL_AGENT {
            payload.insert("planet".to_owned(), planet.to_string().into());
        }
        Event::new(1, event_for(agent), payload)
    }

    #[test]
    fn roster_is_fixed_but_source_rights_are_live_and_exhausted_source_is_copyable() {
        let (mut state, system, planet) = test_state();
        let content = ContentStore::embedded();
        let roster = abilities(&state);
        assert!(
            roster
                .iter()
                .any(|ability| ability.id.contains("letnevagent"))
        );
        let event = event(LETNEV_AGENT, &system, &planet, 4);
        let borrower = PlayerId::new("a");
        let source = PlayerId::new("b");
        assert!(
            copy_plan(
                &state,
                content,
                POK,
                &event,
                &borrower,
                &source,
                LETNEV_AGENT
            )
            .is_some()
        );
        state
            .player_mut(&source)
            .unwrap()
            .leaders
            .insert(LeaderId::new(LETNEV_AGENT), LeaderStatus::Locked);
        assert!(
            copy_plan(
                &state,
                content,
                POK,
                &event,
                &borrower,
                &source,
                LETNEV_AGENT
            )
            .is_none()
        );
        state
            .player_mut(&source)
            .unwrap()
            .leaders
            .remove(&LeaderId::new(LETNEV_AGENT));
        assert!(
            copy_plan(
                &state,
                content,
                POK,
                &event,
                &borrower,
                &source,
                LETNEV_AGENT
            )
            .is_none()
        );
    }

    #[test]
    fn resolver_selects_one_unit_and_invalid_selection_leaves_ssruu_ready() {
        let (mut state, system, planet) = test_state();
        let ability_id = abilities(&state)
            .into_iter()
            .find(|ability| ability.id.contains(LETNEV_AGENT))
            .expect("Letnev source has a fixed copy slot")
            .id;
        let target = target_id(&UnitTarget {
            owner: PlayerId::new("c"),
            unit_type: UnitTypeId::new("cruiser"),
            planet: None,
            sustained_damage: false,
            galvanized: false,
            index: 0,
        });
        state.player_mut(&PlayerId::new("c")).unwrap().faction =
            ti4_model::id::FactionId::new("nomad");
        state
            .player_mut(&PlayerId::new("c"))
            .unwrap()
            .technologies
            .insert(ti4_model::id::TechnologyId::new("tcs"));
        emit_copy(
            &mut state,
            LETNEV_AGENT,
            &system,
            &planet,
            &[ability_id.clone(), target.clone()],
        )
        .expect("borrower can select the exact ship");
        assert_eq!(
            state.player(&PlayerId::new("a")).unwrap().leaders[&LeaderId::new("yssarilagent")],
            LeaderStatus::Exhausted,
        );
        assert!(
            crate::supply::staged_event_types(&state).contains(&"AGENT_EXHAUSTED".to_owned()),
            "a watching Nomad hears Ssruu exhausting"
        );
        assert_eq!(
            read_bonus(&state, &system, None, &PlayerId::new("c"), "cruiser", 0, 4),
            1
        );
        assert_eq!(
            read_bonus(&state, &system, None, &PlayerId::new("c"), "cruiser", 1, 4),
            0
        );

        let (mut invalid, system, planet) = test_state();
        assert!(
            emit_copy(
                &mut invalid,
                LETNEV_AGENT,
                &system,
                &planet,
                &[ability_id, "invented-unit".to_owned()],
            )
            .is_err()
        );
        assert_eq!(
            invalid.player(&PlayerId::new("a")).unwrap().leaders[&LeaderId::new("yssarilagent")],
            LeaderStatus::Readied,
        );
        assert!(invalid.faction_marks.is_empty());
    }

    /// The unit question is an offer card: the copied agent as printed and one caption per
    /// candidate unit with its place and owner (display only; the option ids are unchanged).
    #[test]
    fn the_copied_agent_unit_question_is_an_offer_card() {
        let (mut state, system, planet) = test_state();
        let ability_id = abilities(&state)
            .into_iter()
            .find(|ability| ability.id.contains(LETNEV_AGENT))
            .expect("Letnev source has a fixed copy slot")
            .id;
        let target = target_id(&UnitTarget {
            owner: PlayerId::new("c"),
            unit_type: UnitTypeId::new("cruiser"),
            planet: None,
            sustained_damage: false,
            galvanized: false,
            index: 0,
        });
        let event = event(LETNEV_AGENT, &system, &planet, state.combat_round_seq);
        let mut resolver = crate::timing::Resolver::new(
            vec![PlayerId::new("a"), PlayerId::new("b"), PlayerId::new("c")],
            Some(PlayerId::new("a")),
            crate::choice::Table::new(),
        );
        resolver.register(abilities(&state));
        let (decider, seen) =
            crate::choice::Capturing::new(Box::new(crate::choice::Scripted::new([
                ability_id,
                target.clone(),
            ])));
        let mut table = crate::choice::Table::with_default(Box::new(decider));
        let mut dice = crate::dice::Dice::new();
        let mut rng = crate::rng::GameRng::new(0);
        let mut event_sequence = crate::event::EventSequence::new();
        let mut context = TimingContext {
            state: &mut state,
            content: ContentStore::embedded(),
            sources: POK,
            table: &mut table,
            dice: &mut dice,
            rng: &mut rng,
            event_sequence: &mut event_sequence,
            galaxy: None,
        };
        resolver
            .emit_with_context(&mut context, event, |_, _| {})
            .expect("the copy resolves");
        let asked = seen.borrow();
        let offer = asked
            .iter()
            .find(|choice| {
                choice
                    .context
                    .as_ref()
                    .is_some_and(|context| context.subtype == "leader_ssruu_round_agent_unit")
            })
            .expect("the unit question was asked");
        assert_eq!(offer.details["kind"], "offer");
        assert_eq!(offer.details["card"]["tag"], "agent (copied by Ssruu)");
        let caption = &offer.details["captions"][target.as_str()];
        assert_eq!(caption["label"], "cruiser · In the fleet");
        assert_eq!(caption["seat"], "c");
    }

    #[test]
    fn declining_the_optional_copy_does_not_exhaust_ssruu_or_mark_a_unit() {
        let (mut state, system, planet) = test_state();
        emit_copy(
            &mut state,
            LETNEV_AGENT,
            &system,
            &planet,
            &["decline".to_owned()],
        )
        .expect("decline is a legal response");
        assert_eq!(
            state.player(&PlayerId::new("a")).unwrap().leaders[&LeaderId::new("yssarilagent")],
            LeaderStatus::Readied,
        );
        assert!(state.faction_marks.is_empty());
    }

    #[test]
    fn sol_ground_copy_is_scoped_to_the_event_planet_and_current_round() {
        let (mut state, system, planet) = test_state();
        let source = PlayerId::new("b");
        state.player_mut(&source).unwrap().faction = ti4_model::id::FactionId::new("sol");
        let source_status = state
            .player_mut(&source)
            .unwrap()
            .leaders
            .remove(&LeaderId::new(LETNEV_AGENT));
        assert_eq!(source_status, Some(LeaderStatus::Exhausted));
        state
            .player_mut(&source)
            .unwrap()
            .leaders
            .insert(LeaderId::new(SOL_AGENT), LeaderStatus::Readied);
        let content = ContentStore::embedded();
        let round_event = event(SOL_AGENT, &system, &planet, 4);
        assert!(has_round_copy(&state, content, SOL_AGENT));
        assert!(
            copy_plan(
                &state,
                content,
                POK,
                &round_event,
                &PlayerId::new("a"),
                &source,
                SOL_AGENT
            )
            .is_some()
        );
        let wrong_planet = event(SOL_AGENT, &system, &PlanetId::new("wrong-planet"), 4);
        assert!(
            copy_plan(
                &state,
                content,
                POK,
                &wrong_planet,
                &PlayerId::new("a"),
                &source,
                SOL_AGENT
            )
            .is_none()
        );
        let stale_round = event(SOL_AGENT, &system, &planet, 3);
        assert!(
            copy_plan(
                &state,
                content,
                POK,
                &stale_round,
                &PlayerId::new("a"),
                &source,
                SOL_AGENT
            )
            .is_none()
        );
    }

    #[test]
    fn sol_can_select_a_force_elsewhere_in_the_system_but_it_gets_no_die_in_another_battle() {
        let (mut state, system, battle_planet) = test_state();
        let source = PlayerId::new("b");
        state.player_mut(&source).unwrap().faction = ti4_model::id::FactionId::new("sol");
        state
            .player_mut(&source)
            .unwrap()
            .leaders
            .remove(&LeaderId::new(LETNEV_AGENT));
        state
            .player_mut(&source)
            .unwrap()
            .leaders
            .insert(LeaderId::new(SOL_AGENT), LeaderStatus::Exhausted);
        let other_planet = PlanetId::new("other-planet");
        crate::fixtures::put_on_planet(
            &mut state,
            &system,
            &other_planet,
            "infantry",
            &PlayerId::new("c"),
            1,
        );
        let ability_id = abilities(&state)
            .into_iter()
            .find(|ability| ability.id.contains(SOL_AGENT))
            .expect("Sol source has a fixed copy slot")
            .id;
        let target = target_id(&UnitTarget {
            owner: PlayerId::new("c"),
            unit_type: UnitTypeId::new("infantry"),
            planet: Some(other_planet.clone()),
            sustained_damage: false,
            galvanized: false,
            index: 0,
        });
        emit_copy(
            &mut state,
            SOL_AGENT,
            &system,
            &battle_planet,
            &[ability_id, target],
        )
        .expect("borrower can select a force elsewhere in the active system");

        let mark = state
            .faction_marks
            .values()
            .find_map(|value| serde_json::from_str::<RoundBonus>(value).ok())
            .expect("successful selection writes one serialized mark");
        assert_eq!(mark.combat_planet.as_ref(), Some(&battle_planet));
        assert_eq!(mark.unit_planet.as_ref(), Some(&other_planet));
        assert_eq!(
            read_bonus(
                &state,
                &system,
                Some(&other_planet),
                &PlayerId::new("c"),
                "infantry",
                0,
                4,
            ),
            0
        );
        assert_eq!(
            read_bonus(
                &state,
                &system,
                Some(&battle_planet),
                &PlayerId::new("c"),
                "infantry",
                0,
                4,
            ),
            0
        );
    }

    #[test]
    fn one_mark_boosts_only_one_exact_unit_in_its_combat_and_round() {
        let (mut state, system, planet) = test_state();
        let mark = RoundBonus {
            borrower: PlayerId::new("a"),
            source_owner: PlayerId::new("b"),
            source_agent: LETNEV_AGENT.to_owned(),
            system: system.clone(),
            combat_planet: None,
            unit_planet: None,
            unit_owner: PlayerId::new("c"),
            unit_type: UnitTypeId::new("cruiser"),
            sustained_damage: false,
            galvanized: false,
            unit_index: 1,
            round_seq: 4,
        };
        state.faction_marks.insert(
            format!("{MARK_PREFIX}a:{LETNEV_AGENT}:4"),
            serde_json::to_string(&mark).unwrap(),
        );
        assert_eq!(
            read_bonus(&state, &system, None, &PlayerId::new("c"), "cruiser", 0, 4),
            0
        );
        assert_eq!(
            read_bonus(&state, &system, None, &PlayerId::new("c"), "cruiser", 1, 4),
            1
        );
        assert_eq!(
            read_bonus(
                &state,
                &system,
                Some(&planet),
                &PlayerId::new("c"),
                "cruiser",
                1,
                4
            ),
            0
        );
        assert_eq!(
            read_bonus(&state, &system, None, &PlayerId::new("c"), "cruiser", 1, 3),
            0
        );
        assert_eq!(
            read_bonus(&state, &system, None, &PlayerId::new("a"), "cruiser", 1, 4),
            0
        );
        assert_eq!(
            read_bonus(&state, &system, None, &PlayerId::new("c"), "carrier", 1, 4),
            0
        );
        assert_eq!(
            read_bonus(
                &state,
                &SystemId::new("other"),
                None,
                &PlayerId::new("c"),
                "cruiser",
                1,
                4
            ),
            0
        );
    }

    #[test]
    fn selected_signature_does_not_shift_to_the_next_unit_after_removal() {
        let (mut state, system, _) = test_state();
        let mark = RoundBonus {
            borrower: PlayerId::new("a"),
            source_owner: PlayerId::new("b"),
            source_agent: LETNEV_AGENT.to_owned(),
            system: system.clone(),
            combat_planet: None,
            unit_planet: None,
            unit_owner: PlayerId::new("c"),
            unit_type: UnitTypeId::new("cruiser"),
            sustained_damage: false,
            galvanized: false,
            unit_index: 1,
            round_seq: 4,
        };
        state.faction_marks.insert(
            format!("{MARK_PREFIX}a:{LETNEV_AGENT}:4"),
            serde_json::to_string(&mark).unwrap(),
        );
        assert_eq!(
            read_bonus(&state, &system, None, &PlayerId::new("c"), "cruiser", 1, 4),
            1
        );
        state.board.get_mut(&system).unwrap().units.retain(|unit| {
            !(unit.owner == PlayerId::new("c") && unit.type_id.as_str() == "cruiser")
        });
        assert_eq!(
            read_bonus(&state, &system, None, &PlayerId::new("c"), "cruiser", 1, 4),
            0
        );
    }

    #[test]
    fn registered_letnev_copy_adds_a_die_to_the_selected_ship_in_a_real_combat_window() {
        let (mut state, system, _) = test_state();
        let content = ContentStore::embedded();
        let borrower = PlayerId::new("a");
        let source = PlayerId::new("b");
        let opponent = PlayerId::new("c");
        state.active = Some(source.clone());
        state.system_mut(&system).units.clear();
        crate::fixtures::put(&mut state, &system, "dreadnought", &source, 1);
        crate::fixtures::put(&mut state, &system, "cruiser", &opponent, 1);

        let copy_ability = abilities(&state)
            .into_iter()
            .find(|ability| ability.id.contains(LETNEV_AGENT))
            .expect("the seated Letnev agent has a fixed Ssruu slot")
            .id;
        let target = target_id(&UnitTarget {
            owner: source.clone(),
            unit_type: UnitTypeId::new("dreadnought"),
            planet: None,
            sustained_damage: false,
            galvanized: false,
            index: 0,
        });
        let mut table =
            crate::choice::Table::with_default(Box::new(crate::choice::Scripted::with_fallback(
                [copy_ability, target],
                Box::new(crate::choice::FirstOption),
            )));
        let mut resolver = crate::fixtures::armed_resolver(&state);
        let mut sequence = crate::event::EventSequence::new();
        let mut dice = crate::dice::Dice::from_faces([10; 8]);
        let mut rng = crate::rng::GameRng::new(0);
        let mut window = crate::combat::CombatWindow::new(&state, content, POK, &system);
        let mut context = crate::choice::Resolving {
            content,
            sources: POK,
            dice: &mut dice,
            rng: &mut rng,
            table: &mut table,
            timing: Some(crate::choice::TimingHandle {
                resolver: &mut resolver,
                sequence: &mut sequence,
                galaxy: None,
            }),
        };
        window
            .settle_open(&mut state, &mut context)
            .expect("the real round-start trigger and combat resolve");
        for _ in 0..8 {
            if window.outcome().is_some() {
                break;
            }
            crate::choice::Window::drive(&mut window, &mut state, &mut context)
                .expect("combat choices resolve through the real window");
            if window.outcome().is_none() {
                window
                    .settle_open(&mut state, &mut context)
                    .expect("the next combat stage settles");
            }
        }
        assert!(window.outcome().is_some(), "the combat window completes");

        let selected_owner_roll = dice
            .history()
            .iter()
            .find(|roll| roll.by.as_deref() == Some(source.as_str()) && roll.hits_on.is_some())
            .expect("the selected ship owner rolled combat dice");
        assert_eq!(
            selected_owner_roll.faces.len(),
            2,
            "one dreadnought die plus the copied agent's extra die"
        );
        assert_eq!(
            state.player(&source).unwrap().leaders[&LeaderId::new(LETNEV_AGENT)],
            LeaderStatus::Exhausted,
            "the source agent remains exhausted"
        );
        assert_eq!(
            state.player(&borrower).unwrap().leaders[&LeaderId::new("yssarilagent")],
            LeaderStatus::Exhausted,
            "Ssruu exhausts only after the valid target is selected"
        );
    }

    #[test]
    fn registered_sol_copy_adds_a_die_in_a_real_committed_planet_fight() {
        let (mut state, system, planet) = test_state();
        let content = ContentStore::embedded();
        let borrower = PlayerId::new("a");
        let source = PlayerId::new("b");
        state
            .board
            .get_mut(&system)
            .unwrap()
            .planet_units
            .get_mut(&planet)
            .unwrap()
            .clear();
        crate::fixtures::put_on_planet(&mut state, &system, &planet, "infantry", &borrower, 1);
        crate::fixtures::put_on_planet(&mut state, &system, &planet, "infantry", &source, 1);
        state.player_mut(&source).unwrap().faction = ti4_model::id::FactionId::new("sol");
        let prior_source_status = state
            .player_mut(&source)
            .unwrap()
            .leaders
            .remove(&LeaderId::new(LETNEV_AGENT));
        assert_eq!(prior_source_status, Some(LeaderStatus::Exhausted));
        state
            .player_mut(&source)
            .unwrap()
            .leaders
            .insert(LeaderId::new(SOL_AGENT), LeaderStatus::Exhausted);

        let copy_ability = abilities(&state)
            .into_iter()
            .find(|ability| ability.id.contains(SOL_AGENT))
            .expect("the seated Sol agent has a fixed Ssruu slot")
            .id;
        let target = target_id(&UnitTarget {
            owner: source.clone(),
            unit_type: UnitTypeId::new("infantry"),
            planet: Some(planet.clone()),
            sustained_damage: false,
            galvanized: false,
            index: 0,
        });
        let mut table =
            crate::choice::Table::with_default(Box::new(crate::choice::Scripted::with_fallback(
                [copy_ability, target],
                Box::new(crate::choice::FirstOption),
            )));
        let mut resolver = crate::fixtures::armed_resolver(&state);
        let mut sequence = crate::event::EventSequence::new();
        let mut dice = crate::dice::Dice::from_faces([10; 8]);
        let mut rng = crate::rng::GameRng::new(0);
        let mut context = crate::choice::Resolving {
            content,
            sources: POK,
            dice: &mut dice,
            rng: &mut rng,
            table: &mut table,
            timing: Some(crate::choice::TimingHandle {
                resolver: &mut resolver,
                sequence: &mut sequence,
                galaxy: None,
            }),
        };
        crate::invasion::InvasionWindow::fight_committed_planet(
            &mut state,
            &mut context,
            &borrower,
            &system,
            &planet,
        )
        .expect("the real ground-round trigger and combat resolve");

        let selected_owner_roll = dice
            .history()
            .iter()
            .find(|roll| roll.by.as_deref() == Some(source.as_str()) && roll.hits_on.is_some())
            .expect("the selected unit owner rolled ground-combat dice");
        assert_eq!(
            selected_owner_roll.faces.len(),
            2,
            "one infantry die plus the copied agent's extra die"
        );
        assert_eq!(
            state.player(&source).unwrap().leaders[&LeaderId::new(SOL_AGENT)],
            LeaderStatus::Exhausted,
            "the source agent remains exhausted"
        );
        assert_eq!(
            state.player(&borrower).unwrap().leaders[&LeaderId::new("yssarilagent")],
            LeaderStatus::Exhausted,
            "Ssruu exhausts only after the valid target is selected"
        );
    }
}
