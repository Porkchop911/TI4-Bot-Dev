//! Differential test of the secondary preview against the real flow on recorded games.
//!
//! Every game is replayed step by step. At each boundary where a follower is really asked a
//! strategy-card secondary question, the preview computed from the position JUST BEFORE the
//! window (the boundary itself) is compared with the real `Choice` (options, payloads, context,
//! details), and so is the preview of every follow-up question the real flow then asks that seat
//! (computed with the seat's own earlier answers, as a client would). A second figure measures
//! staleness: the preview computed from the position when the strategic action began (before the
//! primary resolved) against the same real questions, and whether the seats it said would be
//! asked really were.
//!
//! The first comparison must agree everywhere except where the real flow put a faction prompt
//! first that the recorded player did not decline (the preview declines those, and says so in
//! `skipped`). Agreement per card is printed (`--nocapture`) and written to
//! `TI4_PREVIEW_REPORT` when set.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use ti4_content::ContentStore;
use ti4_engine::choice::{Capturing, Choice, DecisionRecord, Table};
use ti4_engine::game::Game;
use ti4_engine::secondary_preview::{SecondaryPreview, follow_up_subtypes, preview_secondary};
use ti4_host::rng_force::RngForce;
use ti4_model::content_types::POK;
use ti4_model::id::{PlayerId, StrategyCardId};
use ti4_model::state::GameState;

use crate::support::{Source, forced_decider, generated, saved_games};

#[derive(Default, Debug, Clone)]
struct Tally {
    windows: usize,
    window_exact: usize,
    follow_ups: usize,
    follow_up_exact: usize,
    /// Real flow asked a faction prompt first that the recording did not decline.
    follow_up_faction_diverged: usize,
    stale_windows: usize,
    stale_exact: usize,
    stale_follow_ups: usize,
    stale_follow_up_exact: usize,
    /// Warfare: the question the engine asks right after the first build (payment, placement or
    /// the next build list), previewed with the recorded build as the scripted answer.
    post_build: usize,
    post_build_exact: usize,
    /// Leadership: every payment, pool and "again?" question the real flow asked a follower after
    /// the window's answer, previewed with the follower's own earlier answers.
    lead_questions: usize,
    lead_questions_exact: usize,
    /// Leadership: whole purchases (the recorded answers replayed in ONE preview) accepted by the
    /// engine with nothing left over, by number of tokens bought.
    lead_replays: usize,
    lead_replays_ok: usize,
    /// ... and the same answers replayed against the position when the action began.
    lead_stale: usize,
    lead_stale_ok: usize,
    lead_replay_by_tokens: BTreeMap<usize, (usize, usize)>,
    /// Leadership payment questions that offered a real choice (more than one option).
    lead_choices: usize,
    /// Seats the action-start preview said would / would not be asked, against what happened.
    asked_predicted_asked: usize,
    asked_predicted_not: usize,
    not_asked_predicted_not: usize,
    not_asked_predicted_asked: usize,
}

impl Tally {
    fn add(&mut self, other: &Tally) {
        self.windows += other.windows;
        self.window_exact += other.window_exact;
        self.follow_ups += other.follow_ups;
        self.follow_up_exact += other.follow_up_exact;
        self.follow_up_faction_diverged += other.follow_up_faction_diverged;
        self.stale_windows += other.stale_windows;
        self.stale_exact += other.stale_exact;
        self.stale_follow_ups += other.stale_follow_ups;
        self.stale_follow_up_exact += other.stale_follow_up_exact;
        self.post_build += other.post_build;
        self.post_build_exact += other.post_build_exact;
        self.lead_questions += other.lead_questions;
        self.lead_questions_exact += other.lead_questions_exact;
        self.lead_replays += other.lead_replays;
        self.lead_replays_ok += other.lead_replays_ok;
        self.lead_stale += other.lead_stale;
        self.lead_stale_ok += other.lead_stale_ok;
        self.lead_choices += other.lead_choices;
        for (tokens, (all, ok)) in &other.lead_replay_by_tokens {
            let entry = self.lead_replay_by_tokens.entry(*tokens).or_default();
            entry.0 += all;
            entry.1 += ok;
        }
        self.asked_predicted_asked += other.asked_predicted_asked;
        self.asked_predicted_not += other.asked_predicted_not;
        self.not_asked_predicted_not += other.not_asked_predicted_not;
        self.not_asked_predicted_asked += other.not_asked_predicted_asked;
    }
}

