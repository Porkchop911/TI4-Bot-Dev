//! Survey: which objectives can a generated map never satisfy?
//!
//! `cargo run -p ti4-server --example objective_feasibility -- [seeds]` (default 100 seeds).
//! For every player count 3..=6, the seeded random board and every template that builds, over the
//! seeds, counts the maps on which each objective is ruled out by the map alone (see
//! `ti4_engine::map_feasibility`). Prints one block per board kind.

use std::collections::BTreeMap;

use ti4_content::ContentStore;
use ti4_engine::map_feasibility::{census, unsatisfiable_public, unsatisfiable_secret};
use ti4_engine::seating;
use ti4_model::content_types::POK;
use ti4_model::id::{PlayerId, SystemId};
use ti4_server::maps::{TemplateLoader, build_template_galaxy};

fn main() {
    let seeds: u64 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(100);
    let content = ContentStore::embedded();
    let loader = TemplateLoader::load().unwrap();
    for n in 3..=6usize {
        let ids: Vec<PlayerId> = (0..n).map(|i| PlayerId::new(format!("p{i}"))).collect();
        let assignments = seating::seat_in_scope_rotated(&ids, 0);
        let homes: Vec<SystemId> = ids
            .iter()
            .map(|p| {
                let one = BTreeMap::from([(p.clone(), assignments[p].clone())]);
                seating::home_systems(content, &one).unwrap().remove(0)
            })
            .collect();
        let by_seat: BTreeMap<PlayerId, _> = ids
            .iter()
            .enumerate()
            .map(|(i, p)| {
                (
                    PlayerId::new(format!("seat_{i:02}")),
                    assignments[p].clone(),
                )
            })
            .collect();

        let mut kinds: Vec<(String, Vec<Option<ti4_content::galaxy::Galaxy>>)> = Vec::new();
        let random = (0..seeds)
            .map(|seed| {
                let filler = seating::map_filler(content, 36, POK, seed);
                let refs: Vec<&str> = filler.iter().map(SystemId::as_str).collect();
                seating::build_board(content, &by_seat, &refs, POK).ok()
            })
            .collect();
        kinds.push((format!("{n}p random board"), random));
        for t in loader.templates_for(n) {
            let maps: Vec<_> = (0..seeds)
                .map(|seed| build_template_galaxy(content, t, &homes, seed, POK).ok())
                .collect();
            if maps.iter().any(Option::is_some) {
                kinds.push((format!("{n}p template {}", t.alias), maps));
            }
        }
        for (name, maps) in kinds {
            let built: Vec<_> = maps.iter().flatten().collect();
            let mut pub_hits: BTreeMap<&str, usize> = BTreeMap::new();
            let mut sec_hits: BTreeMap<&str, usize> = BTreeMap::new();
            let (mut empty, mut anomalies, mut alpha, mut beta, mut legendary) = (0, 0, 0, 0, 0);
            let mut any_hit = 0usize;
            for galaxy in &built {
                let c = census(content, POK, galaxy, &homes);
                empty += c.planetless;
                anomalies += c.anomalies;
                alpha += c.alpha;
                beta += c.beta;
                legendary += c.legendary;
                let p = unsatisfiable_public(&c);
                let s = unsatisfiable_secret(&c);
                any_hit += usize::from(!p.is_empty() || !s.is_empty());
                for a in p {
                    *pub_hits.entry(a).or_default() += 1;
                }
                for a in s {
                    *sec_hits.entry(a).or_default() += 1;
                }
            }
            let total = built.len().max(1);
            println!(
                "{name}: {} maps; mean empty {:.1}, anomalies {:.1}, alpha {:.1}, beta {:.1}, legendary {:.1}; maps with any unsatisfiable objective: {any_hit}",
                built.len(),
                empty as f64 / total as f64,
                anomalies as f64 / total as f64,
                alpha as f64 / total as f64,
                beta as f64 / total as f64,
                legendary as f64 / total as f64,
            );
            for (a, k) in &pub_hits {
                println!("    public {a}: impossible on {k}/{}", built.len());
            }
            for (a, k) in &sec_hits {
                println!("    secret {a}: impossible on {k}/{}", built.len());
            }
        }
    }
}
