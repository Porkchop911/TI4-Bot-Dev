//! What to play: the tables of a run, and the exact game each one makes.
//!
//! A [`Matrix`] is a list of [`SetupSpec`]s (map list, player count, factions, rotation, seeds).
//! [`SetupSpec::expand`] turns each into [`GameSpec`]s: one fully resolved game each (map alias,
//! faction of every seat, speaker), which is all that is needed to play it again.

use std::collections::BTreeSet;

use rand::SeedableRng;
use rand::seq::SliceRandom;
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};
use ti4_content::ContentStore;
use ti4_engine::game::Game;
use ti4_engine::seating::LOBBY_FACTIONS;
use ti4_host::map::create_game_with_template_seated;
use ti4_host::maps::catalog::{self, MapChoice};
use ti4_host::setup::card_set;
use ti4_model::content_types::POK;
use ti4_model::id::PlayerId;

/// The factions the lobby's draw seats, one per seat (`chosen`: `None` for "any").
pub(crate) fn lobby_seat_factions(
    chosen: &[Option<String>],
    seed: u64,
) -> Result<Vec<ti4_model::id::FactionId>, String> {
    let chosen: Vec<Option<&str>> = chosen.iter().map(Option::as_deref).collect();
    ti4_engine::seating::lobby_faction_draw(&chosen, seed).map_err(|e| e.to_string())
}

pub const SEATS: [&str; 8] = ["a", "b", "c", "d", "e", "f", "g", "h"];
pub const MIN_PLAYERS: usize = 3;
pub const MAX_PLAYERS: usize = 8;

/// One game, fully resolved. Everything the opening state is made from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameSpec {
    /// The setup of the matrix this game came from.
    pub setup: String,
    pub seed: u64,
    pub players: usize,
    /// A template alias, or `"random"` for the seeded random board.
    pub map: String,
    /// The faction of every seat, in seat order (`a`, `b`, ...).
    pub factions: Vec<String>,
    /// `"seat"` (seat `a` opens), `"random"` (drawn from the seed) or a 0-based seat number.
    pub speaker: String,
    /// A strategy card set of `ti4_host::card_set::OFFERED`; `None`: the default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub card_set: Option<String>,
    /// Which rotation of the base factions this game plays (`rotate`), if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotation: Option<usize>,
    /// The seats that choose their starting technology are asked (Winnu, Argent Flight, Council
    /// Keleres, Thunder's Edge factions; `ti4_engine::starting_tech`). Left out: off, the game
    /// every summary recorded before the choice was played.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub starting_tech: bool,
}

impl GameSpec {
    /// The same game, in the flags of `selfplay replay`.
    #[must_use]
    pub fn command_line(&self) -> String {
        let mut line = format!(
            "--seed {} --players {} --map {} --factions {} --speaker {}",
            self.seed,
            self.players,
            self.map,
            self.factions.join(","),
            self.speaker
        );
        if let Some(set) = &self.card_set {
            line.push_str(&format!(" --card-set {set}"));
        }
        if self.starting_tech {
            line.push_str(" --starting-tech");
        }
        line
    }

