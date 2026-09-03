//! Factions and their opening positions.
//!
//! Faction records carry their opening position as a terse string —
//! `"2 cv, dd, 3 ff,5 inf j, sd j"` — which is `[count] <code> [where]` repeated. The code
//! is a unit abbreviation and `where` names a home planet; its absence means the space area,
//! which is how the Clan of Saar's Floating Factory starts afloat.
//!
//! Two details make the string harder than it looks:
//!
//! * The abbreviations are **not** the corpus's `asyncId` values. Starting fleets say `cr`,
//!   `inf`, and `pds` where `asyncId` says `ca`, `gf`, and `pd`, and `ws` means a war sun
//!   rather than the `nowarsun` placeholder it maps to there.
//! * The planet reference is usually a prefix of the planet id, but Xxcha writes `at` and
//!   `ar` for Archon Tau and Archon Ren — the initials of the name's words. Since `ar` also
//!   prefixes *both* Xxcha planets, initials are tried first and prefixes only as a
//!   fallback, and an ambiguous prefix is an error rather than a guess.
//!
//! Every base faction is asserted to parse and resolve in the tests, which is the only
//! reason to trust a format this compressed.

use std::collections::BTreeMap;

use ti4_model::content_types::{ContentType, SourceSet};
use ti4_model::id::{PlanetId, UnitTypeId};

use crate::galaxy::all_planets;
use crate::loader::ContentStore;
use crate::record::Record;
use crate::units::faction_unit;

/// Starting-fleet abbreviations. Deliberately separate from the corpus's `asyncId` map.
const FLEET_CODES: [(&str, &str); 14] = [
    ("cv", "carrier"),
    ("cr", "cruiser"),
    ("ca", "cruiser"),
    ("dd", "destroyer"),
    ("dn", "dreadnought"),
    ("ff", "fighter"),
    ("inf", "infantry"),
    ("gf", "infantry"),
    ("pds", "pds"),
    ("pd", "pds"),
    ("sd", "spacedock"),
    ("ws", "warsun"),
    ("fs", "flagship"),
    // Faction-specific: resolved per player at deploy time, not to a single id.
    ("mech", "mech"),
];

/// Where one starting unit goes.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Placement {
    /// The system's space area — the fleet string named no planet.
    Space,
    /// A named home planet.
    Planet(PlanetId),
}

/// One entry from a starting fleet: so many of a unit, in one place.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Deployment {
    pub count: u32,
    /// The generic unit type. It still needs resolving against the faction — see
    /// [`resolve_unit`] — because faction sheets can replace any hull, infantry, or structure.
    pub unit_id: UnitTypeId,
    pub placement: Placement,
}

/// A starting fleet that could not be read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FleetError {
    #[error("cannot parse starting fleet entry {0:?}")]
    Unparsable(String),
    #[error("unknown unit code {code:?} in {entry:?}")]
    UnknownCode { code: String, entry: String },
    #[error("planet reference {token:?} is ambiguous among {candidates:?}")]
    AmbiguousPlanet {
        token: String,
        candidates: Vec<String>,
    },
    #[error("planet reference {token:?} is unknown among {candidates:?}")]
    UnknownPlanet {
        token: String,
        candidates: Vec<String>,
    },
    #[error("no faction {0:?} in the corpus for the requested sources")]
    UnknownFaction(String),
}

/// A typed view over a faction record.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Faction<'a> {
    record: &'a Record,
}

impl<'a> Faction<'a> {
    #[must_use]
    pub const fn new(record: &'a Record) -> Self {
        Self { record }
    }

