//! The public change of the board of one step of the engine, in the terms of the view.
//!
//! A page that holds the board of now and the events of a game can take the board back to any
//! point of the events: each [`GameEventKind::BoardChanged`](super::server::GameEventKind) event
//! has a [`BoardDelta`], and every part of it has the value before the step and after it. Put
//! back each `before` from the last event to a point, and the board is the board of that point;
//! put each `after`, and it goes forward again. A value is whole (a count of units at a spot, a
//! planet, the command tokens of a system, the pools of a seat, the combat under way), so
//! applying one twice changes nothing.
//!
//! The board is the board view of [`project_board_view_full`] with no viewer, no question and no
//! map tiles, and the pools of each seat: nothing that one seat knows and another does not, so the
//! change is the same for every viewer. The dice of a combat round are in its
//! [`CombatView`](super::view::CombatView), the dice of a ground round in the last step of its
//! [`InvasionView`](super::view::InvasionView).

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use ti4_model::id::{PlanetId, PlayerId, SystemId, UnitTypeId};
use ti4_model::state::GameState;

use super::view::{
    BoardView, CombatView, GroundStepView, InvasionView, PlacedUnitView, PlanetView,
    SpaceRoundView, SystemView,
};
use crate::projection::{facts_context, project_board_view_full};

/// Where units stand, and what they are: the units of a count.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct UnitSpot {
    system: SystemId,
    planet: Option<PlanetId>,
    owner: PlayerId,
    unit_type: UnitTypeId,
    damaged: bool,
    galvanized: bool,
}

/// The pools of a seat that the board of a step changes.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct SeatPools {
    pub trade_goods: i32,
    pub commodities: i32,
    pub tactic_tokens: i32,
    pub fleet_tokens: i32,
    pub strategic_tokens: i32,
}

/// The public board of a state: [`BoardView`] with no viewer and no map tiles, as counts, and
/// the pools of each seat.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PublicBoard {
    systems: BTreeSet<SystemId>,
    units: BTreeMap<UnitSpot, u32>,
    planets: BTreeMap<(SystemId, PlanetId), PlanetView>,
    tokens: BTreeMap<SystemId, BTreeSet<PlayerId>>,
    pools: BTreeMap<PlayerId, SeatPools>,
    active_system: Option<SystemId>,
    combat: Option<CombatView>,
    invasion: Option<InvasionView>,
    space_rounds: Vec<SpaceRoundView>,
    ground_steps: Vec<GroundStepView>,
}

/// A system that is on the board after the step and was not before, or the other way.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct SystemChange {
    pub system: SystemId,
    pub before: bool,
    pub after: bool,
}

/// The count of the units of one kind at one spot, before and after.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct UnitChange {
    pub system: SystemId,
    /// `None`: the space area.
    pub planet: Option<PlanetId>,
    pub owner: PlayerId,
    pub unit_type: UnitTypeId,
    pub damaged: bool,
    pub galvanized: bool,
    pub before: u32,
    pub after: u32,
}

/// A planet, before and after (`None`: not in the view of its system).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct PlanetChange {
    pub system: SystemId,
    pub planet: PlanetId,
    pub before: Option<PlanetView>,
    pub after: Option<PlanetView>,
}

/// The command tokens in a system, before and after.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct TokensChange {
    pub system: SystemId,
    pub before: BTreeSet<PlayerId>,
    pub after: BTreeSet<PlayerId>,
}

/// The pools of a seat, before and after.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct PoolsChange {
    pub seat: PlayerId,
    pub before: SeatPools,
    pub after: SeatPools,
}

/// The active system, before and after.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ActiveSystemChange {
    pub before: Option<SystemId>,
    pub after: Option<SystemId>,
}

/// The space combat under way, before and after.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct CombatChange {
    pub before: Option<CombatView>,
    pub after: Option<CombatView>,
}

/// The invasion under way, before and after.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct InvasionChange {
    pub before: Option<InvasionView>,
    pub after: Option<InvasionView>,
}

/// The rounds of the space combats of the turn: the first `kept` stay, the rest is `before`
/// before the step and `after` after it. A step adds a round, rolls the last one again, or ends
/// the turn and takes them all.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct SpaceRoundsChange {
    pub kept: usize,
    pub before: Vec<SpaceRoundView>,
    pub after: Vec<SpaceRoundView>,
}

