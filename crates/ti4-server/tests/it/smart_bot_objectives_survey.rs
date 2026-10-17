//! How the rule bots score objectives, for tuning the objective layer
//! (`ti4_rule_bot::objectives`): `cargo test --release --test smart_bot_objectives_survey --
//! --ignored --nocapture`.
//!
//! Plays all-smart tables (default 4 seats, seeds 1-24, `SURVEY_OBJ_SEEDS=1-24`,
//! `SURVEY_OBJ_SEATS=4`, `SURVEY_OBJ_ROUNDS=8`) on a synchronous engine driver, so every step can
//! be inspected, and reports per round: average victory points per seat at the end of the round,
//! public and secret objectives scored, and **missed** objectives. A public is missed in a round
//! when, at some point of that round's action phase, the seat could have scored a revealed public
//! objective (requirement met, or a bought one affordable) and it then scored no public objective
//! that round; a secret likewise for the secrets it held. Only what that seat could see: public
//! objectives and its own secrets. `left` counts objectives still scoreable when the status phase
//! scoring window opened in a category the seat then did not score: a choice error, never a rule.
//!
//! The seats are rule bots exactly as a `smart` seat plays them (random pick drawn first, then
//! [`ti4_rule_bot::pick`]); the driver is the engine's own `Game::step`.
//! `SURVEY_OBJ_LAYER=off` switches the objective layer off for a before/after comparison.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use ti4_content::ContentStore;
use ti4_engine::choice::{Choice, ChoiceOption, Decider, IllegalChoice, SeatObservation, Table};
use ti4_engine::game::Game;
use ti4_model::id::{ObjectiveId, PlayerId};
use ti4_model::state::{GameState, Phase};
use ti4_server::session::random_bot::{decision_rng, pick_index};

/// A `smart` seat without the session around it.
struct SmartSeat {
    /// The place of the seat in the seat order.
    place: usize,
    seed: u64,
    counter: Arc<AtomicUsize>,
}

impl SmartSeat {
    fn answer(&self, choice: &Choice, seen: Option<&SeatObservation<'_>>) -> ChoiceOption {
        let index = self.counter.fetch_add(1, Ordering::SeqCst);
        let mut rng = decision_rng(self.seed, self.place, index);
        let random = pick_index(choice, &mut rng);
        let picked = seen
            .and_then(|seen| ti4_rule_bot::pick(choice, seen, &mut rng))
            .filter(|&i| i < choice.options.len())
            .unwrap_or(random);
        choice.options[picked].clone()
    }
}

impl Decider for SmartSeat {
    fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
        if choice.options.is_empty() {
            return Err(IllegalChoice::NoOptions {
                player: choice.player.clone(),
                prompt: choice.prompt.clone(),
            });
        }
        Ok(self.answer(choice, None))
    }

    fn choose_seeing(
        &mut self,
        choice: &Choice,
        seen: &SeatObservation<'_>,
    ) -> Result<ChoiceOption, IllegalChoice> {
        if choice.options.is_empty() {
            return Err(IllegalChoice::NoOptions {
                player: choice.player.clone(),
                prompt: choice.prompt.clone(),
            });
        }
        Ok(self.answer(choice, Some(seen)))
    }
}

/// One seat's record of one round.
#[derive(Debug, Clone, Default)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "one flag per survey question"
)]
struct SeatRound {
    vp_end: i32,
    publics: usize,
    secrets: usize,
    /// A public was scoreable at some point of the action phase.
    had_public: bool,
    /// ... and every one of them was a bought objective (spend, trade goods, tokens).
    had_only_bought: bool,
    had_secret: bool,
    /// Scoreable when the status scoring window opened, per category.
    status_public: bool,
    status_secret: bool,
    /// Every objective that was scoreable at some point of the round.
    seen_names: BTreeSet<String>,
    /// Points from objectives at the end of the round (the rest are custodians, Imperial, ...).
    objective_vp: i32,
}

/// One game: per seat (in seat order), per round from 1.
struct GameRecord {
    rounds: Vec<Vec<SeatRound>>,
    finished: bool,
    best: i32,
    last_round: u32,
    names: BTreeMap<String, usize>,
}

fn scored_split(
    state: &GameState,
    player: &PlayerId,
) -> (BTreeSet<ObjectiveId>, BTreeSet<ObjectiveId>) {
    let content = ContentStore::embedded();
    let mut publics = BTreeSet::new();
    let mut secrets = BTreeSet::new();
    for alias in state.scored_by(player) {
        if content
            .get(
                ti4_model::content_types::ContentType::SecretObjectives,
                alias.as_str(),
            )
            .is_some()
        {
            secrets.insert(alias);
        } else {
            publics.insert(alias);
        }
    }
    (publics, secrets)
}