    #[must_use]
    pub const fn record(&self) -> &'a Record {
        self.record
    }

    #[must_use]
    pub fn alias(&self) -> &'a str {
        self.record.id().unwrap_or_default()
    }

    #[must_use]
    pub fn name(&self) -> Option<&'a str> {
        self.record.text("factionName")
    }

    #[must_use]
    pub fn home_system(&self) -> Option<&'a str> {
        self.record.text("homeSystem").filter(|s| !s.is_empty())
    }

    #[must_use]
    pub fn home_planets(&self) -> Vec<&'a str> {
        self.record.strings("homePlanets")
    }

    #[must_use]
    pub fn commodities(&self) -> i32 {
        i32::try_from(self.record.int("commodities").unwrap_or(0)).unwrap_or(0)
    }

    #[must_use]
    pub fn starting_tech(&self) -> Vec<&'a str> {
        self.record.strings("startingTech")
    }

    #[must_use]
    pub fn faction_tech(&self) -> Vec<&'a str> {
        self.record.strings("factionTech")
    }

    #[must_use]
    pub fn abilities(&self) -> Vec<&'a str> {
        self.record.strings("abilities")
    }

    #[must_use]
    pub fn leaders(&self) -> Vec<&'a str> {
        self.record.strings("leaders")
    }

    #[must_use]
    pub fn promissory_notes(&self) -> Vec<&'a str> {
        self.record.strings("promissoryNotes")
    }

    /// Mechanical simplicity, e.g. `"Low"` — not strategic difficulty. Jol-Nar is rated
    /// Low despite being awkward to pilot.
    #[must_use]
    pub fn complexity(&self) -> Option<&'a str> {
        self.record.text("complexity")
    }

    #[must_use]
    pub fn starting_fleet(&self) -> &'a str {
        self.record.text("startingFleet").unwrap_or_default()
    }

    /// The opening position, parsed.
    ///
    /// # Errors
    /// Any [`FleetError`] from reading the fleet string.
    pub fn deployments(&self, store: &ContentStore) -> Result<Vec<Deployment>, FleetError> {
        parse_fleet(store, self.starting_fleet(), &self.home_planets())
    }
}

/// Every faction in scope, keyed by alias.
#[must_use]
pub fn catalogue(store: &ContentStore, sources: SourceSet) -> BTreeMap<&str, Faction<'_>> {
    store
        .from_sources(ContentType::Factions, sources)
        .filter_map(|r| r.id().map(|id| (id, Faction::new(r))))
        .collect()
}

/// One faction by alias.
#[must_use]
pub fn get<'a>(store: &'a ContentStore, alias: &str) -> Option<Faction<'a>> {
    store.get(ContentType::Factions, alias).map(Faction::new)
}

/// The initials of each word in a name, lowercased. `"Archon Tau"` becomes `"at"`.
///
/// Words split on spaces, apostrophes, and hyphens, matching the oracle's
/// `re.split(r"[ '\-]+", name)`.
fn initials(name: &str) -> String {
    name.split([' ', '\'', '-'])
        .filter(|word| !word.is_empty())
        .filter_map(|word| word.chars().next())
        .flat_map(char::to_lowercase)
        .collect()
}

/// Turn a fleet string's planet reference into a planet id.
///
/// Initials are tried before prefixes because `ar` prefixes both of Xxcha's planets while
/// naming exactly one of them by initials. An ambiguous prefix is an error, not a guess.
///
/// # Errors
/// [`FleetError::AmbiguousPlanet`] or [`FleetError::UnknownPlanet`].
pub fn resolve_planet(
    store: &ContentStore,
    token: &str,
    home_planets: &[&str],
) -> Result<PlanetId, FleetError> {
    let planets = all_planets(store, ti4_model::content_types::FULL);
    for planet_id in home_planets {
        let name = planets
            .get(planet_id)
            .and_then(crate::galaxy::Planet::name)
            .unwrap_or(planet_id);
        if initials(name) == token {
            return Ok(PlanetId::new(*planet_id));
        }
    }

    let matches: Vec<&str> = home_planets
        .iter()
        .filter(|p| p.starts_with(token))
        .copied()
        .collect();
    let candidates = home_planets.iter().map(|p| (*p).to_owned()).collect();
    match matches.len() {
        1 => Ok(PlanetId::new(matches[0])),
        0 => Err(FleetError::UnknownPlanet {
            token: token.to_owned(),
            candidates,
        }),
        _ => Err(FleetError::AmbiguousPlanet {
            token: token.to_owned(),
            candidates,
        }),
    }
}

