//! The tactical action: activation (LRR 89.1) and the movement step.
//!
//! Ported from the oracle's `Game._activatable`, `_activate`, `_movable` and `_move_step`.
//!
//! This is the sequence that finally joins [`crate::movement`] (may this ship reach the active
//! system?) to [`crate::transit`] (what happens when it does). Everything after movement —
//! space cannon, combat, invasion, production — is not implemented, and the step stops at a
//! named boundary rather than pretending the action finished.

use ti4_content::ContentStore;
use ti4_content::galaxy::Galaxy;
use ti4_content::units::catalogue;
use ti4_model::content_types::SourceSet;
use ti4_model::id::{PlayerId, SystemId};
use ti4_model::state::{GameState, TokenPool};
use ti4_model::units::Unit;

use crate::choice::{Choice, ChoiceOption, IllegalChoice, validate};
use crate::decision_context::{DecisionContext, DecisionSource};
use crate::movement::{Board, MovementRules};
use crate::preview::{Delta, Preview, Quantity};

/// The choice kind for activating a system.
pub const ACTIVATE_KIND: &str = "activate";
/// The choice kind for moving one ship into the active system.
pub const MOVE_KIND: &str = "move";

/// A tactical action could not be taken.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TacticalError {
    #[error("player {0} has no tactic token to spend")]
    NoTacticToken(PlayerId),
    #[error("player {0} already holds a command token in {1}")]
    AlreadyActivated(PlayerId, SystemId),
    #[error("system {0} is not on the board")]
    UnknownSystem(SystemId),
    #[error("no system is active")]
    NoActiveSystem,
    #[error(transparent)]
    IllegalChoice(#[from] IllegalChoice),
}

/// Systems this player may activate: any without *their own* command token (89.1, 89.1b).
///
/// Another player's token is no obstacle — activating a system they hold is how you attack it.
///
/// Without faction modules this is [`activatable_with`] at any content scope; tests use it.
#[must_use]
pub fn activatable(state: &GameState, galaxy: &Galaxy, player: &PlayerId) -> Vec<SystemId> {
    activatable_with(
        state,
        ContentStore::embedded(),
        ti4_model::content_types::DEFAULT,
        galaxy,
        player,
    )
}

/// [`activatable`] at the game's own content scope, which faction bars need to read the systems
/// (Chaos Mapping asks whether a system is an asteroid field).
#[must_use]
pub fn activatable_with(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: &Galaxy,
    player: &PlayerId,
) -> Vec<SystemId> {
    let held = state.systems_with_token(player);
    let mahact = crate::promissory::has_commander_ability(state, player, "mahactcommander");
    // The galaxy is the *printed map*; the board is what is actually in play. The Fracture adds
    // seven systems after setup and never touches the galaxy, so enumerating the galaxy alone
    // left those tiles reachable -- `movement` already makes an ingress adjacent to each egress
    // -- and unactivatable, which is a state no tactical action can resolve.
    //
    // Ordered and deduplicated: the galaxy first, so the ordinary map keeps the order it had and
    // only the added systems appear after it.
    let mut seen: std::collections::BTreeSet<SystemId> = std::collections::BTreeSet::new();
    let mut found = Vec::new();
    for system in galaxy
        .system_ids_holding_things()
        .into_iter()
        .map(SystemId::new)
        .chain(state.board.keys().cloned())
    {
        // A hyperlane tile is not a system: it is never offered (ruling: may not be moved into).
        if galaxy.is_hyperlane(system.as_str()) {
            continue;
        }
        // Mahact's commander: "During your tactical actions: you can activate systems that contain
        // your command tokens." (The driver then returns both tokens and ends the turn.)
        if (held.contains(&system) && !mahact) || !seen.insert(system.clone()) {
            continue;
        }
        if crate::factions::hooks_movement::cannot_activate(
            state, content, sources, player, &system,
        ) {
            continue; // a faction effect bars it (Chaos Mapping); never offered
        }
        // Artemiris: "Other players must spend 2 influence to activate the system that contains
        // this ship." Not offered to a player who cannot pay; the payment is taken on activation.
        if crate::factions::keleres_units::activation_unpayable(
            state, content, sources, player, &system,
        ) {
            continue;
        }
        found.push(system);
    }
    found
}

/// The activation choice, or `None` when the player cannot take a tactical action.
///
/// 89.1 requires a tactic token to spend, so a player with none is not offered the action at
/// all rather than being offered one they cannot pay for.
#[must_use]
pub fn activation_options(state: &GameState, galaxy: &Galaxy, player: &PlayerId) -> Option<Choice> {
    activation_options_with(
        state,
        ContentStore::embedded(),
        ti4_model::content_types::DEFAULT,
        galaxy,
        player,
    )
}

/// [`activation_options`] at the game's own content scope.
#[must_use]
pub fn activation_options_with(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: &Galaxy,
    player: &PlayerId,
) -> Option<Choice> {
    let tactic_tokens = state
        .player(player)
        .map(|seat| seat.tactic_tokens)
        .filter(|tokens| *tokens > 0)?;
    // OBS-008a1: activating a system spends exactly one tactic token, unconditionally (LRR 89.1) —
    // this mirrors `activate()`'s single `spend_token(TokenPool::Tactic)`, so the preview asserts
    // only what the engine itself does. The cost is the same for every target, but the pool
    // afterwards is a fact the policy would otherwise have to re-derive per option.
    let spend = Preview::certain(vec![Delta::new(
        Quantity::TacticTokens,
        i64::from(tactic_tokens),
        i64::from(tactic_tokens - 1),
    )]);
    let options: Vec<ChoiceOption> = activatable_with(state, content, sources, galaxy, player)
        .into_iter()
        .map(|system| {
            ChoiceOption::labelled(
                system.to_string(),
                ACTIVATE_KIND,
                format!("activate {system}"),
            )
            .previewed(spend.clone())
        })
        .collect();
    if options.is_empty() {
        return None;
    }
    Some(
        Choice::new(player.clone(), "activate a system", options).contextualized(
            DecisionContext::new(
                player.clone(),
                DecisionSource::Rule("89.1".to_owned()),
                "activate_system",
                state.phase,
                state.round,
            ),
        ),
    )
}

/// 89.1: place a tactic token in the system, making it the active system.
///
/// # Errors
/// [`TacticalError::NoTacticToken`] when the player cannot pay, and
/// [`TacticalError::AlreadyActivated`] when they already hold a token there (89.1b).
pub fn activate(
    state: &mut GameState,
    player: &PlayerId,
    system: &SystemId,
) -> Result<(), TacticalError> {
    if state.systems_with_token(player).contains(system)
        && !crate::promissory::has_commander_ability(state, player, "mahactcommander")
    {
        return Err(TacticalError::AlreadyActivated(
            player.clone(),
            system.clone(),
        ));
    }
    let seat = state
        .player_mut(player)
        .ok_or_else(|| TacticalError::NoTacticToken(player.clone()))?;
    if !seat.spend_token(TokenPool::Tactic) {
        return Err(TacticalError::NoTacticToken(player.clone()));
    }

    state.system_mut(system).place_token(player.clone());
    state.active_system = Some(system.clone());
    state.pending = Some("move".to_owned());
    // Bumped so that anything scoped to one activation can tell them apart.
    state.activation_seq = state.activation_seq.saturating_add(1);
    Ok(())
}

