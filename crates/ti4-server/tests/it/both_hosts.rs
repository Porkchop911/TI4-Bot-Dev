//! One table, two hosts: a table of a seed, with the same seats played by the same kinds, is the
//! same game in the wasm host (single player of web2) and on the server.
//!
//! Both start the table with `ti4_host::setup`, and a seat that no person plays decides from the
//! seed, its place in the seat order and the number of the decision (`ti4_rule_bot::choose`).
//! This test holds the two deciders together: the wasm host counts the decisions in its own way.
//! The ids of the players are not the same on the two hosts (the page has letters, the server
//! makes random ones), and the game does not depend on them.
//!
//! A saved game is the same on both too: the option id of every decision, of every seat, in
//! order. The wasm host plays the log of the server again (`resume.log` of `ti4_host_start`).
//! An MLP seat (`ti4_host::bots`) is played by the same model on both, on a deterministic backend,
//! and a saved game cut anywhere goes on as the game that was not cut.
//!
//! One test: the wasm host keeps its game in statics.

use std::time::{Duration, Instant};

use ti4_content::ContentStore;
use ti4_engine::choice::DecisionRecord;
use ti4_host::setup::{self, TableSetup};
use ti4_model::id::PlayerId;
use ti4_server::session::{GameSession, SeatController, SessionConfig};

/// The players of a table in the order in which they came: the kind of the seat, and the
/// faction that was chosen for it.
type Seats = [(&'static str, Option<&'static str>)];

/// The table as the setup exports of the wasm host read it.
fn request(setup: &TableSetup, seats: &Seats) -> serde_json::Value {
    serde_json::json!({
        "setup": setup,
        "seats": seats
            .iter()
            .map(|(kind, faction)| serde_json::json!({ "kind": kind, "faction": faction }))
            .collect::<Vec<_>>(),
    })
}

/// The first round of the table in the wasm host, with the decisions of `saved` given again
/// (a saved game: `resume.log`) and the seats deciding the rest.
fn in_the_page(setup: &TableSetup, seats: &Seats, saved: &[DecisionRecord]) -> Vec<DecisionRecord> {
    ti4_wasm::set_native_host(|output| {
        let envelope: serde_json::Value = serde_json::from_str(output).expect("an envelope");
        assert_eq!(
            envelope["kind"], "progress",
            "no person is at the table: {output}"
        );
        ti4_wasm::host_api::CONTINUE
    });
    let log: Vec<&str> = saved.iter().map(|record| record.chosen.as_str()).collect();
    let start = serde_json::json!({
        "table": request(setup, seats),
        // A resume names the versions of the game it plays; the server plays the current ones.
        "resume": {
            "log": log,
            "bot_version": ti4_rule_bot::tuning::BOT_VERSION,
            "rules_version": ti4_model::state::RULES_VERSION,
        },
    });
    let len = ti4_wasm::host_api::put_input(&start.to_string());
    let status = ti4_wasm::ti4_host_start(u32::try_from(len).expect("a short request"));
    let end = ti4_wasm::host_api::out_text();
    assert!(status >= 0, "the table plays in the wasm host: {end}");
    let end: serde_json::Value = serde_json::from_str(&end).expect("the end envelope");
    assert!(end["end"]["error"].is_null(), "{end}");
    ti4_wasm::decisions_native()
}

