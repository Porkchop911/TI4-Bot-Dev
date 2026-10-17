//! The draft of a secondary as web2 keeps it: a script that is run on a copy of the game and
//! sent to the live game when it asks the seat (`RunDraft` with a scope, `ApplyDraft`). The
//! helpers play a strategic action to the window of its followers, for `ready_secondary` too.

use std::time::{Duration, Instant};

use ti4_content::ContentStore;
use ti4_engine::choice::Choice;
use ti4_engine::fixtures::{game, hub_with_centre, put_on_planet};
use ti4_engine::strategy_cards::RESEARCH_KIND;
use ti4_host::protocol::status::ViewerRole;
use ti4_model::content_types::POK;
use ti4_model::id::{PlanetId, PlayerId, SystemId};
use ti4_model::state::Phase;
use ti4_server::session::{GameSession, SeatController, SessionConfig};

const DEADLINE: Duration = Duration::from_secs(5);

fn pid(id: &str) -> PlayerId {
    PlayerId::new(id)
}

/// "a" is about to play Technology; "b" then "c" may follow and can afford to.
fn session(name: &str) -> GameSession {
    session_playing(name, "Technology", None)
}

/// "a" is about to play `wanted`. The `settled` seat is at home on system 35:
/// both planets controlled, a space dock on each, the first planet exhausted.
fn session_playing(name: &str, wanted: &str, settled: Option<&str>) -> GameSession {
    GameSession::start(config_playing(name, wanted, settled))
}

/// The game of [`session_playing`], to start.
pub(crate) fn config_playing(name: &str, wanted: &str, settled: Option<&str>) -> SessionConfig {
    let content = ContentStore::embedded();
    let mut state = game(&["a", "b", "c"]);
    state.phase = Phase::Action;
    state.active = Some(pid("a"));
    let card = state
        .unclaimed_strategy_cards
        .iter()
        .find(|card| {
            ti4_engine::strategy_cards::card_name(content, card.as_str()).as_deref() == Some(wanted)
        })
        .cloned()
        .expect("the mat carries this card");
    state.unclaimed_strategy_cards.retain(|c| c != &card);
    assert!(state.deal_strategy_card(&pid("a"), card));
    for follower in ["b", "c"] {
        let seat = state.player_mut(&pid(follower)).unwrap();
        seat.strategic_tokens = 2;
        seat.trade_goods = 6;
    }
    let hub = hub_with_centre("35");
    if let Some(settler) = settled {
        let system = SystemId::new(&hub.centre);
        let planets: Vec<_> = ti4_content::galaxy::system(content, "35", POK)
            .unwrap()
            .planets()
            .iter()
            .map(|id| PlanetId::new(*id))
            .collect();
        for planet in &planets {
            state
                .system_mut(&system)
                .set_control(planet.clone(), pid(settler));
            put_on_planet(&mut state, &system, planet, "spacedock", &pid(settler), 1);
        }
        state.exhaust_planet(planets[0].clone());
        state.player_mut(&pid(settler)).unwrap().home_system = Some(system);
    }
    let mut config =
        SessionConfig::new(name, state).with_player_ids(vec![pid("a"), pid("b"), pid("c")]);
    if settled.is_some() {
        config = config.with_galaxy(hub.galaxy, Vec::new());
    }
    for seat in ["a", "b", "c"] {
        config = config.with_seat(pid(seat), SeatController::Human);
    }
    config
}

pub(crate) struct LiveOffer {
    pub(crate) choice: Choice,
    pub(crate) nonce: String,
    pub(crate) game_version: u64,
}

pub(crate) fn live_offer(session: &GameSession, previous_nonce: Option<&str>) -> LiveOffer {
    let deadline = Instant::now() + DEADLINE;
    loop {
        assert!(session.error().is_none(), "{:?}", session.error());
        // Only an open question: one that is reserved (a ready draft checked) may not be asked.
        if let Some((seat, open, _)) = session.open_pending_decision() {
            let snapshot = session.get_snapshot(&ViewerRole::Player(seat));
            if let Some(pending) = snapshot.pending_choice
                && pending.nonce == open
                && previous_nonce != Some(pending.nonce.as_str())
            {
                return LiveOffer {
                    choice: pending.choice,
                    nonce: pending.nonce,
                    game_version: snapshot.game_version,
                };
            }
        }
        assert!(Instant::now() < deadline, "live offer deadline");
        std::thread::sleep(Duration::from_millis(5));
    }
}

pub(crate) fn answer_live(session: &GameSession, offer: &LiveOffer, option: &str) -> LiveOffer {
    session
        .submit_choice(
            &offer.choice.player,
            &offer.nonce,
            offer.game_version,
            option,
        )
        .unwrap();
    live_offer(session, Some(&offer.nonce))
}

/// Submit `option`, then wait for the live offer `wanted` describes. A question
/// answered from a draft is pending only while its step runs: it is never
/// broadcast and is gone again without anybody submitting to it.
pub(crate) fn answer_until(
    session: &GameSession,
    offer: &LiveOffer,
    option: &str,
    wanted: impl Fn(&Choice) -> bool,
) -> LiveOffer {
    let mut next = answer_live(session, offer, option);
    let deadline = Instant::now() + DEADLINE;
    while !wanted(&next.choice) {
        assert!(Instant::now() < deadline, "left with {:?}", next.choice);
        next = live_offer(session, Some(&next.nonce));
    }
    next
}