/// One ship that could legally move into the active system.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Movable {
    pub origin: SystemId,
    /// Index into `state.ships_of(player, origin)`, so identical ships stay distinguishable.
    pub index: usize,
    pub unit: Unit,
    /// Printed capacity, carried because it is part of the oracle's learned option payload.
    pub capacity: i64,
    /// This otherwise-unreachable move spends Gravity Drive's +1 for the activation.
    pub gravity_drive: bool,
    /// This otherwise-unreachable move exhausts the Ionian Fuel Refinery for its +1.
    ///
    /// Separate from `gravity_drive` because they are different resources on different clocks --
    /// Gravity Drive renews every activation, the legendary card once a round -- and a ship two
    /// hexes short needs both.
    pub ionian: bool,
}

/// Effective move value after per-activation bonuses and Gravleash's established origin anchor.
#[must_use]
pub fn effective_move_value(
    state: &GameState,
    kind: &ti4_content::units::UnitType<'_>,
    player: &PlayerId,
    origin: &SystemId,
) -> i32 {
    effective_move_value_with_gravity(state, kind, player, origin, false)
}

/// Effective move with the optional one-ship Gravity Drive bonus applied before Gravleash.
#[must_use]
pub fn effective_move_value_with_gravity(
    state: &GameState,
    kind: &ti4_content::units::UnitType<'_>,
    player: &PlayerId,
    origin: &SystemId,
    gravity_drive: bool,
) -> i32 {
    effective_move_value_with_boosts(state, kind, player, origin, gravity_drive, false)
}

/// Effective move with both one-ship bonuses, applied before Gravleash.
///
/// Kept beside the Gravity-Drive-only entry point rather than replacing it: most callers know
/// about one boost, and widening their signature would make them all state a fact they do not
/// have.
#[must_use]
pub fn effective_move_value_with_boosts(
    state: &GameState,
    kind: &ti4_content::units::UnitType<'_>,
    player: &PlayerId,
    origin: &SystemId,
    gravity_drive: bool,
    ionian: bool,
) -> i32 {
    effective_move_value_for_ship(state, kind, player, origin, None, gravity_drive, ionian)
}

/// [`effective_move_value_with_boosts`] for the ship at `index` in `state.ships_of(player,
/// origin)`, when the caller knows it, so a faction's one-ship move bonus (Winnu Imperator) can
/// pick it out. Faction modules' `MovementHooks::move_bonus` (Creuss Slipstream) is added here,
/// before Gravleash's anchor; with every module empty it adds 0.
///
/// A caller that offers a move with an `index` must look its route up with the same `index`, or the
/// two disagree about the ship's reach.
#[must_use]
pub fn effective_move_value_for_ship(
    state: &GameState,
    kind: &ti4_content::units::UnitType<'_>,
    player: &PlayerId,
    origin: &SystemId,
    index: Option<usize>,
    gravity_drive: bool,
    ionian: bool,
) -> i32 {
    let own_move = i32::try_from(kind.move_value()).unwrap_or(0)
        + crate::action_cards::move_bonus(state, player, state.activation_seq)
        + i32::from(gravity_drive)
        + i32::from(ionian)
        + crate::factions::hooks_movement::move_bonus(
            state,
            &crate::factions::hooks_movement::MoveSite {
                player,
                origin,
                index,
                ship: kind,
            },
        );
    if state
        .player(player)
        .and_then(|seat| seat.breakthrough.as_ref())
        .is_some_and(|alias| alias.as_str() == "letnevbt")
        && !kind.is_fighter()
    {
        own_move.max(
            state
                .gravleash_move_values
                .get(origin)
                .copied()
                .unwrap_or(0),
        )
    } else {
        own_move
    }
}

/// Every ship that could reach the active system, with where it stands.
///
/// Ships already in the active system are skipped: 58.4e's leave-and-return is legal but is not
/// modelled as a *move option*, matching the oracle.
#[must_use]
pub fn movable(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: &Galaxy,
    player: &PlayerId,
) -> Vec<Movable> {
    let Some(active) = state.active_system.clone() else {
        return Vec::new();
    };
    movable_into(state, content, sources, galaxy, player, &active)
}

/// The same question asked of a system that is **not** activated: what could reach it if it were?
///
/// [`movable`] answers this for `state.active_system`, which is the only system it can be asked
/// about once a tactical action is under way. A policy choosing *which* system to activate needs
/// it one step earlier, for each candidate, and the activation is not reversible enough to try.
///
/// Splitting it out rather than cloning and mutating the state keeps the answer exact: the same
/// movement rules, the same action-card effects, the same gravity-drive fallback. The activation
/// target enters `MovementRules` as a destination and nothing else, so asking about a hypothetical
/// one is not a simulation — it is the same computation with a different argument.
#[must_use]
pub fn movable_into(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: &Galaxy,
    player: &PlayerId,
    active: &SystemId,
) -> Vec<Movable> {
    let types = catalogue(content, sources);
    let board = Board::for_player(state, content, sources, player);
    let mut rules = MovementRules::with_laws(
        galaxy,
        content,
        sources,
        active.as_str(),
        board,
        Some(state),
    );
    crate::action_cards::apply_movement_effects(&mut rules, state, player);

    let mut found = Vec::new();
    for origin in state.systems_with_units_of(player) {
        if origin == active {
            continue;
        }
        for (index, hull) in state.ships_of(player, origin).into_iter().enumerate() {
            let Some(kind) = types.get(hull.type_id.as_str()) else {
                continue;
            };
            // Ships, and the Floating Factory ("can move and retreat as if it were a ship").
            if !kind.moves_as_ship() {
                continue;
            }
            let move_value = effective_move_value_for_ship(
                state,
                kind,
                player,
                origin,
                Some(index),
                false,
                false,
            );
            let ship_type = Some(hull.type_id.as_str());
            // Cheapest boost first, and only enough of it to arrive. Gravity Drive is preferred
            // over the Ionian Fuel Refinery when either alone suffices: the drive renews with
            // every activation, the legendary card only in the status phase, so spending the
            // renewable one is strictly the smaller commitment.
            let gravity = crate::technology::gravity_drive_available(state, player);
            let ionian = crate::legendary::ionian_available(state, player);
            let boosts = if rules.can_reach_ship(origin.as_str(), move_value, ship_type) {
                Some((false, false))
            } else {
                [(true, false), (false, true), (true, true)]
                    .into_iter()
                    .filter(|(gd, ion)| (!gd || gravity) && (!ion || ionian))
                    .find(|(gd, ion)| {
                        rules.can_reach_ship(
                            origin.as_str(),
                            effective_move_value_for_ship(
                                state,
                                kind,
                                player,
                                origin,
                                Some(index),
                                *gd,
                                *ion,
                            ),
                            ship_type,
                        )
                    })
            };
            if let Some((gravity_drive, ionian)) = boosts {
                found.push(Movable {
                    origin: origin.clone(),
                    index,
                    unit: hull.clone(),
                    capacity: kind.capacity(),
                    gravity_drive,
                    ionian,
                });
            }
        }
    }
    found
}

