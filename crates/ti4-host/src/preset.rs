//! Start presets: prepared opening states for random smoke playthroughs.
//!
//! A random click policy rarely reaches combat, casualties, Mecatol Rex or the agenda phase from
//! the standard opening. A preset only changes the *starting state* (extra fleets, a little trade
//! goods); it never changes a rule. The result is stored in the game's initial state, so crash
//! recovery restores it from the saved state rather than rebuilding it.

use std::collections::{BTreeMap, BTreeSet};

use ti4_content::ContentStore;
use ti4_content::factions;
use ti4_content::galaxy::{self, Galaxy};
use ti4_content::units;
use ti4_engine::seating::{self, MECATOL};
use ti4_engine::{fleet, invasion, leaders, production};
use ti4_model::content_types::{ContentType, POK};
use ti4_model::id::{
    ActionCardId, FactionId, PlanetId, PlayerId, RelicId, SystemId, TechnologyId, UnitTypeId,
};
use ti4_model::state::{GameState, LeaderStatus};
use ti4_model::units::Unit;

/// Fleets beside opponents' home systems, one raider already in Mecatol Rex and the others beside
/// it, with influence for the custodians.
pub const COMBAT: &str = "combat";

/// Every seat holds a hand of action cards that random play has almost never produced.
pub const CARDS: &str = "cards";

/// The agenda phase from round one, with a hand-ordered deck and agenda-window action cards.
pub const AGENDA: &str = "agenda";

/// Every seat holds relics, one of them with a cultural fragment set for crossing.
pub const RELICS: &str = "relics";

/// Each seat holds a defended colony (infantry, mech, PDS on a planet near its home) and the
/// previous seat's invasion force, with a dreadnought, waits in its space area: space cannon,
/// bombardment and ground combat with casualties are the first thing a tactical action reaches.
pub const INVASION: &str = "invasion";

/// The invasion setup, with every seat owning the technologies that open their own prompts.
pub const TECHS: &str = "techs";

/// Every seat holds all its leaders (heroes and commanders unlocked, agents readied), the factions
/// are rotated so Jol-Nar and L1Z1X sit at small tables, and legendary planets on the map are
/// handed out.
pub const LEADERS: &str = "leaders";

/// Round five with two seats on 9 victory points, the rest on 8, and no public objectives left to
/// reveal: scoring one more point or the status phase ends the game (`game_over`).
pub const ENDGAME: &str = "endgame";

/// The invasion setup with PDS colonies that also hold a defending fleet, and invaders with
/// destroyers, fighters and three kinds of non-fighter ship: space cannon (offense and defense),
/// anti-fighter barrage and Assault Cannon fire before the first move is made, ships are lost for
/// Courageous to the End, and every seat holds the action cards that answer those windows.
pub const SIEGE: &str = "siege";

/// Colonies without PDS (so the planetary shield is down) that hold infantry and a mech, invaded by
/// two dreadnoughts: bombardment and ground combat with a sustaining mech. With three or more
/// seats a lone infantry of a third seat shares each planet, which is what makes the invader
/// choose whose units a bombardment hits.
pub const BOMBARD: &str = "bombard";

/// The Vuil'raith Cabal sits at the table with Vortex ready, captured units of every other seat on
/// its sheet and the other seats' fleets beside its home: capture, and the returns of captured
/// units (Amalgamation, Riftmeld, the agent).
pub const CAPTURE: &str = "capture";

/// Every seat has an unclaimed planet with an exploration trait beside its fleet (landing
/// explores it), a ship pair in a frontier system with Dark Energy Tap, relic fragments to cross,
/// and exploration decks stacked so the first draws are the cards that ask the player something,
/// interleaved with fragments and attachments.
pub const EXPLORE: &str = "explore";

/// Every seat holds other seats' promissory notes (each kind there is, their faction ones too) and the
/// agenda phase runs from round one (custodians gone), so
/// the play, return and give flows have something to work on from the first turn.
pub const NOTES: &str = "notes";

/// Factions outside the original six, seated by seed from [`ROSTER`], each with its faction
/// technologies, every leader usable, its flagship and mech beside a defended colony, and the
/// other seats' promissory notes in hand: faction abilities have something to decide from round one.
pub const WORLD: &str = "world";

/// Suffix on any preset name that also rotates the factions (see [`rotation`]).
pub const ROTATE: &str = "+rot";

/// Suffix on any preset name that seats factions from [`ROSTER`] by seed (`+fac`), or lists the
/// first seats' factions (`+fac:naalu:mentak`) and fills the rest by seed. `world` always does.
pub const FACTIONS: &str = "+fac";

/// Suffix on any preset name that makes the game short: every seat starts on a few victory points
/// and only three public objectives are left to reveal, so the game ends in a few rounds.
pub const SHORT: &str = "+short";

/// Every preset name the server accepts. Keep in step with `KNOWN_PRESETS` in
/// `web/e2e/smokePreset.ts` (a test below compares the two).
pub const KNOWN: &[&str] = &[
    COMBAT, CARDS, AGENDA, RELICS, INVASION, TECHS, LEADERS, ENDGAME, SIEGE, BOMBARD, CAPTURE,
    EXPLORE, NOTES, WORLD,
];

/// Factions a preset may seat, in the order the seeded walk visits them: the original six, then
/// the Prophecy of Kings factions with an engine module. Left out: the Obsidian (cannot be chosen
/// during setup), the three Keleres variants (each takes another faction's home system, so only an
/// explicit `+fac:keleresm` seats one) and the Thunder's Edge factions (Bastion, Crimson Rebellion,
/// Deepwrought, Firmament, Ral Nel): their home tiles are not in the Prophecy of Kings source set
/// that every server game is built from, so seating one fails at setup ("no system 92 in the
/// corpus for the requested sources").
pub const ROSTER: &[&str] = &[
    "sol", "hacan", "letnev", "xxcha", "jolnar", "l1z1x", "arborec", "argent", "cabal", "empyrean",
    "ghost", "mahact", "mentak", "muaat", "naalu", "naaz", "nekro", "nomad", "saar", "sardakk",
    "titans", "winnu", "yin", "yssaril",
];

/// Roster factions the seeded walk skips, so a nightly game is not lost to a known product bug;
/// `+fac:<name>` still seats them. Empty now: Arborec was held out while every game that seated it
/// ended in a replay divergence (see `tests/arborec_batch_replay.rs`).
pub const HELD_OUT: &[&str] = &[];

/// Exploration cards drawn first, per deck, in this order: the ones that ask the player something
/// (reward choices, "remove an infantry" payments, a production offer), with a fragment and an
/// attachment between them. Cards the corpus lacks are skipped.
const EXPLORE_TOPS: &[(&str, &[&str])] = &[
    (
        "CULTURAL",
        &["mo1", "toe", "crf1", "frln1", "pw", "mo2", "crf2", "frln2"],
    ),
    (
        "HAZARDOUS",
        &["cm1", "rw", "hrf1", "exp1", "mw", "vfs1", "hrf2", "cm2"],
    ),
    (
        "INDUSTRIAL",
        &[
            "aw1",
            "biotic",
            "irf1",
            "fb1",
            "cybernetic",
            "lf1",
            "irf2",
            "aw2",
        ],
    ),
    (
        "FRONTIER",
        &["ms1", "ed1", "urf1", "dv1", "lc1", "ion", "urf2", "ms2"],
    ),
];

/// Fragments dealt to seats at the start of `explore`: three of one kind crosses into a relic.
const EXPLORE_FRAGMENTS: &[(&str, i32)] = &[("CULTURAL", 3), ("HAZARDOUS", 2), ("INDUSTRIAL", 3)];

/// Fleet beside an unclaimed trait planet in `explore`: the landing party explores it. Three
/// non-fighter ships fill the opening fleet supply.
const EXPLORE_FLEET: &[(&str, usize)] = &[("carrier", 1), ("cruiser", 1), ("infantry", 3)];

/// A second, lighter landing party per seat in `explore` (the faction mech rides along).
const EXPLORE_FLEET_LIGHT: &[(&str, usize)] = &[("carrier", 1), ("infantry", 2)];

/// A ship pair placed in a frontier system in `explore`.
const FRONTIER_FLEET: &[(&str, usize)] = &[("cruiser", 1), ("destroyer", 1)];

/// The fleet `world` puts in a defended colony's space area: the faction's flagship and mech ride
/// with the landing force.
const WORLD_FLEET: &[(&str, usize)] = &[
    ("carrier", 1),
    ("flagship", 1),
    ("cruiser", 1),
    ("infantry", 3),
];

/// Action cards no nightly game ever played, found by diffing 72 final states' discard piles
/// against the corpus. The first ten are in the standard deck; the rest are Thunder's Edge cards,
/// which a standard game never deals, so a hand is the only way their windows and prompts get
/// exercised at all. Cards the corpus lacks are skipped.
const CARD_POOL: &[&str] = &[
    "confusing",
    "dh2",
    "mjets1",
    "parley",
    "upgrade",
    "ghost_squad",
    "rally",
    "war_machine2",
    "decoy",
    "reparations",
    "crashlanding",
    "exchangeprogram",
    "lieinwait",
    "puppetsonastring",
    "extremeduress",
    "rescue",
    "strategize1",
    "piratecontract1",
    "blackmarketdealing",
    "overrule",
    "mercenarycontract",
    "crisis",
];

/// The top of the agenda deck, two per round: round 1 an elect-player law and a for/against law
/// (so a law is in play), round 2 an elect-law agenda (it is discarded when no law is in play,
/// hence the laws first), an elect-planet-style directive, then more elections and laws. Aliases
/// the corpus lacks are skipped.
const AGENDA_ORDER: &[&str] = &[
    "committee",
    "defense_act",
    "abolishment",
    "redistribution",
    "miscount",
    "classified",
    "rep_govt",
    "crisis",
    "disarmament",
    "arbiter",
    "covert",
    "execution",
    "checks",
];

/// Action cards with agenda windows (two riders, a veto, hacking, bribery, confusing text), dealt
/// so that every revealed agenda gives some seat something to play. Only two riders: a player who
/// predicted an outcome may not vote (vote.rs), so a rider in every hand, played at the first
/// reveal, leaves nobody to vote and the agenda phase never asks `cast_vote` (seen in the first
/// smoke run of this preset).
const AGENDA_CARD_POOL: &[&str] = &[
    "lead_rider",
    "imp_rider",
    "bribery",
    "hack",
    "sanction",
    "insider",
    "veto",
    "assassin",
    "distinguished",
    "confounding",
    "deadly_plot",
    "confusing",
    "abs",
    "dp1",
];

/// Trade goods each seat gets, enough to matter for votes and riders without a purchase spree.
const AGENDA_TRADE_GOODS: i32 = 3;

/// Relics, dealt round-robin in this order so the interesting ones always land: a standard game
/// reaches relics only through exploration and fragments, so almost none was ever held. Thunder's
/// Edge's Heart of Ixth is included for its die-adjust window; the rest are the standard deck's.
/// Relics the corpus lacks are skipped.
const RELIC_POOL: &[&str] = &[
    "neuraloop",
    "stellarconverter",
    "dominusorb",
    "emphidia",
    "titanprototype",
    "heartofixth",
    "thalnos",
    "codex",
    "bookoflatvinia",
    "prophetstears",
    "mawofworlds",
    "enigmaticdevice",
    "dynamiscore",
    "emelpar",
    "nanoforge",
    "circletofthevoid",
];

/// Relics per seat at most.
const RELICS_PER_SEAT: usize = 3;