fn card_name(card: &str) -> String {
    ti4_engine::strategy_cards::card_name(ContentStore::embedded(), card)
        .unwrap_or_else(|| card.to_owned())
}

/// "Technology (pok7technology)": the card by name and by id, so the Thunder's Edge variants are told apart.
fn card_label(card: &str) -> String {
    format!("{} ({card})", card_name(card))
}

fn preview(
    state: &GameState,
    card: &str,
    primary: &PlayerId,
    follower: &PlayerId,
    answers: &[String],
) -> SecondaryPreview {
    preview_secondary(
        state,
        ContentStore::embedded(),
        POK,
        None,
        &StrategyCardId::new(card),
        primary,
        follower,
        answers,
    )
}

/// The game at its start, answering the source's recorded decisions through a recorder of every
/// question asked.
fn recorded_game(source: &Source) -> (Game<'static>, Rc<RefCell<Vec<Choice>>>) {
    let force = RngForce::new(&source.marks);
    let (decider, seen) = Capturing::new(Box::new(forced_decider(source, 0, force.clone())));
    let mut game = Game::with_table(
        source.state.clone(),
        ContentStore::embedded(),
        Table::with_default(Box::new(decider)),
    )
    .with_galaxy(source.galaxy.clone());
    if let Some(force) = &force {
        force.attach(&mut game);
    }
    (game, seen)
}

fn is_window(choice: &Choice) -> bool {
    choice
        .details
        .get("kind")
        .and_then(serde_json::Value::as_str)
        == Some("strategy_secondary")
}

/// What the preview said about every seat when the strategic action began.
struct Track {
    card: String,
    primary: PlayerId,
    start: GameState,
    said: BTreeMap<PlayerId, bool>,
    asked: Vec<PlayerId>,
}

