//! Faction plans for the rule bot ([`crate`]): what each faction researches
//! and builds from round 3 on.
//!
//! The profiles behind the data are written up in `crates/ti4-server/docs/FACTION_PLANS.md`.
//! Each [`Plan`] is an ordered technology priority (after Gravity Drive, which the rule bot
//! already takes whenever offered) and a unit emphasis per phase: a weight on each base unit
//! type and how many of it the faction wants on the board. Three hooks read it:
//!
//! - [`research_pick`]: the first technology of the plan that is offered; when the next one is
//!   out of reach, the best offered step toward it (a technology of a colour its prerequisites
//!   still lack, preferring one that is itself on the plan, then the cheapest); then the same
//!   over [`GENERAL`], the order every faction falls back on.
//! - [`unit_weight`]: a multiplier on a production option's worth, 1.0 in the opening rounds
//!   ([`crate::EXPANSION_ROUNDS`]) so the opening is unchanged.
//! - [`preferred_card`]: from round 3 the strategy card the plan prefers, only where the smart
//!   bot's own rules leave the draft open.
//! - [`ships_wanted`]: how many non-fighter ships the seat builds toward before a dock is no
//!   longer worth a command token just to build there (6 in the opening, as before).
//!
//! Own-seat view only (the seat's faction, technologies and units, all public), deterministic.

use crate::tuning;
use std::collections::BTreeMap;

use ti4_content::units::UnitType;
use ti4_engine::choice::{Choice, Observed};
use ti4_model::id::{PlayerId, TechnologyId};

/// The opening's fleet size: below this many non-fighter ships a dock is worth activating to
/// build ([`crate`]'s rule before faction plans).
pub const OPENING_SHIPS: usize = 6;
/// Weight multiplier while the seat has fewer of a unit type than its plan wants this phase.
pub const SHORT_BONUS: f64 = 1.5;
/// Weight multiplier for a unit the seat has upgraded (it is the better buy now).
pub const UPGRADED_BONUS: f64 = 1.15;
/// The first round of the late game; rounds from 3 up to this one are the mid game.
pub const LATE_FROM: u32 = 5;
/// The last round of the opening (as [`crate::EXPANSION_ROUNDS`]).
const OPENING_ROUNDS: u32 = 2;

/// One unit type's emphasis: `(base type, weight mid, weight late, wanted mid, wanted late)`.
#[derive(Debug, Clone, Copy)]
pub struct Emphasis(pub &'static str, pub f64, pub f64, pub usize, pub usize);

/// A faction's mid- and late-game plan.
#[derive(Debug)]
pub struct Plan {
    /// Faction aliases the plan is for.
    pub factions: &'static [&'static str],
    /// Research priority after Gravity Drive; `a|b` takes either.
    pub techs: &'static [&'static str],
    /// Unit emphasis; unlisted types weigh 1.0 and are not wanted in any number.
    pub units: &'static [Emphasis],
    /// Non-fighter ships wanted on the board: mid game, late game.
    pub ships: [usize; 2],
    /// Strategy cards it takes from round 3 when no rule of the rule bot decides (no Imperial
    /// to score, no card with trade goods on it), by name, best first.
    pub cards: &'static [&'static str],
}

/// The research order every faction falls back on once its own list is done or out of reach:
/// passive and combat technologies first (component actions are mostly left to the random
/// pick, so they are worth less to the bot), the common unit upgrades, then the rest.
pub const GENERAL: &[&str] = &[
    "gd",
    "sar",
    "st",
    "nm",
    "ps",
    "dxa",
    "gls",
    "amd|sr",
    "cv2",
    "ff2",
    "dn2",
    "da",
    "hm",
    "pi",
    "bs",
    "sdn",
    "aida",
    "pa",
    "det",
    "cr2",
    "dd2",
    "inf2",
    "pds2",
    "sd2",
    "fl",
    "asc",
    "td",
    "ie",
    "md|md_c1|md_base",
    "x89|x89_base|x89c4",
    "lwd",
    "ws",
];