/// Technologies owned from the start in the `techs` preset: the ones with their own prompts or
/// exhaust windows (Quantum Datahub Node, Spatial Conduit Cylinders, Nullification Field, Chaos
/// Mapping, Bio-Stims, Transit Diodes, AI Development Algorithm, Psychoarchaeology, Sling Relay,
/// Magen Defense Grid, Scanlink Drone Network, Supercharge, Vortex), plus the production and
/// bombardment ones that change what an invasion costs and does. Prerequisites are not checked.
const TECH_POOL: &[&str] = &[
    "qdn", "scc", "nf", "cm", "bs", "td", "aida", "pa", "sr", "md", "sdn", "sc", "vtx", "st", "mc",
    "l4", "x89c4",
];

/// Cards per seat: well under the hand limit of 7, so nothing is discarded at the status phase.
const HAND_SIZE: usize = 5;

/// Placed one jump from an opponent's home. The dreadnought (with the faction's mech) makes sure
/// sustain damage happens; capacity 5: two fighters, two infantry and the mech fit. Three
/// non-fighter ships fill the opening fleet supply of three.
const STRIKE_FLEET: &[(&str, usize)] = &[
    ("carrier", 1),
    ("destroyer", 1),
    ("dreadnought", 1),
    ("fighter", 2),
    ("infantry", 2),
];

/// Placed in or beside Mecatol Rex with ground forces, so the custodians can be lifted. Ground
/// forces in a system's space area are landable (see `invasion::landable`), so the one already in
/// Mecatol can lift the custodians the first time it activates the system.
const RAIDING_PARTY: &[(&str, usize)] = &[
    ("carrier", 1),
    ("cruiser", 1),
    ("dreadnought", 1),
    ("infantry", 2),
];

/// Placed in a defended colony's space area. The carrier and the dreadnought's own slot carry four
/// infantry and the faction's mech (capacity 5); the dreadnought bombards, and with the mech and
/// the defenders' casualties there is a choice of unit to assign hits to on both sides.
const INVASION_FLEET: &[(&str, usize)] = &[
    ("carrier", 1),
    ("dreadnought", 1),
    ("cruiser", 1),
    ("infantry", 4),
];

/// What a colony's planet holds: ground forces to fight and a PDS (space cannon) to shoot first.
const COLONY_DEFENDERS: &[(&str, usize)] = &[("infantry", 3), ("pds", 2)];

/// What a colony in the `siege` preset holds in its space area: three kinds of non-fighter ship
/// (Assault Cannon needs three and makes the loser choose which to give up) and two fighters for
/// the barrage to shoot at. The seat's own mech is not added to a ship list.
const SIEGE_GARRISON: &[(&str, usize)] = &[
    ("carrier", 1),
    ("cruiser", 1),
    ("destroyer", 1),
    ("fighter", 2),
];

/// The siege invader: carrier, dreadnought and destroyer (three kinds of non-fighter ship, a
/// barrage of its own), two fighters, two infantry and the faction's mech; capacity 5 is full.
const SIEGE_FLEET: &[(&str, usize)] = &[
    ("carrier", 1),
    ("dreadnought", 1),
    ("destroyer", 1),
    ("fighter", 2),
    ("infantry", 2),
];

/// The `bombard` invader: two dreadnoughts roll bombardment, the carrier brings four infantry and
/// the faction's mech (capacity 6).
const BOMBARD_FLEET: &[(&str, usize)] = &[("carrier", 1), ("dreadnought", 2), ("infantry", 3)];

/// The `bombard` defenders: no PDS, so no planetary shield.
const BOMBARD_DEFENDERS: &[(&str, usize)] = &[("infantry", 2)];

/// Action cards of the windows around space cannon, anti-fighter barrage, bombardment, ground
/// combat and ship losses: Waylay and Scramble Frequency (barrage and cannon rolls), Maneuvering
/// Jets (cancel a cannon hit), Shields Holding, Direct Hit, Courageous to the End, Disable and
/// Experimental Battlestation (cannon), Bunker and Blitz (bombardment), Fire Team (ground rolls),
/// Parley and Ghost Squad (landing) and Tactical Bombardment (an action).
const SIEGE_CARD_POOL: &[&str] = &[
    "waylay",
    "mjets1",
    "courageous",
    "scramble",
    "sh1",
    "dh1",
    "disable",
    "experimental",
    "parley",
    "fire_team",
    "ghost_squad",
    "bunker",
    "blitz",
    "tactical",
];

/// The bombardment-side cards of the pool above, for the `bombard` preset.
const BOMBARD_CARD_POOL: &[&str] = &[
    "bunker",
    "blitz",
    "tactical",
    "fire_team",
    "parley",
    "ghost_squad",
    "scramble",
    "sh1",
    "courageous",
    "dh1",
];

/// Technologies of the `siege` preset: Assault Cannon (the opening loss), Plasma Scoring, PDS II
/// and Destroyer II (a longer reach and a stronger barrage), Duranium Armor and the Magen Defense
/// Grid. Prerequisites are not checked.
const SIEGE_TECHS: &[&str] = &["asc", "ps", "pds2", "dd2", "da", "md", "ff2"];

/// Technologies of the `bombard` preset: both X-89 steps, Plasma Scoring and Dreadnought II.
const BOMBARD_TECHS: &[&str] = &["x89c4", "ps", "dn2", "inf2", "da"];

/// Technologies of the `capture` preset for the Cabal: Vortex and its upgrades' prerequisites'
/// neighbours (Dimensional Tear II, Self-Assembly), and unit upgrades that Riftmeld can waive.
const CAPTURE_CABAL_TECHS: &[&str] = &["vtx", "dt2", "sar"];

/// What the Cabal holds captured at the start, one model from each other seat: a fighter, an
/// infantry, a destroyer, a cruiser and a mech are the types Amalgamation and Riftmeld ask about.
const CAPTURED_TYPES: &[&str] = &["infantry", "fighter", "destroyer", "cruiser", "carrier"];

/// The Cabal's second front: every other seat's fleet waits beside the Cabal's home, within
/// Vortex's reach and in the Cabal's way.
const CAPTURE_RAIDERS: &[(&str, usize)] = &[("cruiser", 1), ("destroyer", 1), ("dreadnought", 1)];

/// Reinforcement pool sizes (LRR 76.1): a preset never puts more of a type on the board.
const POOL: &[(&str, usize)] = &[
    ("carrier", 4),
    ("cruiser", 8),
    ("destroyer", 8),
    ("dreadnought", 5),
    ("fighter", 10),
    ("infantry", 12),
    ("mech", 4),
    ("pds", 6),
];

#[must_use]
pub fn is_known(name: &str) -> bool {
    parse(name).is_some()
}

/// A preset name taken apart: `<base>[+rot][+short][+agenda][+fac[:alias...]]` in any order.
struct Parts<'a> {
    base: &'a str,
    rotate: bool,
    short: bool,
    /// `+agenda`: the custodians are gone from the start, so every round has an agenda phase.
    agenda: bool,
    /// `Some` when factions come from [`ROSTER`]: the aliases asked for by name, first seats first.
    factions: Option<Vec<&'a str>>,
}

fn parse(name: &str) -> Option<Parts<'_>> {
    let mut tokens = name.split('+');
    let base = tokens.next()?;
    if !KNOWN.contains(&base) {
        return None;
    }
    let mut parts = Parts {
        base,
        rotate: false,
        short: false,
        agenda: false,
        factions: (base == WORLD).then(Vec::new),
    };
    for token in tokens {
        match token {
            "rot" => parts.rotate = true,
            "short" => parts.short = true,
            "agenda" => parts.agenda = true,
            _ => {
                let mut pieces = token.split(':');
                if pieces.next() != Some("fac") {
                    return None;
                }
                let listed: Vec<&str> = pieces.collect();
                let known = |alias: &&str| ROSTER.contains(alias) || alias.starts_with("keleres");
                if !listed.iter().all(known) {
                    return None;
                }
                parts.factions = Some(listed);
            }
        }
    }
    Some(parts)
}

/// How far the factions are rotated along `IN_SCOPE_FACTIONS` for this preset and seed: 0 unless
/// the preset is `leaders` or has the `+rot` suffix, else 2 to 5, which always seats Jol-Nar or
/// L1Z1X even at three seats (the fixed order reaches them only at five). Seeded, so the same
/// request builds the same table. The game creator and the preset both ask here, so the home
/// systems the preset looks up are those of the factions actually seated.
#[must_use]
pub fn rotation(name: &str, seed: u64) -> usize {
    if parse(name).is_some_and(|parts| parts.rotate || parts.base == LEADERS) {
        2 + (mix(seed, 900) % 4) as usize
    } else {
        0
    }
}

/// The factions this preset seats when it does not use the in-scope six in order: `None` for
/// every preset without `+fac` (and not `world`). Seats named in the request come first; the rest
/// walk [`ROSTER`] from a seeded start in steps coprime to its length, so a table never repeats a
/// faction and different seeds pair different factions.
#[must_use]
pub fn roster(
    name: &str,
    seed: u64,
    players: &[PlayerId],
) -> Option<BTreeMap<PlayerId, FactionId>> {
    let listed = parse(name)?.factions?;
    let mut taken: Vec<&str> = listed.iter().copied().take(players.len()).collect();
    let walk: Vec<&str> = ROSTER
        .iter()
        .copied()
        .filter(|a| !HELD_OUT.contains(a))
        .collect();
    let len = walk.len();
    let step = (3..).step_by(2).find(|s| gcd(*s, len) == 1).unwrap_or(1);
    let mut at = (mix(seed, 950) % len as u64) as usize;
    while taken.len() < players.len() {
        let alias = walk[at % len];
        at += step;
        if !taken.contains(&alias) {
            taken.push(alias);
        }
    }
    Some(
        players
            .iter()
            .cloned()
            .zip(taken.into_iter().map(FactionId::new))
            .collect(),
    )
}

fn gcd(a: usize, b: usize) -> usize {
    if b == 0 { a } else { gcd(b, a % b) }
}

/// Applies the named preset to a freshly seated game.
///
/// # Errors
/// An unknown preset name, or a placement that would break fleet supply, capacity or a unit pool.
pub fn apply(
    content: &ContentStore,
    state: &mut GameState,
    galaxy: &Galaxy,
    players: &[PlayerId],
    seed: u64,
    preset: &str,
) -> Result<(), String> {
    let Some(parts) = parse(preset) else {
        return Err(format!("unknown start_preset {preset:?}"));
    };
    apply_base(content, state, galaxy, players, seed, parts.base)?;
    // A seat's units are its own faction's and its upgraded versions (LRR 90): the fleets above
    // are written with generic ids, and the techs a preset grants replace them at once.
    for player in players {
        ti4_engine::technology::apply_unit_upgrades(state, content, POK, player);
    }
    if parts.short {
        short_game(state, players, seed);
    }
    if parts.agenda {
        agenda_suffix(state, players);
    }
    Ok(())
}

