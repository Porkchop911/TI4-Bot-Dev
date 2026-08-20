//! How much of this is a fight?
//!
//! Every metric so far -- planets, systems, VP -- counts what a seat ended up with, not who it
//! took it from. A four-round land grab on a board with 31 gainable planets could be six players
//! expanding into empty space and never touching each other, which is a very different game from
//! the one the reward is nominally about. This separates the two.
//!
//! Planet control is read from state at every decision and each change is classified:
//!   * expansion -- an uncontrolled planet becomes controlled;
//!   * conquest  -- a planet passes from one player to another.
//! Combat decisions are counted from the trajectories, and unit losses from drops in each seat's
//! unit count.
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use ti4_content::ContentStore;
use ti4_engine::choice::{Choice, ChoiceOption, Decider, IllegalChoice, Observed};
use ti4_model::content_types::FULL;
use ti4_model::id::{FactionId, PlayerId};
use ti4_policy::inference::LearnedBot;
use ti4_policy::learned::{Profile, decision_head};
use ti4_training::rollout::{Horizon, OpeningMap, play_with_deciders};

const FACTIONS: [&str; 6] = ["sol", "letnev", "xxcha", "hacan", "jolnar", "l1z1x"];
const TILE_SEED_OFFSET: u64 = 20_000_000;

#[derive(Default)]
struct Ledger {
    /// planet -> the faction controlling it at the last observation.
    owner: BTreeMap<String, String>,
    /// Uncontrolled planet taken, by the faction taking it.
    expansion: BTreeMap<String, usize>,
    /// Planet taken off another player: (taker, loser).
    conquest: BTreeMap<(String, String), usize>,
    /// Each faction's unit count at the last observation, and the total drop seen.
    units: BTreeMap<String, usize>,
    losses: BTreeMap<String, usize>,
    /// Whether anything was ever taken by force in this game.
    any_conquest: bool,
    /// Every decision head the table raised, and combat decisions by faction.
    heads: BTreeMap<String, usize>,
    combat_by_faction: BTreeMap<String, usize>,
}

struct WatchBot {
    inner: LearnedBot,
    faction: String,
    players: Vec<PlayerId>,
    ledger: Rc<RefCell<Ledger>>,
}

impl Decider for WatchBot {
    fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
        self.inner.choose(choice)
    }

    fn choose_seeing(
        &mut self,
        choice: &Choice,
        seen: &Observed<'_>,
    ) -> Result<ChoiceOption, IllegalChoice> {
        // Heads are counted here rather than from the rollout trajectory: play_with_deciders
        // supplies no trajectory handles (rollout.rs:659), so seat.trajectory is empty on this
        // path and a tally taken from it would silently read zero.
        let mut ledger = self.ledger.borrow_mut();
        let head = decision_head(choice);
        *ledger.heads.entry(head.to_owned()).or_default() += 1;
        if head == "combat" {
            *ledger
                .combat_by_faction
                .entry(self.faction.clone())
                .or_default() += 1;
        }
        for player in &self.players {
            let faction = seen
                .seat(player)
                .map(|seat| seat.faction.to_string())
                .unwrap_or_default();
            for (_, planet) in seen.controlled_planets(player) {
                let key = planet.to_string();
                let previous = ledger.owner.get(&key).cloned();
                match previous {
                    Some(previous) if previous == faction => {}
                    Some(previous) => {
                        *ledger
                            .conquest
                            .entry((faction.clone(), previous))
                            .or_default() += 1;
                        ledger.any_conquest = true;
                        ledger.owner.insert(key, faction.clone());
                    }
                    None => {
                        *ledger.expansion.entry(faction.clone()).or_default() += 1;
                        ledger.owner.insert(key, faction.clone());
                    }
                }
            }
            let held = seen.units_held(player);
            if let Some(before) = ledger.units.get(&faction) {
                if held < *before {
                    *ledger.losses.entry(faction.clone()).or_default() += before - held;
                }
            }
            ledger.units.insert(faction, held);
        }
        drop(ledger);
        self.inner.choose_seeing(choice, seen)
    }
}

fn argument(name: &str) -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

