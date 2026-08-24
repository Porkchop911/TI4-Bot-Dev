//! What actually gets scored, split by deck, with the condition each objective asks for.
//!
//! Public objectives are revealed to the table, so the honest denominator is "revealed"; a public
//! that is never revealed and one that is revealed and never met are different problems. Secrets
//! are held in hand, so their denominator is "drawn by this seat" instead. The two are reported
//! separately because they are measured differently.
//!
//! Imperial's primary is tracked alongside: it opens a "score a public objective with Imperial"
//! window whenever anything is scoreable, then awards a point for Mecatol Rex or draws a secret.
//! That window is an extra scoring opportunity outside the status phase and nothing has counted it.
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use ti4_content::ContentStore;
use ti4_engine::choice::{Choice, ChoiceOption, Decider, IllegalChoice};
use ti4_model::content_types::FULL;
use ti4_model::id::{FactionId, PlayerId};
use ti4_policy::inference::LearnedBot;
use ti4_policy::learned::Profile;
use ti4_training::rollout::{Horizon, OpeningMap, play_with_deciders};

const FACTIONS: [&str; 6] = ["sol", "letnev", "xxcha", "hacan", "jolnar", "l1z1x"];
const TILE_SEED_OFFSET: u64 = 20_000_000;
const IMPERIAL_PROMPT: &str = "score a public objective with Imperial";

#[derive(Default)]
struct Tally {
    revealed: BTreeSet<String>,
    scored: BTreeSet<String>,
    /// Secrets this seat ever held, so a secret's denominator is "drawn", not "revealed".
    held: BTreeSet<String>,
    imperial_offered: usize,
    imperial_scored: usize,
    imperial_declined: usize,
    /// Whether the seat held Mecatol when Imperial's window opened.
    imperial_with_mecatol: usize,
}

struct SeeBot {
    inner: LearnedBot,
    player: PlayerId,
    tally: Rc<RefCell<Tally>>,
}

impl Decider for SeeBot {
    fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
        self.inner.choose(choice)
    }

    fn choose_seeing(
        &mut self,
        choice: &Choice,
        seen: &ti4_engine::choice::SeatObservation<'_>,
    ) -> Result<ChoiceOption, IllegalChoice> {
        let imperial = choice.prompt == IMPERIAL_PROMPT;
        let mecatol = imperial
            && seen
                .controlled_planets(&self.player)
                .into_iter()
                .any(|(system, _)| system.as_str() == "18");
        {
            let mut tally = self.tally.borrow_mut();
            for id in seen.revealed_objectives() {
                tally.revealed.insert(id.to_string());
            }
            for id in seen.scored_by(&self.player) {
                tally.scored.insert(id.to_string());
            }
            let state = seen.held_state();
            if let Some(seat) = state.player(&self.player) {
                for secret in &seat.secret_objectives {
                    tally.held.insert(secret.to_string());
                }
            }
            if imperial {
                tally.imperial_offered += 1;
                if mecatol {
                    tally.imperial_with_mecatol += 1;
                }
            }
        }
        let answer = self.inner.choose_seeing(choice, seen)?;
        if imperial {
            let mut tally = self.tally.borrow_mut();
            if answer.is_decline() {
                tally.imperial_declined += 1;
            } else {
                tally.imperial_scored += 1;
            }
        }
        Ok(answer)
    }
}

fn argument(name: &str) -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

/// alias -> (name, points, condition text)
fn catalogue(file: &str) -> BTreeMap<String, (String, i64, String)> {
    let list: Vec<serde_json::Value> = serde_json::from_str(file).expect("objectives");
    list.into_iter()
        .filter_map(|o| {
            Some((
                o.get("alias")?.as_str()?.to_owned(),
                (
                    o.get("name")?.as_str()?.to_owned(),
                    o.get("points").and_then(serde_json::Value::as_i64).unwrap_or(1),
                    o.get("text")?.as_str()?.replace('\n', " "),
                ),
            ))
        })
        .collect()
}

