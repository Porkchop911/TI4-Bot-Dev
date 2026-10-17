//! Builds a galaxy from a predefined map template.

use std::collections::BTreeSet;

use ti4_content::ContentStore;
use ti4_content::galaxy::{self, Galaxy, GalaxyError};
use ti4_engine::seating;
use ti4_model::Hex;
use ti4_model::content_types::SourceSet;
use ti4_model::id::SystemId;

use super::template_loader::{MapTemplate, TemplateLoader};

#[derive(Debug, thiserror::Error)]
pub enum TemplateError {
    #[error("unknown map template {0:?}")]
    Unknown(String),
    #[error("map template {alias:?} seats {expected} players, but the game has {given}")]
    PlayerCount {
        alias: String,
        expected: usize,
        given: usize,
    },
    #[error("map template {alias:?} has a bad position {pos:?}")]
    BadPosition { alias: String, pos: String },
    #[error("map template {alias:?} ran out of filler tiles")]
    OutOfFiller { alias: String },
    #[error(transparent)]
    Galaxy(#[from] GalaxyError),
}

/// `"RNN"` is ring `R`, clockwise index `NN` (1-based) in [`Hex::ring`] order, which starts at a
/// corner for every ring, so `x01` positions line up and ring adjacency is preserved.
pub fn hex_of(pos: &str) -> Option<Hex> {
    let ring = i32::try_from(pos.get(..1)?.parse::<u32>().ok()?).ok()?;
    let index: usize = pos.get(1..)?.parse().ok()?;
    if ring == 0 {
        return (index == 0).then_some(Hex::ORIGIN);
    }
    Hex::ring(ring).get(index.checked_sub(1)?).copied()
}

/// Static tile ids are not always the corpus spelling: case varies (`84A1`), and a hyperlane
/// carries its rotation as a trailing 0-5 (`83a2`) where the corpus names it `83a` or `83a<degrees>`.
fn resolve_static<'a>(known: &BTreeSet<&str>, id: &'a str) -> std::borrow::Cow<'a, str> {
    let lower = id.to_ascii_lowercase();
    for candidate in [id, lower.as_str()] {
        if known.contains(candidate) {
            return candidate.to_owned().into();
        }
    }
    let bytes = lower.as_bytes();
    if bytes.len() == 4 && bytes[2].is_ascii_lowercase() && matches!(bytes[3], b'0'..=b'5') {
        let turn = u32::from(bytes[3] - b'0') * 60;
        let base = &lower[..3];
        let rotated = if turn == 0 {
            base.to_owned()
        } else {
            format!("{base}{turn}")
        };
        if known.contains(rotated.as_str()) {
            return rotated.into();
        }
    }
    id.into()
}

