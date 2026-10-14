//! Map generation and static board tile extraction.

use serde::{Deserialize, Serialize};
use ti4_content::ContentStore;
use ti4_content::galaxy::{self, Galaxy};
use ti4_engine::seating::{self, SeatingError};
use ti4_engine::setup::start_game_seeded_with_card_set;
use ti4_model::content_types::{FULL, POK, SourceSet};
use ti4_model::id::{PlayerId, SystemId};
use ti4_model::state::GameState;

use crate::view::{BoardTileView, PlanetMetaView};

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
    create_game_with_map_rotated(content, player_ids, seed, 0)
}

/// [`create_game_with_map`] with the factions rotated (see `seating::seat_in_scope_rotated`).
///
/// # Errors
///
/// As [`create_game_with_map`].
pub fn create_game_with_map_rotated(
    content: &ContentStore,
    player_ids: &[PlayerId],
    seed: u64,
    rotation: usize,
) -> Result<(GameState, Galaxy), SeatingError> {
    create_game_with_map_rotated_and_card_set(content, player_ids, seed, rotation, None)
}

/// As [`create_game_with_map`], with the strategy-card set named (`None`: the PoK-scope set).
///
/// # Errors
///
/// Returns [`SeatingError`] if setup (including an unknown card set), seating, deployment, or
/// galaxy board construction fails.
pub fn create_game_with_map_and_card_set(
    content: &ContentStore,
    player_ids: &[PlayerId],
    seed: u64,
    card_set: Option<&str>,
) -> Result<(GameState, Galaxy), SeatingError> {
    create_game_with_map_rotated_and_card_set(content, player_ids, seed, 0, card_set)
}

/// [`create_game_with_map`] with the factions rotated and the strategy-card set named.
///
/// # Errors
///
/// As [`create_game_with_map_and_card_set`].
pub fn create_game_with_map_rotated_and_card_set(
    content: &ContentStore,
    player_ids: &[PlayerId],
    seed: u64,
    rotation: usize,
    card_set: Option<&str>,
) -> Result<(GameState, Galaxy), SeatingError> {
    let assignments = seating::seat_in_scope_rotated(player_ids, rotation);
    create_game_with_map_seated(content, player_ids, seed, &assignments, card_set)
}

/// Deals every seat its faction's promissory notes (69.1).
///
/// Setup deals while the seats still hold the placeholder faction `generic`, which leaves four
/// `<alias>:generic` notes in the last seat's hand and no faction note anywhere. Dealing again once
/// the factions are seated gives each seat its real hand. The opening state is stored with the
/// game, so games created before this call keep the hand they were created with.
fn deal_notes_after_seating(state: &mut GameState, content: &ContentStore) {
    ti4_engine::promissory::deal(state, content, POK);
}

/// As [`create_game_with_map_rotated_and_card_set`], with the faction of every seat given (a
/// start preset may seat a faction outside `IN_SCOPE_FACTIONS`).
///
/// # Errors
///
/// As [`create_game_with_map`].
pub fn create_game_with_map_seated(
    content: &ContentStore,
    player_ids: &[PlayerId],
    seed: u64,
    assignments: &std::collections::BTreeMap<PlayerId, ti4_model::id::FactionId>,
    card_set: Option<&str>,
) -> Result<(GameState, Galaxy), SeatingError> {
    let mut state = start_game_seeded_with_card_set(content, player_ids, POK, None, seed, card_set)
        .map_err(|e| SeatingError::UnknownPlayer(e.to_string()))?;

    for (player, faction) in assignments {
        seating::deploy(&mut state, content, player, faction, POK)?;
    }
    deal_notes_after_seating(&mut state, content);

    let filler = seating::map_filler(content, 36, POK, seed);
    let filler_refs: Vec<&str> = filler.iter().map(SystemId::as_str).collect();
    // `build_board` places homes in key order. Real player ids sort arbitrarily, so key the
    // table by seat number instead: seat order, not id order, decides who sits where, which is
    // what a lobby's map preview (made before anyone has an id) can show.
    let by_seat: std::collections::BTreeMap<PlayerId, _> = player_ids
        .iter()
        .enumerate()
        .map(|(seat, player)| {
            (
                PlayerId::new(format!("seat_{seat:02}")),
                assignments[player].clone(),
            )
        })
        .collect();
    let galaxy = seating::build_board(content, &by_seat, &filler_refs, POK)?;
    // Setup rule: objectives this board can never satisfy are dealt out of the decks.
    ti4_engine::map_feasibility::apply(&mut state, content, POK, &galaxy);

    Ok((state, galaxy))
}