/// Read a starting fleet string into deployments.
///
/// # Errors
/// [`FleetError::Unparsable`] for a malformed entry, [`FleetError::UnknownCode`] for an
/// unrecognised abbreviation — neither is skipped, because a silently dropped entry is a
/// faction that quietly starts a ship short.
pub fn parse_fleet(
    store: &ContentStore,
    fleet: &str,
    home_planets: &[&str],
) -> Result<Vec<Deployment>, FleetError> {
    let mut out = Vec::new();
    for entry in fleet.split(',') {
        let entry = entry.trim();
        if entry.is_empty() {
            continue;
        }
        out.push(parse_entry(store, entry, home_planets)?);
    }
    Ok(out)
}

/// One entry: `[count] <code> [where]`, matching `^(\d+)?\s*([a-z]+)\s*(\S+)?$`.
fn parse_entry(
    store: &ContentStore,
    entry: &str,
    home_planets: &[&str],
) -> Result<Deployment, FleetError> {
    let unparsable = || FleetError::Unparsable(entry.to_owned());

    let rest = entry.trim_start();
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    let rest = rest[digits.len()..].trim_start();

    let code: String = rest.chars().take_while(char::is_ascii_lowercase).collect();
    if code.is_empty() {
        return Err(unparsable());
    }
    let rest = rest[code.len()..].trim_start();

    // Whatever remains is the planet reference, and it must be a single token.
    let where_token = if rest.is_empty() { None } else { Some(rest) };
    if where_token.is_some_and(|w| w.split_whitespace().count() != 1) {
        return Err(unparsable());
    }

    let unit_id = FLEET_CODES
        .iter()
        .find(|(abbreviation, _)| *abbreviation == code)
        .map(|(_, unit)| *unit)
        .ok_or_else(|| FleetError::UnknownCode {
            code: code.clone(),
            entry: entry.to_owned(),
        })?;

    let count = if digits.is_empty() {
        1
    } else {
        digits.parse().map_err(|_| unparsable())?
    };

    let placement = match where_token {
        Some(token) => Placement::Planet(resolve_planet(store, token, home_planets)?),
        None => Placement::Space,
    };

    Ok(Deployment {
        count,
        unit_id: UnitTypeId::new(unit_id),
        placement,
    })
}

