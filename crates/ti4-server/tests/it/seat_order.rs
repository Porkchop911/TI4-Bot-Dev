//! The seats of a table are drawn at its start, from the seed of the game: no host moves a seat,
//! and there is no "Ready".

use std::collections::BTreeSet;
use std::sync::Arc;

use ti4_model::id::PlayerId;
use ti4_server::session::GameRegistry;
use ti4_server::session::registry::LobbyError;
use ti4_server::session::table::SeatOrder;
use ti4_server::storage::FileGameStore;

const GUESTS: [&str; 5] = ["Bo", "Cy", "Di", "Ed", "Fa"];

struct Started {
    registry: GameRegistry,
    /// The player of each nickname, the host ("Ada") first.
    players: Vec<(String, PlayerId)>,
    /// The nicknames in the order of the seats.
    seats: Vec<String>,
}

/// A table of six people on `seed`, started.
fn started(id: &str, seed: u64, order: SeatOrder, registry: GameRegistry) -> Started {
    let (_, host, token) = registry
        .create_player_lobby_with_seating(
            id.to_owned(),
            6,
            seed,
            "Ada",
            None,
            None,
            ti4_server::card_set::DEFAULT,
            ti4_server::map::SpeakerOrder::default(),
            order,
        )
        .unwrap();
    let mut players = vec![("Ada".to_owned(), host)];
    for name in GUESTS {
        let (_, player, _) = registry.join_player_lobby(id, None, Some(name)).unwrap();
        players.push((name.to_owned(), player));
    }
    let view = registry.start_player_lobby(id, token.as_str()).unwrap();
    let seats: Vec<String> = view
        .slots
        .iter()
        .map(|slot| slot.nickname.clone().unwrap())
        .collect();
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
        registry,
        players,
        seats,
    }
}

fn joined() -> Vec<String> {
    std::iter::once("Ada")
        .chain(GUESTS)
        .map(str::to_owned)
        .collect()
}

#[test]
fn the_order_is_the_same_for_a_seed_and_another_for_another_seed() {
    let first = started("order_a", 7, SeatOrder::Random, GameRegistry::new());
    let again = started("order_b", 7, SeatOrder::Random, GameRegistry::new());
    // The ids of the players are random: the order is of the places in the lobby.
    assert_eq!(first.seats, again.seats);
    assert_eq!(
        first.seats.iter().collect::<BTreeSet<_>>(),
        joined().iter().collect::<BTreeSet<_>>(),
        "every player has one seat"
    );
    let orders: BTreeSet<Vec<String>> = (1..=6)
        .map(|seed| {
            started(
                &format!("order_seed_{seed}"),
                seed,
                SeatOrder::Random,
                GameRegistry::new(),
            )
            .seats
        })
        .collect();
    assert!(orders.len() > 1, "six seeds gave one order");
    assert!(
        orders.iter().any(|order| *order != joined()),
        "no seed moved a player"
    );
}

#[test]
fn join_keeps_the_order_in_which_the_players_came() {
    let table = started("order_join", 7, SeatOrder::Join, GameRegistry::new());
    assert_eq!(table.seats, joined());
    let view = table
        .registry
        .player_lobby_status("order_join", None)
        .unwrap()
        .0;
    assert_eq!(view.seat_order, "join");
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
fn a_recovered_game_has_the_seats_that_were_drawn() {
    let dir = std::env::temp_dir().join(format!("ti4_order_{:032x}", rand::random::<u128>()));
    let store = Arc::new(FileGameStore::new(&dir).unwrap());
    let table = started(
        "order_disk",
        9,
        SeatOrder::Random,
        GameRegistry::new().with_store(store.clone()),
    );
    let drawn: Vec<PlayerId> = table
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
        drawn
    );
    drop(table);

    let recovered = GameRegistry::new().with_store(store);
    assert_eq!(recovered.recover_all_games().unwrap(), vec!["order_disk"]);
    let session = recovered.get_game("order_disk").unwrap();
    assert_eq!(session.lobby_details().0, drawn);
    let view = recovered.player_lobby_status("order_disk", None).unwrap().0;
    assert_eq!(
        view.slots
            .iter()
            .map(|slot| slot.occupant.clone().unwrap())
            .collect::<Vec<_>>(),
        drawn
    );
    session.stop();
    let _ = std::fs::remove_dir_all(dir);
}
