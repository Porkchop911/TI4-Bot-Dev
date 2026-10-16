//! The Vuil'raith Cabal (`cabal`). See `factions/mod.rs` for the contract and
//! `plans/BASE_FACTIONS_PLAN_2026-10-02.md` for scope; the per-item record is
//! `plans/evidence/BF-cabal.md`.
//!
//! Card texts (latest printing, `crates/ti4-content/content/*.json`):
//!
//! * Devour: "Capture your opponent's non-structure units that are destroyed during combat."
//! * Amalgamation: "When you produce a unit: You may return 1 captured unit of that type to
//!   produce that unit without spending resources."
//! * Riftmeld: "When you research a unit upgrade technology: You may return 1 captured unit of
//!   that type to ignore all of the technology's prerequisites."
//! * The Terror Between (`cabal_flagship`): "Capture all other non-structure units that are
//!   destroyed in this system, including your own."
//! * Reanimator (`cabal_mech`): "When your infantry on this planet are destroyed, place them on
//!   your faction sheet; those units are captured."
//! * Dimensional Tear I (`cabal_spacedock`): "This system is a gravity rift; your ships do not
//!   roll for this gravity rift. Place a dimensional tear token beneath this unit as a reminder.
//!   Up to 6 fighters in this system do not count against your ships' capacity."
//! * Dimensional Tear II (`cabal_spacedock2`, tech `dt2`): the same with up to 12 fighters and
//!   PRODUCTION 7.
//! * Vortex (`vtx`): "ACTION: Exhaust this card to choose another player's non-structure unit in
//!   a system that is adjacent to 1 or more of your space docks. Capture 1 unit of that type from
//!   that player's reinforcements."
//! * The Stillness of Stars (`cabalagent`): "After another player replenishes commodities: You
//!   may exhaust this card to convert their commodities to trade goods and capture 1 unit from
//!   their reinforcements that has a cost equal to or lower than their commodity value."
//! * That Which Molds Flesh (`cabalcommander`): "When you produce fighter or infantry units: Up
//!   to 2 of those units do not count against your PRODUCTION limit." Unlock: "Have units in 3
//!   gravity rifts."
//! * It Feeds on Carrion (`cabalhero`): "ACTION: Each other player rolls a die for each of his
//!   non-fighter ships that are in or adjacent to a system that contains a dimensional tear. On a
//!   1-3, capture that unit. If this causes a player's ground forces or fighters to be removed,
//!   also capture those units. Then, purge this card."
//! * Crucible (`crucible`): "After you activate a system: Your ships do not roll for gravity
//!   rifts during this movement, apply an additional +1 to the move values of your ships that
//!   would move out of or through a gravity rift instead. Then, return this card to the Vuil'raith
//!   player."
//! * Al'Raith Ix Ianovar (`cabalbt`): "This breakthrough causes The Fracture to enter play
//!   without a roll, if it is not already in play. After this card enters play, move up to 2
//!   ingress tokens into systems that contain gravity rifts. Apply +1 to the MOVE value of each of
//!   your ships that start their movement in The Fracture."
//!
//! # Capture
//!
//! The Cabal does not destroy what it kills: it keeps it. Capture is shared engine state
//! (`supply::capture_from_board`, `supply::capture_from_reinforcements`, `supply::return_captured`,
//! rows in `Player::captured_units` as `(owner, unit type)`), so this module only decides which
//! destructions and which reinforcements the Cabal takes. `MODULE` claims an asset only once every
//! clause of that card is implemented and tested.
//!
//! ## Reanimator
//!
//! Reanimator is a unit trigger, not a leader: the text is on the mech card. The module claims
//! `cabal_mech` and registers a stateful ability on the engine's `GROUND_FORCE_DESTROYED` window,
//! gated by a condition, exactly as Arborec registers `Mitosis` on the same window. The hit is
//! applied before that window is announced, so the infantry is already off the board and cannot be
//! taken with `supply::capture_from_board` (which refuses a captor that is also the owner): the row
//! is written onto the Cabal's own sheet, which is the same representation every other capture
//! uses and the one `supply::captured_by` and `supply::captured_of` read.

use std::collections::BTreeSet;
use std::sync::Arc;

use ti4_content::ContentStore;
use ti4_model::content_types::SourceSet;
use ti4_model::id::{LeaderId, PlanetId, PlayerId, SystemId, TechnologyId, UnitTypeId};
use ti4_model::state::GameState;

use super::hooks_economy::EconomyHooks;
use super::hooks_movement::MovementHooks;
use super::hooks_strategy::{ResearchWaiver, ResearchWaiverPayment, StrategyHooks};
use super::{FactionModule, Hooks};
use crate::choice::{Choice, ChoiceOption};
use crate::decision_context::{DecisionContext, DecisionSource};
use crate::timing::{Ability, Relation, TimingContext};

/// The faction alias, and the faction name used to gate every Cabal hook.
pub const FACTION: &str = "cabal";

/// The Cabal flagship, "The Terror Between".
const FLAGSHIP: &str = "cabal_flagship";

/// Prefix of the participation marks Devour reads. A mark names the combat, not the player, so a
/// second Cabal seat writes its own mark under the same key and overwrites it with its own owner.
const DEVOUR: &str = "cabal:devour";

/// What this faction implements: every asset whose clauses are implemented and tested.
pub const MODULE: FactionModule = FactionModule {
    alias: FACTION,
    abilities: &["amalgamation", "devour", "riftmeld"],
    technologies: &["dt2", "vtx"],
    units: &[
        "cabal_flagship",
        "cabal_mech",
        "cabal_spacedock",
        "cabal_spacedock2",
    ],
    promissory: &["crucible"],
    leaders: &["cabalagent", "cabalcommander", "cabalhero"],
    breakthroughs: &["cabalbt"],
    hooks: Hooks {
        timing_abilities: Some(timing_abilities),
        economy: EconomyHooks {
            production_unit_exchange: Some(amalgamation_offer),
            production_unit_exchange_performed: Some(amalgamation_perform),
            ..EconomyHooks::NONE
        },
        strategy: StrategyHooks {
            research_waiver_offer: Some(riftmeld_offer),
            research_waiver_paid: Some(riftmeld_paid),
            ..StrategyHooks::NONE
        },
        movement: MovementHooks {
            gravity_rifts: Some(tear_systems),
            rift_roll_exempt: Some(tear_exempts_roll),
            rift_crucible: Some(crucible_in_effect),
            move_bonus: Some(alraith_move_bonus),
            ..MovementHooks::NONE
        },
        commander_unlocked: Some(commander_unlocked),
        leader_action: Some(leader_action),
        use_leader: Some(use_leader),
        mapped_component_actions: Some(vortex_component_actions),
        perform_component: Some(vortex_perform),
        ..Hooks::NONE
    },
};

/// Timing abilities this faction claims.
///
/// Every ability is registered for every Cabal seat and gated by its own condition, so the engine
/// owns the window and the card owns the trigger.
fn timing_abilities(state: &GameState, owner_name: &str, seat: &PlayerId) -> Vec<Ability> {
    // A holder's card, so it is armed for every seat whatever its faction.
    // Only while a Cabal is seated: without one the note does not exist.
    let crucible = crucible(owner_name, seat);
    if !is_cabal(state, seat) {
        // A Nekro flagship may carry The Terror Between (Valefar Assimilator Z); the condition
        // decides.
        if super::nekro::is_nekro(state, seat) {
            return vec![
                crucible,
                terror_between_ships(owner_name, seat),
                terror_between_ground_forces(owner_name, seat),
            ];
        }
        return if crate::promissory::seat_of(state, FACTION).is_some() {
            vec![crucible]
        } else {
            Vec::new()
        };
    }
    let mut abilities = vec![
        crucible,
        reanimator(owner_name, seat),
        stillness_of_stars(owner_name, seat),
        alraith(owner_name, seat),
        devour_ships(owner_name, seat),
        devour_ground_forces(owner_name, seat),
        terror_between_ships(owner_name, seat),
        terror_between_ground_forces(owner_name, seat),
    ];
    abilities.extend(combat_participation(owner_name, seat));
    abilities
}

/// Whether this seat really plays the Cabal.
///
/// The resolver has no late registration, so a module's abilities are armed for every seat: the
/// condition is what limits them to the faction that owns the card.
fn is_cabal(state: &GameState, player: &PlayerId) -> bool {
    state
        .player(player)
        .is_some_and(|seat| seat.faction.as_str() == FACTION)
}

/// Reanimator (`units.json` `cabal_mech`): "When your infantry on this planet are destroyed, place
/// them on your faction sheet; those units are captured."
///
/// The condition is evaluated after the hit has been applied, which is what the card needs: a mech
/// that absorbed the hit is damaged and still in the area, so it still carries the trigger.
fn reanimator(owner_name: &str, seat: &PlayerId) -> Ability {
    let captor = seat.clone();
    let trigger_owner = seat.clone();
    Ability::stateful(
        format!("unit:{owner_name}:cabal_mech:GROUND_FORCE_DESTROYED:after"),
        seat.clone(),
        "GROUND_FORCE_DESTROYED",
        Relation::After,
        Arc::new(move |event, _resolver, context| {
            let Some(unit) = event.text("unit").map(UnitTypeId::new) else {
                return Ok(());
            };
            capture_unit(context.state, &captor, &captor, &unit);
            Ok(())
        }),
    )
    .with_stateful_condition(Arc::new(move |event, _resolver, context| {
        let (Some(player), Some(unit)) =
            (event_player(event), event.text("unit").map(UnitTypeId::new))
        else {
            return false;
        };
        let (Some(system), Some(planet)) = (
            event.text("system").map(SystemId::new),
            event.text("planet").map(PlanetId::new),
        ) else {
            return false;
        };
        player == trigger_owner
            && is_cabal(context.state, &trigger_owner)
            && base_type(context.content, context.sources, &unit) == "infantry"
            && reanimator_present(
                context.state,
                context.content,
                context.sources,
                &trigger_owner,
                &system,
                &planet,
            )
            // The flagship's clause takes this loss too, and one destroyed model is captured once.
            && !flagship_present(context.state, &trigger_owner, &system)
    }))
}

/// Devour: "Capture your opponent's non-structure units that are destroyed during combat."
///
/// Two decisions carry this implementation.
///
/// 1. The flagship wins the overlap. Its clause is the wider one — every non-structure loss in the
///    system, the Cabal's own included — so Devour yields to it wherever a flagship is present.
///    The partition is then exact: with a flagship in the system the flagship captures, without one
///    Devour captures, and no destroyed model is captured twice.
/// 2. Participation cannot be read from the board. `combat::destroy_units` removes every loss of
///    the round before any of them is announced, so a Cabal fleet that was wiped in this combat is
///    already gone when its own destruction is announced. `SPACE_COMBAT_STARTED` records the two
///    sides instead, and that mark is visible to every later announcement of the same flush.
///
/// "During combat" follows the reading the engine already applies to the same phrase on Yin's
/// Brother Milor (`crate::factions::yin`): a space loss caused by the space combat itself, or a
/// ground loss caused by the ground combat. Bombardment, space cannon defense and Harrow are not
/// combat, so they do not feed Devour.
fn devour_ships(owner_name: &str, seat: &PlayerId) -> Ability {
    let captor = seat.clone();
    let claimant = seat.clone();
    Ability::stateful(
        format!("ability:{owner_name}:devour:SHIP_DESTROYED:after"),
        seat.clone(),
        "SHIP_DESTROYED",
        Relation::After,
        Arc::new(move |event, _resolver, context| {
            let Some((victim, unit)) =
                destroyed_model(context.content, context.sources, event, &captor, false)
            else {
                return Ok(());
            };
            capture_unit(context.state, &captor, &victim, &unit);
            Ok(())
        }),
    )
    .with_stateful_condition(Arc::new(move |event, _resolver, context| {
        let Some(system) = event.text("system").map(SystemId::new) else {
            return false;
        };
        crate::combat::destroyed_during_combat(event)
            && mark_held(context.state, &space_mark(&system), &claimant)
            && !flagship_present(context.state, &claimant, &system)
    }))
}

/// Devour, ground half. A ground combat is per planet, so its mark is keyed by the planet too.
fn devour_ground_forces(owner_name: &str, seat: &PlayerId) -> Ability {
    let captor = seat.clone();
    let claimant = seat.clone();
    Ability::stateful(
        format!("ability:{owner_name}:devour:GROUND_FORCE_DESTROYED:after"),
        seat.clone(),
        "GROUND_FORCE_DESTROYED",
        Relation::After,
        Arc::new(move |event, _resolver, context| {
            let Some((victim, unit)) =
                destroyed_model(context.content, context.sources, event, &captor, false)
            else {
                return Ok(());
            };
            capture_unit(context.state, &captor, &victim, &unit);
            Ok(())
        }),
    )
    .with_stateful_condition(Arc::new(move |event, _resolver, context| {
        let (Some(system), Some(planet)) = (
            event.text("system").map(SystemId::new),
            event.text("planet").map(PlanetId::new),
        ) else {
            return false;
        };
        event.text("cause") == Some("ground_combat")
            && mark_held(context.state, &ground_mark(&system, &planet), &claimant)
            && !flagship_present(context.state, &claimant, &system)
    }))
}

/// The Terror Between: "Capture all other non-structure units that are destroyed in this system,
/// including your own."
///
/// This is not a combat ability: it takes any non-structure loss in the system the flagship is in,
/// whatever destroyed it, and it does not require the Cabal to be taking part. It is written as two
/// abilities because one `Ability` carries one event type — ships are announced as `SHIP_DESTROYED`
/// and ground forces as `GROUND_FORCE_DESTROYED`.
///
/// "All other" is enforced by the flagship itself rather than by comparing unit ids: the destruction
/// stage has already taken the losses off the board before any announcement, so a flagship that was
/// destroyed in this flush captures nothing, while a second flagship that survived still does.
fn terror_between_ships(owner_name: &str, seat: &PlayerId) -> Ability {
    let captor = seat.clone();
    let claimant = seat.clone();
    Ability::stateful(
        format!("unit:{owner_name}:{FLAGSHIP}:SHIP_DESTROYED:after"),
        seat.clone(),
        "SHIP_DESTROYED",
        Relation::After,
        Arc::new(move |event, _resolver, context| {
            let Some((victim, unit)) =
                destroyed_model(context.content, context.sources, event, &captor, true)
            else {
                return Ok(());
            };
            capture_unit(context.state, &captor, &victim, &unit);
            Ok(())
        }),
    )
    .with_stateful_condition(Arc::new(move |event, _resolver, context| {
        let Some(system) = event.text("system").map(SystemId::new) else {
            return false;
        };
        flagship_present(context.state, &claimant, &system)
    }))
}

/// The Terror Between, ground half.
fn terror_between_ground_forces(owner_name: &str, seat: &PlayerId) -> Ability {
    let captor = seat.clone();
    let claimant = seat.clone();
    Ability::stateful(
        format!("unit:{owner_name}:{FLAGSHIP}:GROUND_FORCE_DESTROYED:after"),
        seat.clone(),
        "GROUND_FORCE_DESTROYED",
        Relation::After,
        Arc::new(move |event, _resolver, context| {
            let Some((victim, unit)) =
                destroyed_model(context.content, context.sources, event, &captor, true)
            else {
                return Ok(());
            };
            capture_unit(context.state, &captor, &victim, &unit);
            Ok(())
        }),
    )
    .with_stateful_condition(Arc::new(move |event, _resolver, context| {
        let Some(system) = event.text("system").map(SystemId::new) else {
            return false;
        };
        flagship_present(context.state, &claimant, &system)
    }))
}

/// Record a captured model on the Cabal's sheet.
///
/// The row is `(victim, unit type)`, which is what takes the model out of the victim's
/// reinforcements: `supply::held` counts a captured row against the player named in the row, not
/// against the captor that holds it.
fn capture_unit(state: &mut GameState, captor: &PlayerId, victim: &PlayerId, unit: &UnitTypeId) {
    if let Some(seat) = state.player_mut(captor) {
        seat.captured_units.push((victim.clone(), unit.clone()));
    }
}

/// The destroyed model a Cabal capture effect may take: a unit the catalogue knows, that is not a
/// structure, and that is not a neutral's. `own` decides whether the Cabal's own models count —
/// Devour takes an opponent's units, the flagship takes everything in its system including its own.
fn destroyed_model(
    content: &ContentStore,
    sources: SourceSet,
    event: &crate::event::Event,
    captor: &PlayerId,
    own: bool,
) -> Option<(PlayerId, UnitTypeId)> {
    let victim = event_player(event)?;
    if crate::neutral_units::is_neutral(&victim) || (!own && &victim == captor) {
        return None;
    }
    let unit = event.text("unit").map(UnitTypeId::new)?;
    // A unit the catalogue has no entry for has no type to capture, so nothing is taken.
    let kind = ti4_content::units::unit_type(content, unit.as_str(), sources)?;
    if kind.is_structure() {
        return None;
    }
    Some((victim, unit))
}

/// True when one of the Cabal's flagships is in that system.
fn flagship_present(state: &GameState, owner: &PlayerId, system: &SystemId) -> bool {
    super::has_flagship_text_in(state, owner, system, FLAGSHIP)
}

/// The participation marks Devour reads. A space combat names its attacker and its defender, and so
/// does a ground combat, so the sides are recorded when the combat starts and cleared when it ends.
/// A mark is written for each side, and it is written before any destruction of that combat is
/// announced, which is what makes it readable after a whole fleet has been removed from the board.
fn combat_participation(owner_name: &str, seat: &PlayerId) -> Vec<Ability> {
    vec![
        participation(owner_name, seat, "SPACE_COMBAT_STARTED", false, false),
        participation(owner_name, seat, "SPACE_COMBAT_ENDED", false, true),
        participation(owner_name, seat, "GROUND_COMBAT_STARTED", true, false),
        participation(owner_name, seat, "GROUND_COMBAT_ENDED", true, true),
    ]
}

/// One half of one participation mark: `ground` selects the key, `clear` selects write or remove.
fn participation(
    owner_name: &str,
    seat: &PlayerId,
    event_name: &str,
    ground: bool,
    clear: bool,
) -> Ability {
    let holder = seat.clone();
    let claimant = seat.clone();
    let action = if clear { "clear" } else { "mark" };
    Ability::stateful(
        format!("ability:{owner_name}:devour_{action}:{event_name}:after"),
        seat.clone(),
        event_name,
        Relation::After,
        Arc::new(move |event, _resolver, context| {
            let Some(mark) = event_mark(event, ground) else {
                return Ok(());
            };
            if clear {
                if mark_held(context.state, &mark, &holder) {
                    context.state.faction_marks.remove(&mark);
                }
            } else {
                context
                    .state
                    .faction_marks
                    .insert(mark, holder.as_str().to_owned());
            }
            Ok(())
        }),
    )
    .with_stateful_condition(Arc::new(move |event, _resolver, _context| {
        event_mark(event, ground).is_some() && took_part(event, &claimant)
    }))
}

/// The mark of the combat an event belongs to: a space combat is keyed by its system, a ground
/// combat by its system and its planet.
fn event_mark(event: &crate::event::Event, ground: bool) -> Option<String> {
    let system = event.text("system").map(SystemId::new)?;
    if ground {
        let planet = event.text("planet").map(PlanetId::new)?;
        return Some(ground_mark(&system, &planet));
    }
    Some(space_mark(&system))
}

