//! Temporary probe: the work queue for action-card effects.
//!
//! Intersection of cards the engine can actually play (window "Action", or a printed window
//! the reaction table maps) with cards that have no effect. Removed once the queue is
//! consumed.

use std::collections::BTreeSet;

use ti4_content::ContentStore;
use ti4_model::content_types::{ContentType, DEFAULT};
use ti4_model::id::ActionCardId;

fn window_of(content: &ContentStore, id: &ActionCardId) -> Option<String> {
    content
        .get(ContentType::ActionCards, id.as_str())
        .and_then(|record| record.text("window"))
        .map(str::trim)
        .map(str::to_owned)
}

fn main() {
    let content = ContentStore::embedded();

    let reachable: BTreeSet<ActionCardId> =
        ti4_engine::reactions::reachable(&content, DEFAULT).into_iter().collect();
    let unimplemented: BTreeSet<ActionCardId> =
        ti4_engine::action_cards::unimplemented(&content).into_iter().collect();

    // Action-window cards are playable whenever the action phase runs; reaction cards only if
    // their printed window is mapped.
    let playable = |id: &ActionCardId| {
        matches!(window_of(&content, id).as_deref(), Some("Action")) || reachable.contains(id)
    };

    let queue: Vec<&ActionCardId> =
        unimplemented.iter().filter(|id| playable(id)).collect();

    println!(
        "reaction-reachable: {}  unimplemented: {}  work queue (playable AND unimplemented): {}",
        reachable.len(),
        unimplemented.len(),
        queue.len()
    );

    println!("\nwork queue:");
    for id in &queue {
        let name = ti4_engine::action_cards::name_of(&content, id);
        let window = window_of(&content, id).unwrap_or_default();
        println!("  {:<22} {:<42} [{}]", id.as_str(), name, window);
    }

    println!("\nunmapped windows:");
    let windows = ti4_engine::reactions::unmapped_windows(&content, DEFAULT);
    for (window, count) in &windows {
        println!("  x{}  {}", count, window);
    }

    let blocked: Vec<&ActionCardId> = unimplemented
        .iter()
        .filter(|id| !playable(id))
        .collect();
    println!("\nblocked cards (unimplemented, window unmapped): {}", blocked.len());
    for id in &blocked {
        let name = ti4_engine::action_cards::name_of(&content, id);
        let window = window_of(&content, id).unwrap_or_default();
        println!("  {:<22} {:<42} [{}]", id.as_str(), name, window);
    }
}