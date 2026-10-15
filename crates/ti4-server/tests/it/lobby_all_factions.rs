//! Every faction of the lobby pool ([`ti4_engine::seating::LOBBY_FACTIONS`]) as a chosen seat: the
//! game builds on every map template that seats the table, the seat starts with its units and home
//! planets, and rule bots play it. Also all factions undecided over many seeds, determinism by
//! seed, and a longer smart-bot game for the factions with special setup.

use std::collections::BTreeSet;
use std::time::{Duration, Instant};

use ti4_content::ContentStore;
use ti4_engine::seating::LOBBY_FACTIONS;
use ti4_host::maps::catalog::MapChoice;
use ti4_host::setup::{self, SpeakerOrder, TableSetup};
use ti4_model::id::PlayerId;
use ti4_model::state::GameState;
use ti4_server::session::{GameSession, SeatController, SessionConfig};

fn ids(n: usize) -> Vec<PlayerId> {
    (1..=n).map(|i| PlayerId::new(format!("p{i}"))).collect()
}

fn build(
    players: &[PlayerId],
    seed: u64,
    template: Option<&str>,
    chosen: &[Option<String>],
) -> Result<(GameState, ti4_content::galaxy::Galaxy), String> {
    // The players sit in the order given: `chosen[i]` is the faction of seat `i`.
    let table = TableSetup {
        map: MapChoice::from_stored(template),
        strategy_cards: setup::card_set::POK.to_owned(),
        speaker_order: SpeakerOrder::Random,
        ..TableSetup::new(players.len(), seed)
    };
    let drawn = setup::draw(&table, chosen)?;
    setup::create(ContentStore::embedded(), &table, players, &drawn)
}

fn check_seated(state: &GameState, players: &[PlayerId], chosen: &[Option<String>], what: &str) {
    let mut seen = BTreeSet::new();
    for (player, want) in players.iter().zip(chosen) {
        let seat = state.player(player).expect("seat");
        if let Some(want) = want {
            assert_eq!(seat.faction.as_str(), want, "{what}");
        }
        assert!(LOBBY_FACTIONS.contains(&seat.faction.as_str()), "{what}");
        assert!(
            seen.insert(seat.faction.as_str().to_owned()),
            "{what}: repeated faction"
        );
        let units = state
            .board
            .values()
            .flat_map(|s| s.units.iter().chain(s.planet_units.values().flatten()))
            .filter(|u| &u.owner == player)
            .count();
        assert!(
            units >= 3,
            "{what}: {} starts with {units} units",
            seat.faction
        );
        assert!(
            !state.controlled_planets(player).is_empty(),
            "{what}: {} controls no home planet",
            seat.faction
        );
        assert!(
            seat.fleet_tokens > 0 || seat.tactic_tokens > 0,
            "{what}: no command tokens"
        );
    }
}

