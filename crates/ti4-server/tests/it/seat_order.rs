//! The seats of a table are the rows of its lobby: the order of the join, until the host shuffles
//! them ("Shuffle seats", `POST …/lobby/shuffle-seats`). Nothing is drawn at the start, and there
//! is no "Ready". A seat that the server plays decides by its place in that order, not by its id,
//! so the same table of a seed is the same game every time.

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::{Duration, Instant};

use ti4_engine::choice::DecisionRecord;
use ti4_host::protocol::status::ViewerRole;
use ti4_model::id::PlayerId;
use ti4_server::session::GameRegistry;
use ti4_server::session::random_bot::{decision_rng, pick_index};
use ti4_server::session::registry::LobbyError;
use ti4_server::session::table::TableSettings;
use ti4_server::storage::FileGameStore;

const GUESTS: [&str; 5] = ["Bo", "Cy", "Di", "Ed", "Fa"];

struct Started {
    /// The player of each nickname, the host ("Ada") first.
    players: Vec<(String, PlayerId)>,
    /// The nicknames in the order of the seats.
    seats: Vec<String>,
    /// The nicknames in the order of the rows just before the start.
    rows: Vec<String>,
}

fn nicknames(view: &ti4_server::session::registry::PlayerLobbyView) -> Vec<String> {
    view.slots
        .iter()
        .map(|slot| slot.nickname.clone().unwrap())
        .collect()
}

/// A table of six people on `seed`, its seats shuffled `shuffles` times, started.
fn started(id: &str, seed: u64, shuffles: usize, registry: GameRegistry) -> Started {
    let (_, host, token) = registry
        .create_player_lobby(id.to_owned(), 6, seed, "Ada")
        .unwrap();
    let mut players = vec![("Ada".to_owned(), host)];
    for name in GUESTS {
        let (_, player, _) = registry.join_player_lobby(id, None, Some(name)).unwrap();
        players.push((name.to_owned(), player));
    }
    for _ in 0..shuffles {
        registry.shuffle_seats(id, token.as_str()).unwrap();
    }
    let rows = nicknames(&registry.player_lobby_status(id, None).unwrap().0);
    let view = registry.start_player_lobby(id, token.as_str()).unwrap();
    let seats = nicknames(&view);
    // The slots of the running table are the seats of the game.
    let session = registry.get_game(id).unwrap();
    let of_the_game: Vec<String> = session
        .lobby_details()
        .0
        .iter()
        .map(|seat| {
            players
                .iter()
                .find(|(_, player)| player == seat)
                .unwrap()
                .0
                .clone()
        })
        .collect();
    assert_eq!(seats, of_the_game);
    session.stop();
    Started {
        players,
        seats,
        rows,
    }
}
fn joined() -> Vec<String> {
    std::iter::once("Ada")
        .chain(GUESTS)
        .map(str::to_owned)
        .collect()
}

#[test]
fn the_seats_are_the_rows_in_the_order_in_which_the_players_came() {
    let table = started("order_join", 7, 0, GameRegistry::new());
    assert_eq!(table.seats, joined());
    assert_eq!(table.rows, joined());
    // Another seed seats them the same way: nothing is drawn.
    assert_eq!(
        started("order_join_2", 8, 0, GameRegistry::new()).seats,
        joined()
    );
}

#[test]
fn the_host_shuffles_the_rows_and_the_game_seats_them_as_shown() {
    let table = started("order_shuffled", 7, 1, GameRegistry::new());
    assert_ne!(table.rows, joined(), "a shuffle gives another order");
    assert_eq!(table.seats, table.rows);
    assert_eq!(
        table.seats.iter().collect::<BTreeSet<_>>(),
        joined().iter().collect::<BTreeSet<_>>(),
        "every player has one seat"
    );
}