pub(crate) fn is_secondary(choice: &Choice) -> bool {
    choice.details.get("kind").and_then(|kind| kind.as_str()) == Some("strategy_secondary")
}

pub(crate) fn research(choice: &Choice) -> Option<String> {
    choice
        .options
        .iter()
        .find(|option| option.kind == RESEARCH_KIND)
        .map(|option| option.id.clone())
}

/// "a" plays Technology and is left answering its primary.
pub(crate) fn play_technology(session: &GameSession) -> LiveOffer {
    let menu = live_offer(session, None);
    assert_eq!(menu.choice.player, pid("a"));
    assert!(
        session
            .get_snapshot(&ViewerRole::Player(pid("b")))
            .secondary
            .is_none()
    );
    let primary = answer_live(session, &menu, "strategic");
    assert_eq!(primary.choice.player, pid("a"));
    assert!(research(&primary.choice).is_some(), "{:?}", primary.choice);
    primary
}

pub(crate) fn follow_and_research(choice: &Choice) -> String {
    if is_secondary(choice) {
        "yes".into()
    } else {
        research(choice).expect("a technology to research")
    }
}

pub(crate) fn technologies(session: &GameSession, player: &str) -> usize {
    session
        .get_snapshot(&ViewerRole::Spectator)
        .state
        .player(&pid(player))
        .unwrap()
        .technologies
        .len()
}

/// web2 keeps the draft of a secondary as a script (`RunDraft` with a scope, `ApplyDraft`), and
/// the server keeps nothing of it. The update names the action from the moment the card is
/// played.
#[test]
fn a_script_drafts_a_secondary_while_the_primary_is_resolved_and_answers_the_seat() {
    use ti4_host::planning::audit::PlanScope;
    use ti4_host::planning::draft::{DraftStep, DraftStop};
    use ti4_host::view::secondary::FollowerState;

    let session = session("secondary_script");
    let b = pid("b");
    assert!(
        session
            .get_snapshot(&ViewerRole::Player(b.clone()))
            .secondary
            .is_none()
    );
    let before = technologies(&session, "b");
    let primary = play_technology(&session);

    // "a" still chooses the technology of the primary: the followers are told of the card.
    let facts = |seat: &str| {
        session
            .get_snapshot(&ViewerRole::Player(pid(seat)))
            .secondary
            .expect("the strategic action")
    };
    let told = facts("b");
    assert_eq!(told.primary, pid("a"));
    assert_eq!(told.name, "Technology");
    assert!(!told.open && told.draft);
    let seats: Vec<_> = told.followers.iter().map(|f| f.seat.clone()).collect();
    assert_eq!(seats, [pid("b"), pid("c")]);
    assert!(facts("c").draft);
    assert!(!facts("a").draft, "the primary player has no secondary");

    let scope = PlanScope::Secondary {
        primary: pid("a"),
        card: told.card.clone(),
    };
    let run = |script: &[DraftStep]| session.run_draft(&b, &scope, script).1;
    let asked = run(&[]);
    assert_eq!(asked.stop, DraftStop::Open);
    let question = asked.update.pending_choice.expect("the question").choice;
    assert!(is_secondary(&question));
    // The seat follows and researches, on the copy only.
    let mut script = vec![DraftStep::choose(follow_and_research(&question))];
    let next = run(&script);
    assert_eq!(next.stop, DraftStop::Open);
    let wanted = research(&next.update.pending_choice.expect("the research").choice)
        .expect("a technology to research");
    script.push(DraftStep::choose(&wanted));
    let done = run(&script);
    assert_eq!(done.stop, DraftStop::Complete);
    assert_eq!(done.consumed, 2);
    assert_eq!(technologies(&session, "b"), before);
    // A secondary of another action is not drafted, and "a" drafts none of its own card.
    let other = PlanScope::Secondary {
        primary: pid("c"),
        card: told.card.clone(),
    };
    assert!(matches!(
        session.run_draft(&b, &other, &[]).1.stop,
        DraftStop::Failed { .. }
    ));
    assert!(matches!(
        session.run_draft(&pid("a"), &scope, &[]).1.stop,
        DraftStop::Failed { .. }
    ));

    // The primary is resolved: the game asks "b", and the script answers the whole secondary.
    let technology = research(&primary.choice).unwrap();
    let reached = answer_until(&session, &primary, &technology, is_secondary);
    assert_eq!(reached.choice.player, b);
    assert!(facts("b").open);
    assert_eq!(run(&script).stop, DraftStop::Complete);
    let stopped = session
        .apply_draft(&b, &reached.nonce, reached.game_version, &script)
        .expect("the script is taken");
    assert_eq!(stopped, None);
    // The questions of the secondary are pending only while the script answers them.
    let mut after = live_offer(&session, Some(&reached.nonce));
    let deadline = Instant::now() + DEADLINE;
    while after.choice.player != pid("c") {
        assert!(Instant::now() < deadline, "left with {:?}", after.choice);
        after = live_offer(&session, Some(&after.nonce));
    }
    assert_eq!(technologies(&session, "b"), before + 1);
    let now = facts("b");
    assert_eq!(now.followers[0].state, FollowerState::Followed);
    assert!(!now.draft && facts("c").draft);
}
