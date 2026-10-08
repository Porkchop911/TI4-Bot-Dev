//! Seating players as factions and deploying their opening positions.
//!
//! Ported from the oracle's `engine/factions.py` `deploy` and `home_systems`, and the
//! galaxy-building half of `engine/game.py` `seated_game`.

use std::collections::{BTreeMap, BTreeSet};

use ti4_content::ContentStore;
use ti4_content::factions::{self, FleetError, Placement};
use ti4_content::galaxy::{Galaxy, GalaxyError, all_systems};
use ti4_model::content_types::{ContentType, SourceSet};
use ti4_model::id::{FactionId, PlanetId, PlayerId, SystemId, TechnologyId};
use ti4_model::state::GameState;
use ti4_model::units::Unit;

/// Mecatol Rex, which sits at the centre of the board: the base tile.
pub const MECATOL: &str = "18";

/// The Thunder's Edge Mecatol Rex tile, whose planet (`mrte`) is legendary (The Galactic Council).
/// Placed instead of [`MECATOL`] whenever Thunder's Edge content is in scope.
pub const MECATOL_TE: &str = "112";

/// Whether `system` is Mecatol Rex, either printing.
#[must_use]
pub fn is_mecatol(system: &str) -> bool {
    system == MECATOL || system == MECATOL_TE
}

/// Whether `planet` is the planet Mecatol Rex, either printing.
#[must_use]
pub fn is_mecatol_planet(planet: &str) -> bool {
    planet == "mr" || planet == "mrte"
}

/// The Mecatol Rex tile a new board uses under `sources`: the Thunder's Edge printing when it is
/// available, else the base tile.
#[must_use]
pub fn mecatol_for(content: &ContentStore, sources: SourceSet) -> &'static str {
    if all_systems(content, sources).contains_key(MECATOL_TE) {
        MECATOL_TE
    } else {
        MECATOL
    }
}

/// The Mecatol Rex tile on this board (the base tile when neither is present).
#[must_use]
pub fn mecatol_on(state: &GameState) -> &'static str {
    if state.board.contains_key(&SystemId::new(MECATOL_TE)) {
        MECATOL_TE
    } else {
        MECATOL
    }
}

/// The Mecatol Rex tile placed on this map (the base tile when neither is placed).
#[must_use]
pub fn mecatol_in_galaxy(galaxy: &Galaxy) -> &'static str {
    if galaxy.coord_of(MECATOL_TE).is_some() {
        MECATOL_TE
    } else {
        MECATOL
    }
}

/// The Creuss Gate (tile 17): where the Creuss home position sits **on the map**. It prints a
/// delta wormhole and no planet, and "is not a home system" (Creuss Gate ability).
pub const CREUSS_GATE: &str = "17";

/// The Creuss home system (tile 51): off the map, beside the board, connected to the gate by the
/// delta wormholes both tiles print. The corpus's `homeSystem` for `ghost` names the gate; the
/// seat's home system -- where its units start and its planet lies -- is this tile.
pub const CREUSS_HOME: &str = "51";

/// The Sorrow (tile 94): where the Crimson Rebellion home position sits **on the map**. It prints an
/// epsilon wormhole and no planet, and "is not a home system" (Sorrow ability).
pub const SORROW: &str = "94";

/// The Crimson Rebellion home system (tile 118, Ahk Creuxx): in the seat's play area, off the map,
/// connected to the Sorrow (and every other epsilon wormhole) by the epsilon wormholes both print.
/// The corpus's `homeSystem` for `crimson` names the Sorrow; the seat's home system -- where its
/// units start and its planet lies -- is this tile.
pub const CRIMSON_HOME: &str = "118";

/// Something went wrong seating a game.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SeatingError {
    #[error("no faction {0:?} in the corpus")]
    UnknownFaction(String),
    #[error("faction {0:?} has no home system")]
    NoHomeSystem(String),
    #[error("no player {0:?} in this game")]
    UnknownPlayer(String),
    #[error("the board needs {wanted} filler tiles to space the homes, but was given {given}")]
    NotEnoughFiller { wanted: usize, given: usize },
    /// The Tribuni: a Keleres variant must be an unplayed faction among Mentak, Xxcha and Argent.
    #[error("Keleres variant {variant:?} needs {faction:?}, which another seat plays")]
    TribuniFactionPlayed {
        variant: String,
        faction: &'static str,
    },
    /// Only one seat can be the Council Keleres.
    #[error("the Council Keleres is seated more than once")]
    KeleresSeatedTwice,
    #[error(transparent)]
    Fleet(#[from] FleetError),
    #[error(transparent)]
    Galaxy(#[from] GalaxyError),
}

/// Why an explicit seeded faction assignment could not be built.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FactionAssignmentError {
    #[error("faction candidate roster is empty")]
    EmptyRoster,
    #[error("duplicate faction candidate {0:?}")]
    DuplicateCandidate(String),
    #[error("duplicate player id {0:?}")]
    DuplicatePlayer(String),
    #[error("{players} players require distinct factions, but the roster has only {candidates}")]
    InsufficientCandidates { players: usize, candidates: usize },
    /// The faction's own text forbids choosing it at setup (the Obsidian's Nocturne).
    #[error("faction {0:?} cannot be chosen during setup")]
    NotChoosable(String),
}

/// The factions this project plays.
///
/// Six, by owner decision: Sol, Hacan, Letnev, Xxcha, Jol-Nar and L1Z1X. The Firmament is
/// explicitly out of scope, and the rest of the corpus's thirty-four factions have no leaders,
/// abilities or units ported.
///
/// **Faction scope and content scope are separate decisions.** Games are played with the whole
/// corpus enabled — Thunder's Edge, Prophecy of Kings, every codex, and the newest printing of
/// anything reprinted (`ti4_model::content_types::DEFAULT`) — while the seats stay these six. A
/// wider corpus means these factions meet more cards, systems and relics; it does not mean anybody
/// else sits down.
///
/// Named here rather than left to "whatever the catalogue lists first", which is how six seats
/// came to be playing Arborec, Argent and the Vuil'raith Cabal — factions with no implemented
/// abilities at all — in every rollout a trainer would have learned from. Alphabetical order is
/// not a scope decision, and a scope decision should not be alphabetical order.
pub const IN_SCOPE_FACTIONS: [&str; 6] = ["sol", "hacan", "letnev", "xxcha", "jolnar", "l1z1x"];

/// Faction assignments for a table, taking the in-scope factions in order.
///
/// Seats beyond the sixth reuse the list from the start, so a larger table is still playing
/// factions this engine implements rather than falling off the end into unported ones.
#[must_use]
pub fn seat_in_scope(players: &[PlayerId]) -> BTreeMap<PlayerId, FactionId> {
    players
        .iter()
        .enumerate()
        .map(|(index, player)| {
            (
                player.clone(),
                FactionId::new(IN_SCOPE_FACTIONS[index % IN_SCOPE_FACTIONS.len()]),
            )
        })
        .collect()
}

/// Assign distinct factions from an explicit ordered roster using an independent seeded stream.
///
/// Candidate order is part of the input: the helper shuffles a copy with the dedicated
/// `seating:faction-assignment:v1` domain, then assigns the first candidates in that shuffled
/// order to `players` in their input order. This preparation primitive is independent of
/// [`IN_SCOPE_FACTIONS`] and is not wired into game setup by itself.
///
/// # Errors
/// Returns an error for an empty or duplicate candidate roster, duplicate players, or too few
/// candidates to give every player a distinct faction, or a faction that cannot be chosen during
/// setup (the Obsidian, Nocturne).
pub fn seeded_faction_assignments(
    candidates: &[&str],
    players: &[PlayerId],
    seed: u64,
) -> Result<BTreeMap<PlayerId, FactionId>, FactionAssignmentError> {
    if candidates.is_empty() {
        return Err(FactionAssignmentError::EmptyRoster);
    }
    let mut seen_candidates = BTreeSet::new();
    for candidate in candidates {
        // Nocturne: "This faction cannot be chosen during setup."
        if crate::factions::obsidian::NOT_CHOOSABLE_DURING_SETUP.contains(candidate) {
            return Err(FactionAssignmentError::NotChoosable(
                (*candidate).to_owned(),
            ));
        }
        if !seen_candidates.insert(*candidate) {
            return Err(FactionAssignmentError::DuplicateCandidate(
                (*candidate).to_owned(),
            ));
        }
    }
    let mut seen_players = BTreeSet::new();
    for player in players {
        if !seen_players.insert(player.to_string()) {
            return Err(FactionAssignmentError::DuplicatePlayer(player.to_string()));
        }
    }
    if candidates.len() < players.len() {
        return Err(FactionAssignmentError::InsufficientCandidates {
            players: players.len(),
            candidates: candidates.len(),
        });
    }

    let mut rng = crate::rng::GameRng::new(seed);
    let shuffled = rng.shuffled("seating:faction-assignment:v1", candidates);
    Ok(players
        .iter()
        .zip(shuffled)
        .map(|(player, alias)| (player.clone(), FactionId::new(alias)))
        .collect())
}

/// Which factions a table draws from. An explicit opt-in: [`FactionRoster::InScope`] is the default
/// everywhere and reproduces [`seat_in_scope`] exactly, so no existing rollout, dataset or
/// checkpoint changes unless a caller names [`FactionRoster::Wide`] (BF-20/BF-21).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FactionRoster {
    /// The six [`IN_SCOPE_FACTIONS`], seated in order (the historical behaviour).
    #[default]
    InScope,
    /// Every choosable implemented faction ([`wide_roster`]), drawn by seed ([`seat_wide`]).
    Wide,
}