/// The table on the server, until `decisions` decisions are made or the game is over. The ids
/// of the players are not those of the wasm host, and they sort in the other order than the
/// seats: the decisions are given back with the id of each player as the page has it (the
/// player of seat `i` is the letter `i`).
fn on_the_server(setup: &TableSetup, seats: &Seats, decisions: usize) -> Vec<DecisionRecord> {
    let content = ContentStore::embedded();
    let joined: Vec<PlayerId> = (0..seats.len())
        .map(|seat| PlayerId::new(format!("player_{:x}", 0xffff - seat * 0x1111)))
        .collect();
    let letters = ["a", "b", "c", "d", "e", "f", "g", "h"];
    let chosen: Vec<Option<String>> = seats
        .iter()
        .map(|(_, faction)| faction.map(str::to_owned))
        .collect();
    let drawn = setup::draw(setup, &chosen).expect("the table is drawn");
    let players = joined.clone();
    let (state, galaxy) = setup::create(content, setup, &players, &drawn).expect("the game");
    let tiles = ti4_host::map::build_board_tiles(content, &galaxy);
    let mut config = SessionConfig::new("both_hosts", state)
        .with_seed(setup.game_seed())
        .with_player_ids(players)
        .with_galaxy(galaxy, tiles)
        .with_random_bot_delay_ms(0);
    for (player, (kind, _)) in joined.iter().cloned().zip(seats) {
        assert_ne!(*kind, setup::HUMAN_KIND, "this test has no person");
        let controller =
            SeatController::Bot(ti4_host::bots::BotKind::parse(kind).expect("a kind of the tests"));
        config = config.with_seat(player, controller);
    }
    let session = GameSession::start(config);
    let mut last = (0, Instant::now());
    loop {
        let made = session.decision_log().len();
        if made != last.0 {
            last = (made, Instant::now());
        }
        assert!(session.error().is_none(), "{:?}", session.error());
        if made >= decisions || session.is_finished() {
            break;
        }
        assert!(
            last.1.elapsed() < Duration::from_secs(60),
            "the server stalled after {made} decisions"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    session.stop();
    session
        .decision_log()
        .iter()
        .map(|record| {
            let mut text = serde_json::to_string(record).expect("a record");
            for (player, letter) in joined.iter().zip(letters) {
                text = text.replace(player.as_str(), letter);
            }
            serde_json::from_str(&text).expect("a record")
        })
        .collect()
}

#[test]
fn slow_a_table_of_a_seed_is_the_same_game_in_the_page_and_on_the_server() {
    ti4_wasm::set_round_limit_native(Some(1));
    // Both hosts play the MLP seats with the model of the tests, on the scalar backend.
    let _ = ti4_host::bots::test_model();
    let mut pok = TableSetup::new(5, 23);
    pok.strategy_cards = setup::card_set::POK.to_owned();
    let tables: [(TableSetup, &Seats); 3] = [
        (
            TableSetup::new(4, 7),
            &[
                ("smart", None),
                ("smart", None),
                ("smart", None),
                ("smart", None),
            ],
        ),
        // Random seats too, a chosen faction, and another set of cards.
        (
            pok,
            &[
                ("smart", Some("hacan")),
                ("random", None),
                ("smart", None),
                ("random", Some("ghost")),
                ("smart", None),
            ],
        ),
        // MLP seats: their decision pipelines, the fleet plans of their activations, and a saved
        // game whose second half they play on from the first (`Bot::follow`).
        (
            TableSetup::new(3, 1),
            &[("mlp:test", None), ("smart", None), ("mlp:test", None)],
        ),
    ];
    for (setup, seats) in tables {
        // A new table of the lobby has the choice of the starting technology, as the page gives
        // it to a game of the current bot.
        let setup = setup.with_starting_tech();
        // The MLP seats play two rounds: their fleets of the second round are planned.
        let mlp = seats
            .iter()
            .any(|(kind, _)| kind.starts_with(ti4_host::bots::MLP_PREFIX));
        ti4_wasm::set_round_limit_native(Some(if mlp { 2 } else { 1 }));
        let page = in_the_page(&setup, seats, &[]);
        assert!(
            page.len() > 100,
            "a round of {} seats has more than {} decisions",
            seats.len(),
            page.len()
        );
        let server = on_the_server(&setup, seats, page.len());
        assert!(server.len() >= page.len(), "the server stopped early");
        for (number, (here, there)) in page.iter().zip(&server).enumerate() {
            assert_eq!(
                here,
                there,
                "decision {number} of {} seats on seed {}: the page (left) and the server",
                seats.len(),
                setup.seed
            );
        }
        // The log of the server is a saved game of the page: the host gives the first half of
        // it, the seats decide the rest, and it is the same game.
        let half = page.len() / 2;
        let again = in_the_page(&setup, seats, &server[..half]);
        assert_eq!(
            again,
            page,
            "the game of a saved game, {} seats",
            seats.len()
        );
        // A save cut before a move of an MLP seat: between the activation (and its fleet
        // decision) and the moves that its plan answers.
        let mlp_seats: Vec<&str> = seats
            .iter()
            .zip(["a", "b", "c", "d", "e", "f", "g", "h"])
            .filter(|((kind, _), _)| kind.starts_with(ti4_host::bots::MLP_PREFIX))
            .map(|(_, letter)| letter)
            .collect();
        let moving = |record: &DecisionRecord| {
            record
                .context
                .as_ref()
                .is_some_and(|context| context.subtype == "movement_step")
        };
        // The first move of each activation of theirs.
        let moves: Vec<usize> = (1..page.len())
            .filter(|&index| {
                let (before, record) = (&page[index - 1], &page[index]);
                mlp_seats.contains(&record.player.as_str())
                    && moving(record)
                    && !(moving(before) && before.player == record.player)
            })
            .collect();
        assert_eq!(moves.is_empty(), mlp_seats.is_empty(), "the MLP seats move");
        for cut in moves {
            let again = in_the_page(&setup, seats, &server[..cut]);
            assert_eq!(again, page, "a saved game cut after {cut} decisions");
        }
    }
    ti4_wasm::set_round_limit_native(None);
}

/// The table of `web/` (`host_setup::Setup`) is a [`TableSetup`]: the wasm host builds the
/// opening state that `ti4_host::setup` (what a lobby start of the server calls) builds for it,
/// with a chosen faction, another set of cards, a start preset and a scenario.
#[test]
fn the_table_of_web_is_the_table_of_the_server() {
    let content = ContentStore::embedded();
    for request in [
        serde_json::json!({ "seed": 7, "players": 4 }),
        serde_json::json!({ "seed": 23, "players": 5, "human_seat": 3, "human_faction": "hacan",
                            "card_set": "pok", "speaker": "seat" }),
        serde_json::json!({ "seed": 5, "players": 4, "preset": "combat", "map": "random" }),
        serde_json::json!({ "seed": 9, "players": 3, "preset": "world+fac:naalu", "bots": "rule" }),
        serde_json::json!({ "seed": 1, "players": 6, "scenario": ti4_host::view::scenario::LATE_GAME }),
    ] {
        let request: ti4_wasm::host_setup::Setup =
            serde_json::from_value(request.clone()).expect("a setup");
        let built = request.build().expect("the wasm host builds the table");
        let resolved = request.resolved().expect("the table is offered");
        let table = resolved.table();
        assert_eq!(table.player_count, built.players);
        assert_eq!(built.humans, 1 << resolved.human_seat.expect("a seat"));
        let chosen: Vec<Option<String>> = (0..built.players)
            .map(|seat| {
                (Some(seat) == resolved.human_seat)
                    .then(|| resolved.human_faction.clone())
                    .flatten()
            })
            .collect();
        let joined: Vec<PlayerId> = ["a", "b", "c", "d", "e", "f", "g", "h"][..built.players]
            .iter()
            .map(|name| PlayerId::new(*name))
            .collect();
        let drawn = setup::draw(&table, &chosen).expect("the table is drawn");
        let (state, galaxy) =
            setup::create(content, &table, &joined, &drawn).expect("the server builds the table");
        // The game of a session places what the galaxy gives (frontier tokens, faction marks).
        let game = ti4_engine::game::Game::with_seeded_random(state, content, table.game_seed())
            .with_sources(ti4_model::content_types::POK)
            .with_galaxy(galaxy);
        let page = serde_json::to_value(&built.game.state).unwrap();
        let server = serde_json::to_value(&game.state).unwrap();
        // Every field, also those that the equality of a state leaves out.
        assert_eq!(page, server, "{table:?}");
    }
}
