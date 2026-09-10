//! Deterministic physical unit differences between two authoritative Rust states.
//!
//! Typed events say *why* a change happened and establish ordering. They are not yet emitted for
//! every mutation, so this structural layer says *what visibly changed*. A coordinator can consume
//! event-explained entries and must refuse any unexplained ambiguity that remains.

use std::collections::{BTreeMap, BTreeSet};

use ti4_model::{
    id::{PlanetId, PlayerId, SystemId, UnitTypeId},
    state::GameState,
    units::Unit,
};

/// A physical region on the table.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct UnitLocation {
    pub system: SystemId,
    pub planet: Option<PlanetId>,
}

/// One interchangeable class of physical unit.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct UnitClass {
    pub owner: PlayerId,
    pub unit_type: UnitTypeId,
    pub galvanized: bool,
}

/// A visible unit mutation. Counts are positive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnitChange {
    Damage {
        location: UnitLocation,
        unit: UnitClass,
        damaged: bool,
        count: usize,
    },
    Add {
        location: UnitLocation,
        unit: UnitClass,
        damaged: bool,
        count: usize,
    },
    Remove {
        location: UnitLocation,
        unit: UnitClass,
        damaged: bool,
        count: usize,
    },
}

type CountKey = (UnitLocation, UnitClass, bool);

/// Compute the complete unit multiset delta, ordered by system, region, owner and unit type.
#[must_use]
pub fn unit_changes(before: &GameState, after: &GameState) -> Vec<UnitChange> {
    let before = counts(before);
    let after = counts(after);
    let classes: BTreeSet<(UnitLocation, UnitClass)> = before
        .keys()
        .chain(after.keys())
        .map(|(location, unit, _)| (location.clone(), unit.clone()))
        .collect();
    let mut changes = Vec::new();

    for (location, unit) in classes {
        let old_fresh = count(&before, &location, &unit, false);
        let old_damaged = count(&before, &location, &unit, true);
        let new_fresh = count(&after, &location, &unit, false);
        let new_damaged = count(&after, &location, &unit, true);

        let newly_damaged = old_fresh
            .saturating_sub(new_fresh)
            .min(new_damaged.saturating_sub(old_damaged));
        let newly_repaired = old_damaged
            .saturating_sub(new_damaged)
            .min(new_fresh.saturating_sub(old_fresh));
        if newly_damaged > 0 {
            changes.push(UnitChange::Damage {
                location: location.clone(),
                unit: unit.clone(),
                damaged: true,
                count: newly_damaged,
            });
        }
        if newly_repaired > 0 {
            changes.push(UnitChange::Damage {
                location: location.clone(),
                unit: unit.clone(),
                damaged: false,
                count: newly_repaired,
            });
        }

        let paired_fresh = newly_damaged + newly_repaired;
        residual(
            &mut changes,
            &location,
            &unit,
            false,
            old_fresh,
            new_fresh,
            paired_fresh,
        );
        residual(
            &mut changes,
            &location,
            &unit,
            true,
            old_damaged,
            new_damaged,
            paired_fresh,
        );
    }
    changes
}

fn residual(
    changes: &mut Vec<UnitChange>,
    location: &UnitLocation,
    unit: &UnitClass,
    damaged: bool,
    old: usize,
    new: usize,
    paired: usize,
) {
    if new > old {
        let count = new - old - paired.min(new - old);
        if count > 0 {
            changes.push(UnitChange::Add {
                location: location.clone(),
                unit: unit.clone(),
                damaged,
                count,
            });
        }
    } else if old > new {
        let count = old - new - paired.min(old - new);
        if count > 0 {
            changes.push(UnitChange::Remove {
                location: location.clone(),
                unit: unit.clone(),
                damaged,
                count,
            });
        }
    }
}

fn count(
    counts: &BTreeMap<CountKey, usize>,
    location: &UnitLocation,
    unit: &UnitClass,
    damaged: bool,
) -> usize {
    counts
        .get(&(location.clone(), unit.clone(), damaged))
        .copied()
        .unwrap_or(0)
}

fn counts(state: &GameState) -> BTreeMap<CountKey, usize> {
    let mut out = BTreeMap::new();
    for (system, board) in &state.board {
        let space = UnitLocation {
            system: system.clone(),
            planet: None,
        };
        add_units(&mut out, &space, &board.units);
        for (planet, units) in &board.planet_units {
            let location = UnitLocation {
                system: system.clone(),
                planet: Some(planet.clone()),
            };
            add_units(&mut out, &location, units);
        }
    }
    out
}

fn add_units(out: &mut BTreeMap<CountKey, usize>, location: &UnitLocation, units: &[Unit]) {
    for unit in units {
        let class = UnitClass {
            owner: unit.owner.clone(),
            unit_type: unit.type_id.clone(),
            galvanized: unit.galvanized,
        };
        *out.entry((location.clone(), class, unit.sustained_damage))
            .or_default() += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ti4_model::units::Unit;

    fn state() -> GameState {
        GameState::new(&[PlayerId::new("a")], &[], BTreeMap::new(), None, 1)
    }

    fn unit(damaged: bool) -> Unit {
        let unit = Unit::new(UnitTypeId::new("dreadnought"), PlayerId::new("a"));
        if damaged { unit.sustained() } else { unit }
    }

    #[test]
    fn a_flag_change_is_one_damage_operation_not_remove_plus_add() {
        let mut before = state();
        before
            .system_mut(&SystemId::new("18"))
            .units
            .push(unit(false));
        let mut after = before.clone();
        after.system_mut(&SystemId::new("18")).units[0].sustained_damage = true;

        assert!(matches!(
            unit_changes(&before, &after).as_slice(),
            [UnitChange::Damage {
                damaged: true,
                count: 1,
                ..
            }]
        ));
    }

    #[test]
    fn an_unannounced_repair_is_visible_in_space_and_on_planets() {
        let mut before = state();
        before
            .system_mut(&SystemId::new("18"))
            .units
            .push(unit(true));
        before
            .system_mut(&SystemId::new("18"))
            .planet_units
            .entry(PlanetId::new("mecatol"))
            .or_default()
            .push(unit(true));
        let mut after = before.clone();
        after.system_mut(&SystemId::new("18")).units[0].sustained_damage = false;
        after
            .system_mut(&SystemId::new("18"))
            .planet_units
            .get_mut(&PlanetId::new("mecatol"))
            .unwrap()[0]
            .sustained_damage = false;

        let changes = unit_changes(&before, &after);
        assert_eq!(changes.len(), 2);
        assert!(changes.iter().all(|change| matches!(
            change,
            UnitChange::Damage {
                damaged: false,
                count: 1,
                ..
            }
        )));
        assert!(changes.iter().any(|change| matches!(
            change,
            UnitChange::Damage {
                location: UnitLocation {
                    planet: Some(_),
                    ..
                },
                ..
            }
        )));
    }

    #[test]
    fn movement_remains_an_explicit_remove_and_add_for_semantic_pairing() {
        let mut before = state();
        before
            .system_mut(&SystemId::new("01"))
            .units
            .push(unit(false));
        let mut after = before.clone();
        let moved = after.system_mut(&SystemId::new("01")).units.pop().unwrap();
        after.system_mut(&SystemId::new("18")).units.push(moved);

        let changes = unit_changes(&before, &after);
        assert_eq!(changes.len(), 2);
        assert!(
            changes
                .iter()
                .any(|c| matches!(c, UnitChange::Remove { .. }))
        );
        assert!(changes.iter().any(|c| matches!(c, UnitChange::Add { .. })));
    }
}
