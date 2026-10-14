//! Checks for the community layouts in `community_maps.json`: each one is a complete, fixed map
//! (no seeded filler), names only tiles the content knows, places every tile once, seats exactly
//! its player count, hangs together as one board, and starts a game headlessly.

use std::collections::{BTreeSet, VecDeque};

use ti4_content::ContentStore;
use ti4_model::Hex;
use ti4_model::content_types::POK;
use ti4_model::id::PlayerId;

use super::build::{build_template_galaxy, hex_of, placeholder_homes};
use super::catalog;
use super::template_loader::{MapTemplate, TemplateLoader};

fn community() -> Vec<MapTemplate> {
    serde_json::from_str(include_str!(
        "../../../ti4-view/src/maps/community_maps.json"
    ))
    .expect("community_maps.json parses")
}

#[test]
fn every_community_map_is_described_and_attributed() {
    let maps = community();
    assert!(maps.len() >= 25, "only {} community maps", maps.len());
    let mut aliases = BTreeSet::new();
    for m in &maps {
        assert!(aliases.insert(m.alias.clone()), "{} repeats", m.alias);
        assert!(
            m.alias.starts_with(&format!("{}p", m.player_count)),
            "{} does not start with its player count",
            m.alias
        );
        assert!(
            (3..=6).contains(&m.player_count),
            "{} seats {}",
            m.alias,
            m.player_count
        );
        assert!(
            m.name.as_deref().is_some_and(|n| !n.is_empty()),
            "{} has no name",
            m.alias
        );
        assert!(!m.author.is_empty(), "{} has no author", m.alias);
        assert!(
            m.source
                .as_deref()
                .is_some_and(|s| s.starts_with("https://")),
            "{} has no source URL",
            m.alias
        );
        assert!(m.source_entry.is_some(), "{} has no source entry", m.alias);
    }
    let loader = TemplateLoader::load().expect("templates load");
    for m in &maps {
        assert!(loader.exists(&m.alias), "{} is not in the loader", m.alias);
    }
}

#[test]
fn every_community_map_is_fixed_with_one_home_per_seat() {
    for m in community() {
        let mut positions = BTreeSet::new();
        let mut seats = Vec::new();
        for tile in &m.template_tiles {
            assert!(
                hex_of(&tile.pos).is_some(),
                "{} has bad position {}",
                m.alias,
                tile.pos
            );
            assert!(
                positions.insert(tile.pos.clone()),
                "{} repeats {}",
                m.alias,
                tile.pos
            );
            if tile.home {
                assert!(tile.static_tile_id.is_none(), "{} {}", m.alias, tile.pos);
                seats.push(tile.player_number.expect("home seat"));
            } else {
                assert!(
                    tile.static_tile_id.is_some(),
                    "{} leaves {} open: community maps are fixed layouts",
                    m.alias,
                    tile.pos
                );
            }
        }
        seats.sort_unstable();
        assert_eq!(
            seats,
            (1..=m.player_count).collect::<Vec<_>>(),
            "{} home slots",
            m.alias
        );
    }
}

#[test]
fn every_community_map_builds_the_same_board_for_any_seed_and_hangs_together() {
    let content = ContentStore::embedded();
    for m in community() {
        let homes = placeholder_homes(content, m.player_count);
        let a = build_template_galaxy(content, &m, &homes, 1, POK)
            .unwrap_or_else(|e| panic!("{} does not build: {e}", m.alias));
        let b = build_template_galaxy(content, &m, &homes, 99, POK).unwrap();
        let mut seen = BTreeSet::new();
        for tile in &m.template_tiles {
            let hex = hex_of(&tile.pos).unwrap();
            let id = a
                .system_at(hex)
                .unwrap_or_else(|| panic!("{} leaves {} empty", m.alias, tile.pos));
            assert_eq!(
                b.system_at(hex),
                Some(id),
                "{} depends on the seed",
                m.alias
            );
            assert!(seen.insert(id.to_owned()), "{} repeats {id}", m.alias);
            if tile.home {
                assert_eq!(
                    id,
                    homes[tile.player_number.unwrap() - 1].as_str(),
                    "{}",
                    m.alias
                );
            }
        }
        assert!(a.coord_of("18").is_some(), "{} has no Mecatol Rex", m.alias);

        // Adjacency: one connected board, and every home touches at least two tiles.
        let on_map: BTreeSet<&str> = m
            .template_tiles
            .iter()
            .filter_map(|t| a.system_at(hex_of(&t.pos).unwrap()))
            .collect();
        let mut reached = BTreeSet::from(["18"]);
        let mut queue = VecDeque::from(["18"]);
        while let Some(at) = queue.pop_front() {
            for next in a.adjacent(at) {
                if on_map.contains(next) && reached.insert(next) {
                    queue.push_back(next);
                }
            }
        }
        assert_eq!(reached, on_map, "{} is not one connected board", m.alias);
        for home in &homes {
            let hex = a.coord_of(home.as_str()).unwrap();
            let touching = hex
                .neighbours()
                .into_iter()
                .filter(|n| a.system_at(*n).is_some())
                .count();
            assert!(
                touching >= 2,
                "{}: {home} at {hex:?} touches {touching}",
                m.alias
            );
            assert_ne!(hex, Hex::ORIGIN, "{} seats a home in the centre", m.alias);
        }
    }
}

#[test]
fn every_community_map_starts_a_game_and_is_offered_by_name() {
    let content = ContentStore::embedded();
    for m in community() {
        let players: Vec<PlayerId> = (1..=m.player_count)
            .map(|i| PlayerId::new(format!("p{i}")))
            .collect();
        let (state, galaxy) =
            crate::map::create_game_with_template(content, &players, 7, Some(&m.alias))
                .unwrap_or_else(|e| panic!("{} does not start: {e}", m.alias));
        assert_eq!(state.players.len(), m.player_count, "{}", m.alias);
        assert!(galaxy.coord_of("18").is_some(), "{}", m.alias);

        let offered = catalog::catalog(Some(m.player_count)).unwrap();
        let entry = offered
            .iter()
            .find(|s| s.alias == m.alias)
            .unwrap_or_else(|| panic!("{} is not offered", m.alias));
        assert!(entry.buildable, "{}", m.alias);
        assert_eq!(entry.name, m.name, "{}", m.alias);
        assert!(!entry.recommended, "{} displaced the default", m.alias);
        assert_eq!(
            catalog::describe(
                &catalog::MapChoice::Template {
                    alias: m.alias.clone()
                },
                m.player_count
            )
            .name,
            m.name
        );
    }
}
