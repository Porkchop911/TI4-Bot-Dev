//! Followers draft a secondary at the same time; the live window still asks them in turn.

use std::time::{Duration, Instant};

use ti4_content::ContentStore;
use ti4_engine::choice::Choice;
use ti4_engine::fixtures::{game, hub_with_centre, put_on_planet};
use ti4_engine::strategy_cards::RESEARCH_KIND;
use ti4_model::content_types::POK;
use ti4_model::id::{PlanetId, PlayerId, SystemId};
use ti4_model::state::Phase;
use ti4_server::planning::runner::{PlanningEnvelope, PlanningUpdate, StopReason};
use ti4_server::protocol::client::SecondaryPlanningRequest as Request;
use ti4_server::protocol::status::ViewerRole;
use ti4_server::session::{GameSession, PlanningError, SeatController, SessionConfig};

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
        if let Some((seat, _, _)) = session.current_pending_decision() {
            let snapshot = session.get_snapshot(&ViewerRole::Player(seat));
            if let Some(pending) = snapshot.pending_choice
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

fn is_action_menu(choice: &Choice) -> bool {
    choice
        .context
        .as_ref()
        .is_some_and(|context| context.subtype == "action_menu")
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
    assert!(session.planning_status(&pid("b")).secondary.is_none());
    let primary = answer_live(session, &menu, "strategic");
    assert_eq!(primary.choice.player, pid("a"));
    assert!(research(&primary.choice).is_some(), "{:?}", primary.choice);
    primary
}

/// Start `player`'s secondary draft and answer each offer until it stops.
fn draft(
    session: &GameSession,
    player: &PlayerId,
    mut pick: impl FnMut(&Choice) -> String,
) -> PlanningEnvelope {
    session
        .secondary_planning(player, &Request::Start {})
        .unwrap();
    loop {
        let envelope = session
            .recv_secondary_planning_timeout(player, DEADLINE)
            .unwrap();
        match &envelope.update {
            PlanningUpdate::SafeOffer(offer) if envelope.awaiting_answer => {
                let option_id = pick(offer.choice.as_ref().unwrap());
                session
                    .secondary_planning(
                        player,
                        &Request::Answer {
                            identity: envelope.identity,
                            option_id,
                        },
                    )
                    .unwrap();
            }
            PlanningUpdate::Stopped { reason, .. } => {
                assert_eq!(*reason, StopReason::SecondaryComplete);
                return envelope;
            }
            PlanningUpdate::Failed(category) => panic!("draft failed: {category:?}"),
            _ => {}
        }
    }
}

fn ready(session: &GameSession, player: &PlayerId) {
    let identity = session
        .planning_status(player)
        .secondary
        .unwrap()
        .identity
        .unwrap();
    session
        .secondary_planning(
            player,
            &Request::SetReady {
                identity,
                ready: true,
            },
        )
        .unwrap();
    assert!(session.planning_status(player).secondary.unwrap().ready);
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

#[test]
fn ready_followers_are_submitted_in_turn_without_being_asked() {
    let session = session("secondary_ready");
    let before = (technologies(&session, "b"), technologies(&session, "c"));
    let primary = play_technology(&session);

    // Both followers draft while "a" is still choosing the primary's technology.
    for follower in ["b", "c"] {
        let status = session.planning_status(&pid(follower)).secondary.unwrap();
        assert_eq!(status.played_by, pid("a"));
        assert!(!status.window_open && status.can_start && !status.has_draft);
    }
    assert!(session.planning_status(&pid("a")).secondary.is_none());
    assert_eq!(
        session.secondary_planning(&pid("a"), &Request::Start {}),
        Err(PlanningError::Unavailable),
        "the primary player has no secondary to draft"
    );
    let drafted = draft(&session, &pid("b"), follow_and_research);
    assert_eq!(drafted.recorded_decisions.len(), 2);
    let wanted = drafted.recorded_decisions[1].option_id.clone();
    draft(&session, &pid("c"), |_| "no".into());
    ready(&session, &pid("b"));
    ready(&session, &pid("c"));
    assert_eq!(
        technologies(&session, "b"),
        before.0,
        "a draft changes nothing live"
    );

    // The primary resolves; the window then reaches "b" and "c" and is answered for them.
    let technology = research(&primary.choice).unwrap();
    // Nobody is prompted: the next question anybody has to answer is the next turn's.
    let next = answer_until(&session, &primary, &technology, is_action_menu);
    assert_ne!(next.choice.player, pid("a"));
    assert_eq!(technologies(&session, "b"), before.0 + 1);
    assert_eq!(technologies(&session, "c"), before.1);
    let state = session.get_snapshot(&ViewerRole::Spectator).state;
    assert!(
        state
            .player(&pid("b"))
            .unwrap()
            .technologies
            .iter()
            .any(|technology| technology.as_str() == wanted)
    );
    assert_eq!(state.player(&pid("b")).unwrap().strategic_tokens, 1);
    assert_eq!(state.player(&pid("c")).unwrap().strategic_tokens, 2);
    let log = session.decision_log();
    let answers: Vec<(&str, &str)> = log
        .iter()
        .rev()
        .take(3)
        .rev()
        .map(|record| (record.player.as_str(), record.chosen.as_str()))
        .collect();
    assert_eq!(
        answers,
        vec![("b", "yes"), ("b", wanted.as_str()), ("c", "no")],
        "the live window still resolved clockwise"
    );
    for follower in ["b", "c"] {
        assert!(session.planning_status(&pid(follower)).secondary.is_none());
    }
}

#[test]
fn a_seat_that_is_not_ready_is_asked_live_and_later_seats_still_apply() {
    let session = session("secondary_mixed");
    let primary = play_technology(&session);
    // "b" drafts but never marks it ready; "c" declines in advance.
    draft(&session, &pid("b"), follow_and_research);
    draft(&session, &pid("c"), |_| "no".into());
    ready(&session, &pid("c"));

    let technology = research(&primary.choice).unwrap();
    let asked = answer_until(&session, &primary, &technology, is_secondary);
    assert_eq!(asked.choice.player, pid("b"));
    let status = session.planning_status(&pid("b")).secondary.unwrap();
    assert!(status.window_open && status.has_draft && !status.ready);
    assert_eq!(status.seats_before, Some(0));
    assert_eq!(
        session
            .planning_status(&pid("c"))
            .secondary
            .unwrap()
            .seats_before,
        Some(1)
    );

    answer_until(&session, &asked, "no", is_action_menu);
    let log = session.decision_log();
    let last: Vec<(&str, &str)> = log
        .iter()
        .rev()
        .take(2)
        .rev()
        .map(|record| (record.player.as_str(), record.chosen.as_str()))
        .collect();
    assert_eq!(last, vec![("b", "no"), ("c", "no")]);
}

#[test]
fn a_new_answer_or_reset_withdraws_readiness() {
    let session = session("secondary_unready");
    let _primary = play_technology(&session);
    let drafted = draft(&session, &pid("b"), |_| "no".into());
    ready(&session, &pid("b"));
    // An empty draft cannot be made ready, and a stale identity is refused.
    session
        .secondary_planning(
            &pid("b"),
            &Request::Reset {
                identity: session
                    .planning_status(&pid("b"))
                    .secondary
                    .unwrap()
                    .identity
                    .unwrap(),
            },
        )
        .unwrap();
    assert!(!session.planning_status(&pid("b")).secondary.unwrap().ready);
    assert!(
        session
            .secondary_planning(
                &pid("b"),
                &Request::SetReady {
                    identity: drafted.identity,
                    ready: true,
                },
            )
            .is_err()
    );
    // A tactical draft is a separate slot: starting one leaves the secondary alone.
    session.start_planning(&pid("b")).unwrap();
    assert!(
        session
            .planning_status(&pid("b"))
            .secondary
            .unwrap()
            .has_draft
    );
}

fn subtype(choice: &Choice) -> Option<&str> {
    choice
        .context
        .as_ref()
        .map(|context| context.subtype.as_str())
}

fn chosen(session: &GameSession, player: &str) -> Vec<String> {
    session
        .decision_log()
        .iter()
        .filter(|record| record.player.as_str() == player)
        .map(|record| record.chosen.to_string())
        .collect()
}

/// "a" plays its card and "b" is being asked the window question live.
/// "c", the next seat, drafts in the meantime.
fn play_until_b_is_asked(session: &GameSession) -> LiveOffer {
    let menu = live_offer(session, None);
    assert_eq!(menu.choice.player, pid("a"));
    let mut offer = answer_live(session, &menu, "strategic");
    let deadline = Instant::now() + DEADLINE;
    while offer.choice.player == pid("a") {
        assert!(Instant::now() < deadline, "left with {:?}", offer.choice);
        let option = offer
            .choice
            .options
            .iter()
            .find(|option| option.is_decline() || option.id == "no")
            .unwrap_or(&offer.choice.options[0])
            .id
            .clone();
        offer = answer_live(session, &offer, &option);
    }
    assert_eq!(offer.choice.player, pid("b"));
    assert!(is_secondary(&offer.choice), "{:?}", offer.choice);
    offer
}

fn recorded(envelope: &PlanningEnvelope) -> Vec<String> {
    envelope
        .recorded_decisions
        .iter()
        .map(|decision| decision.option_id.clone())
        .collect()
}

#[test]
fn a_ready_leadership_draft_buys_its_token_without_prompting() {
    let session = session_playing("secondary_leadership", "Leadership", None);
    let asked = play_until_b_is_asked(&session);
    // Buy one token for the fleet pool, then stop.
    let mut purchases = 0;
    let drafted = draft(&session, &pid("c"), |choice| match subtype(choice) {
        Some("gain_command_token") => "fleet_tokens".into(),
        _ => {
            purchases += 1;
            if purchases == 1 { "yes" } else { "no" }.into()
        }
    });
    assert_eq!(recorded(&drafted), ["yes", "fleet_tokens", "no"]);
    ready(&session, &pid("c"));
    let fleet = |session: &GameSession| {
        session
            .get_snapshot(&ViewerRole::Spectator)
            .state
            .player(&pid("c"))
            .unwrap()
            .fleet_tokens
    };
    let before = fleet(&session);
    // "b" declines live; "c" is then answered from the draft, nested choices included.
    let next = answer_until(&session, &asked, "no", is_action_menu);
    assert_ne!(next.choice.player, pid("a"));
    assert_eq!(fleet(&session), before + 1);
    assert_eq!(chosen(&session, "c"), ["yes", "fleet_tokens", "no"]);
}

#[test]
fn a_warfare_draft_records_exactly_the_answers_the_live_window_accepts() {
    let session = session_playing("secondary_warfare", "Warfare", Some("c"));
    // "b" has nowhere to produce, so the window goes straight to "c".
    let asked = play_until_seat_is_asked(&session, "c");
    let mut built = false;
    let drafted = draft(&session, &pid("c"), |choice| match subtype(choice) {
        _ if is_secondary(choice) => "yes".into(),
        Some("produce_unit") if !built => {
            built = true;
            "build|infantry|2".into()
        }
        Some("produce_unit") => "done_producing".into(),
        Some("pay_resources") => "trade_good".into(),
        _ => choice.options[0].id.clone(),
    });
    let wanted = recorded(&drafted);
    assert!(wanted.len() >= 4, "{wanted:?}");
    // Drafted with the live question already open: the same answers given by
    // hand must be accepted one by one, which is what an applied draft submits.
    let mut offer = asked;
    for answer in &wanted {
        assert_eq!(offer.choice.player, pid("c"));
        assert!(
            offer.choice.option(answer).is_some(),
            "{answer}: {:?}",
            offer.choice
        );
        offer = answer_live(&session, &offer, answer);
    }
    assert!(is_action_menu(&offer.choice), "{:?}", offer.choice);
    let state = session.get_snapshot(&ViewerRole::Spectator).state;
    assert_eq!(state.player(&pid("c")).unwrap().trade_goods, 5);
}

fn play_until_seat_is_asked(session: &GameSession, seat: &str) -> LiveOffer {
    let menu = live_offer(session, None);
    let mut offer = answer_live(session, &menu, "strategic");
    let deadline = Instant::now() + DEADLINE;
    while offer.choice.player != pid(seat) {
        assert!(Instant::now() < deadline, "left with {:?}", offer.choice);
        let option = offer
            .choice
            .options
            .iter()
            .find(|option| option.is_decline() || option.id == "no")
            .unwrap_or(&offer.choice.options[0])
            .id
            .clone();
        offer = answer_live(session, &offer, &option);
    }
    assert!(is_secondary(&offer.choice), "{:?}", offer.choice);
    offer
}

#[test]
fn a_diplomacy_draft_records_exactly_the_answers_the_live_window_accepts() {
    let session = session_playing("secondary_diplomacy", "Diplomacy", Some("c"));
    let asked = play_until_seat_is_asked(&session, "c");
    let drafted = draft(&session, &pid("c"), |choice| {
        if is_secondary(choice) {
            "yes".into()
        } else {
            choice.options[0].id.clone()
        }
    });
    let wanted = recorded(&drafted);
    assert_eq!(wanted.len(), 2);
    // The draft's answers are legal live answers in the same order.
    let readying = answer_live(&session, &asked, &wanted[0]);
    assert_eq!(readying.choice.player, pid("c"));
    assert_eq!(subtype(&readying.choice), Some("ready_planet"));
    assert!(readying.choice.option(&wanted[1]).is_some());
    let next = answer_until(&session, &readying, &wanted[1], is_action_menu);
    assert_ne!(next.choice.player, pid("a"));
}

/// web2 keeps the draft of a secondary as a script (`RunDraft` with a scope, `ApplyDraft`), and
/// the server keeps nothing of it. The update names the action from the moment the card is
/// played.
#[test]
fn a_script_drafts_a_secondary_while_the_primary_is_resolved_and_answers_the_seat() {
    use ti4_view::planning::audit::PlanScope;
    use ti4_view::planning::draft::{DraftStep, DraftStop};
    use ti4_view::secondary::FollowerState;

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