fn apply_base(
    content: &ContentStore,
    state: &mut GameState,
    galaxy: &Galaxy,
    players: &[PlayerId],
    seed: u64,
    preset: &str,
) -> Result<(), String> {
    match preset {
        EXPLORE => explore_preset(content, state, galaxy, players, seed),
        NOTES => {
            deal_notes(content, state, players, 2);
            // The agenda phase from round one: Political Secret, Alliance and the other notes with
            // agenda windows have something to answer, and trade goods leave room to buy a note.
            state.custodians_removed = true;
            for player in players {
                if let Some(seat) = state.player_mut(player) {
                    seat.trade_goods += AGENDA_TRADE_GOODS;
                }
            }
            Ok(())
        }
        WORLD => {
            invasion_preset(content, state, galaxy, players, seed, &ColonySpec::WORLD)?;
            world_kit(content, state, galaxy, players);
            deal_notes(content, state, players, 2);
            Ok(())
        }
        COMBAT => combat(content, state, galaxy, players, seed),
        LEADERS => {
            unlock_leaders(content, state, galaxy, players);
            Ok(())
        }
        ENDGAME => {
            endgame(state, players, seed);
            Ok(())
        }
        CARDS => {
            deal_cards(content, state, players, seed, CARD_POOL, HAND_SIZE);
            Ok(())
        }
        AGENDA => {
            agenda(content, state, players, seed);
            Ok(())
        }
        RELICS => {
            deal_relics(content, state, players, seed);
            Ok(())
        }
        INVASION => invasion_preset(content, state, galaxy, players, seed, &ColonySpec::INVASION),
        TECHS => {
            invasion_preset(content, state, galaxy, players, seed, &ColonySpec::INVASION)?;
            grant_techs(content, state, players, TECH_POOL);
            Ok(())
        }
        SIEGE => {
            invasion_preset(content, state, galaxy, players, seed, &ColonySpec::SIEGE)?;
            grant_techs(content, state, players, SIEGE_TECHS);
            deal_cards(content, state, players, seed, SIEGE_CARD_POOL, SIEGE_HAND);
            Ok(())
        }
        BOMBARD => {
            invasion_preset(content, state, galaxy, players, seed, &ColonySpec::BOMBARD)?;
            grant_techs(content, state, players, BOMBARD_TECHS);
            deal_cards(content, state, players, seed, BOMBARD_CARD_POOL, SIEGE_HAND);
            Ok(())
        }
        CAPTURE => capture(content, state, galaxy, players, seed),
        other => Err(format!("unknown start_preset {other:?}")),
    }
}

/// Puts the faction a preset needs on its seat, before the table is built. Only `capture` does:
/// the Vuil'raith Cabal, which is not one of `IN_SCOPE_FACTIONS`, sits on a seeded seat.
pub fn seat_roster(
    name: &str,
    seed: u64,
    players: &[PlayerId],
    assignments: &mut BTreeMap<PlayerId, FactionId>,
) {
    let already = assignments
        .values()
        .any(|faction| faction.as_str() == "cabal");
    if parse(name).is_some_and(|parts| parts.base == CAPTURE) && !players.is_empty() && !already {
        let seat = (mix(seed, 950) % players.len() as u64) as usize;
        assignments.insert(players[seat].clone(), FactionId::new("cabal"));
    }
}

/// The factions actually seated, read from the state (whatever rotation created the game).
fn seated(state: &GameState, players: &[PlayerId]) -> BTreeMap<PlayerId, FactionId> {
    players
        .iter()
        .filter_map(|player| {
            state
                .player(player)
                .map(|seat| (player.clone(), seat.faction.clone()))
        })
        .collect()
}

/// splitmix64 over `seed` and a salt: a stable choice that depends on nothing but its inputs.
fn mix(seed: u64, salt: u64) -> u64 {
    let mut z = seed
        .wrapping_add(salt.wrapping_mul(0x9E37_79B9_7F4A_7C15))
        .wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Lifting the custodians is not needed: 27.4 makes every round's agenda phase happen once they
/// are gone, so the flag is set and the deck's top is replaced by [`AGENDA_ORDER`].
fn agenda(content: &ContentStore, state: &mut GameState, players: &[PlayerId], seed: u64) {
    state.custodians_removed = true;
    let top: Vec<String> = AGENDA_ORDER
        .iter()
        .filter(|alias| state.agenda_deck.iter().any(|a| a == *alias))
        .map(|alias| (*alias).to_owned())
        .collect();
    state.agenda_deck.retain(|alias| !top.contains(alias));
    state.agenda_deck.splice(0..0, top);
    deal_cards(content, state, players, seed, AGENDA_CARD_POOL, 4);
    for player in players {
        if let Some(seat) = state.player_mut(player) {
            seat.trade_goods += AGENDA_TRADE_GOODS;
        }
    }
}

/// The `+agenda` suffix: the custodians token is already off Mecatol Rex (27.4, so an agenda phase
/// follows every status phase from round one) and every seat has trade goods to spend on votes
/// and notes. Unlike the `agenda` preset it leaves the deck order and the hands alone, so it
/// combines with any other preset without disturbing what that one dealt.
fn agenda_suffix(state: &mut GameState, players: &[PlayerId]) {
    state.custodians_removed = true;
    for player in players {
        if let Some(seat) = state.player_mut(player) {
            seat.trade_goods += AGENDA_TRADE_GOODS;
        }
    }
}

/// Cards per seat in the `siege` and `bombard` presets (the pools are small, so fewer in practice).
const SIEGE_HAND: usize = 4;

/// What a colony preset puts down: the planet's defenders, the ships in its space area, and the
/// invasion force of the previous seat.
struct ColonySpec {
    defenders: &'static [(&'static str, usize)],
    garrison: &'static [(&'static str, usize)],
    attacker: &'static [(&'static str, usize)],
    /// A lone infantry of the seat after the owner shares the planet (three or more seats).
    third_party: bool,
}

impl ColonySpec {
    const INVASION: Self = Self {
        defenders: COLONY_DEFENDERS,
        garrison: &[],
        attacker: INVASION_FLEET,
        third_party: false,
    };
    const WORLD: Self = Self {
        defenders: COLONY_DEFENDERS,
        garrison: &[],
        attacker: WORLD_FLEET,
        third_party: false,
    };
    const SIEGE: Self = Self {
        defenders: COLONY_DEFENDERS,
        garrison: SIEGE_GARRISON,
        attacker: SIEGE_FLEET,
        third_party: false,
    };
    const BOMBARD: Self = Self {
        defenders: BOMBARD_DEFENDERS,
        garrison: &[],
        attacker: BOMBARD_FLEET,
        third_party: true,
    };
}

/// One defended colony per seat, and the previous seat's invasion force waiting above it
/// (`flagship` in a fleet resolves to the faction's own).
fn invasion_preset(
    content: &ContentStore,
    state: &mut GameState,
    galaxy: &Galaxy,
    players: &[PlayerId],
    seed: u64,
    spec: &ColonySpec,
) -> Result<(), String> {
    let assignments = seated(state, players);
    let homes: Vec<SystemId> = players
        .iter()
        .map(|player| {
            let one = BTreeMap::from([(player.clone(), assignments[player].clone())]);
            seating::home_systems(content, &one).map(|mut homes| homes.remove(0))
        })
        .collect::<Result<_, _>>()
        .map_err(|e| e.to_string())?;
    let home_set: BTreeSet<&str> = homes.iter().map(SystemId::as_str).collect();
    let count = players.len();
    let mut used: BTreeSet<String> = BTreeSet::new();

    // Colonies first, so every fleet can be put beside one.
    let mut colonies: Vec<Option<(SystemId, PlanetId)>> = Vec::new();
    for (j, player) in players.iter().enumerate() {
        let colony = pick_site_of(
            content,
            state,
            galaxy,
            homes[j].as_str(),
            &home_set,
            &used,
            mix(seed, 600 + j as u64),
            2,
            Site::Colony,
        )
        .and_then(|site| colony_planet(content, site.as_str()).map(|planet| (site, planet)));
        if let Some((site, planet)) = &colony {
            used.insert(site.as_str().to_owned());
            let mut defenders = fleet_for(content, &assignments[player], spec.defenders);
            // fleet_for adds a dreadnought-less mech; keep it, the planet is a ground area.
            let board = state.system_mut(site);
            board.set_control(planet.clone(), player.clone());
            for (kind, number) in defenders.drain(..) {
                for _ in 0..number {
                    board
                        .planet_units
                        .entry(planet.clone())
                        .or_default()
                        .push(Unit::new(kind.clone(), player.clone()));
                }
            }
            if !spec.garrison.is_empty() {
                let ships = ships_for(content, &assignments[player], spec.garrison);
                place(content, state, player, site, &ships)?;
            }
            if spec.third_party && count > 2 {
                let third = &players[(j + 1) % count];
                state
                    .system_mut(site)
                    .planet_units
                    .entry(planet.clone())
                    .or_default()
                    .push(Unit::new(UnitTypeId::new("infantry"), third.clone()));
                check_pools(state, third)?;
            }
            check_pools(state, player)?;
        }
        colonies.push(colony);
    }

    // The attacker is the previous seat. Its fleet waits in the colony's own space area: ground
    // forces there are landable without moving (see `RAIDING_PARTY`), so the first tactical action
    // that activates the colony meets the PDS's space cannon, the dreadnought's bombardment and a
    // ground combat, instead of a random move that strands the infantry (a carrier that leaves
    // without them drops them over capacity).
    for (i, player) in players.iter().enumerate() {
        let Some((colony, _)) = &colonies[(i + 1) % count] else {
            continue;
        };
        let fleet = fleet_for(content, &assignments[player], spec.attacker);
        place(content, state, player, colony, &fleet)?;
    }
    Ok(())
}

/// The Cabal's table: the combat setup (so the Cabal raids too, and Devour captures what it
/// destroys), every other seat's fleet beside the Cabal's home within Vortex's reach, the Cabal's
/// Vortex ready, its agent ready and hero and commander unlocked, and one captured model of
/// several types from every other seat on its sheet (Amalgamation and Riftmeld offer to return
/// them).
fn capture(
    content: &ContentStore,
    state: &mut GameState,
    galaxy: &Galaxy,
    players: &[PlayerId],
    seed: u64,
) -> Result<(), String> {
    let assignments = seated(state, players);
    let cabal = players
        .iter()
        .find(|player| assignments[*player].as_str() == "cabal")
        .cloned()
        .ok_or_else(|| "the capture preset needs the Cabal seated".to_owned())?;
    combat(content, state, galaxy, players, seed)?;

    let homes: Vec<SystemId> = players
        .iter()
        .map(|player| {
            let one = BTreeMap::from([(player.clone(), assignments[player].clone())]);
            seating::home_systems(content, &one).map(|mut homes| homes.remove(0))
        })
        .collect::<Result<_, _>>()
        .map_err(|e| e.to_string())?;
    let home_set: BTreeSet<&str> = homes.iter().map(SystemId::as_str).collect();
    let cabal_home = players
        .iter()
        .position(|player| *player == cabal)
        .map(|index| homes[index].clone())
        .ok_or_else(|| "the Cabal has no seat".to_owned())?;
    let mut used: BTreeSet<String> = BTreeSet::new();
    for (index, player) in players.iter().enumerate() {
        if *player == cabal {
            continue;
        }
        if let Some(site) = pick_site(
            content,
            state,
            galaxy,
            cabal_home.as_str(),
            &home_set,
            &used,
            mix(seed, 700 + index as u64),
            2,
        ) {
            let fleet = fleet_for(content, &assignments[player], CAPTURE_RAIDERS);
            place(content, state, player, &site, &fleet)?;
            used.insert(site.as_str().to_owned());
        }
    }

    grant_techs(
        content,
        state,
        std::slice::from_ref(&cabal),
        CAPTURE_CABAL_TECHS,
    );
    let others: Vec<PlayerId> = players
        .iter()
        .filter(|player| **player != cabal)
        .cloned()
        .collect();
    if let Some(seat) = state.player_mut(&cabal) {
        for (index, owner) in others.iter().enumerate() {
            // Three or four models each, rotating which types so the sheet is varied.
            for offset in 0..3 {
                let kind = CAPTURED_TYPES[(index + offset) % CAPTURED_TYPES.len()];
                seat.captured_units
                    .push((owner.clone(), UnitTypeId::new(kind)));
            }
        }
        for (leader, status) in [
            ("cabalagent", LeaderStatus::Readied),
            ("cabalcommander", LeaderStatus::Unlocked),
            ("cabalhero", LeaderStatus::Unlocked),
        ] {
            seat.leaders
                .insert(ti4_model::id::LeaderId::new(leader), status);
        }
    }
    Ok(())
}