/// The movement-step choice: one option per distinguishable move, plus "finish movement".
///
/// **One option per distinguishable move, not per hull.** Three cruisers in one system are
/// three ways to write the same move, and the copies are not free: a sampling decider draws per
/// option, so a move written three times drew three times the weight of an equally good one
/// written once — its tie-break was counting hulls rather than weighing moves.
///
/// Damage stays in the key *and* the label. A damaged and an undamaged dreadnought in the same
/// system are genuinely different moves — you would rather advance the fresh one — but both
/// read "move dreadnought from 01", so nothing choosing between them could see which was which.
#[must_use]
pub fn movement_options(player: &PlayerId, movable: &[Movable]) -> Choice {
    let mut seen = std::collections::BTreeSet::new();
    let mut options: Vec<ChoiceOption> = Vec::new();
    for candidate in movable {
        let key = (
            candidate.unit.type_id.to_string(),
            candidate.unit.sustained_damage,
            candidate.origin.to_string(),
            candidate.gravity_drive,
            candidate.ionian,
        );
        if !seen.insert(key) {
            continue;
        }
        let damaged = if candidate.unit.sustained_damage {
            " (damaged)"
        } else {
            ""
        };
        let verb = match (candidate.gravity_drive, candidate.ionian) {
            (false, false) => "move",
            (true, false) => "move_gd",
            (false, true) => "move_ion",
            (true, true) => "move_gd_ion",
        };
        let spent = match (candidate.gravity_drive, candidate.ionian) {
            (false, false) => "",
            (true, false) => " using Gravity Drive",
            (false, true) => " using the Ionian Fuel Refinery",
            (true, true) => " using Gravity Drive and the Ionian Fuel Refinery",
        };
        let mut option = ChoiceOption::labelled(
            format!("{verb}|{}|{}", candidate.origin, candidate.index),
            MOVE_KIND,
            format!(
                "move {}{damaged} from {}{spent}",
                candidate.unit.type_id, candidate.origin,
            ),
        )
        .with("origin", candidate.origin.to_string())
        .with("unit", candidate.unit.type_id.to_string())
        .with("damaged", candidate.unit.sustained_damage)
        .with("capacity", candidate.capacity)
        .with("gravity_drive", candidate.gravity_drive);
        // Attached only when it is true, unlike `gravity_drive` above. `ionian` is a new payload
        // key, so it is out of vocabulary on every bundle trained before it existed and falls into
        // its family's shared OOV column. Attaching it unconditionally would fire that column on
        // every move option in the game and on none of the "finish movement" options beside them,
        // which is a constant nudge for or against finishing -- carried by a weight trained for
        // something else entirely. Attached only where it is true, seats without Tempesta see the
        // options they saw before, byte for byte.
        if candidate.ionian {
            option = option.with("ionian", true);
        }
        options.push(option);
    }
    // 89.2b: the player may choose to move nothing.
    options.push(ChoiceOption::labelled(
        "done_moving",
        crate::choice::DECLINE_KIND,
        "finish movement",
    ));
    Choice::new(player.clone(), "movement", options)
}

/// Attach to each ship-move option the exact fleet-supply and transport change the ship makes on
/// arrival in the active system (OBS-008a2, LRR 37 and 16).
///
/// Deterministic and destination-only: what a move costs the origin is not the fleet a seat is
/// building, and the arrival is what the two limits are enforced against a turn later. Shares
/// [`crate::fleet::standing_using`] with production placement and end-of-turn enforcement, so a
/// preview of the arrival and the removal that may follow it cannot drift apart. The "finish
/// movement" option keeps no preview: ending the step has no bounded quantity.
#[must_use]
pub fn preview_moves(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: &Galaxy,
    player: &PlayerId,
    active: &SystemId,
    mut choice: Choice,
) -> Choice {
    let types = catalogue(content, sources);
    let before = crate::fleet::standing_using(&types, state, content, player, active, None);
    for option in &mut choice.options {
        let Some(kind) = option
            .payload
            .get("unit")
            .and_then(serde_json::Value::as_str)
            .and_then(|id| types.get(id).copied())
        else {
            continue;
        };
        let after = crate::fleet::standing_using(
            &types,
            state,
            content,
            player,
            active,
            Some(crate::fleet::Arrival {
                kind,
                count: 1,
                in_space: true,
            }),
        );
        // `sail` follows the deterministic movement path, then rolls once for every rift it
        // exits. An arrival cannot be presented as certain when that route can destroy the ship.
        let origin = option
            .payload
            .get("origin")
            .and_then(serde_json::Value::as_str);
        let gravity_drive = option
            .payload
            .get("gravity_drive")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        let ionian = option
            .payload
            .get("ionian")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        let kind_id = option
            .payload
            .get("unit")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        let route_exits_rift = origin.is_some_and(|origin| {
            let mut rules = MovementRules::with_laws(
                galaxy,
                content,
                sources,
                active.as_str(),
                Board::for_player(state, content, sources, player),
                Some(state),
            );
            crate::action_cards::apply_movement_effects(&mut rules, state, player);
            let path = rules.path_from_ship(
                origin,
                effective_move_value_with_boosts(
                    state,
                    &kind,
                    player,
                    &SystemId::new(origin),
                    gravity_drive,
                    ionian,
                ),
                Some(kind_id),
            );
            !rules.anomalies_ignored
                && !rules.rifts_ignored
                && path.is_some_and(|path| !crate::transit::rifts_exited(&rules, &path).is_empty())
        });
        if route_exits_rift {
            option.preview = Some(Preview::unknown("gravity-rift survival is unresolved"));
            continue;
        }
        option.preview = Some(Preview::certain(vec![
            Delta::new(
                Quantity::FleetSupplyHeadroom,
                before.fleet_headroom(),
                after.fleet_headroom(),
            ),
            Delta::new(
                Quantity::CapacityFree,
                before.capacity_free(),
                after.capacity_free(),
            ),
        ]));
    }
    choice
}

/// What a movement-step answer asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MoveSelection {
    /// Move this ship, identified by where it stands.
    Ship {
        origin: SystemId,
        index: usize,
        gravity_drive: bool,
        /// Exhausts the Ionian Fuel Refinery for this ship's +1.
        ionian: bool,
    },
    /// 89.2b: move nothing further.
    Done,
}

