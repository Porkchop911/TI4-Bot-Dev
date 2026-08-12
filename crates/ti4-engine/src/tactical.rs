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
use crate::movement::{Board, MovementRules};

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
#[must_use]
pub fn activatable(state: &GameState, galaxy: &Galaxy, player: &PlayerId) -> Vec<SystemId> {
    let held = state.systems_with_token(player);
    galaxy
        .system_ids()
        .into_iter()
        .map(SystemId::new)
        .filter(|system| !held.contains(system))
        .collect()
}

/// The activation choice, or `None` when the player cannot take a tactical action.
///
/// 89.1 requires a tactic token to spend, so a player with none is not offered the action at
/// all rather than being offered one they cannot pay for.
#[must_use]
pub fn activation_options(state: &GameState, galaxy: &Galaxy, player: &PlayerId) -> Option<Choice> {
    if state
        .player(player)
        .is_none_or(|seat| seat.tactic_tokens <= 0)
    {
        return None;
    }
    let options: Vec<ChoiceOption> = activatable(state, galaxy, player)
        .into_iter()
        .map(|system| {
            ChoiceOption::labelled(
                system.to_string(),
                ACTIVATE_KIND,
                format!("activate {system}"),
            )
        })
        .collect();
    if options.is_empty() {
        return None;
    }
    Some(Choice::new(player.clone(), "activate a system", options))
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
    if state.systems_with_token(player).contains(system) {
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
    let types = catalogue(content, sources);
    let board = Board::for_player(state, content, sources, player);
    let rules = MovementRules::new(galaxy, content, sources, active.as_str(), board);

    let mut found = Vec::new();
    for origin in state.systems_with_units_of(player) {
        if origin == &active {
            continue;
        }
        for (index, hull) in state.ships_of(player, origin).into_iter().enumerate() {
            let Some(kind) = types.get(hull.type_id.as_str()) else {
                continue;
            };
            if !kind.is_ship() {
                continue;
            }
            let move_value = i32::try_from(kind.move_value()).unwrap_or(0)
                + crate::action_cards::move_bonus(state, player, state.activation_seq);
            if rules.can_reach(origin.as_str(), move_value) {
                found.push(Movable {
                    origin: origin.clone(),
                    index,
                    unit: hull.clone(),
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
        );
        if !seen.insert(key) {
            continue;
        }
        let damaged = if candidate.unit.sustained_damage {
            " (damaged)"
        } else {
            ""
        };
        options.push(ChoiceOption::labelled(
            format!("move|{}|{}", candidate.origin, candidate.index),
            MOVE_KIND,
            format!(
                "move {}{damaged} from {}",
                candidate.unit.type_id, candidate.origin
            ),
        ));
    }
    // 89.2b: the player may choose to move nothing.
    options.push(ChoiceOption::labelled(
        "done_moving",
        crate::choice::DECLINE_KIND,
        "finish movement",
    ));
    Choice::new(player.clone(), "movement", options)
}

/// What a movement-step answer asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MoveSelection {
    /// Move this ship, identified by where it stands.
    Ship { origin: SystemId, index: usize },
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
    let (Some(_verb), Some(origin), Some(index)) = (parts.next(), parts.next(), parts.next())
    else {
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
            })
        },
    )
}

#[cfg(test)]
mod tests {

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

    #[test]
    fn a_player_without_a_tactic_token_is_not_offered_the_action() {
        let (mut state, galaxy, _) = fixture();
        state.player_mut(&player()).unwrap().tactic_tokens = 0;

        assert!(activation_options(&state, &galaxy, &player()).is_none());
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
                index: 0
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
}
