//! The setup of a table while it is a lobby: its settings, the order of its seats, and the
//! scenario that it starts from. `web/` and web2 use the same table.
//!
//! The players sit in the order of the slots, which is the order of the rows of the lobby: the
//! order of the join, until the host shuffles the seats ([`shuffle_seats`]).
//!
//! Everything here works on a [`PlayerLobbyRecord`]; the registry holds the lock, checks the
//! caller and saves the result.

use serde::{Deserialize, Deserializer};
use ti4_host::setup::TableSetup;
use ti4_model::id::PlayerId;

use ti4_host::maps::MapChoice;

use super::registry::LobbyError;
use crate::storage::{LobbySlotId, PlayerLobbyRecord, PlayerLobbySlot};

pub use ti4_host::setup::{
    BotKindOption, CardOption, CardSetOption, FactionGroup, FactionOption, MAX_BOT_DELAY_MS,
    PLAYER_COUNTS, SetupOptions,
};

/// What the host of `record` chose, as the setup of `ti4-host` has it: the server starts the
/// table and pictures it with the functions that the wasm hosts use for the table of a page.
#[must_use]
pub fn setup_of(record: &PlayerLobbyRecord) -> TableSetup {
    TableSetup {
        player_count: record.slots.len(),
        map: MapChoice::from_stored(record.map_template.as_deref()),
        strategy_cards: crate::storage::stored_card_set(record.strategy_card_set.as_deref())
            .to_owned(),
        speaker_order: ti4_host::setup::SpeakerOrder::from_stored(record.speaker_order.as_deref()),
        start_from: record.start_from.clone(),
        preset: record.start_preset.clone(),
        seed: record.seed,
    }
}

/// The players of a full table in the order of its slots: the seats of the game.
///
/// # Panics
/// When a slot is open: the caller checks that the table is full.
#[must_use]
pub fn joined_players(record: &PlayerLobbyRecord) -> Vec<PlayerId> {
    record
        .slots
        .iter()
        .map(|slot| slot.occupant.clone().expect("full lobby"))
        .collect()
}

/// The table with the players of its slots in another order, open seats included: the rows of
/// the lobby, and so the seats of the game. A table of two or more different slots always gets
/// another order.
#[must_use]
pub fn shuffle_seats(record: &PlayerLobbyRecord, rng: &mut impl rand::Rng) -> PlayerLobbyRecord {
    use rand::seq::SliceRandom;
    let before: Vec<Option<PlayerId>> = record
        .slots
        .iter()
        .map(|slot| slot.occupant.clone())
        .collect();
    let mut order = before.clone();
    let movable = before.iter().any(|occupant| occupant != &before[0]);
    while movable && order == before {
        order.shuffle(rng);
    }
    let mut updated = record.clone();
    for (slot, occupant) in updated.slots.iter_mut().zip(order) {
        slot.occupant = occupant;
    }
    updated
}

/// `null` and a value are told apart from a missing field: `Some(None)` clears.
fn nullable<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

/// Body of `POST /api/games/{game_id}/lobby/settings`. A field that is missing stays as it is.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TableSettings {
    /// 2 to 8.
    pub player_count: Option<usize>,
    /// One of [`ti4_host::setup::card_set::OFFERED`].
    pub strategy_card_set: Option<String>,
    /// 0 to [`MAX_BOT_DELAY_MS`].
    pub bot_delay_ms: Option<u64>,
    /// The id of a scenario of `ti4_host::view::scenario`, or `null` for round 1.
    #[serde(default, deserialize_with = "nullable")]
    pub start_from: Option<Option<String>>,
}

fn people_of(record: &PlayerLobbyRecord) -> Vec<&str> {
    record
        .slots
        .iter()
        .filter_map(|slot| slot.occupant.as_ref())
        .map(|id| &record.players[id])
        .filter(|member| member.bot.is_none())
        .map(|member| member.nickname.as_str())
        .collect()
}

fn listed(names: &[&str]) -> String {
    match names {
        [] => String::new(),
        [one] => (*one).to_owned(),
        [first @ .., last] => format!("{} and {last}", first.join(", ")),
    }
}

