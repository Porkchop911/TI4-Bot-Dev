//! One sender for the answers of a seat: a draft of a secondary that the seat marked ready is
//! sent by the server when the game asks the seat (`session::ready_secondary`), and a second
//! answer to a choice that the seat answered already is quiet for web2 (`answered_elsewhere`).

use std::sync::Arc;
use std::time::{Duration, Instant};

use futures_util::SinkExt;
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::Message;

use ti4_host::planning::audit::PlanScope;
use ti4_host::planning::draft::DraftStep;
use ti4_host::protocol::PROTOCOL_VERSION;
use ti4_host::protocol::client::ClientMessage;
use ti4_host::protocol::server::{SeatStagedMsg, ServerMessage};
use ti4_host::protocol::staged::StagedSlot;
use ti4_host::protocol::status::{RejectionReason, ViewerRole};
use ti4_host::staged::SecondaryDraft;
use ti4_model::id::PlayerId;
use ti4_server::session::{GameRegistry, GameSession};

use crate::draft_scripts::next_message;
use crate::secondary_draft::{
    LiveOffer, answer_live, answer_until, config_playing, follow_and_research, is_secondary,
    live_offer, play_technology, research, technologies,
};

type Socket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

const DEADLINE: Duration = Duration::from_secs(5);

fn pid(id: &str) -> PlayerId {
    PlayerId::new(id)
}

/// The draft of the secondary of "b" as web2 keeps it: the answers with their labels.
fn draft_value(session: &GameSession, answers: &[String], ready: bool) -> serde_json::Value {
    let facts = session
        .get_snapshot(&ViewerRole::Player(pid("b")))
        .secondary
        .expect("the strategic action");
    let round = session.get_snapshot(&ViewerRole::Spectator).state.round;
    serde_json::json!({
        "round": round,
        "primary": facts.primary,
        "card": facts.card,
        "answers": answers
            .iter()
            .map(|id| serde_json::json!({ "id": id, "label": id }))
            .collect::<Vec<_>>(),
        "ready": ready,
    })
}

/// "b" follows the Technology of "a" and researches: the answers, as a draft run finds them.
fn follow(session: &GameSession) -> Vec<String> {
    let facts = session
        .get_snapshot(&ViewerRole::Player(pid("b")))
        .secondary
        .expect("the strategic action");
    let scope = PlanScope::Secondary {
        primary: facts.primary,
        card: facts.card,
    };
    let run = |script: &[DraftStep]| session.run_draft(&pid("b"), &scope, script).1;
    let question = run(&[]).update.pending_choice.expect("the question").choice;
    let first = follow_and_research(&question);
    let next = run(&[DraftStep::choose(&first)]);
    let second =
        research(&next.update.pending_choice.expect("the research").choice).expect("a technology");
    vec![first, second]
}

/// "a" resolves its primary; the game goes on until it asks "c".
fn resolve_primary(session: &GameSession, primary: &LiveOffer) -> LiveOffer {
    let technology = research(&primary.choice).unwrap();
    answer_until(session, primary, &technology, |choice| {
        choice.player == pid("c")
    })
}

fn secondary_slot(session: &GameSession) -> Option<serde_json::Value> {
    session
        .seat_staged(&pid("b"))
        .slots
        .get(&StagedSlot::Secondary)
        .and_then(|slot| slot.value.clone())
}

#[test]
fn a_ready_secondary_is_sent_by_the_server_when_the_seat_is_reached() {
    let session = GameSession::start(config_playing("ready_reached", "Technology", None));
    let before = technologies(&session, "b");
    let primary = play_technology(&session);
    let answers = follow(&session);
    session
        .put_staged(
            &pid("b"),
            StagedSlot::Secondary,
            0,
            Some(draft_value(&session, &answers, true)),
            1,
        )
        .unwrap();

    // "b" is never asked: the game goes on to "c" with the secondary of "b" in it.
    let next = resolve_primary(&session, &primary);
    assert!(is_secondary(&next.choice));
    assert_eq!(technologies(&session, "b"), before + 1);
    // The draft is spent: the slot is cleared, by the server.
    assert_eq!(secondary_slot(&session), None);
}

#[test]
fn a_ready_secondary_that_no_longer_fits_is_not_ready_and_the_seat_is_asked() {
    let session = GameSession::start(config_playing("ready_unfit", "Technology", None));
    let before = technologies(&session, "b");
    let primary = play_technology(&session);
    let wrong = vec!["nothing".to_owned()];
    session
        .put_staged(
            &pid("b"),
            StagedSlot::Secondary,
            0,
            Some(draft_value(&session, &wrong, true)),
            1,
        )
        .unwrap();

    let technology = research(&primary.choice).unwrap();
    let asked = answer_until(&session, &primary, &technology, |choice| {
        choice.player == pid("b")
    });
    assert!(is_secondary(&asked.choice));
    let kept =
        SecondaryDraft::of(&secondary_slot(&session).expect("the draft stays")).expect("a draft");
    assert!(!kept.ready, "the draft is not ready any more");
    assert_eq!(kept.answers.len(), 1);
    // The seat answers in Live.
    answer_live(&session, &asked, "no");
    assert_eq!(technologies(&session, "b"), before);
}