/// Every seat owns every technology of `pool` the corpus has.
fn grant_techs(content: &ContentStore, state: &mut GameState, players: &[PlayerId], pool: &[&str]) {
    for player in players {
        if let Some(seat) = state.player_mut(player) {
            for id in pool {
                if content.get(ContentType::Technologies, id).is_some() {
                    seat.technologies.insert(TechnologyId::new(*id));
                }
            }
        }
    }
}

/// Two seeded seats on 9 victory points, the others on 8 (so a tie for the lead is possible),
/// round 5, and the public objective deck emptied so the next reveal ends the game.
fn endgame(state: &mut GameState, players: &[PlayerId], seed: u64) {
    let mut ranking: Vec<usize> = (0..players.len()).collect();
    ranking.sort_by_key(|i| mix(seed, 800 + *i as u64));
    for (rank, index) in ranking.into_iter().enumerate() {
        if let Some(seat) = state.player_mut(&players[index]) {
            seat.victory_points = if rank < 2 { 9 } else { 8 };
        }
    }
    state.objective_deck.clear();
    state.round = 5;
}

/// Every leader of every seat is usable at once: heroes and commanders unlocked, agents readied.
/// Legendary planets on the map that nobody controls go to the seats in turn, so their abilities
/// have an owner.
fn unlock_leaders(
    content: &ContentStore,
    state: &mut GameState,
    galaxy: &Galaxy,
    players: &[PlayerId],
) {
    for player in players {
        let Some(seat) = state.player_mut(player) else {
            continue;
        };
        let leaders: Vec<_> = seat.leaders.keys().cloned().collect();
        for leader in leaders {
            let status = if leaders::kind_of(content, &leader).as_deref() == Some(leaders::AGENT) {
                LeaderStatus::Readied
            } else {
                LeaderStatus::Unlocked
            };
            seat.leaders.insert(leader, status);
        }
    }
    let on_map: BTreeSet<&str> = galaxy.system_ids().into_iter().collect();
    let mut legendary: Vec<(SystemId, PlanetId)> = galaxy::all_planets(content, POK)
        .values()
        .filter(|planet| planet.is_legendary())
        .filter_map(|planet| {
            let system = planet.system_id()?;
            on_map
                .contains(system)
                .then(|| (SystemId::new(system), PlanetId::new(planet.id())))
        })
        .collect();
    legendary.sort();
    for (index, (system, planet)) in legendary.into_iter().enumerate() {
        let taken = state
            .board
            .get(&system)
            .is_some_and(|board| board.planet_control.contains_key(&planet));
        if !taken {
            let owner = players[index % players.len()].clone();
            state.system_mut(&system).set_control(planet, owner);
        }
    }
}

/// Hands the relics of [`RELIC_POOL`] out round-robin from a seeded first seat and takes them out
/// of the relic deck. The seat after the first also gets three cultural fragments (crossing).
fn deal_relics(content: &ContentStore, state: &mut GameState, players: &[PlayerId], seed: u64) {
    let count = players.len();
    let first = (mix(seed, 500) % count as u64) as usize;
    let pool: Vec<&str> = RELIC_POOL
        .iter()
        .copied()
        .filter(|id| content.get(ContentType::Relics, id).is_some())
        .take(count * RELICS_PER_SEAT)
        .collect();
    for (index, id) in pool.iter().enumerate() {
        state.relic_deck.retain(|relic| relic.as_str() != *id);
        if let Some(seat) = state.player_mut(&players[(first + index) % count]) {
            seat.relics.push(RelicId::new(*id));
        }
    }
    if let Some(seat) = state.player_mut(&players[(first + 1) % count]) {
        seat.relic_fragments.insert("CULTURAL".to_owned(), 3);
    }
}

/// Deals each seat a different seeded slice of `pool_ids` (at most `hand` cards each) and takes
/// those cards out of the deck.
fn deal_cards(
    content: &ContentStore,
    state: &mut GameState,
    players: &[PlayerId],
    seed: u64,
    pool_ids: &[&str],
    hand: usize,
) {
    let mut pool: Vec<&str> = pool_ids
        .iter()
        .copied()
        .filter(|id| content.get(ContentType::ActionCards, id).is_some())
        .collect();
    pool.sort_by_key(|id| {
        mix(
            seed,
            id.bytes()
                .fold(7_u64, |h, b| h.wrapping_mul(31).wrapping_add(u64::from(b))),
        )
    });
    let per_seat = hand.min(pool.len() / players.len().max(1));
    for (player, hand) in players.iter().zip(pool.chunks(per_seat.max(1))) {
        for id in hand {
            state.action_card_deck.retain(|card| card.as_str() != *id);
            if let Some(seat) = state.player_mut(player) {
                seat.action_cards.push(ActionCardId::new(*id));
            }
        }
    }
}

fn combat(
    content: &ContentStore,
    state: &mut GameState,
    galaxy: &Galaxy,
    players: &[PlayerId],
    seed: u64,
) -> Result<(), String> {
    let assignments = seated(state, players);
    let homes: Vec<SystemId> = players
        .iter()
        .map(|player| {
            let one = BTreeMap::from([(player.clone(), assignments[player].clone())]);
            seating::home_systems(content, &one).map(|mut homes| homes.remove(0))
        })
        .collect::<Result<_, _>>()
        .map_err(|e| e.to_string())?;
    let home_set: BTreeSet<&str> = homes.iter().map(SystemId::as_str).collect();
    let count = players.len();

    // Half the seats (rounded up) raid Mecatol, chosen by a seeded ranking; the first of them
    // starts inside Mecatol, the rest beside it.
    let mut ranking: Vec<usize> = (0..count).collect();
    ranking.sort_by_key(|i| mix(seed, 100 + *i as u64));
    let holder = ranking[0];
    let mut raiders = vec![false; count];
    for i in ranking.into_iter().take(count.div_ceil(2)) {
        raiders[i] = true;
    }

    let mut used: BTreeSet<String> = BTreeSet::new();
    for (i, player) in players.iter().enumerate() {
        if count > 1 {
            let other = (i + 1 + (mix(seed, 200 + i as u64) as usize) % (count - 1)) % count;
            if let Some(site) = pick_site(
                content,
                state,
                galaxy,
                homes[other].as_str(),
                &home_set,
                &used,
                mix(seed, 300 + i as u64),
                2,
            ) {
                let fleet = fleet_for(content, &assignments[player], STRIKE_FLEET);
                place(content, state, player, &site, &fleet)?;
                used.insert(site.as_str().to_owned());
            }
        }
        if raiders[i] {
            let mecatol = SystemId::new(MECATOL);
            let site = if i == holder && eligible_in_mecatol(state) {
                Some(mecatol)
            } else {
                pick_site(
                    content,
                    state,
                    galaxy,
                    MECATOL,
                    &home_set,
                    &used,
                    mix(seed, 400 + i as u64),
                    1,
                )
            };
            if let Some(site) = site {
                let fleet = fleet_for(content, &assignments[player], RAIDING_PARTY);
                place(content, state, player, &site, &fleet)?;
                used.insert(site.as_str().to_owned());
            }
            top_up_influence(content, state, player);
        }
    }
    Ok(())
}

/// What kind of system a preset wants to put something in.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Site {
    /// Any empty ordinary system.
    Empty,
    /// An empty ordinary system with a planet that is not legendary, to hold a colony.
    Colony,
    /// An empty ordinary system with a planet that has an exploration trait (and so is explored
    /// when somebody takes control of it).
    Traited,
    /// An empty system that still holds a frontier token.
    Frontier,
}

/// Per seat: a fleet with landing forces beside an unclaimed trait planet, a ship pair in a
/// frontier system, Dark Energy Tap, fragments to cross, and the exploration decks stacked.
fn explore_preset(
    content: &ContentStore,
    state: &mut GameState,
    galaxy: &Galaxy,
    players: &[PlayerId],
    seed: u64,
) -> Result<(), String> {
    let assignments = seated(state, players);
    let homes: Vec<SystemId> = players
        .iter()
        .map(|player| {
            let one = BTreeMap::from([(player.clone(), assignments[player].clone())]);
            seating::home_systems(content, &one).map(|mut homes| homes.remove(0))
        })
        .collect::<Result<_, _>>()
        .map_err(|e| e.to_string())?;
    let home_set: BTreeSet<&str> = homes.iter().map(SystemId::as_str).collect();
    let mut used: BTreeSet<String> = BTreeSet::new();
    for (i, player) in players.iter().enumerate() {
        let landing = pick_site_of(
            content,
            state,
            galaxy,
            homes[i].as_str(),
            &home_set,
            &used,
            mix(seed, 1000 + i as u64),
            2,
            Site::Traited,
        );
        if let Some(site) = landing {
            let fleet = fleet_for(content, &assignments[player], EXPLORE_FLEET);
            place(content, state, player, &site, &fleet)?;
            used.insert(site.as_str().to_owned());
        }
        let second = pick_site_of(
            content,
            state,
            galaxy,
            homes[i].as_str(),
            &home_set,
            &used,
            mix(seed, 1300 + i as u64),
            3,
            Site::Traited,
        );
        if let Some(site) = second {
            let fleet = fleet_for(content, &assignments[player], EXPLORE_FLEET_LIGHT);
            place(content, state, player, &site, &fleet)?;
            used.insert(site.as_str().to_owned());
        }
        let frontier = pick_site_of(
            content,
            state,
            galaxy,
            homes[i].as_str(),
            &home_set,
            &used,
            mix(seed, 1100 + i as u64),
            8,
            Site::Frontier,
        );
        if let Some(site) = frontier {
            let fleet: Vec<(UnitTypeId, usize)> = FRONTIER_FLEET
                .iter()
                .map(|(kind, count)| (UnitTypeId::new(*kind), *count))
                .collect();
            place(content, state, player, &site, &fleet)?;
            used.insert(site.as_str().to_owned());
        }
        if let Some(seat) = state.player_mut(player) {
            seat.technologies.insert(TechnologyId::new("det"));
            // Different seats start with different fragments, so crossing is offered to some.
            let (trait_name, count) = EXPLORE_FRAGMENTS[i % EXPLORE_FRAGMENTS.len()];
            *seat
                .relic_fragments
                .entry(trait_name.to_owned())
                .or_insert(0) += count;
        }
    }
    for (deck, tops) in EXPLORE_TOPS {
        let Some(cards) = state.exploration_decks.get_mut(*deck) else {
            continue;
        };
        let top: Vec<String> = tops
            .iter()
            .filter(|card| cards.iter().any(|c| c == *card))
            .map(|card| (*card).to_owned())
            .collect();
        cards.retain(|card| !top.contains(card));
        cards.splice(0..0, top);
    }
    Ok(())
}

/// Deals every seat its own notes (again, see below) and hands each note to one of the `others`
/// seats before its owner, Support for the Throne to one seat, so each note kind sits in a foreign
/// hand from the start.
fn deal_notes(content: &ContentStore, state: &mut GameState, players: &[PlayerId], others: usize) {
    // Game creation already deals after seating, so this only restates the hands before the
    // spread below (a no-op on a fresh game; kept so a preset applied to any opening state works).
    ti4_engine::promissory::deal(state, content, POK);
    let count = players.len();
    let spread = others.min(count.saturating_sub(1)).max(1);
    let owned: Vec<Vec<String>> = players
        .iter()
        .map(|owner| {
            state
                .promissory_notes
                .iter()
                .filter(|(_, holder)| *holder == owner)
                .map(|(note, _)| note.clone())
                .collect()
        })
        .collect();
    // Each note goes to exactly one other seat: the notes of seat j alternate between the seats
    // just before it, so every seat ends up holding some of two neighbours' notes.
    for (j, notes) in owned.iter().enumerate() {
        for (index, note) in notes.iter().enumerate() {
            let holder = &players[(j + count - 1 - index % spread) % count];
            if holder != &players[j] && note_can_move(state, holder, note) {
                ti4_engine::promissory::take(state, content, holder, note);
            }
        }
    }
    if count > 1 {
        let name = ti4_engine::promissory::faction_name(state, &players[1]);
        ti4_engine::promissory::receive(
            state,
            &players[0],
            &ti4_engine::promissory::support(&name),
        );
    }
}

