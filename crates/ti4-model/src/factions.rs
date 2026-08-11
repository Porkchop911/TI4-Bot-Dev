//! Faction records and capabilities.

use crate::id::UnitTypeId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FactionRecord {
    pub alias: String,
    pub name: String,
    pub home_system: String,
    pub home_planets: Vec<String>,
    pub commodities: i32,
    pub starting_tech: Vec<String>,
    pub faction_tech: Vec<String>,
    pub abilities: Vec<String>,
    pub leaders: Vec<String>,
    pub complexity: Option<String>,
    pub starting_fleet: String,
}

impl FactionRecord {
    pub fn deployments(&self) -> Vec<Deployment> {
        parse_fleet(&self.starting_fleet, &self.home_planets)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Deployment {
    pub count: i32,
    pub unit_id: UnitTypeId,
    pub where_: DeploymentLocation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeploymentLocation {
    Planet(String),
    Space,
}

/// Complexity levels for faction selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Complexity {
    Low,
    Medium,
    High,
}

impl FactionRecord {
    pub fn complexity_level(&self) -> Option<Complexity> {
        match self.complexity.as_deref() {
            Some("Low") => Some(Complexity::Low),
            Some("Medium") => Some(Complexity::Medium),
            Some("High") => Some(Complexity::High),
            _ => None,
        }
    }
}

/// Parse the starting fleet string into deployments.
/// Format: "[count] <code> [where]" repeated, comma-separated.
/// Codes: cv=carrier, cr/ca=cruiser, dd=destroyer, dn=dreadnought, ff=fighter,
///         inf/gf=infantry, pds/pd=pds, sd=spacedock, ws=warsun, fs=flagship,
///         mech=mech
fn parse_fleet(fleet: &str, home_planets: &[String]) -> Vec<Deployment> {
    let mut out = Vec::new();
    for part in fleet.split(',') {
        let part = part.trim();
        if part.is_empty() { continue; }
        if let Some(dep) = parse_fleet_entry(part, home_planets) {
            out.push(dep);
        }
    }
    out
}

fn parse_fleet_entry(part: &str, home_planets: &[String]) -> Option<Deployment> {
    // Match "[count] code [where]" or "code [where]"
    let parts: Vec<&str> = part.split_whitespace().collect();
    if parts.is_empty() { return None; }

    let (count, rest) = if let Ok(c) = parts[0].parse::<i32>() {
        (c, &parts[1..])
    } else {
        (1, parts.as_slice())
    };

    if rest.is_empty() { return None; }

    let code = rest[0];
    let where_token = rest.get(1).copied();

    let unit_id = match code {
        "cv" | "carrier" => UnitTypeId::new("carrier"),
        "cr" | "ca" | "cruiser" => UnitTypeId::new("cruiser"),
        "dd" | "destroyer" => UnitTypeId::new("destroyer"),
        "dn" | "dreadnought" => UnitTypeId::new("dreadnought"),
        "ff" | "fighter" => UnitTypeId::new("fighter"),
        "inf" | "gf" | "infantry" => UnitTypeId::new("infantry"),
        "pds" | "pd" | "pds_unit" => UnitTypeId::new("pds"),
        "sd" | "spacedock" => UnitTypeId::new("spacedock"),
        "ws" | "warsun" => UnitTypeId::new("warsun"),
        "fs" | "flagship" => UnitTypeId::new("flagship"),
        "mech" => UnitTypeId::new("mech"),
        _ => return None,
    };

    let where_ = match where_token {
        Some(w) if w != "space" => DeploymentLocation::Planet(w.to_string()),
        _ => DeploymentLocation::Space,
    };

    Some(Deployment { count, unit_id, where_ })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_fleet_simple() {
        let deps = parse_fleet("1 cv, 2 ff", &["home1".to_string()]);
        assert_eq!(deps.len(), 2);
        assert_eq!(deps[0].count, 1);
        assert_eq!(deps[1].count, 2);
        assert!(matches!(deps[0].where_, DeploymentLocation::Space));
        assert!(matches!(deps[1].where_, DeploymentLocation::Space));
    }

    #[test]
    fn test_parse_fleet_with_planet() {
        let deps = parse_fleet("3 inf, 1 sd home1", &["home1".to_string()]);
        assert_eq!(deps.len(), 2);
        assert_eq!(deps[0].count, 3);
        assert!(matches!(deps[0].where_, DeploymentLocation::Space));
        assert_eq!(deps[1].count, 1);
        assert!(matches!(&deps[1].where_, DeploymentLocation::Planet(p) if p == "home1"));
    }
}
