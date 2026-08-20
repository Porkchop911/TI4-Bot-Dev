//! What the policy actually researches, and whether transactions ever happen.
//!
//! The Technology secondary sits at 0% for every faction and the Trade secondary is thin, which
//! the card economics explain -- but "explained" is not "measured". Two things are worth knowing:
//! which technologies get researched when a seat does take the free primary, and whether a
//! transaction is ever opened at all. If transactions never fire, commodities are dead value and
//! every Trade secondary followed is waste.
use std::collections::BTreeMap;
use std::sync::Arc;

use ti4_content::ContentStore;
use ti4_model::content_types::FULL;
use ti4_model::id::FactionId;
use ti4_policy::learned::Profile;
use ti4_training::rollout::{Horizon, play_rotated_save54_pool_batch};

const POOL: &str = "out/pools/save52_e400_holdout.json.gz";

fn main() {
    let store = ContentStore::embedded();
    let factions: Vec<FactionId> = ["sol", "letnev", "xxcha", "hacan", "jolnar", "l1z1x"]
        .iter()
        .map(|name| FactionId::new(*name))
        .collect();
    let path = std::env::args()
        .skip(1)
        .find(|a| a.ends_with(".json"))
        .expect("checkpoint path");
    let rounds: u32 = std::env::args()
        .find_map(|a| a.strip_prefix("--rounds=").and_then(|v| v.parse().ok()))
        .unwrap_or(4);
    let seeds: u64 = std::env::args()
        .find_map(|a| a.strip_prefix("--seeds=").and_then(|v| v.parse().ok()))
        .unwrap_or(100);
    ti4_training::rollout::set_seat_scramble(true);

    let document: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).expect("read")).expect("parse");
    let loaded: BTreeMap<String, Profile> =
        serde_json::from_value(document["profiles"].clone()).expect("profiles");
    let profiles: BTreeMap<FactionId, Profile> = factions
        .iter()
        .map(|f| (f.clone(), loaded[f.as_str()].clone()))
        .collect();
    let pool = Arc::new(ti4_sim::MapPool::load(std::path::Path::new(POOL)).expect("pool"));
    let seed_block: Vec<u64> = (98_000_000..98_000_000 + seeds).collect();
    let games = play_rotated_save54_pool_batch(
        store,
        &factions,
        &profiles,
        FULL,
        &seed_block,
        Horizon::rounds(rounds),
        ti4_engine::opening::DEFAULT_REQUIREMENT,
        pool,
        20_000_000,
    );

    // Technology aliases from the content store, so a researched tech is recognised wherever the
    // engine asks for it rather than only under a head whose name happens to contain "tech".
    let tech_names: BTreeMap<String, String> = {
        let raw = include_str!("../../ti4-content/content/technologies.json");
        let list: Vec<serde_json::Value> = serde_json::from_str(raw).expect("technologies");
        list.iter()
            .filter_map(|t| {
                Some((
                    t.get("alias")?.as_str()?.to_owned(),
                    t.get("name")?.as_str()?.to_owned(),
                ))
            })
            .collect()
    };

    // Every head the policy was asked, so nothing is missed by guessing a name.
    let mut heads: BTreeMap<String, usize> = BTreeMap::new();
    // Technologies actually taken, by faction.
    let mut techs: BTreeMap<(String, String), usize> = BTreeMap::new();
    // Transaction heads: how often one was offered, and how often something other than "no".
    let mut offered: BTreeMap<String, usize> = BTreeMap::new();
    let mut accepted: BTreeMap<String, usize> = BTreeMap::new();
    let mut accepted_choices: BTreeMap<String, usize> = BTreeMap::new();
    // Which head each researched technology came through.
    let mut tech_heads: BTreeMap<String, usize> = BTreeMap::new();
    // Aliases short enough to collide with another head's option namespace, shown with the
    // features the policy actually saw, so an alias match can be confirmed or thrown out.
    let mut suspect: BTreeMap<String, usize> = BTreeMap::new();
    // Alias hits thrown out because they came through a head that does not research.
    let mut rejected_matches: BTreeMap<String, usize> = BTreeMap::new();
    // Any non-research head that nonetheless offers an option carrying the word "research".
    let mut other_research: BTreeMap<String, usize> = BTreeMap::new();

    for game in &games {
        for seat in &game.seats {
            let faction = seat.faction.to_string();
            for step in &seat.trajectory {
                *heads.entry(step.head.clone()).or_default() += 1;
                let head = step.head.as_str();
                if step.chosen == "mr" || step.chosen == "ss" || step.chosen == "gd" {
                    let words: Vec<String> = step
                        .legal
                        .get(&step.chosen)
                        .map(|v| {
                            v.iter()
                                .filter_map(|(slot, _)| {
                                    ti4_policy::intern::name_of(*slot)
                                        .strip_prefix("option:")
                                        .map(str::to_owned)
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    *suspect
                        .entry(format!("{head}/{} {words:?}", step.chosen))
                        .or_default() += 1;
                }
                // Alias matching alone is unsafe: most tech aliases are two or three letters and
                // collide with option ids in other namespaces ("mr" is Magmus Reactor, and also an
                // agenda vote target). Only the research head counts, and every other head that
                // offers a "research" option is reported separately so gating here cannot hide a
                // second research path.
                let option_words: Vec<String> = step.legal.get(&step.chosen).map_or_else(
                    Vec::new,
                    |vector| {
                        vector
                            .iter()
                            .filter_map(|(slot, _)| {
                                ti4_policy::intern::name_of(*slot)
                                    .strip_prefix("option:")
                                    .map(str::to_owned)
                            })
                            .collect()
                    },
                );
                if let Some(name) = tech_names.get(&step.chosen) {
                    if head == "development" {
                        *techs
                            .entry((faction.clone(), name.clone()))
                            .or_default() += 1;
                    } else {
                        *rejected_matches
                            .entry(format!("{head}/{} {:?}", step.chosen, option_words))
                            .or_default() += 1;
                    }
                    *tech_heads.entry(head.to_owned()).or_default() += 1;
                }
                if head != "development" && option_words.iter().any(|w| w == "research") {
                    *other_research
                        .entry(format!("{head}/{}", step.chosen))
                        .or_default() += 1;
                }
                if head.contains("transact") || head.contains("trade") || head.contains("deal")
                    || head.contains("commod") || head.contains("exchange")
                {
                    *offered.entry(head.to_owned()).or_default() += 1;
                    if !matches!(step.chosen.as_str(), "no" | "refuse" | "decline" | "cancel") {
                        *accepted.entry(head.to_owned()).or_default() += 1;
                        *accepted_choices.entry(step.chosen.clone()).or_default() += 1;
                    }
                }
            }
        }
    }

    println!("{} games, {rounds} rounds, checkpoint {path}", games.len());

    // What the "development" head actually is: the chosen option ids and the feature words the
    // policy saw on them. An alias match alone proves nothing -- "mr" and "gd" are two letters and
    // could belong to any head's option namespace.
    let mut dev: BTreeMap<String, usize> = BTreeMap::new();
    let mut dev_words: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
    let mut dev_offered: BTreeMap<String, usize> = BTreeMap::new();
    let mut dev_decisions = 0usize;
    let mut dev_round: BTreeMap<u32, usize> = BTreeMap::new();
    let mut tech_follow: BTreeMap<u32, usize> = BTreeMap::new();
    let mut follow_by_faction: BTreeMap<String, usize> = BTreeMap::new();
    let mut dev_by_faction: BTreeMap<String, usize> = BTreeMap::new();
    for game in &games {
        for seat in &game.seats {
            let seat_faction = seat.faction.to_string();
            for step in &seat.trajectory {
                // The Technology secondary is the accepting option labelled "spend" (strategy.rs
                // builds it as "spend a strategy token and 4 resources to research"); Leadership
                // also carries "spend" but pairs it with "influence".
                // Declines must be excluded first: Leadership's decline option is labelled
                // "spend nothing further", so its words contain "spend" without "influence" and
                // a naive match counts every Leadership refusal as a Technology follow.
                if step.head == "secondary" && step.chosen != "no" && step.chosen != "decline" {
                    let words: std::collections::BTreeSet<String> = step
                        .legal
                        .get(&step.chosen)
                        .map(|v| {
                            v.iter()
                                .filter_map(|(slot, _)| {
                                    ti4_policy::intern::name_of(*slot)
                                        .strip_prefix("option:")
                                        .map(str::to_owned)
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    if words.contains("spend") && !words.contains("influence") {
                        *tech_follow.entry(step.progress.round_number).or_default() += 1;
                        *follow_by_faction.entry(seat_faction.clone()).or_default() += 1;
                    }
                }
                if step.head == "development" {
                    *dev_round.entry(step.progress.round_number).or_default() += 1;
                    *dev_by_faction.entry(seat_faction.clone()).or_default() += 1;
                    // Offered as well as chosen: "never researched" and "never legal" are
                    // different problems, and only the legal set separates them.
                    for id in step.legal.keys() {
                        *dev_offered.entry(id.clone()).or_default() += 1;
                    }
                    dev_decisions += 1;
                    *dev.entry(step.chosen.clone()).or_default() += 1;
                    if let Some(vector) = step.legal.get(&step.chosen) {
                        let entry = dev_words.entry(step.chosen.clone()).or_default();
                        for (slot, _) in vector {
                            if let Some(name) = ti4_policy::intern::name_of(*slot).strip_prefix("option:") {
                                *entry.entry(name.to_owned()).or_default() += 1;
                            }
                        }
                    }
                }
            }
        }
    }
    println!("
RESEARCH DECISIONS vs TECHNOLOGY-SECONDARY FOLLOWS, by round");
    println!("{:<8}{:>14}{:>18}", "round", "development", "tech-secondary");
    for round in 1..=4 {
        println!(
            "{round:<8}{:>14}{:>18}",
            dev_round.get(&round).copied().unwrap_or(0),
            tech_follow.get(&round).copied().unwrap_or(0)
        );
    }
    println!(
        "{:<8}{:>14}{:>18}",
        "total",
        dev_round.values().sum::<usize>(),
        tech_follow.values().sum::<usize>()
    );

    println!("
PER FACTION: technology-secondary follows against research decisions raised");
    println!("{:<10}{:>10}{:>12}{:>14}", "faction", "follows", "research", "unresolved");
    for faction in ["sol", "letnev", "xxcha", "hacan", "jolnar", "l1z1x"] {
        let f = follow_by_faction.get(faction).copied().unwrap_or(0);
        let d = dev_by_faction.get(faction).copied().unwrap_or(0);
        println!("{faction:<10}{f:>10}{d:>12}{:>14}", i64::try_from(f).unwrap_or(0) - i64::try_from(d).unwrap_or(0));
    }

    println!("
DEVELOPMENT HEAD: offered vs chosen ({dev_decisions} research decisions)");
    println!("{:<12}{:>10}{:>10}{:>10}   {}", "option", "offered", "chosen", "take-rate", "name");
    {
        let mut rows: Vec<_> = dev_offered.iter().collect();
        rows.sort_by(|a, b| b.1.cmp(a.1));
        for (id, offered) in rows {
            let chosen = dev.get(id).copied().unwrap_or(0);
            #[expect(clippy::cast_precision_loss, reason = "counts are small")]
            let rate = 100.0 * chosen as f64 / (*offered).max(1) as f64;
            let name = tech_names.get(id).cloned().unwrap_or_else(|| String::new());
            println!("{id:<12}{offered:>10}{chosen:>10}{rate:>9.1}%   {name}");
        }
    }

    println!("
DEVELOPMENT HEAD: chosen option ids");
    let mut rows: Vec<_> = dev.iter().collect();
    rows.sort_by(|a, b| b.1.cmp(a.1));
    for (choice, n) in rows.into_iter().take(12) {
        let words: Vec<&String> = dev_words
            .get(choice)
            .map(|w| w.keys().take(10).collect())
            .unwrap_or_default();
        println!("  {choice:<14} {n:>6}   words: {words:?}");
    }

    println!("\nHEADS ASKED (decisions across the batch)");
    for (head, count) in &heads {
        println!("  {head:<28} {count:>8}");
    }

    println!("\nTECHNOLOGIES RESEARCHED (count over the batch)");
    if techs.is_empty() {
        println!("  none");
    } else {
        let mut totals: BTreeMap<String, usize> = BTreeMap::new();
        for ((_, tech), n) in &techs {
            *totals.entry(tech.clone()).or_default() += n;
        }
        println!("  by technology:");
        let mut rows: Vec<_> = totals.iter().collect();
        rows.sort_by(|a, b| b.1.cmp(a.1));
        for (tech, n) in rows {
            println!("    {tech:<40} {n:>6}");
        }
        println!("  asked through heads: {tech_heads:?}");
        println!("  by faction x technology:");
        for ((faction, tech), n) in &techs {
            println!("    {faction:<8} {tech:<40} {n:>6}");
        }
    }

    println!("
ALIAS HITS DISCARDED (matched a tech alias outside the research head)");
    for (key, n) in &rejected_matches {
        println!("  {n:>6}  {key}");
    }
    println!("
RESEARCH OPTIONS OUTSIDE THE RESEARCH HEAD");
    if other_research.is_empty() {
        println!("  none -- development is the only path");
    } else {
        for (key, n) in &other_research {
            println!("  {n:>6}  {key}");
        }
    }

    println!("
SUSPECT SHORT ALIASES (head/choice + option words)");
    for (key, n) in &suspect {
        println!("  {n:>6}  {key}");
    }

    println!("\nTRANSACTION-SHAPED HEADS");
    if offered.is_empty() {
        println!("  none asked");
    } else {
        for (head, n) in &offered {
            let yes = accepted.get(head).copied().unwrap_or(0);
            #[expect(clippy::cast_precision_loss, reason = "counts are small")]
            let rate = 100.0 * yes as f64 / *n as f64;
            println!("  {head:<28} asked {n:>7}  acted {yes:>7}  ({rate:.1}%)");
        }
        println!("  chosen options:");
        let mut rows: Vec<_> = accepted_choices.iter().collect();
        rows.sort_by(|a, b| b.1.cmp(a.1));
        for (choice, n) in rows.into_iter().take(25) {
            println!("    {choice:<48} {n:>6}");
        }
    }
}
