//! A follower's secondary, previewed on a disposable fork before the live window reaches them.

use std::time::Duration;

use ti4_content::ContentStore;
use ti4_engine::fixtures::{game, hub_with_centre, put_on_planet};
use ti4_engine::game::Game;
use ti4_model::content_types::POK;
use ti4_model::id::{ActionCardId, PlanetId, PlayerId, StrategyCardId, SystemId};
use ti4_model::state::{GameState, Phase};
use ti4_server::planning::runner::{
    ASSUMPTION, PlanScope, PlanningEnvelope, PlanningRunner, PlanningUpdate, PlayerPlan,
    SECONDARY_ASSUMPTION, SafePublication, StopReason,
};

const DEADLINE: Duration = Duration::from_secs(5);

fn card(state: &GameState, wanted: &str) -> StrategyCardId {
    state
        .players
        .iter()
        .flat_map(|seat| seat.strategy_cards.iter())
        .chain(state.unclaimed_strategy_cards.iter())
        .find(|card| {
            ti4_engine::strategy_cards::card_name(ContentStore::embedded(), card.as_str())
                .as_deref()
                == Some(wanted)
        })
        .cloned()
        .expect("the strategy deck carries this card")
}

/// "a" is the active player and plays `wanted`; "b" can afford any secondary.
fn checkpoint(wanted: &str) -> (Game<'static>, StrategyCardId) {
    let mut state = game(&["a", "b", "c"]);
    state.phase = Phase::Action;
    state.active = Some(PlayerId::new("a"));
    let follower = state.player_mut(&PlayerId::new("b")).unwrap();
    follower.strategic_tokens = 2;
    follower.trade_goods = 6;
    let card = card(&state, wanted);
    (Game::new(state, ContentStore::embedded()), card)
}

fn runner(game: &Game<'static>, card: &StrategyCardId) -> PlanningRunner {
    PlanningRunner::start_scoped(
        game,
        PlayerId::new("b"),
        PlanScope::Secondary {
            primary: PlayerId::new("a"),
            card: card.clone(),
        },
        PlayerPlan::new(10),
        8,
    )
}

/// Answers each waiting offer with `pick`; replayed offers need none.
fn collect(
    runner: &PlanningRunner,
    mut pick: impl FnMut(&SafePublication) -> Option<String>,
) -> Vec<PlanningEnvelope> {
    let mut transcript = Vec::new();
    loop {
        let envelope = runner
            .recv_timeout(DEADLINE)
            .expect("worker publishes before deadline");
        assert_eq!(envelope.assumptions, vec![ASSUMPTION, SECONDARY_ASSUMPTION]);
        if let PlanningUpdate::SafeOffer(offer) = &envelope.update
            && envelope.awaiting_answer
        {
            let answer = pick(offer).expect("an answer for the waiting offer");
            runner.submit(envelope.identity, &answer).unwrap();
        }
        let terminal = matches!(
            envelope.update,
            PlanningUpdate::Stopped { .. } | PlanningUpdate::Failed(_)
        );
        transcript.push(envelope);
        if terminal {
            return transcript;
        }
    }
}

fn stopped(transcript: &[PlanningEnvelope]) -> (StopReason, &SafePublication) {
    match &transcript.last().unwrap().update {
        PlanningUpdate::Stopped {
            reason,
            last_safe_publication: Some(publication),
        } => (*reason, publication),
        other => panic!("expected a stop with a publication, got {other:?}"),
    }
}

/// "yes" to the window, then the first technology on offer.
fn follow_and_research(offer: &SafePublication) -> Option<String> {
    let choice = offer.choice.as_ref()?;
    if choice.details.get("kind").and_then(|kind| kind.as_str()) == Some("strategy_secondary") {
        return Some("yes".into());
    }
    choice
        .options
        .iter()
        .find(|option| option.kind == ti4_engine::strategy_cards::RESEARCH_KIND)
        .map(|option| option.id.clone())
}

#[test]
fn a_follower_drafts_the_technology_secondary_before_the_primary_resolves() {
    let (live, card) = checkpoint("Technology");
    let before = live.state.clone();
    let mut runner = runner(&live, &card);
    let transcript = collect(&runner, follow_and_research);
    let (reason, publication) = stopped(&transcript);
    assert_eq!(reason, StopReason::SecondaryComplete);

    let plan = runner.plan();
    assert_eq!(plan.recorded_decisions.len(), 2);
    assert_eq!(plan.recorded_decisions[0].option_id, "yes");
    assert!(
        plan.recorded_decisions
            .iter()
            .all(|decision| decision.player == PlayerId::new("b"))
    );
    let seat = |publication: &SafePublication, id: &str| {
        publication
            .position
            .players
            .iter()
            .find(|player| player.id == PlayerId::new(id))
            .cloned()
            .unwrap()
    };
    let follower = seat(publication, "b");
    let known = before.player(&PlayerId::new("b")).unwrap();
    assert_eq!(follower.technologies.len(), known.technologies.len() + 1);
    assert!(
        follower
            .technologies
            .iter()
            .any(|technology| technology.as_str() == plan.recorded_decisions[1].option_id)
    );
    // The preview is still the primary player's turn, and nobody else moved.
    assert_eq!(publication.position.active_player, Some(PlayerId::new("a")));
    assert_eq!(
        seat(publication, "c").technologies,
        before.player(&PlayerId::new("c")).unwrap().technologies
    );
    assert!(live.state.identical(&before), "the live game is untouched");

    // A newer checkpoint replays the same script without asking again.
    runner.refresh(&live, 11);
    let replayed = collect(&runner, |_| None);
    assert_eq!(stopped(&replayed).0, StopReason::SecondaryComplete);
    assert_eq!(replayed.last().unwrap().progress.replayed, 2);
    assert_eq!(runner.plan().recorded_decisions, plan.recorded_decisions);
}