/// Gives the table `count` seats. More seats are added open. Fewer: the open seats go first
/// (the last ones), then the seats that the server plays (the last ones, with their chosen
/// factions). A seat of a person is never removed.
///
/// The map of a table is for one count, so a template becomes the default map of the new count;
/// a random board stays a random board.
fn resize(record: &mut PlayerLobbyRecord, count: usize) -> Result<(), LobbyError> {
    if !PLAYER_COUNTS.contains(&count) {
        return Err(LobbyError::InvalidSetting(
            "player_count must be 2-8".to_owned(),
        ));
    }
    if count == record.slots.len() {
        return Ok(());
    }
    let people = people_of(record);
    if people.len() > count {
        let over = &people[count..];
        return Err(LobbyError::SeatsTaken(format!(
            "A table of {count} has no seat for {}: {} people are at the table",
            listed(over),
            people.len()
        )));
    }
    while record.slots.len() > count {
        let open = record
            .slots
            .iter()
            .rposition(|slot| slot.occupant.is_none());
        let index = open.unwrap_or_else(|| {
            record
                .slots
                .iter()
                .rposition(|slot| {
                    slot.occupant
                        .as_ref()
                        .is_some_and(|id| record.players[id].bot.is_some())
                })
                .expect("more seats than people: one is open or played by the server")
        });
        if let Some(player) = record.slots.remove(index).occupant {
            record.players.remove(&player);
        }
    }
    let mut number = 1;
    while record.slots.len() < count {
        let slot_id = LobbySlotId(format!("slot_{number}"));
        number += 1;
        if record.slots.iter().all(|slot| slot.slot_id != slot_id) {
            record.slots.push(PlayerLobbySlot {
                slot_id,
                occupant: None,
            });
        }
    }
    if record.map_template.is_some() {
        record.map_template = ti4_host::maps::catalog::recommended_for(count);
    }
    record.map_revision += 1;
    Ok(())
}

/// Applies `settings` to a copy of `record` and returns it; `record` is untouched when a field
/// is refused. `public`: the table is on the public server, which starts no scenario.
///
/// # Errors
/// [`LobbyError::InvalidSetting`] for a value outside its range or a name nobody offers,
/// [`LobbyError::SeatsTaken`] when a person is in the way of a smaller table, and
/// [`LobbyError::ScenarioTable`] for a player count that the scenario of the table does not have.
pub fn apply_settings(
    record: &PlayerLobbyRecord,
    settings: &TableSettings,
    public: bool,
) -> Result<PlayerLobbyRecord, LobbyError> {
    let mut updated = record.clone();
    if let Some(choice) = &settings.start_from {
        match choice {
            None => {
                if updated.start_from.take().is_some() {
                    updated.map_revision += 1;
                }
            }
            Some(id) => {
                if public {
                    return Err(LobbyError::InvalidSetting(
                        "start_from is not available on the public server".to_owned(),
                    ));
                }
                let scenario = ti4_host::view::scenario::find(id).ok_or_else(|| {
                    let known: Vec<&str> = ti4_host::view::scenario::SCENARIOS
                        .iter()
                        .map(|scenario| scenario.id)
                        .collect();
                    LobbyError::InvalidSetting(format!(
                        "no scenario '{id}': the scenarios are {}",
                        known.join(", ")
                    ))
                })?;
                // A scenario is a game of its own: the table takes its number of seats.
                resize(&mut updated, scenario.players)?;
                if updated.start_from.as_deref() != Some(scenario.id) {
                    updated.start_from = Some(scenario.id.to_owned());
                    updated.map_revision += 1;
                }
            }
        }
    }
    if let Some(count) = settings.player_count {
        if let Some(scenario) = updated
            .start_from
            .as_deref()
            .and_then(ti4_host::view::scenario::find)
            && scenario.players != count
        {
            return Err(LobbyError::ScenarioTable(format!(
                "\"{}\" is a game of {} players",
                scenario.title, scenario.players
            )));
        }
        resize(&mut updated, count)?;
    }
    if let Some(name) = &settings.strategy_card_set {
        let set = ti4_host::setup::card_set::validate(name)
            .map_err(|error| LobbyError::InvalidSetting(error.to_string()))?;
        updated.strategy_card_set = Some(set.to_owned());
    }
    if let Some(delay) = settings.bot_delay_ms {
        if delay > MAX_BOT_DELAY_MS {
            return Err(LobbyError::InvalidSetting(format!(
                "bot_delay_ms must be 0-{MAX_BOT_DELAY_MS}"
            )));
        }
        updated.bot_delay_ms = Some(delay);
    }
    Ok(updated)
}