/// The steps of the invasions of the turn, as [`SpaceRoundsChange`] has the rounds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct GroundStepsChange {
    pub kept: usize,
    pub before: Vec<GroundStepView>,
    pub after: Vec<GroundStepView>,
}

/// What one step changed of the public board. Each part is there only when it changed.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct BoardDelta {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub systems: Vec<SystemChange>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub units: Vec<UnitChange>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub planets: Vec<PlanetChange>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tokens: Vec<TokensChange>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pools: Vec<PoolsChange>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_system: Option<ActiveSystemChange>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub combat: Option<CombatChange>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub invasion: Option<InvasionChange>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub space_rounds: Option<SpaceRoundsChange>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ground_steps: Option<GroundStepsChange>,
}

impl BoardDelta {
    /// Nothing changed.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// `after` going forward, `before` going back.
fn pick<T>(forward: bool, before: T, after: T) -> T {
    if forward { after } else { before }
}

/// How many items two lists share from their start.
fn shared<T: PartialEq>(before: &[T], after: &[T]) -> usize {
    before
        .iter()
        .zip(after)
        .take_while(|(was, is)| was == is)
        .count()
}

/// The list of `kept` items of `list` and then `tail`.
fn splice<T: Clone>(list: &mut Vec<T>, kept: usize, tail: &[T]) {
    list.truncate(kept);
    list.extend_from_slice(tail);
}

/// Every key of two maps, once.
fn keys<'a, K: Ord + Clone, A, B>(
    before: &'a BTreeMap<K, A>,
    after: &'a BTreeMap<K, B>,
) -> BTreeSet<K> {
    before.keys().chain(after.keys()).cloned().collect()
}

impl PublicBoard {
    /// The public board of `state`.
    #[must_use]
    pub fn of(state: &GameState) -> Self {
        let facts = facts_context(None);
        let view =
            project_board_view_full(state, facts.content, facts.sources, &[], None, &[], None);
        let pools = state
            .players
            .iter()
            .map(|player| {
                (
                    player.id.clone(),
                    SeatPools {
                        trade_goods: player.trade_goods,
                        commodities: player.commodities,
                        tactic_tokens: player.tactic_tokens,
                        fleet_tokens: player.fleet_tokens,
                        strategic_tokens: player.strategic_tokens,
                    },
                )
            })
            .collect();
        Self::from_view(view, pools)
    }

    /// The board of `view`, with `pools`. The map tiles are not a part of it.
    #[must_use]
    pub fn from_view(view: BoardView, pools: BTreeMap<PlayerId, SeatPools>) -> Self {
        let mut board = Self {
            pools,
            active_system: view.active_system,
            combat: view.combat,
            invasion: view.invasion,
            space_rounds: view.space_rounds,
            ground_steps: view.ground_steps,
            ..Self::default()
        };
        for (id, system) in view.systems {
            for unit in system.units {
                let spot = UnitSpot {
                    system: id.clone(),
                    planet: unit.planet,
                    owner: unit.owner,
                    unit_type: unit.unit_type,
                    damaged: unit.damaged,
                    galvanized: unit.galvanized,
                };
                *board.units.entry(spot).or_insert(0) += 1;
            }
            for (planet, view) in system.planets {
                board.planets.insert((id.clone(), planet), view);
            }
            if !system.command_tokens.is_empty() {
                board.tokens.insert(id.clone(), system.command_tokens);
            }
            board.systems.insert(id);
        }
        board
    }

    /// The board view of this board, its units in the order of their spot, with no map tiles.
    #[must_use]
    pub fn to_view(&self) -> BoardView {
        let mut systems: BTreeMap<SystemId, SystemView> = self
            .systems
            .iter()
            .map(|id| {
                (
                    id.clone(),
                    SystemView {
                        system_id: id.clone(),
                        command_tokens: self.tokens.get(id).cloned().unwrap_or_default(),
                        planets: BTreeMap::new(),
                        units: Vec::new(),
                    },
                )
            })
            .collect();
        for ((system, planet), view) in &self.planets {
            if let Some(system) = systems.get_mut(system) {
                system.planets.insert(planet.clone(), view.clone());
            }
        }
        for (spot, count) in &self.units {
            let Some(system) = systems.get_mut(&spot.system) else {
                continue;
            };
            for _ in 0..*count {
                system.units.push(PlacedUnitView {
                    unit_type: spot.unit_type.clone(),
                    owner: spot.owner.clone(),
                    planet: spot.planet.clone(),
                    damaged: spot.damaged,
                    galvanized: spot.galvanized,
                });
            }
        }
        BoardView {
            systems,
            active_system: self.active_system.clone(),
            map_tiles: Vec::new(),
            combat: self.combat.clone(),
            invasion: self.invasion.clone(),
            space_rounds: self.space_rounds.clone(),
            ground_steps: self.ground_steps.clone(),
        }
    }