#[test]
fn declining_is_a_complete_one_answer_draft() {
    let (live, card) = checkpoint("Technology");
    let runner = runner(&live, &card);
    let transcript = collect(&runner, |_| Some("no".into()));
    assert_eq!(stopped(&transcript).0, StopReason::SecondaryComplete);
    let plan = runner.plan();
    assert_eq!(plan.recorded_decisions.len(), 1);
    assert_eq!(plan.recorded_decisions[0].option_id, "no");
}

#[test]
fn following_a_draw_is_recorded_but_its_cards_are_never_previewed() {
    let published = |reverse: bool| {
        let (mut live, card) = checkpoint("Politics");
        if reverse {
            live.state.action_card_deck.reverse();
        }
        let runner = runner(&live, &card);
        let transcript = collect(&runner, |_| Some("yes".into()));
        // An ordered draw uses no dice. The knowledge check refuses the position
        // that would show the new cards, so the last publication is the offer.
        let (reason, publication) = stopped(&transcript);
        assert_eq!(reason, StopReason::KnowledgeChanged);
        assert!(publication.choice.is_some());
        assert_eq!(runner.plan().recorded_decisions.len(), 1);
        // Identities differ per attempt; what the player is shown must not.
        transcript
            .into_iter()
            .map(|envelope| serde_json::to_value(envelope.update).unwrap())
            .collect::<Vec<_>>()
    };
    assert_eq!(published(false), published(true));
}

#[test]
fn another_seat_or_card_cannot_stand_in_for_the_announced_secondary() {
    let (live, card) = checkpoint("Technology");
    // The primary player has no secondary of their own card.
    let own = PlanningRunner::start_scoped(
        &live,
        PlayerId::new("a"),
        PlanScope::Secondary {
            primary: PlayerId::new("a"),
            card,
        },
        PlayerPlan::new(10),
        8,
    );
    let envelope = own.recv_timeout(DEADLINE).unwrap();
    assert!(matches!(envelope.update, PlanningUpdate::Failed(_)));
}

/// As `checkpoint`, with "b" at home on system 35: both planets controlled, a
/// space dock on each, and the first planet exhausted.
fn settled(wanted: &str) -> (Game<'static>, StrategyCardId, Vec<PlanetId>) {
    let content = ContentStore::embedded();
    let hub = hub_with_centre("35");
    let system = SystemId::new(&hub.centre);
    let planets: Vec<_> = ti4_content::galaxy::system(content, "35", POK)
        .unwrap()
        .planets()
        .iter()
        .map(|id| PlanetId::new(*id))
        .collect();
    let player = PlayerId::new("b");
    let mut state = game(&["a", "b", "c"]);
    state.phase = Phase::Action;
    state.active = Some(PlayerId::new("a"));
    for planet in &planets {
        state
            .system_mut(&system)
            .set_control(planet.clone(), player.clone());
        put_on_planet(&mut state, &system, planet, "spacedock", &player, 1);
    }
    state.exhaust_planet(planets[0].clone());
    let follower = state.player_mut(&player).unwrap();
    follower.home_system = Some(system);
    follower.strategic_tokens = 2;
    follower.trade_goods = 6;
    let card = card(&state, wanted);
    (
        Game::new(state, content).with_galaxy(hub.galaxy),
        card,
        planets,
    )
}

fn is_window(offer: &SafePublication) -> bool {
    offer
        .choice
        .as_ref()
        .and_then(|choice| choice.details.get("kind"))
        .and_then(|kind| kind.as_str())
        == Some("strategy_secondary")
}

/// Answers in order; the draft must ask exactly these questions.
fn scripted(answers: &[&str]) -> impl FnMut(&SafePublication) -> Option<String> {
    let mut answers = answers
        .iter()
        .map(|answer| (*answer).to_owned())
        .collect::<Vec<_>>()
        .into_iter();
    move |_| answers.next()
}

fn recorded(runner: &PlanningRunner) -> Vec<String> {
    runner
        .plan()
        .recorded_decisions
        .iter()
        .map(|decision| decision.option_id.clone())
        .collect()
}

fn follower(publication: &SafePublication) -> ti4_server::protocol::view::PlayerView {
    publication
        .position
        .players
        .iter()
        .find(|player| player.id == PlayerId::new("b"))
        .cloned()
        .unwrap()
}