/// Every plan. See `docs/FACTION_PLANS.md` for the reasoning per faction.
pub const PLANS: &[Plan] = &[
    Plan {
        factions: &["arborec"],
        techs: &[
            "nm", "dxa", "lw2", "amd|sr", "cv2", "bio", "hm", "sar", "da",
        ],
        units: &[
            Emphasis("carrier", 1.3, 1.0, 3, 2),
            Emphasis("mech", 1.3, 1.2, 2, 3),
            Emphasis("flagship", 1.2, 1.5, 0, 1),
            Emphasis("dreadnought", 1.0, 1.4, 1, 2),
            Emphasis("fighter", 0.8, 0.8, 0, 0),
        ],
        ships: [7, 9],
        cards: &["Construction", "Warfare"],
    },
    Plan {
        factions: &["ghost"],
        techs: &["sr|amd", "ps", "ds", "cv2", "st", "dn2", "fl", "wg", "da"],
        units: &[
            Emphasis("dreadnought", 1.3, 1.5, 2, 3),
            Emphasis("carrier", 1.1, 1.0, 2, 2),
            Emphasis("destroyer", 1.1, 0.9, 0, 0),
            Emphasis("flagship", 1.0, 1.3, 0, 1),
        ],
        ships: [8, 10],
        cards: &["Leadership", "Warfare"],
    },
    Plan {
        factions: &["hacan"],
        techs: &[
            "gls", "sdn|pi", "qdn", "nm", "dxa", "pm", "cv2", "dn2", "ie",
        ],
        units: &[
            Emphasis("dreadnought", 1.4, 1.5, 2, 3),
            Emphasis("carrier", 1.1, 1.0, 2, 2),
            Emphasis("fighter", 1.1, 1.0, 0, 0),
            Emphasis("flagship", 1.0, 1.3, 0, 1),
        ],
        ships: [8, 10],
        cards: &["Trade", "Technology"],
    },
    Plan {
        factions: &["jolnar"],
        techs: &[
            "gls", "ers", "sar", "cv2", "ff2", "dxa", "dn2", "pi", "da", "hm", "fl", "scc", "ie",
            "asc",
        ],
        units: &[
            Emphasis("dreadnought", 1.4, 1.5, 2, 3),
            Emphasis("carrier", 1.1, 1.0, 2, 2),
            Emphasis("fighter", 1.3, 1.2, 0, 0),
            Emphasis("cruiser", 0.8, 0.8, 0, 0),
            Emphasis("destroyer", 0.8, 0.8, 0, 0),
            Emphasis("flagship", 1.0, 1.2, 0, 1),
        ],
        ships: [7, 9],
        cards: &["Technology", "Leadership"],
    },
    Plan {
        factions: &["l1z1x"],
        techs: &[
            "amd|sr", "st", "sdn2", "sar", "ff2", "da", "gls", "is", "asc", "fl",
        ],
        units: &[
            Emphasis("dreadnought", 1.8, 1.8, 4, 5),
            Emphasis("carrier", 1.0, 0.9, 1, 1),
            Emphasis("fighter", 1.2, 1.2, 0, 0),
            Emphasis("flagship", 1.0, 1.3, 0, 1),
            Emphasis("cruiser", 0.7, 0.7, 0, 0),
            Emphasis("destroyer", 0.8, 0.8, 0, 0),
        ],
        ships: [9, 11],
        cards: &["Warfare", "Technology"],
    },
    Plan {
        factions: &["letnev"],
        techs: &["sar", "nes", "st", "dn2", "da", "l4", "asc", "cv2"],
        units: &[
            Emphasis("dreadnought", 1.7, 1.8, 3, 4),
            Emphasis("carrier", 1.0, 0.9, 1, 1),
            Emphasis("fighter", 1.1, 1.1, 0, 0),
            Emphasis("flagship", 1.0, 1.4, 0, 1),
        ],
        ships: [9, 11],
        cards: &["Warfare", "Leadership"],
    },
    Plan {
        factions: &["mentak"],
        techs: &["gls", "so", "sdn|pi", "mc", "nm", "cr2", "sar", "dd2", "da"],
        units: &[
            Emphasis("cruiser", 1.5, 1.2, 4, 4),
            Emphasis("destroyer", 1.1, 1.0, 0, 0),
            Emphasis("dreadnought", 1.0, 1.4, 1, 2),
            Emphasis("carrier", 1.0, 1.0, 1, 1),
            Emphasis("flagship", 1.0, 1.3, 0, 1),
        ],
        ships: [8, 10],
        cards: &["Technology", "Warfare"],
    },
    Plan {
        factions: &["muaat"],
        techs: &[
            "sar", "st", "da", "pws2", "mr", "gls", "amd|sr", "cv2", "asc",
        ],
        units: &[
            Emphasis("warsun", 1.8, 2.0, 2, 3),
            Emphasis("dreadnought", 1.1, 1.0, 1, 1),
            Emphasis("carrier", 1.1, 1.0, 1, 2),
            Emphasis("fighter", 1.2, 1.2, 0, 0),
        ],
        ships: [7, 9],
        cards: &["Warfare", "Technology"],
    },
    Plan {
        factions: &["naalu"],
        techs: &[
            "hcf2", "dxa", "amd|sr", "cv2", "bs", "ng", "ps", "sar", "gls",
        ],
        units: &[
            Emphasis("fighter", 2.0, 1.8, 0, 0),
            Emphasis("carrier", 1.4, 1.2, 3, 3),
            Emphasis("dreadnought", 1.0, 1.3, 1, 2),
            Emphasis("flagship", 1.1, 1.3, 0, 1),
            Emphasis("destroyer", 0.8, 0.8, 0, 0),
        ],
        ships: [7, 9],
        cards: &["Technology", "Leadership"],
    },
    Plan {
        // Propagation: the Nekro never research, so only the units matter.
        factions: &["nekro"],
        techs: &[],
        units: &[
            Emphasis("dreadnought", 1.5, 1.6, 2, 4),
            Emphasis("cruiser", 1.1, 1.0, 0, 0),
            Emphasis("flagship", 1.0, 1.4, 0, 1),
            Emphasis("mech", 1.1, 1.1, 0, 0),
        ],
        ships: [8, 10],
        cards: &["Technology", "Warfare"],
    },
    Plan {
        factions: &["saar"],
        techs: &["cv2", "st", "gls", "ffac2", "cm", "ps", "sar", "da", "dn2"],
        units: &[
            Emphasis("carrier", 1.4, 1.1, 3, 3),
            Emphasis("destroyer", 1.0, 0.9, 0, 0),
            Emphasis("mech", 1.2, 1.1, 1, 2),
            Emphasis("dreadnought", 1.0, 1.3, 1, 2),
        ],
        ships: [8, 10],
        cards: &["Construction", "Leadership"],
    },
    Plan {
        factions: &["sardakk"],
        techs: &[
            "ps", "sar", "vpw", "st", "amd|sr", "exo2", "da", "dd2", "asc",
        ],
        units: &[
            Emphasis("dreadnought", 1.5, 1.7, 2, 4),
            Emphasis("carrier", 1.1, 1.0, 2, 2),
            Emphasis("mech", 1.2, 1.1, 1, 2),
            Emphasis("flagship", 1.0, 1.4, 0, 1),
        ],
        ships: [8, 10],
        cards: &["Warfare", "Leadership"],
    },
    Plan {
        factions: &["sol"],
        techs: &["ac2", "dxa", "so2", "st", "dn2", "hm", "ps", "sar", "ff2"],
        units: &[
            Emphasis("carrier", 1.5, 1.2, 3, 3),
            Emphasis("infantry", 1.2, 1.1, 0, 0),
            Emphasis("flagship", 1.1, 1.5, 0, 1),
            Emphasis("dreadnought", 1.0, 1.3, 1, 2),
        ],
        ships: [8, 10],
        cards: &["Leadership", "Warfare"],
    },
    Plan {
        factions: &["winnu"],
        techs: &[
            "st", "amd|sr", "lgf", "gls", "htp", "ps", "sar", "da", "dn2",
        ],
        units: &[
            Emphasis("dreadnought", 1.3, 1.5, 2, 3),
            Emphasis("carrier", 1.1, 1.0, 2, 2),
            Emphasis("mech", 1.2, 1.1, 1, 2),
            Emphasis("flagship", 1.0, 1.3, 0, 1),
        ],
        ships: [8, 10],
        cards: &["Leadership", "Technology"],
    },
    Plan {
        factions: &["xxcha"],
        techs: &[
            "st", "nf", "nm", "it", "ps", "pds2", "cr2", "sar", "amd|sr", "dn2",
        ],
        units: &[
            Emphasis("cruiser", 1.3, 1.1, 3, 3),
            Emphasis("dreadnought", 1.0, 1.4, 1, 2),
            Emphasis("mech", 1.2, 1.1, 1, 2),
            Emphasis("flagship", 1.0, 1.3, 0, 1),
        ],
        ships: [8, 10],
        cards: &["Politics", "Diplomacy"],
    },
    Plan {
        factions: &["yin"],
        techs: &[
            "nm", "dxa", "yso", "gls", "ic", "ps", "sar", "amd|sr", "cv2", "ff2", "hm",
        ],
        units: &[
            Emphasis("carrier", 1.2, 1.0, 2, 2),
            Emphasis("destroyer", 1.2, 1.0, 0, 0),
            Emphasis("cruiser", 1.2, 1.1, 0, 0),
            Emphasis("infantry", 1.1, 1.0, 0, 0),
            Emphasis("dreadnought", 1.0, 1.3, 1, 2),
        ],
        ships: [8, 10],
        cards: &["Construction", "Warfare"],
    },
    Plan {
        factions: &["yssaril"],
        techs: &[
            "dxa", "tp", "mi", "st", "ps", "sar", "hm", "amd|sr", "cv2", "dn2",
        ],
        units: &[
            Emphasis("carrier", 1.2, 1.0, 2, 2),
            Emphasis("cruiser", 1.1, 1.0, 0, 0),
            Emphasis("mech", 1.1, 1.1, 1, 1),
            Emphasis("dreadnought", 1.0, 1.4, 1, 2),
            Emphasis("flagship", 1.0, 1.3, 0, 1),
        ],
        ships: [8, 10],
        cards: &["Politics", "Technology"],
    },
    Plan {
        factions: &["keleresm", "keleresx", "keleresa"],
        techs: &[
            "st", "iihq", "asn", "sdn|pi", "ps", "sar", "amd|sr", "cv2", "dn2", "da",
        ],
        units: &[
            Emphasis("dreadnought", 1.3, 1.5, 2, 3),
            Emphasis("carrier", 1.1, 1.0, 2, 2),
            Emphasis("flagship", 1.0, 1.3, 0, 1),
        ],
        ships: [8, 10],
        cards: &["Politics", "Leadership"],
    },
    Plan {
        factions: &["argent"],
        techs: &[
            "ps", "sar", "swa2", "st", "ah", "pds2", "da", "amd|sr", "cv2", "dn2",
        ],
        units: &[
            Emphasis("destroyer", 1.5, 1.2, 4, 4),
            Emphasis("carrier", 1.0, 1.0, 1, 2),
            Emphasis("dreadnought", 1.0, 1.4, 1, 2),
            Emphasis("flagship", 1.0, 1.3, 0, 1),
        ],
        ships: [9, 11],
        cards: &["Politics", "Warfare"],
    },
    Plan {
        factions: &["cabal"],
        techs: &[
            "vtx", "da", "st", "gls", "dt2", "asc", "amd|sr", "cv2", "nm", "ff2",
        ],
        units: &[
            Emphasis("dreadnought", 1.5, 1.6, 3, 4),
            Emphasis("fighter", 1.2, 1.2, 0, 0),
            Emphasis("carrier", 1.0, 1.0, 1, 1),
            Emphasis("flagship", 1.2, 1.5, 0, 1),
        ],
        ships: [8, 10],
        cards: &["Warfare", "Technology"],
    },
    Plan {
        factions: &["empyrean"],
        techs: &["as", "nm", "vw", "fl", "dxa", "cv2", "st", "dn2"],
        units: &[
            Emphasis("carrier", 1.2, 1.0, 3, 2),
            Emphasis("destroyer", 1.1, 0.9, 0, 0),
            Emphasis("dreadnought", 1.1, 1.4, 1, 2),
            Emphasis("flagship", 1.0, 1.3, 0, 1),
        ],
        ships: [8, 10],
        cards: &["Diplomacy", "Leadership"],
    },
    Plan {
        factions: &["mahact"],
        techs: &[
            "gr", "cl2", "ps", "sar", "da", "gls", "amd|sr", "dn2", "ff2", "asc",
        ],
        units: &[
            Emphasis("dreadnought", 1.4, 1.6, 2, 3),
            Emphasis("carrier", 1.0, 1.0, 1, 1),
            Emphasis("flagship", 1.2, 1.5, 0, 1),
        ],
        ships: [8, 10],
        cards: &["Warfare", "Technology"],
    },
    Plan {
        factions: &["naaz"],
        techs: &[
            "sc", "dxa", "sar", "bs", "pfa", "da", "amd|sr", "cv2", "st", "dn2",
        ],
        units: &[
            Emphasis("mech", 1.6, 1.4, 4, 5),
            Emphasis("carrier", 1.2, 1.0, 2, 2),
            Emphasis("flagship", 1.1, 1.4, 0, 1),
            Emphasis("dreadnought", 1.0, 1.3, 1, 2),
        ],
        ships: [7, 9],
        cards: &["Construction", "Technology"],
    },
    Plan {
        factions: &["nomad"],
        techs: &["st", "nm", "m2", "tcs", "dn2", "ps", "sar", "cv2", "da"],
        units: &[
            Emphasis("dreadnought", 1.2, 1.5, 1, 3),
            Emphasis("carrier", 1.1, 1.0, 2, 2),
            Emphasis("flagship", 1.3, 1.5, 1, 1),
        ],
        ships: [8, 10],
        cards: &["Leadership", "Technology"],
    },
    Plan {
        factions: &["titans"],
        techs: &["ps", "nm", "se2", "ht2", "sar", "st", "dn2", "da"],
        units: &[
            Emphasis("cruiser", 1.6, 1.3, 4, 4),
            Emphasis("dreadnought", 1.0, 1.4, 1, 2),
            Emphasis("carrier", 0.9, 0.9, 1, 1),
            Emphasis("flagship", 1.0, 1.3, 0, 1),
        ],
        ships: [8, 10],
        cards: &["Construction", "Technology"],
    },
    Plan {
        factions: &["bastion"],
        techs: &[
            "st", "gls", "helios2", "ps", "proxima", "sar", "da", "amd|sr", "dn2",
        ],
        units: &[
            Emphasis("dreadnought", 1.4, 1.5, 2, 3),
            Emphasis("carrier", 1.0, 1.0, 1, 2),
            Emphasis("flagship", 1.0, 1.3, 0, 1),
        ],
        ships: [8, 10],
        cards: &["Warfare", "Technology"],
    },
    Plan {
        factions: &["deepwrought"],
        techs: &[
            "nm",
            "hydrothermal",
            "radical",
            "dxa",
            "ps",
            "sar",
            "ff2",
            "amd|sr",
            "cv2",
            "st",
            "dn2",
        ],
        units: &[
            Emphasis("carrier", 1.2, 1.0, 2, 2),
            Emphasis("fighter", 1.2, 1.1, 0, 0),
            Emphasis("dreadnought", 1.0, 1.4, 1, 2),
            Emphasis("flagship", 1.0, 1.3, 0, 1),
        ],
        ships: [8, 10],
        cards: &["Technology", "Leadership"],
    },
    Plan {
        factions: &["firmament", "obsidian"],
        techs: &[
            "nm",
            "dxa",
            "parasite-firm",
            "st",
            "gls",
            "ps",
            "sar",
            "amd|sr",
            "cv2",
            "dn2",
            "da",
        ],
        units: &[
            Emphasis("carrier", 1.1, 1.0, 2, 2),
            Emphasis("cruiser", 1.1, 1.0, 0, 0),
            Emphasis("dreadnought", 1.0, 1.4, 1, 2),
            Emphasis("flagship", 1.0, 1.3, 0, 1),
        ],
        ships: [8, 10],
        cards: &["Politics", "Leadership"],
    },
    Plan {
        factions: &["crimson"],
        techs: &[
            "ps",
            "sar",
            "exile2",
            "st",
            "subatomic",
            "da",
            "amd|sr",
            "cv2",
            "dn2",
        ],
        units: &[
            Emphasis("destroyer", 1.3, 1.1, 3, 3),
            Emphasis("carrier", 1.1, 1.0, 2, 2),
            Emphasis("dreadnought", 1.0, 1.4, 1, 2),
            Emphasis("flagship", 1.0, 1.3, 0, 1),
        ],
        ships: [8, 10],
        cards: &["Warfare", "Leadership"],
    },
    Plan {
        factions: &["ralnel"],
        techs: &[
            "ps",
            "nanomachines",
            "linkship2",
            "sar",
            "da",
            "st",
            "amd|sr",
            "dn2",
            "cv2",
        ],
        units: &[
            Emphasis("destroyer", 1.3, 1.1, 3, 3),
            Emphasis("dreadnought", 1.1, 1.4, 1, 2),
            Emphasis("carrier", 1.0, 1.0, 1, 2),
        ],
        ships: [8, 10],
        cards: &["Leadership", "Warfare"],
    },
];