fn objective_points(state: &GameState, player: &PlayerId) -> i32 {
    let content = ContentStore::embedded();
    state
        .scored_by(player)
        .iter()
        .map(|alias| ti4_engine::objectives::points_for(content, alias).unwrap_or(1))
        .sum()
}

#[expect(clippy::too_many_lines, reason = "one game, driven and read")]
fn play(seats: usize, seed: u64, max_round: u32) -> GameRecord {
    let content = ContentStore::embedded();
    let ids: Vec<PlayerId> = (1..=seats)
        .map(|i| PlayerId::new(format!("p{i}")))
        .collect();
    let (state, galaxy) = ti4_host::map::create_game_with_template_seated(
        content,
        &ids,
        seed,
        None,
        &ti4_engine::seating::seat_in_scope(&ids),
        Some("te"),
    )
    .expect("create game");
    let counter = Arc::new(AtomicUsize::new(0));
    let mut table = Table::new();
    for (place, id) in ids.iter().enumerate() {
        table.seat(
            id.clone(),
            Box::new(SmartSeat {
                place,
                seed,
                counter: counter.clone(),
            }),
        );
    }
    let mut game = Game::with_table(state, content, table).with_galaxy(galaxy.clone());
    let sources = game.sources();
    let mut rounds: Vec<Vec<SeatRound>> = Vec::new();
    let mut names: BTreeMap<String, usize> = BTreeMap::new();
    let mut scored_before: Vec<(BTreeSet<ObjectiveId>, BTreeSet<ObjectiveId>)> =
        ids.iter().map(|id| scored_split(&game.state, id)).collect();
    let mut status_seen = 0u32;
    let mut steps = 0usize;
    loop {
        let state = &game.state;
        let round = state.round;
        let r = usize::try_from(round).unwrap_or(1).max(1) - 1;
        while rounds.len() <= r {
            rounds.push(vec![SeatRound::default(); seats]);
        }
        match state.phase {
            Phase::Action => {
                for (i, id) in ids.iter().enumerate() {
                    let row = &mut rounds[r][i];
                    let publics = ti4_engine::objectives::scoreable_on(
                        state,
                        content,
                        sources,
                        id,
                        Some(&galaxy),
                    );
                    row.seen_names
                        .extend(publics.iter().map(|a| a.as_str().to_owned()));
                    if !publics.is_empty() {
                        let bought = publics
                            .iter()
                            .all(|alias| ti4_engine::objectives::cost_of(alias).is_some());
                        row.had_only_bought = if row.had_public {
                            row.had_only_bought && bought
                        } else {
                            bought
                        };
                        row.had_public = true;
                    }
                    let secrets = ti4_engine::secrets::scoreable_on(
                        state,
                        content,
                        sources,
                        id,
                        Some(&galaxy),
                    );
                    if !secrets.is_empty() {
                        row.had_secret = true;
                        row.seen_names
                            .extend(secrets.iter().map(|a| format!("s:{}", a.as_str())));
                    }
                }
            }
            Phase::Status if status_seen < round => {
                status_seen = round;
                for (i, id) in ids.iter().enumerate() {
                    let publics = ti4_engine::objectives::scoreable_on(
                        state,
                        content,
                        sources,
                        id,
                        Some(&galaxy),
                    );
                    let secrets = ti4_engine::secrets::scoreable_on(
                        state,
                        content,
                        sources,
                        id,
                        Some(&galaxy),
                    );
                    let row = &mut rounds[r][i];
                    row.status_public = !publics.is_empty();
                    row.status_secret = !secrets.is_empty();
                    // Scoreable at the window counts as reachable during the round.
                    if row.status_public && !row.had_public {
                        row.had_only_bought = publics
                            .iter()
                            .all(|alias| ti4_engine::objectives::cost_of(alias).is_some());
                    }
                    row.had_public |= row.status_public;
                    row.had_secret |= row.status_secret;
                }
            }
            _ => {}
        }
        if state.finished || state.round > max_round || steps > 400_000 {
            break;
        }
        let result = game.step();
        steps += 1;
        // Round bookkeeping after every step: scored objectives and points.
        let state = &game.state;
        let r_after = usize::try_from(state.round).unwrap_or(1).max(1) - 1;
        let target = if state.round > round { r } else { r_after };
        while rounds.len() <= target {
            rounds.push(vec![SeatRound::default(); seats]);
        }
        for (i, id) in ids.iter().enumerate() {
            let (publics, secrets) = scored_split(state, id);
            let (old_publics, old_secrets) = &scored_before[i];
            let new_publics = publics.difference(old_publics).count();
            let new_secrets = secrets.difference(old_secrets).count();
            for alias in publics
                .difference(old_publics)
                .chain(secrets.difference(old_secrets))
            {
                *names.entry(alias.as_str().to_owned()).or_default() += 1;
            }
            let row = &mut rounds[r][i];
            row.publics += new_publics;
            row.secrets += new_secrets;
            scored_before[i] = (publics, secrets);
            let seat = state.player(id).expect("seat");
            let row = &mut rounds[r][i];
            row.vp_end = seat.victory_points;
            row.objective_vp = objective_points(state, id);
        }
        if let Some(error) = result.error {
            eprintln!(
                "seed {seed}: engine error {error:?} in round {}",
                game.state.round
            );
            break;
        }
    }
    let state = &game.state;
    let best = state
        .players
        .iter()
        .map(|p| p.victory_points)
        .max()
        .unwrap_or(0);
    // A round never reached keeps the last points.
    for r in 1..rounds.len() {
        let (done, rest) = rounds.split_at_mut(r);
        for (row, previous) in rest[0].iter_mut().zip(&done[r - 1]) {
            row.vp_end = row.vp_end.max(previous.vp_end);
        }
    }
    GameRecord {
        rounds,
        finished: state.finished,
        best,
        last_round: state.round,
        names,
    }
}