    /// The game at its opening.
    ///
    /// # Errors
    /// A message when the table is not one the host builds.
    pub fn build(&self) -> Result<Game<'static>, String> {
        let content = ContentStore::embedded();
        if !(MIN_PLAYERS..=MAX_PLAYERS).contains(&self.players) {
            return Err(format!(
                "{} players: use {MIN_PLAYERS} to {MAX_PLAYERS}",
                self.players
            ));
        }
        if self.factions.len() != self.players {
            return Err(format!(
                "{} factions for {} seats",
                self.factions.len(),
                self.players
            ));
        }
        let ids: Vec<PlayerId> = SEATS[..self.players]
            .iter()
            .map(|s| PlayerId::new(*s))
            .collect();
        let chosen: Vec<Option<String>> = self.factions.iter().cloned().map(Some).collect();
        let template = (self.map != "random").then_some(self.map.as_str());
        let assignments: std::collections::BTreeMap<_, _> = ids
            .iter()
            .cloned()
            .zip(lobby_seat_factions(&chosen, self.seed)?)
            .collect();
        // The first seat opens (the speaker is set below).
        let (mut state, galaxy) = create_game_with_template_seated(
            content,
            &ids,
            self.seed,
            template,
            &assignments,
            Some(self.card_set.as_deref().unwrap_or(card_set::DEFAULT)),
        )?;
        match self.speaker.as_str() {
            "seat" => {}
            "random" => {
                if let Some(drawn) = ti4_engine::seeded_speaker(&ids, self.seed) {
                    state.speaker = drawn;
                }
            }
            other => {
                let seat: usize = other
                    .parse()
                    .map_err(|_| format!("speaker '{other}': use seat, random or a seat number"))?;
                state.speaker = ids
                    .get(seat)
                    .cloned()
                    .ok_or(format!("speaker seat {seat} is not at the table"))?;
            }
        }
        if self.starting_tech {
            ti4_engine::starting_tech::open(&mut state, content, POK);
        }
        Ok(Game::with_seeded_random(state, content, self.seed)
            .with_sources(POK)
            .with_galaxy(galaxy))
    }
}

/// How several maps share the seeds of a setup.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MapPolicy {
    /// Seed number `i` (in the list) plays map `i mod maps`.
    #[default]
    Cycle,
    /// Each seed draws its map from the seed (uniform, repeatable).
    Random,
    /// Every map for every seed.
    Cross,
}

impl MapPolicy {
    /// # Errors
    /// For a name other than `cycle`, `random`, `cross`.
    pub fn parse(text: &str) -> Result<Self, String> {
        match text {
            "cycle" => Ok(Self::Cycle),
            "random" => Ok(Self::Random),
            "cross" => Ok(Self::Cross),
            other => Err(format!("map policy '{other}': use cycle, random or cross")),
        }
    }
}

/// How the factions of a setup are permuted over the seats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Rotate {
    /// One game per seed and map.
    #[default]
    None,
    /// Each seed is played once per cyclic shift of its factions (a Latin square: every
    /// faction sits in every seat exactly once). `n` games per seed and map.
    Cyclic,
    /// Every permutation (`n!` games per seed and map; at most 6 seats).
    All,
}

impl Rotate {
    /// # Errors
    /// For an unknown name.
    pub fn parse(text: &str) -> Result<Self, String> {
        match text {
            "none" => Ok(Self::None),
            "cyclic" | "latin" => Ok(Self::Cyclic),
            "all" => Ok(Self::All),
            other => Err(format!("rotate '{other}': use none, cyclic (latin) or all")),
        }
    }
}

/// A list of seeds: a range `1..100` (end excluded), a list `3,5,9`, or an array in a file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Seeds {
    Text(String),
    List(Vec<u64>),
}

impl Seeds {
    /// # Errors
    /// When the text is neither a range nor a list.
    pub fn values(&self) -> Result<Vec<u64>, String> {
        match self {
            Self::List(list) => Ok(list.clone()),
            Self::Text(text) => parse_seeds(text),
        }
    }
}

/// `a..b` (b excluded), `a..=b`, or comma separated numbers and ranges mixed.
///
/// # Errors
/// On text that is not a number or a range.
pub fn parse_seeds(text: &str) -> Result<Vec<u64>, String> {
    let mut out = Vec::new();
    for part in text.split(',').map(str::trim).filter(|p| !p.is_empty()) {
        let number = |s: &str| {
            s.trim()
                .parse::<u64>()
                .map_err(|e| format!("seed '{s}': {e}"))
        };
        if let Some((a, b)) = part.split_once("..=") {
            out.extend(number(a)?..=number(b)?);
        } else if let Some((a, b)) = part.split_once("..") {
            out.extend(number(a)?..number(b)?);
        } else {
            out.push(number(part)?);
        }
    }
    Ok(out)
}

fn default_players() -> usize {
    4
}
fn default_speaker() -> String {
    "random".to_owned()
}