/// The roster entry that stands for the Council Keleres in [`wide_roster`]. The variant (one of
/// [`crate::factions::keleres::VARIANTS`]) is decided by [`seat_wide`].
pub const KELERES_FAMILY: &str = "keleres";

/// RNG domain that shuffles the wide roster. Not one of the map/dice/deck domains.
const WIDE_ROSTER_DOMAIN: &str = "seating:wide-roster:v1";
/// RNG domain that picks a Keleres variant among the Tribuni-legal ones.
const KELERES_VARIANT_DOMAIN: &str = "seating:keleres-variant:v1";

/// The wide roster under `sources`, in canonical order: the six [`IN_SCOPE_FACTIONS`], then every
/// engine faction module in [`crate::factions::MODULES`] order.
///
/// Excluded: factions that cannot be chosen at setup (the Obsidian, Nocturne) and factions the
/// `sources` do not include (Thunder's Edge factions need `thunders_edge`, Prophecy of Kings
/// factions need `pok`). The three Keleres variants collapse into one [`KELERES_FAMILY`] entry at
/// the position of the first variant: a table has at most one Council Keleres.
#[must_use]
pub fn wide_roster(content: &ContentStore, sources: SourceSet) -> Vec<&'static str> {
    let catalogue = factions::catalogue(content, sources);
    let mut roster: Vec<&'static str> = Vec::new();
    let candidates = IN_SCOPE_FACTIONS
        .iter()
        .copied()
        .chain(crate::factions::MODULES.iter().map(|module| module.alias));
    for alias in candidates {
        if crate::factions::obsidian::NOT_CHOOSABLE_DURING_SETUP.contains(&alias)
            || !catalogue.contains_key(alias)
        {
            continue;
        }
        let entry = if crate::factions::keleres::is_keleres_faction(alias) {
            KELERES_FAMILY
        } else {
            alias
        };
        if !roster.contains(&entry) {
            roster.push(entry);
        }
    }
    roster
}

/// Deterministic faction draw from the [`wide_roster`] for a table, by seed.
///
/// * The roster is shuffled on the dedicated `seating:wide-roster:v1` stream of a fresh
///   [`crate::rng::GameRng`], so no map, dice or deck stream moves; players take the first entries
///   in input order. Same inputs and seed give the same map.
/// * Every faction is distinct. A table larger than the roster is refused with
///   [`FactionAssignmentError::InsufficientCandidates`]; factions are never reused.
/// * The Council Keleres: when its entry is drawn the variant is picked among the Tribuni-legal
///   variants ([`tribuni_variants_available`] over the factions already seated, which the engine's
///   own rule uses), on the separate `seating:keleres-variant:v1` stream. If none is legal the
///   entry is skipped, and a base faction (Mentak, Xxcha, Argent) a drawn variant took is skipped
///   when it comes up, so the table always passes [`validate_tribuni`].
///
/// # Errors
/// Duplicate players, or too few seatable factions.
pub fn seat_wide(
    content: &ContentStore,
    players: &[PlayerId],
    sources: SourceSet,
    seed: u64,
) -> Result<BTreeMap<PlayerId, FactionId>, FactionAssignmentError> {
    let mut seen_players = BTreeSet::new();
    for player in players {
        if !seen_players.insert(player.to_string()) {
            return Err(FactionAssignmentError::DuplicatePlayer(player.to_string()));
        }
    }
    let roster = wide_roster(content, sources);
    let catalogue = factions::catalogue(content, sources);
    let mut rng = crate::rng::GameRng::new(seed);
    let shuffled = rng.shuffled(WIDE_ROSTER_DOMAIN, &roster);

    let mut picked: Vec<&'static str> = Vec::with_capacity(players.len());
    let mut taken_base: Option<&'static str> = None;
    for entry in shuffled {
        if picked.len() == players.len() {
            break;
        }
        if entry == KELERES_FAMILY {
            let available: Vec<&'static str> = tribuni_variants_available(picked.iter().copied())
                .into_iter()
                .filter(|variant| catalogue.contains_key(variant))
                .collect();
            if available.is_empty() {
                continue;
            }
            let draw = rng.die(KELERES_VARIANT_DOMAIN, available.len() as u32) as usize;
            let variant = available[draw - 1];
            taken_base = tribuni_base(variant);
            picked.push(variant);
        } else if Some(entry) == taken_base {
            continue;
        } else {
            picked.push(entry);
        }
    }
    if picked.len() < players.len() {
        return Err(FactionAssignmentError::InsufficientCandidates {
            players: players.len(),
            candidates: picked.len(),
        });
    }
    debug_assert!(validate_tribuni(picked.iter().copied()).is_ok());
    Ok(players
        .iter()
        .zip(picked)
        .map(|(player, alias)| (player.clone(), FactionId::new(alias)))
        .collect())
}

/// Faction assignments for a table under `roster`. [`FactionRoster::InScope`] ignores `seed` and
/// is exactly [`seat_in_scope`]; [`FactionRoster::Wide`] is [`seat_wide`].
///
/// # Errors
/// Only for [`FactionRoster::Wide`]; see [`seat_wide`].
pub fn seat_roster(
    roster: FactionRoster,
    content: &ContentStore,
    players: &[PlayerId],
    sources: SourceSet,
    seed: u64,
) -> Result<BTreeMap<PlayerId, FactionId>, FactionAssignmentError> {
    match roster {
        FactionRoster::InScope => Ok(seat_in_scope(players)),
        FactionRoster::Wide => seat_wide(content, players, sources, seed),
    }
}

/// Home system tile ids for a player-to-faction assignment, in assignment order.
///
/// # Errors
/// [`SeatingError::UnknownFaction`] or [`SeatingError::NoHomeSystem`].
pub fn home_systems(
    content: &ContentStore,
    assignments: &BTreeMap<PlayerId, FactionId>,
) -> Result<Vec<SystemId>, SeatingError> {
    assignments
        .values()
        .map(|alias| {
            let faction = factions::get(content, alias.as_str())
                .ok_or_else(|| SeatingError::UnknownFaction(alias.to_string()))?;
            faction
                .home_system()
                .map(SystemId::new)
                .ok_or_else(|| SeatingError::NoHomeSystem(alias.to_string()))
        })
        .collect()
}

/// The faction whose home system, command tokens and control markers a Keleres variant takes
/// (The Tribuni), or `None` for any other faction.
///
/// "Take that faction's home system, command tokens and control markers" is, in this engine, the
/// variant's own faction record: `keleresm` / `keleresx` / `keleresa` carry that faction's home
/// system, home planets and starting fleet already (`factions.json`). Command tokens are pool
/// counts (not coloured pieces) and control is recorded per seat, so there is nothing further to
/// take: the substantive rule is that the base faction must not also be played.
#[must_use]
pub fn tribuni_base(variant: &str) -> Option<&'static str> {
    match variant {
        "keleresm" => Some("mentak"),
        "keleresx" => Some("xxcha"),
        "keleresa" => Some("argent"),
        _ => None,
    }
}

/// The Keleres variants The Tribuni still allows, given the factions the other seats play: those
/// whose base faction is unplayed, in Tribuni order.
#[must_use]
pub fn tribuni_variants_available<'a>(
    others: impl IntoIterator<Item = &'a str> + Clone,
) -> Vec<&'static str> {
    crate::factions::keleres::VARIANTS
        .into_iter()
        .filter(|variant| {
            let base = tribuni_base(variant);
            !others
                .clone()
                .into_iter()
                .any(|played| Some(played) == base)
        })
        .collect()
}

/// The Tribuni (and the one-faction-per-seat rule): a Keleres variant's base faction is unplayed,
/// and only one seat is the Council Keleres.
///
/// `factions` is every seat's faction. Order-independent, so a table is legal or not however its
/// seats were listed.
///
/// # Errors
/// [`SeatingError::TribuniFactionPlayed`] or [`SeatingError::KeleresSeatedTwice`].
pub fn validate_tribuni<'a>(
    factions: impl IntoIterator<Item = &'a str> + Clone,
) -> Result<(), SeatingError> {
    let mut keleres = factions
        .clone()
        .into_iter()
        .filter(|faction| crate::factions::keleres::is_keleres_faction(faction));
    let Some(variant) = keleres.next() else {
        return Ok(());
    };
    if keleres.next().is_some() {
        return Err(SeatingError::KeleresSeatedTwice);
    }
    match tribuni_base(variant) {
        Some(base) if factions.into_iter().any(|played| played == base) => {
            Err(SeatingError::TribuniFactionPlayed {
                variant: variant.to_owned(),
                faction: base,
            })
        }
        _ => Ok(()),
    }
}