#[test]
fn only_the_host_shuffles_and_only_before_the_start() {
    let registry = GameRegistry::new();
    let (_, _, token) = registry
        .create_player_lobby("order_host".to_owned(), 3, 5, "Ada")
        .unwrap();
    let (before, _, guest) = registry
        .join_player_lobby("order_host", None, Some("Bo"))
        .unwrap();
    let guest = guest.unwrap();
    assert_eq!(
        registry
            .shuffle_seats("order_host", guest.as_str())
            .unwrap_err(),
        LobbyError::HostRequired
    );
    let after = registry
        .shuffle_seats("order_host", token.as_str())
        .unwrap();
    assert!(after.lobby_version > before.lobby_version);
    let occupants = |view: &ti4_server::session::registry::PlayerLobbyView| {
        view.slots
            .iter()
            .map(|slot| slot.occupant.clone())
            .collect::<Vec<_>>()
    };
    let shown = registry.player_lobby_status("order_host", None).unwrap().0;
    assert_ne!(occupants(&shown), occupants(&before));
    assert_eq!(occupants(&shown), occupants(&after));
    registry
        .add_in_process_bots("order_host", token.as_str(), "smart", None, None, None)
        .unwrap();
    registry
        .start_player_lobby("order_host", token.as_str())
        .unwrap();
    assert_eq!(
        registry
            .shuffle_seats("order_host", token.as_str())
            .unwrap_err(),
        LobbyError::AlreadyRunning
    );
    registry.get_game("order_host").unwrap().stop();
}

