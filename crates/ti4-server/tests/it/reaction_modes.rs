//! A seat's "never offer" choice: applied where the question is asked, journaled as an ordinary
//! decline, delivered to its own clients only, and kept across a server restart.

use std::fs;
use std::sync::Arc;
use std::time::{Duration, Instant};

use ti4_host::protocol::server::ServerMessage;
use ti4_host::protocol::status::ViewerRole;
use ti4_model::id::PlayerId;
use ti4_model::state::ReactionMode;
use ti4_server::dev::launch_scenario;
use ti4_server::session::{GameRegistry, GameSession, MockClient};
use ti4_server::storage::FileGameStore;

/// Both sides of the scenario's combat: Sol holds Direct Hit, Letnev holds Sabotage.
struct Table {
    sol: PlayerId,
    letnev: PlayerId,
}

fn table(session: &GameSession, sol: &str) -> Table {
    let sol = PlayerId::new(sol);
    let snapshot = session.get_snapshot(&ViewerRole::Spectator);
    let letnev = snapshot
        .view
        .players
        .iter()
        .find(|player| player.faction.as_str() == "letnev")
        .map(|player| player.id.clone())
        .expect("a Letnev seat");
    Table { sol, letnev }
}

fn pending(session: &GameSession) -> (PlayerId, String, u64, ti4_engine::choice::Choice) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some((seat, nonce, version)) = session.current_pending_decision() {
            if let Some(envelope) = session
                .get_snapshot(&ViewerRole::Player(seat.clone()))
                .pending_choice
            {
                return (seat, nonce, version, envelope.choice);
            }
        }
        assert!(Instant::now() < deadline, "no pending decision");
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// Answer until `subtype` is pending for `seat`, taking `pick` where it names an option.
fn play_until(session: &GameSession, seat_wanted: &PlayerId, subtype: &str) {
    for _ in 0..20 {
        let (seat, nonce, version, choice) = pending(session);
        let current = choice
            .context
            .as_ref()
            .map(|c| c.subtype.clone())
            .unwrap_or_default();
        if &seat == seat_wanted && current == subtype {
            return;
        }
        let pick = choice
            .options
            .iter()
            .find(|option| option.id.starts_with("reaction:") || option.id.starts_with("sustain|"))
            .or(choice.options.first())
            .expect("an option")
            .id
            .clone();
        session
            .submit_choice(&seat, &nonce, version, &pick)
            .expect("accepted");
        std::thread::sleep(Duration::from_millis(30));
    }
    panic!("never reached {subtype}");
}

