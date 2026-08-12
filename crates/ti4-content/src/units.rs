//! Units, read from the content corpus.
//!
//! Unit statistics are data, never code. A unit upgrade is a different record with the same
//! `baseType`, so upgrading is a lookup rather than a branch, and Thunder's Edge units
//! arrive by re-extraction rather than by editing a table.
//!
//! [`UnitType`] is a borrowed view over a [`Record`], not a copy of one: the corpus is
//! immutable and already in memory, and the accessors exist so that the interpretation of a
//! field — which is where the subtleties live — is written down exactly once.

use std::collections::BTreeMap;

use ti4_model::content_types::{ContentType, SourceSet};

use crate::loader::ContentStore;
use crate::record::Record;

/// A typed view over a unit record.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UnitType<'a> {
    record: &'a Record,
}

impl<'a> UnitType<'a> {
    /// Wrap a record from the `units` category.
    #[must_use]
    pub const fn new(record: &'a Record) -> Self {
        Self { record }
    }

    /// The underlying record, for fields this view does not interpret.
    #[must_use]
    pub const fn record(&self) -> &'a Record {
        self.record
    }

    #[must_use]
    pub fn id(&self) -> &'a str {
        self.record.id().unwrap_or_default()
    }

    /// The unit's kind. An upgrade shares its base type with what it upgrades from, which
    /// is what makes "does this fleet have a carrier" a question about data and not about
    /// which technologies a player has researched.
    #[must_use]
    pub fn base_type(&self) -> &'a str {
        self.record.text("baseType").unwrap_or_else(|| self.id())
    }

    #[must_use]
    pub fn name(&self) -> Option<&'a str> {
        self.record.text("name")
    }

    #[must_use]
    pub fn faction(&self) -> Option<&'a str> {
        self.record.text("faction")
    }

    /// The two-letter code `AsyncTI4` uses in fleet strings, e.g. `cv` for a carrier.
    #[must_use]
    pub fn async_id(&self) -> Option<&'a str> {
        self.record.text("asyncId")
    }

    // -- movement and transport ----------------------------------------------------

    #[must_use]
    pub fn move_value(&self) -> i64 {
        self.record.int("moveValue").unwrap_or(0)
    }

    #[must_use]
    pub fn capacity(&self) -> i64 {
        self.record.int("capacityValue").unwrap_or(0)
    }

    /// Hold space this unit occupies *if* transported.
    ///
    /// The corpus sets this to 1 on nearly every mobile unit, including carriers and
    /// dreadnoughts, so it states a per-unit cost rather than whether the unit actually
    /// consumes capacity. Use [`Self::consumes_capacity`] for that question.
    #[must_use]
    pub fn capacity_cost(&self) -> i64 {
        self.record.int("capacityUsed").unwrap_or(0)
    }

    /// Only fighters and ground forces take up capacity; capital ships do not.
    #[must_use]
    pub fn consumes_capacity(&self) -> bool {
        self.is_fighter() || (self.is_ground_force() && !self.is_structure())
    }

    #[must_use]
    pub fn is_ship(&self) -> bool {
        self.record.flag("isShip")
    }

    #[must_use]
    pub fn is_structure(&self) -> bool {
        self.record.flag("isStructure")
    }

    /// Ground forces are infantry and mechs — plus the Titans' PDS, which is a ground
    /// force that also happens to be a structure.
    #[must_use]
    pub fn is_ground_force(&self) -> bool {
        matches!(self.base_type(), "infantry" | "mech") || self.id() == "titans_pds2"
    }

    #[must_use]
    pub fn is_fighter(&self) -> bool {
        self.base_type() == "fighter"
    }

    // -- combat --------------------------------------------------------------------

    /// The roll this unit hits on, or `None` if it does not fight.
    #[must_use]
    pub fn combat_hits_on(&self) -> Option<i64> {
        positive(self.record.int("combatHitsOn"))
    }

    #[must_use]
    pub fn combat_dice(&self) -> i64 {
        self.record.int("combatDieCount").unwrap_or(0)
    }

    #[must_use]
    pub fn sustain_damage(&self) -> bool {
        self.record.flag("sustainDamage")
    }

    #[must_use]
    pub fn can_be_direct_hit(&self) -> bool {
        self.record.flag("canBeDirectHit")
    }

    /// Anti-Fighter Barrage value, or `None` if the unit has no barrage.
    #[must_use]
    pub fn afb_hits_on(&self) -> Option<i64> {
        positive(self.record.int("afbHitsOn"))
    }

    #[must_use]
    pub fn afb_dice(&self) -> i64 {
        self.record.int("afbDieCount").unwrap_or(0)
    }

    #[must_use]
    pub fn has_anti_fighter_barrage(&self) -> bool {
        self.afb_hits_on().is_some()
    }

    /// Fighters this unit supports without using ship capacity (16.2).
    ///
    /// Read out of the printed ability text rather than assumed, because a Dimensional Tear
    /// supports six or twelve where an ordinary dock supports three — treating every dock as
    /// the generic one would quietly under-count the faction's whole point.
    #[must_use]
    pub fn fighter_support(&self) -> i64 {
        let Some(ability) = self.record.text("ability") else {
            return 0;
        };
        let lower = ability.to_ascii_lowercase();
        let Some(start) = lower.find("up to ") else {
            return 0;
        };
        let rest = &lower[start + "up to ".len()..];
        if !rest.contains("fighter") {
            return 0;
        }
        rest.split_whitespace()
            .next()
            .and_then(|n| n.parse().ok())
            .unwrap_or(0)
    }

    #[must_use]
    pub fn bombard_hits_on(&self) -> Option<i64> {
        positive(self.record.int("bombardHitsOn"))
    }

    /// Bombardment dice, defaulting to one. Only meaningful when the unit bombards.
    #[must_use]
    pub fn bombard_dice(&self) -> i64 {
        self.record.int("bombardDieCount").unwrap_or(1)
    }

    #[must_use]
    pub fn has_bombardment(&self) -> bool {
        self.bombard_hits_on().is_some()
    }

    #[must_use]
    pub fn planetary_shield(&self) -> bool {
        self.record.flag("planetaryShield")
    }

    #[must_use]
    pub fn space_cannon_hits_on(&self) -> Option<i64> {
        positive(self.record.int("spaceCannonHitsOn"))
    }

    /// Space cannon dice, defaulting to one.
    #[must_use]
    pub fn space_cannon_dice(&self) -> i64 {
        self.record.int("spaceCannonDieCount").unwrap_or(1)
    }

    #[must_use]
    pub fn has_space_cannon(&self) -> bool {
        self.space_cannon_hits_on().is_some()
    }

    #[must_use]
    pub fn cost(&self) -> f64 {
        self.record.float("cost").unwrap_or(0.0)
    }

    /// Fighters and infantry come two to a cost (LRR 67.2).
    #[must_use]
    pub fn produces_two(&self) -> bool {
        let cost = self.cost();
        cost > 0.0 && cost < 1.0
    }

    #[must_use]
    pub fn upgrades_to(&self) -> Option<&'a str> {
        self.record.text("upgradesToUnitId")
    }

    #[must_use]
    pub fn upgrades_from(&self) -> Option<&'a str> {
        self.record.text("upgradesFromUnitId")
    }

    /// The technology that unlocks this unit, if it is an upgrade.
    #[must_use]
    pub fn required_technology(&self) -> Option<&'a str> {
        self.record.text("requiredTechId")
    }

    // -- production (LRR 68) -------------------------------------------------------

    #[must_use]
    pub fn has_production(&self) -> bool {
        self.record.raw("productionValue").is_some()
    }

    /// How many units this one may produce (LRR 68.1).
    ///
    /// The corpus writes generic space docks as `"+2"` with `basicProduction: "res"`,
    /// meaning the planet's resource value plus two, and faction docks such as the Clan of
    /// Saar's Floating Factory as a flat `"5"`. Both forms are data, so faction variation
    /// needs no code.
    #[must_use]
    pub fn production(&self, planet_resources: i64) -> i64 {
        let Some(raw) = self.record.raw("productionValue") else {
            return 0;
        };
        let text = raw.as_str().map_or_else(|| raw.to_string(), str::to_owned);
        let relative = text.starts_with('+') || self.record.text("basicProduction") == Some("res");
        let Ok(value) = text.trim_start_matches('+').parse::<i64>() else {
            return 0;
        };
        if relative {
            planet_resources + value
        } else {
            value
        }
    }
}