/// Read a movement-step answer, validating it against the offered options first.
///
/// # Errors
/// [`TacticalError::IllegalChoice`] when the answer was not offered.
pub fn read_move(choice: &Choice, answer: ChoiceOption) -> Result<MoveSelection, TacticalError> {
    let option = validate(choice, answer)?;
    if option.is_decline() {
        return Ok(MoveSelection::Done);
    }
    let mut parts = option.id.splitn(3, '|');
    let (Some(verb), Some(origin), Some(index)) = (parts.next(), parts.next(), parts.next()) else {
        return Err(IllegalChoice::NotOffered {
            player: choice.player.clone(),
            chosen: option.id.clone(),
            offered: choice.ids().into_iter().map(str::to_owned).collect(),
        }
        .into());
    };
    index.parse().map_or_else(
        |_| {
            Err(IllegalChoice::NotOffered {
                player: choice.player.clone(),
                chosen: option.id.clone(),
                offered: choice.ids().into_iter().map(str::to_owned).collect(),
            }
            .into())
        },
        |index| {
            Ok(MoveSelection::Ship {
                origin: SystemId::new(origin),
                index,
                gravity_drive: matches!(verb, "move_gd" | "move_gd_ion"),
                ionian: matches!(verb, "move_ion" | "move_gd_ion"),
            })
        },
    )
}

#[cfg(test)]
mod tests {

