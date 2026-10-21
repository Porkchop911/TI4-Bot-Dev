//! Print the per-faction reward terms as JSON, straight from `faction_terms::TABLE`, so a review
//! page always shows what training pays. Usage: `cargo run -p ti4-training --example
//! faction_terms_report > terms.json`.

use ti4_training::faction_terms::{CAP_VP, TABLE, scale_of};

fn main() {
    let factions: Vec<serde_json::Value> = TABLE
        .iter()
        .map(|row| {
            let scale = scale_of(row);
            let terms: Vec<serde_json::Value> = row
                .terms
                .iter()
                .map(|t| {
                    let weight = t.weight * if t.exempt { 1.0 } else { scale };
                    serde_json::json!({
                        "id": t.id,
                        "goal": t.cond.describe(),
                        "listed_vp": t.weight,
                        "weight_vp": (weight * 1000.0).round() / 1000.0,
                        "deadline": t.deadline,
                        "kind": if t.once { "once" } else { "hold" },
                        "exempt": t.exempt,
                    })
                })
                .collect();
            serde_json::json!({
                "faction": row.faction,
                "home_doubled": row.home_doubled,
                "scale": (scale * 1000.0).round() / 1000.0,
                "terms": terms,
            })
        })
        .collect();
    let report = serde_json::json!({ "cap_vp": CAP_VP, "factions": factions });
    println!(
        "{}",
        serde_json::to_string_pretty(&report).expect("serialisable")
    );
}
