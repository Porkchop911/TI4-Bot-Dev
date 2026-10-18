//! The Naaz-Rokha Alliance (`naaz`). See `factions/mod.rs` for the contract and
//! `plans/BASE_FACTIONS_PLAN_2026-10-02.md` for scope; the per-item record is
//! `plans/evidence/BF-naaz.md`.
//!
//! Card texts (latest printing, `crates/ti4-content/content/*.json`):
//!
//! * Fabrication: "ACTION: Either purge 2 of your relic fragments of the same type to gain 1
//!   relic; or purge 1 of your relic fragments to gain 1 command token."
//! * Black Market Forgery (`bmf`): "ACTION: Purge 2 of your relic fragments of the same type to
//!   gain 1 relic. Then, return this card to the Naaz-Rokha player."
//! * Supercharge (`sc`): "At the start of a combat round, you may exhaust this card to apply +1 to
//!   the result of each of your unit's combat rolls during this combat round."
//! * Visz El Vir (flagship): "Your mechs in this system roll 1 additional die during combat."
//! * Garv and Gunn (`naazagent`): "At the end of a player's turn: You may exhaust this card to
//!   allow that player to explore 1 of their planets."
//! * Dart and Tai (`naazcommander`): "After you gain control of a planet that was controlled by
//!   another player: You may explore that planet." Unlock: "Have mechs in 3 systems."
//! * Hesh and Prit (`naazhero`): "ACTION: Gain 1 relic and perform the secondary ability of up to
//!   2 readied or unchosen strategy cards. During this action, spend command tokens from your
//!   reinforcements instead of your strategy pool. Then, purge this card."
//!
//! Absolute Synergy and the Eidolon Maximum are live but unclaimed (shared seams missing): see the
//! evidence file.

use std::sync::Arc;

use ti4_content::ContentStore;
use ti4_content::units::catalogue;
use ti4_model::content_types::SourceSet;
use ti4_model::id::{LeaderId, PlanetId, PlayerId, StrategyCardId, SystemId};
use ti4_model::state::{GameState, LeaderStatus, TokenPool};

use super::hooks_combat::CombatHooks;
use super::hooks_economy::EconomyHooks;
use super::hooks_ground::GroundHooks;
use super::{CombatUnit, FactionModule, Hooks};
use crate::choice::{Choice, ChoiceOption};
use crate::decision_context::{DecisionContext, DecisionSource};
use crate::timing::{Ability, Relation, TimingContext};

const FACTION: &str = "naaz";
const AGENT: &str = "naazagent";
const COMMANDER: &str = "naazcommander";
const HERO: &str = "naazhero";
const BMF: &str = "bmf";
const FAB_RELIC: &str = "faction|naaz|fabrication_relic";
const FAB_TOKEN: &str = "faction|naaz|fabrication_token";
const BMF_ACTION: &str = "faction|naaz|bmf";
/// The three decks a fragment type can name; frontier fragments stand in for any of them.
const TYPES: [&str; 3] = ["CULTURAL", "HAZARDOUS", "INDUSTRIAL"];

/// What this faction implements, `naaz_voltron` (Eidolon Maximum) and `naazbt` (Absolute Synergy)
/// included: the real-route tests are in this file and the closure is in the evidence file.
pub const MODULE: FactionModule = FactionModule {
    alias: FACTION,
    abilities: &["fabrication", "distant_suns"],
    technologies: &["pfa", "sc"],
    units: &[
        "naaz_flagship",
        "naaz_mech",
        "naaz_mech_space",
        "naaz_voltron",
    ],
    promissory: &[BMF],
    leaders: &[AGENT, COMMANDER, HERO],
    breakthroughs: &["naazbt"],
    hooks: Hooks {
        component_actions: Some(component_actions),
        perform_component: Some(perform_component),
        commander_unlocked: Some(commander_unlocked),
        leader_action: Some(leader_action),
        use_leader: Some(use_leader),
        timing_abilities: Some(timing_abilities),
        unit_roll_modifier: Some(unit_roll_modifier),
        unit_dice: Some(unit_dice),
        space_combat_round_started: Some(space_combat_round_started),
        ground: GroundHooks {
            ground_combat_round_started: Some(ground_combat_round_started),
            ..GroundHooks::NONE
        },
        combat: CombatHooks {
            ability_hit_immune: Some(ability_hit_immune),
            ..CombatHooks::NONE
        },
        economy: EconomyHooks {
            explore_extra_draw: Some(explore_extra_draw),
            explored: Some(explored),
            cannot_produce: Some(cannot_produce),
            effect_placement_forbidden: Some(effect_placement_forbidden),
            captured_unit_return_form: Some(captured_unit_return_form),
            ..EconomyHooks::NONE
        },
        ..Hooks::NONE
    },
};

// -- small readers -------------------------------------------------------------------------------

fn is_naaz(state: &GameState, player: &PlayerId) -> bool {
    state
        .player(player)
        .is_some_and(|seat| seat.faction.as_str() == FACTION)
}

fn leader_status(state: &GameState, player: &PlayerId, leader: &str) -> Option<LeaderStatus> {
    crate::leaders::status(state, player, &LeaderId::new(leader))
}

fn decision(state: &GameState, player: &PlayerId, card: &str, subtype: &str) -> DecisionContext {
    DecisionContext::new(
        player.clone(),
        DecisionSource::FactionAbility(card.to_owned()),
        subtype,
        state.phase,
        state.round,
    )
}

fn fragments(state: &GameState, player: &PlayerId, kind: &str) -> i32 {
    state
        .player(player)
        .and_then(|seat| seat.relic_fragments.get(kind).copied())
        .unwrap_or(0)
}

/// Types of which the player can purge `count` fragments (frontier fragments count as any type).
fn purgeable_types(state: &GameState, player: &PlayerId, count: i32) -> Vec<&'static str> {
    let frontier = fragments(state, player, crate::exploration::FRONTIER);
    TYPES
        .iter()
        .copied()
        .filter(|kind| fragments(state, player, kind) + frontier >= count)
        .collect()
}

/// Purge `count` fragments for a type, matching ones first. `false` and untouched if short.
fn purge_fragments(state: &mut GameState, player: &PlayerId, kind: &str, count: i32) -> bool {
    let matching = fragments(state, player, kind);
    let frontier = if kind == crate::exploration::FRONTIER {
        0
    } else {
        fragments(state, player, crate::exploration::FRONTIER)
    };
    if matching + frontier < count {
        return false;
    }
    let from_matching = matching.min(count);
    let Some(seat) = state.player_mut(player) else {
        return false;
    };
    if from_matching > 0 {
        *seat.relic_fragments.entry(kind.to_owned()).or_insert(0) -= from_matching;
    }
    if count > from_matching {
        *seat
            .relic_fragments
            .entry(crate::exploration::FRONTIER.to_owned())
            .or_insert(0) -= count - from_matching;
    }
    true
}

fn ask(
    context: &mut TimingContext<'_>,
    player: &PlayerId,
    card: &str,
    subtype: &str,
    prompt: &str,
    options: Vec<ChoiceOption>,
) -> Option<String> {
    let choice = Choice::new(player.clone(), prompt.to_owned(), options).contextualized(decision(
        context.state,
        player,
        card,
        subtype,
    ));
    context.ask_seeing(&choice).ok().map(|answer| answer.id)
}

/// Fallible delivery for a timing effect: an invalid nested answer must leave the window retryable.
fn ask_checked(
    context: &mut TimingContext<'_>,
    player: &PlayerId,
    card: &str,
    subtype: &str,
    prompt: &str,
    options: Vec<ChoiceOption>,
) -> Result<String, crate::timing::TimingError> {
    let choice = Choice::new(player.clone(), prompt.to_owned(), options).contextualized(decision(
        context.state,
        player,
        card,
        subtype,
    ));
    context
        .ask_seeing(&choice)
        .map(|answer| answer.id)
        .map_err(crate::timing::TimingError::IllegalChoice)
}

fn type_options(kinds: &[&str], verb: &str) -> Vec<ChoiceOption> {
    kinds
        .iter()
        .map(|kind| {
            ChoiceOption::labelled((*kind).to_owned(), "fragment", format!("{verb} {kind}"))
        })
        .collect()
}

/// Explore `planet` for `actor` through the shared path.
fn explore_planet(context: &mut TimingContext<'_>, actor: &PlayerId, planet: &PlanetId) {
    let mut resolving = crate::choice::Resolving {
        content: context.content,
        sources: context.sources,
        dice: context.dice,
        rng: context.rng,
        table: context.table,
        timing: None,
    };
    if let Some(deck) =
        crate::exploration::choose_deck(&mut resolving, context.state, actor, planet)
    {
        let _ = crate::exploration::explore_with(
            context.state,
            &mut resolving,
            actor,
            &deck,
            Some(planet),
        );
    }
}

fn explorable(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    planet: &PlanetId,
) -> bool {
    let traits = crate::planets::traits_now(state, content, sources, planet);
    !traits.is_empty()
        && traits.iter().any(|deck| {
            state
                .exploration_decks
                .get(deck)
                .is_some_and(|cards| !cards.is_empty())
        })
}

// -- Fabrication and Black Market Forgery --------------------------------------------------------

/// The held Black Market Forgery of a player who is not the Naaz-Rokha player.
fn held_bmf(state: &GameState, player: &PlayerId) -> Option<String> {
    let own = crate::promissory::faction_name(state, player);
    state
        .promissory_notes
        .iter()
        .find(|(note, holder)| {
            *holder == player
                && crate::promissory::alias_of(note) == BMF
                && crate::promissory::owner_of(note).is_some_and(|owner| owner != own)
        })
        .map(|(note, _)| note.clone())
}

fn component_actions(
    state: &GameState,
    _content: &ContentStore,
    player: &PlayerId,
) -> Vec<ChoiceOption> {
    let mut options = Vec::new();
    let relic_left = !state.relic_deck.is_empty();
    if is_naaz(state, player) {
        if relic_left && !purgeable_types(state, player, 2).is_empty() {
            options.push(ChoiceOption::labelled(
                FAB_RELIC,
                crate::faction_abilities::ACTION_KIND,
                "Fabrication: purge 2 relic fragments of the same type to gain 1 relic",
            ));
        }
        let any_fragment = state
            .player(player)
            .is_some_and(|seat| seat.relic_fragments.values().any(|held| *held > 0));
        if any_fragment && state.tokens_in_reinforcements(player) > 0 {
            options.push(ChoiceOption::labelled(
                FAB_TOKEN,
                crate::faction_abilities::ACTION_KIND,
                "Fabrication: purge 1 relic fragment to gain 1 command token",
            ));
        }
    }
    if relic_left
        && held_bmf(state, player).is_some()
        && !purgeable_types(state, player, 2).is_empty()
    {
        options.push(ChoiceOption::labelled(
            BMF_ACTION,
            crate::faction_abilities::ACTION_KIND,
            "Black Market Forgery: purge 2 relic fragments of the same type to gain 1 relic",
        ));
    }
    options
}

/// Choose a fragment type (asking only when there is a choice), purge `count`, gain a relic.
fn purge_for_relic(context: &mut TimingContext<'_>, player: &PlayerId, card: &str) -> bool {
    let mut kinds = purgeable_types(context.state, player, 2);
    // Only frontier fragments: every type is the same purge, so offer one.
    if kinds
        .iter()
        .all(|kind| fragments(context.state, player, kind) == 0)
    {
        kinds.truncate(1);
    }
    let kind = match kinds.as_slice() {
        [] => return false,
        [only] => (*only).to_owned(),
        _ => {
            let Some(id) = ask(
                context,
                player,
                card,
                "fragment_type",
                "purge 2 fragments of which type",
                type_options(&kinds, "purge 2"),
            ) else {
                return false;
            };
            if !kinds.contains(&id.as_str()) {
                return false;
            }
            id
        }
    };
    if context.state.relic_deck.is_empty() || !purge_fragments(context.state, player, &kind, 2) {
        return false;
    }
    crate::relics::gain(context.state, player).is_some()
}

fn perform_component(
    context: &mut TimingContext<'_>,
    player: &PlayerId,
    option: &ChoiceOption,
) -> bool {
    match option.id.as_str() {
        FAB_RELIC if is_naaz(context.state, player) => {
            purge_for_relic(context, player, "fabrication")
        }
        FAB_TOKEN if is_naaz(context.state, player) => fabricate_token(context, player),
        BMF_ACTION => {
            let Some(note) = held_bmf(context.state, player) else {
                return false;
            };
            if !purge_for_relic(context, player, BMF) {
                return false;
            }
            crate::promissory::give_back(context.state, &note);
            true
        }
        _ => false,
    }
}

fn fabricate_token(context: &mut TimingContext<'_>, player: &PlayerId) -> bool {
    if context.state.tokens_in_reinforcements(player) <= 0 {
        return false;
    }
    let held: Vec<&str> = TYPES
        .iter()
        .copied()
        .chain(std::iter::once(crate::exploration::FRONTIER))
        .filter(|kind| fragments(context.state, player, kind) > 0)
        .collect();
    let kind = match held.as_slice() {
        [] => return false,
        [only] => (*only).to_owned(),
        _ => {
            let Some(id) = ask(
                context,
                player,
                "fabrication",
                "fragment_type",
                "purge 1 fragment of which type",
                type_options(&held, "purge 1"),
            ) else {
                return false;
            };
            if !held.contains(&id.as_str()) {
                return false;
            }
            id
        }
    };
    let pools = [
        ("tactic", TokenPool::Tactic),
        ("fleet", TokenPool::Fleet),
        ("strategic", TokenPool::Strategic),
    ];
    let options = pools
        .iter()
        .map(|(id, _)| {
            ChoiceOption::labelled((*id).to_owned(), "pool", format!("gain 1 token in {id}"))
        })
        .collect();
    let Some(id) = ask(
        context,
        player,
        "fabrication",
        "token_pool",
        "Fabrication: which pool gains the token",
        options,
    ) else {
        return false;
    };
    let Some((_, pool)) = pools.iter().find(|(name, _)| *name == id) else {
        return false;
    };
    if !purge_fragments(context.state, player, &kind, 1) {
        return false;
    }
    context.state.gain_token(player, *pool, 1) > 0
}

// -- Distant Suns and Pre-Fab Arcologies ----------------------------------------------------------

/// Distant Suns: "When you explore a planet that contains 1 of your mechs: You may draw 1
/// additional card; choose 1 to resolve and discard the rest." The shared route draws and asks.
fn explore_extra_draw(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    planet: &PlanetId,
) -> bool {
    if !is_naaz(state, player) {
        return false;
    }
    let types = catalogue(content, sources);
    state.board.values().any(|here| {
        here.planet_units.get(planet).is_some_and(|units| {
            units.iter().any(|unit| {
                &unit.owner == player
                    && types
                        .get(unit.type_id.as_str())
                        .is_some_and(|kind| kind.base_type() == "mech")
            })
        })
    })
}