/// Bot version 2 ([`tuning::plans_v2`]): reworked plans of the factions that won least in
/// self-play (golden set: yssaril 8.5%, l1z1x 9%, nekro 9%, hacan 10%, arborec 10%).
///
/// - The Yssaril take Leadership and Technology instead of Politics: the bot plays its action
///   cards mostly at random, so two more of them are worth less than tokens.
/// - The L1Z1X and the Nekro take Leadership first: a dreadnought fleet (and the Nekro, who
///   cannot research, its Propagation tokens from Technology later) moves on command tokens.
/// - The Hacan build carriers to expand on their trade goods, and take Leadership after Trade.
/// - The Arborec take Leadership after Construction and build more carriers for Mitosis' troops.
pub const PLANS_V2: &[Plan] = &[
    Plan {
        factions: &["yssaril"],
        techs: &[
            "dxa", "tp", "mi", "st", "ps", "sar", "hm", "amd|sr", "cv2", "dn2",
        ],
        units: &[
            Emphasis("carrier", 1.3, 1.0, 3, 2),
            Emphasis("cruiser", 1.1, 1.0, 0, 0),
            Emphasis("mech", 1.1, 1.1, 1, 1),
            Emphasis("dreadnought", 1.1, 1.4, 1, 2),
            Emphasis("flagship", 1.0, 1.3, 0, 1),
        ],
        ships: [8, 10],
        cards: &["Leadership", "Technology", "Warfare"],
    },
    Plan {
        factions: &["l1z1x"],
        techs: &[
            "amd|sr", "st", "sdn2", "sar", "ff2", "da", "gls", "is", "asc", "fl",
        ],
        units: &[
            Emphasis("dreadnought", 1.8, 1.8, 4, 5),
            Emphasis("carrier", 1.1, 1.0, 2, 1),
            Emphasis("fighter", 1.2, 1.2, 0, 0),
            Emphasis("flagship", 1.0, 1.3, 0, 1),
            Emphasis("cruiser", 0.7, 0.7, 0, 0),
            Emphasis("destroyer", 0.8, 0.8, 0, 0),
            Emphasis("infantry", 0.8, 0.8, 0, 0),
        ],
        ships: [9, 11],
        cards: &["Leadership", "Warfare", "Technology"],
    },
    Plan {
        factions: &["nekro"],
        techs: &[],
        units: &[
            Emphasis("dreadnought", 1.5, 1.6, 2, 4),
            Emphasis("carrier", 1.1, 1.0, 2, 1),
            Emphasis("cruiser", 1.1, 1.0, 0, 0),
            Emphasis("flagship", 1.0, 1.4, 0, 1),
            Emphasis("mech", 1.1, 1.1, 0, 0),
        ],
        ships: [8, 10],
        cards: &["Leadership", "Technology", "Warfare"],
    },
    Plan {
        factions: &["hacan"],
        techs: &[
            "gls", "sdn|pi", "qdn", "nm", "dxa", "pm", "cv2", "dn2", "ie",
        ],
        units: &[
            Emphasis("dreadnought", 1.3, 1.5, 2, 3),
            Emphasis("carrier", 1.3, 1.0, 3, 2),
            Emphasis("fighter", 1.1, 1.0, 0, 0),
            Emphasis("flagship", 1.0, 1.3, 0, 1),
        ],
        ships: [8, 10],
        cards: &["Trade", "Leadership", "Technology"],
    },
    Plan {
        factions: &["arborec"],
        techs: &[
            "nm", "dxa", "lw2", "amd|sr", "cv2", "bio", "hm", "sar", "da",
        ],
        units: &[
            Emphasis("carrier", 1.4, 1.1, 3, 3),
            Emphasis("mech", 1.3, 1.2, 2, 3),
            Emphasis("flagship", 1.2, 1.5, 0, 1),
            Emphasis("dreadnought", 1.0, 1.4, 1, 2),
            Emphasis("fighter", 0.8, 0.8, 0, 0),
        ],
        ships: [7, 9],
        cards: &["Construction", "Leadership", "Warfare"],
    },
];