    /// The pools of each seat.
    #[must_use]
    pub fn pools(&self) -> &BTreeMap<PlayerId, SeatPools> {
        &self.pools
    }

    /// What changed from `before` to `self`; `None` when nothing did.
    #[must_use]
    pub fn delta_from(&self, before: &Self) -> Option<BoardDelta> {
        let after = self;
        let mut delta = BoardDelta::default();
        for system in before.systems.symmetric_difference(&after.systems) {
            delta.systems.push(SystemChange {
                system: system.clone(),
                before: before.systems.contains(system),
                after: after.systems.contains(system),
            });
        }
        for spot in keys(&before.units, &after.units) {
            let (was, is) = (
                before.units.get(&spot).copied().unwrap_or(0),
                after.units.get(&spot).copied().unwrap_or(0),
            );
            if was != is {
                delta.units.push(UnitChange {
                    system: spot.system,
                    planet: spot.planet,
                    owner: spot.owner,
                    unit_type: spot.unit_type,
                    damaged: spot.damaged,
                    galvanized: spot.galvanized,
                    before: was,
                    after: is,
                });
            }
        }
        for key in keys(&before.planets, &after.planets) {
            let (was, is) = (before.planets.get(&key), after.planets.get(&key));
            if was != is {
                delta.planets.push(PlanetChange {
                    system: key.0,
                    planet: key.1,
                    before: was.cloned(),
                    after: is.cloned(),
                });
            }
        }
        for system in keys(&before.tokens, &after.tokens) {
            let (was, is) = (
                before.tokens.get(&system).cloned().unwrap_or_default(),
                after.tokens.get(&system).cloned().unwrap_or_default(),
            );
            if was != is {
                delta.tokens.push(TokensChange {
                    system,
                    before: was,
                    after: is,
                });
            }
        }
        for seat in keys(&before.pools, &after.pools) {
            let (was, is) = (
                before.pools.get(&seat).cloned().unwrap_or_default(),
                after.pools.get(&seat).cloned().unwrap_or_default(),
            );
            if was != is {
                delta.pools.push(PoolsChange {
                    seat,
                    before: was,
                    after: is,
                });
            }
        }
        if before.active_system != after.active_system {
            delta.active_system = Some(ActiveSystemChange {
                before: before.active_system.clone(),
                after: after.active_system.clone(),
            });
        }
        if before.combat != after.combat {
            delta.combat = Some(CombatChange {
                before: before.combat.clone(),
                after: after.combat.clone(),
            });
        }
        if before.invasion != after.invasion {
            delta.invasion = Some(InvasionChange {
                before: before.invasion.clone(),
                after: after.invasion.clone(),
            });
        }
        if before.space_rounds != after.space_rounds {
            let kept = shared(&before.space_rounds, &after.space_rounds);
            delta.space_rounds = Some(SpaceRoundsChange {
                kept,
                before: before.space_rounds[kept..].to_vec(),
                after: after.space_rounds[kept..].to_vec(),
            });
        }
        if before.ground_steps != after.ground_steps {
            let kept = shared(&before.ground_steps, &after.ground_steps);
            delta.ground_steps = Some(GroundStepsChange {
                kept,
                before: before.ground_steps[kept..].to_vec(),
                after: after.ground_steps[kept..].to_vec(),
            });
        }
        (!delta.is_empty()).then_some(delta)
    }

    /// The board before the step of `delta`, from the board after it.
    pub fn rewind(&mut self, delta: &BoardDelta) {
        self.apply(delta, false);
    }

    /// The board after the step of `delta`, from the board before it.
    pub fn forward(&mut self, delta: &BoardDelta) {
        self.apply(delta, true);
    }

