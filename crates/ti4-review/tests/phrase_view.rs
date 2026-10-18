//! The phrase resolver and the question-detail lines, on a played session (real seats, real board).

use std::path::PathBuf;

use ti4_engine::phrases::{Phrase, Subject};
use ti4_model::id::PlayerId;
use ti4_review::view::{details_lines, option_lines, say_phrase};
use ti4_review::{AdvanceUnit, LiveReview, ProfileTable, ReviewSession, SimulationConfig};

fn session() -> ReviewSession {
    let mut root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    while !root.join("examples/reviewer/checkpoint-473312/slots.json").is_file() {
        assert!(root.pop(), "no workspace root");
    }
    let config = SimulationConfig {
        checkpoint: root.join("examples/reviewer/checkpoint-473312/slots.json"),
        map_pool: root.join("examples/reviewer/full_np8_12_holdout.json"),
        seed: 4_242,
        rotation: 1,
        table: ProfileTable::Learner,
        temperature: 0.5,
        diplomacy: false,
        lineup: None,
        rule_seats: Vec::new(),
    };
    let mut review = LiveReview::start(&config).expect("start");
    review.advance(AdvanceUnit::Step, 2);
    review.session
}

#[test]
fn placeholders_become_names_a_reader_knows() {
    let session = session();
    let store = ti4_content::ContentStore::embedded();
    let record = store
        .records(ti4_model::content_types::ContentType::StrategyCards)
        .into_iter()
        .next()
        .expect("strategy cards exist");
    let (card_id, card_name) = (record.text("id").unwrap().to_owned(), record.text("name").unwrap().to_owned());
    let phrase = Phrase::default()
        .name(Subject::seat(&PlayerId::new("seat0")))
        .say(" loses ")
        .name(Subject::Units { unit_type: "cruiser".into(), count: 2 })
        .say(", ")
        .name(Subject::Units { unit_type: "infantry".into(), count: 1 })
        .say(" and ")
        .name(Subject::StrategyCard { card: card_id.clone() })
        .say(", not {missing}");
    let said = say_phrase(&session, &phrase);
    let faction = &session.manifest.factions[0];
    assert!(said.starts_with("seat0 "), "{said}");
    assert!(said.contains(&format!("seat0 \"")) || said.contains(faction), "{said}");
    assert!(said.contains("2 cruisers, 1 infantry"), "{said}");
    assert!(said.contains(&card_name), "{said}");
    // Text that only looks like a placeholder survives.
    assert!(said.ends_with(", not {missing}"), "{said}");
}

#[test]
fn fleet_supply_pools_and_capacity_read_as_numbers() {
    let session = session();
    let mut details = serde_json::Map::new();
    details.insert(
        "pools".into(),
        serde_json::json!({"tactic": 3, "fleet": 2, "strategic": 2, "fleet_supply_bonus": 2}),
    );
    details.insert(
        "fleet_supply".into(),
        serde_json::json!({"used": 4, "limit": 4, "fleet_supply_bonus": 2}),
    );
    let lines = details_lines(&session, "p", &details);
    assert!(lines.iter().any(|l| l.contains("tactic 3 · fleet 2 · strategic 2") && l.contains("+2")), "{lines:?}");
    assert!(lines.iter().any(|l| l.starts_with("Fleet supply here: 4/4") && l.contains("+2")), "{lines:?}");
    assert!(option_lines(&session, "x", "x", &details, &[]).is_empty());
}
