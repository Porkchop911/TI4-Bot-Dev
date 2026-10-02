//! Map generation and static board tile extraction.

use serde::{Deserialize, Serialize};
use ti4_content::ContentStore;
use ti4_content::galaxy::{self, Galaxy};
use ti4_engine::seating::{self, SeatingError};
use ti4_engine::setup::start_game_seeded;
use ti4_model::content_types::{FULL, POK, SourceSet};
use ti4_model::id::{PlayerId, SystemId};
use ti4_model::state::GameState;

use crate::protocol::view::{BoardTileView, PlanetMetaView};

/// Stable, reconstructible galaxy geometry for stateless consumers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GalaxyLayout {
    pub version: u16,
    pub active_sources: Vec<String>,
    pub placements: Vec<GalaxyPlacement>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub off_map_system_ids: Vec<String>,
}

/// One main-map system placement in axial coordinates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GalaxyPlacement {
    pub system_id: String,
    pub q: i32,
    pub r: i32,
}

impl GalaxyLayout {
    #[must_use]
    pub fn from_galaxy(galaxy: &Galaxy, active_sources: SourceSet) -> Self {
        let mut placements: Vec<_> = galaxy
            .system_ids()
            .into_iter()
            .filter_map(|system_id| {
                galaxy.coord_of(system_id).map(|hex| GalaxyPlacement {
                    system_id: system_id.to_owned(),
                    q: hex.q,
                    r: hex.r,
                })
            })
            .collect();
        placements.sort_by_key(|placement| (placement.q, placement.r, placement.system_id.clone()));
        Self {
            version: 1,
            active_sources: active_source_names(active_sources),
            placements,
            off_map_system_ids: galaxy
                .off_map_system_ids()
                .into_iter()
                .map(str::to_owned)
                .collect(),
        }
    }
}

fn active_source_names(sources: SourceSet) -> Vec<String> {
    [
        (ti4_model::content_types::Source::Base, "base"),
        (ti4_model::content_types::Source::Pok, "pok"),
        (ti4_model::content_types::Source::Codex1, "codex1"),
        (ti4_model::content_types::Source::Codex2, "codex2"),
        (ti4_model::content_types::Source::Codex3, "codex3"),
        (ti4_model::content_types::Source::Codex4, "codex4"),
        (
            ti4_model::content_types::Source::ThundersEdge,
            "thunders_edge",
        ),
    ]
    .into_iter()
    .filter(|(source, _)| sources.contains(*source))
    .map(|(_, name)| name.to_owned())
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ti4_model::hex::Hex;

    #[test]
    fn galaxy_layout_round_trip_preserves_static_topology() {
        let content = ContentStore::embedded();
        let mut galaxy = Galaxy::placed(
            content,
            &[
                ("18", Hex::new(0, 0)),
                ("39", Hex::new(2, 0)),
                ("26", Hex::new(-2, 0)),
            ],
            POK,
        )
        .expect("source systems");
        galaxy
            .place_off_map(content, "82b", POK)
            .expect("nexus system");

        let layout = GalaxyLayout::from_galaxy(&galaxy, POK);
        let placements: Vec<_> = layout
            .placements
            .iter()
            .map(|placement| {
                (
                    placement.system_id.as_str(),
                    Hex::new(placement.q, placement.r),
                )
            })
            .collect();
        let mut rebuilt = Galaxy::placed(content, &placements, POK).expect("layout placements");
        for system_id in &layout.off_map_system_ids {
            rebuilt
                .place_off_map(content, system_id, POK)
                .expect("layout off-map system");
        }

        assert_eq!(layout.version, 1);
        assert_eq!(layout.active_sources, active_source_names(POK));
        assert_eq!(rebuilt.coord_of("39"), galaxy.coord_of("39"));
        assert_eq!(rebuilt.adjacent("39"), galaxy.adjacent("39"));
        assert!(rebuilt.are_adjacent("82b", "39"));
    }
}