fn seeds_from(spec: &str) -> Vec<u64> {
    spec.split(',')
        .flat_map(|part| match part.split_once('-') {
            Some((a, b)) => {
                let (a, b): (u64, u64) = (
                    a.trim().parse().expect("seed"),
                    b.trim().parse().expect("seed"),
                );
                (a..=b).collect::<Vec<_>>()
            }
            None => vec![part.trim().parse().expect("seed")],
        })
        .collect()
}

#[test]
#[ignore = "a survey for tuning, not a check"]
#[expect(
    clippy::cast_precision_loss,
    clippy::too_many_lines,
    reason = "small counts; one report"
)]
fn survey_objectives() {
    let seeds = seeds_from(&std::env::var("SURVEY_OBJ_SEEDS").unwrap_or_else(|_| "1-24".into()));
    let seats: usize = std::env::var("SURVEY_OBJ_SEATS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(4);
    let max_round: u32 = std::env::var("SURVEY_OBJ_ROUNDS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(8);
    // `SURVEY_OBJ_LAYER=off` plays the rule bot without its objective layer, for comparison;
    // `SURVEY_OBJ_LAYER=p1,p3` plays it for those seats only.
    let layer = std::env::var("SURVEY_OBJ_LAYER").unwrap_or_else(|_| "on".to_owned());
    match layer.as_str() {
        "on" => ti4_rule_bot::objectives::set_enabled(true),
        "off" => ti4_rule_bot::objectives::set_enabled(false),
        seats => {
            let seats: Vec<&str> = seats.split(',').map(str::trim).collect();
            ti4_rule_bot::objectives::set_enabled_for(&seats);
        }
    }
    eprintln!("OBJ layer {layer}");
    let threads: usize = std::env::var("SURVEY_OBJ_THREADS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(4);
    let results: Vec<(u64, GameRecord)> = std::thread::scope(|scope| {
        let chunks: Vec<Vec<u64>> = (0..threads)
            .map(|t| seeds.iter().copied().skip(t).step_by(threads).collect())
            .collect();
        let handles: Vec<_> = chunks
            .into_iter()
            .map(|chunk| {
                scope.spawn(move || {
                    chunk
                        .into_iter()
                        .map(|seed| (seed, play(seats, seed, max_round)))
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        let mut all: Vec<(u64, GameRecord)> = handles
            .into_iter()
            .flat_map(|h| h.join().expect("survey thread"))
            .collect();
        all.sort_by_key(|(seed, _)| *seed);
        all
    });
    let games = results.len();
    let mut names: BTreeMap<String, usize> = BTreeMap::new();
    for (seed, record) in &results {
        let mut line = format!(
            "seed {seed:3}: round {} best {}",
            record.last_round, record.best
        );
        for (r, row) in record.rounds.iter().enumerate() {
            let vps: Vec<String> = row.iter().map(|s| s.vp_end.to_string()).collect();
            let _ = write!(line, " | r{} {}", r + 1, vps.join("/"));
        }
        eprintln!("{line}");
        for (name, n) in &record.names {
            *names.entry(name.clone()).or_default() += n;
        }
    }
    // Per seat: final points and wins (the seat holding the most points when the game ended).
    let mut seat_vp = vec![0i32; seats];
    let mut seat_wins = vec![0usize; seats];
    for (_, record) in &results {
        if let Some(last) = record.rounds.last() {
            for (i, row) in last.iter().enumerate() {
                seat_vp[i] += row.vp_end;
                if record.finished && row.vp_end == record.best {
                    seat_wins[i] += 1;
                }
            }
        }
    }
    for i in 0..seats {
        eprintln!(
            "OBJ SEAT p{} final VP {:.2} wins {}",
            i + 1,
            f64::from(seat_vp[i]) / games.max(1) as f64,
            seat_wins[i]
        );
    }
    let to_ten = results.iter().filter(|(_, r)| r.best >= 10).count();
    let finished = results.iter().filter(|(_, r)| r.finished).count();
    let mean_end: f64 = results
        .iter()
        .map(|(_, r)| f64::from(r.last_round))
        .sum::<f64>()
        / games.max(1) as f64;
    eprintln!(
        "OBJ GAMES {games} seats {seats}: {to_ten} reached 10 VP, {finished} finished, mean last round {mean_end:.2}"
    );
    eprintln!(
        "OBJ round | VP/seat | objVP/seat | pub/seat | sec/seat | missed pub (bought only) | missed sec | left at status | seats | games ended"
    );
    let (mut total_missed_pub, mut total_missed_sec, mut total_left) = (0, 0, 0);
    let mut missed_names: BTreeMap<String, usize> = BTreeMap::new();
    let from: usize = std::env::var("SURVEY_OBJ_MISSED_FROM")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(3);
    for r in 0..usize::try_from(max_round).unwrap_or(8) {
        // The round a game ended in is left out: the status phase stops at the winning point,
        // so seats after the winner never get their turn to score.
        let rows: Vec<&SeatRound> = results
            .iter()
            .filter(|(_, rec)| {
                !(rec.finished && r + 1 >= usize::try_from(rec.last_round).unwrap_or(0))
            })
            .filter_map(|(_, rec)| rec.rounds.get(r))
            .flatten()
            .collect();
        let ended = results
            .iter()
            .filter(|(_, rec)| {
                rec.finished && usize::try_from(rec.last_round).unwrap_or(0) <= r + 1
            })
            .count();
        // Points carry over for games that ended earlier.
        let vp_rows: Vec<i32> = results
            .iter()
            .flat_map(|(_, rec)| {
                let row = rec.rounds.get(r).or_else(|| rec.rounds.last());
                row.into_iter().flatten().map(|s| s.vp_end)
            })
            .collect();
        if vp_rows.is_empty() {
            continue;
        }
        let n = (rows.len() as f64).max(1.0);
        let vp = vp_rows.iter().map(|&v| f64::from(v)).sum::<f64>() / vp_rows.len().max(1) as f64;
        let obj = rows.iter().map(|s| f64::from(s.objective_vp)).sum::<f64>() / n;
        let publics = rows.iter().map(|s| s.publics).sum::<usize>() as f64 / n;
        let secrets = rows.iter().map(|s| s.secrets).sum::<usize>() as f64 / n;
        let missed_pub = rows
            .iter()
            .filter(|s| s.had_public && s.publics == 0)
            .count();
        let missed_bought = rows
            .iter()
            .filter(|s| s.had_public && s.had_only_bought && s.publics == 0)
            .count();
        let missed_sec = rows
            .iter()
            .filter(|s| s.had_secret && s.secrets == 0)
            .count();
        let left = rows
            .iter()
            .filter(|s| (s.status_public && s.publics == 0) || (s.status_secret && s.secrets == 0))
            .count();
        for row in rows.iter().filter(|_| r + 1 >= from) {
            if row.had_public && row.publics == 0 {
                for name in row.seen_names.iter().filter(|n| !n.starts_with("s:")) {
                    *missed_names.entry(name.clone()).or_default() += 1;
                }
            }
            if row.had_secret && row.secrets == 0 {
                for name in row.seen_names.iter().filter(|n| n.starts_with("s:")) {
                    *missed_names.entry(name.clone()).or_default() += 1;
                }
            }
        }
        total_missed_pub += missed_pub;
        total_missed_sec += missed_sec;
        total_left += left;
        eprintln!(
            "OBJ r{:<2} | {vp:5.2} | {obj:5.2} | {publics:4.2} | {secrets:4.2} | {missed_pub:4} ({missed_bought:3}) | {missed_sec:4} | {left:4} | {} | {ended}",
            r + 1,
            rows.len()
        );
    }
    eprintln!(
        "OBJ TOTAL missed public {total_missed_pub}, missed secret {total_missed_sec}, left at status {total_left}"
    );
    let mut list: Vec<(&String, &usize)> = missed_names.iter().collect();
    list.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    let text: Vec<String> = list.iter().map(|(n, c)| format!("{n}:{c}")).collect();
    eprintln!("OBJ MISSED from round {from} {}", text.join(" "));
    let mut list: Vec<(&String, &usize)> = names.iter().collect();
    list.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    let text: Vec<String> = list.iter().map(|(n, c)| format!("{n}:{c}")).collect();
    eprintln!("OBJ SCORED {}", text.join(" "));
}