#[test]
fn leadership_drafts_the_purchase_and_the_pool_it_goes_to() {
    let (live, card) = checkpoint("Leadership");
    let before = live.state.clone();
    let runner = runner(&live, &card);
    // Buy one token, put it in the fleet pool, then stop buying.
    let transcript = collect(&runner, scripted(&["yes", "fleet_tokens", "no"]));
    let (reason, publication) = stopped(&transcript);
    assert_eq!(reason, StopReason::SecondaryComplete);
    assert_eq!(recorded(&runner), ["yes", "fleet_tokens", "no"]);
    let known = before.player(&PlayerId::new("b")).unwrap();
    assert_eq!(follower(publication).fleet_tokens, known.fleet_tokens + 1);
    assert!(live.state.identical(&before), "the live game is untouched");
}

#[test]
fn diplomacy_drafts_which_planets_to_ready() {
    let (live, card, planets) = settled("Diplomacy");
    let before = live.state.clone();
    let runner = runner(&live, &card);
    let transcript = collect(&runner, scripted(&["yes", planets[0].as_str()]));
    let (reason, publication) = stopped(&transcript);
    assert_eq!(reason, StopReason::SecondaryComplete);
    assert_eq!(recorded(&runner), ["yes", planets[0].as_str()]);
    assert!(
        !publication.position.board.systems[&SystemId::new("35")].planets[&planets[0]].exhausted,
        "the preview shows the planet readied"
    );
    assert!(live.state.identical(&before));
}

#[test]
fn construction_drafts_where_the_structure_goes() {
    let (live, card, planets) = settled("Construction");
    let before = live.state.clone();
    let runner = runner(&live, &card);
    let build = format!("pds|35|{}", planets[1]);
    let transcript = collect(&runner, scripted(&["yes", &build]));
    let (reason, publication) = stopped(&transcript);
    assert_eq!(reason, StopReason::SecondaryComplete);
    assert_eq!(recorded(&runner), ["yes", build.as_str()]);
    assert!(
        publication.position.board.systems[&SystemId::new("35")]
            .units
            .iter()
            .any(|unit| unit.unit_type.as_str() == "pds"
                && unit.planet.as_ref() == Some(&planets[1]))
    );
    assert!(live.state.identical(&before));
}

#[test]
fn warfare_drafts_home_production_with_its_payment() {
    // What the follower is shown must not depend on anything they cannot see.
    let published = |hand: &str| {
        let (mut live, card, _) = settled("Warfare");
        live.state
            .player_mut(&PlayerId::new("a"))
            .unwrap()
            .action_cards = vec![ActionCardId::new(hand)];
        let before = live.state.clone();
        let runner = runner(&live, &card);
        let mut built = false;
        let transcript = collect(&runner, |offer| {
            let choice = offer.choice.as_ref()?;
            let subtype = choice
                .context
                .as_ref()
                .map(|context| context.subtype.as_str());
            Some(match subtype {
                _ if is_window(offer) => "yes".to_owned(),
                Some("produce_unit") if !built => {
                    built = true;
                    "build|infantry|2".to_owned()
                }
                Some("produce_unit") => "done_producing".to_owned(),
                Some("pay_resources") => "trade_good".to_owned(),
                // Where the new infantry stand.
                _ => choice.options[0].id.clone(),
            })
        });
        let (reason, publication) = stopped(&transcript);
        assert_eq!(reason, StopReason::SecondaryComplete);
        let answers = recorded(&runner);
        assert_eq!(answers[..2], ["yes", "build|infantry|2"]);
        assert!(answers.contains(&"trade_good".to_owned()));
        assert_eq!(answers.last().unwrap(), "done_producing");
        assert_eq!(follower(publication).trade_goods, 5);
        assert_eq!(
            publication.position.board.systems[&SystemId::new("35")]
                .units
                .iter()
                .filter(|unit| unit.unit_type.as_str() == "infantry")
                .count(),
            2
        );
        assert!(live.state.identical(&before));
        let text = serde_json::to_string(&transcript).unwrap();
        assert!(!text.contains(hand));
        transcript
            .into_iter()
            .map(|envelope| serde_json::to_value(envelope.update).unwrap())
            .collect::<Vec<_>>()
    };
    assert_eq!(published("flank_speed"), published("sabotage"));
}

#[test]
fn a_question_for_another_card_is_never_shown() {
    // The fork is asked to preview Warfare while the plan was scoped to Diplomacy.
    let (live, _, _) = settled("Warfare");
    let other = card(&live.state, "Diplomacy");
    let runner = runner(&live, &other);
    let transcript = collect(&runner, |offer| is_window(offer).then(|| "no".to_owned()));
    for envelope in &transcript {
        if let PlanningUpdate::SafeOffer(offer) = &envelope.update {
            let choice = offer.choice.as_ref().unwrap();
            assert_eq!(
                choice.details.get("card").and_then(|card| card.as_str()),
                Some(other.as_str())
            );
        }
    }
}