/// Python treats `0` as absent for hit values; a unit that "hits on 0" does not fight.
const fn positive(value: Option<i64>) -> Option<i64> {
    match value {
        Some(v) if v != 0 => Some(v),
        _ => None,
    }
}

/// Every unit type in scope, keyed by id.
#[must_use]
pub fn catalogue(store: &ContentStore, sources: SourceSet) -> BTreeMap<&str, UnitType<'_>> {
    store
        .from_sources(ContentType::Units, sources)
        .filter_map(|r| r.id().map(|id| (id, UnitType::new(r))))
        .collect()
}

/// One unit type by id, within a source scope.
#[must_use]
pub fn unit_type<'a>(
    store: &'a ContentStore,
    id: &str,
    sources: SourceSet,
) -> Option<UnitType<'a>> {
    let resolved = store.resolve_id(ContentType::Units, id, sources)?;
    store.get(ContentType::Units, resolved).map(UnitType::new)
}

/// A faction's own version of a unit type, e.g. Sol's mech.
///
/// Faction records name these directly, but sometimes by a Thunder's Edge id, so lookups go
/// through [`ContentStore::resolve_id`].
#[must_use]
pub fn faction_unit<'a>(
    store: &'a ContentStore,
    faction: &str,
    base_type: &str,
    sources: SourceSet,
) -> Option<UnitType<'a>> {
    let record = store.get(ContentType::Factions, faction)?;
    record
        .strings("units")
        .into_iter()
        .filter_map(|id| unit_type(store, id, sources))
        .find(|unit| unit.base_type() == base_type)
}