/// The plan for a faction alias, if it has one.
#[must_use]
pub fn plan_for(faction: &str) -> Option<&'static Plan> {
    if tuning::plans_v2()
        && let Some(plan) = PLANS_V2
            .iter()
            .find(|plan| plan.factions.contains(&faction))
    {
        return Some(plan);
    }
    PLANS.iter().find(|plan| plan.factions.contains(&faction))
}

fn seat_plan(seen: &Observed<'_>, me: &PlayerId) -> Option<&'static Plan> {
    if !tuning::faction_plans_enabled() {
        return None;
    }
    if tuning::plan_off() == 1
        && seen.seat(me).is_some_and(|seat| {
            matches!(
                seat.faction.as_str(),
                "ghost" | "hacan" | "empyrean" | "naaz" | "winnu"
            )
        })
    {
        return None;
    }
    seen.seat(me)
        .and_then(|seat| plan_for(seat.faction.as_str()))
}

/// The game phase a plan reads: `None` in the opening, else mid (`0`) or late (`1`).
fn phase(round: u32) -> Option<usize> {
    if round <= OPENING_ROUNDS {
        None
    } else if round < LATE_FROM {
        Some(0)
    } else {
        Some(1)
    }
}

// -- research ------------------------------------------------------------------------------------

/// Colours this seat owns technologies of, counted.
fn owned_colours(seen: &Observed<'_>, me: &PlayerId) -> BTreeMap<&'static str, usize> {
    let mut held = BTreeMap::new();
    if let Some(seat) = seen.seat(me) {
        for alias in seat.technologies {
            if let Some(colour) = ti4_engine::technology::colour_type(seen.content(), alias) {
                *held.entry(colour).or_insert(0) += 1;
            }
        }
    }
    held
}

/// Colours `alias`'s prerequisites still lack, given what the seat owns.
fn missing_colours(
    seen: &Observed<'_>,
    held: &BTreeMap<&'static str, usize>,
    alias: &str,
) -> Vec<&'static str> {
    ti4_engine::technology::prerequisites(seen.content(), &TechnologyId::new(alias))
        .into_iter()
        .filter(|(colour, need)| held.get(colour).copied().unwrap_or(0) < *need)
        .map(|(colour, _)| colour)
        .collect()
}

fn prerequisite_count(seen: &Observed<'_>, alias: &str) -> usize {
    ti4_engine::technology::prerequisites(seen.content(), &TechnologyId::new(alias))
        .values()
        .sum()
}

/// Research: the first entry of the faction's plan, then of [`GENERAL`], that is either offered
/// or has an offered step toward it (see the module docs). `None` when nothing on either list
/// is offered or approachable, which keeps the random pick.
#[must_use]
pub fn research_pick(choice: &Choice, seen: &Observed<'_>, me: &PlayerId) -> Option<usize> {
    let offered: BTreeMap<&str, usize> = choice
        .options
        .iter()
        .enumerate()
        .filter(|(_, o)| !o.is_decline())
        .map(|(i, o)| (o.id.as_str(), i))
        .collect();
    if offered.is_empty() {
        return None;
    }
    let owned = |alias: &str| {
        seen.seat(me)
            .is_some_and(|seat| seat.technologies.contains(&TechnologyId::new(alias)))
    };
    let own_list = seat_plan(seen, me).map_or(&[][..], |plan| plan.techs);
    let order: Vec<&str> = own_list.iter().chain(GENERAL).copied().collect();
    // Where an alias sits in the combined order: a step on the plan is a better step.
    let rank = |alias: &str| {
        order
            .iter()
            .position(|entry| entry.split('|').any(|a| a == alias))
            .unwrap_or(usize::MAX)
    };
    let held = owned_colours(seen, me);
    for entry in &order {
        let alternatives: Vec<&str> = entry.split('|').collect();
        if alternatives.iter().any(|a| owned(a)) {
            continue;
        }
        if let Some(index) = alternatives.iter().find_map(|a| offered.get(a)) {
            return Some(*index);
        }
        // Out of reach: an offered technology of a colour it still lacks.
        let missing: Vec<&'static str> = alternatives
            .iter()
            .flat_map(|a| missing_colours(seen, &held, a))
            .collect();
        if missing.is_empty() {
            continue; // not offered for another reason (another faction's, purged, ...)
        }
        let step = offered
            .iter()
            .filter(|(alias, _)| {
                ti4_engine::technology::colour_type(seen.content(), &TechnologyId::new(**alias))
                    .is_some_and(|colour| missing.contains(&colour))
            })
            .min_by_key(|(alias, _)| (rank(alias), prerequisite_count(seen, alias), **alias));
        if let Some((_, index)) = step {
            return Some(*index);
        }
    }
    None
}

/// The starting technology (bot version 3): the offered technology that comes first in the
/// faction's plan, then in [`GENERAL`]; a technology on neither list comes last, by id.
#[must_use]
pub fn starting_tech_pick(choice: &Choice, seen: &Observed<'_>, me: &PlayerId) -> Option<usize> {
    let own_list = seat_plan(seen, me).map_or(&[][..], |plan| plan.techs);
    let order: Vec<&str> = own_list.iter().chain(GENERAL).copied().collect();
    let rank = |alias: &str| {
        order
            .iter()
            .position(|entry| entry.split('|').any(|a| a == alias))
            .unwrap_or(usize::MAX)
    };
    choice
        .options
        .iter()
        .enumerate()
        .filter(|(_, o)| !o.is_decline())
        .min_by_key(|(_, o)| (rank(&o.id), o.id.as_str()))
        .map(|(index, _)| index)
}

/// Bot version 3: of technologies offered outside a research (a stolen or copied one), the index
/// of the best by the seat's own order (its plan, then [`GENERAL`]); one it already owns never.
/// `candidates` are `(option index, technology alias)`.
#[must_use]
pub fn best_technology(
    seen: &Observed<'_>,
    me: &PlayerId,
    candidates: &[(usize, &str)],
) -> Option<usize> {
    let seat = seen.seat(me)?;
    let own_list = seat_plan(seen, me).map_or(&[][..], |plan| plan.techs);
    let order: Vec<&str> = own_list.iter().chain(GENERAL).copied().collect();
    let rank = |alias: &str| {
        order
            .iter()
            .position(|entry| entry.split('|').any(|a| a == alias))
            .unwrap_or(order.len())
    };
    candidates
        .iter()
        .filter(|(_, alias)| !seat.technologies.contains(&TechnologyId::new(*alias)))
        .min_by_key(|(index, alias)| (rank(alias), *index))
        .map(|(index, _)| *index)
}

// -- production ----------------------------------------------------------------------------------

/// How many of the seat's units of `base` type are on the board (ships and ground forces).
fn count_of(seen: &Observed<'_>, me: &PlayerId, base: &str) -> usize {
    seen.board()
        .values()
        .flat_map(|system| {
            system
                .units
                .iter()
                .chain(system.planet_units.values().flatten())
        })
        .filter(|unit| &unit.owner == me)
        .filter(|unit| {
            ti4_content::units::unit_type(seen.content(), unit.type_id.as_str(), seen.sources())
                .is_some_and(|kind| kind.base_type() == base)
        })
        .count()
}

/// The plan's multiplier on a production option for `kind`: its phase weight, more while the
/// seat has fewer than the plan wants ([`SHORT_BONUS`]), a little more for an upgraded unit
/// ([`UPGRADED_BONUS`]). 1.0 in the opening rounds and for a faction without a plan.
#[must_use]
pub fn unit_weight(seen: &Observed<'_>, me: &PlayerId, kind: UnitType<'_>) -> f64 {
    let Some(phase) = phase(seen.round()) else {
        return 1.0;
    };
    let Some(plan) = seat_plan(seen, me) else {
        return 1.0;
    };
    let base = kind.base_type();
    let mut weight = 1.0;
    if let Some(emphasis) = plan.units.iter().find(|e| e.0 == base) {
        let (w, wanted) = if phase == 0 {
            (emphasis.1, emphasis.3)
        } else {
            (emphasis.2, emphasis.4)
        };
        weight *= w;
        if wanted > 0 && count_of(seen, me, base) < wanted {
            weight *= tuning::short_bonus();
        }
    }
    if kind.upgrades_from().is_some() {
        weight *= tuning::upgraded_bonus();
    }
    weight
}

/// Non-fighter ships the seat builds toward: [`OPENING_SHIPS`] in the opening or without a
/// plan, the plan's mid- or late-game fleet size after.
#[must_use]
pub fn ships_wanted(seen: &Observed<'_>, me: &PlayerId) -> usize {
    match (phase(seen.round()), seat_plan(seen, me)) {
        (Some(phase), Some(plan)) => plan.ships[phase],
        _ => OPENING_SHIPS,
    }
}

// -- strategy cards -------------------------------------------------------------------------------

/// From round 3, the first card of the plan's [`Plan::cards`] on offer; `None` in the opening,
/// without a plan or with none of its cards on offer.
#[must_use]
pub fn preferred_card(choice: &Choice, seen: &Observed<'_>, me: &PlayerId) -> Option<usize> {
    phase(seen.round())?;
    let plan = seat_plan(seen, me)?;
    plan.cards.iter().find_map(|name| {
        choice.options.iter().position(|o| {
            ti4_engine::strategy_cards::card_name(seen.content(), &o.id).as_deref() == Some(*name)
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ti4_content::ContentStore;
    use ti4_engine::choice::ChoiceOption;
    use ti4_engine::fixtures::{Hub, game, hub_from, plain_systems, put};
    use ti4_model::POK;
    use ti4_model::content_types::{ContentType, DEFAULT};
    use ti4_model::id::{FactionId, SystemId};
    use ti4_model::state::GameState;

    fn me() -> PlayerId {
        PlayerId::new("a")
    }

    fn seated(faction: &str, techs: &[&str], round: u32) -> (GameState, Hub) {
        let hub = hub_from(&plain_systems(7));
        let mut state = game(&["a", "b"]);
        state.round = round;
        let seat = state.player_mut(&me()).expect("seat");
        seat.faction = FactionId::new(faction);
        seat.technologies = techs.iter().map(|t| TechnologyId::new(*t)).collect();
        (state, hub)
    }

    fn observe<'a>(state: &'a GameState, hub: &'a Hub) -> Observed<'a> {
        Observed::new(state, ContentStore::embedded(), POK, Some(&hub.galaxy))
    }

    fn research(ids: &[&str]) -> Choice {
        Choice::new(
            me(),
            "research a technology",
            ids.iter()
                .map(|id| ChoiceOption::labelled(*id, "research", *id))
                .collect(),
        )
    }

    fn picked<'c>(choice: &'c Choice, seen: &Observed<'_>) -> Option<&'c str> {
        research_pick(choice, seen, &me()).map(|i| choice.options[i].id.as_str())
    }

    #[test]
    fn every_faction_in_the_corpus_has_exactly_one_plan() {
        let content = ContentStore::embedded();
        for record in content.records(ContentType::Factions) {
            let Some(alias) = record.text("alias") else {
                continue;
            };
            if alias == "neutral" {
                continue;
            }
            let plans = PLANS.iter().filter(|p| p.factions.contains(&alias)).count();
            assert_eq!(plans, 1, "{alias}");
        }
    }

    /// Every technology a plan names exists, is the faction's own or generic, is not a generic
    /// upgrade the faction replaces, and its prerequisites are met by the starting technologies,
    /// Gravity Drive and the entries before it.
    #[test]
    fn every_plan_names_real_technologies_in_an_order_that_can_be_researched() {
        let content = ContentStore::embedded();
        for plan in PLANS {
            for faction in plan.factions {
                let record = content
                    .records(ContentType::Factions)
                    .iter()
                    .find(|r| r.text("alias") == Some(faction))
                    .expect("faction");
                let mut held: BTreeMap<&str, usize> = BTreeMap::new();
                let take = |alias: &str, held: &mut BTreeMap<&str, usize>| {
                    if let Some(colour) =
                        ti4_engine::technology::colour_type(content, &TechnologyId::new(alias))
                    {
                        *held.entry(colour).or_insert(0) += 1;
                    }
                };
                for start in record.strings("startingTech") {
                    take(start, &mut held);
                }
                take("gd", &mut held);
                for entry in plan.techs {
                    let ok = entry.split('|').any(|alias| {
                        let id = TechnologyId::new(alias);
                        assert!(
                            content.get(ContentType::Technologies, alias).is_some(),
                            "{faction}: {alias} is not a technology"
                        );
                        let own = ti4_engine::technology::faction_of(content, &id);
                        assert!(
                            own.is_none_or(|f| f == *faction
                                || faction.starts_with(f)
                                || (*faction == "obsidian" && f == "firmament")),
                            "{faction}: {alias} belongs to {own:?}"
                        );
                        assert!(
                            !ti4_engine::technology::replaced_for_faction(content, faction, &id),
                            "{faction}: {alias} is replaced by its own upgrade"
                        );
                        ti4_engine::technology::prerequisites(content, &id)
                            .iter()
                            .all(|(colour, need)| held.get(colour).copied().unwrap_or(0) >= *need)
                    });
                    assert!(ok, "{faction}: {entry} is out of reach at its place");
                    let first = entry.split('|').next().expect("alias");
                    take(first, &mut held);
                }
            }
        }
        // The fallback order is real technologies too.
        for entry in GENERAL {
            for alias in entry.split('|') {
                assert!(
                    content.get(ContentType::Technologies, alias).is_some(),
                    "{alias}"
                );
            }
        }
    }

    #[test]
    fn every_plan_names_real_unit_types() {
        for plan in PLANS {
            for emphasis in plan.units {
                assert!(
                    ti4_engine::production::BUILDABLE.contains(&emphasis.0),
                    "{:?}: {}",
                    plan.factions,
                    emphasis.0
                );
            }
        }
        // `DEFAULT` sources know every base type named.
        assert!(!ti4_content::units::catalogue(ContentStore::embedded(), DEFAULT).is_empty());
    }

    #[test]
    fn research_follows_the_plan_in_order() {
        // L1Z1X start with Plasma Scoring and Neural Motivator; with Gravity Drive the plan
        // wants a second blue (Antimass Deflectors or Sling Relay) before Sarween Tools.
        let (state, hub) = seated("l1z1x", &["ps", "nm", "gd"], 3);
        let seen = observe(&state, &hub);
        assert_eq!(picked(&research(&["amd", "sar", "st"]), &seen), Some("amd"));
        assert_eq!(picked(&research(&["sar", "sr", "st"]), &seen), Some("sr"));
        let (state, hub) = seated("l1z1x", &["ps", "nm", "gd", "amd", "st"], 3);
        let seen = observe(&state, &hub);
        assert_eq!(
            picked(&research(&["da", "sar", "sdn2", "st"]), &seen),
            Some("sdn2"),
            "Super Dreadnought II once blue-blue-yellow is held"
        );
    }

    #[test]
    fn research_out_of_reach_takes_the_best_step_toward_the_plan() {
        // Sol want Advanced Carrier II (blue-blue). With only Antimass Deflectors (one blue) and
        // no Gravity Drive offered, a blue step is better than anything else on offer, and Sling
        // Relay (one blue prerequisite, met) is the step.
        let (state, hub) = seated("sol", &["amd", "nm"], 3);
        let seen = observe(&state, &hub);
        assert_eq!(
            picked(&research(&["sr", "dxa", "ps"]), &seen),
            Some("sr"),
            "a step toward Advanced Carrier II comes before Dacxive Animators"
        );
        // With no blue on offer it moves on to the next entry it can take.
        assert_eq!(picked(&research(&["dxa", "ps"]), &seen), Some("dxa"));
    }

    #[test]
    fn research_falls_back_to_the_general_order_and_never_invents_an_option() {
        // The Nekro have no list: the general order decides.
        let (state, hub) = seated("nekro", &["dxa"], 4);
        let seen = observe(&state, &hub);
        assert_eq!(picked(&research(&["x89", "st", "sar"]), &seen), Some("sar"));
        assert_eq!(picked(&research(&[]), &seen), None);
        let unknown = research(&["not-a-tech"]);
        assert_eq!(picked(&unknown, &seen), None);
    }

    #[test]
    fn unit_weights_apply_from_round_three_and_favour_the_plan() {
        let (mut state, hub) = seated("l1z1x", &[], 2);
        let content = ContentStore::embedded();
        let dreadnought =
            ti4_content::units::unit_type(content, "l1z1x_dreadnought", POK).expect("unit");
        let carrier = ti4_content::units::unit_type(content, "carrier", POK).expect("unit");
        let seen = observe(&state, &hub);
        assert!(
            (unit_weight(&seen, &me(), dreadnought) - 1.0).abs() < 1e-9,
            "opening"
        );
        assert_eq!(ships_wanted(&seen, &me()), OPENING_SHIPS);
        state.round = 3;
        let seen = observe(&state, &hub);
        let short = unit_weight(&seen, &me(), dreadnought);
        assert!(
            short > unit_weight(&seen, &me(), carrier),
            "dreadnoughts first"
        );
        assert_eq!(ships_wanted(&seen, &me()), 9);
        // Four dreadnoughts on the board: the commander count is met, the bonus goes.
        let system = SystemId::new(hub.outer[0].clone());
        put(&mut state, &system, "l1z1x_dreadnought", &me(), 4);
        let seen = observe(&state, &hub);
        assert!(unit_weight(&seen, &me(), dreadnought) < short);
        state.round = 5;
        let seen = observe(&state, &hub);
        assert_eq!(ships_wanted(&seen, &me()), 11);
    }

    #[test]
    fn a_draft_left_open_takes_the_plans_card_from_round_three() {
        let draft = Choice::new(
            me(),
            "choose a strategy card",
            ["pok2diplomacy", "pok7technology", "pok6warfare"]
                .iter()
                .map(|id| ChoiceOption::labelled(*id, "strategy_card", *id))
                .collect(),
        );
        let (state, hub) = seated("jolnar", &[], 2);
        assert_eq!(preferred_card(&draft, &observe(&state, &hub), &me()), None);
        let (state, hub) = seated("jolnar", &[], 3);
        assert_eq!(
            preferred_card(&draft, &observe(&state, &hub), &me()),
            Some(1)
        );
        let (state, hub) = seated("l1z1x", &[], 4);
        assert_eq!(
            preferred_card(&draft, &observe(&state, &hub), &me()),
            Some(2)
        );
        // Every name a plan lists is a strategy card's name.
        let names: Vec<String> = ContentStore::embedded()
            .records(ContentType::StrategyCards)
            .iter()
            .filter_map(|r| r.text("name").map(str::to_owned))
            .collect();
        for plan in PLANS {
            for card in plan.cards {
                assert!(names.iter().any(|n| n == card), "{card}");
            }
        }
    }

    #[test]
    fn an_upgraded_unit_weighs_a_little_more() {
        let (state, hub) = seated("winnu", &[], 3);
        let seen = observe(&state, &hub);
        let content = ContentStore::embedded();
        let plain = ti4_content::units::unit_type(content, "cruiser", POK).expect("unit");
        let upgraded = ti4_content::units::unit_type(content, "cruiser2", POK).expect("unit");
        let ratio = unit_weight(&seen, &me(), upgraded) / unit_weight(&seen, &me(), plain);
        assert!((ratio - UPGRADED_BONUS).abs() < 1e-9);
    }
}
