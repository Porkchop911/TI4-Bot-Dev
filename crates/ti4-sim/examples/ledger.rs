//! Every coverage ledger this engine keeps, printed together.
fn main() {
    let content = ti4_content::ContentStore::embedded();
    let pok = ti4_model::content_types::DEFAULT;
    let scope = ti4_engine::seating::IN_SCOPE_FACTIONS;
    println!("{}", ti4_engine::registry::report(content, pok));
    println!(
        "leaders unimplemented (in scope): {:?}",
        ti4_engine::leaders::unimplemented(content, pok, &scope)
    );
    println!(
        "faction abilities blocked: {:?}",
        ti4_engine::faction_abilities::blocked()
            .keys()
            .collect::<Vec<_>>()
    );
    println!(
        "reaction windows unsupported: {}",
        ti4_engine::reactions::unsupported_windows().len()
    );
    println!(
        "bot choice kinds unscored: {:?}",
        ti4_policy::bot::unscored_kinds()
    );
}

#[cfg(test)]
mod tests {
    #[test]
    fn bf_twelve_have_no_missing_or_blocked_assets() {
        let content = ti4_content::ContentStore::embedded();
        let factions = [
            "sardakk", "winnu", "arborec", "yin", "mentak", "yssaril", "saar", "naalu", "muaat",
            "ghost", "argent", "naaz",
        ];
        let blocked = ti4_engine::faction_abilities::blocked();
        let sources = ti4_model::content_types::DEFAULT;
        for faction in factions {
            let assets = ti4_engine::factions::assets(content, sources, faction);
            assert!(!assets.is_empty(), "{faction}: empty corpus");
            let missing = ti4_engine::factions::missing(content, sources, faction);
            assert!(missing.is_empty(), "{faction}: {missing:?}");
            for asset in &assets {
                assert!(
                    !blocked.contains_key(asset.id.as_str()),
                    "{faction}: blocked {asset:?}"
                );
            }
            println!(
                "{sources:?} {faction}: {} assets, missing=0 blocked=0",
                assets.len()
            );
        }
        let mechs = ti4_engine::faction_abilities::unimplemented_mechs(content, sources, &factions);
        assert!(mechs.is_empty(), "missing mechs: {mechs:?}");
        let breakthroughs = ti4_engine::breakthroughs::unimplemented(content, sources, &factions);
        assert!(
            breakthroughs.is_empty(),
            "missing breakthroughs: {breakthroughs:?}"
        );

        let leaders = ti4_engine::leaders::unimplemented(
            content,
            ti4_model::content_types::DEFAULT,
            &factions,
        );
        assert!(leaders.is_empty(), "missing leaders: {leaders:?}");
    }
}