fn walk(source: &Source, per_card: &mut BTreeMap<String, Tally>) {
    let (mut game, seen) = recorded_game(source);
    let total = source.records.len();
    let mut action_start: Option<GameState> = None;
    let mut track: Option<Track> = None;
    let finish = |track: &mut Option<Track>, per_card: &mut BTreeMap<String, Tally>| {
        let Some(done) = track.take() else { return };
        let tally = per_card.entry(card_label(&done.card)).or_default();
        for (seat, said) in &done.said {
            match (done.asked.contains(seat), said) {
                (true, true) => tally.asked_predicted_asked += 1,
                (true, false) => tally.asked_predicted_not += 1,
                (false, false) => tally.not_asked_predicted_not += 1,
                (false, true) => tally.not_asked_predicted_asked += 1,
            }
        }
    };
    for _ in 0..total.saturating_mul(4) {
        let len = game.table.log.records.len();
        if len >= total || game.state.finished {
            break;
        }
        let offered = game.legal_options();
        if let Some(choice) = &offered {
            if choice.prompt == "action phase" {
                finish(&mut track, per_card);
                action_start = Some(game.state.clone());
            }
        }
        let window = offered.as_ref().filter(|choice| is_window(choice)).cloned();
        let before_state = window.as_ref().map(|_| game.state.clone());
        let seen_before = seen.borrow().len();
        let result = game.step();
        if result.error.is_some() {
            break;
        }
        let records: Vec<DecisionRecord> = game.table.log.records[len..].to_vec();

        // The strategic action just chosen: what the preview says about every other seat.
        if let (Some(first), Some(start)) = (records.first(), action_start.as_ref()) {
            if first.prompt == "action phase" && first.chosen.starts_with("strategic") {
                finish(&mut track, per_card);
                let primary = first.player.clone();
                let card = first
                    .chosen
                    .strip_prefix("strategic|")
                    .map(ToOwned::to_owned)
                    .or_else(|| {
                        start.player(&primary).and_then(|seat| {
                            seat.unused_strategy_cards().first().map(|c| c.to_string())
                        })
                    });
                if let Some(card) = card {
                    if std::env::var("TI4_PREVIEW_DEBUG").is_ok() {
                        for seat in start.seating_order.iter().filter(|seat| **seat != primary) {
                            let yes = preview(start, &card, &primary, seat, &["yes".to_owned()]);
                            let kind = match &yes {
                                SecondaryPreview::Question { choice, .. } => format!(
                                    "question {:?}",
                                    choice.context.as_ref().map(|c| c.subtype.clone())
                                ),
                                other => format!("{other:?}").chars().take(90).collect(),
                            };
                            println!("DEBUG {card} {seat}: yes -> {kind}");
                        }
                    }
                    let said = start
                        .seating_order
                        .iter()
                        .filter(|seat| **seat != primary)
                        .map(|seat| {
                            let asked = matches!(
                                preview(start, &card, &primary, seat, &[]),
                                SecondaryPreview::Question { .. }
                            );
                            (seat.clone(), asked)
                        })
                        .collect();
                    track = Some(Track {
                        card,
                        primary,
                        start: start.clone(),
                        said,
                        asked: Vec::new(),
                    });
                }
            }
        }

        let (Some(window), Some(before)) = (window, before_state) else {
            continue;
        };
        let card = window.details["card"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        let primary = PlayerId::new(window.details["played_by"].as_str().unwrap_or_default());
        let follower = window.player.clone();
        let tally = per_card.entry(card_label(&card)).or_default();
        tally.windows += 1;
        if let Some(track) = track.as_mut() {
            if track.card == card && track.primary == primary {
                track.asked.push(follower.clone());
            }
        }
        // Just before the window: must be exactly the real question.
        match preview(&before, &card, &primary, &follower, &[]) {
            SecondaryPreview::Question { choice, .. } if choice == window => {
                tally.window_exact += 1
            }
            other => panic!(
                "{}: window preview differs for {follower} on {card}:\n real {window:?}\n preview {other:?}",
                source.name
            ),
        }
        // At the start of the action (before the primary resolved).
        if let Some(track) = track
            .as_ref()
            .filter(|t| t.card == card && t.primary == primary)
        {
            tally.stale_windows += 1;
            if matches!(
                preview(&track.start, &card, &primary, &follower, &[]),
                SecondaryPreview::Question { choice, .. } if choice == window
            ) {
                tally.stale_exact += 1;
            }
        }
        // The follow-ups the real flow asked this seat in that step, in order.
        let own = follow_up_subtypes(&card_name(&card));
        let Some(answer) = records.first().filter(|r| r.player == follower) else {
            continue;
        };
        let mut answers = vec![answer.chosen.clone()];
        if answer.chosen == "no" || answer.chosen == "decline" {
            continue;
        }
        let asked: Vec<Choice> = seen.borrow()[seen_before..]
            .iter()
            .filter(|choice| choice.player == follower && !is_window(choice))
            .cloned()
            .collect();
        let mut faction_diverged = false;
        let mut chosen = records[1..].iter().filter(|r| r.player == follower);
        let mut asked = asked.into_iter();
        while let Some(real) = asked.next() {
            let subtype = real
                .context
                .as_ref()
                .map_or("", |c| c.subtype.as_str())
                .to_owned();
            let recorded = chosen.next().map(|r| r.chosen.clone());
            if !own.contains(&subtype.as_str()) {
                // A faction prompt the preview declines; if the recording did not, the paths part.
                if recorded.as_deref().is_some_and(|id| id != "decline") {
                    faction_diverged = true;
                }
                continue;
            }
            if card_name(&card) == "Leadership" {
                tally.lead_questions += 1;
                if subtype == "pay_influence" && real.options.len() > 1 {
                    tally.lead_choices += 1;
                }
                let got = preview(&before, &card, &primary, &follower, &answers);
                match &got {
                    SecondaryPreview::Question { choice, .. } if *choice == real => {
                        tally.lead_questions_exact += 1;
                    }
                    _ if faction_diverged => tally.follow_up_faction_diverged += 1,
                    other => panic!(
                        "{}: Leadership question differs for {follower} after {answers:?}:\n real {real:?}\n preview {other:?}",
                        source.name
                    ),
                }
                let Some(recorded) = recorded else { break };
                answers.push(recorded);
                continue;
            }
            tally.follow_ups += 1;
            let got = preview(&before, &card, &primary, &follower, &answers);
            match &got {
                SecondaryPreview::Question { choice, .. } if *choice == real => {
                    tally.follow_up_exact += 1;
                }
                _ if faction_diverged => tally.follow_up_faction_diverged += 1,
                other => panic!(
                    "{}: follow-up preview differs for {follower} on {card}:\n real {real:?}\n preview {other:?}",
                    source.name
                ),
            }
            if let Some(track) = track
                .as_ref()
                .filter(|t| t.card == card && t.primary == primary)
            {
                tally.stale_follow_ups += 1;
                if matches!(
                    preview(&track.start, &card, &primary, &follower, &answers),
                    SecondaryPreview::Question { choice, .. } if choice == real
                ) {
                    tally.stale_follow_up_exact += 1;
                }
            }
            // Only Diplomacy's second planet is a follow-up a client can script from the first
            // answer; production's later questions (after payment and placement) are asked by the
            // real window when it opens, and a Technology/Construction card asks nothing more.
            if card_name(&card) == "Warfare"
                && let Some(build) = recorded.as_deref().filter(|id| id.starts_with("build|"))
                && let Some(next) = asked.next()
            {
                tally.post_build += 1;
                let mut scripted = answers.clone();
                scripted.push(build.to_owned());
                if matches!(
                    preview(&before, &card, &primary, &follower, &scripted),
                    SecondaryPreview::Question { choice, .. } if choice == next
                ) {
                    tally.post_build_exact += 1;
                } else {
                    println!(
                        "{}: the question after the build differs ({:?})",
                        source.name,
                        next.context.as_ref().map(|c| c.subtype.clone())
                    );
                }
            }
            if !card_name(&card).eq("Diplomacy") {
                break;
            }
            let Some(recorded) = recorded else { break };
            answers.push(recorded);
        }
        // Leadership: the follower's whole recorded purchase, replayed through one preview.
        if card_name(&card) == "Leadership" && !faction_diverged {
            let tokens = answers.iter().filter(|id| id.ends_with("_tokens")).count();
            let entry = tally.lead_replay_by_tokens.entry(tokens).or_default();
            tally.lead_replays += 1;
            entry.0 += 1;
            let ok = matches!(
                preview(&before, &card, &primary, &follower, &answers),
                SecondaryPreview::Complete {
                    unused_answers: 0,
                    ..
                } | SecondaryPreview::Question { .. }
            );
            // Staleness: the same answers against the position when the action began.
            if let Some(track) = track
                .as_ref()
                .filter(|t| t.card == card && t.primary == primary)
            {
                tally.lead_stale += 1;
                if matches!(
                    preview(&track.start, &card, &primary, &follower, &answers),
                    SecondaryPreview::Complete {
                        unused_answers: 0,
                        ..
                    } | SecondaryPreview::Question { .. }
                ) {
                    tally.lead_stale_ok += 1;
                }
            }
            if ok {
                tally.lead_replays_ok += 1;
                entry.1 += 1;
            } else {
                println!(
                    "{}: the recorded purchase {answers:?} was not accepted",
                    source.name
                );
            }
        }
    }
    finish(&mut track, per_card);
}

fn report(title: &str, per_card: &BTreeMap<String, Tally>) -> String {
    let mut out = format!("== {title}\n");
    let mut all = Tally::default();
    for (card, tally) in per_card {
        all.add(tally);
        out.push_str(&format!(
            "{card:<13} post-build {:>3} exact {:>3} | windows {:>3} exact {:>3} | follow-ups {:>3} exact {:>3} faction-diverged {:>2} | action-start: windows {:>3} same {:>3}, follow-ups {:>3} same {:>3} | seats asked&predicted {:>3} asked&not-predicted {:>3} not-asked&predicted-not {:>3} not-asked&predicted-asked {:>3}\n",
            tally.post_build, tally.post_build_exact,
            tally.windows, tally.window_exact, tally.follow_ups, tally.follow_up_exact,
            tally.follow_up_faction_diverged, tally.stale_windows, tally.stale_exact,
            tally.stale_follow_ups, tally.stale_follow_up_exact, tally.asked_predicted_asked,
            tally.asked_predicted_not, tally.not_asked_predicted_not, tally.not_asked_predicted_asked,
        ));
    }
    for (card, tally) in per_card.iter().filter(|(_, t)| t.lead_questions > 0) {
        let by_tokens: Vec<String> = tally
            .lead_replay_by_tokens
            .iter()
            .map(|(tokens, (all, ok))| format!("{tokens} tokens: {ok}/{all}"))
            .collect();
        out.push_str(&format!(
            "{card:<13} Leadership payment/pool/again questions {:>3} exact {:>3} (payment questions with a real choice {:>3}) | whole purchases replayed {:>3} accepted {:>3} [{}] | same answers against the action-start position {:>3} accepted {:>3}\n",
            tally.lead_questions, tally.lead_questions_exact, tally.lead_choices,
            tally.lead_replays, tally.lead_replays_ok, by_tokens.join(", "), tally.lead_stale, tally.lead_stale_ok
        ));
    }
    out.push_str(&format!(
        "{:<13} windows {:>3} exact {:>3} | follow-ups {:>3} exact {:>3} faction-diverged {:>2}\n",
        "ALL",
        all.windows,
        all.window_exact,
        all.follow_ups,
        all.follow_up_exact,
        all.follow_up_faction_diverged
    ));
    out
}

fn check(title: &str, sources: &[Source]) -> BTreeMap<String, Tally> {
    let mut per_card = BTreeMap::new();
    for source in sources {
        walk(source, &mut per_card);
    }
    let text = report(title, &per_card);
    println!("{text}");
    if let Ok(path) = std::env::var("TI4_PREVIEW_REPORT") {
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .expect("report file");
        std::io::Write::write_all(&mut file, text.as_bytes()).expect("write report");
    }
    for (card, tally) in &per_card {
        assert_eq!(
            tally.windows, tally.window_exact,
            "{card}: every window question is exact"
        );
        assert_eq!(
            tally.follow_ups,
            tally.follow_up_exact + tally.follow_up_faction_diverged,
            "{card}: every follow-up is exact or explained by a faction prompt"
        );
        assert_eq!(
            tally.lead_questions_exact, tally.lead_questions,
            "{card}: every Leadership question is exact"
        );
        assert_eq!(
            tally.lead_replays, tally.lead_replays_ok,
            "{card}: every recorded purchase replays"
        );
        assert!(
            tally.post_build <= tally.post_build_exact + tally.follow_up_faction_diverged,
            "{card}: the question after a build is exact unless a faction prompt intervened"
        );
    }
    per_card
}

#[test]
fn slow_the_preview_equals_the_real_questions_on_generated_games() {
    let sources: Vec<Source> = (1..=6)
        .map(|seed| generated(&format!("generated-{seed}"), 9_000 + seed, 900, 0))
        .collect();
    let per_card = check("generated games", &sources);
    let windows: usize = per_card.values().map(|t| t.windows).sum();
    assert!(
        windows >= 10,
        "the generated games must exercise secondaries ({windows})"
    );
}

#[test]
fn the_preview_equals_the_real_questions_on_saved_games() {
    let sources = saved_games(24);
    if sources.is_empty() {
        eprintln!("no saved games on this machine; skipped");
        return;
    }
    check("saved real games", &sources);
}
