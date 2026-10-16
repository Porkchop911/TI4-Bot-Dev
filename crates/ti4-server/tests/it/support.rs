//! Recorded games shared by the fork-equivalence and batch-differential tests.
//!
//! Two sources: a game generated here from a seed (always available, so CI exercises the
//! tests) and, when present, the saved histories of real smoke-test games
//! (`TI4_FORK_FIXTURES`, a `:`-separated list of `game_*` directories, or the nightly reports
//! of the development machine).
#![allow(dead_code)]

use std::collections::VecDeque;
use std::path::{Path, PathBuf};

use ti4_content::ContentStore;
use ti4_content::galaxy::Galaxy;
use ti4_engine::choice::{
    Choice, ChoiceOption, Decider, DecisionRecord, IllegalChoice, SeededRandom, Table,
};
use ti4_engine::game::Game;
use ti4_model::id::PlayerId;
use ti4_model::state::GameState;
use ti4_server::session::{RngForce, RngMarks};
use ti4_server::storage::{GameHistory, GameInitRecord};

/// A game's initial position and the decisions recorded for it.
#[derive(Clone)]
pub struct Source {
    pub name: String,
    pub state: GameState,
    pub galaxy: Galaxy,
    pub records: Vec<DecisionRecord>,
    pub marks: RngMarks,
}

/// Answers from a recorded list, restoring forced RNG positions like the server's replays.
pub struct Forced {
    pub ids: VecDeque<String>,
    /// Index of the next decision (what `RngForce::before_answer` is keyed by).
    pub index: usize,
    pub force: Option<RngForce>,
}

impl Decider for Forced {
    fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
        let Some(wanted) = self.ids.pop_front() else {
            return Err(IllegalChoice::DeciderFailed {
                player: choice.player.clone(),
                prompt: choice.prompt.clone(),
                reason: "recorded decisions exhausted".to_owned(),
            });
        };
        if let Some(force) = &self.force {
            force.before_answer(self.index);
        }
        self.index += 1;
        choice
            .options
            .iter()
            .find(|option| option.id == wanted)
            .cloned()
            .ok_or_else(|| IllegalChoice::ScriptDiverged {
                player: choice.player.clone(),
                wanted,
                offered: choice.options.iter().map(|o| o.id.clone()).collect(),
            })
    }
}

/// A game positioned after `from` recorded decisions would be built by the caller; this builds
/// the START game answering `records` from index `from` onward (index 0 for a full replay).
#[must_use]
pub fn forced_decider(source: &Source, from: usize, force: Option<RngForce>) -> Forced {
    Forced {
        ids: source.records[from..]
            .iter()
            .map(|record| record.chosen.clone())
            .collect(),
        index: from,
        force,
    }
}

#[must_use]
pub fn forced_table(source: &Source, from: usize, force: Option<RngForce>) -> Table {
    Table::with_default(Box::new(forced_decider(source, from, force)))
}

/// The game at its start, answering the source's recorded decisions.
#[must_use]
pub fn start_game(source: &Source) -> Game<'static> {
    let force = RngForce::new(&source.marks);
    let mut game = Game::with_table(
        source.state.clone(),
        ContentStore::embedded(),
        forced_table(source, 0, force.clone()),
    )
    .with_galaxy(source.galaxy.clone());
    if let Some(force) = &force {
        force.attach(&mut game);
    }
    game
}

/// A seeded random 4-seat game played for at most `max_decisions` decisions. With
/// `mark_every > 0` the source also carries RNG marks (a turn redo's forced positions) at every
/// such decision index, set to the positions the game really had there, so a replay forced to
/// them must change nothing.
#[must_use]
pub fn generated(name: &str, seed: u64, max_decisions: usize, mark_every: usize) -> Source {
    let players: Vec<PlayerId> = ["a", "b", "c", "d"].map(PlayerId::new).to_vec();
    let (state, galaxy) =
        ti4_server::map::create_game_with_template(ContentStore::embedded(), &players, seed, None)
            .expect("game");
    play_source(name, seed, max_decisions, mark_every, state, galaxy)
}

