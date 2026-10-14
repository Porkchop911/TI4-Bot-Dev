use super::*;
use crate::choice::{AlwaysDecline, Capturing};
use ti4_model::content_types::POK;

fn pid(id: &str) -> PlayerId {
    PlayerId::new(id)
}

/// Three seats; `a` holds the named card and is the primary. Every seat has tokens and four
/// trade goods.
fn dealt(primary_card: &str) -> (GameState, StrategyCardId) {
    let content = ContentStore::embedded();
    let mut state =
        crate::fixtures::seated_game(&[("a", "sol"), ("b", "letnev"), ("c", "muaat")], POK);
    let card = state
        .unclaimed_strategy_cards
        .iter()
        .find(|card| {
            crate::strategy_cards::card_name(content, card.as_str()).as_deref()
                == Some(primary_card)
        })
        .cloned()
        .expect("the deck carries this card");
    state.unclaimed_strategy_cards.retain(|c| c != &card);
    assert!(state.deal_strategy_card(&pid("a"), card.clone()));
    state.phase = ti4_model::state::Phase::Action;
    for seat in &mut state.players {
        seat.strategic_tokens = 3;
        seat.trade_goods = 4;
    }
    (state, card)
}

fn preview(state: &GameState, card: &StrategyCardId, answers: &[&str]) -> SecondaryPreview {
    let answers: Vec<String> = answers.iter().map(ToString::to_string).collect();
    preview_secondary(
        state,
        ContentStore::embedded(),
        POK,
        None,
        card,
        &pid("a"),
        &pid("b"),
        &answers,
    )
}

fn strategic() -> ChoiceOption {
    ChoiceOption::labelled("strategic", "action", "go")
}

/// The first question the REAL flow puts to `b` after "yes", through the same window and the
/// same `follow` the game driver uses.
fn real_follow_up(state: &GameState, card: &StrategyCardId) -> Option<Choice> {
    let content = ContentStore::embedded();
    let mut state = state.clone();
    let mut window =
        crate::strategy::begin_strategic_action(&mut state, content, &pid("a"), strategic())
            .expect("a holds the card");
    let first = window
        .next_choice(&mut state, content, POK)
        .expect("a window question");
    assert_eq!(first.player, pid("b"));
    let yes = first
        .options
        .iter()
        .find(|o| o.id == "yes")
        .cloned()
        .unwrap();
    window.take_choice(&mut state, content, POK, yes).unwrap();
    let (decider, seen) = Capturing::new(Box::new(AlwaysDecline));
    let mut table = Table::with_default(Box::new(decider));
    let _ = crate::strategy_cards::follow(
        &mut state,
        content,
        POK,
        None,
        &mut table,
        &pid("b"),
        card.as_str(),
    );
    let seen = seen.borrow();
    seen.first().cloned()
}

#[test]
fn the_window_question_is_the_real_windows_question() {
    let content = ContentStore::embedded();
    for name in [
        "Technology",
        "Warfare",
        "Construction",
        "Politics",
        "Imperial",
        "Trade",
    ] {
        let (mut state, card) = dealt(name);
        state.player_mut(&pid("b")).unwrap().commodities = 0;
        let mut copy = state.clone();
        let mut window =
            crate::strategy::begin_strategic_action(&mut copy, content, &pid("a"), strategic())
                .unwrap();
        let real = window
            .next_choice(&mut copy, content, POK)
            .unwrap_or_else(|| panic!("{name}: b is first"));
        assert_eq!(real.player, pid("b"), "{name}: {:?}", real.prompt);
        let SecondaryPreview::Question { choice, step, .. } = preview(&state, &card, &[]) else {
            panic!("{name}: expected the window question");
        };
        assert_eq!(step, 0);
        assert_eq!(
            choice, real,
            "{name}: the preview IS the real window question"
        );
    }
}

#[test]
fn the_technology_follow_up_is_the_real_one_with_its_payment() {
    let (state, card) = dealt("Technology");
    let real = real_follow_up(&state, &card).expect("b is asked to research");
    assert_eq!(
        real.context.as_ref().unwrap().subtype,
        "research_technology"
    );
    let SecondaryPreview::Question {
        choice,
        step,
        payment,
        skipped,
    } = preview(&state, &card, &["yes"])
    else {
        panic!("expected the research question");
    };
    assert_eq!(step, 1);
    assert!(skipped.is_empty());
    assert_eq!(
        choice, real,
        "same options, payloads and context as the real question"
    );
    let payment = payment.expect("the 4 resources are paid by a plan");
    assert_eq!(payment.cost, 4);
    assert!(payment.worth >= 4);
}