/// Pre-Fab Arcologies: "After you explore a planet, ready that planet."
fn explored(
    state: &mut GameState,
    _content: &ContentStore,
    _sources: SourceSet,
    player: &PlayerId,
    planet: &PlanetId,
) {
    if crate::technology::has_technology_text(state, player, "pfa") {
        state.exhausted_planets.remove(planet);
    }
}

// -- Supercharge ---------------------------------------------------------------------------------

fn supercharge_key(player: &PlayerId) -> String {
    format!("naaz:supercharge:{player}")
}

fn technology_ready(state: &GameState, player: &PlayerId, alias: &str) -> bool {
    crate::technology::technology_text_ready(state, player, alias)
}

fn space_combat_round_started(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    table: &mut crate::choice::Table,
    player: &PlayerId,
) {
    if let Some(system) = state.active_system.clone() {
        repair_voltron(state, content, sources, player, &system, None);
    }
    offer_supercharge(state, content, sources, table, player, "space");
}

fn ground_combat_round_started(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    table: &mut crate::choice::Table,
    player: &PlayerId,
    system: &SystemId,
    planet: &PlanetId,
) {
    repair_voltron(state, content, sources, player, system, Some(planet));
    offer_supercharge(state, content, sources, table, player, "ground");
}

/// "At the start of a combat round": the mark names the kind of combat and the round sequence, so
/// it lapses by itself when the round moves on.
fn offer_supercharge(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    table: &mut crate::choice::Table,
    player: &PlayerId,
    context: &str,
) {
    state.faction_marks.remove(&supercharge_key(player));
    if !technology_ready(state, player, "sc") {
        return;
    }
    let choice = Choice::new(
        player.clone(),
        "Supercharge: exhaust to apply +1 to each combat roll this round".to_owned(),
        vec![
            ChoiceOption::labelled("sc", "technology", "exhaust Supercharge"),
            ChoiceOption::decline(),
        ],
    )
    .contextualized(decision(state, player, "sc", "supercharge"));
    let Ok(answer) = table.ask_seeing(
        &choice,
        &crate::choice::Observed::new(state, content, sources, None),
    ) else {
        return;
    };
    if answer.is_decline() {
        return;
    }
    let seq = state.combat_round_seq;
    state
        .faction_marks
        .insert(supercharge_key(player), format!("{context}:{seq}"));
    crate::technology::exhaust_technology_text(state, player, "sc");
}

fn unit_roll_modifier(
    state: &GameState,
    _content: &ContentStore,
    _sources: SourceSet,
    unit: &CombatUnit<'_>,
) -> i64 {
    i64::from(
        state.faction_marks.get(&supercharge_key(unit.player))
            == Some(&format!("{}:{}", unit.context, state.combat_round_seq)),
    )
}

// -- Visz El Vir ---------------------------------------------------------------------------------

fn unit_dice(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    unit: &CombatUnit<'_>,
    dice: i64,
) -> i64 {
    let Some(system) = unit.system else {
        return dice;
    };
    let is_mech = catalogue(content, sources)
        .get(unit.unit_type)
        .is_some_and(|kind| kind.base_type() == "mech")
        && (unit.unit_type.starts_with("naaz_") || super::nekro::is_nekro(state, unit.player));
    let flagship = super::has_flagship_text_in(state, unit.player, system, "naaz_flagship");
    if is_mech && flagship { dice + 1 } else { dice }
}

// -- Garv and Gunn -------------------------------------------------------------------------------

fn explorable_planets(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    actor: &PlayerId,
) -> Vec<PlanetId> {
    state
        .controlled_planets(actor)
        .into_iter()
        .map(|(_, planet)| planet.clone())
        .filter(|planet| explorable(state, content, sources, planet))
        .collect()
}

fn agent(owner_name: &str, seat: &PlayerId) -> Ability {
    let (owner, condition_owner) = (seat.clone(), seat.clone());
    Ability::stateful(
        format!("leader:{owner_name}:naazagent:TURN_PASSED:after"),
        seat.clone(),
        "TURN_PASSED",
        Relation::After,
        Arc::new(move |event, _resolver, context| {
            let Some(actor) = event.text("player").map(PlayerId::new) else {
                return Ok(());
            };
            if leader_status(context.state, &owner, AGENT) != Some(LeaderStatus::Readied) {
                return Ok(());
            }
            let planets =
                explorable_planets(context.state, context.content, context.sources, &actor);
            if planets.is_empty() {
                return Ok(());
            }
            let options = planets
                .iter()
                .map(|planet| {
                    ChoiceOption::labelled(
                        planet.to_string(),
                        "planet",
                        format!("explore {planet}"),
                    )
                })
                .collect();
            let Some(id) = ask(
                context,
                &actor,
                "naazagent",
                "agent_planet",
                "Garv and Gunn: explore which of your planets",
                options,
            ) else {
                return Ok(());
            };
            let Some(planet) = planets.into_iter().find(|p| p.as_str() == id) else {
                return Ok(());
            };
            if !crate::leaders::exhaust(context.state, &owner, &LeaderId::new(AGENT)) {
                return Ok(());
            }
            explore_planet(context, &actor, &planet);
            Ok(())
        }),
    )
    .with_optional(true)
    .with_stateful_condition(Arc::new(move |event, _, context| {
        leader_status(context.state, &condition_owner, AGENT) == Some(LeaderStatus::Readied)
            && event
                .text("player")
                .map(PlayerId::new)
                .is_some_and(|actor| {
                    !explorable_planets(context.state, context.content, context.sources, &actor)
                        .is_empty()
                })
    }))
}

/// Ssruu copies Garv and Gunn's optional end-of-turn text as the borrower. The ending player,
/// not the borrower, chooses which of their planets to explore after the borrower accepts.
fn borrowed_agent(owner_name: &str, source: &PlayerId, borrower: &PlayerId) -> Ability {
    let (condition_source, condition_borrower) = (source.clone(), borrower.clone());
    let (effect_source, effect_borrower) = (source.clone(), borrower.clone());
    Ability::stateful(
        format!("leader:{owner_name}:{source}:yssarilagent:{AGENT}:TURN_PASSED:after"),
        borrower.clone(),
        "TURN_PASSED",
        Relation::After,
        Arc::new(move |event, _, context| {
            let Some(actor) = event.text("player").map(PlayerId::new) else {
                return Ok(());
            };
            if !has_borrowable_naaz_agent(
                context.state,
                context.content,
                &effect_source,
                &effect_borrower,
            ) {
                return Ok(());
            }
            let planets =
                explorable_planets(context.state, context.content, context.sources, &actor);
            if planets.is_empty() {
                return Ok(());
            }
            let options = planets
                .iter()
                .map(|planet| {
                    ChoiceOption::labelled(
                        planet.to_string(),
                        "planet",
                        format!(
                            "explore {}",
                            ti4_content::galaxy::planet(
                                context.content,
                                planet.as_str(),
                                context.sources
                            )
                            .and_then(|record| record.name())
                            .unwrap_or(planet.as_str())
                        ),
                    )
                })
                .collect();
            let choice = Choice::new(
                actor.clone(),
                "Garv and Gunn: explore which of your planets".to_owned(),
                options,
            )
            .contextualized(decision(
                context.state,
                &actor,
                AGENT,
                "borrowed_agent_planet",
            ));
            let id = context
                .ask_seeing(&choice)
                .map_err(crate::timing::TimingError::IllegalChoice)?
                .id;
            let Some(planet) = planets.into_iter().find(|planet| planet.as_str() == id) else {
                return Ok(());
            };
            if !crate::leaders::exhaust(
                context.state,
                &effect_borrower,
                &LeaderId::new("yssarilagent"),
            ) {
                return Ok(());
            }
            let choice_errors = context.table.choice_error_checkpoint();
            explore_planet(context, &actor, &planet);
            if let Some(error) = context.table.choice_error_since(choice_errors) {
                return Err(crate::timing::TimingError::IllegalChoice(error));
            }
            super::hooks_cards::borrowed_agent_used(
                context,
                &effect_borrower,
                &LeaderId::new(AGENT),
            );
            Ok(())
        }),
    )
    .with_optional(true)
    .with_stateful_condition(Arc::new(move |event, _, context| {
        event.text("player").is_some_and(|actor| {
            !explorable_planets(
                context.state,
                context.content,
                context.sources,
                &PlayerId::new(actor),
            )
            .is_empty()
                && has_borrowable_naaz_agent(
                    context.state,
                    context.content,
                    &condition_source,
                    &condition_borrower,
                )
        })
    }))
}

fn has_borrowable_naaz_agent(
    state: &GameState,
    content: &ContentStore,
    source: &PlayerId,
    borrower: &PlayerId,
) -> bool {
    super::hooks_cards::borrowable_agents(state, content, borrower)
        .iter()
        .any(|(owner, agent)| owner == source && agent.as_str() == AGENT)
}

// -- Dart and Tai --------------------------------------------------------------------------------

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
    let types = catalogue(content, sources);
    let is_mech = |unit: &ti4_model::units::Unit| {
        &unit.owner == player
            && types
                .get(unit.type_id.as_str())
                .is_some_and(|kind| kind.base_type() == "mech")
    };
    let systems = state
        .board
        .values()
        .filter(|here| {
            here.units.iter().any(is_mech)
                || here
                    .planet_units
                    .values()
                    .any(|units| units.iter().any(is_mech))
        })
        .count();
    Some(systems >= 3)
}

fn commander(owner_name: &str, seat: &PlayerId) -> Ability {
    let condition_owner = seat.clone();
    Ability::stateful(
        format!("leader:{owner_name}:naazcommander:PLANET_CONTROL_GAINED:after"),
        seat.clone(),
        "PLANET_CONTROL_GAINED",
        Relation::After,
        Arc::new(move |event, _resolver, context| {
            if let (Some(player), Some(planet)) = (
                event.text("player").map(PlayerId::new),
                event.text("planet").map(PlanetId::new),
            ) {
                explore_planet(context, &player, &planet);
            }
            Ok(())
        }),
    )
    .with_optional(true)
    .with_stateful_condition(Arc::new(move |event, _, context| {
        event.text("player") == Some(condition_owner.as_str())
            && event
                .text("previous_owner")
                .is_some_and(|prev| !prev.is_empty())
            && crate::promissory::has_commander_ability(context.state, &condition_owner, COMMANDER)
            && event
                .text("planet")
                .map(PlanetId::new)
                .is_some_and(|planet| {
                    explorable(context.state, context.content, context.sources, &planet)
                        && context
                            .state
                            .controlled_planets(&condition_owner)
                            .iter()
                            .any(|(_, controlled)| *controlled == &planet)
                })
    }))
}

// -- Hesh and Prit -------------------------------------------------------------------------------

/// Strategy cards that are readied (held and not exhausted) or unchosen.
/// Hesh and Prit resolves secondaries, including Warfare's home production.
fn hero_cards(state: &GameState) -> Vec<StrategyCardId> {
    let mut cards: Vec<StrategyCardId> = state
        .players
        .iter()
        .flat_map(|seat| {
            seat.strategy_cards
                .iter()
                .filter(|card| !seat.exhausted_strategy_cards.contains(*card))
        })
        .cloned()
        .chain(state.unclaimed_strategy_cards.iter().cloned())
        .collect();
    cards.sort();
    cards.dedup();
    cards
}

/// Every secondary but Leadership's (influence) costs a command token.
fn costs_token(content: &ContentStore, card: &StrategyCardId) -> bool {
    crate::strategy_cards::card_name(content, card.as_str()).as_deref() != Some("Leadership")
}

fn leader_action(
    state: &GameState,
    _content: &ContentStore,
    _player: &PlayerId,
    leader: &LeaderId,
) -> Option<bool> {
    (leader.as_str() == HERO).then_some(!state.relic_deck.is_empty())
}

fn use_leader(
    context: &mut TimingContext<'_>,
    player: &PlayerId,
    leader: &LeaderId,
) -> Option<bool> {
    if leader.as_str() != HERO {
        return None;
    }
    if context.state.relic_deck.is_empty() {
        return Some(false);
    }
    // Every choice before the first mutation: up to 2 cards, each at most once.
    let mut chosen: Vec<StrategyCardId> = Vec::new();
    while chosen.len() < 2 {
        let cards: Vec<StrategyCardId> = hero_cards(context.state)
            .into_iter()
            .filter(|card| !chosen.contains(card))
            .collect();
        let spare = context.state.tokens_in_reinforcements(player) > 0;
        let cards: Vec<StrategyCardId> = cards
            .into_iter()
            .filter(|card| spare || !costs_token(context.content, card))
            .collect();
        if cards.is_empty() {
            break;
        }
        let mut options: Vec<ChoiceOption> = cards
            .iter()
            .map(|card| {
                ChoiceOption::labelled(
                    card.to_string(),
                    "strategy_card",
                    crate::draft::strategy_card_label(context.content, card.as_str()),
                )
            })
            .collect();
        options.push(ChoiceOption::decline());
        let Some(id) = ask(
            context,
            player,
            HERO,
            "hero_card",
            "Hesh and Prit: perform the secondary of which card",
            options,
        ) else {
            break;
        };
        match cards.into_iter().find(|card| card.as_str() == id) {
            Some(card) => chosen.push(card),
            None => break,
        }
    }
    let snapshot = context.state.clone();
    if crate::relics::gain(context.state, player).is_none() {
        return Some(false);
    }
    for card in &chosen {
        // A token spent from reinforcements returns to reinforcements: nothing leaves the pools,
        // but one has to be there to spend.
        if costs_token(context.content, card) && context.state.tokens_in_reinforcements(player) <= 0
        {
            break;
        }
        let outcome = crate::strategy_cards::secondary(
            context.state,
            context.content,
            context.sources,
            context.galaxy,
            context.table,
            player,
            card.as_str(),
        );
        if outcome.is_err() {
            *context.state = snapshot;
            return Some(false);
        }
    }
    Some(true)
}

// -- Absolute Synergy and the Eidolon Maximum -------------------------------------------------------
//
// `naazbt`: "When you have 4 mechs in the same system, you may return 3 of those mechs to your
// reinforcements to flip this card and place it on top of your mech card." Eidolon Maximum
// (`naaz_voltron`): "This unit is both a ship and ground force. It cannot be assigned hits from unit
// abilities. Repair it at the start of every combat round. Game effects cannot place or produce
// your mechs. When this unit is destroyed or removed, flip this card and return it to your play
// area."
//
// Claimed: hits from unit abilities, space combat from a planet, effect placement of mechs,
// production, movement, landing and re-offer after destruction each have a real-route test below.
// The card is flipped exactly
// while a `naaz_voltron` stands on the board, so "destroyed or removed, flip back" needs no
// bookkeeping that could go stale. Mech plastic is 4 (`supply::plastic`), so 4 mechs in a system
// are all of them.