    fn apply(&mut self, delta: &BoardDelta, forward: bool) {
        for change in &delta.systems {
            if pick(forward, change.before, change.after) {
                self.systems.insert(change.system.clone());
            } else {
                self.systems.remove(&change.system);
            }
        }
        for change in &delta.units {
            let spot = UnitSpot {
                system: change.system.clone(),
                planet: change.planet.clone(),
                owner: change.owner.clone(),
                unit_type: change.unit_type.clone(),
                damaged: change.damaged,
                galvanized: change.galvanized,
            };
            match pick(forward, change.before, change.after) {
                0 => self.units.remove(&spot),
                count => self.units.insert(spot, count),
            };
        }
        for change in &delta.planets {
            let key = (change.system.clone(), change.planet.clone());
            match pick(forward, &change.before, &change.after) {
                Some(view) => self.planets.insert(key, view.clone()),
                None => self.planets.remove(&key),
            };
        }
        for change in &delta.tokens {
            let tokens = pick(forward, &change.before, &change.after);
            if tokens.is_empty() {
                self.tokens.remove(&change.system);
            } else {
                self.tokens.insert(change.system.clone(), tokens.clone());
            }
        }
        for change in &delta.pools {
            self.pools.insert(
                change.seat.clone(),
                pick(forward, &change.before, &change.after).clone(),
            );
        }
        if let Some(change) = &delta.active_system {
            self.active_system = pick(forward, &change.before, &change.after).clone();
        }
        if let Some(change) = &delta.combat {
            self.combat = pick(forward, &change.before, &change.after).clone();
        }
        if let Some(change) = &delta.invasion {
            self.invasion = pick(forward, &change.before, &change.after).clone();
        }
        if let Some(change) = &delta.space_rounds {
            let tail = pick(forward, &change.before, &change.after);
            splice(&mut self.space_rounds, change.kept, tail);
        }
        if let Some(change) = &delta.ground_steps {
            let tail = pick(forward, &change.before, &change.after);
            splice(&mut self.ground_steps, change.kept, tail);
        }
    }
}

#[cfg(test)]
mod tests {
    use ti4_content::ContentStore;
    use ti4_engine::choice::{
        Choice, ChoiceOption, Decider, IllegalChoice, SeatObservation, Table,
    };
    use ti4_engine::game::Game;

    use super::*;
    use crate::projection::redacted_state;
    use crate::protocol::ViewerRole;

    /// Rule bots for every seat, by their place in the order of the game.
    struct Smart {
        seed: u64,
        order: Vec<PlayerId>,
        index: usize,
    }

    impl Smart {
        fn answer(
            &mut self,
            choice: &Choice,
            seen: Option<&SeatObservation<'_>>,
        ) -> Result<ChoiceOption, IllegalChoice> {
            let place = self
                .order
                .iter()
                .position(|seat| *seat == choice.player)
                .unwrap_or(0);
            let (picked, _) = ti4_rule_bot::choose(choice, seen, self.seed, place, self.index);
            self.index += 1;
            Ok(choice.options[picked].clone())
        }
    }

    impl Decider for Smart {
        fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
            self.answer(choice, None)
        }