#[test]
fn a_decline_completes_and_a_bad_answer_is_unavailable() {
    let (state, card) = dealt("Technology");
    assert!(matches!(
        preview(&state, &card, &["no"]),
        SecondaryPreview::Complete { .. }
    ));
    assert!(matches!(
        preview(&state, &card, &["maybe"]),
        SecondaryPreview::Unavailable { .. }
    ));
    assert!(matches!(
        preview(&state, &card, &["yes", "not-a-technology"]),
        SecondaryPreview::Rejected { at: 1, .. }
    ));
}

#[test]
fn a_seat_that_would_not_be_asked_gets_the_reason() {
    let (mut state, card) = dealt("Technology");
    state.player_mut(&pid("b")).unwrap().strategic_tokens = 0;
    assert_eq!(
        preview(&state, &card, &[]),
        SecondaryPreview::WouldNotBeAsked {
            blocker: SecondaryBlocker::NoStrategyToken
        }
    );

    let (mut state, card) = dealt("Technology");
    state.player_mut(&pid("b")).unwrap().trade_goods = 0;
    let owned: Vec<_> = state
        .controlled_planets(&pid("b"))
        .into_iter()
        .map(|(_, planet)| planet.clone())
        .collect();
    state.exhausted_planets.extend(owned);
    assert_eq!(
        preview(&state, &card, &[]),
        SecondaryPreview::WouldNotBeAsked {
            blocker: SecondaryBlocker::CannotPayResources
        }
    );

    let (mut state, card) = dealt("Trade");
    let limit = crate::strategy_cards::commodity_limit(&state, ContentStore::embedded(), &pid("b"));
    state.player_mut(&pid("b")).unwrap().commodities = limit;
    assert_eq!(
        preview(&state, &card, &[]),
        SecondaryPreview::WouldNotBeAsked {
            blocker: SecondaryBlocker::CommoditiesFull
        }
    );
}

#[test]
fn construction_and_warfare_follow_ups_are_the_real_questions() {
    for (name, subtype) in [
        ("Construction", "place_structure"),
        ("Warfare", "produce_unit"),
    ] {
        let (state, card) = dealt(name);
        let real = real_follow_up(&state, &card).expect("the flow asks something");
        assert_eq!(real.context.as_ref().unwrap().subtype, subtype, "{name}");
        let SecondaryPreview::Question { choice, .. } = preview(&state, &card, &["yes"]) else {
            panic!("{name}: expected a question");
        };
        assert_eq!(choice, real, "{name}");
    }
}

#[test]
fn warfare_previews_the_question_after_a_scripted_build_too() {
    let (mut state, card) = dealt("Warfare");
    // Plenty to spend, so a build is offered and a payment can be chosen between planets.
    state.player_mut(&pid("b")).unwrap().trade_goods = 0;
    let first = real_follow_up(&state, &card).expect("the build list");
    let build = first
        .options
        .iter()
        .find(|option| !option.is_decline() && option.id.starts_with("build|"))
        .expect("a unit can be built")
        .id
        .clone();
    // The real flow after that build: the same window and `follow`, a script for the build, and a
    // recorder for the question that comes next.
    let content = ContentStore::embedded();
    let mut copy = state.clone();
    let mut window =
        crate::strategy::begin_strategic_action(&mut copy, content, &pid("a"), strategic())
            .unwrap();
    let yes = window
        .next_choice(&mut copy, content, POK)
        .unwrap()
        .options
        .iter()
        .find(|o| o.id == "yes")
        .cloned()
        .unwrap();
    window.take_choice(&mut copy, content, POK, yes).unwrap();
    let (decider, seen) =
        Capturing::new(Box::new(crate::choice::Scripted::new(vec![build.clone()])));
    let mut table = Table::with_default(Box::new(decider));
    let _ = crate::strategy_cards::follow(
        &mut copy,
        content,
        POK,
        None,
        &mut table,
        &pid("b"),
        card.as_str(),
    );
    let next = seen
        .borrow()
        .get(1)
        .cloned()
        .expect("the engine asks something after the build");
    let SecondaryPreview::Question { choice, step, .. } = preview(&state, &card, &["yes", &build])
    else {
        panic!("expected the question after the build");
    };
    assert_eq!(step, 2);
    assert_eq!(choice, next);
    assert!(
        ["pay_resources", "place_unit", "produce_unit"]
            .contains(&choice.context.as_ref().unwrap().subtype.as_str()),
        "{:?}",
        choice.context
    );
}

