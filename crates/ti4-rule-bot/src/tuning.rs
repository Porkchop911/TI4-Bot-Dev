//! Tunable numbers of the rule bot, for optimisation runs.
//!
//! The defaults are the crate's constants, so without an override the bot decides exactly as it
//! always did. An override is **per thread** ([`set`], [`with`]): a self-play worker installs
//! the configuration of the bot it plays before each game, which lets a baseline and a candidate
//! share one process. The server and the browser never call [`set`] and so always run the
//! defaults. Keys are the lower-case names of the constants (see [`KEYS`]); [`Tuning::set_key`]
//! parses `key=value`.

use std::cell::Cell;

macro_rules! tuning {
    ($( $field:ident : $ty:ty = $default:expr, $doc:literal; )*) => {
        /// The tunable numbers. `Default` is the shipped bot.
        #[derive(Debug, Clone, Copy, PartialEq)]
        pub struct Tuning { $( #[doc = $doc] pub $field: $ty, )* }

        impl Default for Tuning {
            fn default() -> Self { Self { $( $field: $default, )* } }
        }

        /// Every key accepted by [`Tuning::set_key`], with its default and meaning.
        pub const KEYS: &[(&str, &str)] = &[ $( (stringify!($field), $doc), )* ];

        impl Tuning {
            /// Set one value from text.
            ///
            /// # Errors
            /// A message for an unknown key or a value that does not parse.
            pub fn set_key(&mut self, key: &str, value: &str) -> Result<(), String> {
                if key == "bot_version" {
                    // Every number of that version (later keys still override).
                    let version: u32 =
                        value.trim().parse().map_err(|e| format!("{key}={value}: {e}"))?;
                    *self = for_version(version);
                    return Ok(());
                }
                match key {
                    $( stringify!($field) => {
                        self.$field = value.trim().parse().map_err(|e| format!("{key}={value}: {e}"))?;
                        Ok(())
                    } )*
                    _ => Err(format!("unknown tuning key '{key}' (known: {})",
                        KEYS.iter().map(|k| k.0).collect::<Vec<_>>().join(", "))),
                }
            }

            /// `(key, value)` of every field, as text.
            #[must_use]
            pub fn entries(&self) -> Vec<(&'static str, String)> {
                vec![ $( (stringify!($field), self.$field.to_string()), )* ]
            }
        }

        $( #[doc = $doc] #[must_use] pub fn $field() -> $ty { CURRENT.with(|c| c.get().$field) } )*
    };
}

tuning! {
    win_margin: f64 = crate::WIN_MARGIN, "ln strength ratio a fight must reach to be taken";
    retreat_margin: f64 = crate::RETREAT_MARGIN, "matchup below which a space combat is retreated from";
    mecatol_bonus: f64 = crate::MECATOL_BONUS, "extra worth of Mecatol Rex while the custodians token is on it";
    activation_threshold: f64 = crate::ACTIVATION_THRESHOLD, "least worth of a system to steer activation";
    eager_worth: f64 = crate::EAGER_WORTH, "worth that always gets a tactical action before the strategic one";
    accept_net: f64 = crate::ACCEPT_NET, "least engine-estimated value for accepting a trade";
    min_idle_ground: i64 = crate::MIN_IDLE_GROUND, "ground forces kept waiting at docks";
    rich_resources: f64 = crate::RICH_RESOURCES, "spendable resources from which production is slot-limited";
    fleet_tokens_wanted: i32 = crate::FLEET_TOKENS_WANTED, "fleet pool size kept before tokens go elsewhere";
    expansion_rounds: u32 = crate::EXPANSION_ROUNDS, "opening rounds played for planets above all";
    plenty_resources: i64 = crate::PLENTY_RESOURCES, "spendable resources from which a dock is worth a token";
    enough_tokens: i32 = crate::ENOUGH_TOKENS, "tactic plus strategy tokens beyond which Leadership buys no more";
    objective_round: u32 = crate::objectives::OBJECTIVE_ROUND, "round from which the objective layer plays";
    complete_value: f64 = crate::objectives::COMPLETE_VALUE, "worth per point of an objective completed by an action";
    progress_value: f64 = crate::objectives::PROGRESS_VALUE, "worth of progress toward an objective";
    deny_value: f64 = crate::objectives::DENY_VALUE, "worth of denying a rival about to win";
    deny_within: i32 = 4, "points from winning at which a rival is denied";
    objectives_enabled: bool = true, "false: the objective layer (round 3 on) is switched off for every seat";
    faction_plans_enabled: bool = true, "false: the faction plans (research order, production weights, preferred card) are off";
    short_bonus: f64 = crate::faction_plans::SHORT_BONUS, "production weight of a unit the faction plan is short of";
    upgraded_bonus: f64 = crate::faction_plans::UPGRADED_BONUS, "production weight of an upgraded unit";
    vote_rule: bool = true, "v2: agenda votes, Politics' speaker and agenda placement by rule (answers.rs)";
    casualty_rule: bool = true, "v2: sustain damage whenever possible, least valuable casualty first";
    ground_rule: bool = false, "v2: troops only for the free lift of its ships (glut at lift + 2)";
    empty_fight_rule: bool = false, "v2: rounds 1-2, no fight that takes no planet away from its own systems";
    mecatol_rival_bonus: f64 = 4.0, "v2: extra worth of Mecatol Rex while a rival holds it";
    planner_weight: f64 = 1.0, "v2: production weight per point of an objective within planner_gap that more of a unit meets";
    planner_gap: f64 = 5.0, "v2 (v4 default 5; v1-v3: 3): steps short within which the planner weighs an objective";
    progress_gap: f64 = 2.0, "steps short within which progress toward an objective is worth activating for";
    nebula_margin: f64 = 0.0, "v2: extra matchup needed to attack a fleet in a nebula (defender +1)";
    home_guard: bool = false, "v2: no warship leaves home when the rest could not hold it against a fleet a rival can bring";
    plans_v2: bool = true, "v2: reworked plans of the weakest factions (yssaril, l1z1x, nekro, hacan, arborec)";
    rift_caution: bool = false, "v2: outside a fight, no move that exits a gravity rift (a 30% loss per rift)";
    sftt_rule: bool = false, "v2: never give away a Support for the Throne (a victory point) in a trade";
    end_turn_rule: bool = false, "v2: end the turn when nothing else is wanted; always use a legendary planet's end-of-turn ability";
    secondary_rule: bool = false, "v2: from round 3, Diplomacy/Trade/Construction/Politics secondaries only with a spare token and where they pay";
    misc_rule: bool = true, "v2: ready the best planet, retreat to own systems, Trade replenishes nobody";
    starting_tech_rule: bool = true, "v3: the starting technology of Winnu, Argent and the Keleres is chosen by the faction plan (v2: at random)";
    r1_trade: bool = false, "v3: round 1 Trade for the factions whose guides open with it (nekro, ghost, cabal, empyrean, titans)";
    r1_tech: bool = true, "v3: round 1 Technology first for arborec, naalu, l1z1x";
    r1_leadership: bool = false, "v3: round 1 Leadership high for muaat, mahact, cabal";
    r1_diplomacy: bool = false, "v3: round 1 Diplomacy high for xxcha";
    winnu_mecatol: bool = false, "v3: the Winnu take Mecatol Rex without the six influence (Blood Ties), worth more (Reclamation)";
    letnev_armada: bool = false, "v3: the Letnev keep two fewer fleet tokens (Armada of Fire)";
    lift_rule: f64 = 0.0, "v3: weight slope for transports while more ground forces idle at docks than ships can lift";
    adjacent_bonus: f64 = 0.0, "v3: worth per own system next to a system whose planets it can take";
    tech_hunt: f64 = 0.0, "v3: the Nekro: worth per rival technology it lacks in a fight it takes (Technological Singularity)";
    tech_pick_rule: bool = true, "v3: a stolen or copied technology by the seat's order; the Nekro always use Technological Singularity";
    nekro_tech: bool = false, "v3 (off again in v4): the Nekro draft Technology first in every round (its research is three command tokens)";
    plan_off: i64 = 0, "v3: 1 = no faction plan for ghost, hacan, empyrean, naaz, winnu (they win least in self-play)";
    empyrean_rule: bool = false, "v3: the Empyrean allow Aetherpassage only to themselves";
    home_last_ship: bool = false, "v3: from round 3 the last warship does not leave home while a rival fleet could reach it";
    r1_tech2: bool = false, "v3: round 1 Technology first for winnu, sardakk, argent (no starting technology in this engine)";
    influence_early: f64 = 0.0, "v3: extra worth per influence of a planet in rounds 1-3 while the custodians token stands";
    early_margin: f64 = 0.0, "v3: extra matchup needed for a fight in rounds 1-2";
    mecatol_early: f64 = 0.0, "v3: extra worth of Mecatol Rex in rounds 2-3 while the custodians token is on it";
    ground_margin: f64 = 0.0, "v3: extra ground forces for an invasion, as a share of the defenders' hit points";
    bw_enabled: bool = true, "v4: backward planning (backward/): plans per objective, weighed with Mecatol/Imperial, steers activation and the Imperial draft";
    bw_round: u32 = 3, "v4: first round of backward planning";
    bw_threshold: f64 = 0.35, "v4: expected VP the best plan must reach, else the default logic plays unchanged";
    bw_steer: f64 = 6.0, "v4: activation worth per expected VP of a chosen plan, for the systems on its route";
    bw_likely: f64 = 0.5, "v4: probability from which a plan counts as likely (Imperial synergy)";
    bw_imperial_bonus: f64 = 0.4, "v4: expected VP added to each likely public plan while two are likely (Imperial scores the second)";
    bw_payment_discount: f64 = 0.5, "v4: share of a spend-N objective's value dropped (easy to score unsteered)";
    bw_token_cost: f64 = 0.15, "v4: expected VP charged per command token a plan uses";
    bw_resource_cost: f64 = 0.05, "v4: expected VP charged per resource a plan spends";
    bw_mecatol: bool = true, "v4: the Mecatol Rex / Imperial plan competes with the objective plans";
    bw_mecatol_weight: f64 = 1.0, "v4: scale of the Mecatol plan's points";
    bw_prep_build: bool = true, "v4: prep layer, build: a production (dock, Construction, Warfare secondary) adds what the activation lacks, then act";
    bw_prep_warfare: bool = true, "v4: prep layer, Warfare move: Warfare's extra activation gathers ships beside Mecatol Rex, then act";
    bw_prep_tech_range: bool = true, "v4: prep layer, technology range: a movement technology brings the ships in reach, then act";
    bw_prep_tech_strength: bool = true, "v4: prep layer, technology strength: a combat/upgrade technology wins the Mecatol fight, then act";
    bw_build_weight: f64 = 3.0, "v4: production weight added to a unit type a chosen backward plan wants built";
    bw_card_weight: f64 = 8.0, "v4: draft score per expected VP of a strategy card a chosen backward plan wants";
    bw_fleet_shape: bool = false, "v4: planners of the fleet-shape group: fleet in one system, flagship placement, ground pile (with bw_enabled)";
    bw_territory: bool = true, "v4: planners of the territory group (with bw_enabled)";
    bw_fleet: bool = true, "v4: planners of the fleet group (with bw_enabled)";
    bw_payment: bool = true, "v4: planners of the spend group (with bw_enabled)";
    bw_technology: bool = true, "v4: planners of the technology group (with bw_enabled)";
    bw_combat: bool = true, "v4: planners of the combat and other group (with bw_enabled)";
    bw_combat_abilities: bool = true, "v4 (B): planners of the ability secrets (uf, baf, fwp, mew; with bw_enabled)";
    bw_agenda: bool = true, "v4 (B): the agenda secrets' votes and planner (dtd, dp; with bw_enabled)";
    bw_holdings: bool = true, "v4 (B): the holding secrets' hooks and planner (fsn, dhw, sb; with bw_enabled)";
    bw_ttfd: bool = true, "v4 (B): a PDS on or beside Mecatol Rex for Turn Their Fleets to Dust (with bw_enabled)";
    bw_production: bool = true, "v4 (B): planners of the production secrets (gamf, mtm, pem; with bw_enabled)";
    mecatol_guard: bool = false, "v5 candidate (off in v1-v4): the warships on a held Mecatol Rex stay while they deter the fleet a rival can bring (no activation counts them as movable, no move takes them away)";
    mecatol_imperial_priority: bool = false, "v5 candidate (off in v1-v4): a seat that holds Mecatol Rex (or could take it) drafts an unclaimed Imperial before anything else, and plays it at once when it holds it";
    agenda_value: bool = false, "v5 candidate: every agenda outcome valued for the seat (agenda_vote.rs); a small margin is no opinion";
    agenda_budget: bool = false, "v5 candidate: cast only the votes that make the outcome likely, keep planets for the second agenda";
    agenda_riders: bool = false, "v5 candidate: rider cards played only when the outcome is predictable and the seat's votes are not needed";
    agenda_rival_weight: f64 = 0.5, "v5 candidate: weight of the rivals' change in an outcome's value (agenda_value)";
    agenda_min_margin: f64 = 1.5, "v5 candidate: value points the best outcome must lead by, else no opinion (agenda_value)";
    agenda_rival_share: f64 = 0.6, "v5 candidate: chance that a rival without an opinion votes against the seat";
    agenda_second_prior: f64 = 2.0, "v5 candidate: assumed stake of an unknown second agenda, in value points (agenda_budget)";
    rider_min_p: f64 = 0.7, "v5 candidate: least probability that the predicted outcome wins for a rider to be played (agenda_riders)";
    land_all: bool = false, "v5 (off in v1-v4): the landing step commits every ground force aboard to a planet of the system (never leaves them in space)";
}

/// The rule bot's behaviour version that new games play. Version 1 is the bot before the
/// `v2:` knobs, version 2 the bot before the `v3:` knobs, version 3 adds them, version 4 adds the `v4:` knobs (backward planning) and changes `planner_gap` and `nekro_tech`; a saved offline game records the
/// version it was played with (`ti4_wasm::host_setup::Setup::bot_version`) and is resumed with it.
pub const BOT_VERSION: u32 = 4;

/// The highest version [`for_version`] knows: version 5 is the staging table of the next bot
/// release ([`v5`]); a game can name it (`bot_version` 5, `--bot bot_version=5`) but new games
/// still play [`BOT_VERSION`].
pub const MAX_BOT_VERSION: u32 = 5;

/// The tuning of bot version `version` (1 to 4; anything else: the current one).
///
/// Version 1 is written out in full (every field, no `..`), so a new knob cannot slip into it
/// with a changed default: the compiler asks for its version-1 value. Version 2 is version 1 plus
/// the `v2:` knobs, again named field by field; it takes every `v3:` knob from version 1 (off).
#[must_use]
pub fn for_version(version: u32) -> Tuning {
    match version {
        1 => v1(),
        2 => v2(),
        3 => v3(),
        5 => v5(),
        _ => Tuning::default(),
    }
}

/// Version 5, the **staging table for the next bot release**: version 4 plus the knobs decided
/// for it so far. Nothing ships from it yet ([`BOT_VERSION`] stays 4). Members: `land_all` (the
/// landing step commits every ground force aboard) and the three agenda-voting knobs
/// `agenda_value`, `agenda_budget`, `agenda_riders` (docs/BOT_AGENDA_VOTING.md). Other candidate
/// knobs are NOT in it yet, pending decisions: `mecatol_guard` and `mecatol_imperial_priority`
/// (branch fb-muaat-warsun).
#[must_use]
pub fn v5() -> Tuning {
    Tuning {
        land_all: true,
        agenda_value: true,
        agenda_budget: true,
        agenda_riders: true,
        ..Tuning::default()
    }
}

/// Version 3: version 2 plus the `v3:` knobs (`tech_pick_rule`, `r1_tech`, `nekro_tech`). The `v4:`
/// knobs (backward planning, `planner_gap` 5, no `nekro_tech`) are named here with their old values
/// so a later default cannot change a version-3 game.
fn v3() -> Tuning {
    Tuning {
        starting_tech_rule: true,
        tech_pick_rule: true,
        r1_tech: true,
        nekro_tech: true,
        planner_gap: 3.0,
        bw_enabled: false,
        bw_prep_build: false,
        bw_prep_warfare: false,
        bw_prep_tech_range: false,
        bw_prep_tech_strength: false,
        bw_combat_abilities: false,
        bw_agenda: false,
        bw_holdings: false,
        bw_ttfd: false,
        bw_production: false,
        ..v2()
    }
}

fn v2() -> Tuning {
    Tuning {
        deny_within: 4,
        vote_rule: true,
        casualty_rule: true,
        mecatol_rival_bonus: 4.0,
        planner_weight: 1.0,
        plans_v2: true,
        misc_rule: true,
        ..v1()
    }
}

fn v1() -> Tuning {
    Tuning {
        win_margin: crate::WIN_MARGIN,
        retreat_margin: crate::RETREAT_MARGIN,
        mecatol_bonus: crate::MECATOL_BONUS,
        activation_threshold: crate::ACTIVATION_THRESHOLD,
        eager_worth: crate::EAGER_WORTH,
        accept_net: crate::ACCEPT_NET,
        min_idle_ground: crate::MIN_IDLE_GROUND,
        rich_resources: crate::RICH_RESOURCES,
        fleet_tokens_wanted: crate::FLEET_TOKENS_WANTED,
        expansion_rounds: crate::EXPANSION_ROUNDS,
        plenty_resources: crate::PLENTY_RESOURCES,
        enough_tokens: crate::ENOUGH_TOKENS,
        objective_round: crate::objectives::OBJECTIVE_ROUND,
        complete_value: crate::objectives::COMPLETE_VALUE,
        progress_value: crate::objectives::PROGRESS_VALUE,
        deny_value: crate::objectives::DENY_VALUE,
        deny_within: crate::objectives::DENY_WITHIN,
        objectives_enabled: true,
        faction_plans_enabled: true,
        short_bonus: crate::faction_plans::SHORT_BONUS,
        upgraded_bonus: crate::faction_plans::UPGRADED_BONUS,
        vote_rule: false,
        casualty_rule: false,
        ground_rule: false,
        empty_fight_rule: false,
        mecatol_rival_bonus: 0.0,
        planner_weight: 0.0,
        planner_gap: 3.0,
        progress_gap: 2.0,
        nebula_margin: 0.0,
        home_guard: false,
        plans_v2: false,
        rift_caution: false,
        sftt_rule: false,
        end_turn_rule: false,
        secondary_rule: false,
        misc_rule: false,
        starting_tech_rule: false,
        r1_trade: false,
        r1_tech: false,
        r1_leadership: false,
        r1_diplomacy: false,
        winnu_mecatol: false,
        letnev_armada: false,
        lift_rule: 0.0,
        adjacent_bonus: 0.0,
        tech_hunt: 0.0,
        tech_pick_rule: false,
        nekro_tech: false,
        plan_off: 0,
        empyrean_rule: false,
        home_last_ship: false,
        r1_tech2: false,
        influence_early: 0.0,
        early_margin: 0.0,
        mecatol_early: 0.0,
        ground_margin: 0.0,
        bw_enabled: false,
        bw_round: 3,
        bw_threshold: 0.35,
        bw_steer: 6.0,
        bw_likely: 0.5,
        bw_imperial_bonus: 0.4,
        bw_payment_discount: 0.5,
        bw_token_cost: 0.15,
        bw_resource_cost: 0.05,
        bw_mecatol: true,
        bw_mecatol_weight: 1.0,
        bw_prep_build: false,
        bw_prep_warfare: false,
        bw_prep_tech_range: false,
        bw_prep_tech_strength: false,
        bw_build_weight: 3.0,
        bw_card_weight: 8.0,
        bw_fleet_shape: false,
        bw_territory: true,
        bw_fleet: true,
        bw_payment: true,
        bw_technology: true,
        bw_combat: true,
        bw_combat_abilities: false,
        bw_agenda: false,
        bw_holdings: false,
        bw_ttfd: false,
        bw_production: false,
        mecatol_guard: false,
        mecatol_imperial_priority: false,
        agenda_value: false,
        agenda_budget: false,
        agenda_riders: false,
        agenda_rival_weight: 0.5,
        agenda_min_margin: 1.5,
        agenda_rival_share: 0.6,
        agenda_second_prior: 2.0,
        rider_min_p: 0.7,
        land_all: false,
    }
}

thread_local! {
    static CURRENT: Cell<Tuning> = Cell::new(Tuning::default());
}

/// Install `tuning` for this thread.
pub fn set(tuning: Tuning) {
    CURRENT.with(|c| c.set(tuning));
}

/// The tuning of this thread.
#[must_use]
pub fn get() -> Tuning {
    CURRENT.with(Cell::get)
}

/// Run `f` with `tuning` installed, then restore the previous one.
pub fn with<T>(tuning: Tuning, f: impl FnOnce() -> T) -> T {
    let before = get();
    set(tuning);
    let out = f();
    set(before);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_the_constants_and_keys_parse() {
        assert!((win_margin() - crate::WIN_MARGIN).abs() < f64::EPSILON);
        let mut t = Tuning::default();
        t.set_key("win_margin", "0.9").unwrap();
        assert!(t.set_key("nope", "1").is_err());
        with(t, || assert!((win_margin() - 0.9).abs() < f64::EPSILON));
        assert!((win_margin() - crate::WIN_MARGIN).abs() < f64::EPSILON);
    }

    #[test]
    fn bot_version_key_sets_that_version() {
        let mut t = Tuning::default();
        t.set_key("bot_version", "1").unwrap();
        assert_eq!(t, for_version(1));
        assert_eq!(for_version(BOT_VERSION), Tuning::default());
        assert!(!for_version(1).vote_rule && !for_version(1).casualty_rule);
        assert!(
            for_version(2).vote_rule && !for_version(2).r1_tech && !for_version(2).tech_pick_rule
        );
        assert!(
            for_version(3).r1_tech
                && for_version(3).tech_pick_rule
                && for_version(3).nekro_tech
                && !for_version(2).nekro_tech
        );
        assert!(
            for_version(3).nekro_tech
                && !for_version(3).bw_enabled
                && (for_version(3).planner_gap - 3.0).abs() < f64::EPSILON
                && !for_version(3).bw_ttfd
        );
        let v4 = for_version(4);
        assert_eq!(v4, Tuning::default());
        assert!(!v4.land_all && for_version(5).land_all);
        assert!(!v4.agenda_value && !v4.agenda_budget && !v4.agenda_riders);
        assert!(
            for_version(5).agenda_value
                && for_version(5).agenda_budget
                && for_version(5).agenda_riders
        );
        assert_eq!(
            Tuning {
                land_all: false,
                agenda_value: false,
                agenda_budget: false,
                agenda_riders: false,
                ..for_version(5)
            },
            v4
        );
        assert!(!v4.nekro_tech && v4.tech_pick_rule && v4.r1_tech);
        assert!((v4.planner_gap - 5.0).abs() < f64::EPSILON);
        assert!(v4.bw_enabled && v4.bw_prep_build && v4.bw_prep_warfare && v4.bw_agenda);
        assert!(v4.bw_prep_tech_range && v4.bw_prep_tech_strength && v4.bw_combat_abilities);
        assert!(v4.bw_holdings && v4.bw_ttfd && v4.bw_production);
        assert!(!v4.bw_fleet_shape);
    }
}