/// A table of a person and five seats that the server plays (rule bots and random seats), on
/// `seed`, with no wait. The person answers by a rule of the test, so the game is a function of
/// the seed; its first `decisions` decisions, with the id of every player written as its seat
/// (`seat0`, `seat1`, …).
fn bot_game(id: &str, seed: u64, decisions: usize) -> Vec<String> {
    let registry = GameRegistry::new();
    let (_, host, token) = registry
        .create_player_lobby(id.to_owned(), 6, seed, "Ada")
        .unwrap();
    registry
        .change_player_lobby_settings(
            id,
            token.as_str(),
            &TableSettings {
                bot_delay_ms: Some(0),
                ..TableSettings::default()
            },
        )
        .unwrap();
    for (kind, count) in [("smart", 3), ("random", 2)] {
        registry
            .add_in_process_bots(id, token.as_str(), kind, None, Some(count), None)
            .unwrap();
    }
    registry.start_player_lobby(id, token.as_str()).unwrap();
    let session = registry.get_game(id).unwrap();
    let seats = session.lobby_details().0;
    let deadline = Instant::now() + Duration::from_secs(120);
    while session.decision_log().len() < decisions && !session.is_finished() {
        assert!(session.error().is_none(), "{:?}", session.error());
        assert!(Instant::now() < deadline, "the game stalled");
        let snapshot = session.get_snapshot(&ViewerRole::Player(host.clone()));
        if let Some(pending) = snapshot.pending_choice {
            // The person answers as a random seat of place 0 would: a function of the seed and
            // the number of the decision.
            let mut rng = decision_rng(seed, 0, session.decision_log().len());
            let option = &pending.choice.options[pick_index(&pending.choice, &mut rng)];
            let _ = session.submit_choice(&host, &pending.nonce, snapshot.game_version, &option.id);
        } else {
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    session.stop();
    let log = session.decision_log();
    log[..decisions.min(log.len())]
        .iter()
        .map(|record: &DecisionRecord| {
            let mut text = serde_json::to_string(record).expect("a record");
            for (place, seat) in seats.iter().enumerate() {
                text = text.replace(seat.as_str(), &format!("seat{place}"));
            }
            text
        })
        .collect()
}

#[test]
fn a_table_of_a_seed_is_the_same_game_every_time() {
    // The ids of the players are new for each table: the bots decide by their place.
    let first = bot_game("same_a", 21, 300);
    let again = bot_game("same_b", 21, 300);
    assert_eq!(first.len(), 300, "the game is long enough");
    for (number, (one, two)) in first.iter().zip(&again).enumerate() {
        assert_eq!(one, two, "decision {number}");
    }
    let other = bot_game("same_c", 22, 300);
    assert_ne!(first, other, "another seed is another game");
}

#[test]
fn a_chosen_faction_stays_with_its_player_whatever_its_seat() {
    for seed in [3, 7, 11] {
        let id = format!("order_faction_{seed}");
        let registry = GameRegistry::new();
        let (_, host, token) = registry
            .create_player_lobby(id.clone(), 5, seed, "Ada")
            .unwrap();
        let (_, guest, guest_token) = registry.join_player_lobby(&id, None, Some("Bo")).unwrap();
        registry
            .set_seat_faction(&id, guest_token.unwrap().as_str(), &guest, Some("hacan"))
            .unwrap();
        registry
            .set_seat_faction(&id, token.as_str(), &host, Some("ghost"))
            .unwrap();
        registry
            .add_in_process_bots(&id, token.as_str(), "smart", None, None, None)
            .unwrap();
        registry.start_player_lobby(&id, token.as_str()).unwrap();
        let session = registry.get_game(&id).unwrap();
        let state = session.current_state();
        assert_eq!(state.player(&guest).unwrap().faction.as_str(), "hacan");
        assert_eq!(state.player(&host).unwrap().faction.as_str(), "ghost");
        let factions: BTreeSet<&str> = state
            .players
            .iter()
            .map(|seat| seat.faction.as_str())
            .collect();
        assert_eq!(factions.len(), 5, "every seat has another faction");
        session.stop();
    }
}

#[test]
fn a_table_starts_when_every_seat_is_filled_and_nobody_says_ready() {
    let registry = GameRegistry::new();
    let (_, _, token) = registry
        .create_player_lobby("order_open".to_owned(), 3, 5, "Ada")
        .unwrap();
    let (_, _, guest) = registry
        .join_player_lobby("order_open", None, Some("Bo"))
        .unwrap();
    assert_eq!(
        registry
            .start_player_lobby("order_open", token.as_str())
            .unwrap_err(),
        LobbyError::SeatsOpen
    );
    registry
        .join_player_lobby("order_open", None, Some("Cy"))
        .unwrap();
    assert_eq!(
        registry
            .start_player_lobby("order_open", guest.unwrap().as_str())
            .unwrap_err(),
        LobbyError::HostRequired
    );
    registry
        .start_player_lobby("order_open", token.as_str())
        .unwrap();
    registry.get_game("order_open").unwrap().stop();
}

#[test]
fn a_recovered_game_has_the_seats_that_were_shown() {
    let dir = std::env::temp_dir().join(format!("ti4_order_{:032x}", rand::random::<u128>()));
    let store = Arc::new(FileGameStore::new(&dir).unwrap());
    let table = started(
        "order_disk",
        9,
        1,
        GameRegistry::new().with_store(store.clone()),
    );
    assert_ne!(table.seats, joined());
    let seated: Vec<PlayerId> = table
        .seats
        .iter()
        .map(|name| {
            table
                .players
                .iter()
                .find(|(nickname, _)| nickname == name)
                .unwrap()
                .1
                .clone()
        })
        .collect();
    assert_eq!(
        store.load_player_init("order_disk").unwrap().player_ids,
        seated
    );
    drop(table);

    let recovered = GameRegistry::new().with_store(store);
    assert_eq!(recovered.recover_all_games().unwrap(), vec!["order_disk"]);
    let session = recovered.get_game("order_disk").unwrap();
    assert_eq!(session.lobby_details().0, seated);
    let view = recovered.player_lobby_status("order_disk", None).unwrap().0;
    assert_eq!(
        view.slots
            .iter()
            .map(|slot| slot.occupant.clone().unwrap())
            .collect::<Vec<_>>(),
        seated
    );
    session.stop();
    let _ = std::fs::remove_dir_all(dir);
}