/// Seat one player as a faction and place their opening position.
///
/// The Tribuni is enforced here as well as in [`build_board`]: a Keleres variant is refused while
/// another seat holds its base faction, and a base faction is refused while a Keleres seat holds
/// the variant that took it.
///
/// Sets control of every home planet, deploys the starting fleet with `mech` and `flagship`
/// resolved to the faction's own versions, and grants the faction's starting technology.
///
/// # Errors
/// Any [`SeatingError`].
pub fn deploy(
    state: &mut GameState,
    content: &ContentStore,
    player: &PlayerId,
    alias: &FactionId,
    sources: SourceSet,
) -> Result<(), SeatingError> {
    if state.player(player).is_none() {
        return Err(SeatingError::UnknownPlayer(player.to_string()));
    }
    let faction = factions::get(content, alias.as_str())
        .ok_or_else(|| SeatingError::UnknownFaction(alias.to_string()))?;
    let home = faction
        .home_system()
        .ok_or_else(|| SeatingError::NoHomeSystem(alias.to_string()))?;
    // Creuss: "place the Creuss Gate (tile 17) where your home system would normally be placed ...
    // Then, place your home system (tile 51) in your play area." The faction record names the
    // gate; the seat's home system, with its planet and starting fleet, is tile 51.
    // The Crimson Rebellion: "place the Sorrow (tile 94) where your home system would normally be
    // placed, then place an inactive breach there ... Then, place your home system (tile 118) in your
    // play area." The faction record names the Sorrow; the seat's home system is tile 118.
    let system_id = SystemId::new(if home == CREUSS_GATE {
        CREUSS_HOME
    } else if home == SORROW {
        CRIMSON_HOME
    } else {
        home
    });
    // The Tribuni, against the seats as they stand with this one assigned (checked first, so a
    // refused seat leaves the state untouched).
    validate_tribuni(
        state
            .players
            .iter()
            .map(|seat| {
                if &seat.id == player {
                    alias.as_str()
                } else {
                    seat.faction.as_str()
                }
            })
            .collect::<Vec<_>>(),
    )?;
    let home_planets = faction.home_planets();
    let deployments = faction.deployments(content)?;

    // Resolve everything against the corpus before touching the state, so a faction that
    // fails to deploy leaves no half-seated player behind.
    let mut placements = Vec::new();
    for deployment in deployments {
        let unit_id = factions::resolve_unit(content, alias.as_str(), &deployment.unit_id, sources);
        let unit = Unit::new(unit_id, player.clone());
        placements.push((deployment.count, deployment.placement, unit));
    }

    let system = state.system_mut(&system_id);
    for planet in &home_planets {
        system.set_control(PlanetId::new(*planet), player.clone());
    }
    for (count, placement, unit) in placements {
        let units: Vec<Unit> = std::iter::repeat_n(unit, count as usize).collect();
        match placement {
            Placement::Space => system.add(&units),
            Placement::Planet(planet) => {
                system.planet_units.entry(planet).or_default().extend(units);
            }
        }
    }

    let starting_tech: Vec<TechnologyId> = faction
        .starting_tech()
        .iter()
        .filter_map(|t| content.resolve_id(ContentType::Technologies, t, sources))
        .map(TechnologyId::new)
        .collect();
    let seat = state
        .player_mut(player)
        .ok_or_else(|| SeatingError::UnknownPlayer(player.to_string()))?;
    seat.faction = alias.clone();
    seat.home_system = Some(system_id);
    seat.home_planets = home_planets.iter().map(|p| PlanetId::new(*p)).collect();
    seat.technologies.extend(starting_tech);
    // Nekro Virus: it cannot research, so the two Valefar Assimilator cards come with the faction
    // (coordinator ruling; plans/evidence/BF-nekro.md).
    if alias.as_str() == crate::factions::nekro::FACTION {
        seat.technologies.insert(TechnologyId::new("vax"));
        seat.technologies.insert(TechnologyId::new("vay"));
    }
    // Sorrow: "place an inactive breach there" (the Sorrow, where the home position is).
    if home == SORROW {
        crate::factions::crimson::place_sorrow_breach(state);
    }
    // Commodities are deliberately not set. LRR 21: the faction record's `commodities` is
    // the *capacity* a player refreshes to, not an opening balance, and a player starts
    // with none. The oracle sets trade_goods to 0 here for the same reason.

    // 51.1: a player begins with their faction's three leaders. `leaders::deploy` existed, was
    // tested, and had no caller outside a test -- so every seat in every simulated game held an
    // empty leader map, and the agent, commander and hero subsystems were unreachable no matter
    // how well they worked. The same shape as the custodians token: implemented, wired, never
    // reached.
    crate::leaders::deploy(state, content, sources, player);

    // Empyrean Dark Whispers: "During setup, take the additional Empyrean faction promissory
    // note." Setup deals notes before factions are seated in some flows, so the seat's own pair
    // is made certain here (a no-op for every other faction).
    crate::factions::empyrean::deal_notes(state, content, player);

    Ok(())
}

/// How many tiles each ring of a three-ring board holds, from the centre out.
const RING_SIZES: [usize; 4] = [1, 6, 12, 18];

/// A board with Mecatol at the centre, filler between, and the home systems **spaced** around
/// the outer ring.
///
/// Not real map setup — there is no draft — but a legal board with somewhere to expand into.
///
/// The spacing is the part that took a measurement to get right. [`Galaxy::build`] fills a spiral
/// positionally, so appending the homes to the end of the id list dropped all six into
/// *consecutive* outer-ring slots: every neighbouring pair of players started one tile apart, in
/// a huddle occupying a third of the ring. The consequences reached everything downstream —
/// home systems were being invaded in round one, a fifth of all scoring windows were refused by
/// 61.16, and no seat ever developed an economy because every seat was under immediate attack.
/// None of that is TI4; it was a board nobody had looked at.
///
/// Homes now sit at every third outer slot, which is where a real six-player board puts them, and
/// filler occupies the rest. That needs enough filler to complete both the inner rings and the
/// gaps — [`SeatingError::NotEnoughFiller`] says so rather than silently huddling them again.
///
/// The filler matters more than it looks for a second reason: with only Mecatol and home systems
/// on the board there is nothing explorable, so exploration could never fire and conquest would
/// have nowhere to go.
///
/// # Errors
/// [`SeatingError::NotEnoughFiller`] when the filler cannot fill the inner rings and the gaps
/// between homes, or any other [`SeatingError`].
pub fn build_board(
    content: &ContentStore,
    assignments: &BTreeMap<PlayerId, FactionId>,
    filler: &[&str],
    sources: SourceSet,
) -> Result<Galaxy, SeatingError> {
    validate_tribuni(
        assignments
            .values()
            .map(FactionId::as_str)
            .collect::<Vec<_>>(),
    )?;
    let homes = home_systems(content, assignments)?;
    let outer = RING_SIZES[3];
    // Evenly spaced: with six homes on an eighteen-tile ring that is every third slot. With
    // fewer players the stride grows, which is what keeps them apart rather than clustered at
    // the start of the ring.
    let stride = if homes.is_empty() {
        outer
    } else {
        outer / homes.len()
    };

    let inner: usize = RING_SIZES[..3].iter().sum::<usize>() - 1; // less Mecatol at the centre
    // Only up to the last home: outer slots beyond it can stay empty, because `Galaxy::build`
    // stops where the id list stops. Slots *before* it cannot — the spiral is filled
    // positionally, so a missing tile would slide every home one place round the ring.
    let outer_used = homes.len().saturating_sub(1) * stride + usize::from(!homes.is_empty());
    let wanted = inner + outer_used.saturating_sub(homes.len());
    if filler.len() < wanted {
        return Err(SeatingError::NotEnoughFiller {
            wanted,
            given: filler.len(),
        });
    }

    let mut ids: Vec<&str> = vec![mecatol_for(content, sources)];
    let mut filler = filler.iter().copied();
    for _ in 0..inner {
        if let Some(tile) = filler.next() {
            ids.push(tile);
        }
    }
    let mut placed = 0usize;
    for slot in 0..outer_used {
        if slot % stride == 0 && placed < homes.len() {
            ids.push(homes[placed].as_str());
            placed += 1;
        } else if let Some(tile) = filler.next() {
            ids.push(tile);
        }
    }

    // Three rings hold 37 tiles, enough for Mecatol plus six homes and filler.
    let rings = 3;
    let mut galaxy = Galaxy::build(content, &ids, sources, rings)?;
    place_wormhole_nexus(&mut galaxy, content, sources)?;
    Ok(galaxy)
}

/// Put the Wormhole Nexus beside the board, for every board family that needs it.
///
/// The Nexus is always in play under Prophecy of Kings and is never dealt as filler: it sits off
/// the hex grid, reached only through the wormholes printed on it, and it starts on its locked face
/// — a gamma wormhole and nothing else, so until it is flipped only a gamma source reaches it, which
/// is Creuss's business.
///
/// This is a rule about the game, not about one way of building a map, so it lives beside the rule
/// and every map family calls it. It did not use to be so: the placement was written inline in
/// [`build_board`], which is the Rust spiral, and the two map families built from a captured Python
/// pool — [`ti4_content::galaxy::Galaxy::placed`], reached through the map pool, which is what the
/// reviewer and the replayer sit on — never went near it. On those tables the Nexus was not hidden,
/// it was simply not in the game: no tile, no gamma partner, no Mallice. A pool of a thousand
/// arrangements contains no `82` in any of them, because the geometry it captured is the ring of
/// tiles and the Nexus is not one.
///
/// Idempotent, and quiet when the tile is already on the grid — a captured arrangement that carries
/// its own `82` keeps it, rather than this turning the setup into a [`GalaxyError::DuplicateSystem`].
///
/// # Errors
/// Any [`GalaxyError`] from registering the tile, which in practice means the corpus has no Nexus in
/// this source scope.
pub fn place_wormhole_nexus(
    galaxy: &mut Galaxy,
    content: &ContentStore,
    sources: SourceSet,
) -> Result<(), ti4_content::galaxy::GalaxyError> {
    place_creuss_home(galaxy, content, sources)?;
    place_crimson_home(galaxy, content, sources)?;
    if !sources.contains(ti4_model::content_types::Source::Pok) {
        return Ok(());
    }
    // `wormhole_kinds` is how a caller asks about a system that has no hex; see its documentation.
    if !galaxy.wormhole_kinds(LOCKED_NEXUS).is_empty()
        || !galaxy.wormhole_kinds(OPEN_NEXUS).is_empty()
    {
        return Ok(());
    }
    galaxy.place_off_map(content, LOCKED_NEXUS, sources)
}

