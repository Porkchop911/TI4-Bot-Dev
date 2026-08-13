//! What each content scope puts in a game, and which components resolve to a newer printing.
use ti4_content::ContentStore;
use ti4_model::content_types::{BASE, ContentType, DEFAULT, POK};
fn main() {
    let content = ContentStore::embedded();
    for (name, set) in [("BASE", BASE), ("POK", POK), ("DEFAULT (full)", DEFAULT)] {
        let count = |c: ContentType| content.from_sources(c, set).count();
        println!(
            "{name:16} action_cards {:>4}  relics {:>3}  leaders {:>3}  units {:>3}  systems {:>3}",
            count(ContentType::ActionCards),
            count(ContentType::Relics),
            count(ContentType::Leaders),
            count(ContentType::Units),
            count(ContentType::Systems),
        );
    }
    println!("\nwhat now resolves to a newer printing at full scope:");
    for (category, id) in [
        (ContentType::Leaders, "xxchahero"),
        (ContentType::Leaders, "naaluagent"),
        (ContentType::Units, "naalu_mech"),
        (ContentType::Leaders, "solhero"),
    ] {
        let at_pok = content.resolve_id(category, id, POK).unwrap_or("(none)");
        let at_full = content
            .resolve_id(category, id, DEFAULT)
            .unwrap_or("(none)");
        let note = if at_pok == at_full {
            ""
        } else {
            "   <- changed"
        };
        println!("  {id:14} POK -> {at_pok:18} FULL -> {at_full}{note}");
    }
}