const VOLTRON: &str = "naaz_voltron";
const BREAKTHROUGH: &str = "naazbt";

/// Where a mech stands inside its system: the space area (`None`) or one planet.
type Spot = Option<PlanetId>;

fn is_voltron(unit: &ti4_model::units::Unit, player: &PlayerId) -> bool {
    &unit.owner == player && unit.type_id.as_str() == VOLTRON
}

/// Whether the player's Absolute Synergy is flipped: an Eidolon Maximum is on the board.
fn voltron_on_board(state: &GameState, player: &PlayerId) -> bool {
    state.board.values().any(|here| {
        here.units.iter().any(|unit| is_voltron(unit, player))
            || here
                .planet_units
                .values()
                .any(|units| units.iter().any(|unit| is_voltron(unit, player)))
    })
}

/// A Naaz mech in either form (not the Maximum) of `player`.
fn is_eidolon(
    types: &std::collections::BTreeMap<&str, ti4_content::units::UnitType<'_>>,
    unit: &ti4_model::units::Unit,
    player: &PlayerId,
) -> bool {
    &unit.owner == player
        && unit.type_id.as_str().starts_with("naaz_")
        && unit.type_id.as_str() != VOLTRON
        && types
            .get(unit.type_id.as_str())
            .is_some_and(|kind| kind.base_type() == "mech")
}

/// Every mech of `player` in `system` as `(spot, index in that spot's list)`, board order.
fn eidolons_in(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    system: &SystemId,
) -> Vec<(Spot, usize)> {
    let types = catalogue(content, sources);
    let Some(here) = state.board.get(system) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for (index, unit) in here.units.iter().enumerate() {
        if is_eidolon(&types, unit, player) {
            found.push((None, index));
        }
    }
    for (planet, units) in &here.planet_units {
        for (index, unit) in units.iter().enumerate() {
            if is_eidolon(&types, unit, player) {
                found.push((Some(planet.clone()), index));
            }
        }
    }
    found
}

/// The systems that hold 4 of the player's mechs, with the distinct spots those mechs stand on.
fn synergy_sites(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
) -> Vec<(SystemId, Vec<Spot>)> {
    if !is_naaz(state, player)
        || !crate::breakthroughs::holds(state, player, BREAKTHROUGH)
        || voltron_on_board(state, player)
    {
        return Vec::new();
    }
    let mut found = Vec::new();
    for system in state.board.keys() {
        let mechs = eidolons_in(state, content, sources, player, system);
        if mechs.len() >= 4 {
            let mut spots: Vec<Spot> = Vec::new();
            for (spot, _) in mechs {
                if !spots.contains(&spot) {
                    spots.push(spot);
                }
            }
            found.push((system.clone(), spots));
        }
    }
    found
}

fn pool_mut<'a>(
    state: &'a mut GameState,
    system: &SystemId,
    spot: &Spot,
) -> Option<&'a mut Vec<ti4_model::units::Unit>> {
    let here = state.board.get_mut(system)?;
    match spot {
        None => Some(&mut here.units),
        Some(planet) => here.planet_units.get_mut(planet),
    }
}

/// Physical state that distinguishes which Eidolon the player is keeping.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct SurvivorState {
    form: ti4_model::id::UnitTypeId,
    damaged: bool,
    galvanized: bool,
}

type MechPosition = (Spot, usize);

/// Distinct physical choices at a spot. Identical mechs share an option because the board stores
/// units as interchangeable values with no per-piece identity.
fn survivor_choices(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    system: &SystemId,
    spot: &Spot,
) -> Vec<(SurvivorState, MechPosition)> {
    let Some(here) = state.board.get(system) else {
        return Vec::new();
    };
    let mut choices = std::collections::BTreeMap::new();
    for position in eidolons_in(state, content, sources, player, system) {
        if &position.0 != spot {
            continue;
        }
        let unit = match &position.0 {
            None => here.units.get(position.1),
            Some(planet) => here
                .planet_units
                .get(planet)
                .and_then(|units| units.get(position.1)),
        };
        if let Some(unit) = unit {
            choices
                .entry(SurvivorState {
                    form: unit.type_id.clone(),
                    damaged: unit.sustained_damage,
                    galvanized: unit.galvanized,
                })
                .or_insert(position);
        }
    }
    choices.into_iter().collect()
}

fn survivor_option_id(choice: &SurvivorState) -> String {
    format!(
        "survivor|{}|{}|{}",
        choice.form,
        if choice.damaged {
            "damaged"
        } else {
            "undamaged"
        },
        if choice.galvanized {
            "galvanized"
        } else {
            "plain"
        }
    )
}

fn survivor_label(choice: &SurvivorState) -> String {
    let form = if choice.form.as_str() == "naaz_mech_space" {
        "space-form"
    } else {
        "ground-form"
    };
    format!(
        "keep the {}{} {} Eidolon",
        if choice.damaged {
            "damaged"
        } else {
            "undamaged"
        },
        if choice.galvanized { " galvanized" } else { "" },
        form
    )
}

/// Return 3 mechs and make the exact selected unit the Eidolon Maximum. `false` and untouched
/// unless the system holds 4 mechs and the selected position names one of them.
fn flip_synergy(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    system: &SystemId,
    keep: &MechPosition,
) -> bool {
    let mechs = eidolons_in(state, content, sources, player, system);
    if mechs.len() < 4 || !mechs.contains(keep) {
        return false;
    }
    let mut doomed: Vec<(Spot, usize)> = mechs
        .into_iter()
        .filter(|entry| entry != keep)
        .take(3)
        .collect();
    // Remove from the back of each list so earlier indices stay valid.
    doomed.sort_by(|a, b| b.1.cmp(&a.1));
    for (at, index) in &doomed {
        if let Some(pool) = pool_mut(state, system, at) {
            pool.remove(*index);
        }
    }
    // The survivor's index shifts down by the number removed before it in the same list.
    let shift = doomed
        .iter()
        .filter(|(at, index)| *at == keep.0 && *index < keep.1)
        .count();
    if let Some(unit) =
        pool_mut(state, system, &keep.0).and_then(|pool| pool.get_mut(keep.1 - shift))
    {
        unit.type_id = ti4_model::id::UnitTypeId::new(VOLTRON);
    }
    true
}

/// Absolute Synergy as an optional "when you have 4 mechs in the same system" window. The state
/// has no event of its own, so it is offered when the breakthrough is gained and after each action
/// (where mechs arrive: production, movement, landing).
fn absolute_synergy(owner_name: &str, seat: &PlayerId, event_type: &'static str) -> Ability {
    let (owner, condition_owner) = (seat.clone(), seat.clone());
    Ability::stateful(
        format!("breakthrough:{owner_name}:{BREAKTHROUGH}:{event_type}:after"),
        seat.clone(),
        event_type,
        Relation::After,
        Arc::new(move |_event, _resolver, context| {
            let decision_log = context.table.log.clone();
            let result = (|| {
                let sites = synergy_sites(context.state, context.content, context.sources, &owner);
                // Every question first.
                let (system, spots) = match sites.as_slice() {
                    [] => return Ok(()),
                    [only] => only.clone(),
                    _ => {
                        let options = sites
                            .iter()
                            .map(|(system, _)| {
                                ChoiceOption::labelled(
                                    system.to_string(),
                                    "system",
                                    format!("flip with the mechs in system {system}"),
                                )
                            })
                            .collect();
                        let id = ask_checked(
                            context,
                            &owner,
                            BREAKTHROUGH,
                            "synergy_system",
                            "Absolute Synergy: which system's mechs",
                            options,
                        )?;
                        let Some(site) = sites.iter().find(|(system, _)| system.as_str() == id)
                        else {
                            return Ok(());
                        };
                        site.clone()
                    }
                };
                let spot = if let [only] = spots.as_slice() {
                    only.clone()
                } else {
                    let options = spots
                        .iter()
                        .map(|spot| match spot {
                            None => ChoiceOption::labelled(
                                "space".to_owned(),
                                "spot",
                                "keep the mech in the space area".to_owned(),
                            ),
                            Some(planet) => ChoiceOption::labelled(
                                planet.to_string(),
                                "spot",
                                format!("keep the mech on {planet}"),
                            ),
                        })
                        .collect();
                    let id = ask_checked(
                        context,
                        &owner,
                        BREAKTHROUGH,
                        "synergy_survivor",
                        "Absolute Synergy: which mech becomes the Eidolon Maximum",
                        options,
                    )?;
                    let Some(spot) = spots.iter().find(|spot| match spot {
                        None => id == "space",
                        Some(planet) => planet.as_str() == id,
                    }) else {
                        return Ok(());
                    };
                    spot.clone()
                };
                let candidates = survivor_choices(
                    context.state,
                    context.content,
                    context.sources,
                    &owner,
                    &system,
                    &spot,
                );
                let keep = match candidates.as_slice() {
                    [] => return Ok(()),
                    [only] => only.1.clone(),
                    _ => {
                        let options = candidates
                            .iter()
                            .map(|(choice, _)| {
                                ChoiceOption::labelled(
                                    survivor_option_id(choice),
                                    "survivor",
                                    survivor_label(choice),
                                )
                            })
                            .collect();
                        let id = ask_checked(
                            context,
                            &owner,
                            BREAKTHROUGH,
                            "synergy_survivor_state",
                            "Absolute Synergy: which mech becomes the Eidolon Maximum",
                            options,
                        )?;
                        let Some((_, position)) = candidates
                            .iter()
                            .find(|(choice, _)| survivor_option_id(choice) == id)
                        else {
                            return Ok(());
                        };
                        position.clone()
                    }
                };
                flip_synergy(
                    context.state,
                    context.content,
                    context.sources,
                    &owner,
                    &system,
                    &keep,
                );
                Ok(())
            })();
            if result.is_err() {
                context.table.log = decision_log;
            }
            result
        }),
    )
    .with_optional(true)
    .with_stateful_condition(Arc::new(move |event, _, context| {
        event
            .text("player")
            .is_none_or(|player| player == condition_owner.as_str())
            && !synergy_sites(
                context.state,
                context.content,
                context.sources,
                &condition_owner,
            )
            .is_empty()
    }))
}

/// "Game effects cannot place or produce your mechs": production only; placement by other effects
/// is a shared seam (see the evidence file).
fn cannot_produce(
    state: &GameState,
    _content: &ContentStore,
    player: &PlayerId,
    unit_base: &str,
    _producer_base: &str,
) -> bool {
    unit_base == "mech" && is_naaz(state, player) && voltron_on_board(state, player)
}

/// Eidolon Maximum: "Game effects cannot place or produce your mechs." The shared placement
/// helper passes the resolved faction unit, so upgrades and forms cannot evade this gate.
fn effect_placement_forbidden(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    unit: &ti4_model::id::UnitTypeId,
) -> bool {
    is_naaz(state, player)
        && voltron_on_board(state, player)
        && catalogue(content, sources)
            .get(unit.as_str())
            .is_some_and(|kind| kind.base_type() == "mech")
}

/// A Maximum flips back when it is removed; captured models therefore return as Eidolons.
fn captured_unit_return_form(
    _state: &GameState,
    _content: &ContentStore,
    _sources: SourceSet,
    _owner: &PlayerId,
    unit: &ti4_model::id::UnitTypeId,
) -> ti4_model::id::UnitTypeId {
    if unit.as_str() == VOLTRON {
        ti4_model::id::UnitTypeId::new("naaz_mech")
    } else {
        unit.clone()
    }
}

fn ability_hit_immune(
    _state: &GameState,
    _content: &ContentStore,
    _sources: SourceSet,
    unit: &CombatUnit<'_>,
) -> bool {
    unit.unit_type == VOLTRON
}

/// "Repair it at the start of every combat round": the player's Eidolon Maximum in the space area
/// of `system` (space combat) or on `planet` (ground combat).
fn repair_voltron(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    system: &SystemId,
    planet: Option<&PlanetId>,
) {
    if !is_naaz(state, player) {
        return;
    }
    let repairs_planets = planet.is_none()
        && crate::combat::planetary_maximum_participates(state, content, sources, player, system);
    if repairs_planets {
        for unit in state
            .system_mut(system)
            .planet_units
            .values_mut()
            .flatten()
            .filter(|unit| is_voltron(unit, player))
        {
            unit.sustained_damage = false;
        }
    }
    let spot: Spot = planet.cloned();
    if let Some(pool) = pool_mut(state, system, &spot) {
        for unit in pool.iter_mut().filter(|unit| is_voltron(unit, player)) {
            unit.sustained_damage = false;
        }
    }
}

// -- timing abilities ----------------------------------------------------------------------------