/// Like [`create_game_with_map`], but lays the board out from the predefined map template
/// `template` when one is named; `None` keeps the seeded random-filler board.
///
/// # Errors
///
/// A message for an unknown template, a seat-count mismatch, or any seating/galaxy failure.
pub fn create_game_with_template(
    content: &ContentStore,
    player_ids: &[PlayerId],
    seed: u64,
    template: Option<&str>,
) -> Result<(GameState, Galaxy), String> {
    create_game_with_template_rotated(content, player_ids, seed, template, 0)
}

/// [`create_game_with_template`] with the factions rotated (see `seating::seat_in_scope_rotated`).
///
/// # Errors
///
/// As [`create_game_with_template`].
pub fn create_game_with_template_rotated(
    content: &ContentStore,
    player_ids: &[PlayerId],
    seed: u64,
    template: Option<&str>,
    rotation: usize,
) -> Result<(GameState, Galaxy), String> {
    create_game_with_template_rotated_and_card_set(
        content, player_ids, seed, template, rotation, None,
    )
}

/// As [`create_game_with_template`], with the strategy-card set named (`None`: the PoK-scope set).
///
/// # Errors
///
/// A message for an unknown template or card set, a seat-count mismatch, or any seating/galaxy
/// failure.
pub fn create_game_with_template_and_card_set(
    content: &ContentStore,
    player_ids: &[PlayerId],
    seed: u64,
    template: Option<&str>,
    card_set: Option<&str>,
) -> Result<(GameState, Galaxy), String> {
    create_game_with_template_rotated_and_card_set(content, player_ids, seed, template, 0, card_set)
}

/// [`create_game_with_template`] with the factions rotated and the strategy-card set named.
///
/// # Errors
///
/// As [`create_game_with_template_and_card_set`].
pub fn create_game_with_template_rotated_and_card_set(
    content: &ContentStore,
    player_ids: &[PlayerId],
    seed: u64,
    template: Option<&str>,
    rotation: usize,
    card_set: Option<&str>,
) -> Result<(GameState, Galaxy), String> {
    let assignments = seating::seat_in_scope_rotated(player_ids, rotation);
    create_game_with_template_seated(content, player_ids, seed, template, &assignments, card_set)
}