/// Whether the preset may hand `note` to `holder` (Mahact refuses Alliance, Hubris).
fn note_can_move(state: &GameState, holder: &PlayerId, note: &str) -> bool {
    ti4_engine::promissory::may_receive(state, holder, note)
}

/// The faction kit of `world`: every faction technology, every leader usable, and the legendary
/// planets handed out (see [`unlock_leaders`]).
fn world_kit(content: &ContentStore, state: &mut GameState, galaxy: &Galaxy, players: &[PlayerId]) {
    for player in players {
        let faction = state
            .player(player)
            .map(|seat| seat.faction.as_str().to_owned())
            .unwrap_or_default();
        // The faction sheet's technologies (plus Thunder's Edge reprints), not every record that
        // carries the faction tag: Nekro's `nekroc4r`/`nekroc4y` are not part of the game.
        let owned: Vec<TechnologyId> = ti4_engine::factions::assets(content, POK, &faction)
            .into_iter()
            .filter(|asset| asset.kind == ti4_engine::factions::AssetKind::Technology)
            .map(|asset| TechnologyId::new(asset.id))
            .collect();
        if let Some(seat) = state.player_mut(player) {
            seat.technologies.extend(owned);
        }
    }
    unlock_leaders(content, state, galaxy, players);
}

/// The `+short` suffix: a few victory points each and only three public objectives left, so the
/// game ends within a few rounds (the thin presets then finish inside a night's budget).
fn short_game(state: &mut GameState, players: &[PlayerId], seed: u64) {
    for (i, player) in players.iter().enumerate() {
        if let Some(seat) = state.player_mut(player) {
            seat.victory_points = 3 + (mix(seed, 1200 + i as u64) % 3) as i32;
        }
    }
    state.objective_deck.truncate(3);
}

/// The nearest eligible system to `from`: one jump away if any qualifies, else up to `max_jumps`.
#[allow(clippy::too_many_arguments)]
fn pick_site(
    content: &ContentStore,
    state: &GameState,
    galaxy: &Galaxy,
    from: &str,
    home_set: &BTreeSet<&str>,
    used: &BTreeSet<String>,
    choice: u64,
    max_jumps: usize,
) -> Option<SystemId> {
    pick_site_of(
        content,
        state,
        galaxy,
        from,
        home_set,
        used,
        choice,
        max_jumps,
        Site::Empty,
    )
}

#[allow(clippy::too_many_arguments)]
fn pick_site_of(
    content: &ContentStore,
    state: &GameState,
    galaxy: &Galaxy,
    from: &str,
    home_set: &BTreeSet<&str>,
    used: &BTreeSet<String>,
    choice: u64,
    max_jumps: usize,
    kind: Site,
) -> Option<SystemId> {
    let mut seen: BTreeSet<&str> = BTreeSet::from([from]);
    let mut layer: BTreeSet<&str> = BTreeSet::from([from]);
    for _ in 0..max_jumps {
        let mut next: BTreeSet<&str> = BTreeSet::new();
        for system in &layer {
            for neighbour in galaxy.adjacent(system) {
                if seen.insert(neighbour) {
                    next.insert(neighbour);
                }
            }
        }
        let eligible: Vec<&str> = next
            .iter()
            .copied()
            .filter(|id| eligible(content, state, id, home_set, used, kind == Site::Frontier))
            .filter(|id| match kind {
                Site::Empty => true,
                Site::Colony => colony_planet(content, id).is_some(),
                Site::Traited => traited_planet(content, id).is_some(),
                Site::Frontier => ti4_engine::exploration::frontier_systems(content, POK, galaxy)
                    .contains(&SystemId::new(*id)),
            })
            .collect();
        if !eligible.is_empty() {
            let index = usize::try_from(choice % eligible.len() as u64).unwrap_or(0);
            return Some(SystemId::new(eligible[index]));
        }
        layer = next;
    }
    None
}

/// The planet a colony would sit on: the first non-legendary planet printed on the tile.
fn colony_planet(content: &ContentStore, system: &str) -> Option<PlanetId> {
    let mut planets: Vec<_> = galaxy::planets_in(content, system, POK)
        .into_iter()
        .filter(|planet| !planet.is_legendary() && !planet.is_space_station())
        .map(|planet| planet.id().to_owned())
        .collect();
    planets.sort();
    planets.into_iter().next().map(PlanetId::new)
}

/// The first non-legendary planet on the tile that has an exploration trait.
fn traited_planet(content: &ContentStore, system: &str) -> Option<PlanetId> {
    let mut planets: Vec<_> = galaxy::planets_in(content, system, POK)
        .into_iter()
        .filter(|planet| !planet.is_legendary() && !planet.is_space_station())
        .map(|planet| PlanetId::new(planet.id()))
        .filter(|planet| !ti4_engine::exploration::traits_of(content, POK, planet).is_empty())
        .collect();
    planets.sort();
    planets.into_iter().next()
}

/// Mecatol Rex's space area is free for the first raider.
fn eligible_in_mecatol(state: &GameState) -> bool {
    state
        .board
        .get(&SystemId::new(MECATOL))
        .is_none_or(|board| {
            board.units.is_empty() && board.planet_units.values().all(Vec::is_empty)
        })
}

/// An empty ordinary system: not Mecatol, not a home, not an anomaly or hyperlane, not taken.
fn eligible(
    content: &ContentStore,
    state: &GameState,
    id: &str,
    home_set: &BTreeSet<&str>,
    used: &BTreeSet<String>,
    frontier_site: bool,
) -> bool {
    if id == MECATOL || home_set.contains(id) || used.contains(id) {
        return false;
    }
    let Some(system) = galaxy::system(content, id, POK) else {
        return false;
    };
    // A frontier site may be an asteroid field or a nebula (planetless anomalies are most of the
    // frontier); a supernova, a gravity rift or a scar is never somewhere to start.
    let anomaly_ok =
        frontier_site && !(system.is_supernova() || system.is_gravity_rift() || system.is_scar());
    // A hyperlane tile is not a system (nothing may be placed in it, even though the 3-, 4- and
    // 5-player maps put frontier tokens on some); `frontier_systems` already leaves them out.
    if (system.is_anomaly() && !anomaly_ok) || system.is_hyperlane() {
        return false;
    }
    state.board.get(&SystemId::new(id)).is_none_or(|board| {
        board.units.is_empty() && board.planet_units.values().all(Vec::is_empty)
    })
}

/// The unit types of a preset fleet for one faction: the dreadnought is the faction's own
/// version (L1Z1X's super-dreadnought, for one), and a faction mech rides with the ground forces
/// where the corpus has one. Both sustain damage.
fn fleet_for(
    content: &ContentStore,
    faction: &FactionId,
    spec: &[(&str, usize)],
) -> Vec<(UnitTypeId, usize)> {
    let mut fleet = ships_for(content, faction, spec);
    let mech = factions::resolve_unit(content, faction.as_str(), &UnitTypeId::new("mech"), POK);
    if units::unit_type(content, mech.as_str(), POK).is_some() {
        fleet.push((mech, 1));
    }
    fleet
}

/// As [`fleet_for`] without the mech: only the faction's own dreadnought is resolved.
fn ships_for(
    content: &ContentStore,
    faction: &FactionId,
    spec: &[(&str, usize)],
) -> Vec<(UnitTypeId, usize)> {
    let resolve =
        |kind: &str| factions::resolve_unit(content, faction.as_str(), &UnitTypeId::new(kind), POK);
    spec.iter()
        .map(|(kind, count)| {
            let id = if matches!(*kind, "dreadnought" | "flagship") {
                resolve(kind)
            } else {
                UnitTypeId::new(*kind)
            };
            (id, *count)
        })
        .collect()
}

/// Whether a unit type id is `kind` or a faction's own version of it (`sol_mech` for `mech`).
fn is_kind(id: &str, kind: &str) -> bool {
    id == kind || id.ends_with(&format!("_{kind}"))
}

fn place(
    content: &ContentStore,
    state: &mut GameState,
    player: &PlayerId,
    site: &SystemId,
    fleet_spec: &[(UnitTypeId, usize)],
) -> Result<(), String> {
    let board = state.system_mut(site);
    for (kind, count) in fleet_spec {
        for _ in 0..*count {
            board.units.push(Unit::new(kind.clone(), player.clone()));
        }
    }
    let over_supply = fleet::over_supply(state, content, POK, player, site);
    let over_capacity = fleet::over_capacity(state, content, POK, player, site);
    if over_supply > 0 || over_capacity > 0 {
        return Err(format!(
            "preset fleet for {player} in {site} is over supply ({over_supply}) or capacity ({over_capacity})"
        ));
    }
    check_pools(state, player)?;
    Ok(())
}

/// No preset puts more of a unit type on the board than its reinforcement pool holds.
fn check_pools(state: &GameState, player: &PlayerId) -> Result<(), String> {
    for (kind, limit) in POOL {
        let on_board = state
            .board
            .values()
            .flat_map(|system| {
                system
                    .units
                    .iter()
                    .chain(system.planet_units.values().flatten())
            })
            .filter(|unit| &unit.owner == player && is_kind(unit.type_id.as_str(), kind))
            .count();
        if on_board > *limit {
            return Err(format!(
                "preset puts {on_board} {kind} on the board for {player}, over the pool of {limit}"
            ));
        }
    }
    Ok(())
}

/// Influence a raider keeps beyond the custodians' six. Random play spends it early: two
/// Leadership token buys, or four Letnev Munitions Reserves rerolls (2 trade goods each) in the
/// strike fleets' first fights, drained raiders before they reached Mecatol. 27.2 checks the cost
/// when the invasion starts, not when the game does.
const RAIDER_SPARE_INFLUENCE: i64 = 10;

