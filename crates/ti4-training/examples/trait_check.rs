//! Does the engine now see the traits the corpus records?
//!
//! Verification for the `planetType` / `planetTypes` fix: reads traits back through the engine's
//! own accessor rather than the JSON, so it fails if the accessor regresses.
use std::collections::BTreeMap;

use ti4_content::ContentStore;
use ti4_model::content_types::FULL;

fn main() {
    let store = ContentStore::embedded();
    let catalogue = ti4_content::galaxy::all_planets(store, FULL);
    // Source is not on the accessor, so it is read from the raw corpus purely for grouping.
    let sources: BTreeMap<String, String> = {
        let raw = include_str!("../../ti4-content/content/planets.json");
        let list: Vec<serde_json::Value> = serde_json::from_str(raw).expect("planets");
        list.into_iter()
            .filter_map(|p| {
                Some((
                    p.get("id")?.as_str()?.to_owned(),
                    p.get("source")?.as_str()?.to_owned(),
                ))
            })
            .collect()
    };
    let mut by_source: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut dual = 0usize;
    let mut listed: BTreeMap<String, usize> = BTreeMap::new();

    for (id, record) in &catalogue {
        let source = sources.get(*id).cloned().unwrap_or_else(|| "?".to_owned());
        let entry = by_source.entry(source).or_default();
        entry.0 += 1;
        if !record.traits().is_empty() {
            entry.1 += 1;
        }
        if record.traits().len() > 1 {
            dual += 1;
        }
        for kind in record.planet_types() {
            *listed.entry(kind.to_owned()).or_default() += 1;
        }
    }

    println!("planets the engine can read a trait for, by source:");
    for (source, (total, traited)) in &by_source {
        println!("  {source:<16} {traited:>3} of {total:>3}");
    }
    println!("\nplanets carrying two traits: {dual}");
    println!("\nevery value seen in the trait fields: {listed:?}");
}