#[test]
fn a_secondary_marked_ready_while_the_seat_is_asked_is_sent() {
    let session = GameSession::start(config_playing("ready_asked", "Technology", None));
    let before = technologies(&session, "b");
    let primary = play_technology(&session);
    let answers = follow(&session);
    let technology = research(&primary.choice).unwrap();
    let asked = answer_until(&session, &primary, &technology, |choice| {
        choice.player == pid("b")
    });
    // Not ready: nothing is sent.
    session
        .put_staged(
            &pid("b"),
            StagedSlot::Secondary,
            0,
            Some(draft_value(&session, &answers, false)),
            1,
        )
        .unwrap();
    session.send_ready_secondary(&pid("b"));
    assert_eq!(
        session.current_pending_decision().map(|(seat, ..)| seat),
        Some(pid("b"))
    );
    let revision = session.seat_staged(&pid("b")).slots[&StagedSlot::Secondary].revision;
    session
        .put_staged(
            &pid("b"),
            StagedSlot::Secondary,
            revision,
            Some(draft_value(&session, &answers, true)),
            1,
        )
        .unwrap();
    session.send_ready_secondary(&pid("b"));
    let next = live_offer(&session, Some(&asked.nonce));
    let deadline = Instant::now() + DEADLINE;
    let mut next = next;
    while next.choice.player != pid("c") {
        assert!(Instant::now() < deadline, "left with {:?}", next.choice);
        next = live_offer(&session, Some(&next.nonce));
    }
    assert_eq!(technologies(&session, "b"), before + 1);
    assert!(session.was_answered(&pid("b"), &asked.nonce));
    assert_eq!(secondary_slot(&session), None);
}

// Over sockets: two tabs of "b" (web2) and an old page (web, no staged work).

fn text(message: &ClientMessage) -> Message {
    Message::Text(serde_json::to_string(message).unwrap().into())
}

async fn connect(url: &str, game_id: &str, credential: &str, staged: bool) -> Socket {
    let (mut socket, _) = tokio_tungstenite::connect_async(url).await.unwrap();
    socket
        .send(text(&ClientMessage::Subscribe {
            protocol_version: PROTOCOL_VERSION,
            game_id: game_id.to_owned(),
            player_session: Some(credential.to_owned()),
            staged,
        }))
        .await
        .unwrap();
    next_message(&mut socket, |message| {
        matches!(message, ServerMessage::InitialSnapshot(_))
    })
    .await;
    socket
}

async fn staged(socket: &mut Socket) -> SeatStagedMsg {
    let ServerMessage::SeatStaged(message) = next_message(socket, |message| {
        matches!(message, ServerMessage::SeatStaged(_))
    })
    .await
    else {
        unreachable!()
    };
    message
}

async fn put(socket: &mut Socket, game_id: &str, base_revision: u64, value: serde_json::Value) {
    socket
        .send(text(&ClientMessage::PutStaged {
            protocol_version: PROTOCOL_VERSION,
            game_id: game_id.to_owned(),
            slot: StagedSlot::Secondary,
            base_revision,
            value: Some(value),
        }))
        .await
        .unwrap();
}

async fn answer(socket: &mut Socket, game_id: &str, offer: &LiveOffer, option_id: &str) {
    socket
        .send(text(&ClientMessage::SubmitChoice {
            protocol_version: PROTOCOL_VERSION,
            game_id: game_id.to_owned(),
            expected_version: offer.game_version,
            nonce: offer.nonce.clone(),
            option_id: option_id.to_owned(),
        }))
        .await
        .unwrap();
}

async fn rejection(socket: &mut Socket) -> RejectionReason {
    let ServerMessage::ActionRejected(rejected) = next_message(socket, |message| {
        matches!(message, ServerMessage::ActionRejected(_))
    })
    .await
    else {
        unreachable!()
    };
    rejected.reason
}