/// One table of a matrix: what is fixed, what is drawn, and the seeds to play.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetupSpec {
    /// Names the setup in the results. Left out: `<players>p`.
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default = "default_players")]
    pub players: usize,
    /// Template aliases and/or `random`. Left out: the template recommended for the player count.
    #[serde(default)]
    pub maps: Vec<String>,
    #[serde(default)]
    pub map_policy: MapPolicy,
    /// Per seat: a faction id, or `""`, `"_"`, `"random"` for a free seat. Shorter than the
    /// table: the seats after it are free. Left out: all free.
    #[serde(default)]
    pub factions: Vec<String>,
    /// The factions the free seats are drawn from (no duplicates). Left out: the lobby pool.
    #[serde(default)]
    pub faction_pool: Vec<String>,
    #[serde(default)]
    pub rotate: Rotate,
    /// `seat`, `random` (the default) or a seat number.
    #[serde(default = "default_speaker")]
    pub speaker: String,
    #[serde(default)]
    pub card_set: Option<String>,
    /// The games ask for the starting technology ([`GameSpec::starting_tech`]).
    #[serde(default)]
    pub starting_tech: bool,
    pub seeds: Seeds,
}

impl SetupSpec {
    #[must_use]
    pub fn label(&self) -> String {
        self.name
            .clone()
            .unwrap_or_else(|| format!("{}p", self.players))
    }

    /// The factions of the table for `seed`: fixed seats kept, free seats drawn.
    fn draw_factions(&self, seed: u64) -> Result<Vec<String>, String> {
        let n = self.players;
        let free = |s: &str| matches!(s.trim(), "" | "_" | "random" | "any" | "*");
        let mut chosen: Vec<Option<String>> = (0..n)
            .map(|i| {
                self.factions
                    .get(i)
                    .filter(|s| !free(s))
                    .map(|s| s.trim().to_owned())
            })
            .collect();
        if self.factions.len() > n {
            return Err(format!("{} factions for {n} seats", self.factions.len()));
        }
        let mut seen = BTreeSet::new();
        for f in chosen.iter().flatten() {
            if !LOBBY_FACTIONS.contains(&f.as_str()) {
                return Err(format!(
                    "faction '{f}' is not played here (known: {})",
                    LOBBY_FACTIONS.join(", ")
                ));
            }
            if !seen.insert(f.clone()) {
                return Err(format!("faction '{f}' is fixed on two seats"));
            }
        }
        if self.faction_pool.is_empty() {
            // The draw of the lobby itself (so an unconstrained setup is the game the product makes).
            return lobby_seat_factions(&chosen, seed)
                .map(|ids| ids.iter().map(|f| f.as_str().to_owned()).collect());
        }
        let mut pool: Vec<String> = Vec::new();
        for f in &self.faction_pool {
            if !LOBBY_FACTIONS.contains(&f.as_str()) {
                return Err(format!("pool faction '{f}' is not played here"));
            }
            if !seen.contains(f) && !pool.contains(f) {
                pool.push(f.clone());
            }
        }
        let needed = chosen.iter().filter(|c| c.is_none()).count();
        if pool.len() < needed {
            return Err(format!(
                "the pool has {} unused factions for {needed} free seats",
                pool.len()
            ));
        }
        pool.shuffle(&mut ChaCha8Rng::seed_from_u64(seed ^ 0x5e1f_9a70_fac7_1011));
        let mut next = pool.into_iter();
        for slot in &mut chosen {
            if slot.is_none() {
                *slot = next.next();
            }
        }
        Ok(chosen.into_iter().flatten().collect())
    }