/// As [`create_game_with_template_rotated_and_card_set`], with the faction of every seat given (a
/// start preset may seat a faction outside `IN_SCOPE_FACTIONS`).
///
/// # Errors
///
/// As [`create_game_with_template`].
pub fn create_game_with_template_seated(
    content: &ContentStore,
    player_ids: &[PlayerId],
    seed: u64,
    template: Option<&str>,
    assignments: &std::collections::BTreeMap<PlayerId, ti4_model::id::FactionId>,
    card_set: Option<&str>,
) -> Result<(GameState, Galaxy), String> {
    let Some(alias) = template else {
        return create_game_with_map_seated(content, player_ids, seed, assignments, card_set)
            .map_err(|e| e.to_string());
    };
    let loader = crate::maps::TemplateLoader::load()?;
    let template = loader
        .get(alias)
        .ok_or_else(|| crate::maps::TemplateError::Unknown(alias.to_owned()).to_string())?;

    let mut state = start_game_seeded_with_card_set(content, player_ids, POK, None, seed, card_set)
        .map_err(|e| e.to_string())?;
    for (player, faction) in assignments {
        seating::deploy(&mut state, content, player, faction, POK).map_err(|e| e.to_string())?;
    }
    deal_notes_after_seating(&mut state, content);
    let homes: Vec<SystemId> = player_ids
        .iter()
        .map(|player| {
            let one =
                std::collections::BTreeMap::from([(player.clone(), assignments[player].clone())]);
            seating::home_systems(content, &one).map(|mut homes| homes.remove(0))
        })
        .collect::<Result<_, _>>()
        .map_err(|e| e.to_string())?;
    let galaxy = crate::maps::build_template_galaxy(content, template, &homes, seed, POK)
        .map_err(|e| e.to_string())?;
    // Setup rule: objectives this board can never satisfy are dealt out of the decks.
    ti4_engine::map_feasibility::apply(&mut state, content, POK, &galaxy);
    Ok((state, galaxy))
}

/// Who opens a new game as speaker (and so picks the first strategy card).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpeakerOrder {
    /// A seat drawn from the game seed ([`ti4_engine::seeded_speaker`]): the same seed always
    /// opens with the same speaker. What a lobby game gets unless it asks otherwise.
    #[default]
    Random,
    /// The first seat. Fixtures and tests pin their games to this.
    Seat,
}

impl SpeakerOrder {
    /// The stored/requested name: `"random"` or `"seat"`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Random => "random",
            Self::Seat => "seat",
        }
    }

    /// Parse a requested name.
    ///
    /// # Errors
    /// A message naming the accepted values.
    pub fn parse(name: &str) -> Result<Self, String> {
        match name {
            "random" => Ok(Self::Random),
            "seat" => Ok(Self::Seat),
            other => Err(format!(
                "unknown speaker_order {other:?} (expected \"random\" or \"seat\")"
            )),
        }
    }

    /// A stored value; `None` (a record written before the option) and anything unknown are
    /// [`SpeakerOrder::Random`].
    #[must_use]
    pub fn from_stored(name: Option<&str>) -> Self {
        name.and_then(|n| Self::parse(n).ok()).unwrap_or_default()
    }
}

/// The factions a lobby table plays: `chosen[i]` is what seat `i` asked for, and the undecided seats
/// get distinct factions from the rest of the pool, dealt by `seed`
/// ([`seating::lobby_faction_draw`]). Indexed by seat, so a map preview made before anyone has an
/// id seats the same factions the game will.
///
/// # Errors
/// A message for an unknown or repeated choice.
pub fn lobby_seat_factions(
    chosen: &[Option<String>],
    seed: u64,
) -> Result<Vec<ti4_model::id::FactionId>, String> {
    let chosen: Vec<Option<&str>> = chosen.iter().map(Option::as_deref).collect();
    seating::lobby_faction_draw(&chosen, seed).map_err(|e| e.to_string())
}

#[cfg(test)]
mod template_tests {
    use super::*;

    fn players(n: usize) -> Vec<PlayerId> {
        (1..=n).map(|i| PlayerId::new(format!("p{i}"))).collect()
    }

    #[test]
    fn speaker_order_names_round_trip() {
        for order in [SpeakerOrder::Random, SpeakerOrder::Seat] {
            assert_eq!(SpeakerOrder::parse(order.as_str()), Ok(order));
        }
        assert!(SpeakerOrder::parse("first").is_err());
        assert_eq!(SpeakerOrder::from_stored(None), SpeakerOrder::Random);
    }

    #[test]
    fn an_unknown_template_is_an_error() {
        let err = create_game_with_template(ContentStore::embedded(), &players(6), 1, Some("nope"))
            .unwrap_err();
        assert!(err.contains("unknown map template"), "{err}");
    }

