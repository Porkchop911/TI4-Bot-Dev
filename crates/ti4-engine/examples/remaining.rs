//! Everything the engine still does not implement, by name.
//!
//! Diagnostic only, and the companion to `coverage_report`: that one gives the counts, this one
//! gives the list you actually work from. Every list comes from the module's own `unimplemented`
//! helper, so it cannot drift from the code the way a written-down checklist does.
//!
//! `cargo run -p ti4-engine --example remaining`

use ti4_content::ContentStore;
use ti4_model::content_types::DEFAULT;

/// The six factions this engine trains on. Faction content outside these is out of scope.
const FACTIONS: &[&str] = &["sol", "letnev", "xxcha", "hacan", "jolnar", "l1z1x"];

fn main() {
    let content = ContentStore::embedded();
    let sources = DEFAULT;

    show(
        "relics",
        ti4_engine::relics::unimplemented(content, sources)
            .iter()
            .map(ToString::to_string)
            .collect(),
    );
    show(
        "laws enacted but not enforced",
        ti4_engine::laws::unimplemented(content, sources),
    );
    show(
        "exploration cards",
        ti4_engine::exploration::unimplemented(content, sources),
    );
    show(
        "leaders",
        ti4_engine::leaders::unimplemented(content, FACTIONS)
            .iter()
            .map(ToString::to_string)
            .collect(),
    );
    show(
        "breakthroughs",
        ti4_engine::breakthroughs::unimplemented(content, sources, FACTIONS)
            .iter()
            .map(ToString::to_string)
            .collect(),
    );
    show(
        "mech abilities",
        ti4_engine::faction_abilities::unimplemented_mechs(content, sources, FACTIONS),
    );
    show(
        "action cards",
        ti4_engine::action_cards::unimplemented(content)
            .iter()
            .map(ToString::to_string)
            .collect(),
    );
}

fn show(label: &str, items: Vec<String>) {
    println!("\n{label} ({})", items.len());
    if items.is_empty() {
        println!("  --");
    }
    for item in items {
        println!("  {item}");
    }
}