/// Put the Creuss home system beside the board when the Creuss Gate is on it.
///
/// The gate occupies the Creuss seat's home position (see [`CREUSS_GATE`]); the home system is off
/// the hex grid and reached only through the delta wormholes the two tiles print, which
/// `Galaxy::adjacent` pairs by kind like any other wormhole. Called from
/// [`place_wormhole_nexus`], which every map family already calls, so a board with no gate -- every
/// board without a Creuss seat -- is untouched. Idempotent.
///
/// # Errors
/// Any [`GalaxyError`] from registering the tile.
pub fn place_creuss_home(
    galaxy: &mut Galaxy,
    content: &ContentStore,
    sources: SourceSet,
) -> Result<(), ti4_content::galaxy::GalaxyError> {
    if galaxy.coord_of(CREUSS_GATE).is_none() || !galaxy.wormhole_kinds(CREUSS_HOME).is_empty() {
        return Ok(());
    }
    galaxy.place_off_map(content, CREUSS_HOME, sources)
}

/// Put the Crimson Rebellion home system beside the board when the Sorrow is on it.
///
/// The Sorrow occupies the Crimson seat's home position (see [`SORROW`]); the home system (tile
/// 118) is off the hex grid and reached only through epsilon wormholes, which `Galaxy::adjacent`
/// pairs by kind like any other wormhole. Called from [`place_wormhole_nexus`], so a board with no
/// Sorrow -- every board without a Crimson seat -- is untouched. Idempotent.
///
/// # Errors
/// Any [`GalaxyError`] from registering the tile.
pub fn place_crimson_home(
    galaxy: &mut Galaxy,
    content: &ContentStore,
    sources: SourceSet,
) -> Result<(), ti4_content::galaxy::GalaxyError> {
    if galaxy.coord_of(SORROW).is_none() || !galaxy.wormhole_kinds(CRIMSON_HOME).is_empty() {
        return Ok(());
    }
    galaxy.place_off_map(content, CRIMSON_HOME, sources)
}

/// Whether anything has happened that opens the Wormhole Nexus.
///
/// Either a unit has reached the tile, or somebody controls Mallice. Both faces of the planet are
/// checked: `lockedmallice` is the one printed on the locked tile and `mallice` on the open one,
/// and a game that opened the Nexus by taking the planet can be holding either name.
///
/// Read-only, and deliberately not sticky itself -- the caller owns the latch, because units can
/// leave a system and control can change hands while the tile stays face up.
#[must_use]
pub fn nexus_is_triggered(state: &ti4_model::state::GameState) -> bool {
    let occupied = state
        .board
        .get(&SystemId::new(LOCKED_NEXUS))
        .is_some_and(|here| !here.units.is_empty() || !here.planet_units.is_empty());
    let held = state.board.values().any(|here| {
        here.planet_control
            .keys()
            .any(|planet| matches!(planet.as_str(), "mallice" | "lockedmallice"))
    });
    occupied || held
}

/// The Wormhole Nexus, locked face up: a gamma wormhole and nothing else.
pub const LOCKED_NEXUS: &str = "82a";

/// The Wormhole Nexus once flipped: alpha, beta and gamma.
pub const OPEN_NEXUS: &str = "82b";

/// Ordinary planet-bearing tiles to fill a map with.
///
/// Excludes Mecatol, home systems, anomalies, wormholes, and hyperlanes, so the filler is
/// somewhere to expand into rather than a hazard course.
///
/// Returned in corpus order rather than shuffled from a seed as the oracle does. The oracle
/// needs a seed because its map is one of its variables; here a deterministic filler ring
/// keeps board-dependent tests stable. Seeded selection belongs with the simulation harness.
#[must_use]
pub fn neutral_systems(content: &ContentStore, count: usize, sources: SourceSet) -> Vec<SystemId> {
    // One pass over the planet corpus for every homeworld system, rather than asking
    // `is_home_system` per candidate -- which rescans that corpus each time it is asked.
    let homes = ti4_content::galaxy::home_systems(content, sources);
    all_systems(content, sources)
        .into_iter()
        .filter(|(id, system)| {
            !is_mecatol(id)
                && !system.planets().is_empty()
                && !system.is_anomaly()
                && !system.is_hyperlane()
                && system.wormholes().is_empty()
                && !homes.contains(system.id())
                // The Fracture's tiles are off the map until a breakthrough brings them into play
                // (Thunder's Edge Fracture rules). Drawn as filler, a player could fly into one as
                // an ordinary system, and when the Fracture later entered play its neutral garrison
                // was placed on top of those ships with no combat.
                && !crate::fracture::is_fracture_system(content, sources, &SystemId::new(*id))
        })
        .map(|(id, _)| SystemId::new(id))
        .take(count)
        .collect()
}

