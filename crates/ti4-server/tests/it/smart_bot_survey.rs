//! A survey of whole smart-bot games, for tuning the heuristics: `cargo test --test
//! smart_bot_survey -- --ignored --nocapture`. Plays a fixed set of tables (all-smart 3, 4 and 6
//! seats, and smart against random) and prints per-round standings and a summary per seat kind.
//! Set `SURVEY_DUMP=<dir>` to also write each game's decision log as JSON lines, and
//! `SURVEY_SEEDS=1,2,3` to play all-smart 3, 4 and 6-seat tables on those seeds instead, and
//! `SURVEY_MIXED_SEEDS=1,2,3` to choose the smart-against-random tables' seeds.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::time::{Duration, Instant};

use ti4_content::ContentStore;
use ti4_model::id::PlayerId;
use ti4_model::state::GameState;
use ti4_server::session::{GameSession, SeatController, SessionConfig};

fn config_with(id: &str, seed: u64, controllers: &[SeatController]) -> SessionConfig {
    let ids: Vec<PlayerId> = (1..=controllers.len())
        .map(|i| PlayerId::new(format!("p{i}")))
        .collect();
    let (state, galaxy) = ti4_host::map::create_game_with_template_seated(
        ContentStore::embedded(),
        &ids,
        seed,
        None,
        &ti4_engine::seating::seat_in_scope(&ids),
        Some("te"),
    )
    .expect("create game");
    let tiles = ti4_host::map::build_board_tiles(ContentStore::embedded(), &galaxy);
    let mut config = SessionConfig::new(id, state)
        .with_seed(seed)
        .with_player_ids(ids.clone())
        .with_galaxy(galaxy, tiles)
        .with_random_bot_delay_ms(0);
    for (id, controller) in ids.into_iter().zip(controllers) {
        config = config.with_seat(id, controller.clone());
    }
    config
}

fn line(state: &GameState) -> String {
    let mut out = format!("  r{} {:?}:", state.round, state.phase);
    for seat in &state.players {
        let planets = state.controlled_planets(&seat.id).len();
        let ships = state
            .board
            .values()
            .flat_map(|s| s.units.iter())
            .filter(|u| u.owner == seat.id)
            .count();
        // Ground forces anywhere (space and planets), by id: infantry and mechs of every faction.
        let ground = state
            .board
            .values()
            .flat_map(|s| s.units.iter().chain(s.planet_units.values().flatten()))
            .filter(|u| u.owner == seat.id)
            .filter(|u| {
                let id = u.type_id.as_str();
                id.contains("infantry") || id.contains("mech")
            })
            .count();
        let _ = write!(
            out,
            " {}[{}vp {}pl {}u {}g tg{} t{}/f{}/s{} sc{:?}]",
            seat.id,
            seat.victory_points,
            planets,
            ships,
            ground,
            seat.trade_goods,
            seat.tactic_tokens,
            seat.fleet_tokens,
            seat.strategic_tokens,
            seat.strategy_cards
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
        );
    }
    out
}