/// Static tiles as given, `homes[n - 1]` at each player `n`'s home slot, and every other slot
/// from seeded filler (never a tile the template or a home already uses).
///
/// # Errors
/// [`TemplateError`] for a seat-count mismatch, a bad position, too little filler, or any
/// [`GalaxyError`] (unknown or duplicate tile).
pub fn build_template_galaxy(
    content: &ContentStore,
    template: &MapTemplate,
    homes: &[SystemId],
    seed: u64,
    sources: SourceSet,
) -> Result<Galaxy, TemplateError> {
    let alias = &template.alias;
    if template.player_count != homes.len() {
        return Err(TemplateError::PlayerCount {
            alias: alias.clone(),
            expected: template.player_count,
            given: homes.len(),
        });
    }
    let catalogue = galaxy::all_systems(content, sources);
    let known: BTreeSet<&str> = catalogue.keys().copied().collect();
    let bad = |pos: &str| TemplateError::BadPosition {
        alias: alias.clone(),
        pos: pos.to_owned(),
    };

    let mut placed: Vec<(String, Hex)> = Vec::new();
    let mut open: Vec<Hex> = Vec::new();
    for tile in &template.template_tiles {
        let hex = hex_of(&tile.pos).ok_or_else(|| bad(&tile.pos))?;
        if let Some(id) = &tile.static_tile_id {
            let mut id = resolve_static(&known, id).into_owned();
            // HACK (2026-10-09): the large templates place one hyperlane tile several times, and
            // a galaxy holds a system once. Hyperlane paths are not modelled, so any other
            // hyperlane tile stands in. To undo: when paths are modelled, let a galaxy hold
            // copies of a tile and delete this.
            if placed.iter().any(|(used, _)| *used == id)
                && catalogue
                    .get(id.as_str())
                    .is_some_and(|system| system.is_hyperlane())
            {
                let spare = catalogue.iter().find(|(spare, system)| {
                    system.is_hyperlane() && placed.iter().all(|(used, _)| used != *spare)
                });
                if let Some((spare, _)) = spare {
                    id = (*spare).to_owned();
                }
            }
            placed.push((id, hex));
        } else if tile.home {
            let seat = tile.player_number.ok_or_else(|| bad(&tile.pos))?;
            let home = seat
                .checked_sub(1)
                .and_then(|i| homes.get(i))
                .ok_or_else(|| bad(&tile.pos))?;
            placed.push((home.as_str().to_owned(), hex));
        } else {
            open.push(hex);
        }
    }

    let used: BTreeSet<String> = placed.iter().map(|(id, _)| id.clone()).collect();
    let pool = seating::map_filler(content, usize::MAX, sources, seed);
    // HACK (2026-10-09): the filler pool is planet systems only and is too small for seven or
    // more players. When it is empty, the rest comes from the other numbered tiles of the base
    // game and Prophecy of Kings (anomalies, empty space, wormholes), in tile order. To undo: let
    // `seating::map_filler` draw red tiles in the printed proportion and delete `rest`.
    let rest: Vec<SystemId> = catalogue
        .keys()
        .filter(|id| {
            id.parse::<u32>()
                .is_ok_and(|tile| matches!(tile, 19..=50 | 59..=80))
                && !pool.iter().any(|drawn| drawn.as_str() == **id)
        })
        .map(|id| SystemId::new(*id))
        .collect();
    let mut fill = pool
        .iter()
        .chain(&rest)
        .filter(|id| !used.contains(id.as_str()));
    for hex in open {
        let id = fill.next().ok_or_else(|| TemplateError::OutOfFiller {
            alias: alias.clone(),
        })?;
        placed.push((id.as_str().to_owned(), hex));
    }

    let refs: Vec<(&str, Hex)> = placed.iter().map(|(id, hex)| (id.as_str(), *hex)).collect();
    let mut galaxy = Galaxy::placed(content, &refs, sources)?;
    seating::place_wormhole_nexus(&mut galaxy, content, sources)?;
    Ok(galaxy)
}

/// The home systems of the first `player_count` seats, in seat order (the in-scope factions,
/// cycling), which is what every game of that size seats.
#[must_use]
pub fn placeholder_homes(content: &ContentStore, player_count: usize) -> Vec<SystemId> {
    (0..player_count)
        .filter_map(|seat| {
            ti4_content::factions::get(content, seating::seat_faction(seat))
                .and_then(|f| f.home_system().map(SystemId::new))
        })
        .collect()
}