#[test]
fn politics_imperial_and_trade_are_never_run() {
    // Politics and Imperial draw cards; the preview must not touch the decks. Nothing follows
    // the window question for any of the three.
    for name in ["Politics", "Imperial", "Trade"] {
        let (state, card) = dealt(name);
        let before = serde_json::to_value(&state).unwrap();
        assert!(matches!(
            preview(&state, &card, &["yes"]),
            SecondaryPreview::Complete { .. }
        ));
        assert_eq!(before, serde_json::to_value(&state).unwrap(), "{name}");
    }
}

#[test]
fn a_preview_changes_nothing_it_was_given() {
    for name in [
        "Technology",
        "Warfare",
        "Construction",
        "Diplomacy",
        "Leadership",
    ] {
        let (mut state, card) = dealt(name);
        let owned: Vec<_> = state
            .controlled_planets(&pid("b"))
            .into_iter()
            .map(|(_, planet)| planet.clone())
            .collect();
        state.exhausted_planets.extend(owned);
        let before = serde_json::to_value(&state).unwrap();
        let cursor = state.deck_reserve.as_ref().map(|reserve| reserve.cursor());
        for answers in [&[][..], &["yes"][..], &["yes", "x"][..], &["no"][..]] {
            let _ = preview(&state, &card, answers);
        }
        assert_eq!(
            before,
            serde_json::to_value(&state).unwrap(),
            "{name}: state"
        );
        assert_eq!(
            cursor,
            state.deck_reserve.as_ref().map(|reserve| reserve.cursor()),
            "{name}: deck cursor"
        );
    }
}

/// Stops the flow at the first question the script does not answer, so the last question the
/// recorder saw is "the next question".
struct Halt;

impl crate::choice::Decider for Halt {
    fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, crate::choice::IllegalChoice> {
        Err(crate::choice::IllegalChoice::DeciderFailed {
            player: choice.player.clone(),
            prompt: choice.prompt.clone(),
            reason: "end of script".to_owned(),
        })
    }
}

/// The REAL flow (the window the game driver uses, then `follow`) for `b` after `answers`
/// (the first answers the window, the rest the follow-up questions): the question it asks next,
/// or `None` when the secondary ends.
fn real_after(state: &GameState, answers: &[String]) -> Option<Choice> {
    let content = ContentStore::embedded();
    let mut state = state.clone();
    let mut window =
        crate::strategy::begin_strategic_action(&mut state, content, &pid("a"), strategic())
            .expect("a holds the card");
    let first = window
        .next_choice(&mut state, content, POK)
        .expect("a window question");
    let wanted = first
        .options
        .iter()
        .find(|o| o.id == answers[0])
        .cloned()
        .expect("the window offers the first answer");
    window
        .take_choice(&mut state, content, POK, wanted)
        .unwrap();
    let card = first.details["card"].as_str().unwrap().to_owned();
    let (decider, seen) = Capturing::new(Box::new(crate::choice::Scripted::with_fallback(
        answers[1..].to_vec(),
        Box::new(Halt),
    )));
    let mut table = Table::with_default(Box::new(decider));
    let outcome =
        crate::strategy_cards::follow(&mut state, content, POK, None, &mut table, &pid("b"), &card);
    let last = seen.borrow().last().cloned();
    // The flow either ended on its own (Ok) or was halted at the first unscripted question.
    match outcome {
        Ok(_) => None,
        Err(_) => last,
    }
}