    /// The games of this setup, in a fixed order, and the maps that were left out (with why).
    ///
    /// # Errors
    /// A message for a table that cannot be made (unknown or repeated faction, too small a pool).
    pub fn expand(&self) -> Result<(Vec<GameSpec>, Vec<String>), String> {
        let n = self.players;
        if !(MIN_PLAYERS..=MAX_PLAYERS).contains(&n) {
            return Err(format!("{n} players: use {MIN_PLAYERS} to {MAX_PLAYERS}"));
        }
        if let Some(set) = &self.card_set {
            card_set::validate(set).map_err(|e| e.to_string())?;
        }
        if !matches!(self.speaker.as_str(), "seat" | "random")
            && self.speaker.parse::<usize>().map_or(true, |s| s >= n)
        {
            return Err(format!(
                "speaker '{}': use seat, random or a seat number below {n}",
                self.speaker
            ));
        }
        if self.rotate == Rotate::All && n > 6 {
            return Err(
                "rotate=all with more than 6 seats is too many games; use cyclic".to_owned(),
            );
        }
        let mut skipped = Vec::new();
        let mut maps: Vec<String> = Vec::new();
        let wanted: Vec<String> = if self.maps.is_empty() {
            vec![catalog::recommended_for(n).unwrap_or_else(|| "random".to_owned())]
        } else {
            self.maps.clone()
        };
        for alias in wanted {
            let choice = if alias == "random" {
                MapChoice::Random
            } else {
                MapChoice::Template {
                    alias: alias.clone(),
                }
            };
            match catalog::validate(&choice, n) {
                Ok(()) => maps.push(alias),
                Err(error) => {
                    skipped.push(format!("map '{alias}' for {n} players skipped: {error}"))
                }
            }
        }
        if maps.is_empty() {
            return Err(format!(
                "no map of the list builds for {n} players: {}",
                skipped.join("; ")
            ));
        }
        let name = self.label();
        let mut games = Vec::new();
        for (index, seed) in self.seeds.values()?.into_iter().enumerate() {
            let base = self.draw_factions(seed)?;
            let for_seed: Vec<&String> = match self.map_policy {
                MapPolicy::Cycle => vec![&maps[index % maps.len()]],
                MapPolicy::Random => vec![&maps[(splitmix(seed) % maps.len() as u64) as usize]],
                MapPolicy::Cross => maps.iter().collect(),
            };
            for map in for_seed {
                for (rotation, factions) in rotations(&base, self.rotate) {
                    games.push(GameSpec {
                        setup: name.clone(),
                        seed,
                        players: n,
                        map: map.clone(),
                        factions,
                        speaker: self.speaker.clone(),
                        card_set: self.card_set.clone(),
                        rotation,
                        starting_tech: self.starting_tech,
                    });
                }
            }
        }
        Ok((games, skipped))
    }
}

fn splitmix(seed: u64) -> u64 {
    let mut x = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

/// The faction lists a rotation mode plays, each with its number.
fn rotations(base: &[String], mode: Rotate) -> Vec<(Option<usize>, Vec<String>)> {
    let n = base.len();
    match mode {
        Rotate::None => vec![(None, base.to_vec())],
        Rotate::Cyclic => (0..n)
            .map(|shift| {
                (
                    Some(shift),
                    (0..n).map(|i| base[(i + shift) % n].clone()).collect(),
                )
            })
            .collect(),
        Rotate::All => {
            let mut out = Vec::new();
            let mut index: Vec<usize> = (0..n).collect();
            loop {
                out.push((
                    Some(out.len()),
                    index.iter().map(|&i| base[i].clone()).collect(),
                ));
                if !next_permutation(&mut index) {
                    break;
                }
            }
            out
        }
    }
}

/// The next permutation in lexicographic order; false after the last.
fn next_permutation(items: &mut [usize]) -> bool {
    let Some(pivot) = (1..items.len())
        .rev()
        .find(|&i| items[i - 1] < items[i])
        .map(|i| i - 1)
    else {
        return false;
    };
    let swap = (pivot + 1..items.len())
        .rev()
        .find(|&j| items[j] > items[pivot])
        .expect("a larger item follows");
    items.swap(pivot, swap);
    items[pivot + 1..].reverse();
    true
}

/// Limits of a game; a game that reaches one ends with that termination.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Limits {
    /// Cutoff: the game is stopped when this round is reached without a winner.
    pub max_rounds: u32,
    /// Cutoff in engine steps (decisions and automatic steps).
    pub max_steps: usize,
    /// Safety net in wall seconds per game (`stuck` when exceeded). 0: none.
    pub max_seconds: f64,
    /// Steps in a row asking the same question (the position does not move) that count as stuck.
    pub loop_steps: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_rounds: 50,
            max_steps: 400_000,
            max_seconds: 300.0,
            loop_steps: 5_000,
        }
    }
}