#[expect(clippy::too_many_lines, reason = "one probe, three tables, kept visible")]
fn main() {
    let content = ContentStore::embedded();
    let checkpoint = argument("--checkpoint").expect("--checkpoint");
    let rounds: u32 = argument("--rounds").and_then(|v| v.parse().ok()).unwrap_or(4);
    let seeds: u64 = argument("--seeds").and_then(|v| v.parse().ok()).unwrap_or(25);
    let pool_path = argument("--map-pool")
        .unwrap_or_else(|| "out/pools/full_np8_12_holdout.json".to_owned());

    let publics = catalogue(include_str!(
        "../../ti4-content/content/public_objectives.json"
    ));
    let secrets = catalogue(include_str!(
        "../../ti4-content/content/secret_objectives.json"
    ));

    let document: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&checkpoint).expect("read")).expect("parse");
    let loaded: BTreeMap<String, Profile> =
        serde_json::from_value(document["profiles"].clone()).expect("profiles");
    let pool =
        std::sync::Arc::new(ti4_sim::MapPool::load(std::path::Path::new(&pool_path)).expect("pool"));

    let mut games = 0usize;
    let mut seats = 0usize;
    let mut revealed: BTreeMap<String, usize> = BTreeMap::new();
    let mut scored: BTreeMap<String, usize> = BTreeMap::new();
    let mut held: BTreeMap<String, usize> = BTreeMap::new();
    let (mut imp_offered, mut imp_scored, mut imp_declined, mut imp_mecatol) = (0, 0, 0, 0);

    for seed in 98_000_000..98_000_000 + seeds {
        for rotation in 0..FACTIONS.len() {
            let players: Vec<PlayerId> = (0..FACTIONS.len())
                .map(|index| PlayerId::new(format!("seat{index}")))
                .collect();
            let mut factions = BTreeMap::new();
            for (index, player) in players.iter().enumerate() {
                factions.insert(
                    player.clone(),
                    FactionId::new(FACTIONS[(index + rotation) % FACTIONS.len()]),
                );
            }
            let mut deciders: BTreeMap<PlayerId, Box<dyn Decider>> = BTreeMap::new();
            let mut tallies: BTreeMap<PlayerId, Rc<RefCell<Tally>>> = BTreeMap::new();
            for (index, player) in players.iter().enumerate() {
                let profile = loaded[&factions[player].to_string()].clone();
                let stream = seed
                    .wrapping_mul(1_000_003)
                    .wrapping_add(u64::try_from(index).unwrap_or(0));
                let tally = Rc::new(RefCell::new(Tally::default()));
                deciders.insert(
                    player.clone(),
                    Box::new(SeeBot {
                        inner: LearnedBot::from_shared(std::sync::Arc::new(profile), stream),
                        player: player.clone(),
                        tally: Rc::clone(&tally),
                    }),
                );
                tallies.insert(player.clone(), tally);
            }
            let map = OpeningMap::PythonPool {
                pool: std::sync::Arc::clone(&pool),
                tile_seed_offset: TILE_SEED_OFFSET,
            };
            let rollout = play_with_deciders(
                content,
                &players,
                &factions,
                FULL,
                seed,
                Horizon::rounds(rounds),
                ti4_engine::opening::DEFAULT_REQUIREMENT,
                &map,
                deciders,
            );
            if rollout.error.is_some() {
                continue;
            }
            games += 1;
            let mut board: BTreeSet<String> = BTreeSet::new();
            for tally in tallies.values() {
                seats += 1;
                let tally = tally.borrow();
                board.extend(tally.revealed.iter().cloned());
                for id in &tally.scored {
                    *scored.entry(id.clone()).or_default() += 1;
                }
                for id in &tally.held {
                    *held.entry(id.clone()).or_default() += 1;
                }
                imp_offered += tally.imperial_offered;
                imp_scored += tally.imperial_scored;
                imp_declined += tally.imperial_declined;
                imp_mecatol += tally.imperial_with_mecatol;
            }
            for id in board {
                *revealed.entry(id).or_default() += 1;
            }
        }
    }

    #[expect(clippy::cast_precision_loss, reason = "small counts")]
    let pct = |value: usize, of: usize| 100.0 * value as f64 / of.max(1) as f64;
    println!("{games} games ({seats} seats), {rounds} rounds, pool {pool_path}");
    println!("checkpoint {checkpoint}\n");

    // A public revealed in one game is an opportunity for all six seats, so the rate is scored
    // seats over revealed-games x 6, not over revealed-games.
    println!("PUBLIC OBJECTIVES  (revealed = games face up; scored = seats; rate = of the 6 seats per revealed game)");
    println!("{:<28}{:>4}{:>10}{:>9}{:>8}  {}", "objective", "pts", "revealed", "scored", "per-rev", "condition");
    let mut rows: Vec<_> = publics.iter().collect();
    rows.sort_by_key(|(alias, (_, points, _))| (*points, usize::MAX - revealed.get(*alias).copied().unwrap_or(0)));
    for (alias, (name, points, text)) in rows {
        let shown = revealed.get(alias).copied().unwrap_or(0);
        let got = scored.get(alias).copied().unwrap_or(0);
        if shown == 0 && got == 0 {
            println!("{name:<28}{points:>4}{:>10}{:>9}{:>8}  {text}", "-", "-", "-");
        } else {
            println!(
                "{name:<28}{points:>4}{shown:>10}{got:>9}{:>7.0}%  {text}",
                pct(got, shown * FACTIONS.len())
            );
        }
    }

    println!("\nSECRET OBJECTIVES  (drawn = seats that held it; scored = seats that took it)");
    println!("{:<30}{:>4}{:>8}{:>9}{:>9}  {}", "objective", "pts", "drawn", "scored", "per-draw", "condition");
    let mut rows: Vec<_> = secrets.iter().collect();
    rows.sort_by_key(|(alias, _)| usize::MAX - held.get(*alias).copied().unwrap_or(0));
    for (alias, (name, points, text)) in rows {
        let drawn = held.get(alias).copied().unwrap_or(0);
        let got = scored.get(alias).copied().unwrap_or(0);
        if drawn == 0 && got == 0 {
            continue;
        }
        println!(
            "{name:<30}{points:>4}{drawn:>8}{got:>9}{:>8.0}%  {text}",
            pct(got, drawn)
        );
    }

    println!("\nIMPERIAL PRIMARY");
    println!("  scoring windows opened:        {imp_offered}  ({:.2} per game)", {
        #[expect(clippy::cast_precision_loss, reason = "small counts")]
        let per = imp_offered as f64 / games.max(1) as f64;
        per
    });
    println!("  scored an objective:           {imp_scored}  ({:.0}% of windows)", pct(imp_scored, imp_offered));
    println!("  declined:                      {imp_declined}  ({:.0}%)", pct(imp_declined, imp_offered));
    println!("  held Mecatol at that moment:   {imp_mecatol}  ({:.0}% -- these gain the extra point, the rest draw a secret)", pct(imp_mecatol, imp_offered));
}