        fn choose_seeing(
            &mut self,
            choice: &Choice,
            seen: &SeatObservation<'_>,
        ) -> Result<ChoiceOption, IllegalChoice> {
            self.answer(choice, Some(seen))
        }
    }

    /// The board of every step of a game of rule bots from the invasion preset, from the start.
    fn boards(seed: u64, steps: usize) -> Vec<(GameState, PublicBoard)> {
        let players: Vec<PlayerId> = (1..=4).map(|i| PlayerId::new(format!("p{i}"))).collect();
        let (state, galaxy) = crate::setup::preset_game(
            ContentStore::embedded(),
            &players,
            seed,
            None,
            crate::preset::INVASION,
        )
        .expect("the preset starts");
        let decider = Smart {
            seed,
            order: players,
            index: 0,
        };
        let mut game = Game::with_table(
            state,
            ContentStore::embedded(),
            Table::with_default(Box::new(decider)),
        )
        .with_galaxy(galaxy);
        let mut boards = vec![(game.state.clone(), PublicBoard::of(&game.state))];
        for _ in 0..steps {
            let result = game.step();
            assert!(result.error.is_none(), "{:?}", result.error);
            boards.push((game.state.clone(), PublicBoard::of(&game.state)));
            if result.finished {
                break;
            }
        }
        boards
    }

    /// The units of a board view in one order, for a comparison of the same units.
    fn sorted(mut view: BoardView) -> BoardView {
        for system in view.systems.values_mut() {
            system.units.sort_by(|a, b| {
                (&a.planet, &a.owner, &a.unit_type, a.damaged, a.galvanized).cmp(&(
                    &b.planet,
                    &b.owner,
                    &b.unit_type,
                    b.damaged,
                    b.galvanized,
                ))
            });
        }
        view
    }

    #[test]
    fn the_changes_of_a_game_take_its_board_back_to_every_step_and_forward_again() {
        let boards = boards(2, 450);
        let deltas: Vec<Option<BoardDelta>> = boards
            .windows(2)
            .map(|pair| pair[1].1.delta_from(&pair[0].1))
            .collect();

        // Back from the end: the board of every step.
        let mut board = boards.last().expect("a board").1.clone();
        for (k, delta) in deltas.iter().enumerate().rev() {
            if let Some(delta) = delta {
                board.rewind(delta);
            }
            assert_eq!(board, boards[k].1, "back to step {k}");
        }
        // Forward from the start.
        let mut board = boards[0].1.clone();
        for (k, delta) in deltas.iter().enumerate() {
            if let Some(delta) = delta {
                board.forward(delta);
                // A change is whole: put twice, it is the same.
                board.forward(delta);
            }
            assert_eq!(board, boards[k + 1].1, "forward to step {k}");
        }

        // The game had what the test is for: a space combat of more than one round with its
        // dice, and rounds of ground combat with theirs. Every round that a board showed under
        // way is in the record of the turn, the last round of a combat too.
        let recorded: BTreeSet<(u32, u32)> = boards
            .iter()
            .flat_map(|(_, board)| &board.space_rounds)
            .filter(|round| !round.dice_rolls.is_empty())
            .map(|round| (round.battle_seq, round.round))
            .collect();
        assert!(
            recorded.iter().any(|(_, round)| *round > 1),
            "a space combat of two rounds: {recorded:?}"
        );
        for (_, board) in &boards {
            if let Some(combat) = board.combat.as_ref().filter(|c| !c.dice_rolls.is_empty()) {
                assert!(recorded.contains(&(combat.battle_seq, combat.round)));
            }
        }
        let ground: BTreeSet<(u64, String, u32)> = boards
            .iter()
            .flat_map(|(_, board)| &board.ground_steps)
            .filter(|record| record.step.kind == "ground_round" && !record.step.dice.is_empty())
            .map(|record| {
                (
                    record.invasion_seq,
                    record.step.planet.to_string(),
                    record.step.round,
                )
            })
            .collect();
        assert!(
            ground.iter().any(|(_, _, round)| *round > 1),
            "two rounds of a ground combat: {ground:?}"
        );
    }

    #[test]
    fn the_board_is_the_same_for_every_viewer_and_is_the_whole_board_view() {
        let facts = facts_context(None);
        for (state, board) in boards(5, 250) {
            let whole =
                project_board_view_full(&state, facts.content, facts.sources, &[], None, &[], None);
            assert_eq!(sorted(board.to_view()), sorted(whole.clone()));
            let viewers = state
                .players
                .iter()
                .map(|player| ViewerRole::Player(player.id.clone()))
                .chain([ViewerRole::Spectator]);
            for viewer in viewers {
                let seen = redacted_state(&state, &viewer);
                assert_eq!(
                    PublicBoard::of(&seen),
                    board,
                    "the board of {viewer:?} at round {}",
                    state.round
                );
            }
        }
    }

    #[test]
    fn a_change_is_only_what_changed() {
        let mut before = PublicBoard::default();
        before.systems.insert(SystemId::new("21"));
        let mut after = before.clone();
        assert_eq!(after.delta_from(&before), None);
        after.units.insert(
            UnitSpot {
                system: SystemId::new("21"),
                planet: None,
                owner: PlayerId::new("p1"),
                unit_type: UnitTypeId::new("carrier"),
                damaged: false,
                galvanized: false,
            },
            2,
        );
        let delta = after.delta_from(&before).expect("a change");
        assert_eq!(delta.units.len(), 1);
        assert_eq!((delta.units[0].before, delta.units[0].after), (0, 2));
        assert!(delta.planets.is_empty() && delta.combat.is_none());
        // The wire leaves out what did not change.
        let json = serde_json::to_value(&delta).expect("a delta writes");
        assert_eq!(
            json.as_object()
                .map(|object| object.keys().cloned().collect::<Vec<_>>()),
            Some(vec!["units".to_owned()])
        );
    }
}