    #[test]
    fn no_template_keeps_the_random_board() {
        let content = ContentStore::embedded();
        let (_, a) = create_game_with_template(content, &players(6), 9, None).unwrap();
        let (_, b) = create_game_with_map(content, &players(6), 9).unwrap();
        assert_eq!(
            build_board_tiles(content, &a),
            build_board_tiles(content, &b)
        );
    }

    #[test]
    fn a_template_game_seats_every_player_on_their_home() {
        let content = ContentStore::embedded();
        let (state, galaxy) =
            create_game_with_template(content, &players(6), 4, Some("6pStandard")).unwrap();
        assert_eq!(state.players.len(), 6);
        assert!(galaxy.coord_of("18").is_some());
        let (_, random) = create_game_with_map(content, &players(6), 4).unwrap();
        assert_eq!(
            build_board_tiles(content, &galaxy).len(),
            build_board_tiles(content, &random).len()
        );
    }

    #[test]
    fn seven_and_eight_players_get_a_board_with_a_home_for_every_seat() {
        let content = ContentStore::embedded();
        let loader = crate::maps::TemplateLoader::load().unwrap();
        for n in [7, 8] {
            let alias = crate::maps::default_template_for(content, &loader, n, POK)
                .unwrap_or_else(|| panic!("no template builds for {n} players"));
            let (state, galaxy) =
                create_game_with_template(content, &players(n), 4, Some(&alias)).unwrap();
            let factions: std::collections::BTreeSet<_> = state
                .players
                .iter()
                .map(|player| player.faction.clone())
                .collect();
            assert_eq!(factions.len(), n, "{n} seats, {n} factions");
            for seat in 0..n {
                let home = ti4_content::factions::get(content, seating::seat_faction(seat))
                    .and_then(|faction| faction.home_system())
                    .expect("home");
                assert!(
                    galaxy.coord_of(home).is_some(),
                    "{alias}: home {home} is on the board"
                );
            }
        }
    }

    /// Setup rule: no generated game deals, reveals or holds an objective its own map cannot
    /// satisfy (generated boards have no empty systems, wormholes or anomalies).
    #[test]
    fn a_new_game_never_holds_an_objective_its_map_rules_out() {
        let content = ContentStore::embedded();
        let loader = crate::maps::TemplateLoader::load().unwrap();
        for n in 3..=6usize {
            let list: Vec<PlayerId> = (0..n).map(|i| PlayerId::new(format!("p{i}"))).collect();
            let default = crate::maps::default_template_for(content, &loader, n, POK);
            for template in [None, default.as_deref()] {
                for seed in 0..6 {
                    let (state, galaxy) =
                        create_game_with_template(content, &list, seed, template).unwrap();
                    let homes: Vec<SystemId> = state
                        .players
                        .iter()
                        .filter_map(|p| p.home_system.clone())
                        .collect();
                    let census = ti4_engine::map_feasibility::census(content, POK, &galaxy, &homes);
                    let public = ti4_engine::map_feasibility::unsatisfiable_public(&census);
                    let secret = ti4_engine::map_feasibility::unsatisfiable_secret(&census);
                    assert!(public.contains(&"deep_space") && secret.contains(&"btgk"));
                    for id in state
                        .revealed_objectives
                        .iter()
                        .chain(&state.objective_deck)
                    {
                        assert!(
                            !public.contains(&id.as_str()),
                            "{template:?} {n}p {seed}: {id}"
                        );
                    }
                    for id in state
                        .secret_deck
                        .iter()
                        .chain(state.players.iter().flat_map(|p| &p.secret_objectives))
                    {
                        assert!(
                            !secret.contains(&id.as_str()),
                            "{template:?} {n}p {seed}: {id}"
                        );
                    }
                    assert_eq!(state.revealed_objectives.len(), 2);
                    assert!(state.players.iter().all(|p| p.secret_objectives.len() == 1));
                }
            }
        }
    }
}