/// The template a game of `player_count` uses when the host names none: the first one for that
/// size that builds under `sources`, preferring plain `Standard` layouts, then `StaticEq`, then `Hyperlanes`.
#[must_use]
pub fn default_template_for(
    content: &ContentStore,
    loader: &TemplateLoader,
    player_count: usize,
    sources: SourceSet,
) -> Option<String> {
    let homes = placeholder_homes(content, player_count);
    let rank = |alias: &str| {
        if alias.ends_with("Standard") {
            0
        } else if alias.ends_with("StaticEq") {
            1
        } else if alias.ends_with("Hyperlanes") && !alias.contains("InPerson") {
            2
        } else {
            3
        }
    };
    let mut candidates: Vec<&MapTemplate> = loader.templates_for(player_count);
    candidates.sort_by_key(|t| rank(&t.alias));
    candidates
        .into_iter()
        .find(|t| build_template_galaxy(content, t, &homes, 0, sources).is_ok())
        .map(|t| t.alias.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn homes(content: &ContentStore, n: usize) -> Vec<SystemId> {
        (0..n)
            .map(|seat| {
                SystemId::new(
                    ti4_content::factions::get(content, seating::seat_faction(seat))
                        .and_then(|f| f.home_system())
                        .expect("home"),
                )
            })
            .collect()
    }

    const POK: SourceSet = ti4_model::content_types::POK;

    /// Templates the shipped data cannot build under POK: `0g` is not a tile in the corpus, and
    /// the others place one tile twice.
    const UNBUILDABLE: [&str; 5] = [
        "3pStaticEq",
        "4pStaticEq",
        "5pStaticEq",
        "6pStaticEq",
        "2025scptFinals",
    ];

    #[test]
    fn templates_build_with_every_tile_and_home_in_place() {
        let content = ContentStore::embedded();
        let loader = TemplateLoader::load().unwrap();
        let mut built = 0;
        for n in 3..=6 {
            let homes = homes(content, n);
            for t in loader.templates_for(n) {
                let galaxy = match build_template_galaxy(content, t, &homes, 7, POK) {
                    Ok(galaxy) => galaxy,
                    Err(e) => {
                        assert!(
                            UNBUILDABLE.contains(&t.alias.as_str())
                                || t.alias == "5pAlternateLayout",
                            "{} should build: {e}",
                            t.alias
                        );
                        continue;
                    }
                };
                built += 1;
                let mut seen = BTreeSet::new();
                for tile in &t.template_tiles {
                    let hex = hex_of(&tile.pos).unwrap();
                    let id = galaxy
                        .system_at(hex)
                        .unwrap_or_else(|| panic!("{} leaves {} empty", t.alias, tile.pos));
                    assert!(seen.insert(id.to_owned()), "{} repeats {id}", t.alias);
                    if tile.home {
                        let seat = tile.player_number.unwrap();
                        assert_eq!(id, homes[seat - 1].as_str(), "{} {}", t.alias, tile.pos);
                    }
                }
            }
        }
        assert!(built >= 10, "only {built} templates built");
    }

    #[test]
    fn six_player_standard_places_mecatol_and_spaces_homes_on_the_ring() {
        let content = ContentStore::embedded();
        let loader = TemplateLoader::load().unwrap();
        let t = loader.get("6pStandard").unwrap();
        let homes = homes(content, 6);
        let galaxy = build_template_galaxy(content, t, &homes, 3, POK).unwrap();
        assert_eq!(galaxy.coord_of("18"), Some(Hex::ORIGIN));
        for home in &homes {
            let at = galaxy.coord_of(home.as_str()).unwrap();
            assert_eq!(
                Hex::ORIGIN.distance(at),
                3,
                "{home} is not on the outer ring"
            );
        }
    }

    #[test]
    fn the_seed_only_changes_the_open_slots() {
        let content = ContentStore::embedded();
        let loader = TemplateLoader::load().unwrap();
        let t = loader.get("6pStandard").unwrap();
        let homes = homes(content, 6);
        let a = build_template_galaxy(content, t, &homes, 1, POK).unwrap();
        let b = build_template_galaxy(content, t, &homes, 1, POK).unwrap();
        let c = build_template_galaxy(content, t, &homes, 2, POK).unwrap();
        let ids = |g: &Galaxy| -> Vec<String> {
            Hex::spiral(3)
                .into_iter()
                .filter_map(|h| g.system_at(h).map(str::to_owned))
                .collect()
        };
        assert_eq!(ids(&a), ids(&b));
        assert_ne!(ids(&a), ids(&c));
    }

    #[test]
    fn a_player_count_mismatch_is_rejected() {
        let content = ContentStore::embedded();
        let loader = TemplateLoader::load().unwrap();
        let t = loader.get("6pStandard").unwrap();
        let err = build_template_galaxy(content, t, &homes(content, 4), 1, POK).unwrap_err();
        assert!(matches!(
            err,
            TemplateError::PlayerCount {
                expected: 6,
                given: 4,
                ..
            }
        ));
    }

    #[test]
    fn default_selection_builds_for_every_supported_size() {
        let content = ContentStore::embedded();
        let loader = TemplateLoader::load().unwrap();
        for n in 3..=6 {
            let alias = default_template_for(content, &loader, n, POK)
                .unwrap_or_else(|| panic!("no default for {n} players"));
            let t = loader.get(&alias).unwrap();
            assert_eq!(t.player_count, n);
            assert!(build_template_galaxy(content, t, &homes(content, n), 5, POK).is_ok());
        }
        assert_eq!(
            default_template_for(content, &loader, 6, POK).as_deref(),
            Some("6pStandard")
        );
        assert_eq!(default_template_for(content, &loader, 11, POK), None);
    }
}