/// Trade goods so a raider can pay the custodians' six influence (27.2) without any rule change.
fn top_up_influence(content: &ContentStore, state: &mut GameState, player: &PlayerId) {
    let available =
        production::available(state, content, POK, player, production::Spend::Influence);
    let wanted = invasion::CUSTODIANS_COST + RAIDER_SPARE_INFLUENCE;
    if available < wanted {
        if let Some(seat) = state.player_mut(player) {
            seat.trade_goods += i32::try_from(wanted - available).unwrap_or(0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::create_game_with_template;
    use crate::setup::preset_game;
    use ti4_model::state::LeaderStatus;

    fn players(n: usize) -> Vec<PlayerId> {
        (1..=n).map(|i| PlayerId::new(format!("p{i}"))).collect()
    }

    fn content() -> &'static ContentStore {
        ContentStore::embedded()
    }

    fn homes_of(state_players: &[PlayerId]) -> Vec<SystemId> {
        let assignments = seating::seat_in_scope(state_players);
        state_players
            .iter()
            .map(|p| {
                let one = BTreeMap::from([(p.clone(), assignments[p].clone())]);
                seating::home_systems(content(), &one).unwrap().remove(0)
            })
            .collect()
    }

    #[test]
    fn an_unknown_preset_is_an_error() {
        let err = preset_game(content(), &players(3), 5, None, "nope").unwrap_err();
        assert!(err.contains("unknown start_preset"), "{err}");
        assert!(!is_known("nope") && is_known(COMBAT));
    }

    #[test]
    fn the_preset_is_deterministic_per_seed_and_varies_between_seeds() {
        let build = |seed| {
            let (state, _) = preset_game(content(), &players(4), seed, None, COMBAT).unwrap();
            serde_json::to_string(&state).unwrap()
        };
        assert_eq!(build(7), build(7));
        assert_ne!(build(7), build(8));
    }

    /// The base type of a unit id: a preset seat holds its upgraded or faction form.
    fn base_of(id: &str) -> &'static str {
        units::unit_type(content(), id, POK).map_or("", |kind| kind.base_type())
    }

    #[test]
    fn slow_no_preset_leaves_a_seat_with_a_unit_in_a_form_it_does_not_own() {
        let presets = [
            COMBAT, INVASION, TECHS, SIEGE, BOMBARD, CAPTURE, WORLD, EXPLORE, NOTES, LEADERS,
            CARDS, RELICS, AGENDA, ENDGAME,
        ];
        for preset in presets {
            for n in 3..=6 {
                for seed in 0..4 {
                    let Ok((state, _)) = preset_game(content(), &players(n), seed, None, preset)
                    else {
                        continue;
                    };
                    assert_eq!(
                        ti4_engine::technology::unit_form_violations(&state, content(), POK),
                        Vec::<String>::new(),
                        "{preset} {n}p seed {seed}"
                    );
                }
            }
        }
    }

    #[test]
    fn every_player_count_gets_legal_fleets_within_supply_capacity_and_pools() {
        for n in 3..=6 {
            for seed in 0..8 {
                let (state, _) = preset_game(content(), &players(n), seed, None, COMBAT)
                    .unwrap_or_else(|e| panic!("{n} players seed {seed}: {e}"));
                for player in &players(n) {
                    for (system, board) in &state.board {
                        assert_eq!(
                            fleet::over_supply(&state, content(), POK, player, system),
                            0,
                            "{n}p seed {seed}: {player} over fleet supply in {system}"
                        );
                        assert_eq!(
                            fleet::over_capacity(&state, content(), POK, player, system),
                            0,
                            "{n}p seed {seed}: {player} over capacity in {system}"
                        );
                        // No two players share a space area.
                        let owners: BTreeSet<_> = board.units.iter().map(|u| &u.owner).collect();
                        assert!(owners.len() <= 1, "{n}p seed {seed}: {system} is shared");
                    }
                }
            }
        }
    }

    #[test]
    fn every_preset_fleet_can_sustain_damage_with_a_ship_and_a_mech() {
        for n in 3..=6 {
            for seed in 0..8 {
                let list = players(n);
                let (plain, _) = create_game_with_template(content(), &list, seed, None).unwrap();
                let (state, _) = preset_game(content(), &list, seed, None, COMBAT).unwrap();
                let mut fleets = 0;
                for (system, board) in &state.board {
                    let before = plain.board.get(system).map_or(0, |b| b.units.len());
                    if board.units.len() <= before {
                        continue;
                    }
                    fleets += 1;
                    let kinds: Vec<_> = board
                        .units
                        .iter()
                        .map(|u| units::unit_type(content(), u.type_id.as_str(), POK).unwrap())
                        .collect();
                    assert!(
                        kinds.iter().any(|k| k.is_ship() && k.sustain_damage()),
                        "{n}p seed {seed}: the fleet in {system} has no sustaining ship"
                    );
                    assert!(
                        kinds
                            .iter()
                            .any(|k| k.is_ground_force() && k.sustain_damage()),
                        "{n}p seed {seed}: the fleet in {system} has no sustaining mech"
                    );
                }
                assert!(fleets >= n, "{n}p seed {seed}: only {fleets} fleets placed");
            }
        }
    }

    #[test]
    fn a_preset_dreadnought_is_the_factions_own_version() {
        let fleet = fleet_for(content(), &FactionId::new("l1z1x"), STRIKE_FLEET);
        let dreadnought = fleet
            .iter()
            .find(|(id, _)| id.as_str().contains("dread"))
            .unwrap_or_else(|| panic!("no dreadnought in {fleet:?}"));
        assert_ne!(
            dreadnought.0.as_str(),
            "dreadnought",
            "L1Z1X has a super-dreadnought"
        );
        assert!(fleet.iter().any(|(id, _)| id.as_str().ends_with("mech")));
    }

    #[test]
    fn a_strike_fleet_sits_beside_an_opponents_home_system() {
        for n in 3..=6 {
            for seed in 0..8 {
                let list = players(n);
                let (plain, _) = create_game_with_template(content(), &list, seed, None).unwrap();
                let (state, galaxy) = preset_game(content(), &list, seed, None, COMBAT).unwrap();
                let homes = homes_of(&list);
                let mut beside_a_home = 0;
                for (i, player) in list.iter().enumerate() {
                    let strike: Vec<&SystemId> = state
                        .board
                        .iter()
                        .filter(|(system, board)| {
                            let before = plain.board.get(*system).map_or(0, |b| b.units.len());
                            board.units.len() > before
                                && board.units.iter().any(|u| {
                                    &u.owner == player && u.type_id.as_str() == "destroyer"
                                })
                        })
                        .map(|(system, _)| system)
                        .collect();
                    assert!(
                        !strike.is_empty(),
                        "{n}p seed {seed}: {player} has no strike fleet"
                    );
                    for site in strike {
                        let within_two = homes.iter().enumerate().any(|(j, home)| {
                            j != i
                                && (galaxy.are_adjacent(site.as_str(), home.as_str())
                                    || galaxy
                                        .adjacent(home.as_str())
                                        .iter()
                                        .any(|mid| galaxy.are_adjacent(site.as_str(), mid)))
                        });
                        assert!(
                            within_two,
                            "{n}p seed {seed}: {site} is not near an opponent home"
                        );
                        if homes.iter().enumerate().any(|(j, home)| {
                            j != i && galaxy.are_adjacent(site.as_str(), home.as_str())
                        }) {
                            beside_a_home += 1;
                        }
                    }
                }
                assert!(
                    beside_a_home > 0,
                    "{n}p seed {seed}: no fleet is adjacent to a home"
                );
            }
        }
    }

    #[test]
    fn raiders_start_in_or_beside_mecatol_and_one_can_lift_the_custodians_at_once() {
        let mecatol = SystemId::new(MECATOL);
        for n in 3..=6 {
            for seed in 0..8 {
                let list = players(n);
                let (state, galaxy) = preset_game(content(), &list, seed, None, COMBAT).unwrap();
                let raiders: Vec<&PlayerId> = list
                    .iter()
                    .filter(|player| {
                        state.board.iter().any(|(system, board)| {
                            (system == &mecatol || galaxy.are_adjacent(system.as_str(), MECATOL))
                                && board
                                    .units
                                    .iter()
                                    .any(|u| &u.owner == *player && u.type_id.as_str() == "cruiser")
                        })
                    })
                    .collect();
                assert_eq!(
                    raiders.len(),
                    n.div_ceil(2),
                    "{n}p seed {seed}: half the seats should raid Mecatol"
                );
                let in_mecatol: Vec<&PlayerId> = raiders
                    .iter()
                    .copied()
                    .filter(|player| {
                        state
                            .board
                            .get(&mecatol)
                            .is_some_and(|b| b.units.iter().any(|u| &u.owner == *player))
                    })
                    .collect();
                assert_eq!(
                    in_mecatol.len(),
                    1,
                    "{n}p seed {seed}: one raider starts in Mecatol"
                );
                for player in &raiders {
                    assert!(
                        production::available(
                            &state,
                            content(),
                            POK,
                            player,
                            production::Spend::Influence
                        ) >= invasion::CUSTODIANS_COST + RAIDER_SPARE_INFLUENCE,
                        "{n}p seed {seed}: {player} has no influence to spare beyond the custodians"
                    );
                }
                // The engine's own gate: the raider inside Mecatol has landable ground forces and
                // the influence, so the custodians can be lifted the first time it invades.
                assert!(
                    invasion::custodians_removable(&state, content(), POK, in_mecatol[0], &mecatol),
                    "{n}p seed {seed}: the raider in Mecatol cannot lift the custodians"
                );
                // Nobody else is in Mecatol.
                assert!(
                    state
                        .board
                        .get(&mecatol)
                        .is_none_or(|b| b.units.iter().all(|u| &u.owner == in_mecatol[0])),
                    "{n}p seed {seed}: Mecatol is shared"
                );
            }
        }
    }

    #[test]
    fn the_cards_preset_deals_distinct_never_played_cards_and_leaves_them_out_of_the_deck() {
        for n in 3..=6 {
            let list = players(n);
            let (plain, _) = create_game_with_template(content(), &list, 3, None).unwrap();
            let (state, _) = preset_game(content(), &list, 3, None, CARDS).unwrap();
            let mut dealt: Vec<&ActionCardId> = Vec::new();
            for player in &list {
                let hand = &state.player(player).unwrap().action_cards;
                let before = plain.player(player).unwrap().action_cards.len();
                assert!(hand.len() > before, "{n}p: {player} got no cards");
                assert!(
                    hand.len() <= before + HAND_SIZE,
                    "{n}p: hand over {HAND_SIZE} dealt"
                );
                assert!(hand.len() < 7, "{n}p: the hand limit would trigger at once");
                dealt.extend(hand.iter().skip(before));
            }
            let unique: BTreeSet<_> = dealt.iter().collect();
            assert_eq!(unique.len(), dealt.len(), "{n}p: a card was dealt twice");
            for card in &dealt {
                assert!(CARD_POOL.contains(&card.as_str()));
                assert!(
                    !state.action_card_deck.contains(card),
                    "{card} is still in the deck"
                );
            }
        }
    }

    #[test]
    fn the_agenda_preset_puts_the_ordered_agendas_on_top_and_removes_the_custodians() {
        for n in 3..=6 {
            let list = players(n);
            let (plain, _) = create_game_with_template(content(), &list, 5, None).unwrap();
            assert!(!plain.custodians_removed);
            let (state, _) = preset_game(content(), &list, 5, None, AGENDA).unwrap();
            assert!(state.custodians_removed);
            assert_eq!(
                state.agenda_deck.len(),
                plain.agenda_deck.len(),
                "{n}p: deck size changed"
            );
            let mut a = state.agenda_deck.clone();
            let mut b = plain.agenda_deck.clone();
            a.sort();
            b.sort();
            assert_eq!(a, b, "{n}p: the deck must hold the same cards");
            assert_eq!(
                &state.agenda_deck[..4],
                ["committee", "defense_act", "abolishment", "redistribution"]
            );
            for player in &list {
                let seat = state.player(player).unwrap();
                assert!(seat.trade_goods >= AGENDA_TRADE_GOODS);
                assert!(seat.action_cards.len() < 7);
            }
        }
    }

    #[test]
    fn every_agenda_preset_alias_exists() {
        let aliases: Vec<String> = create_game_with_template(content(), &players(3), 1, None)
            .unwrap()
            .0
            .agenda_deck;
        for alias in AGENDA_ORDER {
            assert!(
                aliases.iter().any(|a| a == alias),
                "{alias} is not in the agenda deck"
            );
        }
        for id in AGENDA_CARD_POOL {
            assert!(
                content().get(ContentType::ActionCards, id).is_some(),
                "{id}"
            );
        }
    }

    #[test]
    fn the_relics_preset_deals_distinct_relics_with_neuraloop_and_something_to_purge() {
        for n in 3..=6 {
            for seed in 0..4 {
                let list = players(n);
                let (state, _) = preset_game(content(), &list, seed, None, RELICS).unwrap();
                let held: Vec<&RelicId> = list
                    .iter()
                    .flat_map(|p| state.player(p).unwrap().relics.iter())
                    .collect();
                let unique: BTreeSet<_> = held.iter().collect();
                assert_eq!(
                    unique.len(),
                    held.len(),
                    "{n}p seed {seed}: a relic dealt twice"
                );
                assert!(held.iter().all(|r| !state.relic_deck.contains(r)));
                let owner = list
                    .iter()
                    .find(|p| {
                        state
                            .player(p)
                            .unwrap()
                            .relics
                            .iter()
                            .any(|r| r.as_str() == "neuraloop")
                    })
                    .expect("neuraloop is dealt");
                assert!(
                    state.player(owner).unwrap().relics.len() >= 2,
                    "{n}p: nothing to purge"
                );
                assert!(
                    list.iter()
                        .any(|p| state.player(p).unwrap().relic_fragments.get("CULTURAL")
                            == Some(&3))
                );
            }
        }
    }

    #[test]
    fn the_invasion_preset_defends_a_colony_per_seat_with_an_attacker_above_it() {
        for n in 3..=6 {
            for seed in 0..8 {
                let list = players(n);
                let (state, _) = preset_game(content(), &list, seed, None, INVASION)
                    .unwrap_or_else(|e| panic!("{n}p seed {seed}: {e}"));
                let pds_systems: BTreeSet<(&SystemId, &PlayerId)> = state
                    .board
                    .iter()
                    .flat_map(|(system, board)| {
                        board
                            .planet_units
                            .values()
                            .flatten()
                            .filter(|u| u.type_id.as_str() == "pds")
                            .map(move |u| (system, &u.owner))
                    })
                    .collect();
                // Home systems start with PDS or not depending on faction; the colony ones are
                // the ones with a planet whose controller matches and that are not homes.
                let homes = homes_of(&list);
                let colonies: Vec<_> = pds_systems
                    .iter()
                    .filter(|(system, _)| !homes.contains(system))
                    .collect();
                assert_eq!(colonies.len(), n, "{n}p seed {seed}: one colony per seat");
                for (i, player) in list.iter().enumerate() {
                    let (colony, owner) = colonies
                        .iter()
                        .map(|c| **c)
                        .find(|(_, owner)| *owner == &list[(i + 1) % n])
                        .expect("the next seat's colony");
                    assert_eq!(owner, &list[(i + 1) % n]);
                    let board = &state.board[colony];
                    assert!(board.planet_control.values().any(|c| c == owner));
                    assert!(board.planet_units.values().flatten().any(|u| {
                        base_of(u.type_id.as_str()) == "infantry" && &u.owner == owner
                    }));
                    // The attacker waits in the colony's space area with a dreadnought and four
                    // infantry, and nobody else is there.
                    assert!(
                        board.units.iter().all(|u| &u.owner == player),
                        "{n}p seed {seed}: the colony's space is shared"
                    );
                    assert!(
                        board
                            .units
                            .iter()
                            .any(|u| u.type_id.as_str().contains("dread"))
                    );
                    let infantry = board
                        .units
                        .iter()
                        .filter(|u| &u.owner == player && base_of(u.type_id.as_str()) == "infantry")
                        .count();
                    assert_eq!(infantry, 4);
                }
            }
        }
    }

    #[test]
    fn the_techs_preset_is_the_invasion_setup_plus_every_pool_technology() {
        let list = players(4);
        let (plain, _) = create_game_with_template(content(), &list, 2, None).unwrap();
        let (invasion, _) = preset_game(content(), &list, 2, None, INVASION).unwrap();
        let (techs, _) = preset_game(content(), &list, 2, None, TECHS).unwrap();
        for player in &list {
            let owned = &techs.player(player).unwrap().technologies;
            for id in TECH_POOL {
                assert!(
                    owned.contains(&TechnologyId::new(*id)),
                    "{player} lacks {id}"
                );
            }
            assert!(owned.len() > plain.player(player).unwrap().technologies.len());
        }
        assert_eq!(
            serde_json::to_string(&techs.board).unwrap(),
            serde_json::to_string(&invasion.board).unwrap(),
            "the board is the invasion preset's"
        );
    }

    #[test]
    fn the_leaders_preset_rotates_the_factions_and_unlocks_every_leader() {
        for n in 3..=6 {
            for seed in 0..8 {
                let list = players(n);
                let (state, _) = preset_game(content(), &list, seed, None, LEADERS)
                    .unwrap_or_else(|e| panic!("{n}p seed {seed}: {e}"));
                let rotated = rotation(LEADERS, seed);
                assert!((2..=5).contains(&rotated));
                let factions: Vec<&str> = list
                    .iter()
                    .map(|p| state.player(p).unwrap().faction.as_str())
                    .collect();
                if n <= 4 {
                    assert!(
                        factions.contains(&"jolnar") || factions.contains(&"l1z1x"),
                        "{n}p seed {seed}: {factions:?}"
                    );
                }
                let distinct: BTreeSet<_> = factions.iter().collect();
                assert_eq!(distinct.len(), n.min(6), "{factions:?}");
                for player in &list {
                    let seat = state.player(player).unwrap();
                    assert!(!seat.leaders.is_empty());
                    for (leader, status) in &seat.leaders {
                        let kind = leaders::kind_of(content(), leader);
                        let expected = if kind.as_deref() == Some(leaders::AGENT) {
                            LeaderStatus::Readied
                        } else {
                            LeaderStatus::Unlocked
                        };
                        assert_eq!(*status, expected, "{n}p seed {seed}: {leader}");
                    }
                }
            }
        }
    }

    #[test]
    fn rotation_is_zero_without_the_leaders_preset_or_suffix_and_the_suffix_is_known() {
        assert_eq!(rotation(COMBAT, 5), 0);
        assert_eq!(rotation(INVASION, 5), 0);
        assert!(rotation("invasion+rot", 5) >= 2);
        assert!(is_known("invasion+rot") && is_known(LEADERS) && !is_known("nope+rot"));
        let list = players(4);
        let (plain, _) = create_game_with_template(content(), &list, 9, None).unwrap();
        let (rotated, _) = preset_game(content(), &list, 9, None, "combat+rot").unwrap();
        let faction = |s: &GameState, i: usize| s.player(&list[i]).unwrap().faction.clone();
        assert_ne!(faction(&plain, 0), faction(&rotated, 0));
        // The combat fleets were built for the factions that actually sit there.
        let (_, _) = preset_game(content(), &players(5), 3, None, "combat+rot").unwrap();
    }

    #[test]
    fn the_endgame_preset_sets_scores_round_and_an_empty_objective_deck() {
        for n in 3..=6 {
            let list = players(n);
            let (state, _) = preset_game(content(), &list, 4, None, ENDGAME).unwrap();
            assert_eq!(state.round, 5);
            assert!(state.objective_deck.is_empty());
            let mut points: Vec<i32> = list
                .iter()
                .map(|p| state.player(p).unwrap().victory_points)
                .collect();
            points.sort_unstable();
            assert_eq!(points.iter().filter(|p| **p == 9).count(), 2, "{points:?}");
            assert!(points.iter().all(|p| *p == 8 || *p == 9));
        }
    }

    #[test]
    fn every_tech_pool_entry_exists() {
        for id in TECH_POOL {
            assert!(
                content().get(ContentType::Technologies, id).is_some(),
                "{id}"
            );
        }
    }

    #[test]
    fn every_relic_pool_entry_exists() {
        for id in RELIC_POOL {
            assert!(content().get(ContentType::Relics, id).is_some(), "{id}");
        }
    }

    #[test]
    fn every_pool_card_exists_in_the_corpus() {
        for id in CARD_POOL {
            assert!(
                content().get(ContentType::ActionCards, id).is_some(),
                "{id}"
            );
        }
    }

    #[test]
    fn the_siege_preset_garrisons_every_colony_and_arms_the_invader_for_assault_cannon() {
        for n in 3..=6 {
            for seed in 0..6 {
                let list = players(n);
                let (state, _) = preset_game(content(), &list, seed, None, SIEGE)
                    .unwrap_or_else(|e| panic!("{n}p seed {seed}: {e}"));
                let mut garrisoned = 0;
                for board in state.board.values() {
                    let owners: BTreeSet<&PlayerId> =
                        board.units.iter().map(|u| &u.owner).collect();
                    if owners.len() != 2
                        || board
                            .planet_units
                            .values()
                            .flatten()
                            .all(|u| base_of(u.type_id.as_str()) != "pds")
                    {
                        continue;
                    }
                    garrisoned += 1;
                    for owner in owners {
                        let kinds: BTreeSet<&str> = board
                            .units
                            .iter()
                            .filter(|u| &u.owner == owner)
                            .map(|u| base_of(u.type_id.as_str()))
                            .filter(|t| {
                                ["carrier", "cruiser", "destroyer", "dreadnought"].contains(t)
                            })
                            .collect();
                        assert!(kinds.len() >= 3, "{n}p seed {seed}: {owner} has {kinds:?}");
                        assert!(
                            board
                                .units
                                .iter()
                                .any(|u| &u.owner == owner
                                    && base_of(u.type_id.as_str()) == "fighter")
                        );
                    }
                }
                assert_eq!(
                    garrisoned, n,
                    "{n}p seed {seed}: one contested colony per seat"
                );
                for player in &list {
                    let seat = state.player(player).unwrap();
                    assert!(seat.technologies.contains(&TechnologyId::new("asc")));
                    assert!(!seat.action_cards.is_empty());
                }
            }
        }
    }

    #[test]
    fn the_bombard_preset_has_no_pds_two_dreadnoughts_and_a_third_seats_infantry() {
        for n in 3..=6 {
            let list = players(n);
            let (state, _) = preset_game(content(), &list, 3, None, BOMBARD).unwrap();
            let mut contested = 0;
            for board in state.board.values() {
                let dreads = board
                    .units
                    .iter()
                    .filter(|u| u.type_id.as_str().contains("dread"))
                    .count();
                if dreads != 2 {
                    continue;
                }
                contested += 1;
                assert!(
                    board
                        .planet_units
                        .values()
                        .flatten()
                        .all(|u| base_of(u.type_id.as_str()) != "pds")
                );
                let owners: BTreeSet<&PlayerId> = board
                    .planet_units
                    .values()
                    .flatten()
                    .map(|u| &u.owner)
                    .collect();
                assert_eq!(owners.len(), 2, "{n}p: the planet is shared by two seats");
            }
            assert_eq!(contested, n);
        }
    }

    #[test]
    fn the_capture_preset_seats_the_cabal_with_vortex_targets_and_captured_units() {
        for n in 3..=6 {
            for seed in 0..6 {
                let list = players(n);
                let (state, galaxy) = preset_game(content(), &list, seed, None, CAPTURE)
                    .unwrap_or_else(|e| panic!("{n}p seed {seed}: {e}"));
                let cabal: Vec<&PlayerId> = list
                    .iter()
                    .filter(|p| state.player(p).unwrap().faction.as_str() == "cabal")
                    .collect();
                assert_eq!(cabal.len(), 1, "{n}p seed {seed}");
                let seat = state.player(cabal[0]).unwrap();
                assert!(seat.technologies.contains(&TechnologyId::new("vtx")));
                assert!(seat.captured_units.len() >= n - 1);
                // Somebody else's fleet is next to the Cabal's home dock.
                let assignments = BTreeMap::from([(cabal[0].clone(), seat.faction.clone())]);
                let home = seating::home_systems(content(), &assignments)
                    .unwrap()
                    .remove(0);
                let adjacent: Vec<&str> = galaxy.adjacent(home.as_str()).into_iter().collect();
                assert!(
                    adjacent.iter().any(|id| state
                        .board
                        .get(&SystemId::new(*id))
                        .is_some_and(|b| b.units.iter().any(|u| &u.owner != cabal[0]))),
                    "{n}p seed {seed}: no raider beside the Cabal"
                );
            }
        }
    }

    #[test]
    fn suffixes_parse_in_any_order_and_unknown_ones_do_not() {
        for good in [
            "world",
            "explore+short",
            "notes+rot",
            "combat+fac",
            "techs+short+fac:naalu:mentak",
            "agenda+fac:keleresm+rot",
            "leaders+short",
            "combat+agenda",
            "world+agenda+short",
            "techs+rot+agenda+fac",
        ] {
            assert!(is_known(good), "{good}");
        }
        for bad in [
            "nope",
            "world+nope",
            "combat+fac:nope",
            "combat+short+",
            "combat+agenda:x",
            "+rot",
            "explore++short",
        ] {
            assert!(!is_known(bad), "{bad}");
        }
    }

    #[test]
    fn the_roster_seats_distinct_factions_and_honours_a_named_prefix() {
        assert!(roster(COMBAT, 3, &players(3)).is_none());
        assert!(roster("combat+rot", 3, &players(3)).is_none());
        for seed in 0..12 {
            for n in 3..=8 {
                let list = players(n);
                let seated = roster("combat+fac", seed, &list).unwrap();
                let distinct: BTreeSet<_> = seated.values().collect();
                assert_eq!(distinct.len(), n, "seed {seed}: {seated:?}");
                assert!(roster(WORLD, seed, &list).is_some());
            }
        }
        let seated = roster("world+fac:yssaril:naalu", 5, &players(4)).unwrap();
        assert_eq!(seated[&PlayerId::new("p1")].as_str(), "yssaril");
        assert_eq!(seated[&PlayerId::new("p2")].as_str(), "naalu");
        assert_eq!(seated.values().collect::<BTreeSet<_>>().len(), 4);
    }

    #[test]
    fn every_roster_faction_is_in_the_corpus_and_the_walk_reaches_all_of_them() {
        for alias in ROSTER {
            assert!(
                ti4_content::factions::get(content(), alias).is_some(),
                "{alias}"
            );
        }
        let mut reached: BTreeSet<String> = BTreeSet::new();
        for seed in 0..40 {
            for faction in roster(WORLD, seed, &players(4)).unwrap().values() {
                reached.insert(faction.as_str().to_owned());
            }
        }
        let expected: BTreeSet<String> = ROSTER
            .iter()
            .filter(|a| !HELD_OUT.contains(a))
            .map(|a| (*a).to_owned())
            .collect();
        assert_eq!(reached, expected);
    }

    #[test]
    fn world_builds_for_every_roster_faction_at_every_table_size() {
        let mut failures = Vec::new();
        for alias in ROSTER {
            for (n, seed) in [(3, 1_u64), (4, 2), (6, 3)] {
                let list = players(n);
                let name = format!("world+fac:{alias}");
                match preset_game(content(), &list, seed, None, &name) {
                    Ok((state, _)) => {
                        let seat = state.player(&list[0]).unwrap();
                        assert_eq!(seat.faction.as_str(), *alias);
                        assert!(!seat.leaders.is_empty(), "{alias}");
                    }
                    Err(e) => failures.push(format!("{alias} {n}p seed {seed}: {e}")),
                }
            }
        }
        assert!(failures.is_empty(), "{failures:#?}");
    }

    #[test]
    fn world_gives_every_seat_foreign_notes_and_its_own_faction_technologies() {
        let list = players(4);
        let (state, _) = preset_game(content(), &list, 7, None, "world+fac:naalu").unwrap();
        for player in &list {
            let name = ti4_engine::promissory::faction_name(&state, player);
            let foreign = ti4_engine::promissory::held_by(&state, player)
                .into_iter()
                .filter(|note| {
                    ti4_engine::promissory::owner_of(note).as_deref() != Some(name.as_str())
                })
                .count();
            assert!(foreign >= 4, "{player} holds {foreign} foreign notes");
        }
        let naalu = state.player(&list[0]).unwrap();
        assert!(naalu.technologies.len() > 2, "{:?}", naalu.technologies);
    }

    #[test]
    fn world_gives_nekro_only_the_sheet_technologies() {
        let list = players(4);
        let (state, _) = preset_game(content(), &list, 7, None, "world+fac:nekro").unwrap();
        let nekro = state.player(&list[0]).unwrap();
        assert_eq!(nekro.faction.as_str(), "nekro", "{:?}", nekro.faction);
        let owned: Vec<&str> = nekro.technologies.iter().map(|t| t.as_str()).collect();
        assert!(owned.len() > 2, "{owned:?}");
        for banned in ["nekroc4r", "nekroc4y"] {
            assert!(!owned.contains(&banned), "{owned:?}");
        }
        assert!(
            owned.contains(&"vax") && owned.contains(&"vay"),
            "{owned:?}"
        );
    }

    #[test]
    fn the_notes_preset_only_moves_notes() {
        let list = players(4);
        let (plain, _) = create_game_with_template(content(), &list, 5, None).unwrap();
        let (notes, _) = preset_game(content(), &list, 5, None, NOTES).unwrap();
        assert_ne!(plain.promissory_notes, notes.promissory_notes);
        // A plain server game deals every seat its own notes after seating: no "generic" ones.
        assert!(
            plain.promissory_notes.len() >= 4 * 4,
            "{:?}",
            plain.promissory_notes
        );
        assert!(
            plain
                .promissory_notes
                .keys()
                .all(|n| !n.ends_with(":generic"))
        );
        assert!(notes.promissory_notes.len() >= 4 * 5);
        assert_eq!(
            serde_json::to_string(&plain.board).unwrap(),
            serde_json::to_string(&notes.board).unwrap()
        );
        assert!(!notes.support_holders.is_empty());
    }

    #[test]
    fn the_explore_preset_stages_landings_frontier_ships_fragments_and_decks() {
        // Hyperlane tiles are not systems, so a frontier token on one is no frontier site: the
        // default 3-player map has no site at all and the 6-player map none; 3pInPersonHyperlanes
        // has real planetless systems (46, 47, 50).
        for (n, seed, template) in [
            (3, 1_u64, Some("3pInPersonHyperlanes")),
            (3, 5, Some("3pInPersonHyperlanes")),
            (3, 7, None),
            (4, 2, None),
            (4, 9, None),
            (6, 4, None),
        ] {
            let list = players(n);
            let loader = crate::maps::TemplateLoader::load().unwrap();
            let template = template
                .map(str::to_owned)
                .or_else(|| crate::maps::default_template_for(content(), &loader, n, POK));
            let (state, galaxy) =
                preset_game(content(), &list, seed, template.as_deref(), EXPLORE).unwrap();
            let frontier = ti4_engine::exploration::frontier_systems(content(), POK, &galaxy);
            let mut landings = 0;
            let mut frontier_ships = 0;
            for player in &list {
                let seat = state.player(player).unwrap();
                assert!(seat.technologies.contains(&TechnologyId::new("det")));
                assert!(!seat.relic_fragments.is_empty());
                for (id, board) in &state.board {
                    let has = |kind: &str| {
                        board
                            .units
                            .iter()
                            .any(|u| &u.owner == player && is_kind(u.type_id.as_str(), kind))
                    };
                    if has("infantry") && board.planet_control.is_empty() {
                        assert!(traited_planet(content(), id.as_str()).is_some(), "{id}");
                        landings += 1;
                    }
                    if has("destroyer") && frontier.contains(id) {
                        frontier_ships += 1;
                    }
                }
            }
            assert!(landings >= n - 1, "{n}p seed {seed}: {landings} landings");
            assert!(
                frontier_ships >= usize::from(!frontier.is_empty() && n < 6),
                "{n}p seed {seed}: {frontier_ships} frontier ships"
            );
            assert_eq!(state.exploration_decks["CULTURAL"][0], "mo1");
            assert_eq!(state.exploration_decks["FRONTIER"][0], "ms1");
        }
    }

    #[test]
    fn the_agenda_suffix_removes_the_custodians_on_any_preset_and_is_deterministic() {
        for n in 3..=6 {
            let list = players(n);
            for name in ["combat", "cards", "world", "leaders", "explore", "techs"] {
                let plain = preset_game(content(), &list, 9, None, name).unwrap().0;
                assert!(!plain.custodians_removed, "{name} {n}p");
                let with = format!("{name}+agenda");
                let (a, _) = preset_game(content(), &list, 9, None, &with).unwrap();
                let (b, _) = preset_game(content(), &list, 9, None, &with).unwrap();
                assert!(a.custodians_removed, "{with} {n}p");
                assert_eq!(a.agenda_deck, b.agenda_deck, "{with} {n}p: deterministic");
                for player in &list {
                    let (x, y) = (a.player(player).unwrap(), plain.player(player).unwrap());
                    assert_eq!(x.trade_goods, y.trade_goods + AGENDA_TRADE_GOODS, "{with}");
                }
            }
            let (s, _) = preset_game(content(), &list, 9, None, "agenda+short+agenda").unwrap();
            assert!(s.custodians_removed && s.objective_deck.len() == 3);
        }
    }

    #[test]
    fn short_games_start_on_a_few_points_with_three_objectives_left() {
        let list = players(4);
        let (plain, _) = create_game_with_template(content(), &list, 6, None).unwrap();
        let (short, _) = preset_game(content(), &list, 6, None, "techs+short").unwrap();
        assert_eq!(short.objective_deck.len(), 3);
        assert!(plain.objective_deck.len() > 3);
        for player in &list {
            let vp = short.player(player).unwrap().victory_points;
            assert!((3..=5).contains(&vp), "{vp}");
        }
    }

    #[test]
    fn exploration_top_cards_exist_in_the_corpus() {
        for (_, cards) in EXPLORE_TOPS {
            for id in *cards {
                assert!(content().get(ContentType::Explores, id).is_some(), "{id}");
            }
        }
    }

    #[test]
    fn slow_every_preset_builds_with_roster_factions_and_short_at_every_table_size() {
        let loader = crate::maps::TemplateLoader::load().unwrap();
        let mut failures = Vec::new();
        for base in KNOWN {
            for suffix in ["+fac", "+fac+short", "+short", "+rot+short"] {
                for n in 3..=6 {
                    for seed in [1_u64, 2, 3, 4] {
                        let list = players(n);
                        let template =
                            crate::maps::default_template_for(content(), &loader, n, POK);
                        let name = format!("{base}{suffix}");
                        if let Err(e) =
                            preset_game(content(), &list, seed, template.as_deref(), &name)
                        {
                            failures.push(format!("{name} {n}p seed {seed}: {e}"));
                        }
                    }
                }
            }
        }
        assert!(
            failures.is_empty(),
            "{} failures: {:#?}",
            failures.len(),
            &failures[..failures.len().min(25)]
        );
    }

    #[test]
    fn leaders_rotates_the_factions_with_or_without_suffixes() {
        for name in ["leaders", "leaders+short", "leaders+rot+short"] {
            assert!((2..=5).contains(&rotation(name, 3)), "{name}");
        }
        assert_eq!(rotation("combat+short", 3), 0);
        assert_eq!(rotation("techs+fac", 3), 0);
        let list = players(3);
        let (state, _) = preset_game(content(), &list, 3, None, "leaders+short").unwrap();
        let factions: Vec<&str> = list
            .iter()
            .map(|p| state.player(p).unwrap().faction.as_str())
            .collect();
        assert!(
            factions.contains(&"jolnar") || factions.contains(&"l1z1x"),
            "{factions:?}"
        );
    }

    #[test]
    fn the_known_presets_match_the_harness_list() {
        let ts = include_str!("../../../web/e2e/smokePreset.ts");
        let line = ts
            .lines()
            .find(|l| l.contains("KNOWN_PRESETS ="))
            .expect("KNOWN_PRESETS in smokePreset.ts");
        let listed: Vec<&str> = line.split('"').skip(1).step_by(2).collect();
        assert_eq!(listed, KNOWN);
    }
}