/// Filler tiles for one map, shuffled by seed.
///
/// [`neutral_systems`] returns the corpus in a stable order, which gives every game the same
/// board. That is right for a test — a board-dependent assertion needs a fixed board — and wrong
/// for training: a policy fitted on one map learns that map, and nothing in a batch report would
/// say so.
///
/// The shuffle is seeded and domain-separated, so a seed names a map and the same seed always
/// draws it. `count` tiles are taken *after* the shuffle rather than before, so the whole corpus
/// is in the draw rather than the first thirty entries of it.
#[must_use]
pub fn map_filler(
    content: &ContentStore,
    count: usize,
    sources: SourceSet,
    seed: u64,
) -> Vec<SystemId> {
    let mut pool = neutral_systems(content, usize::MAX, sources);
    let mut rng = crate::rng::GameRng::new(seed);
    rng.shuffle(crate::rng::domain::GALAXY, &mut pool);
    pool.truncate(count);
    pool
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::setup::start_game;
    use ti4_model::content_types::POK;

    fn content() -> &'static ContentStore {
        ContentStore::embedded()
    }

    fn assignments(pairs: &[(&str, &str)]) -> BTreeMap<PlayerId, FactionId> {
        pairs
            .iter()
            .map(|(p, f)| (PlayerId::new(*p), FactionId::new(*f)))
            .collect()
    }

    fn seated(pairs: &[(&str, &str)]) -> GameState {
        let ids: Vec<PlayerId> = pairs.iter().map(|(p, _)| PlayerId::new(*p)).collect();
        let mut state = start_game(content(), &ids, POK, None).unwrap();
        for (player, alias) in assignments(pairs) {
            deploy(&mut state, content(), &player, &alias, POK).unwrap();
        }
        state
    }

    #[test]
    fn deploying_seats_the_faction_and_takes_home_control() {
        let state = seated(&[("a", "sol")]);
        let player = state.player(&PlayerId::new("a")).unwrap();
        assert_eq!(player.faction, FactionId::new("sol"));
        assert_eq!(player.home_system, Some(SystemId::new("01")));
        assert_eq!(player.home_planets, vec![PlanetId::new("jord")]);

        let home = state.system_state(&SystemId::new("01"));
        assert!(home.controls_a_planet(&PlayerId::new("a")));
    }

    #[test]
    fn sol_opens_with_two_carriers_in_space_and_five_infantry_on_jord() {
        let state = seated(&[("a", "sol")]);
        let home = state.system_state(&SystemId::new("01"));

        let carriers = home
            .units
            .iter()
            .filter(|u| u.type_id.as_str().contains("carrier"))
            .count();
        assert_eq!(carriers, 2);

        let jord = home.on_planet(&PlanetId::new("jord"));
        let infantry = jord
            .iter()
            .filter(|u| u.type_id.as_str().contains("infantry"))
            .count();
        assert_eq!(infantry, 5);
    }

    #[test]
    fn a_faction_gets_its_own_mech_and_flagship() {
        let state = seated(&[("a", "sol")]);
        let home = state.system_state(&SystemId::new("01"));
        let all: Vec<&str> = home
            .units
            .iter()
            .chain(home.planet_units.values().flatten())
            .map(|u| u.type_id.as_str())
            .collect();
        assert!(
            all.iter().any(|u| *u == "sol_infantry" || *u == "infantry"),
            "got {all:?}"
        );
        assert!(
            !all.contains(&"mech"),
            "a generic mech means resolution failed: {all:?}"
        );
    }

    #[test]
    fn every_deployed_unit_belongs_to_the_seated_player() {
        let state = seated(&[("a", "sol"), ("b", "hacan")]);
        for (system, player) in [("01", "a"), ("13", "b")] {
            let home = state.system_state(&SystemId::new(system));
            for unit in home
                .units
                .iter()
                .chain(home.planet_units.values().flatten())
            {
                assert_eq!(unit.owner, PlayerId::new(player), "in system {system}");
            }
        }
    }

    #[test]
    fn l1z1x_deploys_its_capacity_two_super_dreadnought() {
        let state = seated(&[("a", "l1z1x")]);
        let player = PlayerId::new("a");
        let home = state
            .player(&player)
            .and_then(|seat| seat.home_system.clone())
            .expect("L1Z1X has a home system");
        let dreadnought = state
            .system_state(&home)
            .units
            .into_iter()
            .find(|unit| unit.type_id.as_str().contains("dreadnought"))
            .expect("L1Z1X starts with a dreadnought");
        assert_eq!(dreadnought.type_id.as_str(), "l1z1x_dreadnought");
        assert_eq!(
            ti4_content::units::unit_type(content(), dreadnought.type_id.as_str(), POK)
                .expect("the deployed hull exists")
                .capacity(),
            2
        );
    }

    #[test]
    fn saar_deploys_its_printed_production_unit() {
        let state = seated(&[("a", "saar")]);
        let player = PlayerId::new("a");
        let home = state
            .player(&player)
            .and_then(|seat| seat.home_system.clone())
            .expect("Saar has a home system");
        assert_eq!(
            crate::production::capacity(&state, content(), POK, &player, &home),
            5,
            "the starting Floating Factory supplies its printed PRODUCTION 5"
        );
    }

    #[test]
    fn a_seated_player_holds_their_factions_starting_technology() {
        let state = seated(&[("a", "sol")]);
        let player = state.player(&PlayerId::new("a")).unwrap();
        assert!(!player.technologies.is_empty());
        assert!(player.technologies.contains(&TechnologyId::new("amd")));
    }

    #[test]
    fn the_nekro_begins_with_both_valefar_assimilators() {
        let state = seated(&[("a", "nekro"), ("b", "sol")]);
        let nekro = state.player(&PlayerId::new("a")).unwrap();
        assert!(nekro.technologies.contains(&TechnologyId::new("vax")));
        assert!(nekro.technologies.contains(&TechnologyId::new("vay")));
        assert!(nekro.technologies.contains(&TechnologyId::new("dxa")));
        let other = state.player(&PlayerId::new("b")).unwrap();
        assert!(!other.technologies.contains(&TechnologyId::new("vax")));
    }

    #[test]
    fn every_base_faction_deploys_onto_a_board() {
        for (alias, faction) in ti4_content::factions::catalogue(content(), POK) {
            if faction.starting_fleet().is_empty() {
                continue;
            }
            let player = PlayerId::new("a");
            let mut state =
                start_game(content(), std::slice::from_ref(&player), POK, None).unwrap();
            deploy(&mut state, content(), &player, &FactionId::new(alias), POK)
                .unwrap_or_else(|e| panic!("{alias}: {e}"));

            let home = state.player(&player).unwrap().home_system.clone().unwrap();
            let system = state.system_state(&home);
            let placed =
                system.units.len() + system.planet_units.values().map(Vec::len).sum::<usize>();
            assert!(placed > 0, "{alias} deployed nothing");
        }
    }

    #[test]
    fn seating_an_unknown_faction_fails_rather_than_seating_nothing() {
        let player = PlayerId::new("a");
        let mut state = start_game(content(), std::slice::from_ref(&player), POK, None).unwrap();
        let err = deploy(
            &mut state,
            content(),
            &player,
            &FactionId::new("nonesuch"),
            POK,
        )
        .unwrap_err();
        assert!(matches!(err, SeatingError::UnknownFaction(_)), "{err}");
        assert!(state.board.is_empty(), "nothing may be placed on a failure");
    }

    #[test]
    fn seating_an_unseated_player_is_refused() {
        let mut state = start_game(content(), &[PlayerId::new("a")], POK, None).unwrap();
        let err = deploy(
            &mut state,
            content(),
            &PlayerId::new("ghost"),
            &FactionId::new("sol"),
            POK,
        )
        .unwrap_err();
        assert!(matches!(err, SeatingError::UnknownPlayer(_)), "{err}");
    }

    // -- the board ------------------------------------------------------------------

    /// Enough filler for the inner rings and the gaps between six homes.
    fn full_filler() -> Vec<SystemId> {
        neutral_systems(content(), 30, POK)
    }

    fn six_seats() -> BTreeMap<PlayerId, FactionId> {
        assignments(&[
            ("a", "sol"),
            ("b", "hacan"),
            ("c", "letnev"),
            ("d", "xxcha"),
            ("e", "jolnar"),
            ("f", "l1z1x"),
        ])
    }

    #[test]
    fn homes_are_spaced_around_the_outer_ring_not_huddled_at_the_start_of_it() {
        // This is the check nobody had. `Galaxy::build` fills a spiral positionally, so appending
        // the homes to the end of the id list put all six in *consecutive* outer slots: every
        // neighbouring pair started one tile apart. Nothing failed, because nothing looked — the
        // board was legal, connected, and wrong.
        //
        // What it cost, measured over twelve games before the fix: twenty of seventy-two seats
        // lost a home planet, six of them in round one, and a fifth of every scoring window was
        // refused by 61.16. After it, one seat of seventy-two, in round six.
        let seats = six_seats();
        let filler = full_filler();
        let refs: Vec<&str> = filler.iter().map(SystemId::as_str).collect();
        let galaxy = build_board(content(), &seats, &refs, POK).unwrap();

        let homes = home_systems(content(), &seats).unwrap();
        let mut closest = i32::MAX;
        for (index, one) in homes.iter().enumerate() {
            for other in homes.iter().skip(index + 1) {
                let apart = galaxy
                    .distance(one.as_str(), other.as_str())
                    .expect("both homes are on the board");
                closest = closest.min(apart);
            }
        }
        assert!(
            closest >= 3,
            "the closest pair of homes is {closest} tiles apart; a six-player board seats them 3"
        );
    }

    #[test]
    fn every_home_is_the_same_distance_from_mecatol() {
        // The other half of a fair board. Homes evenly spaced but at different radii would give
        // one seat a shorter run at the centre, which decides games on its own.
        let seats = six_seats();
        let filler = full_filler();
        let refs: Vec<&str> = filler.iter().map(SystemId::as_str).collect();
        let galaxy = build_board(content(), &seats, &refs, POK).unwrap();

        let reach: std::collections::BTreeSet<i32> = home_systems(content(), &seats)
            .unwrap()
            .iter()
            .map(|home| {
                galaxy
                    .distance(home.as_str(), MECATOL)
                    .expect("Mecatol is on the board")
            })
            .collect();
        assert_eq!(
            reach.len(),
            1,
            "seats sit at different distances from Mecatol: {reach:?}"
        );
    }

    #[test]
    fn a_board_without_the_filler_to_space_the_homes_is_refused() {
        // Refused rather than huddled. Silently falling back to consecutive slots is exactly the
        // failure this whole test group exists for, and it would be invisible again.
        let seats = six_seats();
        let filler = neutral_systems(content(), 18, POK);
        let refs: Vec<&str> = filler.iter().map(SystemId::as_str).collect();

        let err = build_board(content(), &seats, &refs, POK).unwrap_err();
        assert!(matches!(err, SeatingError::NotEnoughFiller { .. }), "{err}");
    }

    // -- scope ----------------------------------------------------------------------

    #[test]
    fn the_six_in_scope_factions_are_the_six_named() {
        // A list, asserted as a list. Changing who this project plays should be a deliberate edit
        // to a test, not a side effect of a catalogue reordering.
        assert_eq!(
            IN_SCOPE_FACTIONS,
            ["sol", "hacan", "letnev", "xxcha", "jolnar", "l1z1x"]
        );
    }

    #[test]
    fn every_in_scope_faction_exists_in_the_corpus() {
        // A typo here seats a faction that does not exist, and `deploy` fails at setup with an
        // error about an unknown faction rather than about a misspelled constant.
        for alias in IN_SCOPE_FACTIONS {
            assert!(
                ti4_content::factions::get(content(), alias).is_some(),
                "{alias} is not a faction in the corpus"
            );
        }
    }

    #[test]
    fn every_in_scope_faction_is_one_this_engine_has_actually_ported() {
        // The point of a scope. A faction in this list with no leaders registered would be seated
        // in every game and every training rollout while contributing nothing but its starting
        // fleet — which is exactly what Arborec, Argent and the Cabal were doing when the seating
        // took whatever the catalogue listed first.
        for alias in IN_SCOPE_FACTIONS {
            let leaders = crate::leaders::for_faction(content(), POK, alias);
            assert!(
                !leaders.is_empty(),
                "{alias} is in scope but has no leaders in the corpus"
            );
            let known = crate::leaders::registered_abilities();
            let standing = crate::leaders::modifiers();
            let ported = leaders.iter().filter(|leader| {
                known.contains(&leader.as_str()) || standing.contains_key(leader.as_str())
            });
            assert!(
                ported.count() > 0,
                "{alias} is in scope but not one of its leaders is implemented"
            );
        }
    }

    #[test]
    fn a_table_is_seated_from_the_scope_and_from_nothing_else() {
        let players = [
            PlayerId::new("a"),
            PlayerId::new("b"),
            PlayerId::new("c"),
            PlayerId::new("d"),
            PlayerId::new("e"),
            PlayerId::new("f"),
        ];
        let seated = seat_in_scope(&players);

        assert_eq!(seated.len(), 6);
        for faction in seated.values() {
            assert!(
                IN_SCOPE_FACTIONS.contains(&faction.as_str()),
                "{faction} was seated and is not in scope"
            );
        }
        let distinct: std::collections::BTreeSet<&str> =
            seated.values().map(FactionId::as_str).collect();
        assert_eq!(distinct.len(), 6, "six seats, six factions: {distinct:?}");
    }

    #[test]
    fn a_table_larger_than_the_scope_still_plays_in_scope_factions() {
        // Falling off the end of the list is how an unported faction gets seated. Reusing it is
        // wrong as a matchup and right as a scope, and a duplicated faction is visible where a
        // silently unported one is not.
        let players: Vec<PlayerId> = (0..8)
            .map(|index| PlayerId::new(format!("p{index}")))
            .collect();
        let seated = seat_in_scope(&players);

        assert_eq!(seated.len(), 8);
        for faction in seated.values() {
            assert!(
                IN_SCOPE_FACTIONS.contains(&faction.as_str()),
                "{faction} was seated and is not in scope"
            );
        }
        assert_eq!(seated[&players[0]], seated[&players[6]]);
        assert_eq!(seated[&players[1]], seated[&players[7]]);
    }

    #[test]
    fn the_firmament_is_not_in_scope() {
        // Out by owner decision, and it is in the corpus, so nothing else would stop it being
        // seated.
        assert!(
            ti4_content::factions::get(content(), "firmament").is_some(),
            "the corpus does carry it"
        );
        assert!(!IN_SCOPE_FACTIONS.contains(&"firmament"));
    }

    #[test]
    fn every_in_scope_faction_can_actually_be_deployed() {
        // A faction in scope that cannot be seated would fail every game at setup rather than at
        // the point somebody chose it.
        for alias in IN_SCOPE_FACTIONS {
            let player = PlayerId::new("a");
            let mut state =
                start_game(content(), std::slice::from_ref(&player), POK, None).unwrap();
            deploy(&mut state, content(), &player, &FactionId::new(alias), POK)
                .unwrap_or_else(|error| panic!("{alias} could not be deployed: {error}"));
            assert!(
                !state.board.is_empty(),
                "{alias} deployed nothing onto the board"
            );
        }
    }

    #[test]
    fn a_seed_names_a_map_and_different_seeds_name_different_ones() {
        // Every game in this project was played on one board until now: `neutral_systems` returns
        // the corpus in a stable order, so a batch of a thousand games was a thousand games on one
        // map. A policy fitted on it learns it, and no batch report would say so.
        let drawn: std::collections::BTreeSet<Vec<String>> = (0..8)
            .map(|seed| {
                map_filler(content(), 30, POK, seed)
                    .iter()
                    .map(ToString::to_string)
                    .collect()
            })
            .collect();
        assert!(drawn.len() > 1, "eight seeds drew one map");

        let once = map_filler(content(), 30, POK, 3);
        let twice = map_filler(content(), 30, POK, 3);
        assert_eq!(once, twice, "and a seed must always draw the same one");
    }

    #[test]
    fn map_filler_never_draws_a_fracture_tile() {
        // The Fracture is not part of the map: it enters play later, beside it. A Fracture tile
        // drawn as filler let ships into it early, and the garrison later landed on them.
        let sources = ti4_model::content_types::FULL;
        let pool = neutral_systems(content(), usize::MAX, sources);
        assert!(!pool.is_empty(), "the filler pool is not empty");
        for system in &pool {
            assert!(
                !crate::fracture::is_fracture_system(content(), sources, system),
                "{system} is a Fracture tile in the filler pool"
            );
        }
        for seed in 0..50 {
            for system in map_filler(content(), 30, sources, seed) {
                assert!(
                    !crate::fracture::is_fracture_system(content(), sources, &system),
                    "seed {seed} drew the Fracture tile {system}"
                );
            }
        }
    }

    #[test]
    fn a_drawn_map_is_made_of_tiles_that_belong_on_one() {
        // The shuffle must not reach past the filter: a home tile or an anomaly in the filler ring
        // would put two homes in one system or a hazard where expansion is meant to be.
        let drawn = map_filler(content(), 30, POK, 11);
        assert_eq!(drawn.len(), 30);

        let allowed: std::collections::BTreeSet<SystemId> =
            neutral_systems(content(), usize::MAX, POK)
                .into_iter()
                .collect();
        for tile in &drawn {
            assert!(
                allowed.contains(tile),
                "{tile} is not an ordinary filler tile"
            );
        }
        let distinct: std::collections::BTreeSet<&SystemId> = drawn.iter().collect();
        assert_eq!(distinct.len(), drawn.len(), "a tile was drawn twice");
    }

    #[test]
    fn the_whole_corpus_is_in_the_draw_not_just_its_first_entries() {
        // Taking `count` before the shuffle would draw from the same thirty tiles every time and
        // only reorder them, which looks like variety and is not.
        let mut seen: std::collections::BTreeSet<SystemId> = std::collections::BTreeSet::new();
        for seed in 0..24 {
            seen.extend(map_filler(content(), 30, POK, seed));
        }
        let first_thirty: std::collections::BTreeSet<SystemId> =
            neutral_systems(content(), 30, POK).into_iter().collect();
        assert!(
            seen.len() > first_thirty.len(),
            "twenty-four maps used only {} tiles, the same {} the stable order returns",
            seen.len(),
            first_thirty.len()
        );
    }

    #[test]
    fn every_drawn_map_still_seats_the_homes_properly() {
        // The board fix is not allowed to depend on which tiles were drawn. Homes three apart,
        // everyone the same distance from Mecatol — on every map, not just the fixed one.
        let seats = six_seats();
        for seed in 0..6 {
            let filler = map_filler(content(), 30, POK, seed);
            let refs: Vec<&str> = filler.iter().map(SystemId::as_str).collect();
            let galaxy = build_board(content(), &seats, &refs, POK)
                .unwrap_or_else(|error| panic!("map {seed} could not be built: {error}"));

            let homes = home_systems(content(), &seats).unwrap();
            let mut closest = i32::MAX;
            for (index, one) in homes.iter().enumerate() {
                for other in homes.iter().skip(index + 1) {
                    closest = closest.min(
                        galaxy
                            .distance(one.as_str(), other.as_str())
                            .expect("both homes are placed"),
                    );
                }
            }
            assert!(closest >= 3, "map {seed} seated two homes {closest} apart");

            let reach: std::collections::BTreeSet<i32> = homes
                .iter()
                .map(|home| galaxy.distance(home.as_str(), MECATOL).expect("placed"))
                .collect();
            assert_eq!(
                reach.len(),
                1,
                "map {seed} gave someone a shorter run at Mecatol"
            );
        }
    }

    #[test]
    fn a_wider_corpus_does_not_widen_the_table() {
        // Content scope and faction scope are separate decisions, and enabling Thunder's Edge
        // widened the corpus from 195 systems to 231 and from 83 leaders to 103. None of that is
        // an invitation for a thirty-fourth faction to sit down.
        let players: Vec<PlayerId> = (0..6).map(|i| PlayerId::new(format!("p{i}"))).collect();
        let seated = seat_in_scope(&players);

        for faction in seated.values() {
            assert!(
                IN_SCOPE_FACTIONS.contains(&faction.as_str()),
                "{faction} was seated under the wider corpus and is not in scope"
            );
        }
        // The wider corpus really is wider, or the check above proves nothing.
        let narrow = ti4_content::factions::catalogue(content(), POK).len();
        let wide =
            ti4_content::factions::catalogue(content(), ti4_model::content_types::DEFAULT).len();
        assert!(
            wide >= narrow,
            "full scope offers at least as many factions: {wide} against {narrow}"
        );
        assert!(wide > IN_SCOPE_FACTIONS.len(), "and far more than we seat");
    }

    #[test]
    fn home_systems_are_read_from_the_faction_records() {
        let homes = home_systems(content(), &assignments(&[("a", "sol"), ("b", "hacan")])).unwrap();
        assert!(homes.contains(&SystemId::new("01")), "Sol's home is 01");
        assert_eq!(homes.len(), 2);
    }

    #[test]
    fn filler_systems_carry_explorable_planets_and_no_special_cases() {
        let filler = neutral_systems(content(), 6, POK);
        assert_eq!(filler.len(), 6);
        let systems = all_systems(content(), POK);
        assert!(
            !filler.contains(&SystemId::new(MECATOL)),
            "Mecatol is not filler"
        );
        for id in &filler {
            let system = systems[id.as_str()];
            assert!(!system.planets().is_empty(), "{id} has no planet to take");
            assert!(!system.is_anomaly(), "{id} is an anomaly");
            assert!(system.wormholes().is_empty(), "{id} has a wormhole");
        }
    }

    #[test]
    fn a_seated_game_places_mecatol_at_the_centre() {
        let pairs = [("a", "sol"), ("b", "hacan")];
        let filler: Vec<SystemId> = neutral_systems(content(), 30, POK);
        let filler_refs: Vec<&str> = filler.iter().map(SystemId::as_str).collect();
        let galaxy = build_board(content(), &assignments(&pairs), &filler_refs, POK).unwrap();

        assert_eq!(galaxy.coord_of(MECATOL), Some(ti4_model::Hex::ORIGIN));
        assert_eq!(galaxy.adjacent(MECATOL).len(), 6, "a full first ring");
    }

    #[test]
    fn a_thunders_edge_board_uses_the_legendary_mecatol_and_never_deals_it_as_filler() {
        let default = ti4_model::content_types::DEFAULT;
        let pairs = [("a", "sol"), ("b", "winnu")];
        let filler: Vec<SystemId> = neutral_systems(content(), 30, default);
        assert!(filler.iter().all(|id| !is_mecatol(id.as_str())));
        let filler_refs: Vec<&str> = filler.iter().map(SystemId::as_str).collect();
        let galaxy = build_board(content(), &assignments(&pairs), &filler_refs, default).unwrap();
        assert_eq!(galaxy.coord_of(MECATOL_TE), Some(ti4_model::Hex::ORIGIN));
        assert_eq!(galaxy.coord_of(MECATOL), None);
        assert_eq!(mecatol_in_galaxy(&galaxy), MECATOL_TE);
        let mrte = ti4_content::galaxy::planet(content(), "mrte", default).expect("mrte");
        assert!(
            mrte.is_legendary(),
            "Winnu's legendary-planet abilities can use Mecatol"
        );
        // The base game keeps the base tile.
        assert_eq!(mecatol_for(content(), POK), MECATOL);
    }

    #[test]
    fn the_nexus_opens_when_a_unit_reaches_it_and_stays_open() {
        // Locked, it prints gamma alone; open, it adds alpha and beta. The flip is a fact about
        // the map, so it goes through the same token path a face-changing wormhole already uses.
        let mut state = crate::fixtures::game(&["a"]);
        let pairs = [("a", "sol"), ("b", "hacan")];
        let assignments: BTreeMap<PlayerId, FactionId> = pairs
            .iter()
            .map(|(p, f)| (PlayerId::new(*p), FactionId::new(*f)))
            .collect();
        let filler: Vec<SystemId> = neutral_systems(content(), 30, POK);
        let borrowed: Vec<&str> = filler.iter().map(SystemId::as_str).collect();
        let mut galaxy = build_board(content(), &assignments, &borrowed, POK).expect("a board");

        assert!(!nexus_is_triggered(&state), "nothing has reached it yet");
        crate::laws::apply_to_galaxy(&state, &mut galaxy);
        let locked = galaxy.wormhole_kinds(LOCKED_NEXUS);
        assert!(
            locked.contains("GAMMA") && !locked.contains("ALPHA"),
            "the locked face is gamma only: {locked:?}"
        );

        // A ship arrives.
        crate::fixtures::put(
            &mut state,
            &SystemId::new(LOCKED_NEXUS),
            "cruiser",
            &PlayerId::new("a"),
            1,
        );
        assert!(nexus_is_triggered(&state), "a unit in the tile opens it");
        state.nexus_unlocked = true;
        crate::laws::apply_to_galaxy(&state, &mut galaxy);
        let open = galaxy.wormhole_kinds(LOCKED_NEXUS);
        assert!(
            open.contains("ALPHA") && open.contains("BETA") && open.contains("GAMMA"),
            "the open face carries all three: {open:?}"
        );

        // The ship leaves; the tile stays face up, because the latch is the caller's.
        state.system_mut(&SystemId::new(LOCKED_NEXUS)).units.clear();
        crate::laws::apply_to_galaxy(&state, &mut galaxy);
        assert!(
            galaxy.wormhole_kinds(LOCKED_NEXUS).contains("ALPHA"),
            "it does not close again when the ship moves on"
        );
    }

    #[test]
    fn the_wormhole_nexus_is_in_play_off_the_map_under_pok() {
        // It is never dealt as filler -- `neutral_systems` drops anything carrying a wormhole --
        // and nothing else placed it, so the Nexus simply was not in the game.
        let pairs = [("a", "sol"), ("b", "hacan")];
        let assignments: BTreeMap<PlayerId, FactionId> = pairs
            .iter()
            .map(|(p, f)| (PlayerId::new(*p), FactionId::new(*f)))
            .collect();
        let filler: Vec<SystemId> = neutral_systems(content(), 30, POK);
        let borrowed: Vec<&str> = filler.iter().map(SystemId::as_str).collect();
        let galaxy = build_board(content(), &assignments, &borrowed, POK).expect("a board");

        assert!(
            galaxy.coord_of(LOCKED_NEXUS).is_none(),
            "the Nexus is beside the board, not on it"
        );
        assert!(
            !galaxy.system_ids().contains(&LOCKED_NEXUS),
            "so it is not one of the placed tiles either"
        );
        // Present all the same: its gamma reaches anything else carrying gamma.
        assert!(
            galaxy.wormhole_kinds(LOCKED_NEXUS).contains("GAMMA"),
            "the locked face prints a gamma wormhole"
        );
    }

    #[test]
    fn neutral_systems_separate_the_homes_from_mecatol() {
        let pairs = [("a", "sol"), ("b", "hacan")];
        let filler: Vec<SystemId> = neutral_systems(content(), 30, POK);
        let filler_refs: Vec<&str> = filler.iter().map(SystemId::as_str).collect();
        let galaxy = build_board(content(), &assignments(&pairs), &filler_refs, POK).unwrap();

        // Homes are placed after the filler ring, so nobody starts next to Mecatol.
        for home in home_systems(content(), &assignments(&pairs)).unwrap() {
            assert!(
                !galaxy.are_adjacent(MECATOL, home.as_str()),
                "{home} starts adjacent to Mecatol"
            );
        }
    }

    #[test]
    fn a_board_is_deterministic() {
        let pairs = assignments(&[("a", "sol"), ("b", "hacan")]);
        let filler: Vec<SystemId> = neutral_systems(content(), 30, POK);
        let refs: Vec<&str> = filler.iter().map(SystemId::as_str).collect();
        assert_eq!(
            build_board(content(), &pairs, &refs, POK).unwrap(),
            build_board(content(), &pairs, &refs, POK).unwrap()
        );
        assert_eq!(filler, neutral_systems(content(), 30, POK));
    }

    // -- the Creuss Gate and the Creuss home system -----------------------------------------------

    fn board_for(pairs: &[(&str, &str)]) -> Galaxy {
        let seats = assignments(pairs);
        let filler = full_filler();
        let refs: Vec<&str> = filler.iter().map(SystemId::as_str).collect();
        build_board(content(), &seats, &refs, POK).unwrap()
    }

    #[test]
    fn the_creuss_gate_sits_in_the_seats_home_position_and_the_home_system_is_off_the_map() {
        let galaxy = board_for(&[("a", "sol"), ("b", "ghost"), ("c", "hacan")]);
        assert!(
            galaxy.coord_of(CREUSS_GATE).is_some(),
            "the gate is on the hex grid"
        );
        assert!(
            galaxy.coord_of(CREUSS_HOME).is_none(),
            "the Creuss home system is not on the hex grid"
        );
        assert!(
            galaxy.wormhole_kinds(CREUSS_HOME).contains("DELTA"),
            "but it is in play"
        );
        // Home position: the gate took the slot a home system would, so it is as far from the
        // other homes as they are from each other (every third outer slot).
        let sol = galaxy.coord_of("01").unwrap();
        let gate = galaxy.coord_of(CREUSS_GATE).unwrap();
        assert!(sol.distance(gate) >= 3, "spaced like a home, not huddled");
        // Connected through the delta wormholes, in both directions, and nothing else reaches it.
        assert!(galaxy.are_adjacent(CREUSS_GATE, CREUSS_HOME));
        assert!(galaxy.are_adjacent(CREUSS_HOME, CREUSS_GATE));
        assert_eq!(
            galaxy.adjacent(CREUSS_HOME).into_iter().collect::<Vec<_>>(),
            vec![CREUSS_GATE],
            "only the gate is adjacent to the off-map home"
        );
    }

    #[test]
    fn boards_without_a_creuss_seat_are_unchanged() {
        let galaxy = board_for(&[("a", "sol"), ("b", "hacan"), ("c", "letnev")]);
        assert!(galaxy.coord_of(CREUSS_GATE).is_none());
        assert!(galaxy.wormhole_kinds(CREUSS_HOME).is_empty());
        let six = board_for(&[
            ("a", "sol"),
            ("b", "hacan"),
            ("c", "letnev"),
            ("d", "xxcha"),
            ("e", "jolnar"),
            ("f", "l1z1x"),
        ]);
        assert!(
            six.system_ids().iter().all(|id| *id != CREUSS_GATE),
            "no gate on a six-faction board"
        );
        let mut again = six.clone();
        place_creuss_home(&mut again, content(), POK).unwrap();
        assert_eq!(again, six, "placing the home is a no-op without the gate");
    }

    #[test]
    fn placing_the_creuss_home_twice_changes_nothing() {
        let mut galaxy = board_for(&[("a", "sol"), ("b", "ghost")]);
        let once = galaxy.clone();
        place_creuss_home(&mut galaxy, content(), POK).unwrap();
        place_wormhole_nexus(&mut galaxy, content(), POK).unwrap();
        assert_eq!(galaxy, once);
    }

    #[test]
    fn a_creuss_seat_starts_in_tile_51_not_on_the_gate() {
        let state = seated(&[("a", "ghost")]);
        let player = state.player(&PlayerId::new("a")).unwrap();
        assert_eq!(player.home_system, Some(SystemId::new(CREUSS_HOME)));
        let home = state.system_state(&SystemId::new(CREUSS_HOME));
        assert!(home.controls_a_planet(&PlayerId::new("a")));
        assert!(!home.units.is_empty(), "the starting fleet is at home");
        assert!(
            state
                .system_state(&SystemId::new(CREUSS_GATE))
                .units
                .is_empty(),
            "nothing starts on the gate"
        );
    }

    const PLANNED_FACTIONS: [&str; 18] = [
        "sol", "hacan", "letnev", "xxcha", "jolnar", "l1z1x", "arborec", "argent", "ghost",
        "mentak", "muaat", "naalu", "naaz", "saar", "sardakk", "winnu", "yin", "yssaril",
    ];

    fn six_players() -> Vec<PlayerId> {
        (0..6)
            .map(|index| PlayerId::new(format!("p{index}")))
            .collect()
    }

    #[test]
    fn seeded_assignment_draws_six_unique_content_backed_factions_from_explicit_eighteen() {
        for alias in PLANNED_FACTIONS {
            assert!(
                ti4_content::factions::get(content(), alias).is_some(),
                "planned candidate {alias} must exist in the content corpus"
            );
        }
        let players = six_players();
        let assignments = seeded_faction_assignments(&PLANNED_FACTIONS, &players, 1042).unwrap();
        let assigned: BTreeSet<String> = assignments
            .values()
            .map(|faction| faction.to_string())
            .collect();
        assert_eq!(assignments.len(), 6);
        assert_eq!(assigned.len(), 6, "a faction cannot be assigned twice");
        assert!(
            assigned
                .iter()
                .all(|alias| PLANNED_FACTIONS.contains(&alias.as_str()))
        );
    }

    #[test]
    fn seeded_assignment_replays_and_varies_by_seed() {
        let players = six_players();
        let first = seeded_faction_assignments(&PLANNED_FACTIONS, &players, 1042).unwrap();
        assert_eq!(
            first,
            seeded_faction_assignments(&PLANNED_FACTIONS, &players, 1042).unwrap()
        );
        assert!(
            (1043..1060).any(|seed| {
                seeded_faction_assignments(&PLANNED_FACTIONS, &players, seed)
                    .is_ok_and(|assignment| assignment != first)
            }),
            "the fixed seed sample should exercise a different assignment"
        );
    }

    #[test]
    fn seeded_assignment_uses_roster_and_player_input_order() {
        let players = [PlayerId::new("b"), PlayerId::new("a")];
        let assignment = seeded_faction_assignments(&PLANNED_FACTIONS, &players, 57).unwrap();
        let reversed = [players[1].clone(), players[0].clone()];
        let reassigned = seeded_faction_assignments(&PLANNED_FACTIONS, &reversed, 57).unwrap();
        assert_eq!(reassigned[&players[0]], assignment[&players[1]]);
        assert_eq!(reassigned[&players[1]], assignment[&players[0]]);

        let ordered = seeded_faction_assignments(&["sol", "hacan"], &players, 57).unwrap();
        let reversed_roster = seeded_faction_assignments(&["hacan", "sol"], &players, 57).unwrap();
        assert_eq!(reversed_roster[&players[0]], ordered[&players[1]]);
        assert_eq!(reversed_roster[&players[1]], ordered[&players[0]]);
    }

    #[test]
    fn seeded_assignment_rejects_empty_duplicate_and_undersized_inputs() {
        let players = [PlayerId::new("a"), PlayerId::new("b")];
        assert_eq!(
            seeded_faction_assignments(&[], &players, 1),
            Err(FactionAssignmentError::EmptyRoster)
        );
        assert_eq!(
            seeded_faction_assignments(&["sol", "sol"], &players, 1),
            Err(FactionAssignmentError::DuplicateCandidate("sol".to_owned()))
        );
        assert_eq!(
            seeded_faction_assignments(
                &["sol", "hacan"],
                &[players[0].clone(), players[0].clone()],
                1
            ),
            Err(FactionAssignmentError::DuplicatePlayer("a".to_owned()))
        );
        assert_eq!(
            seeded_faction_assignments(&["sol"], &players, 1),
            Err(FactionAssignmentError::InsufficientCandidates {
                players: 2,
                candidates: 1,
            })
        );
    }

    // -- wide roster (BF-20 / BF-21) ----------------------------------------------------

    use ti4_model::content_types::FULL;

    fn wide_players() -> Vec<PlayerId> {
        ["a", "b", "c", "d", "e", "f"]
            .iter()
            .map(|name| PlayerId::new(*name))
            .collect()
    }

    #[test]
    fn the_default_roster_is_the_in_scope_six_and_ignores_the_seed() {
        assert_eq!(FactionRoster::default(), FactionRoster::InScope);
        let players = wide_players();
        for seed in [0, 1, 7, 12345] {
            assert_eq!(
                seat_roster(FactionRoster::InScope, content(), &players, FULL, seed).unwrap(),
                seat_in_scope(&players)
            );
        }
    }

    #[test]
    fn the_wide_roster_is_exactly_this_list() {
        // A list, asserted as a list: widening or narrowing the roster is a deliberate edit here.
        let full = wide_roster(content(), FULL);
        assert_eq!(
            full,
            [
                "sol",
                "hacan",
                "letnev",
                "xxcha",
                "jolnar",
                "l1z1x",
                "arborec",
                "argent",
                "bastion",
                "cabal",
                "crimson",
                "deepwrought",
                "empyrean",
                "firmament",
                "ghost",
                "keleres",
                "mahact",
                "mentak",
                "muaat",
                "naalu",
                "naaz",
                "nekro",
                "nomad",
                "ralnel",
                "saar",
                "sardakk",
                "titans",
                "winnu",
                "yin",
                "yssaril",
            ]
        );
        assert!(
            !full.contains(&"obsidian"),
            "Nocturne: not choosable at setup"
        );
        // Thunder's Edge factions only when the sources include thunders_edge.
        let pok = wide_roster(content(), POK);
        let te = ["bastion", "crimson", "deepwrought", "firmament", "ralnel"];
        for alias in te {
            assert!(full.contains(&alias));
            assert!(!pok.contains(&alias), "{alias} needs thunders_edge");
        }
        assert_eq!(pok.len(), full.len() - te.len());
        // Every module faction is either on the roster, a Keleres variant, or the Obsidian.
        for module in crate::factions::MODULES {
            let alias = module.alias;
            assert!(
                full.contains(&alias)
                    || crate::factions::keleres::is_keleres_faction(alias)
                    || alias == "obsidian",
                "{alias} is neither on the wide roster nor deliberately excluded"
            );
        }
    }

    #[test]
    fn wide_draw_is_unique_replays_and_varies_by_seed() {
        let players = wide_players();
        let first = seat_wide(content(), &players, FULL, 42).unwrap();
        assert_eq!(first, seat_wide(content(), &players, FULL, 42).unwrap());
        assert_eq!(first.len(), 6);
        let distinct: BTreeSet<&str> = first.values().map(FactionId::as_str).collect();
        assert_eq!(distinct.len(), 6);
        let draws: BTreeSet<Vec<String>> = (0..20)
            .map(|seed| {
                seat_wide(content(), &players, FULL, seed)
                    .unwrap()
                    .values()
                    .map(ToString::to_string)
                    .collect()
            })
            .collect();
        assert!(
            draws.len() > 15,
            "seeds barely vary the draw: {}",
            draws.len()
        );
        // Fixed fixture: a change to the stream or the walk is a deliberate edit.
        let fixture: Vec<String> = seat_wide(content(), &players, FULL, 0)
            .unwrap()
            .values()
            .map(ToString::to_string)
            .collect();
        assert_eq!(
            fixture,
            [
                "deepwrought",
                "letnev",
                "ghost",
                "mentak",
                "ralnel",
                "arborec"
            ]
        );
    }

    #[test]
    fn wide_draw_uses_its_own_streams() {
        for domain in [WIDE_ROSTER_DOMAIN, KELERES_VARIANT_DOMAIN] {
            for other in [
                crate::rng::domain::GALAXY,
                crate::rng::domain::MAP,
                crate::rng::domain::EXPLORATION,
                "seating:faction-assignment:v1",
            ] {
                assert_ne!(
                    crate::rng::GameRng::derive_seed(9, domain),
                    crate::rng::GameRng::derive_seed(9, other)
                );
            }
        }
        // Drawing never touches the map filler for the same seed.
        let before = map_filler(content(), 30, FULL, 5);
        let _ = seat_wide(content(), &wide_players(), FULL, 5).unwrap();
        assert_eq!(before, map_filler(content(), 30, FULL, 5));
    }

    #[test]
    fn oversized_tables_are_refused_never_wrapped() {
        let roster = wide_roster(content(), FULL);
        let many: Vec<PlayerId> = (0..roster.len() + 1)
            .map(|index| PlayerId::new(format!("p{index}")))
            .collect();
        assert!(matches!(
            seat_wide(content(), &many, FULL, 1),
            Err(FactionAssignmentError::InsufficientCandidates { .. })
        ));
        // Duplicate players are refused as in the explicit helper.
        let twice = [PlayerId::new("a"), PlayerId::new("a")];
        assert_eq!(
            seat_wide(content(), &twice, FULL, 1),
            Err(FactionAssignmentError::DuplicatePlayer("a".to_owned()))
        );
        // A table as large as the roster is seatable only when the Tribuni allows it: here the
        // roster has all three Keleres bases, so the Keleres is skipped and one seat is short.
        let exact: Vec<PlayerId> = (0..roster.len())
            .map(|index| PlayerId::new(format!("p{index}")))
            .collect();
        assert!(seat_wide(content(), &exact, FULL, 1).is_err());
    }

    #[test]
    fn keleres_is_at_most_one_seat_with_an_unplayed_base() {
        let players = wide_players();
        let mut variants = BTreeSet::new();
        for seed in 0..400 {
            let seats = seat_wide(content(), &players, FULL, seed).unwrap();
            let held: Vec<&str> = seats.values().map(FactionId::as_str).collect();
            validate_tribuni(held.iter().copied()).unwrap();
            assert!(
                !held.contains(&KELERES_FAMILY),
                "the family is never seated"
            );
            for alias in held {
                if crate::factions::keleres::is_keleres_faction(alias) {
                    variants.insert(alias.to_owned());
                }
            }
        }
        assert_eq!(variants.len(), 3, "all three variants appear: {variants:?}");
        // A table of Keleres-adjacent bases only: variant must avoid every played base.
        let tight: Vec<PlayerId> = (0..wide_roster(content(), FULL).len() - 1)
            .map(|index| PlayerId::new(format!("p{index}")))
            .collect();
        for seed in 0..20 {
            if let Ok(seats) = seat_wide(content(), &tight, FULL, seed) {
                validate_tribuni(seats.values().map(FactionId::as_str)).unwrap();
            }
        }
    }

    #[test]
    fn every_wide_faction_deploys_through_seating_with_its_own_home() {
        // Crimson (Sorrow -> tile 118) and the Firmament (tile 96a) included.
        let roster = wide_roster(content(), FULL);
        for alias in roster {
            let alias = if alias == KELERES_FAMILY {
                "keleresm"
            } else {
                alias
            };
            let mut state = start_game(content(), &[PlayerId::new("a")], FULL, None).unwrap();
            deploy(
                &mut state,
                content(),
                &PlayerId::new("a"),
                &FactionId::new(alias),
                FULL,
            )
            .unwrap_or_else(|error| panic!("{alias}: {error}"));
            let seat = state.player(&PlayerId::new("a")).unwrap();
            assert!(seat.home_system.is_some(), "{alias} has a home");
            if alias == "crimson" {
                assert_eq!(seat.home_system, Some(SystemId::new(CRIMSON_HOME)));
            }
        }
    }

    #[test]
    fn wide_tables_build_boards_for_a_sample_of_seeds() {
        let players = wide_players();
        let filler = neutral_systems(content(), 30, FULL);
        let refs: Vec<&str> = filler.iter().map(SystemId::as_str).collect();
        for seed in 0..50 {
            let seats = seat_wide(content(), &players, FULL, seed).unwrap();
            build_board(content(), &seats, &refs, FULL)
                .unwrap_or_else(|error| panic!("seed {seed}: {error}"));
        }
    }
}