/// What a host can choose for a table: `GET /api/setup-options`. The page writes none of these
/// names. `public`: the public server, which starts no scenario. The MLP kinds are the models
/// that the process loaded (`ti4_host::bots::models`).
#[must_use]
pub fn setup_options(public: bool) -> SetupOptions {
    let models: Vec<ti4_host::bots::ModelEntry> = ti4_host::bots::models()
        .iter()
        .map(ti4_host::bots::MlpModel::entry)
        .collect();
    let mut options = ti4_host::setup::setup_options(
        "The server",
        super::random_bot::random_bot_delay_ms(),
        &models,
    );
    if public {
        options.scenarios.clear();
    }
    options
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{PlayerLobbyMember, PlayerSession};

    fn table(count: usize) -> PlayerLobbyRecord {
        PlayerLobbyRecord::create("table_test".to_owned(), count, 7, "Ada")
            .unwrap()
            .0
    }

    fn seat(record: &mut PlayerLobbyRecord, name: &str, bot: Option<&str>) -> PlayerId {
        let id = crate::storage::generate_player_id(&record.players);
        let slot = record
            .slots
            .iter_mut()
            .find(|slot| slot.occupant.is_none())
            .expect("an open seat");
        slot.occupant = Some(id.clone());
        record.players.insert(
            id.clone(),
            PlayerLobbyMember {
                legacy_ready: None,
                session: PlayerSession::generate(),
                nickname: name.to_owned(),
                bot: bot.map(str::to_owned),
                faction: None,
            },
        );
        id
    }

    fn count(count: usize) -> TableSettings {
        TableSettings {
            player_count: Some(count),
            ..TableSettings::default()
        }
    }

    #[test]
    fn a_table_grows_with_open_seats_and_shrinks_from_the_open_and_the_bot_seats() {
        let mut record = table(4);
        let person = seat(&mut record, "Bo", None);
        let bot = seat(&mut record, "Rule bot 1", Some("smart"));
        let grown = apply_settings(&record, &count(6), false).unwrap();
        assert_eq!(grown.slots.len(), 6);
        assert_eq!(
            grown.slots.iter().filter(|s| s.occupant.is_none()).count(),
            3
        );
        grown.validate().unwrap();

        // Three seats are taken: the open seat goes, and the bot stays.
        let three = apply_settings(&record, &count(3), false).unwrap();
        assert!(three.players.contains_key(&bot));
        assert!(three.slots.iter().all(|slot| slot.occupant.is_some()));
        three.validate().unwrap();

        // Two seats: the bot goes, the people stay.
        let two = apply_settings(&record, &count(2), false).unwrap();
        assert!(!two.players.contains_key(&bot));
        assert!(two.players.contains_key(&person));
        two.validate().unwrap();
        assert!(two.map_revision > record.map_revision);
    }

    #[test]
    fn a_person_is_never_removed_and_the_refusal_names_who_is_in_the_way() {
        let mut record = table(4);
        seat(&mut record, "Bo", None);
        seat(&mut record, "Cy", None);
        let Err(LobbyError::SeatsTaken(message)) = apply_settings(&record, &count(2), false) else {
            panic!("three people do not fit a table of two");
        };
        assert!(message.contains("Cy"), "{message}");
        assert!(matches!(
            apply_settings(&record, &count(9), false),
            Err(LobbyError::InvalidSetting(_))
        ));
    }

    #[test]
    fn the_map_follows_the_count_and_a_random_board_stays_random() {
        let mut record = table(4);
        record.map_template = ti4_host::maps::catalog::recommended_for(4);
        assert!(record.map_template.is_some());
        let six = apply_settings(&record, &count(6), false).unwrap();
        assert_eq!(
            six.map_template,
            ti4_host::maps::catalog::recommended_for(6)
        );
        assert_ne!(six.map_template, record.map_template);

        record.map_template = None;
        let random = apply_settings(&record, &count(6), false).unwrap();
        assert_eq!(random.map_template, None);
    }

    #[test]
    fn the_card_set_and_the_wait_are_checked() {
        let record = table(3);
        let set = |name: &str| TableSettings {
            strategy_card_set: Some(name.to_owned()),
            ..TableSettings::default()
        };
        let pok = apply_settings(&record, &set("pok"), false).unwrap();
        assert_eq!(pok.strategy_card_set.as_deref(), Some("pok"));
        assert!(apply_settings(&record, &set("base_game"), false).is_err());

        let wait = |ms: u64| TableSettings {
            bot_delay_ms: Some(ms),
            ..TableSettings::default()
        };
        assert_eq!(
            apply_settings(&record, &wait(0), false)
                .unwrap()
                .bot_delay_ms,
            Some(0)
        );
        assert!(apply_settings(&record, &wait(MAX_BOT_DELAY_MS + 1), false).is_err());
    }

    #[test]
    fn a_scenario_gives_the_table_its_seats_and_the_public_server_refuses_it() {
        let record = table(4);
        let late = |id: Option<&str>| TableSettings {
            start_from: Some(id.map(str::to_owned)),
            ..TableSettings::default()
        };
        let scenario = ti4_host::view::scenario::find(ti4_host::view::scenario::LATE_GAME).unwrap();
        let started = apply_settings(&record, &late(Some(scenario.id)), false).unwrap();
        assert_eq!(started.slots.len(), scenario.players);
        assert!(matches!(
            apply_settings(&started, &count(4), false),
            Err(LobbyError::ScenarioTable(_))
        ));
        let cleared = apply_settings(&started, &late(None), false).unwrap();
        assert_eq!(cleared.start_from, None);
        assert_eq!(cleared.slots.len(), scenario.players);

        assert!(apply_settings(&record, &late(Some("nope")), false).is_err());
        assert!(apply_settings(&record, &late(Some(scenario.id)), true).is_err());
    }

    #[test]
    fn a_missing_field_and_null_are_told_apart() {
        let none: TableSettings = serde_json::from_str("{}").unwrap();
        assert_eq!(none.start_from, None);
        let cleared: TableSettings = serde_json::from_str(r#"{"start_from":null}"#).unwrap();
        assert_eq!(cleared.start_from, Some(None));
    }

    fn occupants(record: &PlayerLobbyRecord) -> Vec<Option<PlayerId>> {
        record
            .slots
            .iter()
            .map(|slot| slot.occupant.clone())
            .collect()
    }

    #[test]
    fn a_shuffle_reorders_the_rows_and_keeps_everything_else() {
        use rand::SeedableRng;
        let mut record = table(6);
        for name in ["Bo", "Cy", "Di"] {
            seat(&mut record, name, None);
        }
        let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(1);
        for _ in 0..50 {
            let shuffled = shuffle_seats(&record, &mut rng);
            assert_ne!(occupants(&shuffled), occupants(&record), "another order");
            let mut before = occupants(&record);
            let mut after = occupants(&shuffled);
            before.sort();
            after.sort();
            assert_eq!(after, before, "the same players and open seats");
            let slots = |r: &PlayerLobbyRecord| -> Vec<LobbySlotId> {
                r.slots.iter().map(|slot| slot.slot_id.clone()).collect()
            };
            assert_eq!(slots(&shuffled), slots(&record));
            assert_eq!(shuffled.players.len(), record.players.len());
            shuffled.validate().unwrap();
            // The game seats the players in the order of the rows.
            let mut full = shuffled.clone();
            for slot in &mut full.slots {
                slot.occupant.get_or_insert_with(|| PlayerId::new("open"));
            }
            assert_eq!(
                joined_players(&full)
                    .into_iter()
                    .map(Some)
                    .collect::<Vec<_>>(),
                occupants(&full)
            );
        }
    }

    #[test]
    fn the_setup_options_name_every_set_faction_and_kind() {
        let options = setup_options(false);
        assert_eq!(options.strategy_card_sets.len(), 3);
        assert!(
            options
                .strategy_card_sets
                .iter()
                .all(|set| set.cards.len() == 8)
        );
        assert_eq!(
            options
                .strategy_card_sets
                .iter()
                .filter(|set| set.default)
                .count(),
            1
        );
        let factions: usize = options
            .faction_groups
            .iter()
            .map(|group| group.factions.len())
            .sum();
        assert_eq!(factions, ti4_engine::seating::LOBBY_FACTIONS.len());
        assert!(
            options
                .faction_groups
                .iter()
                .flat_map(|group| &group.factions)
                .all(|faction| faction.name != faction.id)
        );
        assert_eq!(
            options.bot_kinds[0].description,
            "The server plays this seat by rules: it builds, moves and scores."
        );
        assert!(!options.scenarios.is_empty());
        assert!(setup_options(true).scenarios.is_empty());
    }
}