fn space_mark(system: &SystemId) -> String {
    format!("{DEVOUR}:{system}")
}

fn ground_mark(system: &SystemId, planet: &PlanetId) -> String {
    format!("{DEVOUR}:{system}|{planet}")
}

/// True when this player holds that mark.
fn mark_held(state: &GameState, mark: &str, player: &PlayerId) -> bool {
    state
        .faction_marks
        .get(mark)
        .is_some_and(|holder| holder == player.as_str())
}

/// Whether a combat event names this player as one of the two sides.
fn took_part(event: &crate::event::Event, player: &PlayerId) -> bool {
    event.text("attacker") == Some(player.as_str())
        || event.text("defender") == Some(player.as_str())
}

/// The player an event names, if any.
fn event_player(event: &crate::event::Event) -> Option<PlayerId> {
    event.text("player").map(PlayerId::new)
}

/// The catalogue base type of a unit id, falling back to the id itself.
fn base_type(content: &ContentStore, sources: SourceSet, unit: &UnitTypeId) -> String {
    crate::supply::base_type_of(content, sources, unit)
}

/// True when one of the Cabal's mechs is on that planet.
fn reanimator_present(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    owner: &PlayerId,
    system: &SystemId,
    planet: &PlanetId,
) -> bool {
    state
        .system_state(system)
        .on_planet_of(planet, owner)
        .into_iter()
        .any(|unit| base_type(content, sources, &unit.type_id) == "mech")
}

/// # Amalgamation
///
/// "When you produce a unit: You may return 1 captured unit of that type to produce that unit
/// without spending resources."
///
/// The exchange is a second way to pay for one unit of one type, so it is offered as its own option
/// inside the production window beside the paid build, and it is settled before the affordability
/// test: a unit bought by returning a captured model is not bought with resources. Everything else
/// about the use is unchanged — the unit is placed by the same placement stage, so it still spends
/// this use's PRODUCTION capacity, still respects plastic, and still respects the destination rules.
///
/// Matching is by base type, the same identity every other capture rule uses: returning a captured
/// Cruiser returns a `cruiser` model, whichever seat it was taken from.

/// The seat a captured model of that base type belongs to, if the Cabal holds one.
///
/// Capture rows live on the captor's sheet and name the original owner, so the Cabal's own sheet is
/// the list to search, and the row's owner is the seat the model goes back to.
fn captured_owner_of(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    captor: &PlayerId,
    base: &str,
) -> Option<PlayerId> {
    crate::supply::captured_by(state, captor)
        .iter()
        .find(|(_, unit)| base_type(content, sources, unit) == base)
        .map(|(owner, _)| owner.clone())
}

/// Whether the Cabal may buy this unit by returning a captured model of it.
///
/// Pure: asks nothing and mutates nothing.
fn amalgamation_offer(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    _system: &SystemId,
    unit: &str,
) -> bool {
    if !is_cabal(state, player) {
        return false;
    }
    let base = base_type(content, sources, &UnitTypeId::new(unit));
    captured_owner_of(state, content, sources, player, &base).is_some()
}

/// Perform the exchange: return one captured model of this unit's base type to its owner.
///
/// Atomic: re-checks everything the offer relied on and leaves the state untouched when it cannot
/// happen. Removing the row is the return — a captured unit is off the board and on the captor's
/// sheet, so it reappears in its owner's reinforcements.
fn amalgamation_perform(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    _system: &SystemId,
    unit: &str,
) -> bool {
    if !is_cabal(state, player) {
        return false;
    }
    let base = base_type(content, sources, &UnitTypeId::new(unit));
    let Some(owner) = captured_owner_of(state, content, sources, player, &base) else {
        return false;
    };
    crate::supply::return_captured(state, content, sources, player, &owner, &base).is_some()
}

// -- Riftmeld ------------------------------------------------------------------------------------

/// The unit type a unit-upgrade technology replaces, as a base type, if `tech` is one.
///
/// The corpus keeps both halves of that mapping on the UNIT record: the upgraded type names the
/// technology in `requiredTechId` and the type it replaces in `upgradesFromUnitId`. A technology
/// that no such pair names upgrades no unit, so there is no captured model to return for it.
fn unit_upgrade_base(
    content: &ContentStore,
    sources: SourceSet,
    tech: &TechnologyId,
) -> Option<String> {
    ti4_content::units::catalogue(content, sources)
        .values()
        .find(|kind| {
            kind.required_technology() == Some(tech.as_str()) && kind.upgrades_from().is_some()
        })
        .map(|kind| kind.base_type().to_owned())
}

/// Every captured model the Cabal could return for `tech`, in capture order.
///
/// Each row is a separate player choice, the way every other optional payment is: the card names
/// one captured unit, and the row says whose model it is.
fn riftmeld_payments(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    tech: &TechnologyId,
) -> Vec<(ResearchWaiverPayment, PlayerId, UnitTypeId)> {
    let Some(base) = unit_upgrade_base(content, sources, tech) else {
        return Vec::new();
    };
    crate::supply::captured_by(state, player)
        .iter()
        .filter(|(_, unit)| base_type(content, sources, unit) == base)
        .map(|(owner, unit)| {
            (
                ResearchWaiverPayment {
                    id: format!("captured|{owner}|{unit}"),
                    label: format!("return 1 captured {base} taken from {owner}"),
                },
                owner.clone(),
                unit.clone(),
            )
        })
        .collect()
}

/// Whether the Cabal may ignore this technology's prerequisites by returning a captured unit of the
/// type it upgrades.
///
/// Pure: asks nothing and mutates nothing. The engine offers the waiver only when the research is
/// otherwise impossible, and the player may decline it.
fn riftmeld_offer(
    state: &GameState,
    content: &ContentStore,
    player: &PlayerId,
    tech: &TechnologyId,
) -> Option<ResearchWaiver> {
    if !is_cabal(state, player) {
        return None;
    }
    let payments = riftmeld_payments(
        state,
        content,
        ti4_model::content_types::DEFAULT,
        player,
        tech,
    );
    (!payments.is_empty()).then(|| ResearchWaiver {
        id: "riftmeld".to_owned(),
        label: "return 1 captured unit of that type to ignore all of its prerequisites".to_owned(),
        payments: payments
            .into_iter()
            .map(|(payment, _, _)| payment)
            .collect(),
    })
}

/// Pay the selected captured model. The target is recomputed and revalidated before any mutation,
/// so a stale choice researches nothing.
fn riftmeld_paid(
    state: &mut GameState,
    content: &ContentStore,
    player: &PlayerId,
    tech: &TechnologyId,
    payment: &str,
) -> bool {
    if !is_cabal(state, player) {
        return false;
    }
    let sources = ti4_model::content_types::DEFAULT;
    let Some((_, owner, _)) = riftmeld_payments(state, content, sources, player, tech)
        .into_iter()
        .find(|(choice, _, _)| choice.id == payment)
    else {
        return false;
    };
    let Some(base) = unit_upgrade_base(content, sources, tech) else {
        return false;
    };
    crate::supply::return_captured(state, content, sources, player, &owner, &base).is_some()
}

// -- Dimensional Tear and the commander's unlock ---------------------------------------------------

/// "This system is a gravity rift; your ships do not roll for this gravity rift. Place a
/// dimensional tear token beneath this unit as a reminder."
///
/// The token is only a reminder, so the tear is the dock itself: it is derived from the board on
/// every call, follows the unit, and is gone the moment the dock is destroyed. The fighter clause
/// ("up to 6 / 12 fighters in this system do not count against your ships' capacity") and the
/// PRODUCTION values are printed on the unit and read from the catalogue by `fleet::standing` and
/// `production::capacity`; the research upgrade `dt2` is the generic unit upgrade (90.8).
const TEAR_DOCKS: [&str; 2] = ["cabal_spacedock", "cabal_spacedock2"];

/// Whether this unit is a Cabal space dock, which carries a dimensional tear.
fn is_tear(state: &GameState, unit: &ti4_model::units::Unit) -> bool {
    TEAR_DOCKS.contains(&unit.type_id.as_str()) && is_cabal(state, &unit.owner)
}

/// Whether `owner` has a dimensional tear in this system.
fn owns_tear(state: &GameState, owner: &PlayerId, system: &SystemId) -> bool {
    state.board.get(system).is_some_and(|seat| {
        seat.units
            .iter()
            .chain(seat.planet_units.values().flatten())
            .any(|unit| &unit.owner == owner && is_tear(state, unit))
    })
}

/// Every system that holds a dimensional tear, for any player: the system is a gravity rift for
/// everyone.
fn tear_systems(state: &GameState) -> Vec<String> {
    // Asked per ship in every legal-move evaluation: a game with no Cabal seat has no tear, so
    // do not scan the board for one.
    if !state
        .players
        .iter()
        .any(|seat| seat.faction.as_str() == FACTION)
    {
        return Vec::new();
    }
    state
        .board
        .iter()
        .filter(|(_, seat)| {
            seat.units
                .iter()
                .chain(seat.planet_units.values().flatten())
                .any(|unit| is_tear(state, unit))
        })
        .map(|(system, _)| system.as_str().to_owned())
        .collect()
}

/// "Your ships do not roll for this gravity rift": the owner of the tear in `system`.
fn tear_exempts_roll(state: &GameState, mover: &PlayerId, system: &str) -> bool {
    owns_tear(state, mover, &SystemId::new(system))
}

/// Whether this system is a gravity rift: printed on the tile, or made one by a dimensional tear.
fn is_gravity_rift_system(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    system: &SystemId,
) -> bool {
    ti4_content::galaxy::system(content, system.as_str(), sources)
        .is_some_and(|record| record.is_gravity_rift())
        || state.board.get(system).is_some_and(|seat| {
            seat.units
                .iter()
                .chain(seat.planet_units.values().flatten())
                .any(|unit| is_tear(state, unit))
        })
}

/// That Which Molds Flesh. Unlock: "Have units in 3 gravity rifts."
///
/// Counts systems, not units: three rifts each holding any unit of the player's, a ship or a
/// ground force or a structure, printed rifts and dimensional tears alike.
fn commander_unlocked(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    _galaxy: Option<&ti4_content::galaxy::Galaxy>,
    player: &PlayerId,
    leader: &LeaderId,
) -> Option<bool> {
    if leader.as_str() != "cabalcommander" {
        return None;
    }
    let rifts = state
        .board
        .iter()
        .filter(|(system, seat)| {
            seat.units
                .iter()
                .chain(seat.planet_units.values().flatten())
                .any(|unit| &unit.owner == player)
                && is_gravity_rift_system(state, content, sources, system)
        })
        .count();
    Some(rifts >= 3)
}

// -- It Feeds on Carrion ---------------------------------------------------------------------------

/// The hero's leader id.
const HERO: &str = "cabalhero";

/// On a 1-3 the unit is captured; the die is recorded with the face that would miss.
const CARRION_CAPTURES_ON: u32 = 3;

/// Whether the Cabal could play the hero now: it is the Cabal's, and a dimensional tear is on the
/// board. The ships it would roll for depend on the map, which this hook is not given; the card is
/// still playable with none to roll for, so a tear is what makes it a legal action.
fn leader_action(
    state: &GameState,
    _content: &ContentStore,
    player: &PlayerId,
    leader: &LeaderId,
) -> Option<bool> {
    if leader.as_str() != HERO {
        return None;
    }
    Some(is_cabal(state, player) && !tear_systems(state).is_empty())
}

/// Every ship the hero rolls for, in a fixed order: each other player's non-fighter ship in or
/// adjacent to a system that contains a dimensional tear. Adjacency needs the map; without one only
/// the tear's own system is reached.
fn carrion_targets(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: Option<&ti4_content::galaxy::Galaxy>,
    player: &PlayerId,
) -> Vec<(SystemId, ti4_model::units::Unit)> {
    let mut reach: BTreeSet<String> = BTreeSet::new();
    for tear in tear_systems(state) {
        if let Some(galaxy) = galaxy {
            reach.extend(galaxy.adjacent(&tear).into_iter().map(ToOwned::to_owned));
        }
        reach.insert(tear);
    }
    let types = ti4_content::units::catalogue(content, sources);
    let mut targets = Vec::new();
    for system in reach {
        let id = SystemId::new(system);
        let Some(seat) = state.board.get(&id) else {
            continue;
        };
        for unit in &seat.units {
            if &unit.owner == player {
                continue;
            }
            let ship = types
                .get(unit.type_id.as_str())
                .is_some_and(|kind| kind.is_ship() && !kind.is_fighter());
            if ship {
                targets.push((id.clone(), unit.clone()));
            }
        }
    }
    targets
}

/// The fighters and ground forces a player has in a system's space area, as a sorted list.
fn cargo_in_space(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    owner: &PlayerId,
    system: &SystemId,
) -> Vec<UnitTypeId> {
    let types = ti4_content::units::catalogue(content, sources);
    let mut held: Vec<UnitTypeId> = state
        .system_state(system)
        .units_of(owner)
        .into_iter()
        .filter(|unit| {
            types
                .get(unit.type_id.as_str())
                .is_some_and(|kind| kind.is_fighter() || kind.is_ground_force())
        })
        .map(|unit| unit.type_id.clone())
        .collect();
    held.sort();
    held
}

/// It Feeds on Carrion: "ACTION: Each other player rolls a die for each of his non-fighter ships
/// that are in or adjacent to a system that contains a dimensional tear. On a 1-3, capture that
/// unit. If this causes a player's ground forces or fighters to be removed, also capture those
/// units. Then, purge this card."
///
/// The purge is the shared hero rule (`leaders::use_leader` purges a hero whose effect resolved).
/// Capacity is the engine's own: once the captured ships are off the board, `fleet::enforce_seeing`
/// asks each affected owner which of the fighters and ground forces that no longer fit to remove,
/// and what it removes is captured instead of returned to the reinforcements. The whole card is
/// atomic: a refused answer restores the state, the dice and the random stream.
fn use_leader(
    context: &mut TimingContext<'_>,
    player: &PlayerId,
    leader: &LeaderId,
) -> Option<bool> {
    if leader.as_str() != HERO {
        return None;
    }
    // "In or adjacent to" needs the map: without one the hero cannot say which ships it reaches,
    // and silently reaching only the tear's own system would be a different card.
    if !is_cabal(context.state, player)
        || tear_systems(context.state).is_empty()
        || context.galaxy.is_none()
    {
        return Some(false);
    }
    let before_state = context.state.clone();
    let before_dice = context.dice.clone();
    let before_rng = context.rng.clone();
    let before_log = context.table.log.clone();
    let done = carrion(context, player);
    if done.is_err() {
        *context.state = before_state;
        *context.dice = before_dice;
        *context.rng = before_rng;
        context.table.log = before_log;
        return Some(false);
    }
    Some(true)
}

fn carrion(
    context: &mut TimingContext<'_>,
    player: &PlayerId,
) -> Result<(), crate::choice::IllegalChoice> {
    let targets = carrion_targets(
        context.state,
        context.content,
        context.sources,
        context.galaxy,
        player,
    );
    let mut hit: Vec<(SystemId, ti4_model::units::Unit)> = Vec::new();
    for (system, unit) in targets {
        let roll = context.dice.roll_by(
            context.rng,
            1,
            HERO,
            Some(CARRION_CAPTURES_ON + 1),
            &unit.owner,
        );
        if roll
            .faces
            .first()
            .is_some_and(|face| *face <= CARRION_CAPTURES_ON)
        {
            hit.push((system, unit));
        }
    }
    let mut affected: BTreeSet<(PlayerId, SystemId)> = BTreeSet::new();
    for (system, unit) in &hit {
        if crate::supply::capture_from_board(context.state, player, system, None, unit) {
            affected.insert((unit.owner.clone(), system.clone()));
        }
    }
    // What the captured ships were carrying and no longer has room: removed by the capacity rule,
    // so captured here.
    for (owner, system) in affected {
        let before = cargo_in_space(
            context.state,
            context.content,
            context.sources,
            &owner,
            &system,
        );
        crate::fleet::enforce_seeing(
            context.state,
            context.content,
            context.sources,
            context.galaxy,
            context.table,
            &owner,
            &system,
        )?;
        let mut after = cargo_in_space(
            context.state,
            context.content,
            context.sources,
            &owner,
            &system,
        );
        for unit in before {
            if let Some(index) = after.iter().position(|kept| kept == &unit) {
                after.remove(index);
            } else {
                capture_unit(context.state, player, &owner, &unit);
            }
        }
    }
    Ok(())
}

// -- The Stillness of Stars ------------------------------------------------------------------------

/// The agent's leader id.
const AGENT: &str = "cabalagent";

/// The typed event a replenish announces. Staged by [`note_replenished`] from the one function every
/// replenish site already calls, and announced by the driver's usual staged-event flush.
const REPLENISHED: &str = "COMMODITIES_REPLENISHED";

/// Stage `COMMODITIES_REPLENISHED` (`player`, `commodities`) for a seat that has just had its
/// commodities replenished. Staged only while a Cabal is seated: nothing else listens, so every
/// other game keeps its event stream exactly.
pub(crate) fn note_replenished(state: &mut GameState, player: &PlayerId) {
    if !state
        .players
        .iter()
        .any(|seat| seat.faction.as_str() == FACTION)
    {
        return;
    }
    let value = state.player(player).map_or(0, |seat| seat.commodities);
    if value <= 0 {
        return;
    }
    crate::supply::stage_event(
        state,
        REPLENISHED,
        &std::collections::BTreeMap::from([
            ("player".to_owned(), player.to_string().into()),
            ("commodities".to_owned(), i64::from(value).into()),
        ]),
    );
}

fn agent_ready(state: &GameState, owner: &PlayerId) -> bool {
    crate::leaders::status(state, owner, &LeaderId::new(AGENT))
        == Some(ti4_model::state::LeaderStatus::Readied)
}

/// The form of a unit type `owner` places today: an upgrade it holds, else its faction's own, else
/// the generic one.
fn current_form(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    owner: &PlayerId,
    base: &str,
) -> Option<UnitTypeId> {
    let seat = state.player(owner)?;
    let faction = seat.faction.to_string();
    let held: Vec<String> = seat
        .technologies
        .iter()
        .map(|tech| tech.as_str().to_owned())
        .collect();
    let kind = ti4_content::units::unlocked_upgrade(content, sources, base, &faction, &held)
        .or_else(|| ti4_content::units::faction_unit(content, &faction, base, sources))
        .or_else(|| ti4_content::units::unit_type(content, base, sources))?;
    Some(UnitTypeId::new(kind.id()))
}