fn survey(id: &str, seed: u64, seats: &[SeatController], max_round: u32) -> GameState {
    let session = GameSession::start(config_with(id, seed, seats));
    let start = Instant::now();
    let mut round = 0;
    let mut last_len = 0;
    let mut last_progress = Instant::now();
    loop {
        let state = session.current_state();
        if state.round != round {
            round = state.round;
            eprintln!("{}", line(&state));
        }
        let len = session.decision_log().len();
        if len != last_len {
            last_len = len;
            last_progress = Instant::now();
        }
        if session.is_finished()
            || session.error().is_some()
            || state.round > max_round
            || last_progress.elapsed() > Duration::from_secs(60)
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    session.stop();
    let state = session.current_state();
    eprintln!("{}", line(&state));
    eprintln!(
        "{id}: seed {seed} finished={} error={:?} decisions={} in {:.0?}",
        session.is_finished(),
        session.error(),
        session.decision_log().len(),
        start.elapsed()
    );
    if let Ok(dir) = std::env::var("SURVEY_DUMP") {
        let mut text = String::new();
        for record in session.decision_log() {
            text.push_str(&serde_json::to_string(&record).expect("json"));
            text.push('\n');
        }
        std::fs::write(format!("{dir}/{id}.jsonl"), text).expect("write dump");
        std::fs::write(
            format!("{dir}/{id}.state.json"),
            serde_json::to_string(&state).expect("json"),
        )
        .expect("write state");
    }
    state
}

#[test]
#[ignore = "a long survey for tuning, not a check"]
fn survey_smart_bot_games() {
    let only = std::env::var("SURVEY_ONLY").ok();
    let max_round: u32 = std::env::var("SURVEY_ROUNDS")
        .ok()
        .and_then(|r| r.parse().ok())
        .unwrap_or(12);
    let smart = SeatController::SMART;
    let random = SeatController::RANDOM;
    let mut tables: Vec<(String, u64, Vec<SeatController>)> = Vec::new();
    let seeds: Option<Vec<u64>> = std::env::var("SURVEY_SEEDS").ok().map(|list| {
        list.split(',')
            .filter_map(|s| s.trim().parse().ok())
            .collect()
    });
    let all_smart: Vec<(usize, u64)> = seeds.as_ref().map_or_else(
        || vec![(3, 5), (3, 23), (4, 7), (4, 31), (6, 13), (6, 47)],
        |seeds| {
            seeds
                .iter()
                .flat_map(|&seed| [3, 4, 6].map(|n| (n, seed)))
                .collect()
        },
    );
    for (n, seed) in all_smart {
        tables.push((format!("smart{n}_{seed}"), seed, vec![smart.clone(); n]));
    }
    let mixed_seeds: Vec<u64> = std::env::var("SURVEY_MIXED_SEEDS").ok().map_or_else(
        || {
            if seeds.is_some() {
                vec![]
            } else {
                vec![3, 17, 29]
            }
        },
        |list| {
            list.split(',')
                .filter_map(|s| s.trim().parse().ok())
                .collect()
        },
    );
    for seed in mixed_seeds {
        for swap in [false, true] {
            let seats: Vec<SeatController> = (0..4)
                .map(|i| {
                    if (i % 2 == 0) == swap {
                        random.clone()
                    } else {
                        smart.clone()
                    }
                })
                .collect();
            tables.push((format!("mixed_{seed}_{swap}"), seed, seats));
        }
    }
    let (mut smart_vp, mut random_vp, mut smart_seats, mut random_seats) = (0, 0, 0, 0);
    let (mut smart_wins, mut random_wins) = (0, 0);
    let (mut games, mut to_ten, mut end_rounds) = (0, 0, 0);
    for (id, seed, seats) in tables {
        if only.as_deref().is_some_and(|o| !id.starts_with(o)) {
            continue;
        }
        let state = survey(&id, seed, &seats, max_round);
        let best = state
            .players
            .iter()
            .map(|p| p.victory_points)
            .max()
            .unwrap_or(0);
        games += 1;
        to_ten += usize::from(best >= 10);
        end_rounds += state.round;
        eprintln!("RESULT {id} round {} best {best}", state.round);
        for (seat, controller) in state.players.iter().zip(&seats) {
            let won = state.finished && seat.victory_points == best;
            if *controller == SeatController::SMART {
                smart_vp += seat.victory_points;
                smart_seats += 1;
                smart_wins += usize::from(won && id.starts_with("mixed"));
            } else {
                random_vp += seat.victory_points;
                random_seats += 1;
                random_wins += usize::from(won);
            }
        }
    }
    eprintln!("GAMES {games}: {to_ten} reached 10 VP, rounds played {end_rounds}");
    eprintln!(
        "SUMMARY smart {smart_vp} VP over {smart_seats} seats ({smart_wins} mixed wins); random {random_vp} VP over {random_seats} seats ({random_wins} wins)"
    );
}

/// One seat after a round's action phase: systems and planets gained since the start of the game,
/// and a note.
struct SeatRow {
    faction: String,
    cards: String,
    planets: usize,
    systems: usize,
    /// Draft position (`pos1` is the speaker), tactic tokens left and the round's activations
    /// (`a`, `w` for Warfare's free one).
    note: String,
}

/// A table after each of its opening rounds: the seats, and who held Mecatol Rex.
struct RoundRow {
    seats: Vec<SeatRow>,
    mecatol: Option<String>,
}

/// One all-smart table played through `rounds` rounds, then replayed up to the end of each round's
/// action phase, so each board is exactly the one that round left behind.
#[expect(
    clippy::too_many_lines,
    reason = "one table, set up, played and read back"
)]
fn opening(seats: usize, seed: u64, roster: bool, rounds: u32) -> Vec<RoundRow> {
    let ids: Vec<PlayerId> = (1..=seats)
        .map(|i| PlayerId::new(format!("p{i}")))
        .collect();
    // Factions from the whole roster by seed (`+fac`), or the default in-scope seating.
    let assignments = match ti4_host::preset::roster("explore+fac", seed, &ids) {
        Some(assignments) if roster => assignments,
        _ => ti4_engine::seating::seat_in_scope(&ids),
    };
    let (mut initial, galaxy) = ti4_host::map::create_game_with_template_seated(
        ContentStore::embedded(),
        &ids,
        seed,
        None,
        &assignments,
        Some("te"),
    )
    .expect("create game");
    // The speaker (first draft pick) rotates with the seed, so no seat always drafts first; only
    // this survey's tables, not the game setup.
    let speaker = usize::try_from(seed).unwrap_or(0) % seats;
    initial.speaker = ids[speaker].clone();
    let tiles = ti4_host::map::build_board_tiles(ContentStore::embedded(), &galaxy);
    let mut config = SessionConfig::new(format!("opening_{seats}_{seed}"), initial.clone())
        .with_seed(seed)
        .with_player_ids(ids.clone())
        .with_galaxy(galaxy.clone(), tiles)
        .with_random_bot_delay_ms(0);
    for id in &ids {
        config = config.with_seat(id.clone(), SeatController::SMART);
    }
    let session = GameSession::start(config);
    let start = Instant::now();
    loop {
        let state = session.current_state();
        if session.is_finished()
            || session.error().is_some()
            || state.round > rounds
            || start.elapsed() > Duration::from_secs(180)
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    session.stop();
    let log = session.decision_log();
    let mut rows = Vec::new();
    let mut begin = 0;
    for round in 1..=rounds {
        let end = log
            .iter()
            .position(|r| {
                r.context.as_ref().is_some_and(|c| {
                    c.round > round
                        || (c.round == round
                            && !matches!(
                                c.phase,
                                ti4_model::state::Phase::Strategy | ti4_model::state::Phase::Action
                            ))
                })
            })
            .unwrap_or(log.len());
        if let Ok(dir) = std::env::var("SURVEY_DUMP") {
            let mut text = String::new();
            for record in &log[begin..end] {
                text.push_str(&serde_json::to_string(record).expect("json"));
                text.push('\n');
            }
            std::fs::write(format!("{dir}/opening_{seats}_{seed}_r{round}.jsonl"), text)
                .expect("write dump");
        }
        let state = match ti4_host::replay::replay_session(&initial, Some(&galaxy), &log[..end]) {
            Ok(report) => report.final_state,
            Err(error) => {
                // A replay that diverges says nothing about the bot: the table is left out.
                eprintln!("REPLAY seed {seed} round {round} failed: {error}");
                break;
            }
        };
        if let Ok(system) = std::env::var("SURVEY_SHOW_SYSTEM") {
            eprintln!(
                "SYSTEM r{round} {system}: {}",
                serde_json::to_string(&state.board.get(&ti4_model::id::SystemId::new(&system)))
                    .expect("json")
            );
        }
        let mecatol = state
            .board
            .values()
            .find_map(|system| {
                system
                    .planet_control
                    .get(&ti4_model::id::PlanetId::new("mr"))
            })
            .and_then(|owner| state.player(owner))
            .map(|seat| seat.faction.as_str().to_owned());
        let window = &log[begin..end];
        let seats_rows = state
            .players
            .iter()
            .map(|seat| {
                let home: BTreeSet<_> = initial
                    .controlled_planets(&seat.id)
                    .into_iter()
                    .map(|(s, p)| (s.clone(), p.clone()))
                    .collect();
                let gained: Vec<_> = state
                    .controlled_planets(&seat.id)
                    .into_iter()
                    .filter(|(s, p)| !home.contains(&((*s).clone(), (*p).clone())))
                    .collect();
                let systems: BTreeSet<_> = gained.iter().map(|(s, _)| (*s).clone()).collect();
                let asked = |subtype: &str| {
                    window
                        .iter()
                        .filter(|r| r.player == seat.id)
                        .filter(|r| r.context.as_ref().is_some_and(|c| c.subtype == subtype))
                        .count()
                };
                let index = ids.iter().position(|id| id == &seat.id).unwrap_or(0);
                SeatRow {
                    faction: seat.faction.as_str().to_owned(),
                    cards: seat
                        .strategy_cards
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join("+"),
                    planets: gained.len(),
                    systems: systems.len(),
                    note: format!(
                        "pos{} vp{} t{} a{} w{}",
                        (index + seats - speaker) % seats + 1,
                        seat.victory_points,
                        seat.tactic_tokens,
                        asked("activate_system"),
                        asked("warfare_free_tactical")
                    ),
                }
            })
            .collect();
        rows.push(RoundRow {
            seats: seats_rows,
            mecatol,
        });
        begin = end;
    }
    rows
}

/// How many planets and systems each smart seat holds after the opening rounds of a 4-player game:
/// `SURVEY_TURN_ONE=1-48 cargo test --release --test smart_bot_survey survey_turn_one --
/// --ignored --nocapture`. `SURVEY_TURN_ONE_ROUNDS` (default 1) rounds are played; for each the
/// systems gained since the start (and within the round) are reported per seat, draft position,
/// faction and cards, with who holds Mecatol Rex. `SURVEY_TURN_ONE_SEATS` for another table size;
/// factions come from the whole roster by seed, `SURVEY_TURN_ONE_FACTIONS=base` seats the
/// default ones.
#[test]
#[ignore = "a survey for tuning, not a check"]
#[expect(
    clippy::cast_precision_loss,
    clippy::too_many_lines,
    reason = "small counts; one report"
)]
fn survey_turn_one() {
    let spec = std::env::var("SURVEY_TURN_ONE").unwrap_or_else(|_| "1-40".to_owned());
    let seats: usize = std::env::var("SURVEY_TURN_ONE_SEATS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(4);
    let rounds: u32 = std::env::var("SURVEY_TURN_ONE_ROUNDS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);
    let roster = std::env::var("SURVEY_TURN_ONE_FACTIONS").as_deref() != Ok("base");
    let seeds: Vec<u64> = spec
        .split(',')
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
        .collect();
    let results: Vec<(u64, Vec<RoundRow>)> = std::thread::scope(|scope| {
        let chunks: Vec<Vec<u64>> = seeds
            .chunks(seeds.len().div_ceil(6).max(1))
            .map(<[u64]>::to_vec)
            .collect();
        let handles: Vec<_> = chunks
            .into_iter()
            .map(|chunk| {
                scope.spawn(move || {
                    chunk
                        .into_iter()
                        .map(|seed| (seed, opening(seats, seed, roster, rounds)))
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|h| h.join().expect("survey thread"))
            .collect()
    });
    for round in 1..=rounds {
        let r = usize::try_from(round - 1).unwrap_or(0);
        let mut systems_hist = [0usize; 8];
        let mut gained_hist = [0usize; 6];
        let mut planets_hist = [0usize; 10];
        let mut by_faction: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        let mut by_card: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        let mut by_position: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        let mut mecatol: BTreeMap<String, usize> = BTreeMap::new();
        for (seed, rows) in &results {
            let Some(row) = rows.get(r) else { continue };
            let previous = r.checked_sub(1).and_then(|p| rows.get(p));
            let mut line = format!("seed {seed:3}:");
            for (i, seat) in row.seats.iter().enumerate() {
                let before = previous.map_or(0, |p| p.seats[i].systems);
                let new = seat.systems.saturating_sub(before);
                let _ = write!(
                    line,
                    " {}[{}] {}s/{}p +{new}s ({});",
                    seat.faction, seat.cards, seat.systems, seat.planets, seat.note
                );
                systems_hist[seat.systems.min(7)] += 1;
                gained_hist[new.min(5)] += 1;
                planets_hist[seat.planets.min(9)] += 1;
                by_faction
                    .entry(seat.faction.clone())
                    .or_default()
                    .push(new);
                by_card.entry(seat.cards.clone()).or_default().push(new);
                let position = seat.note.split(' ').next().unwrap_or_default().to_owned();
                by_position.entry(position).or_default().push(new);
            }
            let holder = row.mecatol.clone().unwrap_or_else(|| "-".to_owned());
            let _ = write!(line, " MR {holder}");
            *mecatol.entry(holder).or_default() += 1;
            eprintln!("R{round} {line}");
        }
        let total: usize = gained_hist.iter().sum();
        let at_least = |n: usize| gained_hist[n..].iter().sum::<usize>();
        eprintln!(
            "R{round} GAINED systems this round 0..5+: {gained_hist:?} over {total} seats; >=3: {} ({:.0}%)",
            at_least(3),
            100.0 * at_least(3) as f64 / total.max(1) as f64
        );
        eprintln!("R{round} HELD systems since start 0..7+: {systems_hist:?}");
        eprintln!("R{round} HELD planets since start 0..9+: {planets_hist:?}");
        eprintln!("R{round} MECATOL holders: {mecatol:?}");
        for (label, map) in [
            ("POSITION", &by_position),
            ("FACTION", &by_faction),
            ("CARD", &by_card),
        ] {
            for (key, list) in map {
                let mean = list.iter().sum::<usize>() as f64 / list.len() as f64;
                let three = list.iter().filter(|&&s| s >= 3).count();
                eprintln!(
                    "R{round} {label} {key:24} n={:2} mean gained {mean:.2} >=3: {three}",
                    list.len()
                );
            }
        }
    }
}

/// One seat's standing at the end of a round (its status phase done).
#[derive(Clone, Debug, serde::Serialize)]
struct MidRow {
    seed: u64,
    round: u32,
    faction: String,
    vp: i32,
    planets: usize,
    techs: usize,
    /// Expected hits per combat round of its ships, sustain counted twice (fighters included).
    strength: f64,
    /// Printed cost of its ships on the board.
    fleet_cost: f64,
    ground: usize,
    /// Non-fighter ships by base type, e.g. `dreadnought:2`.
    ships: BTreeMap<String, usize>,
    winner: bool,
}

fn mid_rows(state: &GameState, seed: u64, round: u32) -> Vec<MidRow> {
    let content = ContentStore::embedded();
    let sources = ti4_model::content_types::DEFAULT;
    let best = state
        .players
        .iter()
        .map(|p| p.victory_points)
        .max()
        .unwrap_or(0);
    state
        .players
        .iter()
        .map(|seat| {
            let (mut strength, mut fleet_cost, mut ground) = (0.0, 0.0, 0);
            let mut ships: BTreeMap<String, usize> = BTreeMap::new();
            for system in state.board.values() {
                for unit in system
                    .units
                    .iter()
                    .chain(system.planet_units.values().flatten())
                {
                    if unit.owner != seat.id {
                        continue;
                    }
                    let Some(kind) =
                        ti4_content::units::unit_type(content, unit.type_id.as_str(), sources)
                    else {
                        continue;
                    };
                    if kind.is_ground_force() && !kind.is_ship() {
                        ground += 1;
                    }
                    if !kind.is_ship() {
                        continue;
                    }
                    #[expect(clippy::cast_precision_loss, reason = "dice counts are small")]
                    let dice = kind.combat_dice() as f64;
                    let hits = kind
                        .combat_hits_on()
                        .map_or(0.0, |on| ti4_policy::fleet_strength::hit_chance(on, 0));
                    strength += dice * hits * (1.0 + f64::from(u8::from(kind.sustain_damage())));
                    fleet_cost += kind.cost();
                    if !kind.is_fighter() {
                        *ships.entry(kind.base_type().to_owned()).or_default() += 1;
                    }
                }
            }
            MidRow {
                seed,
                round,
                faction: seat.faction.as_str().to_owned(),
                vp: seat.victory_points,
                planets: state.controlled_planets(&seat.id).len(),
                techs: seat.technologies.len(),
                strength,
                fleet_cost,
                ground,
                ships,
                winner: state.finished && seat.victory_points == best,
            }
        })
        .collect()
}

/// One all-smart 4-seat table with roster factions by seed, sampled at the end of every round.
fn midgame(seats: usize, seed: u64, rounds: u32) -> (Vec<MidRow>, bool, u32) {
    let ids: Vec<PlayerId> = (1..=seats)
        .map(|i| PlayerId::new(format!("p{i}")))
        .collect();
    let assignments =
        ti4_host::preset::roster("explore+fac", seed, &ids).expect("roster assignment");
    let (mut initial, galaxy) = ti4_host::map::create_game_with_template_seated(
        ContentStore::embedded(),
        &ids,
        seed,
        None,
        &assignments,
        Some("te"),
    )
    .expect("create game");
    let speaker = usize::try_from(seed).unwrap_or(0) % seats;
    initial.speaker = ids[speaker].clone();
    let tiles = ti4_host::map::build_board_tiles(ContentStore::embedded(), &galaxy);
    let mut config = SessionConfig::new(format!("midgame_{seats}_{seed}"), initial)
        .with_seed(seed)
        .with_player_ids(ids.clone())
        .with_galaxy(galaxy, tiles)
        .with_random_bot_delay_ms(0);
    for id in &ids {
        config = config.with_seat(id.clone(), SeatController::SMART);
    }
    let session = GameSession::start(config);
    let mut rows = Vec::new();
    let mut seen_round = 1;
    let mut last_len = 0;
    let mut last_progress = Instant::now();
    loop {
        let state = session.current_state();
        // The first look at a new round is the previous round's end (status and agenda done).
        while state.round > seen_round {
            rows.extend(mid_rows(&state, seed, seen_round));
            seen_round += 1;
        }
        let len = session.decision_log().len();
        if len != last_len {
            last_len = len;
            last_progress = Instant::now();
        }
        if session.is_finished()
            || session.error().is_some()
            || state.round > rounds
            || last_progress.elapsed() > Duration::from_secs(60)
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    session.stop();
    let state = session.current_state();
    if state.finished {
        // The final position, apart from the end-of-round rows (round 100).
        rows.extend(mid_rows(&state, seed, 100));
    }
    if let Some(error) = session.error() {
        eprintln!("MIDGAME seed {seed} error {error:?}");
    }
    (rows, state.finished, state.round)
}

/// Mid- and late-game standings of all-smart 4-player tables, per faction, at the end of rounds
/// 3 to 5 (or to `SURVEY_MID_ROUNDS`): `SURVEY_MID=1-24 cargo test --release --test
/// smart_bot_survey survey_midgame -- --ignored --nocapture`. Factions come from the whole roster
/// by seed. `SURVEY_MID_OUT=<file>` writes every row as JSON lines for comparing two builds.
#[test]
#[ignore = "a survey for tuning, not a check"]
#[expect(
    clippy::cast_precision_loss,
    clippy::too_many_lines,
    reason = "small counts; one report"
)]
fn survey_midgame() {
    let spec = std::env::var("SURVEY_MID").unwrap_or_else(|_| "1-24".to_owned());
    let rounds: u32 = std::env::var("SURVEY_MID_ROUNDS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(5);
    let seats: usize = std::env::var("SURVEY_MID_SEATS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(4);
    let threads: usize = std::env::var("SURVEY_MID_THREADS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(4);
    let seeds: Vec<u64> = spec
        .split(',')
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
        .collect();
    let queue = std::sync::Mutex::new(seeds.clone());
    let results: Vec<(u64, Vec<MidRow>, bool, u32)> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..threads)
            .map(|_| {
                scope.spawn(|| {
                    let mut out = Vec::new();
                    loop {
                        let Some(seed) = queue.lock().expect("queue").pop() else {
                            break;
                        };
                        let (rows, finished, round) = midgame(seats, seed, rounds);
                        eprintln!("MIDGAME seed {seed} finished={finished} round {round}");
                        out.push((seed, rows, finished, round));
                    }
                    out
                })
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|h| h.join().expect("survey thread"))
            .collect()
    });
    let rows: Vec<&MidRow> = results.iter().flat_map(|(_, rows, _, _)| rows).collect();
    if let Ok(path) = std::env::var("SURVEY_MID_OUT") {
        let mut text = String::new();
        for row in &rows {
            text.push_str(&serde_json::to_string(row).expect("json"));
            text.push('\n');
        }
        std::fs::write(path, text).expect("write rows");
    }
    let finished = results.iter().filter(|(_, _, f, _)| *f).count();
    let end_rounds: Vec<u32> = results
        .iter()
        .filter(|(_, _, f, _)| *f)
        .map(|(_, _, _, r)| *r)
        .collect();
    let winners: Vec<&str> = rows
        .iter()
        .filter(|r| r.round == 100 && r.winner)
        .map(|r| r.faction.as_str())
        .collect();
    eprintln!(
        "MID GAMES {}: {finished} reached a winner (end rounds {end_rounds:?}; winners {winners:?})",
        results.len()
    );
    let mean = |list: &[&MidRow], f: &dyn Fn(&MidRow) -> f64| {
        list.iter().map(|r| f(r)).sum::<f64>() / list.len().max(1) as f64
    };
    for round in 3..=rounds {
        let at: Vec<&MidRow> = rows.iter().copied().filter(|r| r.round == round).collect();
        if at.is_empty() {
            continue;
        }
        let mut by_faction: BTreeMap<&str, Vec<&MidRow>> = BTreeMap::new();
        for row in &at {
            by_faction
                .entry(row.faction.as_str())
                .or_default()
                .push(row);
        }
        let line = |label: &str, list: &[&MidRow]| {
            let mut ships: BTreeMap<&str, usize> = BTreeMap::new();
            for row in list {
                for (kind, n) in &row.ships {
                    *ships.entry(kind.as_str()).or_default() += n;
                }
            }
            let ships: Vec<String> = ships
                .iter()
                .map(|(k, n)| format!("{}{:.1}", &k[..2], *n as f64 / list.len() as f64))
                .collect();
            eprintln!(
                "MID r{round} {label:10} n={:3} vp {:.2} pl {:.2} tech {:.2} str {:.2} cost {:5.1} gr {:.1} [{}]",
                list.len(),
                mean(list, &|r| f64::from(r.vp)),
                mean(list, &|r| r.planets as f64),
                mean(list, &|r| r.techs as f64),
                mean(list, &|r| r.strength),
                mean(list, &|r| r.fleet_cost),
                mean(list, &|r| r.ground as f64),
                ships.join(" ")
            );
        };
        line("ALL", &at);
        for (faction, list) in &by_faction {
            line(faction, list);
        }
    }
}