/// Like [`generated`], but the four seats play exactly `factions` (any catalogue faction, also
/// those outside `IN_SCOPE_FACTIONS`), set up the way the base-faction soak does it: the
/// seated state, a board with those homes, then each faction deployed.
#[must_use]
pub fn generated_with_factions(
    name: &str,
    seed: u64,
    factions: [&str; 4],
    max_decisions: usize,
    mark_every: usize,
) -> Source {
    let (state, galaxy) = factions_start(seed, factions);
    play_source(name, seed, max_decisions, mark_every, state, galaxy)
}

/// The opening position of [`generated_with_factions`]: seats a..d playing `factions`, dealt
/// their own promissory notes, on a board with those homes.
#[must_use]
pub fn factions_start(seed: u64, factions: [&str; 4]) -> (GameState, Galaxy) {
    use ti4_model::content_types::DEFAULT;
    use ti4_model::id::FactionId;
    let content = ContentStore::embedded();
    let players: Vec<PlayerId> = ["a", "b", "c", "d"].map(PlayerId::new).to_vec();
    let assignments: std::collections::BTreeMap<PlayerId, FactionId> = players
        .iter()
        .cloned()
        .zip(factions.iter().map(|f| FactionId::new(*f)))
        .collect();
    let mut state = ti4_engine::setup::start_game_seeded(content, &players, DEFAULT, None, seed)
        .expect("setup");
    for (player, faction) in &assignments {
        state.player_mut(player).expect("seat").faction = faction.clone();
    }
    // Setup dealt the notes before factions were known: re-deal so every seat holds its own.
    ti4_engine::promissory::deal(&mut state, content, DEFAULT);
    let filler: Vec<String> = ti4_engine::seating::map_filler(content, 30, DEFAULT, seed)
        .into_iter()
        .map(|system| system.to_string())
        .collect();
    let borrowed: Vec<&str> = filler.iter().map(String::as_str).collect();
    let galaxy =
        ti4_engine::seating::build_board(content, &assignments, &borrowed, DEFAULT).expect("board");
    for (player, faction) in &assignments {
        ti4_engine::seating::deploy(&mut state, content, player, faction, DEFAULT).expect("deploy");
    }
    (state, galaxy)
}

fn play_source(
    name: &str,
    seed: u64,
    max_decisions: usize,
    mark_every: usize,
    state: GameState,
    galaxy: Galaxy,
) -> Source {
    let table = Table::with_default(Box::new(SeededRandom::new(seed ^ 0x5eed)));
    let mut game = Game::with_table(state.clone(), ContentStore::embedded(), table)
        .with_galaxy(galaxy.clone());
    let mut marks = RngMarks::new();
    for _ in 0..max_decisions.saturating_mul(4) {
        let len = game.table.log.records.len();
        if len >= max_decisions || game.state.finished {
            break;
        }
        if mark_every > 0 && len % mark_every == 0 && len > 0 {
            marks.insert(len, game.rng_positions());
        }
        let result = game.step();
        if result.error.is_some() || result.finished {
            break;
        }
    }
    Source {
        name: name.to_owned(),
        state,
        galaxy,
        records: game.table.log.records.clone(),
        marks,
    }
}