/// A game of "a", "b" and "c" on a server; the credential of "b".
async fn served(game_id: &str) -> (Arc<GameSession>, String, String) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("a port");
    let address = listener.local_addr().expect("an address");
    let registry = Arc::new(GameRegistry::new());
    let session = registry
        .create_game(config_playing(game_id, "Technology", None))
        .expect("a game");
    let credential = session.seat_tokens()[&pid("b")].clone();
    let app = ti4_server::create_app(registry.clone());
    tokio::spawn(async move { axum::serve(listener, app).await.expect("the server runs") });
    (
        session,
        credential,
        format!("ws://{address}/ws/games/{game_id}"),
    )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn two_tabs_mark_a_secondary_ready_and_the_server_sends_it_once() {
    let game_id = "ready_two_tabs";
    let (session, credential, url) = served(game_id).await;
    let mut one = connect(&url, game_id, &credential, true).await;
    staged(&mut one).await;
    let mut two = connect(&url, game_id, &credential, true).await;
    staged(&mut two).await;
    let before = technologies(&session, "b");
    let primary = {
        let session = session.clone();
        tokio::task::spawn_blocking(move || play_technology(&session))
            .await
            .unwrap()
    };
    let answers = follow(&session);

    // Both tabs mark the draft ready, on the revision that each was told.
    put(&mut one, game_id, 0, draft_value(&session, &answers, false)).await;
    let revision = staged(&mut two).await.slots[&StagedSlot::Secondary].revision;
    staged(&mut one).await;
    let ready = draft_value(&session, &answers, true);
    put(&mut one, game_id, revision, ready.clone()).await;
    put(&mut two, game_id, revision, ready).await;

    // The game asks "b": the server sends the draft, once, and "c" is asked next.
    let next = {
        let session = session.clone();
        tokio::task::spawn_blocking(move || resolve_primary(&session, &primary))
            .await
            .unwrap()
    };
    assert!(is_secondary(&next.choice));
    assert_eq!(technologies(&session, "b"), before + 1);
    // Every tab is told that the draft is spent: the server clears it.
    for socket in [&mut one, &mut two] {
        let deadline = Instant::now() + DEADLINE;
        loop {
            assert!(Instant::now() < deadline, "the draft is not cleared");
            let told = staged(socket).await;
            if told.writer.is_none() && told.slots[&StagedSlot::Secondary].value.is_none() {
                break;
            }
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_second_answer_of_a_seat_is_answered_elsewhere_for_web2_and_stale_for_web() {
    let game_id = "answered_elsewhere";
    let (session, credential, url) = served(game_id).await;
    let mut one = connect(&url, game_id, &credential, true).await;
    staged(&mut one).await;
    let mut two = connect(&url, game_id, &credential, true).await;
    staged(&mut two).await;
    let mut old_page = connect(&url, game_id, &credential, false).await;
    let asked = {
        let session = session.clone();
        tokio::task::spawn_blocking(move || {
            let primary = play_technology(&session);
            let technology = research(&primary.choice).unwrap();
            answer_until(&session, &primary, &technology, |choice| {
                choice.player == pid("b")
            })
        })
        .await
        .unwrap()
    };

    answer(&mut one, game_id, &asked, "no").await;
    next_message(&mut one, |message| {
        matches!(message, ServerMessage::ActionAccepted(_))
    })
    .await;
    answer(&mut two, game_id, &asked, "no").await;
    assert_eq!(
        rejection(&mut two).await,
        RejectionReason::AnsweredElsewhere
    );
    answer(&mut old_page, game_id, &asked, "no").await;
    assert_eq!(rejection(&mut old_page).await, RejectionReason::StaleNonce);
    // The tab that answered, again: its own answer is no answer of another tab.
    answer(&mut one, game_id, &asked, "no").await;
    assert_ne!(
        rejection(&mut one).await,
        RejectionReason::AnsweredElsewhere
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_tab_that_marks_a_secondary_ready_while_the_seat_is_asked_has_it_sent() {
    let game_id = "ready_while_asked";
    let (session, credential, url) = served(game_id).await;
    let mut tab = connect(&url, game_id, &credential, true).await;
    staged(&mut tab).await;
    let before = technologies(&session, "b");
    let (asked, answers) = {
        let session = session.clone();
        tokio::task::spawn_blocking(move || {
            let primary = play_technology(&session);
            let answers = follow(&session);
            let technology = research(&primary.choice).unwrap();
            let asked = answer_until(&session, &primary, &technology, |choice| {
                choice.player == pid("b")
            });
            (asked, answers)
        })
        .await
        .unwrap()
    };
    put(&mut tab, game_id, 0, draft_value(&session, &answers, true)).await;
    let next = {
        let session = session.clone();
        tokio::task::spawn_blocking(move || {
            let mut next = live_offer(&session, Some(&asked.nonce));
            let deadline = Instant::now() + DEADLINE;
            while next.choice.player != pid("c") {
                assert!(Instant::now() < deadline, "left with {:?}", next.choice);
                next = live_offer(&session, Some(&next.nonce));
            }
            next
        })
        .await
        .unwrap()
    };
    assert!(is_secondary(&next.choice));
    assert_eq!(technologies(&session, "b"), before + 1);
}