/// Extracts static geometry and metadata for all tiles in a galaxy.
#[must_use]
pub fn build_board_tiles(content: &ContentStore, galaxy: &Galaxy) -> Vec<BoardTileView> {
    let mut board: Vec<BoardTileView> = galaxy
        .system_ids()
        .into_iter()
        .filter_map(|id| {
            let coord = galaxy.coord_of(id)?;
            let special_area = matches!(id, "82a" | "82b").then(|| "nexus".to_owned());
            system_tile_metadata(content, id, coord.q, coord.r, special_area)
        })
        .collect();

    board.sort_by_key(|tile| (tile.special_area.is_some(), tile.q, tile.r));

    for id in ["82a", "82b"] {
        if !board.iter().any(|tile| tile.system_id == id)
            && let Some(tile) = system_tile_metadata(content, id, 0, 0, Some("nexus".to_owned()))
        {
            board.push(tile);
        }
    }

    board.extend(
        ti4_engine::fracture::systems(content, FULL)
            .into_iter()
            .enumerate()
            .filter_map(|(index, id)| {
                system_tile_metadata(
                    content,
                    id.as_str(),
                    i32::try_from(index).unwrap_or(0),
                    0,
                    Some("fracture".to_owned()),
                )
            }),
    );

    board
}

fn system_tile_metadata(
    content: &ContentStore,
    id: &str,
    q: i32,
    r: i32,
    special_area: Option<String>,
) -> Option<BoardTileView> {
    let system = galaxy::system(content, id, FULL)?;
    let planets = system
        .planets()
        .into_iter()
        .filter_map(|planet_id| galaxy::planet(content, planet_id, FULL))
        .map(|planet| PlanetMetaView {
            id: planet.id().to_owned(),
            label: planet.name().unwrap_or(planet.id()).to_owned(),
            resources: i32::try_from(planet.resources()).unwrap_or(0),
            influence: i32::try_from(planet.influence()).unwrap_or(0),
            traits: planet.traits().into_iter().map(str::to_owned).collect(),
            tech_specialties: planet
                .tech_specialties()
                .into_iter()
                .map(str::to_owned)
                .collect(),
            legendary: planet.is_legendary(),
            space_station: planet.is_space_station(),
        })
        .collect();

    let anomalies = [
        (system.is_nebula(), "nebula"),
        (system.is_supernova(), "supernova"),
        (system.is_asteroid_field(), "asteroid field"),
        (system.is_gravity_rift(), "gravity rift"),
        (system.is_scar(), "entropic scar"),
    ]
    .into_iter()
    .filter(|(present, _)| *present)
    .map(|(_, kind)| kind.to_owned())
    .collect();

    let egress = special_area.as_deref() == Some("fracture")
        && system
            .name()
            .is_some_and(|name| name.to_ascii_lowercase().contains("egress"));

    Some(BoardTileView {
        system_id: id.to_owned(),
        label: system.name().unwrap_or(id).to_owned(),
        q,
        r,
        hyperlane: system.is_hyperlane(),
        special_area,
        anomalies,
        wormholes: system.wormholes().into_iter().map(str::to_owned).collect(),
        egress,
        planets,
    })
}

/// Initializes a fresh game with seated factions, deployed starting fleets, and a constructed galaxy.
///
/// # Errors
///
/// Returns [`SeatingError`] if seating, deployment, or galaxy board construction fails.
pub fn create_game_with_map(
    content: &ContentStore,
    player_ids: &[PlayerId],
    seed: u64,
) -> Result<(GameState, Galaxy), SeatingError> {
    let mut state = start_game_seeded(content, player_ids, POK, None, seed)
        .map_err(|e| SeatingError::UnknownPlayer(e.to_string()))?;

    let assignments = seating::seat_in_scope(player_ids);
    for (player, faction) in &assignments {
        seating::deploy(&mut state, content, player, faction, POK)?;
    }

    let filler = seating::map_filler(content, 30, POK, seed);
    let filler_refs: Vec<&str> = filler.iter().map(SystemId::as_str).collect();
    let galaxy = seating::build_board(content, &assignments, &filler_refs, POK)?;

    Ok((state, galaxy))
}