#[expect(clippy::too_many_lines, reason = "one probe, the reporting kept visible")]
fn main() {
    let content = ContentStore::embedded();
    let checkpoint = argument("--checkpoint").expect("--checkpoint");
    let rounds: u32 = argument("--rounds").and_then(|v| v.parse().ok()).unwrap_or(4);
    let seeds: u64 = argument("--seeds").and_then(|v| v.parse().ok()).unwrap_or(20);
    let pool_path = argument("--map-pool")
        .unwrap_or_else(|| "out/pools/full_np8_12_holdout.json".to_owned());

    let document: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&checkpoint).expect("read")).expect("parse");
    let loaded: BTreeMap<String, Profile> =
        serde_json::from_value(document["profiles"].clone()).expect("profiles");
    let pool =
        std::sync::Arc::new(ti4_sim::MapPool::load(std::path::Path::new(&pool_path)).expect("pool"));

    let mut games = 0usize;
    let mut games_with_conquest = 0usize;
    let mut expansion: BTreeMap<String, usize> = BTreeMap::new();
    let mut conquest: BTreeMap<(String, String), usize> = BTreeMap::new();
    let mut losses: BTreeMap<String, usize> = BTreeMap::new();
    let mut combat_steps: BTreeMap<String, usize> = BTreeMap::new();
    let mut all_heads: BTreeMap<String, usize> = BTreeMap::new();
    let mut seats: BTreeMap<String, usize> = BTreeMap::new();

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
            let ledger = Rc::new(RefCell::new(Ledger::default()));
            let mut deciders: BTreeMap<PlayerId, Box<dyn Decider>> = BTreeMap::new();
            for (index, player) in players.iter().enumerate() {
                let profile = loaded[&factions[player].to_string()].clone();
                let stream = seed
                    .wrapping_mul(1_000_003)
                    .wrapping_add(u64::try_from(index).unwrap_or(0));
                deciders.insert(
                    player.clone(),
                    Box::new(WatchBot {
                        inner: LearnedBot::from_shared(std::sync::Arc::new(profile), stream),
                        faction: factions[player].to_string(),
                        players: players.clone(),
                        ledger: Rc::clone(&ledger),
                    }),
                );
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
            for seat in &rollout.seats {
                *seats.entry(seat.faction.to_string()).or_default() += 1;
            }
            let ledger = ledger.borrow();
            if ledger.any_conquest {
                games_with_conquest += 1;
            }
            for (who, n) in &ledger.expansion {
                *expansion.entry(who.clone()).or_default() += n;
            }
            for (pair, n) in &ledger.conquest {
                *conquest.entry(pair.clone()).or_default() += n;
            }
            for (who, n) in &ledger.losses {
                *losses.entry(who.clone()).or_default() += n;
            }
            for (head, n) in &ledger.heads {
                *all_heads.entry(head.clone()).or_default() += n;
            }
            for (who, n) in &ledger.combat_by_faction {
                *combat_steps.entry(who.clone()).or_default() += n;
            }
        }
    }

    println!("{games} games, {rounds} rounds, pool {pool_path}");
    println!("checkpoint {checkpoint}\n");

    let total_expansion: usize = expansion.values().sum();
    let total_conquest: usize = conquest.values().sum();
    #[expect(clippy::cast_precision_loss, reason = "small counts")]
    let conquest_share =
        100.0 * total_conquest as f64 / (total_conquest + total_expansion).max(1) as f64;
    println!("PLANET CONTROL CHANGES");
    println!("  expansion (uncontrolled -> a player): {total_expansion}");
    println!("  conquest  (player -> player):         {total_conquest}  ({conquest_share:.1}% of all takes)");
    #[expect(clippy::cast_precision_loss, reason = "small counts")]
    let with = 100.0 * games_with_conquest as f64 / games.max(1) as f64;
    println!("  games with any conquest at all:       {games_with_conquest} ({with:.1}%)");

    println!("\nPER FACTION (per seat)");
    println!("{:<10}{:>10}{:>10}{:>10}{:>12}", "faction", "expand", "took", "lost", "unit-losses");
    for faction in FACTIONS {
        let n = seats.get(faction).copied().unwrap_or(0).max(1);
        let took: usize = conquest
            .iter()
            .filter(|((taker, _), _)| taker == faction)
            .map(|(_, count)| *count)
            .sum();
        let lost: usize = conquest
            .iter()
            .filter(|((_, loser), _)| loser == faction)
            .map(|(_, count)| *count)
            .sum();
        #[expect(clippy::cast_precision_loss, reason = "small counts")]
        let per = |value: usize| value as f64 / n as f64;
        println!(
            "{faction:<10}{:>10.2}{:>10.3}{:>10.3}{:>12.2}",
            per(expansion.get(faction).copied().unwrap_or(0)),
            per(took),
            per(lost),
            per(losses.get(faction).copied().unwrap_or(0))
        );
    }

    println!("\nCOMBAT DECISIONS");
    let total_combat: usize = combat_steps.values().sum();
    #[expect(clippy::cast_precision_loss, reason = "small counts")]
    let per_game = total_combat as f64 / games.max(1) as f64;
    println!("  {total_combat} combat-head decisions ({per_game:.2} per game)");
    for faction in FACTIONS {
        let n = seats.get(faction).copied().unwrap_or(0).max(1);
        #[expect(clippy::cast_precision_loss, reason = "small counts")]
        let per = combat_steps.get(faction).copied().unwrap_or(0) as f64 / n as f64;
        println!("    {faction:<10} {per:>6.2} per seat");
    }
    println!("
  all decision heads, per game:");
    let mut rows: Vec<_> = all_heads.iter().collect();
    rows.sort_by(|a, b| b.1.cmp(a.1));
    for (head, count) in rows {
        #[expect(clippy::cast_precision_loss, reason = "small counts")]
        let per = *count as f64 / games.max(1) as f64;
        println!("    {head:<14} {per:>9.1}");
    }

    println!("\nWHO TAKES FROM WHOM (taker <- loser)");
    let mut rows: Vec<_> = conquest.iter().collect();
    rows.sort_by(|a, b| b.1.cmp(a.1));
    for ((taker, loser), count) in rows.into_iter().take(12) {
        println!("  {taker:<10} <- {loser:<10} {count:>6}");
    }
}