/// Leadership: for several trade-good stocks and purchase sizes the engine's payment, pool and
/// "again?" questions after each scripted answer are exactly the real flow's, a whole prepared
/// purchase replays through the preview, and a payment that is not offered is rejected.
#[test]
fn leadership_payment_questions_are_the_real_ones_for_several_purchase_sizes() {
    let mut payment_questions = 0;
    let mut sizes_seen = std::collections::BTreeSet::new();
    let mut multi_option_payments = 0;
    for goods in [0, 2, 4, 9, 12] {
        for tokens in 1..=4usize {
            let (mut state, card) = dealt("Leadership");
            state.player_mut(&pid("b")).unwrap().trade_goods = goods;
            if matches!(
                preview(&state, &card, &[]),
                SecondaryPreview::WouldNotBeAsked { .. }
            ) {
                continue;
            }
            let mut answers: Vec<String> = vec!["yes".to_owned()];
            let mut bought = 0;
            let mut guard = 0;
            loop {
                guard += 1;
                assert!(guard < 60, "the flow must end");
                let real = real_after(&state, &answers);
                let got = preview(
                    &state,
                    &card,
                    &answers.iter().map(String::as_str).collect::<Vec<_>>(),
                );
                match (&real, &got) {
                    (None, SecondaryPreview::Complete { unused_answers, .. }) => {
                        assert_eq!(*unused_answers, 0, "goods {goods}, tokens {tokens}");
                        break;
                    }
                    (Some(real), SecondaryPreview::Question { choice, step, .. }) => {
                        assert_eq!(
                            choice, real,
                            "goods {goods} tokens {tokens} after {answers:?}"
                        );
                        assert_eq!(*step, answers.len());
                        let subtype = real.context.as_ref().unwrap().subtype.clone();
                        let pick = match subtype.as_str() {
                            "pay_influence" => {
                                payment_questions += 1;
                                if real.options.len() > 1 {
                                    multi_option_payments += 1;
                                }
                                // The largest payment first, like a player covering the bill fast.
                                real.options
                                    .iter()
                                    .max_by_key(|o| {
                                        o.payload.get("worth").and_then(serde_json::Value::as_i64)
                                    })
                                    .unwrap()
                                    .id
                                    .clone()
                            }
                            "gain_command_token" => {
                                bought += 1;
                                "tactic_tokens".to_owned()
                            }
                            "buy_token_with_influence" => {
                                if bought >= tokens {
                                    "no".to_owned()
                                } else {
                                    "yes".to_owned()
                                }
                            }
                            other => panic!("unexpected {other}"),
                        };
                        answers.push(pick);
                    }
                    other => panic!(
                        "goods {goods} tokens {tokens}: real and preview disagree: {other:?}"
                    ),
                }
            }
            sizes_seen.insert(bought);
            // The whole purchase, replayed in one go, is accepted with nothing left over; one
            // extra answer is not (the engine asked nothing more).
            let refs: Vec<&str> = answers.iter().map(String::as_str).collect();
            assert!(
                matches!(
                    preview(&state, &card, &refs),
                    SecondaryPreview::Complete {
                        unused_answers: 0,
                        ..
                    }
                ),
                "goods {goods}, tokens {tokens}: {answers:?}"
            );
            let mut extra = refs.clone();
            extra.push("yes");
            assert!(
                matches!(
                    preview(&state, &card, &extra),
                    SecondaryPreview::Complete {
                        unused_answers: 1,
                        ..
                    }
                ),
                "the leftover answer is reported"
            );
            // A payment the engine does not offer is rejected at its index.
            if answers.len() > 2 {
                let mut broken = refs.clone();
                broken[1] = "exhaust|no-such-planet";
                match preview(&state, &card, &broken) {
                    SecondaryPreview::Rejected { at, answer, choice } => {
                        assert_eq!(at, 1);
                        assert_eq!(answer, "exhaust|no-such-planet");
                        assert_eq!(choice.context.as_ref().unwrap().subtype, "pay_influence");
                    }
                    other => panic!("expected a rejection, got {other:?}"),
                }
            }
        }
    }
    assert!(
        payment_questions >= 5,
        "payments were asked ({payment_questions})"
    );
    assert!(
        multi_option_payments >= 1,
        "a real payment choice was exercised"
    );
    assert!(
        sizes_seen.len() >= 2,
        "several purchase sizes ({sizes_seen:?})"
    );
}

/// A payment answer for a payment the engine settles by itself (a lone option) is skipped, as
/// the real batch skips it.
#[test]
fn a_lone_payment_option_is_settled_by_the_engine_and_its_scripted_answer_skipped() {
    let (mut state, card) = dealt("Leadership");
    // Nothing but three trade goods: the only way to pay is a trade good, so no question is asked.
    state.player_mut(&pid("b")).unwrap().trade_goods = 3;
    let exhausted: Vec<_> = state
        .controlled_planets(&pid("b"))
        .into_iter()
        .map(|(_, planet)| planet.clone())
        .collect();
    state.exhausted_planets.extend(exhausted);
    let SecondaryPreview::Question { choice, .. } = preview(&state, &card, &["yes"]) else {
        panic!("a question");
    };
    assert_eq!(
        choice.context.as_ref().unwrap().subtype,
        "gain_command_token",
        "the lone trade-good payments were settled without a question"
    );
    let planned = [
        "yes",
        "trade_good",
        "trade_good",
        "trade_good",
        "tactic_tokens",
    ];
    assert!(matches!(
        preview(&state, &card, &planned),
        SecondaryPreview::Complete {
            unused_answers: 0,
            ..
        }
    ));
}
