//! Regenerates `tests/objective_baseline.json` for the M09-021 pinning test.
//!
//! The fixture is a fully deployed three-player board with four revealed public objectives and
//! two held secrets on seat "a". Two choices are extracted: one mixed-kind (`StateCross::ByKind`)
//! and one binary strategy secondary (`StateCross::ByOption`). Every option's full feature vector
//! is dumped as `{name: value}`. The pinning test asserts that, after the objective-feature work,
//! every name not containing `":objective-"` still matches this dump exactly — the legacy factual
//! subvector is unchanged by construction of the comparison, and any later drift in legacy
//! emission fails the pin.

use std::collections::BTreeMap;

use ti4_content::ContentStore;
use ti4_engine::choice::{Choice, ChoiceOption, Observed};
use ti4_model::content_types::POK;
use ti4_model::id::{FactionId, ObjectiveId, PlayerId, SecretObjectiveId};

fn fixture() -> (ti4_model::state::GameState, ti4_content::galaxy::Galaxy) {
    let content = ContentStore::embedded();
    let players = ["a", "b", "c"].map(PlayerId::new);
    let factions: BTreeMap<PlayerId, FactionId> = players
        .iter()
        .cloned()
        .zip(["letnev", "jolnar", "hacan"].map(FactionId::new))
        .collect();
    let mut state =
        ti4_engine::setup::start_game_seeded(content, &players, POK, None, 17).expect("setup");
    for (player, faction) in &factions {
        state.player_mut(player).unwrap().faction = faction.clone();
    }
    let filler: Vec<String> = ti4_engine::seating::map_filler(content, 30, POK, 17)
        .into_iter()
        .map(|system| system.to_string())
        .collect();
    let refs: Vec<&str> = filler.iter().map(String::as_str).collect();
    let galaxy = ti4_engine::seating::build_board(content, &factions, &refs, POK).unwrap();
    for (player, faction) in &factions {
        ti4_engine::seating::deploy(&mut state, content, player, faction, POK).unwrap();
    }

    // Four revealed publics: two counting families with distinct stages and one bought card.
    state.revealed_objectives = vec![
        ObjectiveId::new("outer_rim"),
        ObjectiveId::new("diversify"),
        ObjectiveId::new("unify_colonies"),
        ObjectiveId::new("trade_routes"),
    ];
    // Two held secrets on seat "a" only.
    state
        .player_mut(&PlayerId::new("a"))
        .unwrap()
        .secret_objectives = vec![SecretObjectiveId::new("otf"), SecretObjectiveId::new("mlp")];
    (state, galaxy)
}

fn main() {
    let content = ContentStore::embedded();
    let (state, galaxy) = fixture();
    let seen = Observed::new(&state, content, POK, Some(&galaxy));
    let player = PlayerId::new("a");

    // Choice 0: mixed kinds -> StateCross::ByKind.
    let choice_0 = Choice::new(
        player.clone(),
        "activate a system or pass",
        vec![
            ChoiceOption::new("18", "activate"),
            ChoiceOption::labelled("no", "decline", "pass"),
        ],
    );
    // Choice 1: uniform kind, small fixed-vocabulary set -> StateCross::ByOption.
    let choice_1 = Choice::new(
        player.clone(),
        "spend a strategy token to replenish commodities",
        vec![
            ChoiceOption::labelled("no", "strategy", "decline"),
            ChoiceOption::labelled("yes", "strategy", "replenish"),
        ],
    );

    // Offline context with full state: the held-secret records are computed explicitly and
    // passed to the feature path — the same data live play receives bound to its SeatObservation.
    let held =
        ti4_engine::choice::held_secret_progress(&state, content, POK, Some(&galaxy), &player);
    let mut dump = Vec::new();
    for (choice_index, choice) in [choice_0, choice_1].into_iter().enumerate() {
        let vectors =
            ti4_policy::features::explicit_choice_features(&seen, &choice, &player, &held);
        assert_eq!(vectors.len(), choice.options.len());
        for (option_index, vector) in vectors.into_iter().enumerate() {
            let mut features = BTreeMap::new();
            for (key, value) in vector.iter() {
                features.insert(ti4_policy::intern::name_of(*key), *value);
            }
            dump.push(serde_json::json!({
                "choice": choice_index,
                "option": option_index,
                "features": features,
            }));
        }
    }

    let out = format!(
        "{}\n",
        serde_json::to_string_pretty(&dump).expect("serialises")
    );
    let path = format!(
        "{}/tests/objective_baseline.json",
        env!("CARGO_MANIFEST_DIR")
    );
    std::fs::write(&path, out).expect("writes the baseline fixture");
    println!(
        "wrote tests/objective_baseline.json ({} entries)",
        dump.len()
    );
}