/// The unit types the agent could take from `owner`'s reinforcements: the models its sheet still has
/// in the box whose cost is no more than `value`, each by the form `owner` places today.
fn stillness_targets(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    owner: &PlayerId,
    value: i64,
) -> Vec<UnitTypeId> {
    const BASES: [&str; 11] = [
        "carrier",
        "cruiser",
        "destroyer",
        "dreadnought",
        "fighter",
        "flagship",
        "infantry",
        "mech",
        "pds",
        "spacedock",
        "warsun",
    ];
    let mut found = Vec::new();
    for base in BASES {
        let Some(form) = current_form(state, content, sources, owner, base) else {
            continue;
        };
        let Some(kind) = ti4_content::units::unit_type(content, form.as_str(), sources) else {
            continue;
        };
        // A structure has no printed cost, so it is not "a unit that has a cost".
        #[allow(
            clippy::cast_precision_loss,
            reason = "a commodity value is a small count"
        )]
        if kind.cost() <= 0.0 || kind.cost() > value as f64 {
            continue;
        }
        if crate::supply::remaining(state, content, sources, owner, &form) == 0 {
            continue;
        }
        found.push(form);
    }
    found
}

/// The Stillness of Stars: "After another player replenishes commodities: You may exhaust this card
/// to convert their commodities to trade goods and capture 1 unit from their reinforcements that has
/// a cost equal to or lower than their commodity value."
///
/// Both halves happen. The Cabal player picks the type when several qualify, and that question is
/// asked before anything changes, so a refused answer leaves the table as it was. "Their commodity
/// value" is the amount the seat was replenished to (the event's `commodities`), the number printed
/// on its faction sheet; the trade goods are whatever commodities it holds when the card resolves.
fn stillness_of_stars(owner_name: &str, seat: &PlayerId) -> Ability {
    let owner = seat.clone();
    let claimant = seat.clone();
    Ability::stateful(
        format!("leader:{owner_name}:{AGENT}:{REPLENISHED}:after"),
        seat.clone(),
        REPLENISHED,
        Relation::After,
        Arc::new(move |event, _resolver, context| {
            let (Some(victim), Some(value)) = (event_player(event), event.integer("commodities"))
            else {
                return Ok(());
            };
            if victim == owner || !agent_ready(context.state, &owner) {
                return Ok(());
            }
            let targets = stillness_targets(
                context.state,
                context.content,
                context.sources,
                &victim,
                value,
            );
            let taken = match targets.as_slice() {
                [] => None,
                [only] => Some(only.clone()),
                _ => {
                    let options: Vec<ChoiceOption> = targets
                        .iter()
                        .map(|unit| {
                            ChoiceOption::labelled(
                                unit.to_string(),
                                "stillness",
                                format!("capture a {unit} from {victim}'s reinforcements"),
                            )
                        })
                        .collect();
                    let choice = Choice::new(
                        owner.clone(),
                        "The Stillness of Stars: which unit do you capture".to_owned(),
                        options,
                    )
                    .contextualized(decision(
                        context.state,
                        &owner,
                        AGENT,
                        "stillness_target",
                    ));
                    let answer = context
                        .ask_seeing(&choice)
                        .map_err(crate::timing::TimingError::IllegalChoice)?;
                    targets
                        .iter()
                        .find(|unit| unit.as_str() == answer.id)
                        .cloned()
                }
            };
            crate::leaders::exhaust(context.state, &owner, &LeaderId::new(AGENT));
            let goods = context
                .state
                .player(&victim)
                .map_or(0, |held| held.commodities);
            if goods > 0 {
                if let Some(held) = context.state.player_mut(&victim) {
                    held.commodities = 0;
                }
                crate::supply::gain_trade_goods_staged(context.state, &victim, goods, AGENT);
            }
            if let Some(unit) = taken {
                crate::supply::capture_from_reinforcements(
                    context.state,
                    context.content,
                    context.sources,
                    &owner,
                    &victim,
                    &unit,
                );
            }
            Ok(())
        }),
    )
    .with_optional(true)
    .with_stateful_condition(Arc::new(move |event, _, context| {
        event_player(event).is_some_and(|victim| victim != claimant)
            && agent_ready(context.state, &claimant)
            && is_cabal(context.state, &claimant)
    }))
}

// -- Crucible ---------------------------------------------------------------------------------------

/// The promissory note's printed alias, and the note id of the Cabal's copy.
const CRUCIBLE: &str = "crucible";
const CRUCIBLE_NOTE: &str = "crucible:cabal";

/// The mark that scopes the card to one activation: `cabal:crucible:<player>` holds the activation
/// number it was played in, so it cannot leak into the holder's next tactical action.
fn crucible_mark(player: &PlayerId) -> String {
    format!("cabal:crucible:{player}")
}

/// Whether `holder` has the Cabal's Crucible in hand and could play it: it holds the note, it is not
/// the Cabal's own seat (the note is the Cabal's to lend, "return this card to the Vuil'raith
/// player"), and the note has not already been put in a play area.
fn crucible_playable(state: &GameState, holder: &PlayerId) -> bool {
    state.promissory_notes.get(CRUCIBLE_NOTE) == Some(holder)
        && !state.promissory_faceup.contains(CRUCIBLE_NOTE)
        && crate::promissory::seat_of(state, FACTION).is_some_and(|owner| &owner != holder)
}

/// Whether the mark says Crucible was played in the activation now under way.
fn crucible_active(state: &GameState, player: &PlayerId) -> bool {
    state
        .faction_marks
        .get(&crucible_mark(player))
        .is_some_and(|seq| *seq == state.activation_seq.to_string())
}

/// Crucible: "After you activate a system: Your ships do not roll for gravity rifts during this
/// movement, apply an additional +1 to the move values of your ships that would move out of or
/// through a gravity rift instead. Then, return this card to the Vuil'raith player."
///
/// A holder's card, so it is registered for every seat and gated by the note it holds. Playing it
/// records the activation it covers and sends the note home; the movement rules read the record
/// through `MovementHooks::rift_crucible` (`rifts_ignored` and one extra step per route).
fn crucible(owner_name: &str, seat: &PlayerId) -> Ability {
    let holder = seat.clone();
    let claimant = seat.clone();
    Ability::stateful(
        format!("promissory:{owner_name}:{CRUCIBLE}:SYSTEM_ACTIVATED:after"),
        seat.clone(),
        "SYSTEM_ACTIVATED",
        Relation::After,
        Arc::new(move |event, _resolver, context| {
            if event_player(event).as_ref() != Some(&holder)
                || !crucible_playable(context.state, &holder)
            {
                return Ok(());
            }
            let seq = context.state.activation_seq.to_string();
            context
                .state
                .faction_marks
                .insert(crucible_mark(&holder), seq);
            crate::promissory::give_back(context.state, CRUCIBLE_NOTE);
            Ok(())
        }),
    )
    .with_optional(true)
    .with_stateful_condition(Arc::new(move |event, _, context| {
        event_player(event).as_ref() == Some(&claimant)
            && crucible_playable(context.state, &claimant)
    }))
}

/// Movement hook: the mover played Crucible for this activation.
fn crucible_in_effect(state: &GameState, mover: &PlayerId) -> bool {
    crucible_active(state, mover)
}

// -- Al'Raith Ix Ianovar ----------------------------------------------------------------------------

const BREAKTHROUGH: &str = "cabalbt";

/// How many ingress tokens the breakthrough may move.
const INGRESS_MOVES: usize = 2;

/// Al'Raith Ix Ianovar: "This breakthrough causes The Fracture to enter play without a roll, if it is
/// not already in play. After this card enters play, move up to 2 ingress tokens into systems that
/// contain gravity rifts. Apply +1 to the MOVE value of each of your ships that start their movement
/// in The Fracture."
///
/// The entry is `fracture::after_breakthrough_gained`, which skips the roll for this card and places
/// the synergy ingress tokens as it does for any breakthrough; the game's own gain path calls it and
/// this ability calls it again only when the card arrived by a route that did not (the call is a
/// no-op once the Fracture is in play, so the entry never happens twice). The moves are the player's
/// choices, each offered up front: an ingress token that is not already in a rift, into a rift system
/// that holds no ingress token (rule 14). A rift is a printed one or a dimensional tear.
fn alraith(owner_name: &str, seat: &PlayerId) -> Ability {
    let owner = seat.clone();
    let claimant = seat.clone();
    Ability::stateful(
        format!("breakthrough:{owner_name}:{BREAKTHROUGH}:BREAKTHROUGH_GAINED:after"),
        seat.clone(),
        "BREAKTHROUGH_GAINED",
        Relation::After,
        Arc::new(move |event, _resolver, context| {
            if event_player(event).as_ref() != Some(&owner)
                || event.text("breakthrough") != Some(BREAKTHROUGH)
            {
                return Ok(());
            }
            crate::fracture::after_breakthrough_gained(
                context.state,
                context.content,
                context.sources,
                context.galaxy,
                context.table,
                context.rng,
                &owner,
            )
            .map_err(crate::timing::TimingError::IllegalChoice)?;
            for _ in 0..INGRESS_MOVES {
                let moves = ingress_moves(context.state, context.content, context.sources);
                if moves.is_empty() {
                    break;
                }
                let mut options: Vec<ChoiceOption> = moves
                    .iter()
                    .map(|(from, to)| {
                        ChoiceOption::labelled(
                            format!("{from}>{to}"),
                            "ingress_move",
                            format!("move the ingress token in {from} into {to}"),
                        )
                    })
                    .collect();
                options.push(ChoiceOption::labelled(
                    "done",
                    crate::choice::DECLINE_KIND,
                    "move no more ingress tokens",
                ));
                let choice = Choice::new(
                    owner.clone(),
                    "Al'Raith Ix Ianovar: move an ingress token into a gravity rift".to_owned(),
                    options,
                )
                .contextualized(decision(
                    context.state,
                    &owner,
                    BREAKTHROUGH,
                    "ingress_move",
                ));
                let answer = context
                    .ask_seeing(&choice)
                    .map_err(crate::timing::TimingError::IllegalChoice)?;
                if answer.is_decline() {
                    break;
                }
                let Some((from, to)) = moves
                    .into_iter()
                    .find(|(from, to)| format!("{from}>{to}") == answer.id)
                else {
                    break;
                };
                context.state.ingress_tokens.remove(&from);
                context.state.ingress_tokens.insert(to);
            }
            Ok(())
        }),
    )
    .with_stateful_condition(Arc::new(move |event, _, context| {
        event_player(event).as_ref() == Some(&claimant)
            && event.text("breakthrough") == Some(BREAKTHROUGH)
            && crate::breakthroughs::holds(context.state, &claimant, BREAKTHROUGH)
    }))
}

/// Every legal move of one ingress token: from a system that is not itself a rift, into a gravity
/// rift system on the board that holds no ingress token.
fn ingress_moves(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
) -> Vec<(SystemId, SystemId)> {
    if !state.fracture_in_play {
        return Vec::new();
    }
    let targets: Vec<SystemId> = state
        .board
        .keys()
        .filter(|system| !state.ingress_tokens.contains(*system))
        .filter(|system| is_gravity_rift_system(state, content, sources, system))
        .cloned()
        .collect();
    let mut moves = Vec::new();
    for from in &state.ingress_tokens {
        if is_gravity_rift_system(state, content, sources, from) {
            continue;
        }
        for to in &targets {
            moves.push((from.clone(), to.clone()));
        }
    }
    moves
}

/// "Apply +1 to the MOVE value of each of your ships that start their movement in The Fracture."
fn alraith_move_bonus(state: &GameState, site: &super::hooks_movement::MoveSite<'_>) -> i32 {
    i32::from(
        crate::breakthroughs::holds(state, site.player, BREAKTHROUGH)
            && crate::fracture::is_fracture_system(
                ContentStore::embedded(),
                ti4_model::content_types::DEFAULT,
                site.origin,
            ),
    )
}

// -- Vortex ------------------------------------------------------------------------------------

const VORTEX: &str = "vtx";
const VORTEX_ACTION: &str = "faction|cabal|vortex";