/// Plays all-smart seats until `decisions` decisions were taken, a round is reached, or the
/// game ends; returns the final state and the session error, if any.
fn play(
    players: &[PlayerId],
    state: GameState,
    galaxy: ti4_content::galaxy::Galaxy,
    seed: u64,
    until_round: u32,
    decisions: usize,
    budget: Duration,
) -> (GameState, Option<String>, usize) {
    let tiles = ti4_host::map::build_board_tiles(ContentStore::embedded(), &galaxy);
    let mut config = SessionConfig::new(format!("allfac_{seed}"), state)
        .with_seed(seed)
        .with_player_ids(players.to_vec())
        .with_galaxy(galaxy, tiles)
        .with_random_bot_delay_ms(0);
    for id in players {
        config = config.with_seat(id.clone(), SeatController::SMART);
    }
    let session = GameSession::start(config);
    let start = Instant::now();
    let mut last = (0usize, Instant::now());
    loop {
        let now = session.current_state();
        let len = session.decision_log().len();
        if len != last.0 {
            last = (len, Instant::now());
        }
        if session.is_finished()
            || session.error().is_some()
            || now.round > until_round
            || len >= decisions
            || start.elapsed() > budget
            || last.1.elapsed() > Duration::from_secs(60)
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    session.stop();
    (
        session.current_state(),
        session.error().map(|e| format!("{e:?}")),
        session.decision_log().len(),
    )
}

#[test]
fn slow_every_faction_builds_as_a_chosen_seat_on_every_template_that_seats_the_table() {
    let mut combos = 0;
    for n in 3..=6usize {
        let players = ids(n);
        let mut maps: Vec<Option<String>> = vec![None];
        maps.extend(
            ti4_host::maps::catalog::catalog(Some(n))
                .expect("catalog")
                .into_iter()
                .map(|t| Some(t.alias)),
        );
        for map in &maps {
            for (i, faction) in LOBBY_FACTIONS.iter().enumerate() {
                // Rotate the chosen seat so every position (and fixed home slot) is tried.
                let seat = i % n;
                let mut chosen = vec![None; n];
                chosen[seat] = Some((*faction).to_owned());
                let seed = 1000 + i as u64;
                let what = format!("{faction} at seat {seat} of {n} on {map:?} seed {seed}");
                let (state, _) = build(&players, seed, map.as_deref(), &chosen)
                    .unwrap_or_else(|e| panic!("{what}: {e}"));
                check_seated(&state, &players, &chosen, &what);
                combos += 1;
            }
        }
    }
    assert!(combos > 24 * 4, "{combos}");
}

#[test]
fn slow_every_faction_chosen_at_a_four_seat_table_plays_smart_decisions() {
    let players = ids(4);
    for (i, faction) in LOBBY_FACTIONS.iter().enumerate() {
        let mut chosen = vec![None; 4];
        chosen[i % 4] = Some((*faction).to_owned());
        let seed = 40 + i as u64;
        let (state, galaxy) = build(&players, seed, None, &chosen).expect("build");
        check_seated(&state, &players, &chosen, faction);
        let (after, error, decisions) = play(
            &players,
            state,
            galaxy,
            seed,
            3,
            120,
            Duration::from_secs(120),
        );
        assert!(error.is_none(), "{faction}: {error:?}");
        assert!(decisions >= 40, "{faction}: only {decisions} decisions");
        assert_eq!(
            after.player(&players[i % 4]).unwrap().faction.as_str(),
            *faction
        );
    }
}

#[test]
fn slow_undecided_tables_over_many_seeds_seat_distinct_pool_factions_and_play() {
    for (n, seed) in (3..=6usize).flat_map(|n| (0..12u64).map(move |s| (n, s * 7 + n as u64))) {
        let players = ids(n);
        let chosen = vec![None; n];
        let (state, galaxy) = build(&players, seed, None, &chosen).expect("build");
        check_seated(&state, &players, &chosen, &format!("{n} seats seed {seed}"));
        // Deterministic by seed.
        let (again, _) = build(&players, seed, None, &chosen).expect("build");
        assert_eq!(
            serde_json::to_string(&state).unwrap(),
            serde_json::to_string(&again).unwrap(),
            "{n} seats seed {seed}"
        );
        if seed % 4 == 0 {
            let (_, error, decisions) = play(
                &players,
                state,
                galaxy,
                seed,
                2,
                50,
                Duration::from_secs(60),
            );
            assert!(error.is_none(), "{n} seats seed {seed}: {error:?}");
            assert!(decisions > 10, "{n} seats seed {seed}");
        }
    }
}

#[test]
fn slow_tricky_factions_play_a_longer_smart_game() {
    let tricky = [
        "nekro", "ghost", "winnu", "naaz", "titans", "mahact", "empyrean", "nomad", "cabal",
        "yssaril", "argent", "arborec",
    ];
    let players = ids(4);
    for (k, faction) in tricky.iter().enumerate() {
        let mut chosen = vec![None; 4];
        chosen[0] = Some((*faction).to_owned());
        // The pairing also seats another tricky faction beside it.
        chosen[1] = Some(tricky[(k + 5) % tricky.len()].to_owned());
        let seed = 900 + k as u64;
        let (state, galaxy) = build(&players, seed, None, &chosen).expect("build");
        let (after, error, decisions) = play(
            &players,
            state,
            galaxy,
            seed,
            5,
            usize::MAX,
            Duration::from_secs(150),
        );
        assert!(error.is_none(), "{faction}: {error:?}");
        assert!(
            after.round >= 3 || decisions > 400,
            "{faction}: round {} after {decisions}",
            after.round
        );
    }
}

#[test]
fn the_pool_is_the_survey_roster() {
    let roster: BTreeSet<&str> = ti4_host::preset::ROSTER.iter().copied().collect();
    let pool: BTreeSet<&str> = LOBBY_FACTIONS.iter().copied().collect();
    assert_eq!(roster, pool);
    // Plans exist for every faction in the pool.
    for faction in LOBBY_FACTIONS {
        assert!(
            ti4_rule_bot::faction_plans::plan_for(faction).is_some(),
            "{faction}"
        );
    }
}