/// The faction's own version of a generic starting unit, where one exists.
///
/// Starting-fleet strings use generic codes even when a faction sheet replaces that unit. Resolve
/// every type, not only mechs and flagships: L1Z1X's `dn` is a Super Dreadnought with capacity 2,
/// and Saar/Cabal `sd` entries are their faction production units. Falls back to the generic id so
/// a faction with no replacement still gets the ordinary unit.
#[must_use]
pub fn resolve_unit(
    store: &ContentStore,
    faction: &str,
    unit_id: &UnitTypeId,
    sources: SourceSet,
) -> UnitTypeId {
    faction_unit(store, faction, unit_id.as_str(), sources)
        .map_or_else(|| unit_id.clone(), |u| UnitTypeId::new(u.id()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ti4_model::content_types::{BASE, FULL, POK};

    fn store() -> &'static ContentStore {
        ContentStore::embedded()
    }

    fn faction(alias: &str) -> Faction<'static> {
        get(store(), alias).unwrap_or_else(|| panic!("no faction {alias}"))
    }

    fn deployments(alias: &str) -> Vec<Deployment> {
        faction(alias).deployments(store()).unwrap()
    }

    fn count_of(deps: &[Deployment], unit: &str) -> u32 {
        deps.iter()
            .filter(|d| d.unit_id.as_str() == unit)
            .map(|d| d.count)
            .sum()
    }

    // -- the catalogue ---------------------------------------------------------

    #[test]
    fn the_base_game_has_seventeen_factions() {
        assert_eq!(catalogue(store(), BASE).len(), 17);
    }

    #[test]
    fn every_official_faction_is_reachable() {
        assert_eq!(catalogue(store(), FULL).len(), 34);
    }

    #[test]
    fn low_complexity_factions_are_listed() {
        let low: Vec<&str> = catalogue(store(), FULL)
            .into_iter()
            .filter(|(_, f)| f.complexity() == Some("Low"))
            .map(|(alias, _)| alias)
            .collect();
        assert!(low.contains(&"sol"));
        // Rated for mechanical simplicity, not strategic difficulty.
        assert!(
            low.contains(&"jolnar"),
            "Jol-Nar is Low despite being hard to pilot"
        );
    }

    // -- parsing the starting fleet ----------------------------------------------

    #[test]
    fn every_base_faction_parses_and_resolves() {
        // The only reason to trust a format this compressed.
        for (alias, faction) in catalogue(store(), BASE) {
            let deps = faction
                .deployments(store())
                .unwrap_or_else(|e| panic!("{alias}: {e}"));
            assert!(!deps.is_empty(), "{alias} deploys nothing");
        }
    }

    #[test]
    fn every_official_faction_parses_and_resolves() {
        for (alias, faction) in catalogue(store(), FULL) {
            if faction.starting_fleet().is_empty() {
                continue; // the `neutral` placeholder is never seated
            }
            faction
                .deployments(store())
                .unwrap_or_else(|e| panic!("{alias}: {e}"));
        }
    }

    #[test]
    fn sol_opens_with_two_carriers_and_five_infantry_on_jord() {
        let deps = deployments("sol");
        assert_eq!(count_of(&deps, "carrier"), 2);
        assert_eq!(count_of(&deps, "infantry"), 5);
        let infantry = deps
            .iter()
            .find(|d| d.unit_id.as_str() == "infantry")
            .unwrap();
        assert_eq!(infantry.placement, Placement::Planet(PlanetId::new("jord")));
    }

    #[test]
    fn muaat_opens_with_a_war_sun() {
        // `ws` means a war sun here, not the `nowarsun` placeholder asyncId maps it to.
        assert_eq!(count_of(&deployments("muaat"), "warsun"), 1);
    }

    #[test]
    fn the_clan_of_saar_starts_with_its_space_dock_afloat() {
        // No planet named means the space area, which is the whole point of the format.
        let dock = deployments("saar")
            .into_iter()
            .find(|d| d.unit_id.as_str() == "spacedock")
            .expect("Saar has a dock");
        assert_eq!(dock.placement, Placement::Space);
    }

    #[test]
    fn a_count_defaults_to_one() {
        let deps = parse_fleet(store(), "dd, 2 ff", &["jord"]).unwrap();
        assert_eq!(deps[0].count, 1);
        assert_eq!(deps[1].count, 2);
    }

    // -- planet references --------------------------------------------------------

    #[test]
    fn initials_beat_an_ambiguous_prefix() {
        // Xxcha writes `at` and `ar` for Archon Tau and Archon Ren. `ar` also prefixes
        // *both* planets, so a prefix-first resolver would fail or guess wrong.
        let xxcha = faction("xxcha");
        let planets = xxcha.home_planets();
        assert_eq!(planets.len(), 2);
        let tau = resolve_planet(store(), "at", &planets).unwrap();
        let ren = resolve_planet(store(), "ar", &planets).unwrap();
        assert_ne!(
            tau, ren,
            "the two references must not collapse to one planet"
        );
    }

    #[test]
    fn xxcha_splits_correctly_between_its_two_archons() {
        let deps = deployments("xxcha");
        let planets: std::collections::BTreeSet<&Placement> = deps
            .iter()
            .map(|d| &d.placement)
            .filter(|p| matches!(p, Placement::Planet(_)))
            .collect();
        assert_eq!(planets.len(), 2, "units land on both archons");
    }

    #[test]
    fn hacan_spreads_across_three_planets() {
        assert_eq!(faction("hacan").home_planets().len(), 3);
        let deps = deployments("hacan");
        let planets: std::collections::BTreeSet<&Placement> = deps
            .iter()
            .map(|d| &d.placement)
            .filter(|p| matches!(p, Placement::Planet(_)))
            .collect();
        assert_eq!(planets.len(), 3);
    }

    #[test]
    fn a_unique_prefix_still_works() {
        assert_eq!(
            resolve_planet(store(), "j", &["jord"]).unwrap(),
            PlanetId::new("jord")
        );
    }

    #[test]
    fn an_ambiguous_prefix_is_an_error_rather_than_a_guess() {
        let err = resolve_planet(store(), "arc", &["arcprime", "arcsecond"]).unwrap_err();
        assert!(matches!(err, FleetError::AmbiguousPlanet { .. }), "{err}");
    }

    #[test]
    fn an_unknown_planet_reference_is_an_error() {
        let err = resolve_planet(store(), "zz", &["jord"]).unwrap_err();
        assert!(matches!(err, FleetError::UnknownPlanet { .. }), "{err}");
    }

    // -- error handling -------------------------------------------------------------

    #[test]
    fn an_unknown_unit_code_is_an_error_not_a_silently_dropped_entry() {
        // Skipping it would leave a faction quietly starting a ship short.
        let err = parse_fleet(store(), "2 zz", &["jord"]).unwrap_err();
        assert!(
            matches!(err, FleetError::UnknownCode { ref code, .. } if code == "zz"),
            "{err}"
        );
    }

    #[test]
    fn a_malformed_entry_is_an_error() {
        assert!(parse_fleet(store(), "2", &["jord"]).is_err());
        assert!(parse_fleet(store(), "2 cv jord extra", &["jord"]).is_err());
    }

    #[test]
    fn an_empty_fleet_deploys_nothing_without_erroring() {
        assert!(parse_fleet(store(), "", &[]).unwrap().is_empty());
        assert!(parse_fleet(store(), " , ", &[]).unwrap().is_empty());
    }

    // -- faction-specific units -------------------------------------------------------

    #[test]
    fn a_mech_resolves_to_the_factions_own_version() {
        let generic = UnitTypeId::new("mech");
        let sol = resolve_unit(store(), "sol", &generic, POK);
        assert_eq!(sol.as_str(), "sol_mech");
        assert_ne!(sol, generic);
    }

    #[test]
    fn a_flagship_resolves_to_the_factions_own_version() {
        let flagship = resolve_unit(store(), "sol", &UnitTypeId::new("flagship"), POK);
        assert_eq!(flagship.as_str(), "sol_flagship");
    }

    #[test]
    fn an_ordinary_unit_without_a_faction_replacement_stays_generic() {
        let carrier = UnitTypeId::new("carrier");
        assert_eq!(resolve_unit(store(), "hacan", &carrier, POK), carrier);
    }

    #[test]
    fn l1z1x_starting_dreadnought_resolves_to_capacity_two() {
        let dreadnought = resolve_unit(store(), "l1z1x", &UnitTypeId::new("dreadnought"), POK);
        assert_eq!(dreadnought.as_str(), "l1z1x_dreadnought");
        assert_eq!(
            crate::units::unit_type(store(), dreadnought.as_str(), POK)
                .expect("the resolved hull exists")
                .capacity(),
            2
        );
    }

    #[test]
    fn faction_production_units_are_resolved_in_starting_fleets() {
        let dock = resolve_unit(store(), "saar", &UnitTypeId::new("spacedock"), POK);
        assert_eq!(dock.as_str(), "saar_spacedock");
        assert_eq!(
            crate::units::unit_type(store(), dock.as_str(), POK)
                .expect("the resolved dock exists")
                .production(0),
            5
        );
    }

    #[test]
    fn the_naalu_mech_resolves_through_its_thunders_edge_id() {
        let mech = resolve_unit(store(), "naalu", &UnitTypeId::new("mech"), POK);
        assert_ne!(mech.as_str(), "mech", "the Naalu must get their own mech");
    }

    // -- initials --------------------------------------------------------------------

    #[test]
    fn initials_split_on_spaces_apostrophes_and_hyphens() {
        assert_eq!(initials("Archon Tau"), "at");
        assert_eq!(initials("Jord"), "j");
        assert_eq!(initials("Mecatol Rex"), "mr");
        // Three words, not two: the hyphen and the apostrophe both split.
        assert_eq!(initials("Quinarra-Tren'lak"), "qtl");
    }
}