    #[test]
    fn a_floating_factory_is_offered_as_a_mover_and_an_ordinary_dock_is_not() {
        let hub = crate::fixtures::plain_hub();
        let player = PlayerId::new("a");
        let origin = SystemId::new(hub.outer[0].clone());
        let target = SystemId::new(hub.centre.clone());
        let mut state = crate::fixtures::game(&["a"]);
        crate::fixtures::put(&mut state, &origin, "saar_spacedock", &player, 1);
        crate::fixtures::put(&mut state, &origin, "spacedock", &player, 1);
        activate(&mut state, &player, &target).unwrap();
        let found = movable(&state, ContentStore::embedded(), POK, &hub.galaxy, &player);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].unit.type_id.as_str(), "saar_spacedock");
        assert_eq!(found[0].capacity, 4, "it carries like a ship");
    }

    #[test]
    fn a_floating_factory_out_of_range_is_not_offered() {
        let hub = crate::fixtures::plain_hub();
        let player = PlayerId::new("a");
        let origin = SystemId::new(hub.outer[0].clone());
        let far = SystemId::new(hub.across(&hub.outer[0]));
        let mut state = crate::fixtures::game(&["a"]);
        crate::fixtures::put(&mut state, &origin, "saar_spacedock", &player, 1);
        activate(&mut state, &player, &far).unwrap();
        // Move 1 reaches the centre but not the far side of it.
        assert!(movable(&state, ContentStore::embedded(), POK, &hub.galaxy, &player).is_empty());
    }

    #[test]
    fn nav_suite_opens_a_supernova_that_barred_the_route() {
        // The consumer again: setting `anomalies_ignored_activation` and never applying it to
        // the rules leaves the card doing nothing, and a field check cannot tell.
        let supernova = crate::fixtures::a_system_where("supernova");
        let hub = crate::fixtures::hub_with_centre(&supernova);
        let player = PlayerId::new("a");
        let origin = SystemId::new(hub.outer[0].clone());
        let target = SystemId::new(hub.centre.clone());

        let mut state = crate::fixtures::game(&["a"]);
        crate::fixtures::put(&mut state, &origin, "cruiser", &player, 1);
        activate(&mut state, &player, &target).unwrap();

        let reach = |state: &GameState| {
            movable(state, ContentStore::embedded(), POK, &hub.galaxy, &player).len()
        };

        assert_eq!(reach(&state), 0, "a supernova bars the way");

        state
            .player_mut(&player)
            .unwrap()
            .anomalies_ignored_activation = Some(state.activation_seq);
        assert_eq!(
            reach(&state),
            1,
            "Nav Suite ignores the effect of anomalies"
        );

        state
            .player_mut(&player)
            .unwrap()
            .anomalies_ignored_activation = Some(state.activation_seq + 1);
        assert_eq!(reach(&state), 0, "and only for this tactical action");
    }

    #[test]
    fn in_the_silence_of_space_frees_only_the_system_it_named() {
        // "Your ships in the chosen system." The permission follows where the ships start, so
        // naming a different origin must leave this one blocked.
        //
        // One ship, deliberately. A second fleet elsewhere on the ring is not a control: the
        // ring is itself a route, so its path to the far seat never crosses the blocked centre
        // and it would be movable whatever this card says.
        let hub = crate::fixtures::plain_hub();
        let mine = PlayerId::new("a");
        let theirs = PlayerId::new("b");
        let near = SystemId::new(hub.outer[0].clone());
        let far = SystemId::new(hub.across(&hub.outer[0]));

        let mut state = crate::fixtures::game(&["a", "b"]);
        crate::fixtures::put(&mut state, &near, "cruiser", &mine, 1);
        crate::fixtures::put(
            &mut state,
            &SystemId::new(hub.centre.clone()),
            "cruiser",
            &theirs,
            1,
        );
        activate(&mut state, &mine, &far).unwrap();

        let can_move = |state: &GameState| {
            !movable(state, ContentStore::embedded(), POK, &hub.galaxy, &mine).is_empty()
        };

        assert!(
            !can_move(&state),
            "an enemy fleet in the centre blocks the way"
        );

        let activation = state.activation_seq;
        let seat = state.player_mut(&mine).unwrap();
        seat.silence_activation = Some(activation);
        seat.silence_system = Some(near.clone());
        assert!(can_move(&state), "the named system may pass the blockade");

        let other = hub
            .outer
            .iter()
            .find(|id| **id != hub.outer[0])
            .unwrap()
            .clone();
        state.player_mut(&mine).unwrap().silence_system = Some(SystemId::new(other));
        assert!(
            !can_move(&state),
            "an origin that was not named is still blocked"
        );
    }

    #[test]
    fn flank_speed_puts_a_system_in_reach_that_was_not() {
        // The consumer, not the field. Setting `move_bonus_activation` and never reading it
        // leaves the card doing nothing, and a test that only checks the field cannot tell.
        let hub = crate::fixtures::plain_hub();
        let player = PlayerId::new("a");
        let origin = SystemId::new(hub.outer[0].clone());
        let far = SystemId::new(hub.across(&hub.outer[0]));

        let mut state = crate::fixtures::game(&["a"]);
        // A carrier moves 1; the far seat is two systems away across the ring.
        crate::fixtures::put(&mut state, &origin, "carrier", &player, 1);
        activate(&mut state, &player, &far).unwrap();

        let reach = |state: &GameState| {
            movable(state, ContentStore::embedded(), POK, &hub.galaxy, &player).len()
        };

        assert_eq!(reach(&state), 0, "a carrier cannot cross two systems");

        state.player_mut(&player).unwrap().move_bonus_activation = Some(state.activation_seq);
        assert_eq!(reach(&state), 1, "Flank Speed carries it one further");

        // And only for the activation it was played in.
        state.player_mut(&player).unwrap().move_bonus_activation = Some(state.activation_seq + 1);
        assert_eq!(
            reach(&state),
            0,
            "a later activation's bonus is not this one's"
        );
    }

    /// The Ionian Fuel Refinery buys the same reach Gravity Drive does, from a different clock.
    ///
    /// The two are separate resources, so a seat holding only Tempesta gets the boosted move on
    /// its own -- and once the card is exhausted the move stops being offered, which is what makes
    /// it "1 of your ships" rather than all of them.
    #[test]
    fn the_ionian_fuel_refinery_reaches_where_the_hull_alone_cannot() {
        let hub = crate::fixtures::plain_hub();
        let player = PlayerId::new("a");
        let origin = SystemId::new(hub.outer[0].clone());
        let far = SystemId::new(hub.across(&hub.outer[0]));
        let mut state = crate::fixtures::game(&["a"]);
        crate::fixtures::put(&mut state, &origin, "carrier", &player, 1);
        // Tempesta, held somewhere else entirely: the card is a possession, not a place.
        let home = SystemId::new(hub.centre.clone());
        state.board.entry(home.clone()).or_default();
        state
            .system_mut(&home)
            .set_control(ti4_model::id::PlanetId::new("tempesta"), player.clone());
        activate(&mut state, &player, &far).unwrap();

        let found = movable(&state, ContentStore::embedded(), POK, &hub.galaxy, &player);
        assert_eq!(found.len(), 1);
        assert!(found[0].ionian && !found[0].gravity_drive, "{found:?}");
        assert_eq!(
            movement_options(&player, &found).options[0].id,
            format!("move_ion|{origin}|0")
        );

        // Exhausted, the reach is gone: one ship, once a round.
        state
            .player_mut(&player)
            .unwrap()
            .exhausted_legendary
            .insert(ti4_model::id::PlanetId::new("tempesta"));
        assert!(
            movable(&state, ContentStore::embedded(), POK, &hub.galaxy, &player).is_empty(),
            "the boosted move is not offered a second time"
        );
    }

    #[test]
    fn gravity_drive_offers_only_the_one_ship_that_needs_its_bonus() {
        let hub = crate::fixtures::plain_hub();
        let player = PlayerId::new("a");
        let origin = SystemId::new(hub.outer[0].clone());
        let far = SystemId::new(hub.across(&hub.outer[0]));
        let mut state = crate::fixtures::game(&["a"]);
        crate::fixtures::put(&mut state, &origin, "carrier", &player, 1);
        state
            .player_mut(&player)
            .unwrap()
            .technologies
            .insert(ti4_model::id::TechnologyId::new("gd"));
        activate(&mut state, &player, &far).unwrap();

        let found = movable(&state, ContentStore::embedded(), POK, &hub.galaxy, &player);
        assert_eq!(found.len(), 1);
        assert!(found[0].gravity_drive);
        assert_eq!(
            movement_options(&player, &found).options[0].id,
            format!("move_gd|{origin}|0")
        );
    }

    #[test]
    fn an_ionian_boost_does_not_hide_a_gravity_rift_in_the_route_preview() {
        let content = ContentStore::embedded();
        let rift = crate::fixtures::a_system_where("gravity rift");
        let plain = crate::fixtures::plain_systems(3);
        let galaxy = Galaxy::placed(
            content,
            &[
                (rift.as_str(), ti4_model::hex::Hex::new(0, 0)),
                (plain[0].as_str(), ti4_model::hex::Hex::new(1, 0)),
                (plain[1].as_str(), ti4_model::hex::Hex::new(2, 0)),
                (plain[2].as_str(), ti4_model::hex::Hex::new(3, 0)),
            ],
            POK,
        )
        .unwrap();
        let player = player();
        let origin = SystemId::new(rift);
        let destination = SystemId::new(&plain[2]);
        let mut state = crate::fixtures::game(&["a"]);
        crate::fixtures::put(&mut state, &origin, "carrier", &player, 1);
        state
            .system_mut(&origin)
            .set_control(ti4_model::id::PlanetId::new("tempesta"), player.clone());
        activate(&mut state, &player, &destination).unwrap();
        let moves = movable(&state, content, POK, &galaxy, &player);
        assert_eq!(moves.len(), 1);
        assert!(moves[0].ionian);
        let choice = preview_moves(
            &state,
            content,
            POK,
            &galaxy,
            &player,
            &destination,
            movement_options(&player, &moves),
        );
        assert!(matches!(
            choice.options[0].preview.as_ref().unwrap().outcome,
            crate::preview::Outcome::Unknown { .. }
        ));
    }

    use ti4_model::content_types::POK;
    use ti4_model::id::UnitTypeId;

    use super::*;
    use crate::setup::start_game;

    fn player() -> PlayerId {
        PlayerId::new("a")
    }

    fn plain_systems(count: usize) -> Vec<String> {
        ti4_content::galaxy::all_systems(ContentStore::embedded(), POK)
            .iter()
            .filter(|(_, system)| !system.is_anomaly() && !system.is_hyperlane())
            .map(|(id, _)| (*id).to_owned())
            .take(count)
            .collect()
    }

    fn fixture() -> (GameState, Galaxy, Vec<SystemId>) {
        let players = [player(), PlayerId::new("b")];
        let state = start_game(ContentStore::embedded(), &players, POK, None).unwrap();
        let ids = plain_systems(7);
        let refs: Vec<&str> = ids.iter().map(String::as_str).collect();
        let galaxy = Galaxy::build(ContentStore::embedded(), &refs, POK, 1).unwrap();
        (state, galaxy, ids.into_iter().map(SystemId::new).collect())
    }

    fn ship(kind: &str) -> Unit {
        Unit::new(UnitTypeId::new(kind), player())
    }

    #[test]
    fn every_system_without_your_own_token_may_be_activated() {
        // 89.1b bars only *your own* token. Another player's is no obstacle — activating a
        // system they hold is how you attack it.
        let (mut state, galaxy, ids) = fixture();
        state.system_mut(&ids[0]).place_token(player());
        state.system_mut(&ids[1]).place_token(PlayerId::new("b"));

        let options = activatable(&state, &galaxy, &player());

        assert!(!options.contains(&ids[0]), "your own token bars it");
        assert!(
            options.contains(&ids[1]),
            "an opponent's token does not - that is the attack"
        );
        assert_eq!(options.len(), 6, "seven tiles, one of them yours");
    }

    fn a_hyperlane_id() -> String {
        ti4_content::galaxy::all_systems(ContentStore::embedded(), POK)
            .iter()
            .find(|(_, system)| system.is_hyperlane())
            .map(|(id, _)| (*id).to_owned())
            .expect("the corpus has hyperlane tiles")
    }

    #[test]
    fn a_hyperlane_tile_is_never_offered_for_activation() {
        // Ruling: hyperlanes may not be moved into, so they cannot be activated either.
        let mut ids = plain_systems(6);
        let lane = a_hyperlane_id();
        ids.push(lane.clone());
        let refs: Vec<&str> = ids.iter().map(String::as_str).collect();
        let galaxy = Galaxy::build(ContentStore::embedded(), &refs, POK, 1).unwrap();
        let state = start_game(
            ContentStore::embedded(),
            &[player(), PlayerId::new("b")],
            POK,
            None,
        )
        .unwrap();

        let options = activatable(&state, &galaxy, &player());

        assert!(!options.contains(&SystemId::new(&lane)), "{options:?}");
        assert_eq!(options.len(), 6, "the six real systems stay offered");
        let choice = activation_options(&state, &galaxy, &player()).unwrap();
        assert!(choice.options.iter().all(|option| option.id != lane));
    }

    #[test]
    fn a_player_without_a_tactic_token_is_not_offered_the_action() {
        let (mut state, galaxy, _) = fixture();
        state.player_mut(&player()).unwrap().tactic_tokens = 0;

        assert!(activation_options(&state, &galaxy, &player()).is_none());
    }

    /// OBS-003d: the activation choice names its rule and stays constant across an unrelated
    /// state change, so a policy sees "this is 89.1" rather than inferring it from the prompt.
    #[test]
    fn obs003d_activation_carries_its_typed_context() {
        let (state, galaxy, _) = fixture();
        let choice = activation_options(&state, &galaxy, &player()).expect("offered");
        let context = choice.context.as_ref().expect("typed context");
        assert_eq!(
            context.source,
            crate::decision_context::DecisionSource::Rule("89.1".to_owned())
        );
        assert_eq!(context.subtype, "activate_system");
        assert_eq!(context.actor, player());
    }

    #[test]
    fn activating_spends_a_token_and_places_it() {
        let (mut state, _, ids) = fixture();
        let before = state.player(&player()).unwrap().tactic_tokens;

        activate(&mut state, &player(), &ids[0]).unwrap();

        assert_eq!(
            state.player(&player()).unwrap().tactic_tokens,
            before - 1,
            "89.1 spends a tactic token"
        );
        assert!(
            state
                .system_state(&ids[0])
                .command_tokens
                .contains(&player())
        );
        assert_eq!(state.active_system, Some(ids[0].clone()));
        assert_eq!(state.pending.as_deref(), Some("move"));
        assert_eq!(state.activation_seq, 1);
    }

    /// OBS-008a1: every activation option previews the exact command-token consequence, and the
    /// preview agrees with actually activating and re-measuring — two independent computations, not
    /// one derived from the other. Option identity and the legal set are untouched.
    #[test]
    fn obs008a1_activation_options_preview_the_exact_tactic_token_spend() {
        let (mut state, galaxy, ids) = fixture();
        // A non-default pool so the test pins "one less", not "two".
        state.player_mut(&player()).unwrap().tactic_tokens = 4;

        let choice = activation_options(&state, &galaxy, &player()).expect("offered");
        let ids_before: Vec<String> = choice.options.iter().map(|o| o.id.clone()).collect();

        for option in &choice.options {
            let preview = option.preview.as_ref().expect("every option is previewed");
            assert_eq!(
                preview.outcome,
                crate::preview::Outcome::Certain {
                    deltas: vec![Delta::new(Quantity::TacticTokens, 4, 3)],
                },
                "activating spends exactly one tactic token (LRR 89.1)"
            );
        }

        // The preview is a claim about `activate()`; check it against `activate()`.
        let target = choice
            .options
            .iter()
            .map(|o| SystemId::new(o.id.clone()))
            .find(|system| *system != ids[0])
            .expect("a target to activate");
        activate(&mut state, &player(), &target).unwrap();
        assert_eq!(
            state.player(&player()).unwrap().tactic_tokens,
            3,
            "the pool after activating is exactly what the preview said"
        );

        // Attaching a preview changes nothing a replay or the legal set reads.
        let again = activation_options(&fixture().0, &galaxy, &player()).expect("offered");
        let ids_after: Vec<String> = again.options.iter().map(|o| o.id.clone()).collect();
        assert_eq!(ids_before, ids_after, "option identity is unchanged");
        assert_eq!(again.options[0].label, format!("activate {}", ids_after[0]));
    }

    /// OBS-008a1: a seat that cannot pay is still not offered the action, so the preview never has
    /// to describe an activation from a zero pool.
    #[test]
    fn obs008a1_no_activation_choice_means_no_preview_to_get_wrong() {
        let (mut state, galaxy, _) = fixture();
        state.player_mut(&player()).unwrap().tactic_tokens = 0;
        assert!(activation_options(&state, &galaxy, &player()).is_none());
    }

    /// OBS-008a2: each ship-move option states the exact fleet-supply and transport change the ship
    /// makes on arrival in the active system, and the previewed `after` matches `fleet::standing`
    /// recomputed once the ship is actually there.
    #[test]
    #[expect(
        clippy::too_many_lines,
        reason = "one fixture, the per-option loop, and the real-arrival agreement check stay together"
    )]
    fn obs008a2_move_options_preview_the_arriving_ships_fleet_and_capacity_effect() {
        let hub = crate::fixtures::plain_hub();
        let player = PlayerId::new("a");
        let origin = SystemId::new(hub.outer[0].clone());
        let active = SystemId::new(hub.centre.clone());

        let mut state = crate::fixtures::game(&["a"]);
        crate::fixtures::put(&mut state, &origin, "carrier", &player, 1);
        crate::fixtures::put(&mut state, &origin, "cruiser", &player, 1);
        activate(&mut state, &player, &active).unwrap();

        let moves = movable(&state, ContentStore::embedded(), POK, &hub.galaxy, &player);
        let choice = preview_moves(
            &state,
            ContentStore::embedded(),
            POK,
            &hub.galaxy,
            &player,
            &active,
            movement_options(&player, &moves),
        );

        let carrier_capacity =
            ti4_content::units::unit_type(ContentStore::embedded(), "carrier", POK)
                .expect("carrier is a unit")
                .capacity();

        let mut saw_carrier = false;
        let mut saw_cruiser = false;
        for option in &choice.options {
            let Some(unit) = option
                .payload
                .get("unit")
                .and_then(serde_json::Value::as_str)
            else {
                assert!(option.is_decline(), "only the decline option lacks a unit");
                assert!(
                    option.preview.is_none(),
                    "finishing movement has no preview"
                );
                continue;
            };
            let deltas = match &option
                .preview
                .as_ref()
                .expect("a move is previewed")
                .outcome
            {
                crate::preview::Outcome::Certain { deltas } => deltas.clone(),
                other => panic!("a move preview is certain, got {other:?}"),
            };
            let change = |quantity| {
                deltas
                    .iter()
                    .find(|delta| delta.quantity == quantity)
                    .map(Delta::change)
                    .expect("the preview names this quantity")
            };
            assert_eq!(
                change(Quantity::FleetSupplyHeadroom),
                -1,
                "a non-fighter ship costs one of the active system's fleet supply"
            );
            match unit {
                "carrier" => {
                    saw_carrier = true;
                    assert_eq!(change(Quantity::CapacityFree), carrier_capacity);
                }
                "cruiser" => {
                    saw_cruiser = true;
                    assert_eq!(change(Quantity::CapacityFree), 0);
                }
                other => panic!("unexpected movable unit {other}"),
            }
        }
        assert!(saw_carrier && saw_cruiser, "both ships offered a move");

        // The previewed `after` is a claim about the real position once the ship is there.
        let carrier_after = {
            let choice = preview_moves(
                &state,
                ContentStore::embedded(),
                POK,
                &hub.galaxy,
                &player,
                &active,
                movement_options(&player, &moves),
            );
            let option = choice
                .options
                .iter()
                .find(|option| {
                    option
                        .payload
                        .get("unit")
                        .and_then(serde_json::Value::as_str)
                        == Some("carrier")
                })
                .expect("the carrier move");
            match &option.preview.as_ref().unwrap().outcome {
                crate::preview::Outcome::Certain { deltas } => {
                    deltas
                        .iter()
                        .find(|delta| delta.quantity == Quantity::CapacityFree)
                        .unwrap()
                        .after
                }
                _ => unreachable!(),
            }
        };
        crate::fixtures::put(&mut state, &active, "carrier", &player, 1);
        let standing = crate::fleet::standing(
            &state,
            ContentStore::embedded(),
            POK,
            &player,
            &active,
            None,
        );
        assert_eq!(
            standing.capacity_free(),
            carrier_after,
            "the capacity the preview promised is the capacity the arrival actually leaves"
        );
    }

    #[test]
    fn a_system_you_already_hold_cannot_be_activated_again() {
        // 89.1b, and the state must not move on the refusal.
        let (mut state, _, ids) = fixture();
        activate(&mut state, &player(), &ids[0]).unwrap();
        let settled = state.clone();

        assert_eq!(
            activate(&mut state, &player(), &ids[0]),
            Err(TacticalError::AlreadyActivated(player(), ids[0].clone()))
        );
        assert!(state.identical(&settled));
    }

    #[test]
    fn the_mahact_commander_lets_a_held_system_be_activated_again() {
        let (mut state, galaxy, ids) = fixture();
        state.player_mut(&player()).unwrap().tactic_tokens = 2;
        activate(&mut state, &player(), &ids[0]).unwrap();
        assert!(!activatable(&state, &galaxy, &player()).contains(&ids[0]));
        assert!(crate::promissory::grant_commander_ability(
            &mut state,
            ContentStore::embedded(),
            &player(),
            "mahactcommander"
        ));
        assert!(activatable(&state, &galaxy, &player()).contains(&ids[0]));
        let tokens = state.player(&player()).unwrap().tactic_tokens;
        activate(&mut state, &player(), &ids[0]).unwrap();
        assert_eq!(state.player(&player()).unwrap().tactic_tokens, tokens - 1);
    }

    #[test]
    fn activating_without_a_token_is_refused_and_spends_nothing() {
        let (mut state, _, ids) = fixture();
        state.player_mut(&player()).unwrap().tactic_tokens = 0;
        let settled = state.clone();

        assert_eq!(
            activate(&mut state, &player(), &ids[0]),
            Err(TacticalError::NoTacticToken(player()))
        );
        assert!(state.identical(&settled));
    }

    #[test]
    fn only_ships_within_range_are_movable() {
        let (mut state, galaxy, ids) = fixture();
        // ids[0] is the hub centre; the rest ring it. Activate the centre.
        state.system_mut(&ids[1]).units.push(ship("destroyer"));
        activate(&mut state, &player(), &ids[0]).unwrap();

        let found = movable(&state, ContentStore::embedded(), POK, &galaxy, &player());

        assert_eq!(found.len(), 1);
        assert_eq!(found[0].origin, ids[1]);
    }

    #[test]
    fn gravleash_anchor_gives_the_remaining_nonfighters_the_fastest_moved_value() {
        let (mut state, galaxy, ids) = fixture();
        let origin = ids[1].clone();
        let destination = galaxy
            .system_ids()
            .into_iter()
            .find(|id| galaxy.distance(origin.as_str(), id) == Some(2))
            .map(SystemId::new)
            .unwrap();
        state.system_mut(&origin).units.push(ship("carrier"));
        state.player_mut(&player()).unwrap().breakthrough =
            Some(ti4_model::id::BreakthroughId::new("letnevbt"));
        state.gravleash_move_values.insert(origin.clone(), 2);
        activate(&mut state, &player(), &destination).unwrap();

        let found = movable(&state, ContentStore::embedded(), POK, &galaxy, &player());
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].unit.type_id.as_str(), "carrier");
    }

    #[test]
    fn ground_forces_are_not_movable_by_themselves() {
        // Only ships move; infantry travels as cargo.
        let (mut state, galaxy, ids) = fixture();
        state.system_mut(&ids[1]).units.push(ship("infantry"));
        activate(&mut state, &player(), &ids[0]).unwrap();

        assert!(movable(&state, ContentStore::embedded(), POK, &galaxy, &player()).is_empty());
    }

    #[test]
    fn ships_already_in_the_active_system_are_not_offered_a_move() {
        let (mut state, galaxy, ids) = fixture();
        state.system_mut(&ids[0]).units.push(ship("destroyer"));
        activate(&mut state, &player(), &ids[0]).unwrap();

        assert!(movable(&state, ContentStore::embedded(), POK, &galaxy, &player()).is_empty());
    }

    #[test]
    fn interchangeable_ships_are_one_option_not_three() {
        // A sampling decider draws per option, so three copies of one move would carry three
        // times the weight of an equally good move written once.
        let (mut state, galaxy, ids) = fixture();
        for _ in 0..3 {
            state.system_mut(&ids[1]).units.push(ship("cruiser"));
        }
        activate(&mut state, &player(), &ids[0]).unwrap();

        let found = movable(&state, ContentStore::embedded(), POK, &galaxy, &player());
        assert_eq!(found.len(), 3, "three hulls can each move");

        let choice = movement_options(&player(), &found);
        assert_eq!(choice.options.len(), 2, "one move, plus finish movement");
    }

    #[test]
    fn a_damaged_ship_is_a_different_move_from_a_fresh_one() {
        // You would rather advance the fresh one, and both read "move dreadnought from x".
        let (mut state, galaxy, ids) = fixture();
        state.system_mut(&ids[1]).units.push(ship("dreadnought"));
        state
            .system_mut(&ids[1])
            .units
            .push(ship("dreadnought").sustained());
        activate(&mut state, &player(), &ids[0]).unwrap();

        let found = movable(&state, ContentStore::embedded(), POK, &galaxy, &player());
        let choice = movement_options(&player(), &found);

        assert_eq!(choice.options.len(), 3, "two moves, plus finish movement");
        assert!(
            choice
                .options
                .iter()
                .any(|o| o.display().contains("damaged")),
            "the label says which is which"
        );
    }

    #[test]
    fn movement_always_offers_finishing() {
        // 89.2b: the player may choose to move nothing.
        let choice = movement_options(&player(), &[]);
        assert_eq!(choice.options.len(), 1);
        assert!(choice.options[0].is_decline());
    }

    #[test]
    fn a_move_answer_names_the_ship_it_selected() {
        let (mut state, galaxy, ids) = fixture();
        state.system_mut(&ids[1]).units.push(ship("carrier"));
        activate(&mut state, &player(), &ids[0]).unwrap();
        let found = movable(&state, ContentStore::embedded(), POK, &galaxy, &player());
        let choice = movement_options(&player(), &found);

        let picked = choice.options[0].clone();
        assert_eq!(
            read_move(&choice, picked).unwrap(),
            MoveSelection::Ship {
                origin: ids[1].clone(),
                index: 0,
                gravity_drive: false,
                ionian: false,
            }
        );
    }

    #[test]
    fn finishing_movement_is_read_as_done() {
        let choice = movement_options(&player(), &[]);
        let done = choice.option("done_moving").unwrap().clone();
        assert_eq!(read_move(&choice, done).unwrap(), MoveSelection::Done);
    }

    #[test]
    fn an_answer_that_was_not_offered_is_refused() {
        let choice = movement_options(&player(), &[]);
        let error = read_move(&choice, ChoiceOption::new("move|nowhere|0", MOVE_KIND)).unwrap_err();
        assert!(matches!(error, TacticalError::IllegalChoice(_)));
    }

    #[test]
    fn an_enemy_blockade_removes_a_ship_from_the_movable_list() {
        // The join between this module and the movement rules: legality is not re-derived
        // here, so a blockade discovered there disappears from the options offered here.
        let (mut state, galaxy, ids) = fixture();
        // The true opposite, not merely something two away: ring tiles two seats round are
        // also two apart by a route that never touches the centre, so blocking the centre
        // would prove nothing. The opposite is the one whose only shared neighbour is ids[0].
        let neighbours_of = |id: &str| -> std::collections::BTreeSet<String> {
            galaxy
                .adjacent(id)
                .into_iter()
                .map(ToOwned::to_owned)
                .collect()
        };
        let from_neighbours = neighbours_of(ids[1].as_str());
        let across = galaxy
            .system_ids()
            .into_iter()
            .find(|id| {
                galaxy.distance(ids[1].as_str(), id) == Some(2)
                    && &from_neighbours & &neighbours_of(id)
                        == std::collections::BTreeSet::from([ids[0].to_string()])
            })
            .map(SystemId::new)
            .expect("a system directly across the centre");
        state.system_mut(&ids[1]).units.push(ship("cruiser"));
        activate(&mut state, &player(), &across).unwrap();

        let before = movable(&state, ContentStore::embedded(), POK, &galaxy, &player());
        assert_eq!(before.len(), 1, "a cruiser moves 2 and can get there");

        state
            .system_mut(&ids[0])
            .units
            .push(Unit::new(UnitTypeId::new("destroyer"), PlayerId::new("b")));
        let after = movable(&state, ContentStore::embedded(), POK, &galaxy, &player());
        assert!(
            after.is_empty(),
            "58.4b closed the only route, so the move is no longer offered"
        );
    }

    // -- faction-module movement hooks (BF-00e) -----------------------------------------------

    fn across_the_centre(galaxy: &Galaxy, ids: &[SystemId]) -> SystemId {
        let neighbours_of = |id: &str| -> std::collections::BTreeSet<String> {
            galaxy
                .adjacent(id)
                .into_iter()
                .map(ToOwned::to_owned)
                .collect()
        };
        let from_neighbours = neighbours_of(ids[1].as_str());
        galaxy
            .system_ids()
            .into_iter()
            .find(|id| {
                galaxy.distance(ids[1].as_str(), id) == Some(2)
                    && &from_neighbours & &neighbours_of(id)
                        == std::collections::BTreeSet::from([ids[0].to_string()])
            })
            .map(SystemId::new)
            .expect("a system directly across the centre")
    }

    #[test]
    fn a_hook_move_bonus_reaches_what_the_printed_value_cannot_and_is_neutral_without_one() {
        use crate::factions::hooks_movement::{MovementHooks, with_test_hooks};
        let (mut state, galaxy, ids) = fixture();
        let across = across_the_centre(&galaxy, &ids);
        // A carrier moves 1 and is two systems short.
        state.system_mut(&ids[1]).units.push(ship("carrier"));
        activate(&mut state, &player(), &across).unwrap();
        let content = ContentStore::embedded();
        assert!(movable(&state, content, POK, &galaxy, &player()).is_empty());
        let kind = ti4_content::units::unit_type(content, "carrier", POK).unwrap();
        let printed = effective_move_value(&state, &kind, &player(), &ids[1]);

        // Slipstream's shape: +1 for ships starting in a marked system, +1 more for a hook that
        // reads the ship, so the two sum.
        state
            .faction_marks
            .insert("bonus_from".to_owned(), ids[1].to_string());
        let slipstream = MovementHooks {
            move_bonus: Some(|state, site| {
                i32::from(
                    state.faction_marks.get("bonus_from").map(String::as_str)
                        == Some(site.origin.as_str())
                        && site.player.as_str() == "a",
                )
            }),
            ..MovementHooks::NONE
        };
        let second = MovementHooks {
            move_bonus: Some(|_, site| i32::from(!site.ship.is_fighter())),
            ..MovementHooks::NONE
        };
        with_test_hooks(slipstream, || {
            assert_eq!(
                effective_move_value(&state, &kind, &player(), &ids[1]),
                printed + 1
            );
            assert_eq!(
                effective_move_value(&state, &kind, &player(), &ids[0]),
                printed,
                "only ships starting in the marked system"
            );
            // Still one short of two systems: a bonus of 1 on a move of 1 is a move of 2.
            let found = movable(&state, content, POK, &galaxy, &player());
            assert_eq!(found.len(), 1, "move 1 + 1 reaches two systems");
            assert_eq!(found[0].origin, ids[1]);
        });
        with_test_hooks(second, || {
            assert_eq!(
                effective_move_value(&state, &kind, &player(), &ids[1]),
                printed + 1
            );
        });
        assert!(
            movable(&state, content, POK, &galaxy, &player()).is_empty(),
            "the hook is gone and so is the move"
        );
        assert_eq!(
            effective_move_value(&state, &kind, &player(), &ids[1]),
            printed
        );
    }

    #[test]
    fn a_ship_type_that_passes_blockades_is_offered_the_move_the_others_are_refused() {
        use crate::factions::hooks_movement::{MovementHooks, with_test_hooks};
        let (mut state, galaxy, ids) = fixture();
        let across = across_the_centre(&galaxy, &ids);
        let enemy = PlayerId::new("b");
        state
            .system_mut(&ids[0])
            .units
            .push(Unit::new(UnitTypeId::new("destroyer"), enemy));
        state.system_mut(&ids[1]).units.push(ship("cruiser"));
        state.system_mut(&ids[1]).units.push(ship("destroyer"));
        activate(&mut state, &player(), &across).unwrap();
        let content = ContentStore::embedded();
        assert!(
            movable(&state, content, POK, &galaxy, &player()).is_empty(),
            "the centre is blockaded for both"
        );
        let corsair = MovementHooks {
            may_move_through_ships: Some(|_, _, _, site| {
                site.player.as_str() == "a" && site.ship_type == "cruiser"
            }),
            ..MovementHooks::NONE
        };
        with_test_hooks(corsair, || {
            let found = movable(&state, content, POK, &galaxy, &player());
            assert_eq!(found.len(), 1, "only the cruiser: {found:?}");
            assert_eq!(found[0].unit.type_id.as_str(), "cruiser");
        });
    }
}
