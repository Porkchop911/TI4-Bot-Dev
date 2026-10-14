//! Game creation with a start preset or a lobby's choices; templates and the board geometry are in
//! `ti4-view`.

use ti4_content::ContentStore;
use ti4_content::galaxy::Galaxy;
use ti4_engine::seating;
use ti4_model::id::PlayerId;
use ti4_model::state::GameState;

pub use ti4_view::map::{
    GalaxyLayout, GalaxyPlacement, SpeakerOrder, build_board_tiles, create_game_with_map,
    create_game_with_map_and_card_set, create_game_with_map_rotated,
    create_game_with_map_rotated_and_card_set, create_game_with_map_seated,
    create_game_with_template, create_game_with_template_and_card_set,
    create_game_with_template_rotated, create_game_with_template_rotated_and_card_set,
    create_game_with_template_seated, lobby_seat_factions,
};

/// As [`create_game_with_template`], then applies the named start preset (if any) to the opening
/// state. Without a preset the result is exactly [`create_game_with_template`]'s.
///
/// # Errors
/// Anything [`create_game_with_template`] reports, or an unknown or unplaceable preset.
pub fn create_game_with_preset(
    content: &ContentStore,
    player_ids: &[PlayerId],
    seed: u64,
    template: Option<&str>,
    preset: Option<&str>,
) -> Result<(GameState, Galaxy), String> {
    create_game_with_options(content, player_ids, seed, template, preset, None)
}

/// As [`create_game_with_preset`], with the strategy-card set named (`None`: the PoK-scope set).
///
/// # Errors
/// Anything [`create_game_with_preset`] reports, or an unknown card set.
pub fn create_game_with_options(
    content: &ContentStore,
    player_ids: &[PlayerId],
    seed: u64,
    template: Option<&str>,
    preset: Option<&str>,
    card_set: Option<&str>,
) -> Result<(GameState, Galaxy), String> {
    create_game_with_speaker(
        content,
        player_ids,
        seed,
        template,
        preset,
        card_set,
        SpeakerOrder::Seat,
    )
}

/// As [`create_game_with_options`], with the opening speaker chosen by `speaker`. The speaker is
/// part of the returned opening state, so a game replayed from its stored opening state keeps it.
///
/// # Errors
/// As [`create_game_with_options`].
pub fn create_game_with_speaker(
    content: &ContentStore,
    player_ids: &[PlayerId],
    seed: u64,
    template: Option<&str>,
    preset: Option<&str>,
    card_set: Option<&str>,
    speaker: SpeakerOrder,
) -> Result<(GameState, Galaxy), String> {
    create_game_with_factions(
        content, player_ids, seed, template, preset, card_set, speaker, None,
    )
}

/// As [`create_game_with_speaker`], seating a lobby's table: with `lobby_factions` (one entry per
/// seat, `None` for "any") and no start preset the factions come from [`lobby_seat_factions`].
/// A start preset seats its own factions, so the lobby's choices do not apply to it. Without
/// `lobby_factions` the fixed in-scope order seats (what fixtures and tests pin their games to).
///
/// # Errors
/// As [`create_game_with_speaker`], or an unknown or repeated faction choice.
#[allow(clippy::too_many_arguments)]
pub fn create_game_with_factions(
    content: &ContentStore,
    player_ids: &[PlayerId],
    seed: u64,
    template: Option<&str>,
    preset: Option<&str>,
    card_set: Option<&str>,
    speaker: SpeakerOrder,
    lobby_factions: Option<&[Option<String>]>,
) -> Result<(GameState, Galaxy), String> {
    let rotation = preset.map_or(0, |name| crate::preset::rotation(name, seed));
    let lobby_roster = match (preset, lobby_factions) {
        (None, Some(chosen)) => {
            if chosen.len() != player_ids.len() {
                return Err("one faction entry per seat is required".to_owned());
            }
            Some(
                player_ids
                    .iter()
                    .cloned()
                    .zip(lobby_seat_factions(chosen, seed)?)
                    .collect::<std::collections::BTreeMap<_, _>>(),
            )
        }
        _ => None,
    };
    // A preset that seats factions outside the in-scope six names them itself (`+fac`, `world`).
    let mut assignments = lobby_roster
        .or_else(|| preset.and_then(|name| crate::preset::roster(name, seed, player_ids)))
        .unwrap_or_else(|| seating::seat_in_scope_rotated(player_ids, rotation));
    if let Some(name) = preset {
        crate::preset::seat_roster(name, seed, player_ids, &mut assignments);
    }
    let (mut state, galaxy) = create_game_with_template_seated(
        content,
        player_ids,
        seed,
        template,
        &assignments,
        card_set,
    )?;
    if speaker == SpeakerOrder::Random
        && let Some(drawn) = ti4_engine::seeded_speaker(player_ids, seed)
    {
        state.speaker = drawn;
    }
    if let Some(preset) = preset {
        crate::preset::apply(content, &mut state, &galaxy, player_ids, seed, preset)?;
    }
    Ok((state, galaxy))
}

