//! What the engine implements, by content area, counted rather than asserted.
//!
//! Every area that can name its gaps already does, through an `unimplemented`/`registered_aliases`
//! pair kept beside the registry it reports on. Nothing gathered them in one place, so the overall
//! shape of the gap has never been stated -- only the per-area tests that say "some are missing".
//!
//! This prints the counts for a rules audit to work from. It is a report, not a gate: an area at
//! 0% is a fact about scope, not a defect, and the audit says which ones matter.

use ti4_content::ContentStore;
use ti4_model::content_types::{ContentType, DEFAULT, SourceSet};

const FACTIONS: [&str; 6] = ["sol", "letnev", "xxcha", "hacan", "jolnar", "l1z1x"];

fn row(area: &str, implemented: usize, total: usize) {
    let share = if total == 0 {
        "   --".to_owned()
    } else {
        #[expect(clippy::cast_precision_loss, reason = "content counts are small")]
        let value = 100.0 * implemented as f64 / total as f64;
        format!("{value:5.1}%")
    };
    println!("  {area:<22} {implemented:>5} of {total:>5}   {share}");
}

fn count_of(content: &ContentStore, kind: ContentType, sources: SourceSet) -> usize {
    content
        .records(kind)
        .iter()
        .filter(|record| record.in_sources(sources))
        .count()
}