/// Non-faction unit types — the standard eight plus their upgrades.
#[must_use]
pub fn generic_types(store: &ContentStore, sources: SourceSet) -> BTreeMap<&str, UnitType<'_>> {
    catalogue(store, sources)
        .into_iter()
        .filter(|(_, unit)| unit.faction().is_none())
        .collect()
}

/// Total transport capacity provided by the ships in a group.
#[must_use]
pub fn fleet_capacity(units: &[UnitType<'_>]) -> i64 {
    units.iter().map(UnitType::capacity).sum()
}

/// Capacity consumed by the fighters and ground forces in a group.
#[must_use]
pub fn fleet_capacity_used(units: &[UnitType<'_>]) -> i64 {
    units
        .iter()
        .filter(|u| u.consumes_capacity())
        .map(UnitType::capacity_cost)
        .sum()
}

/// Whether a group fits in its own hold.
#[must_use]
pub fn within_capacity(units: &[UnitType<'_>]) -> bool {
    fleet_capacity_used(units) <= fleet_capacity(units)
}

/// The slowest ship sets how far a fleet can move together. Zero if there are no ships.
#[must_use]
pub fn fleet_move_value(units: &[UnitType<'_>]) -> i64 {
    units
        .iter()
        .filter(|u| u.is_ship())
        .map(UnitType::move_value)
        .min()
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ti4_model::content_types::{FULL, POK};

    fn store() -> &'static ContentStore {
        ContentStore::embedded()
    }

    fn unit(id: &str) -> UnitType<'static> {
        unit_type(store(), id, FULL).unwrap_or_else(|| panic!("no unit {id}"))
    }

    #[test]
    fn carrier_stats_come_from_data() {
        let carrier = unit("carrier");
        assert_eq!(carrier.name(), Some("Carrier I"));
        assert_eq!(carrier.move_value(), 1);
        assert_eq!(carrier.capacity(), 4);
        assert_eq!(carrier.combat_hits_on(), Some(9));
        assert_eq!(carrier.combat_dice(), 1);
        assert!((carrier.cost() - 3.0).abs() < f64::EPSILON);
        assert!(carrier.is_ship());
    }

    #[test]
    fn a_unit_upgrade_shares_its_base_type_and_improves_it() {
        let carrier = unit("carrier");
        let carrier2 = unit("carrier2");
        assert_eq!(carrier.base_type(), carrier2.base_type());
        assert_eq!(carrier.upgrades_to(), Some("carrier2"));
        assert_eq!(carrier2.upgrades_from(), Some("carrier"));
        assert!(carrier2.move_value() > carrier.move_value());
        assert!(carrier2.capacity() > carrier.capacity());
        assert_eq!(carrier2.required_technology(), Some("cv2"));
    }

    #[test]
    fn ground_forces_are_not_ships() {
        assert!(unit("infantry").is_ground_force());
        assert!(!unit("infantry").is_ship());
        assert!(unit("mech").is_ground_force());
        assert!(!unit("carrier").is_ground_force());
    }

    #[test]
    fn the_titans_pds_is_a_ground_force_despite_being_a_structure() {
        let pds = unit("titans_pds2");
        assert!(pds.is_ground_force());
        assert!(pds.is_structure());
        // It is a structure, so it does not eat capacity even though it is a ground force.
        assert!(!pds.consumes_capacity());
    }

    #[test]
    fn capital_ships_do_not_consume_capacity_despite_declaring_a_cost() {
        let carrier = unit("carrier");
        // The corpus says 1, which is the per-unit cost *if* carried, not a claim that a
        // carrier fills its own hold.
        assert_eq!(carrier.capacity_cost(), 1);
        assert!(!carrier.consumes_capacity());
        assert!(!unit("dreadnought").consumes_capacity());
    }

    #[test]
    fn fighters_consume_capacity_despite_being_ships() {
        let fighter = unit("fighter");
        assert!(fighter.is_ship());
        assert!(fighter.is_fighter());
        assert!(fighter.consumes_capacity());
    }

    #[test]
    fn fighters_and_infantry_come_two_to_a_cost() {
        assert!(unit("fighter").produces_two());
        assert!(unit("infantry").produces_two());
        assert!(!unit("carrier").produces_two());
        assert!(!unit("dreadnought").produces_two());
    }

    #[test]
    fn a_generic_space_dock_produces_planet_resources_plus_two() {
        let dock = unit("spacedock");
        assert!(dock.has_production());
        assert_eq!(dock.production(0), 2);
        assert_eq!(dock.production(3), 5);
    }

    #[test]
    fn a_faction_dock_may_produce_a_flat_amount() {
        // The Clan of Saar's Floating Factory produces a fixed number regardless of where
        // it sits, so faction variation is data rather than a branch.
        let saar = faction_unit(store(), "saar", "spacedock", POK).expect("saar has a dock");
        assert!(saar.has_production());
        assert_eq!(saar.production(0), saar.production(5));
    }

    #[test]
    fn a_unit_without_production_produces_nothing() {
        assert!(!unit("carrier").has_production());
        assert_eq!(unit("carrier").production(5), 0);
    }

    #[test]
    fn a_fleet_moves_at_the_speed_of_its_slowest_ship() {
        let fleet = [unit("carrier"), unit("destroyer"), unit("cruiser")];
        assert_eq!(fleet_move_value(&fleet), 1, "the carrier is the slowest");
        let fast = [unit("destroyer"), unit("cruiser")];
        assert!(fleet_move_value(&fast) >= 2);
    }

    #[test]
    fn a_fleet_of_no_ships_does_not_move() {
        assert_eq!(fleet_move_value(&[unit("infantry")]), 0);
        assert_eq!(fleet_move_value(&[]), 0);
    }

    #[test]
    fn fleet_capacity_counts_only_what_is_carried() {
        let fleet = [unit("carrier"), unit("fighter"), unit("infantry")];
        assert_eq!(
            fleet_capacity(&fleet),
            4,
            "only the carrier provides capacity"
        );
        assert_eq!(
            fleet_capacity_used(&fleet),
            2,
            "the fighter and the infantry"
        );
        assert!(within_capacity(&fleet));
    }

    #[test]
    fn overloading_a_fleet_is_detectable() {
        let mut fleet = vec![unit("carrier")];
        fleet.extend(std::iter::repeat_n(unit("fighter"), 5));
        assert_eq!(fleet_capacity(&fleet), 4);
        assert_eq!(fleet_capacity_used(&fleet), 5);
        assert!(!within_capacity(&fleet));
    }

    #[test]
    fn a_war_sun_bombards_and_a_carrier_does_not() {
        let war_sun = unit("warsun");
        assert!(war_sun.has_bombardment());
        assert_eq!(war_sun.bombard_hits_on(), Some(3));
        assert_eq!(war_sun.bombard_dice(), 3);
        assert!(!unit("carrier").has_bombardment());
    }

    #[test]
    fn a_destroyer_has_anti_fighter_barrage() {
        let destroyer = unit("destroyer");
        assert!(destroyer.has_anti_fighter_barrage());
        assert!(destroyer.afb_dice() >= 2);
        assert!(!unit("carrier").has_anti_fighter_barrage());
    }

    #[test]
    fn a_pds_has_space_cannon_and_a_planetary_shield() {
        let pds = unit("pds");
        assert!(pds.has_space_cannon());
        assert_eq!(
            pds.space_cannon_dice(),
            1,
            "one die unless stated otherwise"
        );
        assert!(pds.planetary_shield());
    }

    #[test]
    fn a_dreadnought_sustains_damage() {
        assert!(unit("dreadnought").sustain_damage());
        assert!(!unit("destroyer").sustain_damage());
    }

    #[test]
    fn a_unit_that_does_not_fight_has_no_combat_value() {
        // Absent and zero both mean "does not fight"; neither may read as hitting on 0.
        let spacedock = unit("spacedock");
        assert_eq!(spacedock.combat_hits_on(), None);
        assert_eq!(spacedock.combat_dice(), 0);
    }

    #[test]
    fn generic_types_exclude_faction_specific_ones() {
        let generic = generic_types(store(), FULL);
        assert!(generic.contains_key("carrier"));
        assert!(!generic.contains_key("sol_carrier"));
        assert!(generic.values().all(|u| u.faction().is_none()));
    }

    #[test]
    fn a_faction_gets_its_own_version_of_a_unit() {
        let sol = faction_unit(store(), "sol", "carrier", POK).expect("Sol has a carrier");
        assert_eq!(sol.id(), "sol_carrier");
        assert_eq!(sol.faction(), Some("sol"));
        assert_eq!(sol.base_type(), "carrier");
    }

    #[test]
    fn a_faction_without_a_special_version_falls_back_to_nothing() {
        // Sol has no faction destroyer, so the lookup finds the generic one they were
        // given in their unit list rather than inventing a Sol destroyer.
        let destroyer = faction_unit(store(), "sol", "destroyer", POK).unwrap();
        assert_eq!(destroyer.id(), "destroyer");
        assert_eq!(destroyer.faction(), None);
    }

    #[test]
    fn the_naalu_mech_resolves_through_its_thunders_edge_id() {
        // The faction record names `naalu_mech_te`; under POK that must still find a mech.
        let mech = faction_unit(store(), "naalu", "mech", POK).expect("Naalu must have a mech");
        assert_eq!(mech.base_type(), "mech");
        assert_eq!(mech.faction(), Some("naalu"));
    }

    #[test]
    fn every_unit_in_the_catalogue_has_a_base_type() {
        for (id, unit) in catalogue(store(), FULL) {
            assert!(!unit.base_type().is_empty(), "{id} has no base type");
        }
    }

    #[test]
    fn every_faction_can_field_the_units_it_names() {
        for faction in store().factions(POK) {
            let alias = faction.id().unwrap();
            for named in faction.strings("units") {
                assert!(
                    unit_type(store(), named, POK).is_some(),
                    "{alias} names unit {named}, which does not resolve under POK"
                );
            }
        }
    }
}