fn wait_for_decisions(session: &GameSession, at_least: usize) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while session.decision_log().len() < at_least {
        assert!(Instant::now() < deadline, "decisions did not arrive");
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// The reason of each note of a skipped window, and the cards that the note names.
fn skipped_notes(client: &MockClient) -> Vec<(String, Vec<String>)> {
    let mut notes = Vec::new();
    while let Ok(message) = client.try_recv() {
        if let ServerMessage::StateUpdate(update) = message {
            notes.extend(
                update
                    .auto_resolved
                    .into_iter()
                    .map(|note| (note.reason, note.passed)),
            );
        }
    }
    notes
}

#[test]
fn never_declines_the_window_unasked_journals_it_and_survives_a_restart() {
    let dir = std::env::temp_dir().join(format!(
        "ti4_reaction_modes_{:032x}",
        rand::random::<u128>()
    ));
    let store = Arc::new(FileGameStore::new(&dir).expect("store"));
    let registry = Arc::new(GameRegistry::new().with_store(store.clone()));

    // Control: with no preference the Letnev seat is asked about Sabotage.
    let control = launch_scenario(&registry, "ongoing_combat_four_views", Some(54_322))
        .expect("launch control");
    let control_session = registry.get_game(&control.game_id).unwrap();
    let seats = table(&control_session, &control.player_id);
    play_until(
        &control_session,
        &seats.letnev,
        "reaction_when_ACTION_CARD_PLAYED",
    );

    // With Never: the same play opens no window for Letnev.
    let launch =
        launch_scenario(&registry, "ongoing_combat_four_views", Some(54_322)).expect("launch");
    let session = registry.get_game(&launch.game_id).unwrap();
    let seats = table(&session, &launch.player_id);
    let letnev_client =
        MockClient::connect(session.clone(), ViewerRole::Player(seats.letnev.clone()));
    let sol_client = MockClient::connect(session.clone(), ViewerRole::Player(seats.sol.clone()));
    session
        .set_reaction_mode(&seats.letnev, "Sabotage", ReactionMode::Never)
        .expect("owner may set");
    // View sync: only the owner's clients carry it.
    let mine = session.get_snapshot(&ViewerRole::Player(seats.letnev.clone()));
    assert_eq!(
        mine.reaction_modes.get("Sabotage"),
        Some(&ReactionMode::Never)
    );
    assert!(
        session
            .get_snapshot(&ViewerRole::Player(seats.sol.clone()))
            .reaction_modes
            .is_empty()
    );
    assert!(
        session
            .get_snapshot(&ViewerRole::Spectator)
            .reaction_modes
            .is_empty()
    );

    play_until(&session, &seats.sol, "reaction_after_SUSTAIN_DAMAGE_USED");
    let (seat, nonce, version, _) = pending(&session);
    session
        .submit_choice(
            &seat,
            &nonce,
            version,
            "reaction:sol:SUSTAIN_DAMAGE_USED:after",
        )
        .expect("Sol plays Direct Hit");
    wait_for_decisions(&session, 4);
    let (next, _, _, choice) = pending(&session);
    assert_eq!(next, seats.letnev);
    assert_eq!(
        choice.context.as_ref().map(|c| c.subtype.as_str()),
        Some("sustain_damage"),
        "Letnev is asked about the next hit, not about Sabotage"
    );
    let log = session.decision_log();
    let skipped = log
        .iter()
        .find(|record| record.player == seats.letnev && record.prompt == "when ACTION_CARD_PLAYED")
        .expect("the skipped window is journaled");
    assert_eq!(skipped.chosen, "decline");
    // The seat is told why, once, with the cards by name; the other seat is not.
    assert!(
        skipped_notes(&letnev_client)
            .iter()
            .any(|(reason, passed)| reason.contains("Sabotage") && passed == &["Sabotage"]),
        "the owner gets a skip note"
    );
    assert!(skipped_notes(&sol_client).is_empty());

    // The same play in the control game recorded a question that was answered by the human.
    assert!(
        control_session
            .decision_log()
            .iter()
            .all(|record| record.prompt != "when ACTION_CARD_PLAYED" || record.chosen != "decline")
    );

    // Restart: replay reproduces the game from the journal and the preference is still there.
    let hashes = session.decision_hashes();
    let records = session.decision_log();
    let state = session.current_state();
    let letnev_id = seats.letnev.clone();
    let game_id = launch.game_id.clone();
    drop((
        letnev_client,
        sol_client,
        session,
        control_session,
        registry,
    ));
    let restarted = Arc::new(GameRegistry::new().with_store(store.clone()));
    let report = restarted.recover_all_games_report().expect("recover");
    assert!(report.failed.is_empty(), "{:?}", report.failed);
    let recovered = restarted.get_game(&game_id).expect("recovered");
    recovered.wait_replayed().expect("replayed");
    assert_eq!(recovered.decision_log(), records);
    assert_eq!(recovered.decision_hashes(), hashes);
    assert_eq!(recovered.current_state(), state);
    let back = recovered.get_snapshot(&ViewerRole::Player(letnev_id.clone()));
    assert_eq!(
        back.reaction_modes.get("Sabotage"),
        Some(&ReactionMode::Never)
    );
    assert!(
        recovered
            .get_snapshot(&ViewerRole::Player(PlayerId::new(&launch.player_id)))
            .reaction_modes
            .is_empty()
    );

    // Always puts it back, and that is saved too.
    recovered
        .set_reaction_mode(&letnev_id, "Sabotage", ReactionMode::Always)
        .expect("reset");
    assert!(
        recovered
            .get_snapshot(&ViewerRole::Player(letnev_id))
            .reaction_modes
            .is_empty()
    );
    assert!(
        store
            .load_reaction_modes(&game_id)
            .unwrap()
            .into_modes()
            .is_empty()
    );
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn only_a_seat_can_set_its_own_known_cards_and_old_saves_have_no_file() {
    let dir = std::env::temp_dir().join(format!(
        "ti4_reaction_modes_{:032x}",
        rand::random::<u128>()
    ));
    let store = Arc::new(FileGameStore::new(&dir).expect("store"));
    let registry = Arc::new(GameRegistry::new().with_store(store.clone()));
    let launch = launch_scenario(&registry, "ongoing_combat_four_views", Some(7)).unwrap();
    let session = registry.get_game(&launch.game_id).unwrap();
    let seats = table(&session, &launch.player_id);

    assert!(
        session
            .set_reaction_mode(&seats.sol, "Not A Card", ReactionMode::Never)
            .is_err()
    );
    assert!(
        session
            .set_reaction_mode(&PlayerId::new("nobody"), "Sabotage", ReactionMode::Never)
            .is_err()
    );
    // Nothing was saved by the refusals; a game with no preference has no file at all.
    assert!(
        !store
            .game_dir(&launch.game_id)
            .unwrap()
            .join("reaction_modes.json")
            .exists()
    );
    assert!(
        store
            .load_reaction_modes(&launch.game_id)
            .unwrap()
            .into_modes()
            .is_empty()
    );

    session
        .set_reaction_mode(&seats.sol, "Sabotage", ReactionMode::Never)
        .unwrap();
    // The change is the seat's alone.
    assert!(
        session
            .get_snapshot(&ViewerRole::Player(seats.letnev.clone()))
            .reaction_modes
            .is_empty()
    );
    assert_eq!(
        store.load_reaction_modes(&launch.game_id).unwrap().modes[&seats.sol],
        std::iter::once(("Sabotage".to_owned(), ReactionMode::Never)).collect()
    );
    // The card of an ability is set by its key, and "only on my turns" is a mode of its own.
    session
        .set_reaction_mode(&seats.sol, "leader:solagent", ReactionMode::OwnTurn)
        .unwrap();
    assert_eq!(
        session
            .get_snapshot(&ViewerRole::Player(seats.sol.clone()))
            .reaction_modes
            .get("leader:solagent"),
        Some(&ReactionMode::OwnTurn)
    );
    assert!(
        session
            .set_reaction_mode(&seats.sol, "leader:not a key", ReactionMode::Never)
            .is_err()
    );
    session
        .set_reaction_mode(&seats.sol, "leader:solagent", ReactionMode::Always)
        .unwrap();
    // A save of before the modes of abilities has the Never cards alone, and is read.
    let old: ti4_server::storage::ReactionModesRecord =
        serde_json::from_str(r#"{"never":{"p1":["Sabotage"]}}"#).unwrap();
    assert_eq!(
        old.into_modes()[&PlayerId::new("p1")]["Sabotage"],
        ReactionMode::Never
    );
    // A restarted worker (undo, redo, batch) keeps it.
    assert_eq!(session.restart_config().reaction_modes[&seats.sol].len(), 1);
    let _ = fs::remove_dir_all(dir);
}