/// A run: setups, limits and the bot configurations.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Matrix {
    pub setups: Vec<SetupSpec>,
    pub limits: Limits,
    /// `key=value` overrides of the rule bot's numbers (see `selfplay knobs`), for every seat.
    pub bot: std::collections::BTreeMap<String, String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup(players: usize, seeds: &str) -> SetupSpec {
        SetupSpec {
            name: None,
            players,
            maps: vec![],
            map_policy: MapPolicy::Cycle,
            factions: vec![],
            faction_pool: vec![],
            rotate: Rotate::None,
            speaker: "random".to_owned(),
            card_set: None,
            starting_tech: false,
            seeds: Seeds::Text(seeds.to_owned()),
        }
    }

    #[test]
    fn seeds_parse_ranges_and_lists() {
        assert_eq!(
            parse_seeds("1..4,9,12..=13").unwrap(),
            vec![1, 2, 3, 9, 12, 13]
        );
        assert!(parse_seeds("x").is_err());
    }

    #[test]
    fn fixed_and_free_factions_and_cyclic_rotation_form_a_latin_square() {
        let mut s = setup(4, "5");
        s.factions = vec!["sol".into(), "_".into(), "hacan".into()];
        s.rotate = Rotate::Cyclic;
        let (games, skipped) = s.expand().unwrap();
        assert!(skipped.is_empty());
        assert_eq!(games.len(), 4);
        assert_eq!(games[0].factions[0], "sol");
        assert_eq!(games[0].factions[2], "hacan");
        let mut seen = BTreeSet::new();
        for g in &games {
            let distinct: BTreeSet<_> = g.factions.iter().collect();
            assert_eq!(distinct.len(), 4, "no duplicates at a table");
            for (seat, f) in g.factions.iter().enumerate() {
                assert!(
                    seen.insert((seat, f.clone())),
                    "every faction sits in every seat once"
                );
            }
        }
    }

    #[test]
    fn a_pool_limits_the_free_seats_and_duplicates_are_errors() {
        let mut s = setup(3, "1..6");
        s.faction_pool = vec!["sol".into(), "xxcha".into(), "naalu".into(), "yin".into()];
        s.factions = vec!["".into(), "naalu".into()];
        for g in s.expand().unwrap().0 {
            assert_eq!(g.factions[1], "naalu");
            assert!(
                g.factions
                    .iter()
                    .all(|f| ["sol", "xxcha", "naalu", "yin"].contains(&f.as_str()))
            );
        }
        s.factions = vec!["sol".into(), "sol".into()];
        assert!(s.expand().is_err());
        s.factions = vec!["nope".into()];
        assert!(s.expand().is_err());
    }

    #[test]
    fn maps_cycle_cross_and_skip_the_ones_that_do_not_fit() {
        let mut s = setup(4, "0..4");
        s.maps = vec![
            "4pHyperlanes".into(),
            "3pHyperlanes".into(),
            "random".into(),
        ];
        let (games, skipped) = s.expand().unwrap();
        assert_eq!(skipped.len(), 1, "{skipped:?}");
        let maps: Vec<_> = games.iter().map(|g| g.map.as_str()).collect();
        assert_eq!(maps, ["4pHyperlanes", "random", "4pHyperlanes", "random"]);
        s.map_policy = MapPolicy::Cross;
        assert_eq!(s.expand().unwrap().0.len(), 8);
        s.map_policy = MapPolicy::Random;
        let again = s.expand().unwrap().0;
        assert_eq!(
            again,
            s.expand().unwrap().0,
            "the draw depends on the seed only"
        );
    }

    #[test]
    fn all_permutations_are_listed_once() {
        let base: Vec<String> = ["a", "b", "c", "d"]
            .iter()
            .map(|s| (*s).to_owned())
            .collect();
        let all = rotations(&base, Rotate::All);
        assert_eq!(all.len(), 24);
        assert_eq!(
            all.iter()
                .map(|r| r.1.clone())
                .collect::<BTreeSet<_>>()
                .len(),
            24
        );
    }
}
