//! Unit types, properties, and tactical structures.

use crate::id::UnitTypeId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnitType {
    pub id: UnitTypeId,
    pub name: String,
    pub code: String,
    pub cost: i32,
    pub combat: i32,
    pub bomb: i32,
    pub movement: i32,
    pub capacity: i32,
    pub fuel_capacity: i32,
    pub upgrade_cost: i32,
    pub upgrade_combat: i32,
    pub abilities: Vec<String>,
    pub unit_type: UnitCategory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnitCategory {
    Fighter,
    Cruiser,
    Destroyer,
    Carrier,
    Dreadnought,
    Infantry,
    Pds,
    Spacedock,
    Warsun,
    Flagship,
    Mech,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FleetState {
    pub unit_types: Vec<UnitFleetEntry>,
    pub total_movement: i32,
    pub total_fuel: i32,
    pub total_capacity: i32,
    pub total_casualties: i32,
    pub has_flagship: bool,
    pub has_warsun: bool,
    pub has_pds: bool,
    pub has_infantry: bool,
    pub has_mech: bool,
    pub has_fighter: bool,
    pub has_cruiser: bool,
    pub has_destroyer: bool,
    pub has_carrier: bool,
    pub has_dreadnought: bool,
    pub has_spacedock: bool,
}

impl Default for FleetState {
    fn default() -> Self {
        Self {
            unit_types: vec![],
            total_movement: 0,
            total_fuel: 0,
            total_capacity: 0,
            total_casualties: 0,
            has_flagship: false,
            has_warsun: false,
            has_pds: false,
            has_infantry: false,
            has_mech: false,
            has_fighter: false,
            has_cruiser: false,
            has_destroyer: false,
            has_carrier: false,
            has_dreadnought: false,
            has_spacedock: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnitFleetEntry {
    pub unit_type: UnitTypeId,
    pub count: i32,
    pub upgraded: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CombatPhase {
    SpaceCannon,
    CombatSetup,
    CombatRound,
    Bombardment,
    GroundCombat,
    Casualties,
    Retreat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MovementType {
    Normal,
    Jump,
    Wormhole,
    Fracture,
    Ingress,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CombatSide {
    Attacker,
    Defender,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CombatResult {
    Success,
    Failure,
    Retreat,
    Destroyed,
}