/// The galaxy a game was built with, rebuilt for recovery from the factions its saved opening state
/// seats. Rebuilding with the in-scope order instead (as recovery did) lays the board out for the
/// wrong home systems whenever a start preset rotated the factions or seated others (`+rot`,
/// `+fac`, `leaders`, `world`), and the replay then diverges at the first move.
///
/// # Errors
///
/// As [`create_game_with_template_seated`].
pub fn rebuild_galaxy(
    content: &ContentStore,
    player_ids: &[PlayerId],
    seed: u64,
    template: Option<&str>,
    opening: &GameState,
) -> Result<Galaxy, String> {
    let seated: std::collections::BTreeMap<PlayerId, ti4_model::id::FactionId> = player_ids
        .iter()
        .filter_map(|player| {
            opening
                .player(player)
                .map(|seat| (player.clone(), seat.faction.clone()))
        })
        .collect();
    // A state whose seats have no faction yet (the "generic" placeholder of a bare test session)
    // keeps the old behaviour.
    let real = seated.len() == player_ids.len()
        && seated
            .values()
            .all(|faction| ti4_content::factions::get(content, faction.as_str()).is_some());
    let assignments = if real {
        seated
    } else {
        seating::seat_in_scope(player_ids)
    };
    create_game_with_template_seated(content, player_ids, seed, template, &assignments, None)
        .map(|(_, galaxy)| galaxy)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn players(n: usize) -> Vec<PlayerId> {
        (1..=n).map(|i| PlayerId::new(format!("p{i}"))).collect()
    }

    #[test]
    fn a_rebuilt_galaxy_follows_the_seated_factions_not_the_in_scope_order() {
        let content = ContentStore::embedded();
        let list = players(4);
        for preset in [
            "world",
            "combat+rot",
            "leaders",
            "agenda+fac:naalu:mentak",
            "combat",
        ] {
            let (state, galaxy) =
                create_game_with_preset(content, &list, 7, Some("4pHyperlanes"), Some(preset))
                    .unwrap();
            let rebuilt = rebuild_galaxy(content, &list, 7, Some("4pHyperlanes"), &state).unwrap();
            assert_eq!(
                build_board_tiles(content, &galaxy),
                build_board_tiles(content, &rebuilt),
                "{preset}"
            );
        }
    }

    #[test]
    fn a_random_speaker_follows_the_seed_and_changes_nothing_else() {
        let content = ContentStore::embedded();
        for n in [3, 4, 5, 6] {
            let list = players(n);
            let mut speakers = std::collections::BTreeSet::new();
            for seed in 0..24_u64 {
                let (seat, seat_galaxy) =
                    create_game_with_options(content, &list, seed, None, None, None).unwrap();
                assert_eq!(
                    seat.speaker, list[0],
                    "seat order opens with the first seat"
                );
                let (random, galaxy) = create_game_with_speaker(
                    content,
                    &list,
                    seed,
                    None,
                    None,
                    None,
                    SpeakerOrder::Random,
                )
                .unwrap();
                let (again, _) = create_game_with_speaker(
                    content,
                    &list,
                    seed,
                    None,
                    None,
                    None,
                    SpeakerOrder::Random,
                )
                .unwrap();
                assert_eq!(
                    random.speaker, again.speaker,
                    "{n}p seed {seed}: same seed, same speaker"
                );
                assert_eq!(
                    Some(random.speaker.clone()),
                    ti4_engine::seeded_speaker(&list, seed)
                );
                // Seats, factions, board and decks are untouched.
                let mut reseated = random.clone();
                reseated.speaker = seat.speaker.clone();
                assert_eq!(reseated, seat, "{n}p seed {seed}");
                assert_eq!(
                    build_board_tiles(content, &galaxy),
                    build_board_tiles(content, &seat_galaxy)
                );
                speakers.insert(random.speaker);
            }
            assert!(speakers.len() > 1, "{n}p: the speaker varies across seeds");
        }
        // A preset game draws the same speaker.
        let list = players(4);
        let (state, _) = create_game_with_speaker(
            content,
            &list,
            5,
            None,
            Some("combat"),
            None,
            SpeakerOrder::Random,
        )
        .unwrap();
        assert_eq!(Some(state.speaker), ti4_engine::seeded_speaker(&list, 5));
    }

    #[test]
    fn every_roster_faction_is_dealt_its_own_notes_at_every_table_size() {
        use crate::preset::ROSTER;
        let content = ContentStore::embedded();
        let mut failures = Vec::new();
        for alias in ROSTER {
            for (n, seed) in [(3, 1_u64), (4, 2), (6, 3)] {
                let list = players(n);
                let name = format!("world+fac:{alias}");
                // Both the plain map path and a preset (which seats named factions).
                let (state, _) = create_game_with_map(content, &list, seed).unwrap();
                assert!(
                    state
                        .promissory_notes
                        .keys()
                        .all(|k| !k.ends_with(":generic")),
                    "generic notes left in a {n}p default game"
                );
                let _ = &name;
                let Ok((state, _)) =
                    create_game_with_preset(content, &list, seed, None, Some(&name))
                else {
                    failures.push(format!("{alias} {n}p"));
                    continue;
                };
                for seat in &state.players {
                    let faction = seat.faction.as_str();
                    for generic in ["cf", "ps", "ta"] {
                        let id = ti4_engine::promissory::note_id(generic, faction);
                        if !state.promissory_notes.contains_key(&id) {
                            failures.push(format!("{alias} {n}p: {faction} lacks {id}"));
                        }
                    }
                    let own = ti4_content::factions::get(content, faction)
                        .map(|f| f.promissory_notes().len())
                        .unwrap_or(0);
                    let held = state
                        .promissory_notes
                        .keys()
                        .filter(|k| ti4_engine::promissory::owner_of(k).as_deref() == Some(faction))
                        .count();
                    if own == 0 || held < 3 + own {
                        failures.push(format!("{alias} {n}p: {faction} holds {held}, own {own}"));
                    }
                }
            }
        }
        assert!(failures.is_empty(), "{failures:#?}");
    }
}