fn timing_abilities(state: &GameState, owner_name: &str, seat: &PlayerId) -> Vec<Ability> {
    let mut abilities = vec![
        agent(owner_name, seat),
        commander(owner_name, seat),
        absolute_synergy(owner_name, seat, "BREAKTHROUGH_GAINED"),
        absolute_synergy(owner_name, seat, "ACTION_COMPLETED"),
        absolute_synergy(owner_name, seat, "NAAZ_MECH_PLACED"),
    ];
    if state
        .player(seat)
        .is_some_and(|source| source.leaders.contains_key(&LeaderId::new(AGENT)))
    {
        for candidate in &state.players {
            if &candidate.id != seat
                && candidate
                    .leaders
                    .contains_key(&LeaderId::new("yssarilagent"))
            {
                abilities.push(borrowed_agent(owner_name, seat, &candidate.id));
            }
        }
    }
    abilities
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::choice::Window;
    use std::collections::BTreeMap;
    use ti4_model::content_types::DEFAULT;
    use ti4_model::id::TechnologyId;

    fn a() -> PlayerId {
        PlayerId::new("a")
    }
    fn b() -> PlayerId {
        PlayerId::new("b")
    }
    fn game() -> GameState {
        crate::fixtures::seated_game(&[("a", FACTION), ("b", "sol")], DEFAULT)
    }
    fn ssruu_game() -> GameState {
        crate::fixtures::seated_game(&[("a", FACTION), ("b", "yssaril"), ("c", "sol")], DEFAULT)
    }
    fn scripted(answers: &[&str]) -> crate::choice::Table {
        crate::choice::Table::with_default(Box::new(crate::choice::Scripted::new(
            answers.iter().map(|s| (*s).to_owned()),
        )))
    }
    #[derive(Debug)]
    struct SsruuAgentDecider {
        planet: String,
    }
    impl crate::choice::Decider for SsruuAgentDecider {
        fn choose(
            &mut self,
            choice: &Choice,
        ) -> Result<ChoiceOption, crate::choice::IllegalChoice> {
            if choice.player == a()
                && choice
                    .options
                    .iter()
                    .any(|option| option.id == AGENT_ABILITY)
            {
                return choice
                    .options
                    .iter()
                    .find(|option| option.is_decline())
                    .cloned()
                    .ok_or_else(|| crate::choice::IllegalChoice::NoOptions {
                        player: choice.player.clone(),
                        prompt: choice.prompt.clone(),
                    });
            }
            if let Some(option) = choice.options.iter().find(|option| {
                option
                    .id
                    .contains(":yssarilagent:naazagent:TURN_PASSED:after")
            }) {
                assert_eq!(choice.player, b(), "the copied window belongs to Ssruu");
                return Ok(option.clone());
            }
            if choice.options.iter().any(|option| option.id == self.planet) {
                assert_eq!(
                    choice.player,
                    PlayerId::new("c"),
                    "the ending player chooses their exploration planet"
                );
            }
            if choice.player == PlayerId::new("c") {
                if let Some(option) = choice.options.iter().find(|o| o.id == self.planet) {
                    return Ok(option.clone());
                }
                if let Some(option) = choice.options.iter().find(|o| !o.is_decline()) {
                    return Ok(option.clone());
                }
            }
            choice
                .options
                .iter()
                .find(|option| option.is_decline())
                .cloned()
                .ok_or_else(|| crate::choice::IllegalChoice::NoOptions {
                    player: choice.player.clone(),
                    prompt: choice.prompt.clone(),
                })
        }
    }
    fn ssruu_agent_table(planet: &PlanetId) -> crate::choice::Table {
        crate::choice::Table::with_default(Box::new(SsruuAgentDecider {
            planet: planet.to_string(),
        }))
    }
    fn emit(
        state: &mut GameState,
        table: &mut crate::choice::Table,
        event_type: &str,
        pairs: &[(&str, &str)],
    ) {
        let payload: BTreeMap<String, serde_json::Value> = pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), serde_json::Value::from(*v)))
            .collect();
        let mut resolver = crate::fixtures::armed_resolver(state);
        crate::fixtures::with_context(state, DEFAULT, None, table, |ctx| {
            let event = ctx
                .event_sequence
                .next(event_type, payload)
                .expect("an event id");
            resolver
                .emit_with_context(ctx, event, |_, _| {})
                .expect("the window resolves");
        });
    }
    fn give_fragments(state: &mut GameState, who: &PlayerId, kind: &str, n: i32) {
        state
            .player_mut(who)
            .unwrap()
            .relic_fragments
            .insert(kind.to_owned(), n);
    }
    fn perform(state: &mut GameState, who: &PlayerId, id: &str, answers: &[&str]) -> bool {
        let options = component_actions(state, ContentStore::embedded(), who);
        let option = options.into_iter().find(|o| o.id == id).expect("offered");
        crate::fixtures::with_context(state, DEFAULT, None, &mut scripted(answers), |ctx| {
            perform_component(ctx, who, &option)
        })
    }
    /// An explorable planet outside every home system, put under `who`'s control.
    fn take_explorable(state: &mut GameState, who: &PlayerId) -> PlanetId {
        let content = ContentStore::embedded();
        let catalogue = ti4_content::galaxy::all_planets(content, DEFAULT);
        let planet = crate::fixtures::non_home_planets(200)
            .into_iter()
            .map(PlanetId::new)
            .find(|p| {
                explorable(state, content, DEFAULT, p)
                    && catalogue
                        .get(p.as_str())
                        .and_then(ti4_content::Planet::system_id)
                        .is_some()
            })
            .expect("an explorable planet");
        let system = SystemId::new(catalogue[planet.as_str()].system_id().unwrap());
        state
            .system_mut(&system)
            .planet_control
            .insert(planet.clone(), who.clone());
        planet
    }
    fn controlled_by(state: &GameState, who: &PlayerId) -> Vec<(SystemId, PlanetId)> {
        state
            .controlled_planets(who)
            .into_iter()
            .map(|(s, p)| (s.clone(), p.clone()))
            .collect()
    }

    // -- neutrality ------------------------------------------------------------------------------

    #[test]
    fn a_game_without_naaz_is_offered_nothing() {
        let mut state = crate::fixtures::seated_game(&[("a", "sol"), ("b", "hacan")], DEFAULT);
        let content = ContentStore::embedded();
        give_fragments(&mut state, &a(), "CULTURAL", 3);
        assert!(component_actions(&state, content, &a()).is_empty());
        assert_eq!(
            leader_action(&state, content, &a(), &LeaderId::new(HERO)),
            Some(true),
            "the id check is by leader, ownership is the leaders module's"
        );
        let before = state.clone();
        let planet = controlled_by(&state, &a())[0].1.clone();
        emit(
            &mut state,
            &mut scripted(&[]),
            "TURN_PASSED",
            &[("player", "a")],
        );
        emit(
            &mut state,
            &mut scripted(&[]),
            "PLANET_CONTROL_GAINED",
            &[
                ("player", "a"),
                ("planet", planet.as_str()),
                ("previous_owner", "b"),
            ],
        );
        assert_eq!(state.exploration_log, before.exploration_log);
        assert_eq!(state.board, before.board);
        assert!(state.faction_marks.is_empty());
    }

    // -- Fabrication -----------------------------------------------------------------------------

    #[test]
    fn fabrication_turns_two_matching_fragments_into_a_relic() {
        let mut state = game();
        assert!(component_actions(&state, ContentStore::embedded(), &a()).is_empty());
        give_fragments(&mut state, &a(), "CULTURAL", 1);
        assert!(
            !component_actions(&state, ContentStore::embedded(), &a())
                .iter()
                .any(|o| o.id == FAB_RELIC),
            "one fragment is not enough"
        );
        give_fragments(&mut state, &a(), "CULTURAL", 2);
        let relics = state.player(&a()).unwrap().relics.len();
        assert!(perform(&mut state, &a(), FAB_RELIC, &[]));
        assert_eq!(state.player(&a()).unwrap().relics.len(), relics + 1);
        assert_eq!(fragments(&state, &a(), "CULTURAL"), 0);
    }

    #[test]
    fn fabrication_drawing_the_shard_of_the_throne_awards_its_point() {
        // The purge goes through `relics::gain`, which applies the Shard's +1 VP for every version
        // (the Naaz path was reported as missing it; it never did).
        for version in [ti4_model::state::RULES_FACTION_EFFECTS - 1, ti4_model::state::RULES_VERSION] {
            let mut state = game();
            state.rules_version = version;
            state.relic_deck = vec![ti4_model::id::RelicId::new(crate::relics::SHARD)];
            give_fragments(&mut state, &a(), "CULTURAL", 2);
            let before = state.player(&a()).unwrap().victory_points;
            assert!(perform(&mut state, &a(), FAB_RELIC, &[]));
            assert_eq!(state.player(&a()).unwrap().victory_points, before + 1, "version {version}");
        }
    }

    #[test]
    fn fabrication_frontier_fragments_stand_in_and_are_spent_last() {
        let mut state = game();
        give_fragments(&mut state, &a(), "HAZARDOUS", 1);
        give_fragments(&mut state, &a(), "FRONTIER", 1);
        assert!(perform(&mut state, &a(), FAB_RELIC, &[]));
        assert_eq!(fragments(&state, &a(), "HAZARDOUS"), 0);
        assert_eq!(fragments(&state, &a(), "FRONTIER"), 0);
    }

    #[test]
    fn fabrication_turns_one_fragment_into_a_token_of_the_chosen_pool() {
        let mut state = game();
        give_fragments(&mut state, &a(), "INDUSTRIAL", 1);
        let before = state.player(&a()).unwrap().tokens(TokenPool::Fleet);
        assert!(perform(&mut state, &a(), FAB_TOKEN, &["fleet"]));
        assert_eq!(
            state.player(&a()).unwrap().tokens(TokenPool::Fleet),
            before + 1
        );
        assert_eq!(fragments(&state, &a(), "INDUSTRIAL"), 0);
        assert!(
            !component_actions(&state, ContentStore::embedded(), &a())
                .iter()
                .any(|o| o.id == FAB_TOKEN),
            "no fragment left"
        );
    }

    // -- Black Market Forgery --------------------------------------------------------------------

    #[test]
    fn black_market_forgery_is_the_holders_action_and_returns_home() {
        let mut state = game();
        give_fragments(&mut state, &b(), "CULTURAL", 2);
        assert!(
            component_actions(&state, ContentStore::embedded(), &b()).is_empty(),
            "not held"
        );
        let note = crate::promissory::note_id(BMF, "naaz");
        state.promissory_notes.insert(note.clone(), b());
        let relics = state.player(&b()).unwrap().relics.len();
        assert!(perform(&mut state, &b(), BMF_ACTION, &[]));
        assert_eq!(state.player(&b()).unwrap().relics.len(), relics + 1);
        assert_eq!(state.promissory_notes.get(&note), Some(&a()));
        assert!(
            !component_actions(&state, ContentStore::embedded(), &a())
                .iter()
                .any(|o| o.id == BMF_ACTION),
            "its owner cannot play it"
        );
    }

    #[test]
    fn black_market_forgery_needs_two_matching_fragments() {
        let mut state = game();
        state
            .promissory_notes
            .insert(crate::promissory::note_id(BMF, "naaz"), b());
        give_fragments(&mut state, &b(), "CULTURAL", 1);
        assert!(component_actions(&state, ContentStore::embedded(), &b()).is_empty());
    }

    // -- Visz El Vir -----------------------------------------------------------------------------

    #[test]
    fn the_flagship_gives_mechs_in_its_system_a_die() {
        let mut state = game();
        let content = ContentStore::embedded();
        let system = SystemId::new("18");
        let mech = |state: &GameState, kind: &str, who: &PlayerId| {
            unit_dice(
                state,
                content,
                DEFAULT,
                &CombatUnit {
                    player: who,
                    system: Some(&system),
                    planet: None,
                    unit_type: kind,
                    context: "space",
                },
                2,
            )
        };
        assert_eq!(mech(&state, "naaz_mech", &a()), 2, "no flagship");
        crate::fixtures::put(&mut state, &system, "naaz_flagship", &a(), 1);
        assert_eq!(mech(&state, "naaz_mech", &a()), 3);
        assert_eq!(mech(&state, "naaz_mech_space", &a()), 3);
        assert_eq!(mech(&state, "naaz_flagship", &a()), 2, "not a mech");
        assert_eq!(mech(&state, "sol_mech", &a()), 2, "not a Naaz mech");
        assert_eq!(mech(&state, "naaz_mech", &b()), 2, "another player's");
    }

    // -- Supercharge (space combat rounds) ---------------------------------------------------------

    #[test]
    fn supercharge_adds_one_for_the_round_it_was_exhausted_in() {
        let mut state = game();
        let content = ContentStore::embedded();
        let system = SystemId::new("18");
        let probe = |state: &GameState| {
            unit_roll_modifier(
                state,
                content,
                DEFAULT,
                &CombatUnit {
                    player: &a(),
                    system: Some(&system),
                    planet: None,
                    unit_type: "naaz_flagship",
                    context: "space",
                },
            )
        };
        // Not owned: not asked.
        space_combat_round_started(&mut state, content, DEFAULT, &mut scripted(&["sc"]), &a());
        assert_eq!(probe(&state), 0);
        crate::technology::grant(&mut state, &a(), &TechnologyId::new("sc"));
        space_combat_round_started(
            &mut state,
            content,
            DEFAULT,
            &mut scripted(&["decline"]),
            &a(),
        );
        assert_eq!(probe(&state), 0);
        assert!(technology_ready(&state, &a(), "sc"));
        space_combat_round_started(&mut state, content, DEFAULT, &mut scripted(&["sc"]), &a());
        assert_eq!(probe(&state), 1);
        assert!(!technology_ready(&state, &a(), "sc"));
        state.combat_round_seq += 1;
        assert_eq!(probe(&state), 0, "only the round it was used in");
    }

    #[test]
    fn supercharge_also_starts_ground_combat_rounds() {
        let mut state = game();
        let content = ContentStore::embedded();
        let (system, planet) = (SystemId::new("18"), PlanetId::new("mr"));
        let probe = |state: &GameState, context: &str| {
            unit_roll_modifier(
                state,
                content,
                DEFAULT,
                &CombatUnit {
                    player: &a(),
                    system: Some(&system),
                    planet: Some(&planet),
                    unit_type: "naaz_mech",
                    context,
                },
            )
        };
        let ask = |state: &mut GameState, answers: &[&str]| {
            ground_combat_round_started(
                state,
                content,
                DEFAULT,
                &mut scripted(answers),
                &a(),
                &system,
                &planet,
            );
        };
        ask(&mut state, &["sc"]);
        assert_eq!(probe(&state, "ground"), 0, "not owned: not asked");
        crate::technology::grant(&mut state, &a(), &TechnologyId::new("sc"));
        ask(&mut state, &["decline"]);
        assert_eq!(probe(&state, "ground"), 0);
        ask(&mut state, &["sc"]);
        assert_eq!(probe(&state, "ground"), 1);
        assert_eq!(
            probe(&state, "space"),
            0,
            "a ground round's use is not a space round's"
        );
        assert!(!technology_ready(&state, &a(), "sc"));
    }

    // -- Distant Suns and Pre-Fab Arcologies -------------------------------------------------------

    fn system_of(planet: &PlanetId) -> SystemId {
        let catalogue = ti4_content::galaxy::all_planets(ContentStore::embedded(), DEFAULT);
        SystemId::new(catalogue[planet.as_str()].system_id().unwrap())
    }

    fn deck_len(state: &GameState, planet: &PlanetId) -> usize {
        let deck = crate::exploration::trait_of(ContentStore::embedded(), DEFAULT, planet).unwrap();
        state.exploration_decks[&deck].len()
    }

    fn explore_once(state: &mut GameState, who: &PlayerId, planet: &PlanetId) {
        let deck = crate::exploration::trait_of(ContentStore::embedded(), DEFAULT, planet).unwrap();
        crate::exploration::explore(state, ContentStore::embedded(), who, &deck, Some(planet))
            .expect("a card");
    }

    #[test]
    fn distant_suns_draws_an_extra_card_for_a_planet_with_a_mech() {
        let mut state = game();
        let planet = take_explorable(&mut state, &a());
        let system = system_of(&planet);
        let before = deck_len(&state, &planet);
        explore_once(&mut state, &a(), &planet);
        assert_eq!(deck_len(&state, &planet), before - 1, "no mech: one card");
        crate::fixtures::put_on_planet(&mut state, &system, &planet, "naaz_mech", &a(), 1);
        explore_once(&mut state, &a(), &planet);
        assert_eq!(deck_len(&state, &planet), before - 3, "a mech: one extra");
        // Another player's mech does not count, and neither does a Sol player's own.
        let other = take_explorable(&mut state, &b());
        crate::fixtures::put_on_planet(&mut state, &system_of(&other), &other, "sol_mech", &b(), 1);
        let held = deck_len(&state, &other);
        explore_once(&mut state, &b(), &other);
        assert_eq!(deck_len(&state, &other), held - 1);
    }

    #[test]
    fn pre_fab_arcologies_readies_the_planet_after_exploring() {
        let mut state = game();
        let planet = take_explorable(&mut state, &a());
        state.exhausted_planets.insert(planet.clone());
        explore_once(&mut state, &a(), &planet);
        assert!(state.exhausted_planets.contains(&planet), "no technology");
        crate::technology::grant(&mut state, &a(), &TechnologyId::new("pfa"));
        explore_once(&mut state, &a(), &planet);
        assert!(!state.exhausted_planets.contains(&planet));
    }

    // -- Eidolon forms ---------------------------------------------------------------------------

    #[test]
    fn the_eidolon_is_a_ship_in_the_active_space_area_and_flips_back() {
        let mut state = game();
        let content = ContentStore::embedded();
        let system = SystemId::new("18");
        crate::fixtures::put(&mut state, &system, "naaz_mech", &a(), 1);
        let kinds = |state: &GameState| -> Vec<String> {
            state
                .system_state(&system)
                .units
                .iter()
                .filter(|unit| unit.owner == a())
                .map(|unit| unit.type_id.to_string())
                .collect()
        };
        let flipped =
            crate::fleet::flip_to_ship_forms(&mut state, content, DEFAULT, &[a()], &system);
        assert_eq!(flipped.len(), 1);
        assert_eq!(kinds(&state), ["naaz_mech_space"]);
        let types = catalogue(content, DEFAULT);
        assert!(types["naaz_mech_space"].is_ship() && !types["naaz_mech"].is_ship());
        crate::fleet::flip_to_ground_forms(&mut state, content, DEFAULT, &[a()], &system);
        assert_eq!(kinds(&state), ["naaz_mech"]);
        // Another faction's mech never flips.
        crate::fixtures::put(&mut state, &system, "sol_mech", &b(), 1);
        assert!(
            crate::fleet::flip_to_ship_forms(&mut state, content, DEFAULT, &[b()], &system)
                .is_empty()
        );
    }

    // -- Garv and Gunn ---------------------------------------------------------------------------

    const AGENT_ABILITY: &str = "leader:naaz:naazagent:TURN_PASSED:after";

    #[test]
    fn the_agent_lets_a_player_explore_one_of_their_planets() {
        let mut state = game();
        let planet = take_explorable(&mut state, &b());
        let log = state.exploration_log.len();
        emit(
            &mut state,
            &mut scripted(&[AGENT_ABILITY, planet.as_str()]),
            "TURN_PASSED",
            &[("player", "b")],
        );
        assert_eq!(state.exploration_log.len(), log + 1);
        assert_eq!(state.exploration_log[log].player, b());
        assert_eq!(state.exploration_log[log].planet.as_ref(), Some(&planet));
        assert_eq!(
            leader_status(&state, &a(), AGENT),
            Some(LeaderStatus::Exhausted)
        );
    }

    #[test]
    fn the_agent_may_be_declined_and_must_be_ready() {
        let mut state = game();
        take_explorable(&mut state, &b());
        let log = state.exploration_log.len();
        emit(
            &mut state,
            &mut scripted(&["decline"]),
            "TURN_PASSED",
            &[("player", "b")],
        );
        assert_eq!(state.exploration_log.len(), log);
        assert_eq!(
            leader_status(&state, &a(), AGENT),
            Some(LeaderStatus::Readied)
        );
        crate::leaders::exhaust(&mut state, &a(), &LeaderId::new(AGENT));
        emit(
            &mut state,
            &mut scripted(&[AGENT_ABILITY]),
            "TURN_PASSED",
            &[("player", "b")],
        );
        assert_eq!(state.exploration_log.len(), log);
    }

    #[test]
    fn ssruu_copies_garv_and_gunn_for_the_ending_players_planet() {
        for source_status in [LeaderStatus::Readied, LeaderStatus::Exhausted] {
            let mut state = ssruu_game();
            state
                .player_mut(&a())
                .unwrap()
                .leaders
                .insert(LeaderId::new(AGENT), source_status);
            state
                .player_mut(&b())
                .unwrap()
                .leaders
                .insert(LeaderId::new("yssarilagent"), LeaderStatus::Readied);
            let target = PlayerId::new("c");
            let planet = take_explorable(&mut state, &target);
            let before = state.exploration_log.len();
            emit(
                &mut state,
                &mut ssruu_agent_table(&planet),
                "TURN_PASSED",
                &[("player", "c")],
            );

            assert_eq!(state.exploration_log.len(), before + 1);
            assert_eq!(state.exploration_log[before].player, target);
            assert_eq!(state.exploration_log[before].planet.as_ref(), Some(&planet));
            assert_eq!(
                leader_status(&state, &b(), "yssarilagent"),
                Some(LeaderStatus::Exhausted),
                "Ssruu carries the copied text"
            );
            assert_eq!(
                leader_status(&state, &a(), AGENT),
                Some(source_status),
                "native use is independently declined; copying leaves the source unchanged"
            );
        }
    }

    #[test]
    fn copied_naaz_planet_choice_errors_before_exhaustion_and_can_retry() {
        let mut state = ssruu_game();
        state
            .player_mut(&a())
            .unwrap()
            .leaders
            .insert(LeaderId::new(AGENT), LeaderStatus::Exhausted);
        state
            .player_mut(&b())
            .unwrap()
            .leaders
            .insert(LeaderId::new("yssarilagent"), LeaderStatus::Readied);
        let planet = take_explorable(&mut state, &PlayerId::new("c"));
        let before = state.clone();
        let mut resolver = crate::fixtures::armed_resolver(&state);
        let copied = "leader:naaz:a:yssarilagent:naazagent:TURN_PASSED:after";
        let mut table = scripted(&[copied, "invalid-planet"]);
        let result = crate::fixtures::with_context(&mut state, DEFAULT, None, &mut table, |ctx| {
            let event = ctx
                .event_sequence
                .next(
                    "TURN_PASSED",
                    BTreeMap::from([("player".to_owned(), serde_json::Value::from("c"))]),
                )
                .unwrap();
            resolver.emit_with_context(ctx, event, |_, _| {})
        });
        assert!(
            matches!(result, Err(crate::timing::TimingError::IllegalChoice(_))),
            "{result:?}"
        );
        assert_eq!(
            state, before,
            "invalid planet selection must precede any mutation or exhaustion"
        );
        emit(
            &mut state,
            &mut ssruu_agent_table(&planet),
            "TURN_PASSED",
            &[("player", "c")],
        );
        assert_eq!(
            state.exploration_log.len(),
            before.exploration_log.len() + 1
        );
        assert_eq!(
            leader_status(&state, &b(), "yssarilagent"),
            Some(LeaderStatus::Exhausted)
        );
        assert_eq!(
            leader_status(&state, &a(), AGENT),
            Some(LeaderStatus::Exhausted)
        );
    }

    #[derive(Debug)]
    struct SsruuNestedExplorationDecider {
        planet: String,
        fail_local_fabricators_once: bool,
    }

    impl crate::choice::Decider for SsruuNestedExplorationDecider {
        fn choose(
            &mut self,
            choice: &Choice,
        ) -> Result<ChoiceOption, crate::choice::IllegalChoice> {
            if choice.options.iter().any(|option| option.id == "pass") {
                return choice
                    .options
                    .iter()
                    .find(|option| option.id == "pass")
                    .cloned()
                    .ok_or_else(|| crate::choice::IllegalChoice::NoOptions {
                        player: choice.player.clone(),
                        prompt: choice.prompt.clone(),
                    });
            }
            if let Some(option) = choice.options.iter().find(|option| {
                option
                    .id
                    .contains(":yssarilagent:naazagent:TURN_PASSED:after")
            }) {
                return Ok(option.clone());
            }
            if let Some(option) = choice
                .options
                .iter()
                .find(|option| option.id == self.planet)
            {
                return Ok(option.clone());
            }
            if choice.prompt == "Local Fabricators" {
                if self.fail_local_fabricators_once {
                    self.fail_local_fabricators_once = false;
                    return Err(crate::choice::IllegalChoice::NotOffered {
                        player: choice.player.clone(),
                        chosen: "invalid-nested-answer".to_owned(),
                        offered: choice.ids().into_iter().map(str::to_owned).collect(),
                    });
                }
                return choice
                    .options
                    .iter()
                    .find(|option| option.id == "spend_tg")
                    .cloned()
                    .ok_or_else(|| crate::choice::IllegalChoice::NoOptions {
                        player: choice.player.clone(),
                        prompt: choice.prompt.clone(),
                    });
            }
            if let Some(option) = choice
                .options
                .iter()
                .find(|option| option.id == "INDUSTRIAL")
            {
                return Ok(option.clone());
            }
            choice
                .options
                .iter()
                .find(|option| option.is_decline())
                .or_else(|| choice.options.first())
                .cloned()
                .ok_or_else(|| crate::choice::IllegalChoice::NoOptions {
                    player: choice.player.clone(),
                    prompt: choice.prompt.clone(),
                })
        }
    }

    #[test]
    fn copied_naaz_nested_exploration_error_rolls_back_and_same_game_retries() {
        let mut state = ssruu_game();
        state.phase = ti4_model::state::Phase::Action;
        let actor = PlayerId::new("c");
        state.active = Some(actor.clone());
        state
            .player_mut(&a())
            .unwrap()
            .leaders
            .insert(LeaderId::new(AGENT), LeaderStatus::Exhausted);
        state
            .player_mut(&b())
            .unwrap()
            .leaders
            .insert(LeaderId::new("yssarilagent"), LeaderStatus::Readied);
        state.player_mut(&actor).unwrap().trade_goods = 1;
        state.player_mut(&actor).unwrap().commodities = 0;

        let content = ContentStore::embedded();
        let catalogue = ti4_content::galaxy::all_planets(content, DEFAULT);
        let planet = crate::fixtures::non_home_planets(200)
            .into_iter()
            .map(PlanetId::new)
            .find(|planet| {
                explorable(&state, content, DEFAULT, planet)
                    && crate::planets::traits_now(&state, content, DEFAULT, planet)
                        .contains(&"INDUSTRIAL".to_owned())
            })
            .expect("an unclaimed industrial planet");
        let system = SystemId::new(catalogue[planet.as_str()].system_id().unwrap());
        state
            .system_mut(&system)
            .planet_control
            .insert(planet.clone(), actor.clone());
        state.exploration_decks.insert(
            "INDUSTRIAL".to_owned(),
            vec!["lf1".to_owned(), "lf2".to_owned()],
        );

        let table = crate::choice::Table::with_default(Box::new(SsruuNestedExplorationDecider {
            planet: planet.to_string(),
            fail_local_fabricators_once: true,
        }));
        let mut game = crate::game::Game::with_table(state, content, table);
        let deck_before = game.state.exploration_decks["INDUSTRIAL"].clone();
        let exploration_before = game.state.exploration_log.clone();
        let trade_goods_before = game.state.player(&actor).unwrap().trade_goods;
        let commodity_before = game.state.player(&actor).unwrap().commodities;

        let failed = game.step();
        assert!(
            matches!(
                failed.error,
                Some(crate::game::GameError::Timing(
                    crate::timing::TimingError::IllegalChoice(_)
                ))
            ),
            "the swallowed nested-card choice must fail the copied turn-end effect: {failed:?}"
        );
        assert!(game.state.player(&actor).unwrap().passed);
        assert!(game.events.iter().any(|event| event == "PLAYER_PASSED"));
        assert!(!game.events.iter().any(|event| event == "TURN_PASSED"));
        assert_eq!(game.state.active.as_ref(), Some(&actor));
        assert_eq!(game.state.exploration_decks["INDUSTRIAL"], deck_before);
        assert_eq!(game.state.exploration_log, exploration_before);
        assert_eq!(
            game.state.player(&actor).unwrap().trade_goods,
            trade_goods_before
        );
        assert_eq!(
            game.state.player(&actor).unwrap().commodities,
            commodity_before
        );
        assert!(
            game.state
                .system_state(&system)
                .on_planet(&planet)
                .iter()
                .all(|unit| { unit.owner != actor || unit.type_id.as_str() != "sol_mech" })
        );
        assert_eq!(
            leader_status(&game.state, &b(), "yssarilagent"),
            Some(LeaderStatus::Readied),
            "failed exploration does not exhaust Ssruu"
        );
        assert_eq!(
            leader_status(&game.state, &a(), AGENT),
            Some(LeaderStatus::Exhausted),
            "the copied source remains unchanged"
        );
        assert!(game.table.log.records.iter().all(|record| {
            !record
                .chosen
                .contains(":yssarilagent:naazagent:TURN_PASSED:after")
                && record.chosen != planet.as_str()
                && record.chosen != "invalid-nested-answer"
        }));

        assert_eq!(game.step().error, None, "the same table retries the pass");
        assert_eq!(
            game.events
                .iter()
                .filter(|event| event.as_str() == "TURN_PASSED")
                .count(),
            2, // one driver label and one mirror of the successfully applied typed event
        );
        assert_eq!(
            game.timing_mut()
                .applied_events()
                .iter()
                .filter(|event| event.event_type == "TURN_PASSED")
                .count(),
            1
        );
        assert_eq!(
            game.state.exploration_decks["INDUSTRIAL"],
            vec!["lf2".to_owned()]
        );
        assert_eq!(
            game.state.exploration_log.len(),
            exploration_before.len() + 1
        );
        assert_eq!(game.state.player(&actor).unwrap().trade_goods, 0);
        assert_eq!(
            game.state.player(&actor).unwrap().commodities,
            commodity_before
        );
        assert_eq!(
            game.state
                .system_state(&system)
                .on_planet(&planet)
                .iter()
                .filter(|unit| unit.owner == actor && unit.type_id.as_str() == "sol_mech")
                .count(),
            1
        );
        assert_eq!(
            leader_status(&game.state, &b(), "yssarilagent"),
            Some(LeaderStatus::Exhausted)
        );
        assert_eq!(
            leader_status(&game.state, &a(), AGENT),
            Some(LeaderStatus::Exhausted)
        );
        assert_eq!(
            game.table
                .log
                .records
                .iter()
                .filter(|record| record
                    .chosen
                    .contains(":yssarilagent:naazagent:TURN_PASSED:after"))
                .count(),
            1,
            "the failed borrowed copy is rolled out of the log; the successful retry is counted once"
        );
        assert_eq!(
            game.table
                .log
                .records
                .iter()
                .filter(|record| record.chosen == planet.as_str())
                .count(),
            1,
            "the failed target-planet choice is also rolled out of the log"
        );
    }

    #[test]
    fn copied_agent_is_inert_without_readied_ssruu() {
        let mut state = ssruu_game();
        state
            .player_mut(&a())
            .unwrap()
            .leaders
            .insert(LeaderId::new(AGENT), LeaderStatus::Exhausted);
        let target = PlayerId::new("c");
        take_explorable(&mut state, &target);
        let before = state.exploration_log.len();
        for status in [None, Some(LeaderStatus::Exhausted)] {
            match status {
                Some(status) => {
                    state
                        .player_mut(&b())
                        .unwrap()
                        .leaders
                        .insert(LeaderId::new("yssarilagent"), status);
                }
                None => {
                    state
                        .player_mut(&b())
                        .unwrap()
                        .leaders
                        .remove(&LeaderId::new("yssarilagent"));
                }
            }
            emit(
                &mut state,
                &mut scripted(&[]),
                "TURN_PASSED",
                &[("player", "c")],
            );
        }
        assert_eq!(state.exploration_log.len(), before);
        assert_eq!(
            leader_status(&state, &a(), AGENT),
            Some(LeaderStatus::Exhausted)
        );
    }

    #[test]
    fn copied_agent_is_inert_when_the_source_agent_is_absent() {
        let mut state = ssruu_game();
        state
            .player_mut(&a())
            .unwrap()
            .leaders
            .remove(&LeaderId::new(AGENT));
        state
            .player_mut(&b())
            .unwrap()
            .leaders
            .insert(LeaderId::new("yssarilagent"), LeaderStatus::Readied);
        let target = PlayerId::new("c");
        take_explorable(&mut state, &target);
        let before = state.exploration_log.len();
        emit(
            &mut state,
            &mut scripted(&[]),
            "TURN_PASSED",
            &[("player", "c")],
        );
        assert_eq!(state.exploration_log.len(), before);
        assert_eq!(leader_status(&state, &a(), AGENT), None);
        assert_eq!(
            leader_status(&state, &b(), "yssarilagent"),
            Some(LeaderStatus::Readied)
        );
    }

    // -- Dart and Tai ----------------------------------------------------------------------------

    #[test]
    fn the_commander_unlocks_with_mechs_in_three_systems() {
        let mut state = game();
        let content = ContentStore::embedded();
        let ask = |state: &GameState, leader: &str| {
            commander_unlocked(state, content, DEFAULT, None, &a(), &LeaderId::new(leader))
        };
        let systems = ["18", "19", "20", "21"].map(SystemId::new);
        // The home system already holds the starting mech.
        crate::fixtures::put(&mut state, &systems[0], "naaz_mech", &a(), 2);
        assert_eq!(ask(&state, COMMANDER), Some(false), "two systems");
        crate::fixtures::put(&mut state, &systems[2], "naaz_mech", &b(), 1);
        assert_eq!(ask(&state, COMMANDER), Some(false), "another player's mech");
        crate::fixtures::put(&mut state, &systems[1], "naaz_mech_space", &a(), 1);
        assert_eq!(ask(&state, COMMANDER), Some(true));
        assert_eq!(ask(&state, "naalucommander"), None);
    }

    #[test]
    fn the_commander_explores_a_planet_taken_from_another_player() {
        let mut state = game();
        let planet = take_explorable(&mut state, &a());
        let ability = "leader:naaz:naazcommander:PLANET_CONTROL_GAINED:after";
        let pairs = [
            ("player", "a"),
            ("planet", planet.as_str()),
            ("previous_owner", "b"),
        ];
        let log = state.exploration_log.len();
        // Locked: nothing.
        emit(
            &mut state,
            &mut scripted(&[ability]),
            "PLANET_CONTROL_GAINED",
            &pairs,
        );
        assert_eq!(state.exploration_log.len(), log);
        state
            .player_mut(&a())
            .unwrap()
            .leaders
            .insert(LeaderId::new(COMMANDER), LeaderStatus::Unlocked);
        // A planet nobody held is explored by the rules, not by the commander.
        emit(
            &mut state,
            &mut scripted(&[ability]),
            "PLANET_CONTROL_GAINED",
            &[("player", "a"), ("planet", planet.as_str())],
        );
        assert_eq!(state.exploration_log.len(), log);
        emit(
            &mut state,
            &mut scripted(&[ability]),
            "PLANET_CONTROL_GAINED",
            &pairs,
        );
        assert_eq!(state.exploration_log.len(), log + 1);
        assert_eq!(state.exploration_log[log].player, a());
    }

    #[test]
    fn ownerless_commander_grant_explores_only_the_recipients_captured_planet() {
        let mut state = game();
        state.player_mut(&a()).unwrap().faction = ti4_model::id::FactionId::new("yin");
        state
            .player_mut(&a())
            .unwrap()
            .leaders
            .remove(&LeaderId::new(COMMANDER));
        let planet = take_explorable(&mut state, &a());
        assert!(crate::promissory::grant_commander_ability(
            &mut state,
            ContentStore::embedded(),
            &a(),
            COMMANDER
        ));
        let before = state.exploration_log.len();
        emit(
            &mut state,
            &mut scripted(&["leader:yin:naazcommander:PLANET_CONTROL_GAINED:after"]),
            "PLANET_CONTROL_GAINED",
            &[
                ("player", "a"),
                ("planet", planet.as_str()),
                ("previous_owner", "b"),
            ],
        );
        assert_eq!(state.exploration_log.len(), before + 1);
        assert_eq!(state.exploration_log[before].player, a());
        assert_eq!(leader_status(&state, &a(), COMMANDER), None);
        for board in state.board.values_mut() {
            if board.planet_control.contains_key(&planet) {
                board.set_control(planet.clone(), b());
            }
        }
        emit(
            &mut state,
            &mut scripted(&[]),
            "PLANET_CONTROL_GAINED",
            &[
                ("player", "a"),
                ("planet", planet.as_str()),
                ("previous_owner", "b"),
            ],
        );
        assert_eq!(
            state.exploration_log.len(),
            before + 1,
            "a stale event cannot explore another owner's planet"
        );
    }

    // -- Hesh and Prit ---------------------------------------------------------------------------

    #[test]
    fn the_hero_gains_a_relic_and_performs_up_to_two_secondaries() {
        let mut state = game();
        let hero = LeaderId::new(HERO);
        let cards = hero_cards(&state);
        assert!(cards.len() >= 2);
        let relics = state.player(&a()).unwrap().relics.len();
        let deck = state.relic_deck.len();
        let pick = |card: &StrategyCardId| card.to_string();
        let done = crate::fixtures::with_context(
            &mut state,
            DEFAULT,
            None,
            &mut scripted(&[&pick(&cards[0]), "decline"]),
            |ctx| use_leader(ctx, &a(), &hero),
        );
        assert_eq!(done, Some(true));
        assert_eq!(state.player(&a()).unwrap().relics.len(), relics + 1);
        assert_eq!(state.relic_deck.len(), deck - 1);
        assert_eq!(
            use_leader_other(&mut state),
            None,
            "another leader is not this module's"
        );
    }

    fn use_leader_other(state: &mut GameState) -> Option<bool> {
        crate::fixtures::with_context(state, DEFAULT, None, &mut scripted(&[]), |ctx| {
            use_leader(ctx, &a(), &LeaderId::new("naaluhero"))
        })
    }

    #[test]
    fn the_hero_offers_te_warfare_secondary_despite_its_free_tactical_primary() {
        let mut state = game();
        let card = StrategyCardId::new("te6warfare");
        state.unclaimed_strategy_cards = vec![card.clone()];
        assert!(hero_cards(&state).contains(&card));
        let content = ContentStore::embedded();
        let (home, planet) = crate::fixtures::a_placed_planet();
        state.player_mut(&a()).unwrap().home_system = Some(home.clone());
        state.player_mut(&a()).unwrap().trade_goods = 10;
        state.system_mut(&home).set_control(planet.clone(), a());
        crate::fixtures::put_on_planet(&mut state, &home, &planet, "spacedock", &a(), 1);
        let before = state.system_state(&home).units.len()
            + state.system_state(&home).on_planet(&planet).len();
        let relics = state.player(&a()).unwrap().relics.len();
        let mut table = scripted(&["te6warfare"]);
        assert_eq!(
            crate::fixtures::with_context(&mut state, DEFAULT, None, &mut table, |ctx| use_leader(
                ctx,
                &a(),
                &LeaderId::new(HERO)
            )),
            Some(true)
        );
        assert_eq!(state.player(&a()).unwrap().relics.len(), relics + 1);
        let after = state.system_state(&home).units.len()
            + state.system_state(&home).on_planet(&planet).len();
        assert!(
            after > before,
            "Warfare's secondary actually produces units"
        );
        assert!(crate::production::capacity(&state, content, DEFAULT, &a(), &home) > 0);
    }

    #[test]
    fn the_hero_is_not_offered_without_a_relic_to_gain() {
        let mut state = game();
        state.relic_deck.clear();
        assert_eq!(
            leader_action(&state, ContentStore::embedded(), &a(), &LeaderId::new(HERO)),
            Some(false)
        );
        let done =
            crate::fixtures::with_context(&mut state, DEFAULT, None, &mut scripted(&[]), |ctx| {
                use_leader(ctx, &a(), &LeaderId::new(HERO))
            });
        assert_eq!(done, Some(false));
    }

    #[test]
    fn the_hero_does_not_offer_exhausted_cards() {
        let mut state = game();
        let card = state.unclaimed_strategy_cards[0].clone();
        state.player_mut(&b()).unwrap().strategy_cards = vec![card.clone()];
        state
            .player_mut(&b())
            .unwrap()
            .exhausted_strategy_cards
            .insert(card.clone());
        state.unclaimed_strategy_cards.retain(|c| *c != card);
        assert!(!hero_cards(&state).contains(&card));
    }

    // -- Absolute Synergy and the Eidolon Maximum ------------------------------------------------

    const SYNERGY: &str = "breakthrough:naaz:naazbt:ACTION_COMPLETED:after";

    fn give_breakthrough(state: &mut GameState) {
        state.player_mut(&a()).unwrap().breakthrough =
            Some(ti4_model::id::BreakthroughId::new(BREAKTHROUGH));
    }
    fn mechs(state: &GameState, who: &PlayerId) -> Vec<(SystemId, Spot, String)> {
        let content = ContentStore::embedded();
        let mut found = Vec::new();
        for system in state.board.keys() {
            for (spot, index) in eidolons_in(state, content, DEFAULT, who, system) {
                let here = &state.board[system];
                let unit = match &spot {
                    None => &here.units[index],
                    Some(planet) => &here.planet_units[planet][index],
                };
                found.push((system.clone(), spot.clone(), unit.type_id.to_string()));
            }
        }
        found
    }
    fn voltrons(state: &GameState, who: &PlayerId) -> usize {
        state
            .board
            .values()
            .map(|here| {
                here.units
                    .iter()
                    .chain(here.planet_units.values().flatten())
                    .filter(|unit| is_voltron(unit, who))
                    .count()
            })
            .sum()
    }
    /// Four mechs in the home system: the starting one is topped up with space-area mechs.
    /// Take every Naaz mech off the board (the starting one included).
    fn strip_mechs(state: &mut GameState) {
        for here in state.board.values_mut() {
            here.units
                .retain(|unit| !unit.type_id.as_str().starts_with("naaz_mech"));
            for units in here.planet_units.values_mut() {
                units.retain(|unit| !unit.type_id.as_str().starts_with("naaz_mech"));
            }
        }
    }
    fn four_mechs_home(state: &mut GameState) -> SystemId {
        let system = SystemId::new("18");
        strip_mechs(state);
        crate::fixtures::put(state, &system, "naaz_mech_space", &a(), 4);
        system
    }

    #[test]
    fn four_mechs_in_one_system_flip_into_one_eidolon_maximum() {
        let mut state = game();
        give_breakthrough(&mut state);
        let system = four_mechs_home(&mut state);
        assert_eq!(mechs(&state, &a()).len(), 4);
        emit(
            &mut state,
            &mut scripted(&[SYNERGY, "space"]),
            "ACTION_COMPLETED",
            &[("player", "a")],
        );
        assert_eq!(voltrons(&state, &a()), 1);
        assert!(
            mechs(&state, &a()).is_empty(),
            "three returned, one flipped"
        );
        let held = crate::supply::held(&state, ContentStore::embedded(), DEFAULT, &a(), "mech");
        assert_eq!(held, 1, "three are back in the reinforcements");
        assert!(
            state
                .system_state(&system)
                .units
                .iter()
                .any(|unit| is_voltron(unit, &a()))
        );
    }

    #[test]
    fn breakthrough_gain_window_can_flip_four_mechs_into_the_maximum() {
        let mut state = game();
        give_breakthrough(&mut state);
        let system = four_mechs_home(&mut state);
        emit(
            &mut state,
            &mut scripted(&[
                "breakthrough:naaz:naazbt:BREAKTHROUGH_GAINED:after",
                "space",
            ]),
            "BREAKTHROUGH_GAINED",
            &[("player", "a"), ("breakthrough", BREAKTHROUGH)],
        );
        assert_eq!(voltrons(&state, &a()), 1);
        assert_eq!(mechs(&state, &a()).len(), 0);
        assert!(
            state
                .system_state(&system)
                .units
                .iter()
                .any(|unit| is_voltron(unit, &a()))
        );
    }

    #[test]
    fn absolute_synergy_needs_the_card_and_four_mechs_in_one_system() {
        let mut state = game();
        // No breakthrough.
        four_mechs_home(&mut state);
        let before = state.board.clone();
        emit(
            &mut state,
            &mut scripted(&[SYNERGY, "space"]),
            "ACTION_COMPLETED",
            &[("player", "a")],
        );
        assert_eq!(state.board, before, "not held");
        // Held, but another player's turn.
        give_breakthrough(&mut state);
        emit(
            &mut state,
            &mut scripted(&[SYNERGY, "space"]),
            "ACTION_COMPLETED",
            &[("player", "b")],
        );
        assert_eq!(state.board, before, "someone else's action");
        // Four mechs, but in two systems.
        let mut split = game();
        give_breakthrough(&mut split);
        crate::fixtures::put(&mut split, &SystemId::new("18"), "naaz_mech_space", &a(), 1);
        crate::fixtures::put(&mut split, &SystemId::new("19"), "naaz_mech_space", &a(), 2);
        assert_eq!(mechs(&split, &a()).len(), 4);
        let before = split.board.clone();
        emit(
            &mut split,
            &mut scripted(&[SYNERGY, "space"]),
            "ACTION_COMPLETED",
            &[("player", "a")],
        );
        assert_eq!(split.board, before, "four mechs, never four in one system");
        // Declined.
        give_breakthrough(&mut state);
        let before = state.board.clone();
        emit(
            &mut state,
            &mut scripted(&["decline"]),
            "ACTION_COMPLETED",
            &[("player", "a")],
        );
        assert_eq!(state.board, before);
    }

    #[test]
    fn the_survivor_is_the_mech_the_player_keeps_and_planets_are_offered() {
        let mut state = game();
        give_breakthrough(&mut state);
        let system = SystemId::new("18");
        strip_mechs(&mut state);
        let planet = PlanetId::new("mr");
        crate::fixtures::put_on_planet(&mut state, &system, &planet, "naaz_mech", &a(), 1);
        let total = eidolons_in(&state, ContentStore::embedded(), DEFAULT, &a(), &system).len();
        crate::fixtures::put(&mut state, &system, "naaz_mech_space", &a(), 4 - total);
        assert_eq!(mechs(&state, &a()).len(), 4);
        emit(
            &mut state,
            &mut scripted(&[SYNERGY, planet.as_str()]),
            "ACTION_COMPLETED",
            &[("player", "a")],
        );
        assert_eq!(voltrons(&state, &a()), 1);
        assert!(
            state
                .system_state(&system)
                .on_planet(&planet)
                .iter()
                .any(|unit| is_voltron(unit, &a())),
            "the mech on the planet is the one that stays"
        );
        assert!(
            !state
                .system_state(&system)
                .units
                .iter()
                .any(|unit| is_voltron(unit, &a()))
        );
    }

    #[test]
    fn absolute_synergy_can_keep_a_distinguishable_damaged_form() {
        let mut state = game();
        give_breakthrough(&mut state);
        let system = four_mechs_home(&mut state);
        let damaged = state
            .system_mut(&system)
            .units
            .iter_mut()
            .find(|unit| unit.type_id.as_str() == "naaz_mech_space")
            .unwrap();
        damaged.type_id = ti4_model::id::UnitTypeId::new("naaz_mech");
        damaged.sustained_damage = true;

        emit(
            &mut state,
            &mut scripted(&[SYNERGY, "survivor|naaz_mech|damaged|plain"]),
            "ACTION_COMPLETED",
            &[("player", "a")],
        );

        let maximum = state
            .system_state(&system)
            .units
            .iter()
            .find(|unit| is_voltron(unit, &a()))
            .cloned()
            .expect("the selected mech becomes the Maximum");
        assert!(
            maximum.sustained_damage,
            "the selected damage state survives"
        );
        assert_eq!(voltrons(&state, &a()), 1);
        assert_eq!(mechs(&state, &a()).len(), 0);
    }

    #[test]
    fn production_of_mechs_is_barred_while_the_maximum_stands() {
        let mut state = game();
        let content = ContentStore::embedded();
        assert!(!cannot_produce(&state, content, &a(), "mech", "spacedock"));
        crate::fixtures::put(&mut state, &SystemId::new("18"), "naaz_voltron", &a(), 1);
        assert!(cannot_produce(&state, content, &a(), "mech", "spacedock"));
        assert!(!cannot_produce(
            &state,
            content,
            &a(),
            "infantry",
            "spacedock"
        ));
        assert!(
            !cannot_produce(&state, content, &b(), "mech", "spacedock"),
            "another player's mechs"
        );
        // Destroyed or removed: the card is back.
        state
            .system_mut(&SystemId::new("18"))
            .units
            .retain(|unit| !is_voltron(unit, &a()));
        assert!(!cannot_produce(&state, content, &a(), "mech", "spacedock"));
    }

    #[test]
    fn production_choices_hide_mechs_only_while_the_maximum_stands() {
        let mut state = game();
        let content = ContentStore::embedded();
        let system = SystemId::new("18");
        state.player_mut(&a()).unwrap().trade_goods = 10;
        state
            .system_mut(&system)
            .set_control(PlanetId::new("mr"), a());
        let choices = |state: &GameState| {
            crate::production::ProductionWindow::for_ability(
                state,
                content,
                DEFAULT,
                &a(),
                &system,
                Some(5),
            )
            .pending_choice(state, content, DEFAULT)
            .map(|choice| {
                choice
                    .options
                    .iter()
                    .map(|option| option.id.clone())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
        };
        crate::fixtures::put(&mut state, &system, "naaz_voltron", &a(), 1);
        assert!(
            !choices(&state)
                .iter()
                .any(|id| id.starts_with("build|naaz_mech")),
            "actual production options apply the Maximum restriction"
        );
        state
            .system_mut(&system)
            .units
            .retain(|unit| !is_voltron(unit, &a()));
        assert!(
            choices(&state)
                .iter()
                .any(|id| id.starts_with("build|naaz_mech")),
            "the mech returns to the actual build offer when the Maximum leaves"
        );
    }

    #[test]
    fn the_maximum_is_repaired_at_the_start_of_each_combat_round() {
        let mut state = game();
        let content = ContentStore::embedded();
        let system = SystemId::new("18");
        crate::fixtures::put(&mut state, &system, "naaz_voltron", &a(), 1);
        state
            .system_mut(&system)
            .units
            .last_mut()
            .unwrap()
            .sustained_damage = true;
        state.active_system = Some(system.clone());
        // Another player's round start does not repair it.
        space_combat_round_started(&mut state, content, DEFAULT, &mut scripted(&[]), &b());
        assert!(
            state
                .system_state(&system)
                .units
                .last()
                .unwrap()
                .sustained_damage
        );
        space_combat_round_started(&mut state, content, DEFAULT, &mut scripted(&[]), &a());
        assert!(
            !state
                .system_state(&system)
                .units
                .last()
                .unwrap()
                .sustained_damage
        );
        // Ground: on the planet, not elsewhere.
        let planet = PlanetId::new("mr");
        crate::fixtures::put_on_planet(&mut state, &system, &planet, "naaz_voltron", &a(), 1);
        state
            .system_mut(&system)
            .planet_units
            .get_mut(&planet)
            .unwrap()
            .iter_mut()
            .for_each(|unit| unit.sustained_damage = true);
        ground_combat_round_started(
            &mut state,
            content,
            DEFAULT,
            &mut scripted(&[]),
            &a(),
            &system,
            &planet,
        );
        assert!(
            state
                .system_state(&system)
                .on_planet(&planet)
                .iter()
                .all(|unit| !unit.sustained_damage)
        );
    }

    /// Assign `hits` to `a`'s fleet in `system` through the shared absorption path, with the hits
    /// labelled `origin` and a decider that either always takes SUSTAIN DAMAGE when one is offered
    /// or always declines it. The producer is `b`, so nothing here is `a`'s own ability firing.
    fn assign(
        state: &mut GameState,
        system: &SystemId,
        hits: usize,
        origin: crate::combat::HitOrigin,
        take_sustain: bool,
    ) {
        let content = ContentStore::embedded();
        let mut dice = crate::dice::Dice::new();
        let mut rng = crate::rng::GameRng::new(0);
        let decider: Box<dyn crate::choice::Decider> = if take_sustain {
            Box::new(crate::choice::FirstOption)
        } else {
            Box::new(crate::choice::AlwaysDecline)
        };
        let mut table = crate::choice::Table::with_default(decider);
        let mut ctx = crate::choice::Resolving {
            content,
            sources: DEFAULT,
            dice: &mut dice,
            rng: &mut rng,
            table: &mut table,
            timing: None,
        };
        crate::combat::absorb_hits_seeing_with_origin(
            state,
            content,
            DEFAULT,
            None,
            &mut ctx,
            &a(),
            system,
            &b(),
            hits,
            origin,
        )
        .expect("the hits are assigned");
    }

    /// A Maximum and a fighter in the space area of system 18, in that order.
    fn maximum_and_a_fighter(state: &mut GameState) -> SystemId {
        let system = SystemId::new("18");
        crate::fixtures::put(state, &system, "naaz_voltron", &a(), 1);
        crate::fixtures::put(state, &system, "fighter", &a(), 1);
        system
    }

    /// `a`'s Maximum in `system`, undamaged and still whole.
    fn maximum_whole(state: &GameState, system: &SystemId) -> bool {
        state
            .system_state(system)
            .units
            .iter()
            .any(|unit| is_voltron(unit, &a()) && !unit.sustained_damage)
    }

    #[test]
    fn a_maximum_is_never_assigned_a_hit_from_a_unit_ability() {
        let mut state = game();
        let system = maximum_and_a_fighter(&mut state);
        // A decider that takes every SUSTAIN DAMAGE it is offered: if the Maximum were eligible for
        // an ability hit at all, that is where the hit would end up.
        assign(
            &mut state,
            &system,
            1,
            crate::combat::HitOrigin::UnitAbility,
            true,
        );
        assert!(
            maximum_whole(&state, &system),
            "the Maximum was damaged or destroyed by a hit from a unit ability"
        );
        assert_eq!(
            state
                .system_state(&system)
                .units
                .iter()
                .filter(|unit| unit.type_id.as_str() == "fighter")
                .count(),
            0,
            "the fighter is the unit the hit could be assigned to"
        );
    }

    #[test]
    fn an_ability_hit_is_discarded_when_nothing_else_can_take_it() {
        let mut state = game();
        let system = SystemId::new("18");
        crate::fixtures::put(&mut state, &system, "naaz_voltron", &a(), 1);
        assign(
            &mut state,
            &system,
            1,
            crate::combat::HitOrigin::UnitAbility,
            true,
        );
        assert!(
            maximum_whole(&state, &system),
            "15.2a discards a hit that cannot be assigned; the Maximum is not a unit an ability hit can be assigned to, and its SUSTAIN DAMAGE is not a way of making it assignable"
        );
        assert_eq!(voltrons(&state, &a()), 1);
    }

    #[test]
    fn a_maximum_takes_ordinary_combat_hits() {
        let mut state = game();
        let system = maximum_and_a_fighter(&mut state);
        // Declining SUSTAIN DAMAGE: the hit lands, and the Maximum is an ordinary ship here.
        assign(
            &mut state,
            &system,
            1,
            crate::combat::HitOrigin::CombatRoll,
            false,
        );
        assert_eq!(
            voltrons(&state, &a()),
            0,
            "a combat-roll hit destroys the Maximum when its SUSTAIN DAMAGE is declined"
        );
        assert_eq!(
            state
                .system_state(&system)
                .units
                .iter()
                .filter(|unit| unit.type_id.as_str() == "fighter")
                .count(),
            1,
            "the fighter is untouched"
        );
    }

    #[test]
    fn a_game_without_naaz_is_not_offered_absolute_synergy() {
        let mut state = crate::fixtures::seated_game(&[("a", "sol"), ("b", "hacan")], DEFAULT);
        let before = state.clone();
        emit(
            &mut state,
            &mut scripted(&[]),
            "ACTION_COMPLETED",
            &[("player", "a")],
        );
        emit(
            &mut state,
            &mut scripted(&[]),
            "BREAKTHROUGH_GAINED",
            &[("player", "a"), ("breakthrough", "naazbt")],
        );
        assert_eq!(state.board, before.board);
        assert!(!cannot_produce(
            &state,
            ContentStore::embedded(),
            &a(),
            "mech",
            "spacedock"
        ));
    }

    #[test]
    fn the_claims_are_the_sheet() {
        assert!(MODULE.units.contains(&"naaz_flagship"));
        assert!(MODULE.units.contains(&"naaz_voltron"));
        assert_eq!(MODULE.breakthroughs, &["naazbt"]);
        assert!(MODULE.leaders.len() == 3);
    }
    #[test]
    fn planetary_maximum_repairs_and_rolls_in_a_real_space_combat() {
        let mut state = game();
        let content = ContentStore::embedded();
        let system = SystemId::new("18");
        let planet = PlanetId::new("mr");
        state.active = Some(a());
        state.active_system = Some(system.clone());
        crate::fixtures::put(&mut state, &system, "fighter", &a(), 1);
        crate::fixtures::put(&mut state, &system, "cruiser", &b(), 1);
        crate::fixtures::put_on_planet(&mut state, &system, &planet, VOLTRON, &a(), 1);
        state
            .system_mut(&system)
            .planet_units
            .get_mut(&planet)
            .unwrap()[0]
            .sustained_damage = true;
        let mut dice = crate::dice::Dice::from_faces([1, 10, 10, 10, 10, 1]);
        let mut rng = crate::rng::GameRng::new(0);
        let mut table = crate::choice::Table::with_default(Box::new(crate::choice::AlwaysDecline));
        let result = crate::combat::resolve(
            &mut state, content, DEFAULT, &mut table, &mut dice, &mut rng, &system,
        )
        .unwrap();
        assert_eq!(result.winner, Some(a()));
        assert_eq!(
            result.rounds, 1,
            "the planet Maximum's four dice must join the fleet"
        );
        assert!(!state.system_state(&system).on_planet(&planet)[0].sustained_damage);
    }

    #[test]
    fn cargo_alone_does_not_offer_a_planetary_maximum_sustain() {
        let mut state = game();
        let system = SystemId::new("18");
        let planet = PlanetId::new("mr");
        crate::fixtures::put(&mut state, &system, "infantry", &a(), 1);
        crate::fixtures::put_on_planet(&mut state, &system, &planet, VOLTRON, &a(), 1);
        assign(
            &mut state,
            &system,
            1,
            crate::combat::HitOrigin::CombatRoll,
            true,
        );
        assert!(!state.system_state(&system).on_planet(&planet)[0].sustained_damage);
        assert!(
            crate::combat::ships_of(&state, ContentStore::embedded(), DEFAULT, &a(), &system)
                .is_empty()
        );
    }
    #[test]
    fn maximum_blocks_effect_placement_and_returns_captured_as_an_eidolon() {
        let mut state = game();
        let content = ContentStore::embedded();
        let system = SystemId::new("18");
        let planet = PlanetId::new("mr");
        crate::fixtures::put_on_planet(&mut state, &system, &planet, VOLTRON, &a(), 1);
        let mut table = scripted(&[]);
        crate::fixtures::with_context(&mut state, DEFAULT, None, &mut table, |ctx| {
            crate::action_cards::place_units(ctx, &a(), &system, Some(&planet), "mech", 1);
            assert_eq!(
                crate::action_cards::place_units_counted(
                    ctx,
                    &a(),
                    &system,
                    Some(&planet),
                    "mech",
                    1
                ),
                0
            );
            assert_eq!(
                crate::action_cards::place_units_counted(
                    ctx,
                    &a(),
                    &system,
                    Some(&planet),
                    "infantry",
                    1
                ),
                1
            );
        });
        assert_eq!(mechs(&state, &a()).len(), 1);
        let maximum = state
            .system_state(&system)
            .on_planet(&planet)
            .iter()
            .find(|unit| is_voltron(unit, &a()))
            .unwrap()
            .clone();
        assert!(crate::supply::capture_from_board(
            &mut state,
            &b(),
            &system,
            Some(&planet),
            &maximum
        ));
        assert_eq!(
            crate::supply::return_captured(&mut state, content, DEFAULT, &b(), &a(), "mech"),
            Some(ti4_model::id::UnitTypeId::new("naaz_mech"))
        );
        assert_eq!(voltrons(&state, &a()), 0);
        assert!(!cannot_produce(&state, content, &a(), "mech", "spacedock"));
    }

    #[test]
    fn planetary_maximum_retreats_with_its_fleet_without_duplicating_the_model() {
        let mut state = game();
        let system = SystemId::new("18");
        let planet = PlanetId::new("mr");
        let destination = SystemId::new("19");
        crate::fixtures::put(&mut state, &system, "carrier", &a(), 1);
        crate::fixtures::put_on_planet(&mut state, &system, &planet, VOLTRON, &a(), 1);
        assert_eq!(
            crate::combat::retreat_to(
                &mut state,
                ContentStore::embedded(),
                DEFAULT,
                &a(),
                &system,
                &destination
            ),
            0
        );
        assert!(state.system_state(&system).on_planet(&planet).is_empty());
        assert_eq!(
            state
                .system_state(&destination)
                .units
                .iter()
                .filter(|unit| is_voltron(unit, &a()))
                .count(),
            1
        );
        assert_eq!(voltrons(&state, &a()), 1);
    }

    #[test]
    fn synergy_nested_invalid_answer_restores_state_and_log_then_retries() {
        let mut state = game();
        give_breakthrough(&mut state);
        let system = four_mechs_home(&mut state);
        let damaged = &mut state.system_mut(&system).units[0];
        damaged.type_id = ti4_model::id::UnitTypeId::new("naaz_mech");
        damaged.sustained_damage = true;
        let mut game =
            crate::game::Game::new(state, ContentStore::embedded()).with_sources(DEFAULT);
        crate::supply::stage_event(
            &mut game.state,
            "ACTION_COMPLETED",
            &[("player".to_owned(), "a".into())].into(),
        );
        let before = game.state.clone();
        game.table = scripted(&[
            SYNERGY,
            "bogus-survivor-state",
            SYNERGY,
            "survivor|naaz_mech|damaged|plain",
        ]);
        let before_log = game.table.log.clone();
        assert!(game.step().error.is_some());
        assert_eq!(game.state, before);
        assert_eq!(game.table.log, before_log);
        assert_eq!(game.step().error, None);
        assert_eq!(voltrons(&game.state, &a()), 1);
        assert!(game.state.system_state(&system).units[0].sustained_damage);
        assert_eq!(
            game.table
                .log
                .records
                .iter()
                .filter(|r| r.chosen == SYNERGY)
                .count(),
            1
        );
    }

    #[test]
    fn rearmament_fourth_mech_opens_synergy_through_the_real_game_staged_delivery() {
        let mut state = game();
        give_breakthrough(&mut state);
        strip_mechs(&mut state);
        let home = state.player(&a()).unwrap().home_system.clone().unwrap();
        let planet = state
            .controlled_planets(&a())
            .into_iter()
            .find(|(s, _)| **s == home)
            .unwrap()
            .1
            .clone();
        crate::fixtures::put_on_planet(&mut state, &home, &planet, "naaz_mech", &a(), 3);
        let mut game =
            crate::game::Game::new(state, ContentStore::embedded()).with_sources(DEFAULT);
        assert!(matches!(
            crate::agenda_effects::resolve(
                &mut game.state,
                ContentStore::embedded(),
                "rearmament",
                "for",
                &crate::Ballot::default()
            ),
            crate::agenda_effects::Effect::Resolved { .. }
        ));
        assert_eq!(mechs(&game.state, &a()).len(), 4);
        assert!(
            crate::supply::staged_event_types(&game.state)
                .iter()
                .any(|kind| kind == "NAAZ_MECH_PLACED")
        );
        game.table = scripted(&["breakthrough:naaz:naazbt:NAAZ_MECH_PLACED:after"]);
        // The next real driver step flushes the post-effect event before presenting any next choice.
        assert_eq!(game.step().error, None);
        assert_eq!(voltrons(&game.state, &a()), 1);
        assert_eq!(mechs(&game.state, &a()).len(), 0);
    }

    // -- Real-route closure tests for the Eidolon Maximum claim ---------------------------------

    /// Takes the first mech build offered, then the first non-decline option for every other
    /// question (placement, payment).
    #[derive(Debug)]
    struct BuildMechDecider;
    impl crate::choice::Decider for BuildMechDecider {
        fn choose(
            &mut self,
            choice: &Choice,
        ) -> Result<ChoiceOption, crate::choice::IllegalChoice> {
            choice
                .options
                .iter()
                .find(|option| option.id.starts_with("build|naaz_mech|"))
                .or_else(|| choice.options.iter().find(|option| !option.is_decline()))
                .or_else(|| choice.options.first())
                .cloned()
                .ok_or_else(|| crate::choice::IllegalChoice::NoOptions {
                    player: choice.player.clone(),
                    prompt: choice.prompt.clone(),
                })
        }
    }

    /// G1: a real production window (another seat's turn, as in a strategic secondary) that
    /// places the fourth mech announces `NAAZ_MECH_PLACED`, and the staged event opens Absolute
    /// Synergy before the next choice.
    #[test]
    fn production_of_a_fourth_mech_off_turn_announces_it_and_opens_synergy() {
        let mut state = game();
        give_breakthrough(&mut state);
        strip_mechs(&mut state);
        let content = ContentStore::embedded();
        let system = SystemId::new("18");
        let planet = PlanetId::new("mr");
        state.system_mut(&system).set_control(planet.clone(), a());
        crate::fixtures::put_on_planet(&mut state, &system, &planet, "naaz_mech", &a(), 3);
        state.player_mut(&a()).unwrap().trade_goods = 10;
        state.active = Some(b());
        assert_eq!(mechs(&state, &a()).len(), 3);
        assert!(crate::supply::staged_event_types(&state).is_empty());

        let mut dice = crate::dice::Dice::new();
        let mut rng = crate::rng::GameRng::new(0);
        let mut table = crate::choice::Table::with_default(Box::new(BuildMechDecider));
        let report = {
            let mut ctx = crate::choice::Resolving {
                content,
                sources: DEFAULT,
                dice: &mut dice,
                rng: &mut rng,
                table: &mut table,
                timing: None,
            };
            crate::production::produce_by_ability(
                &mut state,
                &mut ctx,
                None,
                &a(),
                &system,
                Some(1),
            )
            .expect("production resolves")
        };
        assert_eq!(report.produced.len(), 1, "one mech was produced");
        assert_eq!(mechs(&state, &a()).len(), 4, "the fourth mech stands");
        assert!(
            crate::supply::staged_event_types(&state)
                .iter()
                .any(|kind| kind == "NAAZ_MECH_PLACED"),
            "production announced the placement"
        );

        // The staged event is delivered by the real driver and opens the window.
        let mut game =
            crate::game::Game::new(state, ContentStore::embedded()).with_sources(DEFAULT);
        game.table = scripted(&["breakthrough:naaz:naazbt:NAAZ_MECH_PLACED:after"]);
        assert_eq!(game.step().error, None);
        assert_eq!(voltrons(&game.state, &a()), 1);
        assert_eq!(mechs(&game.state, &a()).len(), 0);
    }

    /// G2a: a space-area Maximum moves its printed 3 in a real tactical action (here two hexes
    /// through the hub centre).
    #[test]
    fn a_space_maximum_moves_into_the_activated_system_in_a_real_tactical_action() {
        let hub = crate::fixtures::plain_hub();
        let origin = SystemId::new(hub.outer[0].clone());
        let target = SystemId::new(hub.across(&hub.outer[0]));
        let mut state = game();
        state.phase = ti4_model::state::Phase::Action;
        state.active = Some(a());
        crate::fixtures::put(&mut state, &origin, VOLTRON, &a(), 1);
        let content = ContentStore::embedded();
        let types = ti4_content::units::catalogue(content, DEFAULT);
        assert_eq!(
            types[VOLTRON].move_value(),
            3,
            "the Eidolon Maximum moves 3"
        );
        let table = crate::choice::Table::with_default(Box::new(MoveEverything {
            target: target.to_string(),
        }));
        let mut game = crate::game::Game::with_table(state, content, table)
            .with_galaxy(hub.galaxy)
            .with_sources(DEFAULT);
        for _ in 0..4 {
            assert_eq!(game.step().error, None);
        }
        assert!(
            game.state
                .system_state(&target)
                .units
                .iter()
                .any(|unit| is_voltron(unit, &a())),
            "the Maximum arrived in the active system"
        );
        assert!(
            !game
                .state
                .system_state(&origin)
                .units
                .iter()
                .any(|unit| is_voltron(unit, &a())),
            "and left its origin"
        );
        assert_eq!(voltrons(&game.state, &a()), 1, "never duplicated");
    }

    /// Takes a tactical action on `target` and makes every move offered.
    #[derive(Debug)]
    struct MoveEverything {
        target: String,
    }
    impl crate::choice::Decider for MoveEverything {
        fn choose(
            &mut self,
            choice: &Choice,
        ) -> Result<ChoiceOption, crate::choice::IllegalChoice> {
            let ids = choice.ids();
            let wanted = if ids.contains(&crate::game::TACTICAL_ACTION_ID) {
                choice.option(crate::game::TACTICAL_ACTION_ID)
            } else if ids.contains(&self.target.as_str()) {
                choice.option(&self.target)
            } else if ids.contains(&"done_moving") {
                choice
                    .options
                    .iter()
                    .find(|option| option.id != "done_moving")
                    .or_else(|| choice.option("done_moving"))
            } else {
                choice.options.first()
            };
            wanted
                .cloned()
                .ok_or_else(|| crate::choice::IllegalChoice::NoOptions {
                    player: choice.player.clone(),
                    prompt: choice.prompt.clone(),
                })
        }
    }

    /// G2b: a Maximum in the active system's space area is committed to a planet in the real
    /// commit step, and stays one `naaz_voltron` (it is both a ship and a ground force).
    #[test]
    fn a_space_maximum_is_committed_to_a_planet_in_the_invasion_commit_step() {
        let mut state = game();
        let content = ContentStore::embedded();
        // Not Mecatol Rex: 27.1 keeps it closed to landings while the custodians token is on it.
        let (system, planet) = ti4_content::galaxy::all_planets(content, DEFAULT)
            .iter()
            .find(|(_, p)| {
                p.system_id().is_some()
                    && !p.is_placed_during_play()
                    && p.system_id() != Some(crate::seating::MECATOL)
            })
            .map(|(id, p)| (SystemId::new(p.system_id().unwrap()), PlanetId::new(*id)))
            .expect("the corpus has a placed planet outside Mecatol Rex");
        state.active = Some(a());
        state.active_system = Some(system.clone());
        crate::fixtures::put(&mut state, &system, VOLTRON, &a(), 1);
        let mut table = scripted(&[&format!("commit|0|{planet}"), "done_committing"]);
        let committed = crate::invasion::commit_ground_forces(
            &mut state,
            content,
            DEFAULT,
            &mut table,
            &a(),
            &system,
        )
        .unwrap();
        assert_eq!(committed, vec![planet.clone()]);
        let here = state.system_state(&system);
        let landed: Vec<&str> = here
            .on_planet_of(&planet, &a())
            .iter()
            .map(|unit| unit.type_id.as_str())
            .collect();
        assert_eq!(landed, [VOLTRON], "it lands as itself, not as a mech");
        assert!(here.units_of(&a()).is_empty(), "it left the space area");
        assert_eq!(voltrons(&state, &a()), 1);
    }

    /// G3: the card state is derived from the board, so a destroyed Maximum puts Absolute
    /// Synergy back on offer for any four mechs in one system.
    #[test]
    fn absolute_synergy_is_offered_again_after_the_maximum_is_destroyed() {
        let mut state = game();
        give_breakthrough(&mut state);
        let system = four_mechs_home(&mut state);
        emit(
            &mut state,
            &mut scripted(&[SYNERGY, "space"]),
            "ACTION_COMPLETED",
            &[("player", "a")],
        );
        assert_eq!(voltrons(&state, &a()), 1);

        // While the Maximum stands the card is flipped: four more mechs are not offered it.
        crate::fixtures::put(&mut state, &SystemId::new("19"), "naaz_mech_space", &a(), 4);
        let before = state.board.clone();
        emit(
            &mut state,
            &mut scripted(&[SYNERGY, "space"]),
            "ACTION_COMPLETED",
            &[("player", "a")],
        );
        assert_eq!(state.board, before, "flipped card is not offered");

        // Destroy it with ordinary combat hits (sustain declined).
        assign(
            &mut state,
            &system,
            10,
            crate::combat::HitOrigin::CombatRoll,
            false,
        );
        assert_eq!(voltrons(&state, &a()), 0, "the Maximum was destroyed");

        emit(
            &mut state,
            &mut scripted(&[SYNERGY, "space"]),
            "ACTION_COMPLETED",
            &[("player", "a")],
        );
        assert_eq!(voltrons(&state, &a()), 1, "offered and taken a second time");
        assert!(
            state
                .system_state(&SystemId::new("19"))
                .units
                .iter()
                .any(|unit| is_voltron(unit, &a()))
        );
    }

    #[test]
    fn a_nekro_flagship_with_the_naaz_z_token_gives_its_mechs_a_die() {
        let content = ContentStore::embedded();
        let system = SystemId::new("18");
        let dice = |lent: &[&str], kind: &str| {
            let mut state = crate::fixtures::nekro_with_z(&[("a", "nekro"), ("b", "sol")], lent);
            crate::fixtures::put(&mut state, &system, "nekro_flagship", &a(), 1);
            unit_dice(
                &state,
                content,
                DEFAULT,
                &CombatUnit {
                    player: &a(),
                    system: Some(&system),
                    planet: None,
                    unit_type: kind,
                    context: "space",
                },
                2,
            )
        };
        assert_eq!(dice(&[], "nekro_mech"), 2, "off by default");
        assert_eq!(dice(&["naaz"], "nekro_mech"), 3);
        assert_eq!(dice(&["naaz"], "nekro_flagship"), 2, "not a mech");
    }
}