// One long straight-line report: every line is a `row` for one content type, and cutting it into
// sections would put a function boundary where the report has no section.
#[allow(clippy::too_many_lines)]
fn main() {
    let content = ContentStore::embedded();
    let sources = DEFAULT;

    println!("engine content coverage (sources: DEFAULT = everything including Thunder's Edge)");
    println!();
    println!("  area                   implemented        share");

    let cards = count_of(content, ContentType::ActionCards, sources);
    let missing_cards = ti4_engine::action_cards::unimplemented(content).len();
    row("action cards", cards.saturating_sub(missing_cards), cards);

    let agendas = count_of(content, ContentType::Agendas, sources);
    let missing_agendas = ti4_engine::agenda_effects::unimplemented(content, sources).len();
    row("agendas", agendas.saturating_sub(missing_agendas), agendas);

    let laws_missing = ti4_engine::laws::unimplemented(content, sources).len();
    println!("  {:<22} {:>5} unenforced once in play", "  of which laws", laws_missing);

    let explores = count_of(content, ContentType::Explores, sources);
    let missing_explores = ti4_engine::exploration::unimplemented(content, sources).len();
    row("exploration cards", explores.saturating_sub(missing_explores), explores);

    let relics = count_of(content, ContentType::Relics, sources);
    let missing_relics = ti4_engine::relics::unimplemented(content, sources).len();
    row("relics", relics.saturating_sub(missing_relics), relics);

    let secrets = count_of(content, ContentType::SecretObjectives, sources);
    let missing_secrets = ti4_engine::secrets::unimplemented(content, sources).len();
    row("secret objectives", secrets.saturating_sub(missing_secrets), secrets);

    // Two families, and counting only one of them was wrong. `registered_aliases` holds the
    // counting and position objectives; the ten "spend N" cards are implemented through
    // `bought_aliases`/`cost_of` instead, and reporting 30 of 40 said ten cards were missing when
    // every one of them worked. `requirement_for` is the authority -- it is what the scorer asks --
    // so the count is taken from it rather than from either list.
    let objectives: Vec<String> = content
        .records(ContentType::PublicObjectives)
        .iter()
        .filter(|record| record.in_sources(sources))
        .filter_map(|record| record.text("alias").map(std::borrow::ToOwned::to_owned))
        .collect();
    // `scoreable_on` is the authority, and it accepts either family: a bought objective is offered
    // when `cost_of` prices it, everything else when `requirement_for` can decide it. Counting
    // either list alone reported ten working cards as missing.
    let scored = objectives
        .iter()
        .filter(|alias| {
            let id = ti4_model::id::ObjectiveId::new((*alias).clone());
            ti4_engine::objectives::requirement_for(&id).is_some()
                || ti4_engine::objectives::cost_of(&id).is_some()
        })
        .count();
    row("public objectives", scored, objectives.len());

    let abilities_missing = ti4_engine::faction_abilities::unimplemented(content, sources).len();
    let abilities = count_of(content, ContentType::Abilities, sources);
    row("faction abilities", abilities.saturating_sub(abilities_missing), abilities);

    let leaders_missing = ti4_engine::leaders::unimplemented(content, &FACTIONS).len();
    println!(
        "  {:<22} {:>5} unimplemented across the six trained factions",
        "leaders", leaders_missing
    );

    // The same, restricted to the six trained factions. Faction content for factions nobody plays
    // is not a gap in a six-player game with a fixed roster -- counting it makes the shortfall look
    // four times larger than the work actually required.
    println!();
    println!("  scoped to sol, letnev, xxcha, hacan, jolnar, l1z1x");
    println!();
    println!("  area                   implemented        share");

    let faction_of = |kind: ContentType, alias: &str| -> Option<String> {
        content
            .records(kind)
            .iter()
            .find(|record| record.text("alias") == Some(alias) || record.text("id") == Some(alias))
            .and_then(|record| record.text("faction"))
            .map(str::to_ascii_lowercase)
    };
    let is_ours = |faction: Option<String>| -> bool {
        faction.is_some_and(|name| FACTIONS.contains(&name.as_str()))
    };

    let ours_abilities = content
        .records(ContentType::Abilities)
        .iter()
        .filter(|record| record.in_sources(sources))
        .filter(|record| {
            record
                .text("faction")
                .is_some_and(|f| FACTIONS.contains(&f.to_ascii_lowercase().as_str()))
        })
        .count();
    let missing_ours_abilities = ti4_engine::faction_abilities::unimplemented(content, sources)
        .into_iter()
        .filter(|alias| is_ours(faction_of(ContentType::Abilities, alias)))
        .count();
    row(
        "faction abilities",
        ours_abilities.saturating_sub(missing_ours_abilities),
        ours_abilities,
    );

    let ours_leaders = content
        .records(ContentType::Leaders)
        .iter()
        .filter(|record| record.in_sources(sources))
        .filter(|record| {
            record
                .text("faction")
                .is_some_and(|f| FACTIONS.contains(&f.to_ascii_lowercase().as_str()))
        })
        .count();
    let missing_ours_leaders = ti4_engine::leaders::unimplemented(content, &FACTIONS).len();
    row(
        "leaders",
        ours_leaders.saturating_sub(missing_ours_leaders),
        ours_leaders,
    );

    let ours_breakthroughs = content
        .records(ContentType::Breakthroughs)
        .iter()
        .filter(|record| record.in_sources(sources))
        .filter(|record| {
            record
                .text("faction")
                .is_some_and(|f| FACTIONS.contains(&f.to_ascii_lowercase().as_str()))
        })
        .count();
    // Counted from the registry and filtered by scope, not written as a literal: the previous
    // version said 2 with a comment explaining that only two were read anywhere, which was true
    // when it was written and had drifted to 5 without the report noticing.
    let implemented_breakthroughs = ti4_engine::breakthroughs::registered_aliases()
        .into_iter()
        .filter(|alias| {
            content
                .get(ContentType::Breakthroughs, alias)
                .is_some_and(|record| record.in_sources(sources))
        })
        .count();
    row("breakthroughs", implemented_breakthroughs, ours_breakthroughs);

    let windows = ti4_engine::reactions::unsupported_windows();
    println!(
        "  {:<22} {:>5} reaction windows unsupported",
        "reaction windows",
        windows.len()
    );
    for (window, why) in &windows {
        println!("      {window:<28} {why}");
    }
}