/// True when the seat holds the card and has not exhausted it.
fn technology_ready(state: &GameState, player: &PlayerId, alias: &str) -> bool {
    crate::technology::technology_text_ready(state, player, alias)
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

/// Whether this unit is one of the seat's own space docks.
fn is_own_dock(
    content: &ContentStore,
    sources: SourceSet,
    unit: &ti4_model::units::Unit,
    player: &PlayerId,
) -> bool {
    unit.owner == *player && base_type(content, sources, &unit.type_id) == "spacedock"
}

/// The systems that hold one of this seat's space docks.
///
/// A space dock is a structure, so it normally sits in a planet's units; the space layer is
/// scanned too because the engine does not forbid a structure in open space.
fn own_dock_systems(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
) -> BTreeSet<String> {
    let mut docks = BTreeSet::new();
    for (system, seat) in &state.board {
        let present = seat
            .units
            .iter()
            .chain(seat.planet_units.values().flatten());
        for unit in present {
            if is_own_dock(content, sources, unit, player) {
                docks.insert(system.as_str().to_owned());
            }
        }
    }
    docks
}

/// Every (owner, unit type) the card could take: another player's non-structure unit type in a
/// system next to one of our space docks, with a unit of that type still in their reinforcements.
fn vortex_targets(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: &ti4_content::galaxy::Galaxy,
    player: &PlayerId,
) -> Vec<(PlayerId, UnitTypeId)> {
    let mut targets: Vec<(PlayerId, UnitTypeId)> = Vec::new();
    for dock in own_dock_systems(state, content, sources, player) {
        for neighbour in galaxy.adjacent(dock.as_str()) {
            let Some(seat) = state.board.get(&SystemId::new(neighbour)) else {
                continue;
            };
            let present = seat
                .units
                .iter()
                .chain(seat.planet_units.values().flatten());
            for unit in present {
                if &unit.owner == player || crate::neutral_units::is_neutral(&unit.owner) {
                    continue;
                }
                let Some(kind) =
                    ti4_content::units::unit_type(content, unit.type_id.as_str(), sources)
                else {
                    continue;
                };
                if kind.is_structure() {
                    continue;
                }
                if crate::supply::remaining(state, content, sources, &unit.owner, &unit.type_id)
                    == 0
                {
                    continue;
                }
                let target = (unit.owner.clone(), unit.type_id.clone());
                if !targets.contains(&target) {
                    targets.push(target);
                }
            }
        }
    }
    targets.sort();
    targets.dedup();
    targets
}

/// The card is offered only when a legal capture exists, so the table never shows an action that
/// cannot be taken.
fn vortex_component_actions(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: &ti4_content::galaxy::Galaxy,
    player: &PlayerId,
) -> Vec<ChoiceOption> {
    if !is_cabal(state, player) || !technology_ready(state, player, VORTEX) {
        return Vec::new();
    }
    if vortex_targets(state, content, sources, galaxy, player).is_empty() {
        return Vec::new();
    }
    vec![ChoiceOption::labelled(
        VORTEX_ACTION,
        crate::faction_abilities::ACTION_KIND,
        "Vortex: exhaust to capture another player's unit type from their reinforcements",
    )]
}

/// Perform the card. Everything is revalidated, the capture is the only mutation, and the card is
/// exhausted only after a unit was actually taken.
fn vortex_perform(
    context: &mut TimingContext<'_>,
    player: &PlayerId,
    option: &ChoiceOption,
) -> bool {
    if option.id != VORTEX_ACTION {
        return false;
    }
    if !is_cabal(context.state, player) || !technology_ready(context.state, player, VORTEX) {
        return false;
    }
    let Some(galaxy) = context.galaxy else {
        return false;
    };
    let targets = vortex_targets(
        context.state,
        context.content,
        context.sources,
        galaxy,
        player,
    );
    let (owner, unit) = match targets.as_slice() {
        [] => return false,
        [only] => (only.0.clone(), only.1.clone()),
        _ => {
            let options: Vec<ChoiceOption> = targets
                .iter()
                .map(|(owner, unit)| {
                    ChoiceOption::labelled(
                        format!("{}|{}", owner.as_str(), unit.as_str()),
                        "vortex",
                        format!(
                            "take a {} from {}'s reinforcements",
                            unit.as_str(),
                            owner.as_str()
                        ),
                    )
                })
                .collect();
            let choice = Choice::new(
                player.clone(),
                "Vortex: which unit type do you take".to_owned(),
                options,
            )
            .contextualized(decision(context.state, player, VORTEX, "vortex_target"));
            let Ok(answer) = context.ask_seeing(&choice) else {
                return false;
            };
            let Some((owner, unit)) = targets
                .iter()
                .find(|(owner, unit)| format!("{}|{}", owner.as_str(), unit.as_str()) == answer.id)
            else {
                return false;
            };
            (owner.clone(), unit.clone())
        }
    };
    let captured = crate::supply::capture_from_reinforcements(
        context.state,
        context.content,
        context.sources,
        player,
        &owner,
        &unit,
    );
    if !captured {
        return false;
    }
    context
        .state
        .player_mut(player)
        .unwrap()
        .exhausted_technologies
        .insert(TechnologyId::new(VORTEX));
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::choice::Window;
    use ti4_model::content_types::DEFAULT;
    use ti4_model::units::Unit;

    fn a() -> PlayerId {
        PlayerId::new("a")
    }

    fn b() -> PlayerId {
        PlayerId::new("b")
    }

    fn game() -> GameState {
        crate::fixtures::seated_game(&[("a", FACTION), ("b", "sol")], DEFAULT)
    }

    /// Two planets in two different systems, so a test can put a mech out of reach of a hit.
    ///
    /// # Panics
    ///
    /// If the corpus does not place planets in two different systems.
    fn separate_planets() -> [(SystemId, PlanetId); 2] {
        let store = ContentStore::embedded();
        let mut found: Vec<(SystemId, PlanetId)> = Vec::new();
        for (id, planet) in ti4_content::galaxy::all_planets(&store, DEFAULT) {
            let Some(system) = planet.system_id() else {
                continue;
            };
            if planet.is_placed_during_play() {
                continue;
            }
            let candidate = (SystemId::new(system), PlanetId::new(id));
            if found.iter().any(|(taken, _)| taken == &candidate.0) {
                continue;
            }
            found.push(candidate);
            if found.len() == 2 {
                break;
            }
        }
        assert_eq!(found.len(), 2, "the corpus places planets in two systems");
        [
            (found[0].0.clone(), found[0].1.clone()),
            (found[1].0.clone(), found[1].1.clone()),
        ]
    }

    /// How many of `owner`'s units of this type are on this planet.
    fn on_planet(
        state: &GameState,
        system: &SystemId,
        planet: &PlanetId,
        owner: &PlayerId,
        kind: &str,
    ) -> Vec<ti4_model::units::Unit> {
        state
            .system_state(system)
            .on_planet_of(planet, owner)
            .into_iter()
            .filter(|unit| unit.type_id.as_str() == kind)
            .cloned()
            .collect()
    }

    /// How many of `owner`'s units of this type are in this system's space area.
    fn in_space(
        state: &GameState,
        system: &SystemId,
        owner: &PlayerId,
        kind: &str,
    ) -> Vec<ti4_model::units::Unit> {
        state
            .system_state(system)
            .units_of(owner)
            .into_iter()
            .filter(|unit| unit.type_id.as_str() == kind)
            .cloned()
            .collect()
    }

    /// Apply ground hits the way the live game does: through the timing API, with every faction
    /// module armed exactly as `crate::reactions::arm` arms them.
    fn strike(
        state: &mut GameState,
        system: &SystemId,
        planet: &PlanetId,
        victim: &PlayerId,
        hits: usize,
    ) {
        let mut resolver = crate::fixtures::armed_resolver(state);
        let mut table = crate::choice::Table::default();
        crate::fixtures::with_context(state, DEFAULT, None, &mut table, |context| {
            crate::invasion::assign_ground_hits_in_timing(
                &mut resolver,
                context,
                system,
                planet,
                victim,
                hits,
                "ground_combat",
            )
            .expect("the hits resolve");
        });
    }

    #[test]
    fn infantry_destroyed_beside_a_cabal_mech_is_captured() {
        let (system, planet) = crate::fixtures::a_placed_planet();
        let mut state = game();
        crate::fixtures::put_on_planet(&mut state, &system, &planet, "cabal_mech", &a(), 1);
        crate::fixtures::put_on_planet(&mut state, &system, &planet, "infantry", &a(), 2);

        // Three hits: the mech absorbs the first, the two infantry take the next two.
        strike(&mut state, &system, &planet, &a(), 3);

        assert_eq!(
            crate::supply::captured_by(&state, &a()).to_vec(),
            vec![
                (a(), UnitTypeId::new("infantry")),
                (a(), UnitTypeId::new("infantry")),
            ],
            "both destroyed infantry are on the Cabal's own sheet"
        );
        assert_eq!(
            on_planet(&state, &system, &planet, &a(), "infantry").len(),
            0,
            "a captured unit is off the board"
        );
        let mechs = on_planet(&state, &system, &planet, &a(), "cabal_mech");
        assert_eq!(mechs.len(), 1, "the mech is still standing");
        assert!(mechs[0].sustained_damage, "the mech absorbed the first hit");
    }

    #[test]
    fn infantry_destroyed_without_a_mech_are_lost() {
        let (system, planet) = crate::fixtures::a_placed_planet();
        let mut state = game();
        crate::fixtures::put_on_planet(&mut state, &system, &planet, "infantry", &a(), 2);

        strike(&mut state, &system, &planet, &a(), 2);

        assert!(
            crate::supply::captured_by(&state, &a()).is_empty(),
            "with no Reanimator on the planet the infantry are destroyed, not captured"
        );
        assert_eq!(
            on_planet(&state, &system, &planet, &a(), "infantry").len(),
            0,
            "they are gone either way"
        );
    }

    #[test]
    fn a_mech_in_another_system_does_not_reach_the_hit() {
        let [home, away] = separate_planets();
        let mut state = game();
        crate::fixtures::put_on_planet(&mut state, &home.0, &home.1, "cabal_mech", &a(), 1);
        crate::fixtures::put_on_planet(&mut state, &away.0, &away.1, "infantry", &a(), 2);

        strike(&mut state, &away.0, &away.1, &a(), 2);

        assert!(
            crate::supply::captured_by(&state, &a()).is_empty(),
            "the Reanimator only reaches its own planet"
        );
        assert_eq!(
            on_planet(&state, &home.0, &home.1, &a(), "cabal_mech").len(),
            1,
            "the untouched mech is unharmed"
        );
    }

    #[test]
    fn another_players_infantry_is_not_taken() {
        let (system, planet) = crate::fixtures::a_placed_planet();
        let mut state = game();
        crate::fixtures::put_on_planet(&mut state, &system, &planet, "cabal_mech", &a(), 1);
        crate::fixtures::put_on_planet(&mut state, &system, &planet, "infantry", &b(), 2);

        strike(&mut state, &system, &planet, &b(), 2);

        assert!(
            crate::supply::captured_by(&state, &a()).is_empty(),
            "the Reanimator takes its own infantry, not an opponent's"
        );
        assert!(
            crate::supply::captured_by(&state, &b()).is_empty(),
            "and nobody captures for the victim either"
        );
        assert_eq!(
            on_planet(&state, &system, &planet, &a(), "cabal_mech").len(),
            1,
            "the Cabal is not involved in the hit"
        );
    }

    #[test]
    fn a_destroyed_cabal_mech_is_not_captured() {
        let (system, planet) = crate::fixtures::a_placed_planet();
        let mut state = game();
        crate::fixtures::put_on_planet(&mut state, &system, &planet, "cabal_mech", &a(), 2);

        // Three hits: the first two damage the mechs, the third destroys one of them.
        strike(&mut state, &system, &planet, &a(), 3);

        assert_eq!(
            on_planet(&state, &system, &planet, &a(), "cabal_mech").len(),
            1,
            "one mech was destroyed"
        );
        assert!(
            crate::supply::captured_by(&state, &a()).is_empty(),
            "the Reanimator captures infantry, not mechs"
        );
    }

    /// No Cabal in the seat, no capture: the ability is registered for a Cabal seat only, so a
    /// game without one is the ordinary rules.
    #[test]
    fn a_game_without_a_cabal_seat_is_unchanged() {
        let (system, planet) = crate::fixtures::a_placed_planet();
        let mut state = crate::fixtures::seated_game(&[("a", "sol"), ("b", "sol")], DEFAULT);
        crate::fixtures::put_on_planet(&mut state, &system, &planet, "mech", &a(), 1);
        crate::fixtures::put_on_planet(&mut state, &system, &planet, "infantry", &a(), 2);

        strike(&mut state, &system, &planet, &a(), 3);

        assert!(
            crate::supply::captured_by(&state, &a()).is_empty(),
            "nothing is captured when the destroyed infantry belong to a non-Cabal"
        );
        assert_eq!(
            on_planet(&state, &system, &planet, &a(), "infantry").len(),
            0,
            "the hits are applied exactly as they were before this module existed"
        );
    }

    #[test]
    fn a_mech_taking_a_hit_is_not_captured() {
        let (system, planet) = crate::fixtures::a_placed_planet();
        let mut state = game();
        crate::fixtures::put_on_planet(&mut state, &system, &planet, "cabal_mech", &a(), 1);

        strike(&mut state, &system, &planet, &a(), 1);

        assert!(
            crate::supply::captured_by(&state, &a()).is_empty(),
            "a damaged mech is not a destroyed infantry"
        );
        assert_eq!(
            on_planet(&state, &system, &planet, &a(), "cabal_mech").len(),
            1,
            "it absorbed the hit"
        );
    }

    /// A Cabal that can produce: it controls a planet and owns a spacedock there.
    ///
    /// The activation is funded with trade goods, which `available` counts as resources, so a test
    /// can see whether the activation paid anything.
    fn production_scene() -> (GameState, SystemId, PlanetId) {
        let (system, planet) = crate::fixtures::a_placed_planet();
        let mut state = game();
        state.system_mut(&system).set_control(planet.clone(), a());
        crate::fixtures::put_on_planet(&mut state, &system, &planet, "spacedock", &a(), 1);
        if let Some(seat) = state.player_mut(&a()) {
            seat.trade_goods = 30;
        }
        (state, system, planet)
    }

    /// Every option a production activation offered, and every answer it took.
    struct Run {
        offered: Vec<String>,
        taken: Vec<String>,
    }

    impl Run {
        fn took(&self, id: &str) -> bool {
            self.taken.iter().any(|taken| taken == id)
        }

        fn offered(&self, id: &str) -> bool {
            self.offered.iter().any(|offered| offered == id)
        }
    }

    /// Drive a real production activation, taking `wanted` whenever it is offered.
    ///
    /// `limit` opens the activation as an ability-driven production of that many units, which is how
    /// a test proves an exchanged unit spends the activation's capacity.
    fn produce(
        state: &mut GameState,
        content: &ContentStore,
        system: &SystemId,
        player: &PlayerId,
        limit: Option<i64>,
        wanted: &str,
    ) -> Run {
        let mut window = match limit {
            Some(units) => crate::production::ProductionWindow::for_ability(
                state,
                content,
                DEFAULT,
                player,
                system,
                Some(units),
            ),
            None => {
                crate::production::ProductionWindow::new(state, content, DEFAULT, player, system)
            }
        };
        let mut run = Run {
            offered: Vec::new(),
            taken: Vec::new(),
        };
        let mut dice = crate::dice::Dice::new();
        let mut rng = crate::rng::GameRng::new(0);
        let mut table = crate::choice::Table::new();
        let mut ctx = crate::choice::Resolving {
            content,
            sources: DEFAULT,
            dice: &mut dice,
            rng: &mut rng,
            table: &mut table,
            timing: None,
        };
        for _ in 0..40 {
            let Some(choice) = window.pending_choice(state, content, DEFAULT) else {
                break;
            };
            run.offered
                .extend(choice.options.iter().map(|option| option.id.clone()));
            let answer = choice
                .options
                .iter()
                .find(|option| option.id == wanted)
                .or_else(|| choice.options.first())
                .cloned()
                .expect("a production choice offers at least one option");
            run.taken.push(answer.id.clone());
            window
                .resolve(state, &mut ctx, answer)
                .expect("production accepts an option it generated");
        }
        run
    }

    #[test]
    fn a_captured_cruiser_is_returned_to_produce_a_cruiser() {
        let content = ContentStore::embedded();
        let (mut state, system, _) = production_scene();
        let cruiser = UnitTypeId::new("cruiser");
        assert!(
            crate::supply::capture_from_reinforcements(
                &mut state,
                &content,
                DEFAULT,
                &a(),
                &b(),
                &cruiser
            ),
            "the Cabal holds a captured cruiser"
        );

        let run = produce(
            &mut state,
            &content,
            &system,
            &a(),
            None,
            "exchange|cruiser",
        );

        assert!(
            run.offered("exchange|cruiser"),
            "the exchange is offered as its own option: {:?}",
            run.offered
        );
        assert!(
            run.took("exchange|cruiser"),
            "the activation took the exchange: {:?}",
            run.taken
        );
        assert_eq!(
            in_space(&state, &system, &a(), "cruiser").len(),
            1,
            "the cruiser is produced"
        );
        assert!(
            crate::supply::captured_by(&state, &a()).is_empty(),
            "the captured model is no longer held"
        );
    }

    #[test]
    fn an_exchanged_unit_costs_no_resources() {
        let content = ContentStore::embedded();
        let (mut state, system, _planet) = production_scene();
        assert!(
            crate::supply::capture_from_reinforcements(
                &mut state,
                &content,
                DEFAULT,
                &a(),
                &b(),
                &UnitTypeId::new("cruiser"),
            ),
            "the Cabal holds a captured cruiser"
        );

        let _ = produce(
            &mut state,
            &content,
            &system,
            &a(),
            None,
            "exchange|cruiser",
        );

        assert_eq!(
            state.player(&a()).map_or(0, |seat| seat.trade_goods),
            30,
            "the card charges no resources for the exchanged unit"
        );
    }

    #[test]
    fn the_exchange_is_not_offered_without_a_matching_capture() {
        let content = ContentStore::embedded();
        let (mut state, system, _planet) = production_scene();

        let run = produce(
            &mut state,
            &content,
            &system,
            &a(),
            None,
            "exchange|nothing",
        );

        assert!(
            run.offered.iter().any(|id| id.starts_with("build|")),
            "the activation still offers paid builds: {:?}",
            run.offered
        );
        assert!(
            !run.offered.iter().any(|id| id.starts_with("exchange|")),
            "nothing was captured, so nothing is offered: {:?}",
            run.offered
        );
    }

    #[test]
    fn the_exchange_is_offered_only_for_the_matching_type() {
        let content = ContentStore::embedded();
        let (mut state, system, _planet) = production_scene();
        assert!(
            crate::supply::capture_from_reinforcements(
                &mut state,
                &content,
                DEFAULT,
                &a(),
                &b(),
                &UnitTypeId::new("fighter"),
            ),
            "the Cabal holds a captured fighter"
        );

        let run = produce(
            &mut state,
            &content,
            &system,
            &a(),
            None,
            "exchange|fighter",
        );

        assert!(
            run.offered("exchange|fighter"),
            "a captured fighter exchanges for a fighter: {:?}",
            run.offered
        );
        assert!(
            !run.offered("exchange|cruiser"),
            "a captured fighter does not exchange for a cruiser: {:?}",
            run.offered
        );
    }

    #[test]
    fn an_exchanged_unit_still_spends_the_productions_capacity() {
        let content = ContentStore::embedded();
        let (mut state, system, _planet) = production_scene();
        assert!(
            crate::supply::capture_from_reinforcements(
                &mut state,
                &content,
                DEFAULT,
                &a(),
                &b(),
                &UnitTypeId::new("cruiser"),
            ),
            "the Cabal holds a captured cruiser"
        );

        let run = produce(
            &mut state,
            &content,
            &system,
            &a(),
            Some(1),
            "exchange|cruiser",
        );

        assert!(
            run.took("exchange|cruiser"),
            "the exchange is how the unit was built: {:?}",
            run.taken
        );
        assert_eq!(
            run.taken
                .iter()
                .filter(|id| id.starts_with("build|"))
                .count(),
            0,
            "the activation never had a second unit to pay for"
        );
        assert_eq!(
            in_space(&state, &system, &a(), "cruiser").len(),
            1,
            "the exchanged unit is the one unit the activation produced"
        );
    }

    #[test]
    fn a_returned_unit_goes_back_to_its_owners_reinforcements() {
        let content = ContentStore::embedded();
        let (mut state, system, _planet) = production_scene();
        let cruiser = UnitTypeId::new("cruiser");
        let before = crate::supply::remaining(&state, &content, DEFAULT, &b(), &cruiser);
        assert!(
            crate::supply::capture_from_reinforcements(
                &mut state,
                &content,
                DEFAULT,
                &a(),
                &b(),
                &cruiser
            ),
            "the Cabal holds a captured cruiser"
        );
        assert_eq!(
            crate::supply::remaining(&state, &content, DEFAULT, &b(), &cruiser),
            before - 1,
            "a captured model is off its owner's sheet"
        );

        let _ = produce(
            &mut state,
            &content,
            &system,
            &a(),
            None,
            "exchange|cruiser",
        );

        assert_eq!(
            crate::supply::remaining(&state, &content, DEFAULT, &b(), &cruiser),
            before,
            "returning it puts it back in its owner's reinforcements"
        );
    }

    #[test]
    fn a_non_cabal_is_never_offered_an_exchange() {
        let content = ContentStore::embedded();
        let (system, planet) = crate::fixtures::a_placed_planet();
        let mut state = crate::fixtures::seated_game(&[("a", "sol"), ("b", "sol")], DEFAULT);
        state.system_mut(&system).set_control(planet.clone(), a());
        crate::fixtures::put_on_planet(&mut state, &system, &planet, "spacedock", &a(), 1);
        if let Some(seat) = state.player_mut(&a()) {
            seat.trade_goods = 30;
        }
        assert!(
            crate::supply::capture_from_reinforcements(
                &mut state,
                &content,
                DEFAULT,
                &a(),
                &b(),
                &UnitTypeId::new("cruiser"),
            ),
            "a non-Cabal can hold a captured cruiser too"
        );

        let run = produce(
            &mut state,
            &content,
            &system,
            &a(),
            None,
            "exchange|cruiser",
        );

        assert!(
            !run.offered.iter().any(|id| id.starts_with("exchange|")),
            "the hook is gated on the Cabal: {:?}",
            run.offered
        );
    }

    /// Cruiser II asks for GYR, which a fresh Cabal seat does not have, and it upgrades a cruiser,
    /// so it is the card's own case: return a captured cruiser, research the upgrade.
    fn cr2() -> TechnologyId {
        TechnologyId::new("cr2")
    }

    #[test]
    fn a_returned_cruiser_pays_for_cruiser_ii_prerequisites() {
        let content = ContentStore::embedded();
        let mut state = game();
        let cruiser = UnitTypeId::new("cruiser");
        assert!(
            crate::supply::capture_from_reinforcements(
                &mut state,
                &content,
                DEFAULT,
                &a(),
                &b(),
                &cruiser
            ),
            "the Cabal holds a captured cruiser"
        );
        let held_by_b = crate::supply::remaining(&state, &content, DEFAULT, &b(), &cruiser);

        let waiver = riftmeld_offer(&state, &content, &a(), &cr2())
            .expect("a captured cruiser pays for Cruiser II");
        assert!(
            !waiver.payments.is_empty(),
            "each captured cruiser is a player choice"
        );
        assert!(
            crate::technology::can_research(&state, &content, DEFAULT, &a(), &cr2()),
            "the offer is what makes the technology researchable"
        );

        let payment = waiver.payments[0].id.clone();
        let before = state.clone();
        assert!(
            !crate::technology::research_with_waiver(
                &mut state,
                &content,
                DEFAULT,
                &a(),
                &cr2(),
                0,
                "not-a-captured-unit"
            ),
            "an illegal payment researches nothing"
        );
        assert_eq!(state, before, "and the whole transition is unchanged");

        assert!(
            crate::technology::research_with_waiver(
                &mut state,
                &content,
                DEFAULT,
                &a(),
                &cr2(),
                0,
                &payment
            ),
            "the captured cruiser pays for the prerequisites"
        );
        assert!(
            state.player(&a()).unwrap().technologies.contains(&cr2()),
            "the upgrade is researched"
        );
        assert!(
            crate::supply::captured_by(&state, &a()).is_empty(),
            "the captured cruiser was returned"
        );
        assert_eq!(
            crate::supply::remaining(&state, &content, DEFAULT, &b(), &cruiser),
            held_by_b + 1,
            "it went back to its owner's reinforcements"
        );
    }

    #[test]
    fn riftmeld_is_not_offered_without_a_matching_capture() {
        let content = ContentStore::embedded();
        let mut state = game();
        assert!(
            riftmeld_offer(&state, &content, &a(), &cr2()).is_none(),
            "nothing to return, no offer"
        );
        assert!(
            !crate::technology::can_research(&state, &content, DEFAULT, &a(), &cr2()),
            "so Cruiser II stays unresearchable"
        );

        assert!(
            crate::supply::capture_from_reinforcements(
                &mut state,
                &content,
                DEFAULT,
                &a(),
                &b(),
                &UnitTypeId::new("fighter")
            ),
            "the Cabal holds a captured fighter instead"
        );
        assert!(
            riftmeld_offer(&state, &content, &a(), &cr2()).is_none(),
            "a captured fighter does not pay for a cruiser upgrade"
        );
    }

    #[test]
    fn riftmeld_is_not_offered_for_a_technology_that_upgrades_no_unit() {
        let content = ContentStore::embedded();
        let mut state = game();
        assert!(
            crate::supply::capture_from_reinforcements(
                &mut state,
                &content,
                DEFAULT,
                &a(),
                &b(),
                &UnitTypeId::new("cruiser")
            ),
            "the Cabal holds a captured cruiser"
        );

        let propulsion = TechnologyId::new("gd");
        assert!(
            unit_upgrade_base(&content, DEFAULT, &propulsion).is_none(),
            "Gravity Drive upgrades no unit, so no captured unit matches it"
        );
        assert!(
            riftmeld_offer(&state, &content, &a(), &propulsion).is_none(),
            "and the card is not offered for it"
        );
    }

    #[test]
    fn a_non_cabal_cannot_return_a_captured_unit_for_research() {
        let content = ContentStore::embedded();
        let mut state = crate::fixtures::seated_game(&[("a", "sol"), ("b", "sol")], DEFAULT);
        assert!(
            crate::supply::capture_from_reinforcements(
                &mut state,
                &content,
                DEFAULT,
                &a(),
                &b(),
                &UnitTypeId::new("cruiser")
            ),
            "a non-Cabal can hold a captured cruiser too"
        );
        assert!(
            riftmeld_offer(&state, &content, &a(), &cr2()).is_none(),
            "the offer is gated on the Cabal"
        );
    }

    /// A hub whose centre holds a Cabal space dock, with every hub tile cleared of whatever the
    /// seats deployed at setup, so a Vortex test sees only what it places.
    fn vortex_scene() -> (GameState, crate::fixtures::Hub, SystemId) {
        let content = ContentStore::embedded();
        let (dock_system, dock_planet) = crate::fixtures::a_placed_planet();
        let hub = crate::fixtures::hub_with_centre(dock_system.as_str());
        let mut state = game();
        for id in hub.galaxy.system_ids() {
            let system = state.system_mut(&SystemId::new(id));
            system.units.clear();
            system.planet_units.clear();
        }
        // Deploying the faction puts a dimensional tear in its home system. Removing it leaves the
        // one dock this fixture is about, so the adjacency the action reads is unambiguous.
        for id in state.board.keys().cloned().collect::<Vec<SystemId>>() {
            let system = state.system_mut(&id);
            system
                .units
                .retain(|unit| !is_own_dock(&content, DEFAULT, unit, &a()));
            for units in system.planet_units.values_mut() {
                units.retain(|unit| !is_own_dock(&content, DEFAULT, unit, &a()));
            }
        }
        state
            .player_mut(&a())
            .unwrap()
            .technologies
            .insert(TechnologyId::new(VORTEX));
        crate::fixtures::put_on_planet(
            &mut state,
            &dock_system,
            &dock_planet,
            "cabal_spacedock",
            &a(),
            1,
        );
        (state, hub, dock_system)
    }

    /// A scripted answer table, so an ask that must not happen fails loudly.
    fn scripted(values: &[&str]) -> crate::choice::Table {
        crate::choice::Table::with_default(Box::new(crate::choice::Scripted::new(
            values
                .iter()
                .map(|value| (*value).to_owned())
                .collect::<Vec<String>>(),
        )))
    }

    fn offers(state: &GameState, hub: &crate::fixtures::Hub, player: &PlayerId) -> Vec<String> {
        let content = ContentStore::embedded();
        vortex_component_actions(state, &content, DEFAULT, &hub.galaxy, player)
            .iter()
            .map(|option| option.id.clone())
            .collect()
    }

    #[test]
    fn a_vortex_action_captures_a_reinforcement_unit() {
        let content = ContentStore::embedded();
        let (mut state, hub, _) = vortex_scene();
        let target = SystemId::new(hub.outer[0].clone());
        state
            .system_mut(&target)
            .units
            .push(Unit::new(UnitTypeId::new("cruiser"), b()));
        let before =
            crate::supply::remaining(&state, &content, DEFAULT, &b(), &UnitTypeId::new("cruiser"));

        assert_eq!(
            offers(&state, &hub, &a()),
            vec![VORTEX_ACTION.to_owned()],
            "a dock with a legal neighbour offers the action"
        );
        assert!(
            offers(&state, &hub, &b()).is_empty(),
            "a seat that is not the Cabal is offered nothing"
        );

        let mut table = scripted(&[]);
        let done = crate::fixtures::with_context(
            &mut state,
            DEFAULT,
            Some(&hub.galaxy),
            &mut table,
            |ctx| {
                vortex_perform(
                    ctx,
                    &a(),
                    &ChoiceOption::labelled(VORTEX_ACTION, "component", ""),
                )
            },
        );
        assert!(done, "one legal target needs no question");
        assert_eq!(
            crate::supply::captured_by(&state, &a()),
            vec![(b(), UnitTypeId::new("cruiser"))],
            "the captured unit comes from the other player's reinforcements"
        );
        assert_eq!(
            crate::supply::remaining(&state, &content, DEFAULT, &b(), &UnitTypeId::new("cruiser")),
            before - 1
        );
        assert!(
            state
                .player(&a())
                .unwrap()
                .exhausted_technologies
                .contains(&TechnologyId::new(VORTEX)),
            "the card is exhausted by the action"
        );
        assert!(
            offers(&state, &hub, &a()).is_empty(),
            "an exhausted card is not offered again"
        );
    }

    #[test]
    fn a_vortex_question_takes_the_selected_type() {
        let (mut state, hub, _) = vortex_scene();
        let target = SystemId::new(hub.outer[0].clone());
        state.system_mut(&target).units.extend([
            Unit::new(UnitTypeId::new("cruiser"), b()),
            Unit::new(UnitTypeId::new("dreadnought"), b()),
        ]);

        let answer = format!("{}|dreadnought", b().as_str());
        let mut table = scripted(&[answer.as_str()]);
        let done = crate::fixtures::with_context(
            &mut state,
            DEFAULT,
            Some(&hub.galaxy),
            &mut table,
            |ctx| {
                vortex_perform(
                    ctx,
                    &a(),
                    &ChoiceOption::labelled(VORTEX_ACTION, "component", ""),
                )
            },
        );
        assert!(done, "two legal targets ask which type to take");
        assert_eq!(
            crate::supply::captured_by(&state, &a()),
            vec![(b(), UnitTypeId::new("dreadnought"))],
            "only the selected type is captured"
        );
    }

    #[test]
    fn a_refused_vortex_changes_nothing() {
        let (mut state, hub, _) = vortex_scene();
        let target = SystemId::new(hub.outer[0].clone());
        state.system_mut(&target).units.extend([
            Unit::new(UnitTypeId::new("cruiser"), b()),
            Unit::new(UnitTypeId::new("dreadnought"), b()),
        ]);
        let before = state.clone();

        let mut table = scripted(&["not-a-target"]);
        let done = crate::fixtures::with_context(
            &mut state,
            DEFAULT,
            Some(&hub.galaxy),
            &mut table,
            |ctx| {
                vortex_perform(
                    ctx,
                    &a(),
                    &ChoiceOption::labelled(VORTEX_ACTION, "component", ""),
                )
            },
        );
        assert!(
            !done,
            "an answer that is not a legal type does not spend the action"
        );
        assert_eq!(state, before, "a refusal changes nothing");
    }

    #[test]
    fn vortex_ignores_structures_and_its_own_players_units() {
        let content = ContentStore::embedded();
        let (mut state, hub, dock_system) = vortex_scene();
        let target = SystemId::new(hub.outer[0].clone());
        state
            .system_mut(&target)
            .units
            .push(Unit::new(UnitTypeId::new("cruiser"), a()));
        crate::fixtures::put_on_planet(
            &mut state,
            &target,
            &PlanetId::new("foo"),
            "spacedock",
            &b(),
            1,
        );
        assert!(
            vortex_targets(&state, &content, DEFAULT, &hub.galaxy, &a()).is_empty(),
            "the Cabal's own ships and another player's structures are not targets"
        );
        assert!(offers(&state, &hub, &a()).is_empty());

        // The dock itself is not a target either, even in its own system.
        assert!(
            !vortex_targets(&state, &content, DEFAULT, &hub.galaxy, &a())
                .iter()
                .any(|(_, unit)| unit.as_str() == "cabal_spacedock")
        );
        assert_eq!(
            own_dock_systems(&state, &content, DEFAULT, &a()),
            std::collections::BTreeSet::from([dock_system.as_str().to_owned()]),
            "the dock is what the action reads adjacency from"
        );
    }

    #[test]
    fn vortex_does_not_offer_neutral_units() {
        let content = ContentStore::embedded();
        let (mut state, hub, _) = vortex_scene();
        let target = SystemId::new(hub.outer[0].clone());
        state.system_mut(&target).units.push(Unit::new(
            UnitTypeId::new("fighter"),
            PlayerId::new(crate::neutral_units::NEUTRAL),
        ));
        assert!(
            vortex_targets(&state, &content, DEFAULT, &hub.galaxy, &a()).is_empty(),
            "a neutral unit has no reinforcements to capture from"
        );
        assert!(offers(&state, &hub, &a()).is_empty());
    }

    #[test]
    fn a_vortex_target_must_still_be_in_the_reinforcements() {
        let content = ContentStore::embedded();
        let (mut state, hub, _) = vortex_scene();
        let target = SystemId::new(hub.outer[0].clone());
        while crate::supply::capture_from_reinforcements(
            &mut state,
            &content,
            DEFAULT,
            &a(),
            &b(),
            &UnitTypeId::new("cruiser"),
        ) {}
        state
            .system_mut(&target)
            .units
            .push(Unit::new(UnitTypeId::new("cruiser"), b()));
        assert!(
            crate::supply::remaining(&state, &content, DEFAULT, &b(), &UnitTypeId::new("cruiser"))
                == 0,
            "the player has no cruiser left to capture"
        );
        assert!(
            vortex_targets(&state, &content, DEFAULT, &hub.galaxy, &a()).is_empty(),
            "a type with nothing left in reinforcements is not a target"
        );
        assert!(offers(&state, &hub, &a()).is_empty());
    }

    #[test]
    fn a_vortex_needs_the_card_and_the_cabal() {
        let content = ContentStore::embedded();
        let (mut state, hub, _) = vortex_scene();
        let target = SystemId::new(hub.outer[0].clone());
        state
            .system_mut(&target)
            .units
            .push(Unit::new(UnitTypeId::new("cruiser"), b()));
        state
            .player_mut(&a())
            .unwrap()
            .technologies
            .retain(|tech| tech.as_str() != VORTEX);
        assert!(
            offers(&state, &hub, &a()).is_empty(),
            "an unresearched card is not an action"
        );

        let mut other = crate::fixtures::seated_game(&[("a", "cabal"), ("b", "sol")], DEFAULT);
        other
            .player_mut(&b())
            .unwrap()
            .technologies
            .insert(TechnologyId::new(VORTEX));
        crate::fixtures::put_on_planet(
            &mut other,
            &target,
            &PlanetId::new("foo"),
            "cabal_spacedock",
            &b(),
            1,
        );
        assert!(
            vortex_component_actions(&other, &content, DEFAULT, &hub.galaxy, &b()).is_empty(),
            "only the Cabal plays this card"
        );
    }

    #[test]
    fn an_exhausted_vortex_is_not_offered() {
        let content = ContentStore::embedded();
        let (mut state, hub, _) = vortex_scene();
        let target = SystemId::new(hub.outer[0].clone());
        state
            .system_mut(&target)
            .units
            .push(Unit::new(UnitTypeId::new("cruiser"), b()));
        state
            .player_mut(&a())
            .unwrap()
            .exhausted_technologies
            .insert(TechnologyId::new(VORTEX));
        assert!(
            vortex_component_actions(&state, &content, DEFAULT, &hub.galaxy, &a()).is_empty(),
            "an exhausted card is not an action"
        );
    }

    // -- CB-01: Devour and The Terror Between ----------------------------------------------------

    /// A system the fixture galaxy knows, so a flagship can be placed in it.
    fn c() -> PlayerId {
        PlayerId::new("c")
    }

    fn space_system() -> SystemId {
        SystemId::new(crate::fixtures::plain_hub().outer[0].clone())
    }

    /// A second hub system, so a test can name a system the flagship is not in.
    fn space_system_other() -> SystemId {
        SystemId::new(crate::fixtures::plain_hub().outer[1].clone())
    }

    /// Announce one event with every faction module armed, exactly as the live game does.
    fn emit(
        state: &mut GameState,
        event_type: &str,
        payload: &[(&'static str, serde_json::Value)],
    ) -> Result<(), String> {
        let galaxy = crate::fixtures::plain_hub().galaxy;
        let mut resolver = crate::fixtures::armed_resolver(state);
        let mut table = crate::choice::Table::default();
        crate::fixtures::with_context(state, DEFAULT, Some(&galaxy), &mut table, |context| {
            let payload: std::collections::BTreeMap<String, serde_json::Value> = payload
                .iter()
                .map(|(key, value)| ((*key).to_owned(), value.clone()))
                .collect();
            let event = context
                .event_sequence
                .next(event_type, payload)
                .expect("event id");
            resolver
                .emit_with_context(context, event, |_, _| {})
                .map(|_| ())
                .map_err(|error| format!("{error:?}"))
        })
    }

    fn space_started(system: &SystemId) -> Vec<(&'static str, serde_json::Value)> {
        vec![
            ("system", system.to_string().into()),
            ("attacker", "a".into()),
            ("defender", "b".into()),
            ("player", "a".into()),
        ]
    }

    fn space_ended(system: &SystemId) -> Vec<(&'static str, serde_json::Value)> {
        vec![
            ("system", system.to_string().into()),
            ("attacker", "a".into()),
            ("defender", "b".into()),
            ("winner", "a".into()),
        ]
    }

    fn ground_started(
        system: &SystemId,
        planet: &PlanetId,
    ) -> Vec<(&'static str, serde_json::Value)> {
        vec![
            ("system", system.to_string().into()),
            ("planet", planet.to_string().into()),
            ("attacker", "a".into()),
            ("defender", "b".into()),
        ]
    }

    fn ship_destroyed(
        system: &SystemId,
        owner: &str,
        unit: &str,
        during_combat: bool,
    ) -> Vec<(&'static str, serde_json::Value)> {
        vec![
            ("system", system.to_string().into()),
            ("player", owner.into()),
            ("unit", unit.into()),
            ("last", false.into()),
            ("cause", "space_combat".into()),
            ("during_space_combat", during_combat.into()),
        ]
    }

    /// Put a Cabal flagship in that system's space area.
    fn park_flagship(state: &mut GameState, system: &SystemId) {
        state
            .system_mut(system)
            .units
            .push(Unit::new(UnitTypeId::new(FLAGSHIP), a()));
    }

    /// What the Cabal has captured, as `(victim, unit type)`.
    fn captured(state: &GameState) -> Vec<(String, String)> {
        crate::supply::captured_by(state, &a())
            .iter()
            .map(|(owner, unit)| (owner.as_str().to_owned(), unit.as_str().to_owned()))
            .collect()
    }

    #[test]
    fn a_combat_the_cabal_retreated_from_leaves_no_mark_for_a_later_combat_of_others() {
        let system = space_system();
        let mut state =
            crate::fixtures::seated_game(&[("a", FACTION), ("b", "sol"), ("c", "sol")], DEFAULT);
        emit(&mut state, "SPACE_COMBAT_STARTED", &space_started(&system)).unwrap();
        assert!(!state.faction_marks.is_empty(), "the Cabal took part");
        // The Cabal retreats: the combat ends with no winner and the mark goes with it.
        emit(
            &mut state,
            "SPACE_COMBAT_ENDED",
            &[
                ("system", system.to_string().into()),
                ("attacker", "a".into()),
                ("defender", "b".into()),
            ],
        )
        .unwrap();
        assert!(state.faction_marks.is_empty(), "the mark is cleared");

        // Later, b and c fight in the same system: the Cabal is not in it.
        emit(
            &mut state,
            "SPACE_COMBAT_STARTED",
            &[
                ("system", system.to_string().into()),
                ("attacker", "b".into()),
                ("defender", "c".into()),
                ("player", "b".into()),
            ],
        )
        .unwrap();
        emit(
            &mut state,
            "SHIP_DESTROYED",
            &ship_destroyed(&system, "c", "cruiser", true),
        )
        .unwrap();
        assert!(captured(&state).is_empty(), "Devour does not feed on it");
    }

    #[test]
    fn devour_takes_an_opponents_ship_destroyed_in_a_combat_the_cabal_fought() {
        let content = ContentStore::embedded();
        let system = space_system();
        let untouched = game();
        let mut state = game();
        emit(&mut state, "SPACE_COMBAT_STARTED", &space_started(&system)).unwrap();

        emit(
            &mut state,
            "SHIP_DESTROYED",
            &ship_destroyed(&system, "b", "destroyer", true),
        )
        .unwrap();

        assert_eq!(
            captured(&state),
            vec![("b".to_owned(), "destroyer".to_owned())],
            "the destroyed ship is on the Cabal's sheet"
        );
        assert_eq!(
            crate::supply::remaining(
                &state,
                &content,
                DEFAULT,
                &b(),
                &UnitTypeId::new("destroyer")
            ),
            crate::supply::remaining(
                &untouched,
                &content,
                DEFAULT,
                &b(),
                &UnitTypeId::new("destroyer")
            ) - 1,
            "a captured model leaves the victim's reinforcements"
        );
    }

    #[test]
    fn devour_takes_nothing_from_a_combat_the_cabal_is_not_in() {
        let system = space_system();
        let mut state =
            crate::fixtures::seated_game(&[("a", FACTION), ("b", "sol"), ("c", "sol")], DEFAULT);
        // The Cabal is not one of the two sides, so no mark is written for it.
        emit(
            &mut state,
            "SPACE_COMBAT_STARTED",
            &[
                ("system", system.to_string().into()),
                ("attacker", "b".into()),
                ("defender", "c".into()),
                ("player", "b".into()),
            ],
        )
        .unwrap();
        emit(
            &mut state,
            "SHIP_DESTROYED",
            &ship_destroyed(&system, "c", "cruiser", true),
        )
        .unwrap();
        assert_eq!(captured(&state), Vec::<(String, String)>::new());
    }

    #[test]
    fn devour_reads_the_combat_not_the_board_when_the_cabals_own_fleet_is_wiped() {
        let system = space_system();
        let mut state = game();
        state
            .system_mut(&system)
            .units
            .push(Unit::new(UnitTypeId::new("cruiser"), a()));
        emit(&mut state, "SPACE_COMBAT_STARTED", &space_started(&system)).unwrap();
        // The destruction stage removes every loss of the round before any of them is announced, so
        // by the time this opponent's ship is announced the Cabal has nothing left in the system.
        state.system_mut(&system).units.clear();
        emit(
            &mut state,
            "SHIP_DESTROYED",
            &ship_destroyed(&system, "b", "destroyer", true),
        )
        .unwrap();
        assert_eq!(
            captured(&state),
            vec![("b".to_owned(), "destroyer".to_owned())],
            "participation is recorded at the start of the combat, not read from the board"
        );
    }

    #[test]
    fn devour_takes_nothing_after_the_combat_ends_or_outside_combat() {
        let system = space_system();
        let mut state = game();
        // Outside any combat.
        emit(
            &mut state,
            "SHIP_DESTROYED",
            &ship_destroyed(&system, "b", "destroyer", true),
        )
        .unwrap();
        assert_eq!(captured(&state), Vec::<(String, String)>::new());

        emit(&mut state, "SPACE_COMBAT_STARTED", &space_started(&system)).unwrap();
        // A loss in the system that the combat did not cause: an action card, a supernova.
        emit(
            &mut state,
            "SHIP_DESTROYED",
            &ship_destroyed(&system, "b", "destroyer", false),
        )
        .unwrap();
        assert_eq!(
            captured(&state),
            Vec::<(String, String)>::new(),
            "a destruction that is not combat does not feed Devour"
        );

        // In combat again, then the combat ends.
        emit(
            &mut state,
            "SHIP_DESTROYED",
            &ship_destroyed(&system, "b", "cruiser", true),
        )
        .unwrap();
        assert_eq!(captured(&state).len(), 1);
        emit(&mut state, "SPACE_COMBAT_ENDED", &space_ended(&system)).unwrap();
        assert!(
            !state.faction_marks.contains_key(&space_mark(&system)),
            "the participation mark does not outlive the combat"
        );
        emit(
            &mut state,
            "SHIP_DESTROYED",
            &ship_destroyed(&system, "b", "fighter", true),
        )
        .unwrap();
        assert_eq!(
            captured(&state).len(),
            1,
            "a later combat is a different combat"
        );
    }

    #[test]
    fn devour_takes_the_cabals_own_ships_and_structures_and_neutrals() {
        let system = space_system();
        let mut state =
            crate::fixtures::seated_game(&[("a", FACTION), ("b", "sol"), ("c", "sol")], DEFAULT);
        state
            .system_mut(&system)
            .units
            .push(Unit::new(UnitTypeId::new("cruiser"), a()));
        emit(&mut state, "SPACE_COMBAT_STARTED", &space_started(&system)).unwrap();
        for (owner, unit) in [
            ("a", "cruiser"),
            ("b", "spacedock"),
            ("b", "pds"),
            ("neutral", "destroyer"),
        ] {
            emit(
                &mut state,
                "SHIP_DESTROYED",
                &ship_destroyed(&system, owner, unit, true),
            )
            .unwrap();
        }
        assert_eq!(
            captured(&state),
            Vec::<(String, String)>::new(),
            "Devour takes an opponent's non-structure units: not its own, not a structure, not a neutral's"
        );
    }

    #[test]
    fn devour_takes_an_opponents_ground_force_destroyed_by_ground_combat() {
        let (system, planet) = crate::fixtures::a_placed_planet();
        let mut state = game();
        crate::fixtures::put_on_planet(&mut state, &system, &planet, "infantry", &b(), 1);
        emit(
            &mut state,
            "GROUND_COMBAT_STARTED",
            &ground_started(&system, &planet),
        )
        .unwrap();
        strike(&mut state, &system, &planet, &b(), 1);
        assert_eq!(
            captured(&state),
            vec![("b".to_owned(), "infantry".to_owned())],
            "the ground half of Devour takes the opponent's ground force"
        );
    }

    #[test]
    fn devour_takes_nothing_from_a_ground_force_removed_outside_combat() {
        let (system, planet) = crate::fixtures::a_placed_planet();
        let mut state = game();
        crate::fixtures::put_on_planet(&mut state, &system, &planet, "mech", &b(), 1);
        // No ground combat started, so this is a raid, not a battle.
        strike(&mut state, &system, &planet, &b(), 1);
        assert_eq!(captured(&state), Vec::<(String, String)>::new());

        // A combat the Cabal is not in does not feed it either.
        let (other_system, other_planet) = separate_planets()[1].clone();
        let mut other =
            crate::fixtures::seated_game(&[("a", FACTION), ("b", "sol"), ("c", "sol")], DEFAULT);
        crate::fixtures::put_on_planet(&mut other, &other_system, &other_planet, "mech", &c(), 1);
        emit(
            &mut other,
            "GROUND_COMBAT_STARTED",
            &[
                ("system", other_system.to_string().into()),
                ("planet", other_planet.to_string().into()),
                ("attacker", "b".into()),
                ("defender", "c".into()),
            ],
        )
        .unwrap();
        strike(&mut other, &other_system, &other_planet, &c(), 1);
        assert_eq!(captured(&other), Vec::<(String, String)>::new());
    }

    #[test]
    fn the_flagship_takes_every_non_structure_loss_in_its_system_including_its_own() {
        let system = space_system();
        let mut state = game();
        park_flagship(&mut state, &system);
        // No combat is announced at all: this clause is not a combat ability.
        for (owner, unit) in [("b", "destroyer"), ("a", "cruiser"), ("b", "spacedock")] {
            emit(
                &mut state,
                "SHIP_DESTROYED",
                &ship_destroyed(&system, owner, unit, false),
            )
            .unwrap();
        }
        assert_eq!(
            captured(&state),
            vec![
                ("b".to_owned(), "destroyer".to_owned()),
                ("a".to_owned(), "cruiser".to_owned()),
            ],
            "the Cabal's own ships too, but never a structure"
        );
    }

    #[test]
    fn the_flagship_takes_a_ground_force_destroyed_in_its_system() {
        let (system, planet) = crate::fixtures::a_placed_planet();
        let mut state = game();
        park_flagship(&mut state, &system);
        crate::fixtures::put_on_planet(&mut state, &system, &planet, "infantry", &b(), 1);
        strike(&mut state, &system, &planet, &b(), 1);
        assert_eq!(
            captured(&state),
            vec![("b".to_owned(), "infantry".to_owned())],
            "the flagship's clause covers ground forces as well as ships"
        );
    }

    #[test]
    fn the_flagship_takes_nothing_after_it_is_destroyed_or_elsewhere() {
        let system = space_system();
        let other = space_system_other();
        let mut state = game();
        park_flagship(&mut state, &system);
        emit(
            &mut state,
            "SHIP_DESTROYED",
            &ship_destroyed(&other, "b", "destroyer", false),
        )
        .unwrap();
        assert_eq!(
            captured(&state),
            Vec::<(String, String)>::new(),
            "a system the flagship is not in is not covered"
        );

        // "All other": the flagship does not capture itself, and once it is gone it captures
        // nothing at all, even though its own destruction is what removed it.
        // The destruction stage has already taken the flagship off the board when it announces it.
        state.system_mut(&system).units.clear();
        emit(
            &mut state,
            "SHIP_DESTROYED",
            &ship_destroyed(&system, "a", FLAGSHIP, false),
        )
        .unwrap();
        assert_eq!(
            captured(&state),
            Vec::<(String, String)>::new(),
            "the flagship is not captured by its own ability"
        );
        emit(
            &mut state,
            "SHIP_DESTROYED",
            &ship_destroyed(&system, "b", "cruiser", false),
        )
        .unwrap();
        assert_eq!(
            captured(&state),
            Vec::<(String, String)>::new(),
            "a destroyed flagship stops claiming"
        );
    }

    #[test]
    fn the_flagship_wins_the_overlap_and_a_destroyed_model_is_captured_once() {
        let system = space_system();
        let mut state = game();
        park_flagship(&mut state, &system);
        // Devour is armed for this combat and the flagship is in the system: exactly one of them
        // may take each loss.
        emit(&mut state, "SPACE_COMBAT_STARTED", &space_started(&system)).unwrap();
        emit(
            &mut state,
            "SHIP_DESTROYED",
            &ship_destroyed(&system, "b", "destroyer", true),
        )
        .unwrap();
        assert_eq!(
            captured(&state),
            vec![("b".to_owned(), "destroyer".to_owned())],
            "one claim, not two"
        );
    }

    #[test]
    fn reanimator_yields_to_the_flagship_in_the_same_system() {
        let (system, planet) = crate::fixtures::a_placed_planet();
        let mut state = game();
        crate::fixtures::put_on_planet(&mut state, &system, &planet, "cabal_mech", &a(), 1);
        crate::fixtures::put_on_planet(&mut state, &system, &planet, "infantry", &a(), 1);
        park_flagship(&mut state, &system);
        // The mech absorbs the first hit, so the second one destroys the infantry.
        strike(&mut state, &system, &planet, &a(), 2);
        assert_eq!(
            captured(&state),
            vec![("a".to_owned(), "infantry".to_owned())],
            "the infantry is captured once, by whichever Cabal ability claims the system"
        );
    }

    #[test]
    fn the_cabal_flagship_is_registered_and_devour_is_claimed() {
        assert!(MODULE.units.contains(&FLAGSHIP));
        assert!(MODULE.abilities.contains(&"devour"));
        let content = ContentStore::embedded();
        let kind = ti4_content::units::unit_type(&content, FLAGSHIP, DEFAULT)
            .expect("the corpus defines the Cabal flagship");
        assert_eq!(
            kind.base_type(),
            "flagship",
            "it is a flagship, so Amalgamation can become one"
        );
        assert!(
            !kind.is_structure(),
            "a flagship is not a structure, so it is itself capturable"
        );
    }

    #[test]
    fn flagship_does_not_capture_a_structure_destroyed_in_the_same_system() {
        let mut state = game();
        let system = space_system_other();
        park_flagship(&mut state, &system);
        state
            .system_mut(&system)
            .units
            .push(Unit::new(UnitTypeId::new("cabal_spacedock"), b()));
        emit(&mut state, "SPACE_COMBAT_STARTED", &space_started(&system)).unwrap();
        emit(
            &mut state,
            "SHIP_DESTROYED",
            &ship_destroyed(&system, "b", "cabal_spacedock", true),
        )
        .unwrap();
        assert_eq!(captured(&state), Vec::<(String, String)>::new());
    }

    // -- CB-01: real routes (combat window and invasion window, armed as the game arms them) -----

    /// Fight the space combat in `system` through the combat window with every die a hit.
    fn fight_space(state: &mut GameState, system: &SystemId) {
        use crate::choice::{Resolving, TimingHandle};
        let content = ContentStore::embedded();
        let mut resolver = crate::fixtures::armed_resolver(state);
        let mut sequence = crate::event::EventSequence::new();
        let mut table = crate::choice::Table::with_default(Box::new(crate::choice::FirstOption));
        let mut dice = crate::dice::Dice::from_faces(std::iter::repeat_n(10u32, 60));
        let mut rng = crate::rng::GameRng::new(5);
        let mut window = crate::combat::CombatWindow::new(state, content, DEFAULT, system);
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
        window.settle_open(state, &mut ctx).expect("opens");
        while window.outcome().is_none() {
            window.drive(state, &mut ctx).expect("drives");
            if window.outcome().is_some() {
                break;
            }
            let _ = window.take_scoring_occurrence();
            window.settle_open(state, &mut ctx).expect("settles");
        }
    }

    /// Run the invasion of `system` by `invader` through the invasion window with every die a hit.
    fn invade(state: &mut GameState, system: &SystemId, invader: &PlayerId) {
        use crate::choice::{Resolving, TimingHandle};
        let content = ContentStore::embedded();
        let mut resolver = crate::fixtures::armed_resolver(state);
        let mut sequence = crate::event::EventSequence::new();
        let mut table = crate::choice::Table::with_default(Box::new(crate::choice::FirstOption));
        let mut dice = crate::dice::Dice::from_faces(std::iter::repeat_n(10u32, 60));
        let mut rng = crate::rng::GameRng::new(5);
        let mut window = crate::invasion::InvasionWindow::at_commit_step(state, invader, system);
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
        window.settle(state, &mut ctx);
        let mut guard = 0;
        while !window.is_done() && guard < 40 {
            guard += 1;
            window.drive(state, &mut ctx).expect("drives");
            while window.take_scoring_occurrence().is_some() {}
            window.settle(state, &mut ctx);
        }
        assert!(window.is_done(), "the invasion finished");
    }

    /// Every capture held by every seat, as `(captor, victim, unit)`, sorted.
    fn all_captured(state: &GameState) -> Vec<(String, String, String)> {
        let mut rows: Vec<(String, String, String)> = state
            .players
            .iter()
            .flat_map(|seat| {
                crate::supply::captured_by(state, &seat.id)
                    .iter()
                    .map(|(owner, unit)| {
                        (
                            seat.id.as_str().to_owned(),
                            owner.as_str().to_owned(),
                            unit.as_str().to_owned(),
                        )
                    })
                    .collect::<Vec<_>>()
            })
            .collect();
        rows.sort();
        rows
    }

    fn row(captor: &str, victim: &str, unit: &str) -> (String, String, String) {
        (captor.to_owned(), victim.to_owned(), unit.to_owned())
    }

    #[test]
    fn a_real_space_combat_feeds_devour() {
        let system = SystemId::new("18");
        let mut state = game();
        crate::fixtures::put(&mut state, &system, "cruiser", &a(), 3);
        crate::fixtures::put(&mut state, &system, "destroyer", &b(), 1);
        crate::fixtures::put(&mut state, &system, "fighter", &b(), 1);
        fight_space(&mut state, &system);
        assert_eq!(
            all_captured(&state),
            vec![row("a", "b", "destroyer"), row("a", "b", "fighter")],
            "the opponent's destroyed ship and fighter are captured; the Cabal's own losses are not"
        );
        assert!(
            state.faction_marks.is_empty(),
            "the participation mark is cleared with the combat"
        );
    }

    #[test]
    fn a_real_space_combat_the_cabal_is_not_in_feeds_nothing() {
        let system = SystemId::new("18");
        let mut state =
            crate::fixtures::seated_game(&[("a", FACTION), ("b", "sol"), ("c", "sol")], DEFAULT);
        crate::fixtures::put(&mut state, &system, "cruiser", &b(), 2);
        crate::fixtures::put(&mut state, &system, "destroyer", &c(), 1);
        fight_space(&mut state, &system);
        assert_eq!(all_captured(&state), Vec::new());
    }

    #[test]
    fn a_real_space_combat_with_the_flagship_captures_each_loss_once() {
        let system = SystemId::new("18");
        let mut state = game();
        park_flagship(&mut state, &system);
        crate::fixtures::put(&mut state, &system, "cruiser", &a(), 1);
        crate::fixtures::put(&mut state, &system, "destroyer", &b(), 1);
        fight_space(&mut state, &system);
        let mut expected = vec![row("a", "b", "destroyer")];
        if in_space(&state, &system, &a(), "cruiser").is_empty() {
            expected.push(row("a", "a", "cruiser"));
        }
        expected.sort();
        assert_eq!(
            all_captured(&state),
            expected,
            "one row per destroyed model, the Cabal's own loss included, none twice"
        );
        assert!(
            !in_space(&state, &system, &a(), FLAGSHIP).is_empty(),
            "the flagship survived to witness the losses"
        );
    }

    #[test]
    fn a_real_space_combat_without_a_cabal_captures_nothing() {
        let system = SystemId::new("18");
        let mut state = crate::fixtures::seated_game(&[("a", "sol"), ("b", "sol")], DEFAULT);
        crate::fixtures::put(&mut state, &system, "cruiser", &a(), 3);
        crate::fixtures::put(&mut state, &system, "destroyer", &b(), 1);
        fight_space(&mut state, &system);
        assert_eq!(all_captured(&state), Vec::new());
    }

    /// An invasion of a planet by player a: two infantry arrive from space, one of b's defends.
    fn invasion_scene(state: &mut GameState) -> (SystemId, PlanetId) {
        let (system, planet) = crate::fixtures::a_placed_planet();
        let seat = state.system_mut(&system);
        seat.units.clear();
        seat.planet_units.clear();
        state.system_mut(&system).set_control(planet.clone(), b());
        crate::fixtures::put(state, &system, "carrier", &a(), 1);
        crate::fixtures::put(state, &system, "infantry", &a(), 2);
        crate::fixtures::put_on_planet(state, &system, &planet, "infantry", &b(), 1);
        (system, planet)
    }

    #[test]
    fn a_real_ground_combat_feeds_devour() {
        let mut state = game();
        let (system, _) = invasion_scene(&mut state);
        invade(&mut state, &system, &a());
        assert_eq!(
            all_captured(&state),
            vec![row("a", "b", "infantry")],
            "the defender's infantry is captured; the invader's own loss is not"
        );
    }

    #[test]
    fn a_real_ground_combat_with_the_flagship_captures_each_loss_once() {
        let mut state = game();
        let (system, _) = invasion_scene(&mut state);
        park_flagship(&mut state, &system);
        invade(&mut state, &system, &a());
        assert_eq!(
            all_captured(&state),
            vec![row("a", "a", "infantry"), row("a", "b", "infantry")],
            "the flagship takes both losses, the Cabal's own included, once each"
        );
    }

    #[test]
    fn a_real_ground_combat_without_a_cabal_captures_nothing() {
        let mut state = crate::fixtures::seated_game(&[("a", "sol"), ("b", "sol")], DEFAULT);
        let (system, _) = invasion_scene(&mut state);
        invade(&mut state, &system, &a());
        assert_eq!(all_captured(&state), Vec::new());
    }

    // -- CB-05 / CB-08: Dimensional Tear and the commander ----------------------------------------

    /// The Cabal game with nothing of player a's left on the board, so a test places only what it
    /// is about. Deploying the faction puts a dimensional tear in its home system.
    fn bare_cabal() -> GameState {
        let mut state = game();
        for seat in state.board.values_mut() {
            seat.units.retain(|unit| unit.owner != a());
            for units in seat.planet_units.values_mut() {
                units.retain(|unit| unit.owner != a());
            }
        }
        state
    }

    /// Printed gravity rifts in the corpus, in system-id order.
    fn printed_rifts() -> Vec<SystemId> {
        let content = ContentStore::embedded();
        ti4_content::galaxy::all_systems(content, DEFAULT)
            .iter()
            .filter(|(_, system)| system.is_gravity_rift())
            .map(|(id, _)| SystemId::new(*id))
            .collect()
    }

    fn commander() -> LeaderId {
        LeaderId::new("cabalcommander")
    }

    fn unlocked(state: &GameState) -> Option<bool> {
        commander_unlocked(
            state,
            ContentStore::embedded(),
            DEFAULT,
            None,
            &a(),
            &commander(),
        )
    }

    /// A planet in a system that is not a printed rift, for a dock.
    fn dock_planet() -> (SystemId, PlanetId) {
        let (system, planet) = crate::fixtures::a_placed_planet();
        assert!(!printed_rifts().contains(&system));
        (system, planet)
    }

    #[test]
    fn the_commander_unlocks_with_units_in_three_gravity_rifts() {
        let rifts = printed_rifts();
        assert!(rifts.len() >= 2, "the corpus prints at least two rifts");
        let mut state = bare_cabal();
        assert_eq!(unlocked(&state), Some(false), "no units anywhere");

        crate::fixtures::put(&mut state, &rifts[0], "cruiser", &a(), 2);
        crate::fixtures::put(&mut state, &rifts[1], "fighter", &a(), 1);
        assert_eq!(
            unlocked(&state),
            Some(false),
            "two rifts, however many units they hold"
        );

        // A third rift made by a Dimensional Tear: the dock is the unit in it.
        let (system, planet) = dock_planet();
        crate::fixtures::put_on_planet(&mut state, &system, &planet, "cabal_spacedock", &a(), 1);
        assert_eq!(
            unlocked(&state),
            Some(true),
            "a dimensional tear is a gravity rift"
        );

        // The tear is the dock: without it the system is no rift and the count drops again.
        state.system_mut(&system).planet_units.clear();
        assert_eq!(unlocked(&state), Some(false));
    }

    #[test]
    fn another_players_units_do_not_unlock_the_commander_and_it_names_only_its_own_card() {
        let rifts = printed_rifts();
        let mut state = bare_cabal();
        for rift in rifts.iter().take(2) {
            crate::fixtures::put(&mut state, rift, "cruiser", &a(), 1);
        }
        let (system, planet) = dock_planet();
        crate::fixtures::put_on_planet(&mut state, &system, &planet, "infantry", &b(), 1);
        assert_eq!(
            unlocked(&state),
            Some(false),
            "an opponent's unit is not yours"
        );
        for rift in &rifts {
            crate::fixtures::put(&mut state, rift, "cruiser", &b(), 1);
        }
        assert_eq!(
            unlocked(&state),
            Some(false),
            "nor are an opponent's units in rifts"
        );
        assert_eq!(
            commander_unlocked(
                &state,
                ContentStore::embedded(),
                DEFAULT,
                None,
                &a(),
                &LeaderId::new("solcommander")
            ),
            None,
            "not this module's commander"
        );
    }

    #[test]
    fn the_shared_unlock_check_delivers_the_commander_and_its_exemption_is_then_live() {
        let content = ContentStore::embedded();
        let rifts = printed_rifts();
        let mut state = bare_cabal();
        let (system, planet) = dock_planet();
        state.system_mut(&system).set_control(planet.clone(), a());
        crate::fixtures::put_on_planet(&mut state, &system, &planet, "cabal_spacedock", &a(), 1);
        state.player_mut(&a()).unwrap().trade_goods = 30;
        let limit_one = |state: &GameState| {
            let window = crate::production::ProductionWindow::for_ability(
                state,
                content,
                DEFAULT,
                &a(),
                &system,
                Some(1),
            );
            window
                .pending_choice(state, content, DEFAULT)
                .map(|choice| choice.options.iter().map(|o| o.id.clone()).collect())
                .unwrap_or_else(Vec::<String>::new)
        };
        assert!(
            !limit_one(&state).contains(&"build|infantry|2".to_owned()),
            "a locked commander exempts nothing"
        );

        for rift in rifts.iter().take(2) {
            crate::fixtures::put(&mut state, rift, "cruiser", &a(), 1);
        }
        let freed = crate::leaders::check_unlocks(&mut state, content, DEFAULT, None, &a());
        assert!(
            freed.contains(&commander()),
            "the unlock sweep finds it: {freed:?}"
        );
        assert_eq!(
            state.player(&a()).unwrap().leaders.get(&commander()),
            Some(&ti4_model::state::LeaderStatus::Unlocked)
        );
        assert!(crate::promissory::has_commander_ability(
            &state,
            &a(),
            "cabalcommander"
        ));
        assert!(
            limit_one(&state).contains(&"build|infantry|2".to_owned()),
            "two of the infantry do not count against a production limit of one: {:?}",
            limit_one(&state)
        );
    }

    #[test]
    fn a_game_without_a_cabal_seat_never_makes_a_dimensional_tear() {
        let mut state = crate::fixtures::seated_game(&[("a", "sol"), ("b", "sol")], DEFAULT);
        let (system, planet) = dock_planet();
        crate::fixtures::put_on_planet(&mut state, &system, &planet, "spacedock", &a(), 1);
        assert!(tear_systems(&state).is_empty(), "no Cabal, no tear");
        // Even a stray Cabal dock id on a non-Cabal seat is nothing: the tear is the Cabal's.
        crate::fixtures::put_on_planet(&mut state, &system, &planet, "cabal_spacedock", &a(), 1);
        assert!(tear_systems(&state).is_empty());
        assert!(!is_gravity_rift_system(
            &state,
            ContentStore::embedded(),
            DEFAULT,
            &system
        ));
        let rifts = printed_rifts();
        for rift in rifts.iter().take(3) {
            crate::fixtures::put(&mut state, rift, "cruiser", &a(), 1);
        }
        let before = state.clone();
        let freed = crate::leaders::check_unlocks(
            &mut state,
            ContentStore::embedded(),
            DEFAULT,
            None,
            &a(),
        );
        assert!(!freed.contains(&commander()));
        assert_eq!(
            state, before,
            "a seat with no Cabal commander changes nothing"
        );
    }

    /// Rules for moving into the centre of a hub, as the engine builds them for a real move.
    fn rules_for<'g>(
        state: &GameState,
        hub: &'g crate::fixtures::Hub,
        mover: &PlayerId,
    ) -> crate::movement::MovementRules<'g> {
        let mut rules = crate::movement::MovementRules::new(
            &hub.galaxy,
            ContentStore::embedded(),
            DEFAULT,
            &hub.centre,
            crate::movement::Board::default(),
        );
        crate::action_cards::apply_movement_effects(&mut rules, state, mover);
        rules
    }

    /// A hub whose first outer system holds a Cabal dock, with that dock the only one on the map.
    fn tear_hub() -> (GameState, crate::fixtures::Hub, SystemId) {
        let (dock_system, dock_planet) = crate::fixtures::a_placed_planet();
        let hub = crate::fixtures::hub_with_outer(dock_system.as_str());
        let mut state = bare_cabal();
        // Nobody's fleet is in space: the opponents' home fleets would otherwise sit next to it.
        for seat in state.board.values_mut() {
            seat.units.clear();
        }
        crate::fixtures::put_on_planet(
            &mut state,
            &dock_system,
            &dock_planet,
            "cabal_spacedock",
            &a(),
            1,
        );
        (state, hub, dock_system)
    }

    #[test]
    fn a_dimensional_tear_is_a_gravity_rift_that_only_its_owner_does_not_roll_for() {
        let (state, hub, tear) = tear_hub();
        let path = vec![tear.to_string(), hub.centre.clone()];

        let theirs = rules_for(&state, &hub, &b());
        assert!(
            theirs.is_rift(tear.as_str()),
            "the system is a gravity rift"
        );
        assert_eq!(
            crate::transit::rifts_exited(&theirs, &path),
            vec![tear.to_string()],
            "an opponent leaving the tear rolls for it"
        );
        let mut dice = crate::dice::Dice::from_faces([1u32]);
        let mut rng = crate::rng::GameRng::new(1);
        assert!(
            !crate::transit::survives_gravity_rifts(&mut dice, &mut rng, &theirs, &path),
            "and a 1 destroys the ship"
        );

        let ours = rules_for(&state, &hub, &a());
        assert!(ours.is_rift(tear.as_str()), "still a rift for its owner");
        assert!(
            crate::transit::rifts_exited(&ours, &path).is_empty(),
            "the Cabal's ships do not roll for its own tear"
        );
        let mut dice = crate::dice::Dice::from_faces([1u32]);
        assert!(crate::transit::survives_gravity_rifts(
            &mut dice, &mut rng, &ours, &path
        ));
        assert!(dice.history().is_empty(), "no die was rolled");
    }

    #[test]
    fn a_printed_rift_still_rolls_for_the_cabal() {
        let (state, hub, _tear) = tear_hub();
        let printed = printed_rifts()[0].clone();
        let mut rules = rules_for(&state, &hub, &a());
        rules.gravity_rift_systems.insert(printed.to_string());
        assert!(
            !rules.rift_roll_exempt.contains(printed.as_str()),
            "a printed rift is never exempt"
        );
        let path = vec![printed.to_string(), hub.centre.clone()];
        assert_eq!(
            crate::transit::rifts_exited(&rules, &path),
            vec![printed.to_string()],
            "the exemption is the tear's alone"
        );
    }

    #[test]
    fn a_destroyed_dock_stops_being_a_rift() {
        let (mut state, hub, tear) = tear_hub();
        assert!(rules_for(&state, &hub, &b()).is_rift(tear.as_str()));
        state.system_mut(&tear).planet_units.clear();
        assert!(
            !rules_for(&state, &hub, &b()).is_rift(tear.as_str()),
            "the tear is the unit itself"
        );
    }

    #[test]
    fn a_game_without_a_cabal_seat_has_no_extra_rift() {
        let (dock_system, dock_planet) = crate::fixtures::a_placed_planet();
        let hub = crate::fixtures::hub_with_outer(dock_system.as_str());
        let mut state = crate::fixtures::seated_game(&[("a", "sol"), ("b", "sol")], DEFAULT);
        crate::fixtures::put_on_planet(
            &mut state,
            &dock_system,
            &dock_planet,
            "spacedock",
            &a(),
            1,
        );
        let rules = rules_for(&state, &hub, &b());
        assert!(!rules.is_rift(dock_system.as_str()));
        assert!(rules.gravity_rift_systems.is_empty());
        assert!(rules.rift_roll_exempt.is_empty());
    }

    /// Fighters in a system whose only support is a dock, as `fleet::over_capacity` counts them.
    fn fighters_over(dock: &str, fighters: usize) -> usize {
        let (system, planet) = crate::fixtures::a_placed_planet();
        let mut state = bare_cabal();
        crate::fixtures::put_on_planet(&mut state, &system, &planet, dock, &a(), 1);
        crate::fixtures::put(&mut state, &system, "fighter", &a(), fighters);
        crate::fleet::over_capacity(&state, ContentStore::embedded(), DEFAULT, &a(), &system)
    }

    #[test]
    fn dimensional_tear_i_and_ii_carry_six_and_twelve_fighters_free() {
        assert_eq!(fighters_over("cabal_spacedock", 6), 0, "up to 6 are free");
        assert_eq!(fighters_over("cabal_spacedock", 7), 1, "the seventh is not");
        assert_eq!(
            fighters_over("cabal_spacedock2", 12),
            0,
            "up to 12 are free"
        );
        assert_eq!(
            fighters_over("cabal_spacedock2", 13),
            1,
            "the thirteenth is not"
        );
        assert_eq!(
            fighters_over("spacedock", 6),
            3,
            "an ordinary dock carries only 3: the extra support is the Cabal's"
        );
    }

    #[test]
    fn the_tears_print_production_five_and_seven() {
        let (system, planet) = crate::fixtures::a_placed_planet();
        let content = ContentStore::embedded();
        for (dock, production) in [("cabal_spacedock", 5), ("cabal_spacedock2", 7)] {
            let mut state = bare_cabal();
            state.system_mut(&system).set_control(planet.clone(), a());
            crate::fixtures::put_on_planet(&mut state, &system, &planet, dock, &a(), 1);
            assert_eq!(
                crate::production::capacity(&state, content, DEFAULT, &a(), &system),
                production,
                "{dock}"
            );
        }
    }

    #[test]
    fn researching_dimensional_tear_ii_upgrades_the_dock_on_the_board() {
        let content = ContentStore::embedded();
        let (system, planet) = dock_planet();
        let mut state = bare_cabal();
        crate::fixtures::put_on_planet(&mut state, &system, &planet, "cabal_spacedock", &a(), 1);
        {
            let seat = state.player_mut(&a()).unwrap();
            seat.technologies.remove(&TechnologyId::new("dt2"));
            seat.technologies.insert(TechnologyId::new("st"));
            seat.technologies.insert(TechnologyId::new("sdn"));
        }
        let dt2 = TechnologyId::new("dt2");
        assert!(
            crate::technology::research(&mut state, content, DEFAULT, &a(), &dt2),
            "two yellow technologies pay Dimensional Tear II's prerequisites"
        );
        let docks: Vec<String> = state
            .system_state(&system)
            .planet_units
            .values()
            .flatten()
            .filter(|unit| unit.owner == a())
            .map(|unit| unit.type_id.as_str().to_owned())
            .collect();
        assert_eq!(
            docks,
            vec!["cabal_spacedock2".to_owned()],
            "the sheet card covers the dock"
        );
        assert_eq!(
            tear_systems(&state),
            vec![system.to_string()],
            "and it is still a tear"
        );
    }

    // -- CB-09: It Feeds on Carrion ---------------------------------------------------------------

    fn carrion_leader() -> LeaderId {
        LeaderId::new(HERO)
    }

    fn unlock_hero(state: &mut GameState) {
        state
            .player_mut(&a())
            .unwrap()
            .leaders
            .insert(carrion_leader(), ti4_model::state::LeaderStatus::Unlocked);
    }

    /// Play the hero through the shared leader route with these dice faces loaded; returns whether
    /// it resolved and how many dice were rolled.
    fn play_hero(
        state: &mut GameState,
        galaxy: Option<&ti4_content::galaxy::Galaxy>,
        faces: &[u32],
    ) -> (bool, usize) {
        let mut table = crate::choice::Table::with_default(Box::new(crate::choice::FirstOption));
        let faces = faces.to_vec();
        crate::fixtures::with_context(state, DEFAULT, galaxy, &mut table, |context| {
            *context.dice = crate::dice::Dice::from_faces(faces);
            let done = crate::leaders::use_leader(context, &a(), &carrion_leader());
            (done, context.dice.history().len())
        })
    }

    fn hero_status(state: &GameState) -> Option<ti4_model::state::LeaderStatus> {
        crate::leaders::status(state, &a(), &carrion_leader())
    }

    #[test]
    fn the_hero_is_refused_without_a_map_rather_than_skipping_adjacency() {
        let (mut state, _hub, tear) = tear_hub();
        unlock_hero(&mut state);
        crate::fixtures::put(&mut state, &tear, "cruiser", &b(), 1);
        let before = state.clone();
        let (done, rolled) = play_hero(&mut state, None, &[1]);
        assert!(!done);
        assert_eq!(rolled, 0);
        assert_eq!(state, before, "nothing was rolled, captured or purged");
    }

    #[test]
    fn the_hero_captures_on_a_one_to_three_and_not_on_a_four_or_more_then_is_purged() {
        let (mut state, hub, tear) = tear_hub();
        unlock_hero(&mut state);
        crate::fixtures::put(&mut state, &tear, "cruiser", &b(), 2);
        assert_eq!(
            leader_action(&state, ContentStore::embedded(), &a(), &carrion_leader()),
            Some(true),
            "a tear on the board makes the action legal"
        );

        let (done, rolled) = play_hero(&mut state, Some(&hub.galaxy), &[3, 4]);
        assert!(done);
        assert_eq!(rolled, 2, "one die per ship");
        assert_eq!(
            all_captured(&state),
            vec![row("a", "b", "cruiser")],
            "the 3 captures, the 4 does not"
        );
        assert_eq!(in_space(&state, &tear, &b(), "cruiser").len(), 1);
        assert_eq!(
            hero_status(&state),
            Some(ti4_model::state::LeaderStatus::Purged),
            "then purge this card"
        );
    }

    #[test]
    fn the_hero_reaches_adjacent_systems_only_with_a_map_and_never_rolls_for_fighters_or_its_own() {
        let (mut state, hub, _tear) = tear_hub();
        unlock_hero(&mut state);
        let next_door = SystemId::new(hub.centre.clone());
        crate::fixtures::put(&mut state, &next_door, "destroyer", &b(), 1);
        crate::fixtures::put(&mut state, &next_door, "fighter", &b(), 3);
        crate::fixtures::put(&mut state, &next_door, "cruiser", &a(), 1);
        let far = SystemId::new(hub.outer[3].clone());
        crate::fixtures::put(&mut state, &far, "cruiser", &b(), 1);

        let mut without_map = state.clone();
        let (_, rolled) = play_hero(&mut without_map, None, &[1, 1, 1]);
        assert_eq!(
            rolled, 0,
            "no map, no adjacency: nothing is in the tear's system"
        );
        assert!(all_captured(&without_map).is_empty());

        let (done, rolled) = play_hero(&mut state, Some(&hub.galaxy), &[1, 1, 1]);
        assert!(done);
        assert_eq!(
            rolled, 1,
            "the adjacent destroyer only: no fighters, no own ships, not far away"
        );
        assert_eq!(all_captured(&state), vec![row("a", "b", "destroyer")]);
        assert_eq!(
            in_space(&state, &next_door, &b(), "fighter").len(),
            3,
            "fighters are not rolled for, and a destroyer carries none"
        );
    }

    #[test]
    fn a_captured_carrier_takes_the_fighters_and_ground_forces_that_no_longer_fit() {
        let (mut state, hub, tear) = tear_hub();
        unlock_hero(&mut state);
        crate::fixtures::put(&mut state, &tear, "carrier", &b(), 1);
        crate::fixtures::put(&mut state, &tear, "fighter", &b(), 2);
        crate::fixtures::put(&mut state, &tear, "infantry", &b(), 2);

        let (done, rolled) = play_hero(&mut state, Some(&hub.galaxy), &[1]);
        assert!(done);
        assert_eq!(rolled, 1);
        assert_eq!(
            all_captured(&state),
            vec![
                row("a", "b", "carrier"),
                row("a", "b", "fighter"),
                row("a", "b", "fighter"),
                row("a", "b", "infantry"),
                row("a", "b", "infantry"),
            ],
            "the carrier, and everything it was carrying"
        );
        assert!(state.system_state(&tear).units_of(&b()).is_empty());
    }

    #[test]
    fn a_ship_that_keeps_enough_capacity_keeps_its_cargo() {
        let (mut state, hub, tear) = tear_hub();
        unlock_hero(&mut state);
        crate::fixtures::put(&mut state, &tear, "carrier", &b(), 2);
        crate::fixtures::put(&mut state, &tear, "fighter", &b(), 2);

        // Only one carrier is captured; the other still holds both fighters.
        let (done, _) = play_hero(&mut state, Some(&hub.galaxy), &[2, 6]);
        assert!(done);
        assert_eq!(all_captured(&state), vec![row("a", "b", "carrier")]);
        assert_eq!(in_space(&state, &tear, &b(), "fighter").len(), 2);
    }

    #[test]
    fn the_hero_is_not_offered_without_a_tear_and_a_non_cabal_has_none() {
        let content = ContentStore::embedded();
        let mut state = bare_cabal();
        assert!(tear_systems(&state).is_empty());
        assert_eq!(
            leader_action(&state, content, &a(), &carrion_leader()),
            Some(false),
            "no tear, no action"
        );
        assert_eq!(
            leader_action(&state, content, &a(), &LeaderId::new("solhero")),
            None,
            "not this module's hero"
        );
        unlock_hero(&mut state);
        let before = state.clone();
        let (done, rolled) = play_hero(&mut state, None, &[1]);
        assert!(!done, "refused with no tear");
        assert_eq!(rolled, 0);
        assert_eq!(state, before, "and nothing changed");
    }

    #[test]
    fn a_game_without_a_cabal_seat_cannot_play_the_hero() {
        let content = ContentStore::embedded();
        let mut state = crate::fixtures::seated_game(&[("a", "sol"), ("b", "sol")], DEFAULT);
        let (system, planet) = dock_planet();
        crate::fixtures::put_on_planet(&mut state, &system, &planet, "cabal_spacedock", &a(), 1);
        crate::fixtures::put(&mut state, &system, "cruiser", &b(), 1);
        assert_eq!(
            leader_action(&state, content, &a(), &carrion_leader()),
            Some(false)
        );
        let before = state.clone();
        let (done, rolled) = play_hero(&mut state, None, &[1]);
        assert!(!done);
        assert_eq!(rolled, 0);
        assert_eq!(state, before);
    }

    // -- CB-07: The Stillness of Stars ------------------------------------------------------------

    /// Answers with the option of this id when it is offered, else the first; records every prompt.
    struct Pick {
        id: String,
        asked: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    }

    impl crate::choice::Decider for Pick {
        fn choose(
            &mut self,
            choice: &crate::choice::Choice,
        ) -> Result<ChoiceOption, crate::choice::IllegalChoice> {
            self.asked.lock().unwrap().push(choice.prompt.clone());
            let chosen = choice
                .options
                .iter()
                .find(|option| option.id == self.id)
                .or_else(|| choice.options.first())
                .cloned();
            chosen.ok_or_else(|| crate::choice::IllegalChoice::NoOptions {
                player: choice.player.clone(),
                prompt: choice.prompt.clone(),
            })
        }
    }

    fn agent_status(state: &GameState) -> Option<ti4_model::state::LeaderStatus> {
        crate::leaders::status(state, &a(), &LeaderId::new(AGENT))
    }

    /// The Trade secondary for `player`, then the driver's flush, answering the Cabal's questions
    /// with `decider`. Returns the prompts that were asked.
    fn trade_secondary_then_flush(
        state: &mut GameState,
        player: &PlayerId,
        decider: Box<dyn crate::choice::Decider>,
    ) -> usize {
        let content = ContentStore::embedded();
        let mut table = crate::choice::Table::default();
        crate::strategy_cards::secondary(
            state,
            content,
            DEFAULT,
            None,
            &mut table,
            player,
            "pok5trade",
        )
        .expect("the secondary resolves");
        let staged = crate::supply::staged_events(state);
        let mut resolver = crate::fixtures::armed_resolver(state);
        let mut dice = crate::dice::Dice::new();
        let mut rng = crate::rng::GameRng::new(0);
        let mut sequence = crate::event::EventSequence::new();
        let mut table = crate::choice::Table::with_default(decider);
        let mut resolving = crate::choice::Resolving {
            content,
            sources: DEFAULT,
            dice: &mut dice,
            rng: &mut rng,
            table: &mut table,
            timing: Some(crate::choice::TimingHandle {
                resolver: &mut resolver,
                sequence: &mut sequence,
                galaxy: None,
            }),
        };
        crate::supply::flush_staged_events(state, &mut resolving);
        staged
    }

    fn drained(state: &mut GameState, player: &PlayerId) {
        state.player_mut(player).unwrap().commodities = 0;
    }

    #[test]
    fn a_merchant_station_replenish_is_announced_and_offers_the_agent() {
        let mut state = game();
        drained(&mut state, &b());
        let (system, planet) = crate::fixtures::a_placed_planet();
        state.system_mut(&system).set_control(planet.clone(), b());
        state
            .exploration_decks
            .insert("CULTURAL".to_owned(), vec!["ms1".to_owned()]);
        let content = ContentStore::embedded();
        {
            let mut table =
                crate::choice::Table::with_default(Box::new(crate::choice::Scripted::new([
                    "replenish".to_owned(),
                ])));
            let mut dice = crate::dice::Dice::new();
            let mut rng = crate::rng::GameRng::new(0);
            let mut ctx = crate::choice::Resolving {
                content,
                sources: DEFAULT,
                dice: &mut dice,
                rng: &mut rng,
                table: &mut table,
                timing: None,
            };
            crate::exploration::explore_with(&mut state, &mut ctx, &b(), "CULTURAL", Some(&planet));
        }
        assert!(
            crate::supply::staged_events(&state) >= 1,
            "the Merchant Station replenish was announced"
        );
        let asked = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let mut resolver = crate::fixtures::armed_resolver(&state);
        let mut dice = crate::dice::Dice::new();
        let mut rng = crate::rng::GameRng::new(0);
        let mut sequence = crate::event::EventSequence::new();
        let mut table = crate::choice::Table::with_default(Box::new(Pick {
            id: "dreadnought".to_owned(),
            asked: asked.clone(),
        }));
        let mut resolving = crate::choice::Resolving {
            content,
            sources: DEFAULT,
            dice: &mut dice,
            rng: &mut rng,
            table: &mut table,
            timing: Some(crate::choice::TimingHandle {
                resolver: &mut resolver,
                sequence: &mut sequence,
                galaxy: None,
            }),
        };
        crate::supply::flush_staged_events(&mut state, &mut resolving);
        assert_eq!(
            agent_status(&state),
            Some(ti4_model::state::LeaderStatus::Exhausted),
            "the agent was offered and used after the replenish"
        );
        assert_eq!(all_captured(&state).len(), 1);
    }

    #[test]
    fn another_players_trade_replenish_converts_their_commodities_and_captures_a_unit() {
        let mut state = game();
        drained(&mut state, &b());
        let goods = state.player(&b()).unwrap().trade_goods;
        assert_eq!(
            agent_status(&state),
            Some(ti4_model::state::LeaderStatus::Readied)
        );
        let asked = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let staged = trade_secondary_then_flush(
            &mut state,
            &b(),
            Box::new(Pick {
                id: "dreadnought".to_owned(),
                asked: asked.clone(),
            }),
        );
        assert!(staged >= 1, "the replenish was announced");
        let limit = crate::strategy_cards::commodity_limit(&state, ContentStore::embedded(), &b());
        assert!(limit > 0);
        assert_eq!(state.player(&b()).unwrap().commodities, 0, "converted");
        assert_eq!(
            state.player(&b()).unwrap().trade_goods,
            goods + limit,
            "to trade goods, one for one"
        );
        assert_eq!(
            agent_status(&state),
            Some(ti4_model::state::LeaderStatus::Exhausted)
        );
        let taken = all_captured(&state);
        assert_eq!(taken.len(), 1, "one unit: {taken:?}");
        assert_eq!(taken[0].0, "a");
        assert_eq!(taken[0].1, "b");
        assert!(
            asked
                .lock()
                .unwrap()
                .iter()
                .any(|prompt| prompt.contains("which unit do you capture")),
            "the Cabal player picked the type"
        );
    }

    #[test]
    fn the_agent_takes_only_a_unit_whose_cost_is_within_the_commodity_value() {
        let content = ContentStore::embedded();
        let state = game();
        let cost = |unit: &UnitTypeId| {
            ti4_content::units::unit_type(content, unit.as_str(), DEFAULT)
                .unwrap()
                .cost()
        };
        for value in [0_i64, 1, 3, 4, 8, 20] {
            let found = stillness_targets(&state, content, DEFAULT, &b(), value);
            #[allow(clippy::cast_precision_loss, reason = "tiny test values")]
            let ceiling = value as f64;
            assert!(
                found.iter().all(|unit| cost(unit) <= ceiling),
                "{value}: {found:?}"
            );
        }
        let cheap = stillness_targets(&state, content, DEFAULT, &b(), 1);
        let rich = stillness_targets(&state, content, DEFAULT, &b(), 20);
        assert!(!cheap.is_empty() && cheap.len() < rich.len());
        assert!(
            stillness_targets(&state, content, DEFAULT, &b(), 0).is_empty(),
            "nothing costs nothing"
        );
        // A type with nothing left in the box is not a target.
        let mut empty = game();
        while crate::supply::capture_from_reinforcements(
            &mut empty,
            content,
            DEFAULT,
            &a(),
            &b(),
            &UnitTypeId::new("dreadnought"),
        ) {}
        assert!(
            !stillness_targets(&empty, content, DEFAULT, &b(), 20)
                .iter()
                .any(|unit| unit.as_str().contains("dreadnought"))
        );
    }

    #[test]
    fn declining_the_agent_changes_nothing() {
        let mut state = game();
        drained(&mut state, &b());
        let goods = state.player(&b()).unwrap().trade_goods;
        trade_secondary_then_flush(&mut state, &b(), Box::new(crate::choice::AlwaysDecline));
        let limit = crate::strategy_cards::commodity_limit(&state, ContentStore::embedded(), &b());
        assert_eq!(
            state.player(&b()).unwrap().commodities,
            limit,
            "the replenish stands"
        );
        assert_eq!(state.player(&b()).unwrap().trade_goods, goods);
        assert!(all_captured(&state).is_empty());
        assert_eq!(
            agent_status(&state),
            Some(ti4_model::state::LeaderStatus::Readied)
        );
    }

    #[test]
    fn the_cabals_own_replenish_does_not_trigger_its_agent() {
        let mut state = game();
        drained(&mut state, &a());
        // Anything asked would fail this table.
        trade_secondary_then_flush(
            &mut state,
            &a(),
            Box::new(crate::choice::Scripted::new(["never offered"])),
        );
        assert!(all_captured(&state).is_empty());
        assert_eq!(
            agent_status(&state),
            Some(ti4_model::state::LeaderStatus::Readied)
        );
        assert!(state.player(&a()).unwrap().commodities > 0, "it kept them");
    }

    #[test]
    fn an_exhausted_agent_is_not_offered() {
        let mut state = game();
        drained(&mut state, &b());
        crate::leaders::exhaust(&mut state, &a(), &LeaderId::new(AGENT));
        trade_secondary_then_flush(
            &mut state,
            &b(),
            Box::new(crate::choice::Scripted::new(["never offered"])),
        );
        assert!(all_captured(&state).is_empty());
    }

    #[test]
    fn a_game_without_a_cabal_seat_stages_no_replenish_event() {
        let mut state = crate::fixtures::seated_game(&[("a", "sol"), ("b", "sol")], DEFAULT);
        drained(&mut state, &b());
        let staged = trade_secondary_then_flush(
            &mut state,
            &b(),
            Box::new(crate::choice::Scripted::new(["never offered"])),
        );
        assert_eq!(staged, 0, "nothing is announced");
        assert_eq!(crate::supply::staged_events(&state), 0);
        assert!(all_captured(&state).is_empty());
    }

    // -- CB-10: Crucible --------------------------------------------------------------------------

    /// Announce SYSTEM_ACTIVATED for `player` with this decider answering the optional-play question.
    fn activate_with(
        state: &mut GameState,
        player: &str,
        system: &SystemId,
        decider: Box<dyn crate::choice::Decider>,
    ) {
        let galaxy = crate::fixtures::plain_hub().galaxy;
        let mut resolver = crate::fixtures::armed_resolver(state);
        let mut table = crate::choice::Table::with_default(decider);
        crate::fixtures::with_context(state, DEFAULT, Some(&galaxy), &mut table, |context| {
            let payload: std::collections::BTreeMap<String, serde_json::Value> =
                std::collections::BTreeMap::from([
                    ("player".to_owned(), player.into()),
                    ("system".to_owned(), system.to_string().into()),
                ]);
            let event = context
                .event_sequence
                .next("SYSTEM_ACTIVATED", payload)
                .expect("event id");
            resolver
                .emit_with_context(context, event, |_, _| {})
                .expect("the window resolves");
        });
    }

    fn lend_crucible(state: &mut GameState, to: &PlayerId) {
        state
            .promissory_notes
            .insert(CRUCIBLE_NOTE.to_owned(), to.clone());
    }

    #[test]
    fn the_holder_plays_crucible_after_activating_and_the_note_goes_home() {
        let (mut state, hub, tear) = tear_hub();
        lend_crucible(&mut state, &b());
        state.activation_seq = 4;
        assert!(!rules_for(&state, &hub, &b()).rifts_ignored);

        activate_with(&mut state, "b", &tear, Box::new(crate::choice::FirstOption));

        assert_eq!(
            state.promissory_notes.get(CRUCIBLE_NOTE),
            Some(&a()),
            "then return this card to the Vuil'raith player"
        );
        let rules = rules_for(&state, &hub, &b());
        assert!(rules.rifts_ignored, "no rolls for any rift this movement");
        assert_eq!(rules.rift_extra_steps, 1, "and an additional +1 per route");
        let path = vec![tear.to_string(), hub.centre.clone()];
        let mut dice = crate::dice::Dice::from_faces([1u32]);
        let mut rng = crate::rng::GameRng::new(1);
        assert!(crate::transit::survives_gravity_rifts(
            &mut dice, &mut rng, &rules, &path
        ));
        assert!(dice.history().is_empty());

        // Only this activation: the next one has no Crucible.
        state.activation_seq = 5;
        let later = rules_for(&state, &hub, &b());
        assert!(!later.rifts_ignored && later.rift_extra_steps == 0);
        // And it is the holder's alone.
        assert!(!rules_for(&state, &hub, &a()).rifts_ignored);
    }

    #[test]
    fn declining_keeps_the_note_and_the_rolls() {
        let (mut state, hub, tear) = tear_hub();
        lend_crucible(&mut state, &b());
        activate_with(
            &mut state,
            "b",
            &tear,
            Box::new(crate::choice::AlwaysDecline),
        );
        assert_eq!(state.promissory_notes.get(CRUCIBLE_NOTE), Some(&b()));
        assert!(!rules_for(&state, &hub, &b()).rifts_ignored);
    }

    #[test]
    fn the_cabal_cannot_play_its_own_note_and_only_the_activator_may() {
        let (mut state, hub, tear) = tear_hub();
        // At home: the Cabal activating is offered nothing.
        activate_with(
            &mut state,
            "a",
            &tear,
            Box::new(crate::choice::Scripted::new(["never offered"])),
        );
        assert!(!rules_for(&state, &hub, &a()).rifts_ignored);
        // Lent to b, but a activates: still nothing for a, and b is not the activator.
        lend_crucible(&mut state, &b());
        activate_with(
            &mut state,
            "a",
            &tear,
            Box::new(crate::choice::Scripted::new(["never offered"])),
        );
        assert_eq!(state.promissory_notes.get(CRUCIBLE_NOTE), Some(&b()));
    }

    #[test]
    fn a_game_without_a_cabal_seat_has_no_crucible() {
        let mut state = crate::fixtures::seated_game(&[("a", "sol"), ("b", "sol")], DEFAULT);
        let (system, _) = dock_planet();
        let hub = crate::fixtures::hub_with_outer(system.as_str());
        state.promissory_notes.insert(CRUCIBLE_NOTE.to_owned(), b());
        let before = state.clone();
        activate_with(
            &mut state,
            "b",
            &system,
            Box::new(crate::choice::Scripted::new(["never offered"])),
        );
        assert_eq!(state, before, "a stray note is no Cabal card");
        let rules = rules_for(&state, &hub, &b());
        assert!(!rules.rifts_ignored && rules.rift_extra_steps == 0);
    }

    // -- CB-11: Al'Raith Ix Ianovar ---------------------------------------------------------------

    fn grant_bt(state: &mut GameState) {
        state.player_mut(&a()).unwrap().breakthrough =
            Some(ti4_model::id::BreakthroughId::new(BREAKTHROUGH));
    }

    fn gain_bt(state: &mut GameState, decider: Box<dyn crate::choice::Decider>) {
        let galaxy = crate::fixtures::plain_hub().galaxy;
        let mut resolver = crate::fixtures::armed_resolver(state);
        let mut table = crate::choice::Table::with_default(decider);
        crate::fixtures::with_context(state, DEFAULT, Some(&galaxy), &mut table, |context| {
            let payload: std::collections::BTreeMap<String, serde_json::Value> =
                std::collections::BTreeMap::from([
                    ("player".to_owned(), "a".into()),
                    ("breakthrough".to_owned(), BREAKTHROUGH.into()),
                ]);
            let event = context
                .event_sequence
                .next("BREAKTHROUGH_GAINED", payload)
                .expect("event id");
            resolver
                .emit_with_context(context, event, |_, _| {})
                .expect("the window resolves");
        });
    }

    /// Chooses the first ingress move on offer, never the decline.
    struct FirstMove;

    impl crate::choice::Decider for FirstMove {
        fn choose(
            &mut self,
            choice: &crate::choice::Choice,
        ) -> Result<ChoiceOption, crate::choice::IllegalChoice> {
            choice
                .options
                .iter()
                .find(|option| !option.is_decline())
                .or(choice.options.first())
                .cloned()
                .ok_or_else(|| crate::choice::IllegalChoice::NoOptions {
                    player: choice.player.clone(),
                    prompt: choice.prompt.clone(),
                })
        }
    }

    fn bonus_site<'a>(
        origin: &'a SystemId,
        player: &'a PlayerId,
        ship: &'a ti4_content::units::UnitType<'a>,
    ) -> super::super::hooks_movement::MoveSite<'a> {
        super::super::hooks_movement::MoveSite {
            player,
            origin,
            index: None,
            ship,
        }
    }

    fn rift_ingresses(state: &GameState) -> usize {
        let content = ContentStore::embedded();
        state
            .ingress_tokens
            .iter()
            .filter(|system| is_gravity_rift_system(state, content, DEFAULT, system))
            .count()
    }

    #[test]
    fn the_breakthrough_brings_the_fracture_into_play_without_a_roll_and_moves_ingress_tokens() {
        let mut state = game();
        grant_bt(&mut state);
        assert!(!state.fracture_in_play);
        let before = rift_ingresses(&state);

        gain_bt(&mut state, Box::new(FirstMove));

        assert!(state.fracture_in_play, "no roll was needed");
        let moved = rift_ingresses(&state) - before;
        assert!(
            (1..=INGRESS_MOVES).contains(&moved),
            "up to two tokens now sit in rifts: {moved}"
        );
    }

    #[test]
    fn declining_moves_nothing_but_the_fracture_still_enters() {
        let mut state = game();
        grant_bt(&mut state);
        let before = rift_ingresses(&state);
        gain_bt(&mut state, Box::new(crate::choice::AlwaysDecline));
        assert!(state.fracture_in_play);
        assert_eq!(rift_ingresses(&state), before);
    }

    #[test]
    fn a_fracture_already_in_play_does_not_enter_twice() {
        let mut state = game();
        grant_bt(&mut state);
        let content = ContentStore::embedded();
        crate::fracture::enter_play(&mut state, content, DEFAULT, &[]).expect("enters once");
        let tokens = state.ingress_tokens.len();
        gain_bt(&mut state, Box::new(FirstMove));
        assert!(state.fracture_in_play);
        assert_eq!(
            state.ingress_tokens.len(),
            tokens,
            "tokens move, none are added"
        );
    }

    #[test]
    fn ships_starting_in_the_fracture_get_one_more_move_only_with_the_breakthrough() {
        let content = ContentStore::embedded();
        let cruiser = ti4_content::units::unit_type(content, "cruiser", DEFAULT).unwrap();
        let fracture = crate::fracture::systems(content, DEFAULT)[0].clone();
        let elsewhere = space_system();
        let mut state = game();
        let (me, other) = (a(), b());
        assert_eq!(
            alraith_move_bonus(&state, &bonus_site(&fracture, &me, &cruiser)),
            0,
            "no card yet"
        );
        grant_bt(&mut state);
        assert_eq!(
            alraith_move_bonus(&state, &bonus_site(&fracture, &me, &cruiser)),
            1
        );
        assert_eq!(
            alraith_move_bonus(&state, &bonus_site(&elsewhere, &me, &cruiser)),
            0,
            "only ships that start in the Fracture"
        );
        assert_eq!(
            alraith_move_bonus(&state, &bonus_site(&fracture, &other, &cruiser)),
            0,
            "only the card's holder"
        );
    }

    #[test]
    fn a_game_without_a_cabal_seat_ignores_the_event() {
        let mut state = crate::fixtures::seated_game(&[("a", "sol"), ("b", "sol")], DEFAULT);
        let before = state.clone();
        gain_bt(
            &mut state,
            Box::new(crate::choice::Scripted::new(["never offered"])),
        );
        assert_eq!(state, before);
    }

    #[test]
    fn a_nekro_flagship_with_the_cabal_z_token_captures_losses_in_its_system() {
        let system = space_system();
        let run = |lent: &[&str]| {
            let mut state = crate::fixtures::nekro_with_z(&[("a", "nekro"), ("b", "cabal")], lent);
            state
                .system_mut(&system)
                .units
                .push(Unit::new(UnitTypeId::new("nekro_flagship"), a()));
            emit(
                &mut state,
                "SHIP_DESTROYED",
                &ship_destroyed(&system, "b", "destroyer", false),
            )
            .unwrap();
            captured(&state)
        };
        assert!(run(&[]).is_empty(), "off by default");
        assert_eq!(
            run(&["cabal"]),
            vec![("b".to_owned(), "destroyer".to_owned())]
        );
    }
}