/// Load one saved game directory (read only).
///
/// # Errors
/// Anything that stops the save from loading (the caller reports and skips it).
pub fn load_saved(dir: &Path) -> Result<Source, String> {
    // The envelope checksum covers a re-serialisation that newer fields change; the payload
    // itself is what a recovery replays, so read that and skip the integrity check.
    let payload = |file: &str| -> Result<serde_json::Value, String> {
        let bytes = std::fs::read(dir.join(file)).map_err(|e| format!("{file}: {e}"))?;
        let mut value: serde_json::Value =
            serde_json::from_slice(&bytes).map_err(|e| format!("{file}: {e}"))?;
        Ok(value
            .get_mut("payload")
            .map(serde_json::Value::take)
            .unwrap_or(value))
    };
    let init: GameInitRecord =
        serde_json::from_value(payload("init.json")?).map_err(|e| format!("init: {e}"))?;
    let history: GameHistory =
        serde_json::from_value(payload("history.json")?).map_err(|e| format!("history: {e}"))?;
    let seed = init.seed.ok_or_else(|| "no seed".to_owned())?;
    let (_, galaxy) = ti4_server::map::create_game_with_template(
        ContentStore::embedded(),
        &init.player_ids,
        seed,
        init.map_template.as_deref(),
    )?;
    Ok(Source {
        name: format!("{}:{}", dir.display(), history.decisions.len()),
        state: init.initial_state,
        galaxy,
        records: history.decisions,
        marks: history.rng_marks,
    })
}

/// Saved real games available on this machine, at most `limit`.
#[must_use]
pub fn saved_games(limit: usize) -> Vec<Source> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Ok(list) = std::env::var("TI4_FORK_FIXTURES") {
        dirs.extend(list.split(':').filter(|s| !s.is_empty()).map(PathBuf::from));
    } else {
        for root in [
            // Late games that replay completely on the current engine (older saves can diverge
            // where a later rules fix changed the offers).
            "/root/TI4-Bot-Dev/nightly-reports/2026-10-07/runs/02-2338/server-data",
            "/root/TI4-Bot-Dev/nightly-reports/manual-2026-10-07/run-3/server-data",
            "/root/TI4-Bot-Dev/nightly-reports/2026-10-06/runs/01-2030/server-data",
            "/root/TI4-Bot-Dev/nightly-reports/2026-10-07/runs/04-0526/server-data",
            "/root/TI4-Bot-Dev/nightly-reports/2026-10-07/runs/07-0206/server-data",
            "/root/TI4-Bot-Dev/nightly-reports/2026-10-06/runs/04-2123/server-data",
            "/root/TI4-Bot-Dev/nightly-reports/manual-2026-10-07/run-2/server-data",
            "/root/TI4-Bot-Dev/nightly-reports/2026-10-06/runs/06-2156/server-data",
        ] {
            if let Ok(entries) = std::fs::read_dir(root) {
                dirs.extend(entries.flatten().map(|entry| entry.path()));
            }
        }
    }
    let mut out = Vec::new();
    for dir in dirs {
        if out.len() >= limit {
            break;
        }
        match load_saved(&dir) {
            Ok(source) => out.push(source),
            Err(error) => eprintln!("skipping saved game {}: {error}", dir.display()),
        }
    }
    out
}

/// Whether the batch simulator's strict prefix check (offered ids and context, not only the
/// chosen option) accepts the whole recorded game on this engine. Saves recorded before a
/// rules fix can pass a lenient replay and still fail this one.
#[must_use]
pub fn strictly_replayable(source: &Source) -> bool {
    let mut config = ti4_server::session::SessionConfig::new("strict", source.state.clone())
        .with_galaxy(source.galaxy.clone(), Vec::new());
    config.rng_marks = source.marks.clone();
    ti4_server::session::batch::replay_boundary_state(&config, &source.records).is_ok()
}

/// [`saved_games`] restricted to the ones [`strictly_replayable`] accepts.
#[must_use]
pub fn strict_saved_games(limit: usize) -> Vec<Source> {
    let mut out = Vec::new();
    for source in saved_games(usize::MAX) {
        if out.len() >= limit {
            break;
        }
        if strictly_replayable(&source) {
            out.push(source);
        } else {
            eprintln!(
                "skipping {}: not strictly replayable on this engine",
                source.name
            );
        }
    }
    out
}
