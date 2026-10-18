//! Fast checks of the self-play runner. Short games (2 rounds), so they stay quick in debug.
//! The full-length determinism check is `selfplay run --verify tools/rulebot/baseline.json`.

use std::collections::BTreeMap;

use ti4_rule_bot::tuning::Tuning;
use ti4_selfplay::driver::run_matrix;
use ti4_selfplay::run::compare;
use ti4_selfplay::spec::{Limits, MapPolicy, Matrix, Rotate, Seeds, SetupSpec};

fn setup(players: usize, seeds: Vec<u64>) -> SetupSpec {
    SetupSpec {
        name: None,
        players,
        maps: vec![],
        map_policy: MapPolicy::Cycle,
        factions: vec![],
        faction_pool: vec![],
        rotate: Rotate::None,
        speaker: "random".to_owned(),
        card_set: None,
        starting_tech: false,
        rules_version: None,
        seeds: Seeds::List(seeds),
    }
}

fn short_matrix() -> Matrix {
    Matrix {
        setups: vec![setup(4, vec![1, 2]), setup(3, vec![7])],
        limits: Limits {
            max_rounds: 2,
            ..Limits::default()
        },
        bot: BTreeMap::new(),
    }
}

/// What must not change between two runs: everything but the clock.
fn fingerprint(
    s: &ti4_selfplay::stats::Summary,
) -> Vec<(String, u64, String, Vec<i32>, usize, String)> {
    s.games
        .iter()
        .map(|g| {
            (
                g.spec.setup.clone(),
                g.spec.seed,
                g.termination.clone(),
                g.vp.clone(),
                g.decisions,
                g.hash.clone(),
            )
        })
        .collect()
}

#[test]
fn same_seeds_give_identical_games_on_any_number_of_workers_and_the_summary_has_its_schema() {
    let matrix = short_matrix();
    let (one, _) = run_matrix(&matrix, 1, false, false, None).unwrap();
    let (two, _) = run_matrix(&matrix, 2, false, false, None).unwrap();
    assert_eq!(fingerprint(&one), fingerprint(&two));
    assert_eq!(one.games.len(), 3);
    assert!(
        one.games
            .iter()
            .all(|g| g.decisions > 50 && !g.hash.is_empty())
    );
    // Different seeds must not be the same game.
    assert_ne!(one.games[0].hash, one.games[1].hash);

    let json = serde_json::to_value(&one).unwrap();
    for key in [
        "meta", "limits", "bot", "skipped", "combined", "setups", "problems", "games",
    ] {
        assert!(json.get(key).is_some(), "summary lacks {key}");
    }
    for key in [
        "games",
        "terminations",
        "rounds",
        "win_by_seat",
        "win_by_faction",
        "by_players",
        "by_map",
        "by_subtype",
        "completion",
        "cutoff_rate",
    ] {
        assert!(json["combined"].get(key).is_some(), "aggregate lacks {key}");
    }
    assert!(json["setups"].get("4p").is_some() && json["setups"].get("3p").is_some());
    // A cut-off game is a cutoff, not a problem.
    assert_eq!(one.combined.terminations.get("cutoff_rounds"), Some(&3));
    assert!(one.problems.is_empty());
    let text = ti4_selfplay::stats::render(&one, 5);
    assert!(text.contains("time to completion") && text.contains("win rate by seat"));
}

#[test]
fn starting_technology_is_asked_only_when_the_spec_says_so_and_the_bot_chooses_by_plan() {
    let mut asked = setup(4, vec![3]);
    asked.factions = vec!["winnu".to_owned(), "argent".to_owned()];
    let mut matrix = Matrix {
        setups: vec![asked.clone()],
        limits: Limits {
            max_rounds: 2,
            ..Limits::default()
        },
        bot: BTreeMap::new(),
    };
    let (plain, _) = run_matrix(&matrix, 1, false, false, None).unwrap();
    let unasked = plain.combined.by_subtype.get("starting_technology");
    assert!(unasked.is_none(), "a spec without the flag asks nothing");
    matrix.setups[0].starting_tech = true;
    let (with, _) = run_matrix(&matrix, 1, false, false, None).unwrap();
    let question = &with.combined.by_subtype["starting_technology"];
    assert_eq!(question.n, 3, "one Winnu pick, two Argent picks");
    assert_eq!(question.smart, 3, "the version 3 bot chooses by its plan");
    assert_ne!(plain.games[0].hash, with.games[0].hash);
    // Bot version 2 answers the same questions at random, as it answered every unruled one.
    matrix.bot.insert("bot_version".to_owned(), "2".to_owned());
    let (old, _) = run_matrix(&matrix, 1, false, false, None).unwrap();
    assert_eq!(old.combined.by_subtype["starting_technology"].smart, 0);
}

#[test]
fn an_unchanged_candidate_is_exactly_as_strong_as_the_baseline() {
    // Cut-off games are excluded from the comparison, so use a full game of a small table.
    let specs3 = setup(3, vec![5]).expand().unwrap().0;
    let (c, results) = compare(
        &specs3,
        &Limits::default(),
        Tuning::default(),
        Tuning::default(),
        true,
        2,
        false,
    );
    assert_eq!(results.len(), 2);
    assert_eq!(c.excluded, 0, "{results:?}");
    assert!(c.win_share_advantage.mean.abs() < 1e-12);
    assert!(c.vp_advantage.mean.abs() < 1e-12);
    assert_eq!(
        results[0].hash, results[1].hash,
        "equal bots play the same game whichever seats are 'candidate'"
    );
}

#[test]
fn a_changed_number_changes_the_game_and_an_unknown_key_is_refused() {
    let mut t = Tuning::default();
    assert!(t.set_key("no_such_key", "1").is_err());
    t.set_key("eager_worth", "0.5").unwrap();
    let specs = setup(4, vec![1]).expand().unwrap().0;
    let limits = Limits {
        max_rounds: 2,
        ..Limits::default()
    };
    let (c, r) = compare(&specs, &limits, Tuning::default(), t, false, 2, false);
    assert_eq!(c.mode, "separate");
    assert_ne!(r[0].hash, r[1].hash);
}
