//! Battle inputs and the frozen space-combat predictor (ARENA-002).
//!
//! One encoding serves both the arena trainer and live play, so the predictor never sees a
//! different vector in a game than it was trained on. The predictor itself is a plain-Rust
//! forward pass: the live policy crate must not depend on the tensor library.
//!
//! Scope: the six factions the bots play, with and without upgrades, space combat only, both
//! sides' space cannon before combat, the declared sustain-first casualty order. A side
//! containing anything else is unsupported rather than approximated.
//!
//! Feature versions. A predictor records the version it was trained on and is fed exactly that:
//!
//! - **1** — ships, damage and dice shift per side; three outcome probabilities. A destination
//!   with guns on its planets is unsupported, and one without enemy ships is not a fight.
//! - **2** — adds guns per side (PDS, PDS II, the Xxcha mech on planets there, and guns next door
//!   whose cards reach), so a move into a covered system with no enemy ships is a fight too; and
//!   adds each ship type's expected survival.
//! - **3** — version 2's space network unchanged, plus a ground network for invasion commits:
//!   ground forces, damage and dice shift per side, the defender's PDS (space cannon defense) and
//!   the L1Z1X invader's Harrow ships; outcome and survival per ground-force type.
//! - **4** — the space input gains a flag for a fight already under way (space cannon and the
//!   round-1 barrage behind it), and retreat announcements carry the odds of staying in.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use serde::{Deserialize, Serialize};
use ti4_engine::choice::{Choice, ChoiceOption, Observed};
use ti4_model::id::{PlayerId, SystemId};

/// The version new predictors are trained on.
pub const FEATURE_VERSION: u32 = 4;

/// Every version this build can feed.
pub const SUPPORTED_VERSIONS: [u32; 4] = [1, 2, 3, 4];

/// Units the encoding knows, in slot order. Never reorder; append under a new version.
pub const UNIT_IDS: [&str; 21] = [
    "carrier",
    "carrier2",
    "cruiser",
    "cruiser2",
    "destroyer",
    "destroyer2",
    "dreadnought",
    "dreadnought2",
    "fighter",
    "fighter2",
    "hacan_flagship",
    "jolnar_flagship",
    "l1z1x_dreadnought",
    "l1z1x_dreadnought2",
    "l1z1x_flagship",
    "letnev_flagship",
    "sol_carrier",
    "sol_carrier2",
    "sol_flagship",
    "warsun",
    "xxcha_flagship",
];

/// Ground forces version 3 knows, in slot order.
pub const GROUND_IDS: [&str; 10] = [
    "hacan_mech",
    "infantry",
    "infantry2",
    "jolnar_mech",
    "l1z1x_mech",
    "letnev_mech",
    "sol_infantry",
    "sol_infantry2",
    "sol_mech",
    "xxcha_mech",
];

/// Structures that fire space cannon defense, in slot order.
pub const DEFENSE_GUN_IDS: [&str; 2] = ["pds", "pds2"];

/// Ships whose bombardment L1Z1X's Harrow repeats, in slot order.
pub const HARROW_IDS: [&str; 3] = ["l1z1x_dreadnought", "l1z1x_dreadnought2", "warsun"];

/// One side's ground block: forces, damaged forces, dice shift, defense guns, Harrow ships.
pub const GROUND_SIDE_WIDTH: usize =
    2 * GROUND_IDS.len() + 1 + DEFENSE_GUN_IDS.len() + HARROW_IDS.len();

/// The ground network's input: invader block then defender block.
pub const GROUND_INPUT_WIDTH: usize = 2 * GROUND_SIDE_WIDTH;

/// The ground network's output: take / hold / (unused) logits, then survival per ground slot per
/// side.
pub const GROUND_OUTPUT_WIDTH: usize = 3 + 2 * GROUND_IDS.len();

/// Guns version 2 knows: units that fire space cannon before combat and take no part in it.
pub const GUN_IDS: [&str; 4] = ["pds", "pds2", "xxcha_mech", "xxcha_flagship"];

/// Guns whose cards let them fire into an adjacent system.
const REACHING_GUNS: [&str; 3] = ["pds2", "xxcha_mech", "xxcha_flagship"];

/// Factions covered, with their shift to every combat roll.
pub const FACTIONS: [(&str, i64); 6] = [
    ("sol", 0),
    ("letnev", 0),
    ("xxcha", 0),
    ("hacan", 0),
    ("jolnar", -1),
    ("l1z1x", 0),
];

/// Width of one side's block: counts, damaged counts, dice shift, and from version 2 the guns.
#[must_use]
pub const fn side_width(version: u32) -> usize {
    let ships = 2 * UNIT_IDS.len() + 1;
    if version >= 2 {
        ships + GUN_IDS.len()
    } else {
        ships
    }
}

/// Width of a whole input: attacker block then defender block.
#[must_use]
pub const fn input_width(version: u32) -> usize {
    let flag = if version >= 4 { 1 } else { 0 };
    2 * side_width(version) + flag
}

/// Width of a predictor's output: three outcome logits, and from version 2 a survival logit per
/// ship type per side.
#[must_use]
pub const fn output_width(version: u32) -> usize {
    if version >= 2 {
        3 + 2 * UNIT_IDS.len()
    } else {
        3
    }
}

/// Input width of the current version.
pub const INPUT_WIDTH: usize = input_width(FEATURE_VERSION);

/// Output width of the current version.
pub const OUTPUT_WIDTH: usize = output_width(FEATURE_VERSION);

/// One side of a space battle: ships by unit id, how many of each start damaged, the guns that
/// fire for it before combat, and the faction's shift to every combat roll.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BattleSide {
    pub units: Vec<(String, usize)>,
    pub damaged: Vec<(String, usize)>,
    pub guns: Vec<(String, usize)>,
    pub modifier: i64,
}

/// Why a fight cannot be encoded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unsupported {
    /// A ship outside `UNIT_IDS`.
    Unit(String),
    /// More damaged ships of an id than there are ships of it.
    Damage(String),
    /// A faction outside `FACTIONS`, whose abilities the predictor never saw.
    Faction(String),
    /// Ships of more than one other player at the destination.
    Opponents,
    /// The destination is an anomaly, whose combat effects the predictor never saw.
    Anomaly,
    /// A gun the predictor's version does not cover.
    Guns,
    /// A galvanized ship, whose modifiers the predictor never saw.
    Galvanized,
    /// The acting seat would fight with no ships.
    EmptyFleet,
}

/// What a movement option means for the fight at its destination.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BattleQuery {
    /// Not a movement decision, or nothing at the destination that fights.
    NotApplicable,
    /// A fight follows, but outside what the predictor covers. The reason is public.
    Unsupported(Unsupported),
    /// The fight as it would stand if movement ended with this option taken.
    Supported {
        attacker: BattleSide,
        defender: BattleSide,
    },
}

/// Space-cannon units by owner: `(owner, unit id)` for each.
fn guns_in(
    seen: &Observed<'_>,
    system: &SystemId,
    planets_only: bool,
    reaching_only: bool,
) -> Vec<(PlayerId, String)> {
    let (content, sources) = (seen.content(), seen.sources());
    let board = seen.system(system);
    let space = board.units.iter().filter(|_| !planets_only);
    board
        .planet_units
        .values()
        .flatten()
        .chain(space)
        .filter(|unit| {
            ti4_content::units::unit_type(content, unit.type_id.as_str(), sources)
                .is_some_and(|kind| kind.has_space_cannon())
        })
        .filter(|unit| !reaching_only || deep_space_cannon(seen, unit.type_id.as_str()))
        .map(|unit| (unit.owner.clone(), unit.type_id.to_string()))
        .collect()
}

/// Whether a unit's space cannon reaches adjacent systems (PDS II, the Xxcha mech and flagship,
/// and any unit outside this encoding that carries the same clause).
fn deep_space_cannon(seen: &Observed<'_>, id: &str) -> bool {
    REACHING_GUNS.contains(&id)
        || ti4_content::units::unit_type(seen.content(), id, seen.sources())
            .and_then(|kind| kind.record().text("ability"))
            .is_some_and(|ability| {
                let ability = ability.to_ascii_lowercase();
                ability.contains("space cannon against ships that are")
                    && ability.contains("adjacent")
            })
}

/// The battle a movement option leads to, from public information only, as `version` sees it.
///
/// The acting seat is the attacker. For a ship move the attacking fleet is the seat's ships
/// already at the destination plus the moved ship; for "finish movement" it is those ships alone.
/// Cargo still to be loaded and ships still to be moved are not guessed at.
#[must_use]
pub fn movement_query(
    seen: &Observed<'_>,
    choice: &Choice,
    option: &ChoiceOption,
    player: &PlayerId,
    version: u32,
) -> BattleQuery {
    if choice.prompt != "movement" {
        return BattleQuery::NotApplicable;
    }
    let moving = if option.kind == "move" {
        let Some(unit) = option
            .payload
            .get("unit")
            .and_then(serde_json::Value::as_str)
        else {
            return BattleQuery::NotApplicable;
        };
        let damaged = option
            .payload
            .get("damaged")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        Some((unit, damaged))
    } else if option.id == "done_moving" {
        None
    } else {
        return BattleQuery::NotApplicable;
    };
    let Some(active) = seen.active_system() else {
        return BattleQuery::NotApplicable;
    };
    let (content, sources) = (seen.content(), seen.sources());
    let here = seen.system(active);
    let kind = |id: &str| ti4_content::units::unit_type(content, id, sources);
    let is_ship = |id: &str| kind(id).is_some_and(|unit| unit.is_ship());

    let opponents: BTreeSet<&PlayerId> = here
        .units
        .iter()
        .filter(|unit| &unit.owner != player && is_ship(unit.type_id.as_str()))
        .map(|unit| &unit.owner)
        .collect();

    // Guns: on the planets here, and (from version 2) next door where the card reaches.
    let mut guns = guns_in(seen, active, true, false);
    if version >= 2
        && let Some(galaxy) = seen.galaxy()
    {
        for neighbour in galaxy.adjacent(active.as_str()) {
            guns.extend(guns_in(seen, &SystemId::new(neighbour), false, true));
        }
    }
    let enemy_guns = guns.iter().any(|(owner, _)| owner != player);

    let enemy = match opponents.first() {
        Some(enemy) => (*enemy).clone(),
        None if version >= 2 && enemy_guns => guns
            .iter()
            .find(|(owner, _)| owner != player)
            .map(|(owner, _)| owner.clone())
            .expect("an enemy gun exists"),
        None => return BattleQuery::NotApplicable,
    };
    if opponents.len() > 1 {
        return BattleQuery::Unsupported(Unsupported::Opponents);
    }
    if ti4_content::galaxy::all_systems(content, sources)
        .get(active.as_str())
        .is_some_and(ti4_content::galaxy::System::is_anomaly)
    {
        return BattleQuery::Unsupported(Unsupported::Anomaly);
    }
    if version < 2 && !guns.is_empty() {
        return BattleQuery::Unsupported(Unsupported::Guns);
    }
    if guns.iter().any(|(_, id)| !GUN_IDS.contains(&id.as_str())) {
        return BattleQuery::Unsupported(Unsupported::Guns);
    }

    let side = |owner: &PlayerId,
                extra: Option<(&str, bool)>,
                gunners: &dyn Fn(&PlayerId) -> bool|
     -> Result<BattleSide, Unsupported> {
        let faction = seen
            .seat(owner)
            .map(|seat| seat.faction.as_str().to_owned())
            .unwrap_or_default();
        let modifier = FACTIONS
            .iter()
            .find(|(known, _)| *known == faction)
            .map(|(_, shift)| *shift)
            .ok_or_else(|| Unsupported::Faction(faction.clone()))?;
        let mut units: BTreeMap<String, usize> = BTreeMap::new();
        let mut damaged: BTreeMap<String, usize> = BTreeMap::new();
        for unit in here
            .units
            .iter()
            .filter(|unit| &unit.owner == owner && is_ship(unit.type_id.as_str()))
        {
            if unit.galvanized {
                return Err(Unsupported::Galvanized);
            }
            *units.entry(unit.type_id.to_string()).or_default() += 1;
            if unit.sustained_damage {
                *damaged.entry(unit.type_id.to_string()).or_default() += 1;
            }
        }
        if let Some((id, hurt)) = extra {
            *units.entry(id.to_owned()).or_default() += 1;
            if hurt {
                *damaged.entry(id.to_owned()).or_default() += 1;
            }
        }
        let mut side_guns: BTreeMap<String, usize> = BTreeMap::new();
        for (gunner, id) in &guns {
            if gunners(gunner) {
                *side_guns.entry(id.clone()).or_default() += 1;
            }
        }
        Ok(BattleSide {
            units: units.into_iter().collect(),
            damaged: damaged.into_iter().collect(),
            guns: side_guns.into_iter().collect(),
            modifier,
        })
    };
    // Every other player's guns fire at the attacker, so all of them join the defender.
    let built = side(player, moving, &|gunner| gunner == player).and_then(|attacker| {
        if attacker.units.is_empty() {
            return Err(Unsupported::EmptyFleet);
        }
        let defender = side(&enemy, None, &|gunner| gunner != player)?;
        encode(version, &attacker, &defender)?;
        Ok((attacker, defender))
    });
    match built {
        Ok((attacker, defender)) => BattleQuery::Supported { attacker, defender },
        Err(reason) => BattleQuery::Unsupported(reason),
    }
}

fn slot(id: &str) -> Option<usize> {
    UNIT_IDS.iter().position(|known| *known == id)
}

fn scale(id: &str, count: usize) -> f32 {
    #[expect(clippy::cast_precision_loss, reason = "counts are small")]
    let count = count as f32;
    if id.starts_with("fighter") {
        count / 16.0
    } else {
        count / 8.0
    }
}

impl BattleSide {
    fn encode(&self, version: u32, out: &mut [f32]) -> Result<(), Unsupported> {
        for (id, count) in &self.units {
            let at = slot(id).ok_or_else(|| Unsupported::Unit(id.clone()))?;
            out[at] += scale(id, *count);
        }
        for (id, count) in &self.damaged {
            let at = slot(id).ok_or_else(|| Unsupported::Unit(id.clone()))?;
            let fielded: usize = self
                .units
                .iter()
                .filter(|(unit, _)| unit == id)
                .map(|(_, n)| n)
                .sum();
            if *count > fielded {
                return Err(Unsupported::Damage(id.clone()));
            }
            out[UNIT_IDS.len() + at] += scale(id, *count);
        }
        #[expect(clippy::cast_precision_loss, reason = "a dice shift of -1, 0 or 1")]
        let shift = self.modifier as f32;
        out[2 * UNIT_IDS.len()] = shift;
        if version < 2 {
            return if self.guns.is_empty() {
                Ok(())
            } else {
                Err(Unsupported::Guns)
            };
        }
        for (id, count) in &self.guns {
            let at = GUN_IDS
                .iter()
                .position(|known| known == id)
                .ok_or(Unsupported::Guns)?;
            #[expect(clippy::cast_precision_loss, reason = "counts are small")]
            let value = *count as f32 / 4.0;
            out[2 * UNIT_IDS.len() + 1 + at] += value;
        }
        Ok(())
    }
}

/// The input `version` expects for a fight between `attacker` (the active player) and
/// `defender`. Version 1 has no fight without defending ships.
///
/// # Errors
///
/// A side carries a ship, damage or gun the version does not cover.
pub fn encode(
    version: u32,
    attacker: &BattleSide,
    defender: &BattleSide,
) -> Result<Vec<f32>, Unsupported> {
    encode_at(version, attacker, defender, false)
}

/// As [`encode`], for a fight already under way when `in_progress` (version 4).
///
/// # Errors
///
/// As [`encode`]; and a fight under way needs version 4 and defending ships.
pub fn encode_at(
    version: u32,
    attacker: &BattleSide,
    defender: &BattleSide,
    in_progress: bool,
) -> Result<Vec<f32>, Unsupported> {
    if attacker.units.is_empty() || (version < 2 && defender.units.is_empty()) {
        return Err(Unsupported::EmptyFleet);
    }
    if in_progress && (version < 4 || defender.units.is_empty()) {
        return Err(Unsupported::EmptyFleet);
    }
    let width = side_width(version);
    let mut out = vec![0.0; input_width(version)];
    attacker.encode(version, &mut out[..width])?;
    defender.encode(version, &mut out[width..2 * width])?;
    if version >= 4 && in_progress {
        out[2 * width] = 1.0;
    }
    Ok(out)
}

/// One side of a ground combat on one planet.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GroundBattleSide {
    pub forces: Vec<(String, usize)>,
    pub damaged: Vec<(String, usize)>,
    /// The defender's space cannon defense structures.
    pub guns: Vec<(String, usize)>,
    /// The invader's ships whose bombardment Harrow repeats; empty unless it applies.
    pub harrow: Vec<(String, usize)>,
    pub modifier: i64,
}

fn place(list: &[&str], id: &str, value: f32, out: &mut [f32]) -> Result<(), Unsupported> {
    let at = list
        .iter()
        .position(|known| *known == id)
        .ok_or_else(|| Unsupported::Unit(id.to_owned()))?;
    out[at] += value;
    Ok(())
}

impl GroundBattleSide {
    fn encode(&self, out: &mut [f32]) -> Result<(), Unsupported> {
        let n = GROUND_IDS.len();
        #[expect(clippy::cast_precision_loss, reason = "counts are small")]
        let scaled = |count: usize, by: f32| count as f32 / by;
        for (id, count) in &self.forces {
            place(&GROUND_IDS, id, scaled(*count, 8.0), &mut out[..n])?;
        }
        for (id, count) in &self.damaged {
            let fielded: usize = self
                .forces
                .iter()
                .filter(|(unit, _)| unit == id)
                .map(|(_, k)| k)
                .sum();
            if *count > fielded {
                return Err(Unsupported::Damage(id.clone()));
            }
            place(&GROUND_IDS, id, scaled(*count, 4.0), &mut out[n..2 * n])?;
        }
        #[expect(clippy::cast_precision_loss, reason = "a dice shift of -1, 0 or 1")]
        let shift = self.modifier as f32;
        out[2 * n] = shift;
        let guns = 2 * n + 1;
        for (id, count) in &self.guns {
            place(
                &DEFENSE_GUN_IDS,
                id,
                scaled(*count, 4.0),
                &mut out[guns..guns + DEFENSE_GUN_IDS.len()],
            )
            .map_err(|_| Unsupported::Guns)?;
        }
        let harrow = guns + DEFENSE_GUN_IDS.len();
        for (id, count) in &self.harrow {
            place(&HARROW_IDS, id, scaled(*count, 4.0), &mut out[harrow..])?;
        }
        Ok(())
    }
}

/// The ground network's input for an invasion of one planet.
///
/// # Errors
///
/// The invader lands nothing, or a side carries a unit the encoding does not cover.
pub fn encode_ground(
    attacker: &GroundBattleSide,
    defender: &GroundBattleSide,
) -> Result<Vec<f32>, Unsupported> {
    if attacker.forces.is_empty() {
        return Err(Unsupported::EmptyFleet);
    }
    let mut out = vec![0.0; GROUND_INPUT_WIDTH];
    attacker.encode(&mut out[..GROUND_SIDE_WIDTH])?;
    defender.encode(&mut out[GROUND_SIDE_WIDTH..])?;
    Ok(out)
}

/// What a predictor expects of a fight.
#[derive(Debug, Clone, PartialEq)]
pub struct BattlePrediction {
    pub attacker_wins: f32,
    pub defender_wins: f32,
    pub mutual_destruction: f32,
    /// Expected surviving fraction of each `UNIT_IDS` slot, attacker's; empty before version 2.
    pub attacker_survival: Vec<f32>,
    /// The same for the defender.
    pub defender_survival: Vec<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Layer {
    inputs: usize,
    outputs: usize,
    /// Row-major, `outputs` rows of `inputs`.
    weight: Vec<f32>,
    bias: Vec<f32>,
}

/// A trained predictor: dense layers with ReLU between them; softmax over the three outcomes and,
/// from version 2, a sigmoid per survival slot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BattlePredictor {
    pub feature_version: u32,
    /// Identity of the labels it was trained on, e.g. the casualty policy and rule choices.
    pub continuation: String,
    unit_ids: Vec<String>,
    layers: Vec<Layer>,
    /// The ground network, from version 3.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    ground_layers: Vec<Layer>,
}

fn forward(layers: &[Layer], input: &[f32]) -> Vec<f32> {
    let mut current: Vec<f32> = input.to_vec();
    for (index, layer) in layers.iter().enumerate() {
        let mut next = layer.bias.clone();
        for (row, value) in next.iter_mut().enumerate() {
            let weights = &layer.weight[row * layer.inputs..(row + 1) * layer.inputs];
            *value += weights
                .iter()
                .zip(&current)
                .map(|(w, x)| w * x)
                .sum::<f32>();
        }
        if index + 1 < layers.len() {
            for value in &mut next {
                *value = value.max(0.0);
            }
        }
        current = next;
    }
    current
}

fn chains(layers: &[Layer], input: usize, output: usize) -> bool {
    let mut width = input;
    for layer in layers {
        if layer.inputs != width
            || layer.weight.len() != layer.inputs * layer.outputs
            || layer.bias.len() != layer.outputs
        {
            return false;
        }
        width = layer.outputs;
    }
    !layers.is_empty() && width == output
}

fn to_layers(layers: Vec<(usize, usize, Vec<f32>, Vec<f32>)>) -> Vec<Layer> {
    layers
        .into_iter()
        .map(|(inputs, outputs, weight, bias)| Layer {
            inputs,
            outputs,
            weight,
            bias,
        })
        .collect()
}

/// Softmax over the first three outputs, sigmoid over the rest split evenly by side.
fn read_out(out: &[f32]) -> BattlePrediction {
    let top = out[..3].iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let exp: Vec<f32> = out[..3].iter().map(|v| (v - top).exp()).collect();
    let total: f32 = exp.iter().sum();
    let survival: Vec<f32> = out[3..].iter().map(|v| 1.0 / (1.0 + (-v).exp())).collect();
    let (attacker_survival, defender_survival) = survival.split_at(survival.len() / 2);
    BattlePrediction {
        attacker_wins: exp[0] / total,
        defender_wins: exp[1] / total,
        mutual_destruction: exp[2] / total,
        attacker_survival: attacker_survival.to_vec(),
        defender_survival: defender_survival.to_vec(),
    }
}

/// Why a predictor file was refused.
#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    #[error("predictor file: {0}")]
    Json(#[from] serde_json::Error),
    #[error("predictor built for feature version {found}; this build feeds {SUPPORTED_VERSIONS:?}")]
    Version { found: u32 },
    #[error("predictor unit list does not match this build's")]
    Units,
    #[error("predictor layers do not chain from its version's input width to its output width")]
    Shape,
}

impl BattlePredictor {
    /// Assemble from `(inputs, outputs, weight, bias)` layers, checking shapes.
    ///
    /// # Errors
    ///
    /// An unsupported version, layers that do not chain from the version's input width to its
    /// output width, or a buffer of the wrong length.
    pub fn new(
        version: u32,
        continuation: impl Into<String>,
        layers: Vec<(usize, usize, Vec<f32>, Vec<f32>)>,
    ) -> Result<Self, LoadError> {
        let predictor = Self {
            feature_version: version,
            continuation: continuation.into(),
            unit_ids: UNIT_IDS.iter().map(|id| (*id).to_owned()).collect(),
            layers: to_layers(layers),
            ground_layers: Vec::new(),
        };
        predictor.validate()?;
        Ok(predictor)
    }

    /// A predictor of any version from its space and (version 3 on) ground layers.
    ///
    /// # Errors
    ///
    /// An unsupported version, or layers that do not chain for it.
    pub fn assemble(
        version: u32,
        continuation: impl Into<String>,
        space: Vec<(usize, usize, Vec<f32>, Vec<f32>)>,
        ground: Vec<(usize, usize, Vec<f32>, Vec<f32>)>,
    ) -> Result<Self, LoadError> {
        let predictor = Self {
            feature_version: version,
            continuation: continuation.into(),
            unit_ids: UNIT_IDS.iter().map(|id| (*id).to_owned()).collect(),
            layers: to_layers(space),
            ground_layers: to_layers(ground),
        };
        predictor.validate()?;
        Ok(predictor)
    }

    /// The ground layers as `(inputs, outputs, weight, bias)`, to carry into a later version.
    #[must_use]
    pub fn ground_layer_parts(&self) -> Vec<(usize, usize, Vec<f32>, Vec<f32>)> {
        self.ground_layers
            .iter()
            .map(|layer| {
                (
                    layer.inputs,
                    layer.outputs,
                    layer.weight.clone(),
                    layer.bias.clone(),
                )
            })
            .collect()
    }

    /// A version-3 predictor: this version-2 space network plus a ground network.
    ///
    /// # Errors
    ///
    /// This predictor is not version 2, or the ground layers do not chain from
    /// `GROUND_INPUT_WIDTH` to `GROUND_OUTPUT_WIDTH`.
    pub fn with_ground(
        mut self,
        continuation: impl Into<String>,
        layers: Vec<(usize, usize, Vec<f32>, Vec<f32>)>,
    ) -> Result<Self, LoadError> {
        if self.feature_version != 2 {
            return Err(LoadError::Version {
                found: self.feature_version,
            });
        }
        self.feature_version = 3;
        self.continuation = format!("{}; ground: {}", self.continuation, continuation.into());
        self.ground_layers = to_layers(layers);
        self.validate()?;
        Ok(self)
    }

    /// Whether this predictor covers invasions.
    #[must_use]
    pub fn has_ground(&self) -> bool {
        !self.ground_layers.is_empty()
    }

    /// What this predictor expects of an encoded invasion; `None` before version 3.
    ///
    /// # Panics
    ///
    /// If the input is not `GROUND_INPUT_WIDTH` wide.
    #[must_use]
    pub fn predict_ground(&self, input: &[f32]) -> Option<BattlePrediction> {
        if !self.has_ground() {
            return None;
        }
        assert_eq!(input.len(), GROUND_INPUT_WIDTH, "ground input width");
        Some(read_out(&forward(&self.ground_layers, input)))
    }

    /// The ground network's raw outputs, for checking an export.
    #[must_use]
    pub fn ground_logits(&self, input: &[f32]) -> Vec<f32> {
        forward(&self.ground_layers, input)
    }

    fn validate(&self) -> Result<(), LoadError> {
        if !SUPPORTED_VERSIONS.contains(&self.feature_version) {
            return Err(LoadError::Version {
                found: self.feature_version,
            });
        }
        if self.unit_ids.iter().map(String::as_str).ne(UNIT_IDS) {
            return Err(LoadError::Units);
        }
        let version = self.feature_version;
        if !chains(&self.layers, input_width(version), output_width(version)) {
            return Err(LoadError::Shape);
        }
        let ground_ok = if version >= 3 {
            chains(&self.ground_layers, GROUND_INPUT_WIDTH, GROUND_OUTPUT_WIDTH)
        } else {
            self.ground_layers.is_empty()
        };
        if !ground_ok {
            return Err(LoadError::Shape);
        }
        Ok(())
    }

    /// # Errors
    ///
    /// Malformed JSON, or a predictor for an encoding this build cannot feed.
    pub fn from_json(text: &str) -> Result<Self, LoadError> {
        let predictor: Self = serde_json::from_str(text)?;
        predictor.validate()?;
        Ok(predictor)
    }

    /// # Errors
    ///
    /// Serialisation failure.
    pub fn to_json(&self) -> Result<String, LoadError> {
        Ok(serde_json::to_string(self)?)
    }

    /// The raw outputs for an encoded input of this predictor's version.
    ///
    /// # Panics
    ///
    /// If the input is not `input_width(self.feature_version)` wide.
    #[must_use]
    pub fn logits(&self, input: &[f32]) -> Vec<f32> {
        assert_eq!(
            input.len(),
            input_width(self.feature_version),
            "input width"
        );
        forward(&self.layers, input)
    }

    /// What this predictor expects of an encoded fight.
    ///
    /// # Panics
    ///
    /// If the input is not `input_width(self.feature_version)` wide.
    #[must_use]
    pub fn predict(&self, input: &[f32]) -> BattlePrediction {
        let mut prediction = read_out(&self.logits(input));
        if output_width(self.feature_version) == 3 {
            prediction.attacker_survival.clear();
            prediction.defender_survival.clear();
        }
        prediction
    }
}

/// The closed battle facts of version 1, under the `action-plan` family.
pub const FACT_NAMES_V1: [&str; 6] = [
    "action-plan:battle-fight",
    "action-plan:battle-unsupported",
    "action-plan:battle-win",
    "action-plan:battle-loss",
    "action-plan:battle-mutual",
    "action-plan:battle-win-change",
];

/// Version 2 adds the expected resource cost each side loses, in tens of resources.
pub const FACT_NAMES_V2: [&str; 8] = [
    "action-plan:battle-fight",
    "action-plan:battle-unsupported",
    "action-plan:battle-win",
    "action-plan:battle-loss",
    "action-plan:battle-mutual",
    "action-plan:battle-win-change",
    "action-plan:battle-own-cost-lost",
    "action-plan:battle-enemy-cost-lost",
];

/// Version 3 adds the invasion facts on ground-force commits.
pub const FACT_NAMES_V3: [&str; 14] = [
    "action-plan:battle-fight",
    "action-plan:battle-unsupported",
    "action-plan:battle-win",
    "action-plan:battle-loss",
    "action-plan:battle-mutual",
    "action-plan:battle-win-change",
    "action-plan:battle-own-cost-lost",
    "action-plan:battle-enemy-cost-lost",
    "action-plan:ground-fight",
    "action-plan:ground-unsupported",
    "action-plan:ground-take",
    "action-plan:ground-take-change",
    "action-plan:ground-own-cost-lost",
    "action-plan:ground-enemy-cost-lost",
];

/// Version 4 adds the odds of staying in, on retreat announcements.
pub const FACT_NAMES_V4: [&str; 18] = [
    "action-plan:battle-fight",
    "action-plan:battle-unsupported",
    "action-plan:battle-win",
    "action-plan:battle-loss",
    "action-plan:battle-mutual",
    "action-plan:battle-win-change",
    "action-plan:battle-own-cost-lost",
    "action-plan:battle-enemy-cost-lost",
    "action-plan:ground-fight",
    "action-plan:ground-unsupported",
    "action-plan:ground-take",
    "action-plan:ground-take-change",
    "action-plan:ground-own-cost-lost",
    "action-plan:ground-enemy-cost-lost",
    "action-plan:battle-stay-win",
    "action-plan:battle-stay-loss",
    "action-plan:battle-stay-own-cost-lost",
    "action-plan:battle-stay-enemy-cost-lost",
];

/// The facts a predictor of `version` emits; the arena migration appends exactly these.
#[must_use]
pub fn fact_names(version: u32) -> &'static [&'static str] {
    match version {
        0 | 1 => &FACT_NAMES_V1,
        2 => &FACT_NAMES_V2,
        3 => &FACT_NAMES_V3,
        _ => &FACT_NAMES_V4,
    }
}

/// The fight a retreat announcement is about: the ships in the combat system, with the active
/// player as attacker, as it stands under way. `None` when the choice is not an announcement or
/// the fight is outside the predictor's cover.
#[must_use]
pub fn retreat_query(
    seen: &Observed<'_>,
    choice: &Choice,
    player: &PlayerId,
) -> Option<(BattleSide, BattleSide, bool)> {
    let announcing = choice
        .context
        .as_ref()
        .is_some_and(|context| context.subtype == "announce_retreat");
    if !announcing {
        return None;
    }
    let system = choice.options.iter().find_map(|option| {
        option
            .payload
            .get("system")
            .and_then(serde_json::Value::as_str)
    })?;
    let system = SystemId::new(system);
    let active = seen.active_player()?.clone();
    let (content, sources) = (seen.content(), seen.sources());
    let here = seen.system(&system);
    let is_ship = |id: &str| {
        ti4_content::units::unit_type(content, id, sources).is_some_and(|unit| unit.is_ship())
    };
    let owners: BTreeSet<&PlayerId> = here
        .units
        .iter()
        .filter(|unit| is_ship(unit.type_id.as_str()))
        .map(|unit| &unit.owner)
        .collect();
    if owners.len() != 2 || !owners.contains(player) || !owners.contains(&active) {
        return None;
    }
    let enemy = owners.iter().find(|owner| **owner != &active)?;
    if ti4_content::galaxy::all_systems(content, sources)
        .get(system.as_str())
        .is_some_and(ti4_content::galaxy::System::is_anomaly)
    {
        return None;
    }
    let side = |owner: &PlayerId| -> Option<BattleSide> {
        let faction = seen.seat(owner)?.faction.as_str().to_owned();
        let modifier = FACTIONS
            .iter()
            .find(|(known, _)| *known == faction)
            .map(|(_, shift)| *shift)?;
        let mut units: BTreeMap<String, usize> = BTreeMap::new();
        let mut damaged: BTreeMap<String, usize> = BTreeMap::new();
        for unit in here
            .units
            .iter()
            .filter(|unit| &unit.owner == owner && is_ship(unit.type_id.as_str()))
        {
            if unit.galvanized {
                return None;
            }
            *units.entry(unit.type_id.to_string()).or_default() += 1;
            if unit.sustained_damage {
                *damaged.entry(unit.type_id.to_string()).or_default() += 1;
            }
        }
        Some(BattleSide {
            units: units.into_iter().collect(),
            damaged: damaged.into_iter().collect(),
            guns: Vec::new(),
            modifier,
        })
    };
    let attacker = side(&active)?;
    let defender = side(enemy)?;
    Some((attacker, defender, player == &active))
}

/// Staying-in facts for a retreat announcement, the same on every option.
fn retreat_facts(
    seen: &Observed<'_>,
    choice: &Choice,
    player: &PlayerId,
    predictor: &BattlePredictor,
) -> Vec<Vec<(&'static str, f64)>> {
    let names = &FACT_NAMES_V4[14..];
    let facts: Vec<(&'static str, f64)> = retreat_query(seen, choice, player)
        .and_then(|(attacker, defender, acting_attacks)| {
            let input = encode_at(predictor.feature_version, &attacker, &defender, true).ok()?;
            let p = predictor.predict(&input);
            let (own, enemy, own_survival, enemy_survival) = if acting_attacks {
                (
                    &attacker,
                    &defender,
                    &p.attacker_survival,
                    &p.defender_survival,
                )
            } else {
                (
                    &defender,
                    &attacker,
                    &p.defender_survival,
                    &p.attacker_survival,
                )
            };
            let (win, loss) = if acting_attacks {
                (p.attacker_wins, p.defender_wins)
            } else {
                (p.defender_wins, p.attacker_wins)
            };
            Some(vec![
                (names[0], f64::from(win)),
                (names[1], f64::from(loss)),
                (names[2], cost_lost(seen, own, own_survival)),
                (names[3], cost_lost(seen, enemy, enemy_survival)),
            ])
        })
        .unwrap_or_default();
    choice.options.iter().map(|_| facts.clone()).collect()
}

/// Expected resource cost a side loses, in tens of resources.
fn cost_lost(seen: &Observed<'_>, side: &BattleSide, survival: &[f32]) -> f64 {
    side.units
        .iter()
        .filter_map(|(id, count)| {
            let at = slot(id)?;
            let cost = ti4_content::units::unit_type(seen.content(), id, seen.sources())
                .map_or(0.0, |kind| kind.cost());
            #[expect(clippy::cast_precision_loss, reason = "counts are small")]
            let count = *count as f64;
            Some(cost * count * (1.0 - f64::from(survival[at])))
        })
        .sum::<f64>()
        / 10.0
}

/// What landing a ground force means for the fight on its planet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GroundQuery {
    /// Not a commit option, or nobody to fight on the planet.
    NotApplicable,
    /// A fight follows, but outside what the predictor covers.
    Unsupported(Unsupported),
    /// The invasion as it would stand with this unit landed, and without it.
    Supported {
        attacker: GroundBattleSide,
        defender: GroundBattleSide,
        /// The forces already committed there, when there are any.
        baseline: Option<GroundBattleSide>,
    },
}

/// The ground combat a commit option leads to, from public information only.
///
/// Bombardment has already happened when forces are committed, so it is not predicted; space
/// cannon defense, the combat rounds and Harrow are.
#[must_use]
pub fn invasion_query(
    seen: &Observed<'_>,
    option: &ChoiceOption,
    player: &PlayerId,
) -> GroundQuery {
    if option.kind != "commit" {
        return GroundQuery::NotApplicable;
    }
    let (Some(planet), Some(unit)) = (
        option
            .payload
            .get("planet")
            .and_then(serde_json::Value::as_str),
        option
            .payload
            .get("unit")
            .and_then(serde_json::Value::as_str),
    ) else {
        return GroundQuery::NotApplicable;
    };
    let Some(active) = seen.active_system() else {
        return GroundQuery::NotApplicable;
    };
    let (content, sources) = (seen.content(), seen.sources());
    let here = seen.system(active);
    let planet_id = ti4_model::id::PlanetId::new(planet);
    let kind = |id: &str| ti4_content::units::unit_type(content, id, sources);
    let is_ground = |id: &str| kind(id).is_some_and(|k| k.is_ground_force());
    let standing = here.on_planet(&planet_id);

    let defenders: BTreeSet<&PlayerId> = standing
        .iter()
        .filter(|u| &u.owner != player && is_ground(u.type_id.as_str()))
        .map(|u| &u.owner)
        .collect();
    if defenders.len() > 1 {
        return GroundQuery::Unsupported(Unsupported::Opponents);
    }
    let enemy = defenders.first().map(|e| (*e).clone()).or_else(|| {
        here.planet_control
            .get(&planet_id)
            .filter(|holder| *holder != player)
            .cloned()
    });
    let Some(enemy) = enemy else {
        return GroundQuery::NotApplicable;
    };
    if ti4_content::galaxy::all_systems(content, sources)
        .get(active.as_str())
        .is_some_and(ti4_content::galaxy::System::is_scar)
    {
        return GroundQuery::Unsupported(Unsupported::Anomaly);
    }
    let faction_of = |owner: &PlayerId| -> Result<(String, i64), Unsupported> {
        let faction = seen
            .seat(owner)
            .map(|seat| seat.faction.as_str().to_owned())
            .unwrap_or_default();
        FACTIONS
            .iter()
            .find(|(known, _)| *known == faction)
            .map(|(_, shift)| (faction.clone(), *shift))
            .ok_or(Unsupported::Faction(faction))
    };
    let tally =
        |owner: &PlayerId| -> Result<(Vec<(String, usize)>, Vec<(String, usize)>), Unsupported> {
            let mut forces: BTreeMap<String, usize> = BTreeMap::new();
            let mut damaged: BTreeMap<String, usize> = BTreeMap::new();
            for u in standing
                .iter()
                .filter(|u| &u.owner == owner && is_ground(u.type_id.as_str()))
            {
                if u.galvanized {
                    return Err(Unsupported::Galvanized);
                }
                *forces.entry(u.type_id.to_string()).or_default() += 1;
                if u.sustained_damage {
                    *damaged.entry(u.type_id.to_string()).or_default() += 1;
                }
            }
            Ok((forces.into_iter().collect(), damaged.into_iter().collect()))
        };
    let built = (|| -> Result<GroundQuery, Unsupported> {
        let (_, own_shift) = faction_of(player)?;
        let (enemy_faction, enemy_shift) = faction_of(&enemy)?;
        let _ = enemy_faction;
        let (committed, committed_damage) = tally(player)?;
        let (forces, damaged) = tally(&enemy)?;
        let mut guns: BTreeMap<String, usize> = BTreeMap::new();
        for u in standing.iter().filter(|u| u.owner == enemy) {
            let id = u.type_id.as_str();
            if kind(id).is_some_and(|k| k.has_space_cannon() && !k.is_ground_force()) {
                if !DEFENSE_GUN_IDS.contains(&id) {
                    return Err(Unsupported::Guns);
                }
                *guns.entry(id.to_owned()).or_default() += 1;
            }
        }
        // Harrow: L1Z1X only, and only where bombardment is allowed. A planetary shield stops it
        // unless a war sun is present.
        let (own_faction, _) = faction_of(player)?;
        let mut harrow: BTreeMap<String, usize> = BTreeMap::new();
        if own_faction == "l1z1x" {
            let shielded = standing.iter().any(|u| {
                u.owner != *player && kind(u.type_id.as_str()).is_some_and(|k| k.planetary_shield())
            });
            let war_sun = here.units.iter().any(|u| {
                &u.owner == player
                    && kind(u.type_id.as_str()).is_some_and(|k| k.base_type() == "warsun")
            });
            if !shielded || war_sun {
                for u in here.units.iter().filter(|u| &u.owner == player) {
                    if HARROW_IDS.contains(&u.type_id.as_str()) {
                        *harrow.entry(u.type_id.to_string()).or_default() += 1;
                    }
                }
            }
        }
        let harrow: Vec<(String, usize)> = harrow.into_iter().collect();
        let side = |forces: Vec<(String, usize)>, damaged: Vec<(String, usize)>| GroundBattleSide {
            forces,
            damaged,
            guns: Vec::new(),
            harrow: harrow.clone(),
            modifier: own_shift,
        };
        let baseline =
            (!committed.is_empty()).then(|| side(committed.clone(), committed_damage.clone()));
        let mut landed: BTreeMap<String, usize> = committed.into_iter().collect();
        *landed.entry(unit.to_owned()).or_default() += 1;
        let mut landed_damage: BTreeMap<String, usize> = committed_damage.into_iter().collect();
        if option.label.contains("(damaged)") {
            *landed_damage.entry(unit.to_owned()).or_default() += 1;
        }
        let attacker = side(
            landed.into_iter().collect(),
            landed_damage.into_iter().collect(),
        );
        let defender = GroundBattleSide {
            forces,
            damaged,
            guns: guns.into_iter().collect(),
            harrow: Vec::new(),
            modifier: enemy_shift,
        };
        encode_ground(&attacker, &defender)?;
        Ok(GroundQuery::Supported {
            attacker,
            defender,
            baseline,
        })
    })();
    built.unwrap_or_else(GroundQuery::Unsupported)
}

/// Expected resource cost a ground side loses, in tens of resources.
fn ground_cost_lost(seen: &Observed<'_>, side: &GroundBattleSide, survival: &[f32]) -> f64 {
    side.forces
        .iter()
        .filter_map(|(id, count)| {
            let at = GROUND_IDS.iter().position(|known| known == id)?;
            let cost = ti4_content::units::unit_type(seen.content(), id, seen.sources())
                .map_or(0.0, |kind| kind.cost());
            #[expect(clippy::cast_precision_loss, reason = "counts are small")]
            let count = *count as f64;
            Some(cost * count * (1.0 - f64::from(survival[at])))
        })
        .sum::<f64>()
        / 10.0
}

/// Invasion facts for every option of a commit decision.
fn ground_facts(
    seen: &Observed<'_>,
    choice: &Choice,
    player: &PlayerId,
    predictor: &BattlePredictor,
) -> Vec<Vec<(&'static str, f64)>> {
    let names = &FACT_NAMES_V3[8..];
    choice
        .options
        .iter()
        .map(|option| {
            let mut facts = Vec::new();
            match invasion_query(seen, option, player) {
                GroundQuery::NotApplicable => {}
                GroundQuery::Unsupported(_) => {
                    facts.push((names[0], 1.0));
                    facts.push((names[1], 1.0));
                }
                GroundQuery::Supported {
                    attacker,
                    defender,
                    baseline,
                } => {
                    facts.push((names[0], 1.0));
                    let predict = |a: &GroundBattleSide| {
                        encode_ground(a, &defender)
                            .ok()
                            .and_then(|input| predictor.predict_ground(&input))
                    };
                    if let Some(p) = predict(&attacker) {
                        let before = baseline
                            .as_ref()
                            .and_then(predict)
                            .map_or(0.0, |b| b.attacker_wins);
                        facts.push((names[2], f64::from(p.attacker_wins)));
                        facts.push((names[3], f64::from(p.attacker_wins - before)));
                        facts.push((
                            names[4],
                            ground_cost_lost(seen, &attacker, &p.attacker_survival),
                        ));
                        facts.push((
                            names[5],
                            ground_cost_lost(seen, &defender, &p.defender_survival),
                        ));
                    }
                }
            }
            facts
        })
        .collect()
}

/// Battle facts for every option of a decision, in option order.
///
/// An option that leads to no fight carries nothing: a missing estimate is not zero odds. A fight
/// outside the predictor's cover carries only the two flags. A covered fight carries the acting
/// seat's win, loss and mutual-destruction odds, and -- when finishing movement is itself a
/// covered fight -- how much this option changes the win odds against finishing now. From
/// version 2 it also carries the expected cost each side loses. Identical inputs within the
/// decision are predicted once.
#[must_use]
pub fn decision_facts(
    seen: &Observed<'_>,
    choice: &Choice,
    player: &PlayerId,
    predictor: &BattlePredictor,
) -> Vec<Vec<(&'static str, f64)>> {
    let version = predictor.feature_version;
    if predictor.has_ground() && choice.options.iter().any(|option| option.kind == "commit") {
        return ground_facts(seen, choice, player, predictor);
    }
    if version >= 4
        && choice
            .context
            .as_ref()
            .is_some_and(|context| context.subtype == "announce_retreat")
    {
        return retreat_facts(seen, choice, player, predictor);
    }
    let names = fact_names(version);
    let queries: Vec<BattleQuery> = choice
        .options
        .iter()
        .map(|option| movement_query(seen, choice, option, player, version))
        .collect();
    let mut cache: HashMap<Vec<u32>, BattlePrediction> = HashMap::new();
    let predictions: Vec<Option<BattlePrediction>> = queries
        .iter()
        .map(|query| {
            let BattleQuery::Supported { attacker, defender } = query else {
                return None;
            };
            let input = encode(version, attacker, defender).ok()?;
            let key: Vec<u32> = input.iter().map(|value| value.to_bits()).collect();
            Some(
                cache
                    .entry(key)
                    .or_insert_with(|| predictor.predict(&input))
                    .clone(),
            )
        })
        .collect();
    let finish = choice
        .options
        .iter()
        .position(|option| option.id == "done_moving")
        .and_then(|index| predictions[index].clone());
    queries
        .iter()
        .zip(&predictions)
        .map(|(query, prediction)| {
            let mut facts = Vec::new();
            match query {
                BattleQuery::NotApplicable => {}
                BattleQuery::Unsupported(_) => {
                    facts.push((names[0], 1.0));
                    facts.push((names[1], 1.0));
                }
                BattleQuery::Supported { attacker, defender } => {
                    facts.push((names[0], 1.0));
                    if let Some(p) = prediction {
                        facts.push((names[2], f64::from(p.attacker_wins)));
                        facts.push((names[3], f64::from(p.defender_wins)));
                        facts.push((names[4], f64::from(p.mutual_destruction)));
                        if let Some(base) = &finish {
                            facts.push((names[5], f64::from(p.attacker_wins - base.attacker_wins)));
                        }
                        if version >= 2 {
                            facts.push((names[6], cost_lost(seen, attacker, &p.attacker_survival)));
                            facts.push((names[7], cost_lost(seen, defender, &p.defender_survival)));
                        }
                    }
                }
            }
            facts
        })
        .collect()
}

/// Add each option's battle facts to its projected vector.
///
/// # Panics
///
/// If `facts` does not have one entry per vector.
#[must_use]
pub fn append_facts(
    vectors: Vec<crate::features::FeatureVector>,
    facts: &[Vec<(&'static str, f64)>],
) -> Vec<crate::features::FeatureVector> {
    assert_eq!(vectors.len(), facts.len(), "one fact list per option");
    vectors
        .into_iter()
        .zip(facts)
        .map(|(vector, extra)| {
            if extra.is_empty() {
                return vector;
            }
            crate::features::FeatureVector::from_pairs(
                vector.iter().map(|(key, value)| (*key, *value)).chain(
                    extra
                        .iter()
                        .map(|(name, value)| (crate::intern::register(name), *value)),
                ),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn side(units: &[(&str, usize)], damaged: &[(&str, usize)], modifier: i64) -> BattleSide {
        let owned = |list: &[(&str, usize)]| {
            list.iter()
                .map(|(id, n)| ((*id).to_owned(), *n))
                .collect::<Vec<_>>()
        };
        BattleSide {
            units: owned(units),
            damaged: owned(damaged),
            guns: Vec::new(),
            modifier,
        }
    }

    fn position(list: &[&str], id: &str) -> usize {
        list.iter().position(|known| *known == id).expect("slot")
    }

    #[test]
    fn encoding_places_counts_damage_shift_and_guns_by_role() {
        let attacker = side(
            &[("dreadnought", 2), ("fighter", 4)],
            &[("dreadnought", 1)],
            -1,
        );
        let mut defender = side(&[("carrier", 1)], &[], 0);
        defender.guns = vec![("pds2".to_owned(), 2)];
        let x = encode(2, &attacker, &defender).expect("supported");
        assert_eq!(x.len(), input_width(2));
        assert_eq!(
            input_width(4),
            input_width(2) + 1,
            "version 4 adds the under-way flag"
        );
        let (d, f, c) = (
            position(&UNIT_IDS, "dreadnought"),
            position(&UNIT_IDS, "fighter"),
            position(&UNIT_IDS, "carrier"),
        );
        let width = side_width(2);
        assert!((x[d] - 0.25).abs() < 1e-6);
        assert!((x[f] - 0.25).abs() < 1e-6);
        assert!((x[UNIT_IDS.len() + d] - 0.125).abs() < 1e-6);
        assert!((x[2 * UNIT_IDS.len()] + 1.0).abs() < 1e-6);
        assert!((x[width + c] - 0.125).abs() < 1e-6);
        let gun = width + 2 * UNIT_IDS.len() + 1 + position(&GUN_IDS, "pds2");
        assert!((x[gun] - 0.5).abs() < 1e-6);

        // Version 1 knows no guns and no fight without defending ships.
        assert_eq!(encode(1, &attacker, &defender), Err(Unsupported::Guns));
        let mut guns_only = BattleSide::default();
        guns_only.guns = vec![("pds".to_owned(), 1)];
        assert!(encode(2, &attacker, &guns_only).is_ok());
        assert_eq!(
            encode(1, &attacker, &BattleSide::default()),
            Err(Unsupported::EmptyFleet)
        );
    }

    #[test]
    fn unknown_units_and_impossible_damage_are_refused() {
        let plain = side(&[("carrier", 1)], &[], 0);
        let naalu = side(&[("naalu_fighter", 2)], &[], 0);
        assert_eq!(
            encode(2, &naalu, &plain),
            Err(Unsupported::Unit("naalu_fighter".to_owned()))
        );
        let hurt = side(&[("dreadnought", 1)], &[("dreadnought", 2)], 0);
        assert_eq!(
            encode(2, &plain, &hurt),
            Err(Unsupported::Damage("dreadnought".to_owned()))
        );
    }

    /// Ground layers that turn the invader's landed forces into the take logit, so more landed
    /// forces take more often.
    fn ground_layers() -> Vec<(usize, usize, Vec<f32>, Vec<f32>)> {
        let mut first = vec![0.0; GROUND_INPUT_WIDTH];
        for slot in 0..GROUND_IDS.len() {
            first[slot] = 8.0;
        }
        let mut second = vec![0.0; GROUND_OUTPUT_WIDTH];
        second[0] = 1.0;
        vec![
            (GROUND_INPUT_WIDTH, 1, first, vec![0.0]),
            (
                1,
                GROUND_OUTPUT_WIDTH,
                second,
                vec![0.0; GROUND_OUTPUT_WIDTH],
            ),
        ]
    }

    fn upgrade(version: u32, predictor: BattlePredictor) -> BattlePredictor {
        if version >= 3 {
            predictor
                .with_ground("test", ground_layers())
                .expect("valid")
        } else {
            predictor
        }
    }

    fn tiny(version: u32) -> BattlePredictor {
        let space = |v: u32| {
            let (input, output) = (input_width(v), output_width(v));
            vec![
                (input, 2, vec![0.1; input * 2], vec![0.0, 0.5]),
                (2, output, vec![0.2; 2 * output], vec![0.0; output]),
            ]
        };
        if version >= 4 {
            return BattlePredictor::assemble(version, "test", space(version), ground_layers())
                .expect("valid");
        }
        let base = version.min(2);
        let predictor = BattlePredictor::new(base, "test", space(base)).expect("valid");
        upgrade(version, predictor)
    }

    #[test]
    fn predictors_of_both_versions_round_trip() {
        let a = side(&[("carrier", 1)], &[], 0);
        let d = side(&[("cruiser", 1)], &[], 0);
        for version in SUPPORTED_VERSIONS {
            let predictor = tiny(version);
            let again =
                BattlePredictor::from_json(&predictor.to_json().expect("json")).expect("loads");
            let x = encode(version, &a, &d).expect("supported");
            let p = again.predict(&x);
            let total = p.attacker_wins + p.defender_wins + p.mutual_destruction;
            assert!((total - 1.0).abs() < 1e-5);
            assert_eq!(p, predictor.predict(&x));
            let expected = if version >= 2 { UNIT_IDS.len() } else { 0 };
            assert_eq!(p.attacker_survival.len(), expected);
            assert_eq!(p.defender_survival.len(), expected);
        }
    }

    fn movement(options: Vec<ChoiceOption>) -> Choice {
        Choice::new(PlayerId::new("a"), "movement", options)
    }

    fn board(
        attacker: &str,
        defender: &str,
    ) -> (ti4_model::state::GameState, ti4_model::id::SystemId) {
        let mut state = ti4_engine::fixtures::game(&["a", "b"]);
        for (seat, faction) in [("a", attacker), ("b", defender)] {
            if let Some(player) = state.player_mut(&PlayerId::new(seat)) {
                player.faction = ti4_model::id::FactionId::new(faction);
            }
        }
        let system =
            ti4_model::id::SystemId::new(ti4_engine::fixtures::plain_systems(1)[0].clone());
        state.active_system = Some(system.clone());
        (state, system)
    }

    #[test]
    fn a_move_adds_its_ship_and_finishing_keeps_the_committed_fleet() {
        let content = ti4_content::ContentStore::embedded();
        let (mut state, system) = board("jolnar", "hacan");
        let (a, b) = (PlayerId::new("a"), PlayerId::new("b"));
        ti4_engine::fixtures::put(&mut state, &system, "dreadnought", &a, 1);
        ti4_engine::fixtures::put(&mut state, &system, "cruiser", &b, 2);
        let seen = Observed::new(&state, content, ti4_model::POK, None);

        let carrier = ChoiceOption::new("move|18|0", "move")
            .with("unit", "carrier")
            .with("damaged", false);
        let hurt_dread = ChoiceOption::new("move|18|1", "move")
            .with("unit", "dreadnought")
            .with("damaged", true);
        let done = ChoiceOption::new("done_moving", "decline");
        let choice = movement(vec![carrier.clone(), hurt_dread.clone(), done.clone()]);

        for version in SUPPORTED_VERSIONS {
            let BattleQuery::Supported { attacker, defender } =
                movement_query(&seen, &choice, &carrier, &a, version)
            else {
                panic!("carrier move is supported");
            };
            assert_eq!(
                attacker.units,
                vec![("carrier".to_owned(), 1), ("dreadnought".to_owned(), 1)]
            );
            assert_eq!(attacker.modifier, -1);
            assert_eq!(defender.units, vec![("cruiser".to_owned(), 2)]);
        }

        let BattleQuery::Supported { attacker, .. } =
            movement_query(&seen, &choice, &hurt_dread, &a, 2)
        else {
            panic!("dreadnought move is supported");
        };
        assert_eq!(attacker.units, vec![("dreadnought".to_owned(), 2)]);
        assert_eq!(attacker.damaged, vec![("dreadnought".to_owned(), 1)]);

        let BattleQuery::Supported { attacker, .. } = movement_query(&seen, &choice, &done, &a, 2)
        else {
            panic!("finishing is supported");
        };
        assert_eq!(attacker.units, vec![("dreadnought".to_owned(), 1)]);
    }

    #[test]
    fn planet_guns_join_the_defender_from_version_two() {
        let content = ti4_content::ContentStore::embedded();
        let (mut state, system) = board("sol", "xxcha");
        let (a, b) = (PlayerId::new("a"), PlayerId::new("b"));
        let planet = ti4_model::id::PlanetId::new("gun-planet");
        state
            .system_mut(&system)
            .planet_units
            .entry(planet)
            .or_default()
            .extend([
                ti4_model::units::Unit::new(ti4_model::id::UnitTypeId::new("pds"), b.clone()),
                ti4_model::units::Unit::new(
                    ti4_model::id::UnitTypeId::new("xxcha_mech"),
                    b.clone(),
                ),
            ]);
        let seen = Observed::new(&state, content, ti4_model::POK, None);
        let carrier = ChoiceOption::new("move|18|0", "move").with("unit", "carrier");
        let choice = movement(vec![carrier.clone()]);

        // No enemy ships: version 1 sees no fight, version 2 sees the guns.
        assert_eq!(
            movement_query(&seen, &choice, &carrier, &a, 1),
            BattleQuery::NotApplicable
        );
        let BattleQuery::Supported { defender, .. } =
            movement_query(&seen, &choice, &carrier, &a, 2)
        else {
            panic!("a covered system is a fight in version 2");
        };
        assert!(defender.units.is_empty());
        assert_eq!(
            defender.guns,
            vec![("pds".to_owned(), 1), ("xxcha_mech".to_owned(), 1)]
        );

        // With enemy ships as well, version 1 declines rather than ignore the guns.
        ti4_engine::fixtures::put(&mut state, &system, "cruiser", &b, 1);
        let seen = Observed::new(&state, content, ti4_model::POK, None);
        assert_eq!(
            movement_query(&seen, &choice, &carrier, &a, 1),
            BattleQuery::Unsupported(Unsupported::Guns)
        );
    }

    #[test]
    fn peaceful_moves_and_uncovered_fights_say_so() {
        let content = ti4_content::ContentStore::embedded();
        let a = PlayerId::new("a");
        let carrier = ChoiceOption::new("move|18|0", "move").with("unit", "carrier");
        let choice = movement(vec![carrier.clone()]);

        let (state, _) = board("sol", "hacan");
        let seen = Observed::new(&state, content, ti4_model::POK, None);
        assert_eq!(
            movement_query(&seen, &choice, &carrier, &a, 2),
            BattleQuery::NotApplicable
        );

        let (mut state, system) = board("sol", "sardakk");
        ti4_engine::fixtures::put(&mut state, &system, "cruiser", &PlayerId::new("b"), 1);
        let seen = Observed::new(&state, content, ti4_model::POK, None);
        assert_eq!(
            movement_query(&seen, &choice, &carrier, &a, 2),
            BattleQuery::Unsupported(Unsupported::Faction("sardakk".to_owned()))
        );

        let other = Choice::new(a.clone(), "activation", vec![carrier.clone()]);
        assert_eq!(
            movement_query(&seen, &other, &carrier, &a, 2),
            BattleQuery::NotApplicable
        );
    }

    /// A predictor that scores only the attacker's dreadnought slot, so more dreadnoughts win.
    fn dreadnought_counter(version: u32) -> BattlePredictor {
        if version >= 4 {
            let space = dreadnought_counter_space(2);
            let (input, output) = (input_width(version), output_width(version));
            let mut first = vec![0.0; input];
            first[position(&UNIT_IDS, "dreadnought")] = 8.0;
            let mut second = vec![0.0; output];
            second[0] = 1.0;
            let _ = space;
            return BattlePredictor::assemble(
                version,
                "test",
                vec![
                    (input, 1, first, vec![0.0]),
                    (1, output, second, vec![0.0; output]),
                ],
                ground_layers(),
            )
            .expect("valid");
        }
        let predictor = dreadnought_counter_space(version.min(2));
        upgrade(version, predictor)
    }

    fn dreadnought_counter_space(version: u32) -> BattlePredictor {
        let (input, output) = (input_width(version), output_width(version));
        let mut first = vec![0.0; input];
        first[position(&UNIT_IDS, "dreadnought")] = 8.0;
        let mut second = vec![0.0; output];
        second[0] = 1.0;
        BattlePredictor::new(
            version,
            "test",
            vec![
                (input, 1, first, vec![0.0]),
                (1, output, second, vec![0.0; output]),
            ],
        )
        .expect("valid")
    }

    #[test]
    fn decision_facts_compare_each_move_with_finishing() {
        let content = ti4_content::ContentStore::embedded();
        let (mut state, system) = board("hacan", "letnev");
        let (a, b) = (PlayerId::new("a"), PlayerId::new("b"));
        ti4_engine::fixtures::put(&mut state, &system, "cruiser", &a, 1);
        ti4_engine::fixtures::put(&mut state, &system, "cruiser", &b, 2);
        let seen = Observed::new(&state, content, ti4_model::POK, None);
        let dreadnought = ChoiceOption::new("move|18|0", "move").with("unit", "dreadnought");
        let done = ChoiceOption::new("done_moving", "decline");
        let other = ChoiceOption::new("pass", "decline");
        let choice = movement(vec![dreadnought, done, other]);
        let value =
            |row: &[(&str, f64)], name: &str| row.iter().find(|(n, _)| *n == name).map(|(_, v)| *v);
        for version in SUPPORTED_VERSIONS {
            let facts = decision_facts(&seen, &choice, &a, &dreadnought_counter(version));
            assert_eq!(facts.len(), 3);
            let change = value(&facts[0], "action-plan:battle-win-change").expect("compared");
            assert!(
                change > 0.0,
                "adding a dreadnought raises the win odds: {change}"
            );
            assert_eq!(value(&facts[1], "action-plan:battle-win-change"), Some(0.0));
            assert!(facts[2].is_empty(), "a non-movement option carries nothing");
            let names: Vec<&str> = facts[0].iter().map(|(name, _)| *name).collect();
            assert!(names.iter().all(|name| fact_names(version).contains(name)));
            // Survival sigmoids sit at 0.5 in this predictor, so half of each fleet is lost.
            let own = value(&facts[0], "action-plan:battle-own-cost-lost");
            assert_eq!(own.is_some(), version >= 2);
            if let Some(own) = own {
                assert!((own - 0.3).abs() < 1e-6, "(4 + 2) / 2 / 10, got {own}");
            }
        }
    }

    #[test]
    fn mismatched_shapes_and_unknown_versions_are_refused() {
        let layers = vec![(INPUT_WIDTH, 3, vec![0.0; 3], vec![0.0; 3])];
        assert!(matches!(
            BattlePredictor::new(2, "test", layers),
            Err(LoadError::Shape)
        ));
        let layers = vec![(
            input_width(3),
            3,
            vec![0.0; 3 * input_width(3)],
            vec![0.0; 3],
        )];
        assert!(matches!(
            BattlePredictor::new(5, "test", layers.clone()),
            Err(LoadError::Version { found: 5 })
        ));
        // Version 3 is only reachable by adding ground layers, and they must chain.
        assert!(matches!(
            BattlePredictor::new(3, "test", layers),
            Err(LoadError::Shape)
        ));
        let bad_ground = vec![(GROUND_INPUT_WIDTH, 3, vec![0.0; 3], vec![0.0; 3])];
        assert!(matches!(
            tiny(2).with_ground("test", bad_ground),
            Err(LoadError::Shape)
        ));
        assert!(matches!(
            tiny(3).with_ground("test", ground_layers()),
            Err(LoadError::Version { found: 3 })
        ));
    }

    #[test]
    fn ground_encoding_places_forces_damage_guns_and_harrow() {
        let invader = GroundBattleSide {
            forces: vec![("infantry2".to_owned(), 4), ("l1z1x_mech".to_owned(), 2)],
            damaged: vec![("l1z1x_mech".to_owned(), 1)],
            guns: Vec::new(),
            harrow: vec![("warsun".to_owned(), 1)],
            modifier: 0,
        };
        let defender = GroundBattleSide {
            forces: vec![("infantry".to_owned(), 2)],
            damaged: Vec::new(),
            guns: vec![("pds2".to_owned(), 2)],
            harrow: Vec::new(),
            modifier: -1,
        };
        let x = encode_ground(&invader, &defender).expect("supported");
        let n = GROUND_IDS.len();
        assert!((x[position(&GROUND_IDS, "infantry2")] - 0.5).abs() < 1e-6);
        assert!((x[n + position(&GROUND_IDS, "l1z1x_mech")] - 0.25).abs() < 1e-6);
        let harrow = 2 * n + 1 + DEFENSE_GUN_IDS.len() + position(&HARROW_IDS, "warsun");
        assert!((x[harrow] - 0.25).abs() < 1e-6);
        let d = GROUND_SIDE_WIDTH;
        assert!((x[d + 2 * n] + 1.0).abs() < 1e-6);
        assert!((x[d + 2 * n + 1 + position(&DEFENSE_GUN_IDS, "pds2")] - 0.5).abs() < 1e-6);
        assert_eq!(
            encode_ground(&GroundBattleSide::default(), &defender),
            Err(Unsupported::EmptyFleet)
        );
    }

    #[test]
    fn a_commit_lands_its_unit_beside_the_committed_forces() {
        let content = ti4_content::ContentStore::embedded();
        let (mut state, system) = board("l1z1x", "sol");
        let (a, b) = (PlayerId::new("a"), PlayerId::new("b"));
        let planet = ti4_model::id::PlanetId::new("contested");
        let put =
            |state: &mut ti4_model::state::GameState, kind: &str, owner: &PlayerId, n: usize| {
                ti4_engine::fixtures::put_on_planet(state, &system, &planet, kind, owner, n);
            };
        put(&mut state, "infantry", &a, 1);
        put(&mut state, "sol_infantry", &b, 2);
        put(&mut state, "pds", &b, 1);
        ti4_engine::fixtures::put(&mut state, &system, "l1z1x_dreadnought", &a, 2);
        ti4_engine::fixtures::put(&mut state, &system, "warsun", &a, 1);
        let seen = Observed::new(&state, content, ti4_model::POK, None);
        let mech = ChoiceOption::labelled(
            "commit|0|contested",
            "commit",
            "land l1z1x_mech on contested",
        )
        .with("planet", "contested")
        .with("unit", "l1z1x_mech");

        let GroundQuery::Supported {
            attacker,
            defender,
            baseline,
        } = invasion_query(&seen, &mech, &a)
        else {
            panic!("an invasion is supported");
        };
        assert_eq!(
            attacker.forces,
            vec![("infantry".to_owned(), 1), ("l1z1x_mech".to_owned(), 1)]
        );
        // The shield stands, but the war sun lets Harrow through.
        assert_eq!(
            attacker.harrow,
            vec![
                ("l1z1x_dreadnought".to_owned(), 2),
                ("warsun".to_owned(), 1)
            ]
        );
        assert_eq!(defender.forces, vec![("sol_infantry".to_owned(), 2)]);
        assert_eq!(defender.guns, vec![("pds".to_owned(), 1)]);
        assert_eq!(
            baseline.map(|side| side.forces),
            Some(vec![("infantry".to_owned(), 1)])
        );

        // A non-commit option is not an invasion; nor is a planet nobody holds.
        let done = ChoiceOption::new("done_committing", "decline");
        assert_eq!(invasion_query(&seen, &done, &a), GroundQuery::NotApplicable);
        let empty = ChoiceOption::labelled("commit|0|elsewhere", "commit", "land")
            .with("planet", "elsewhere")
            .with("unit", "infantry");
        assert_eq!(
            invasion_query(&seen, &empty, &a),
            GroundQuery::NotApplicable
        );

        // Ground facts: take odds rise with the landed mech against the committed infantry.
        let choice = Choice::new(a.clone(), "commit ground forces", vec![mech, done]);
        let facts = decision_facts(&seen, &choice, &a, &tiny(3));
        let value =
            |row: &[(&str, f64)], name: &str| row.iter().find(|(n, _)| *n == name).map(|(_, v)| *v);
        assert!(value(&facts[0], "action-plan:ground-take-change").expect("compared") > 0.0);
        assert!(value(&facts[0], "action-plan:ground-own-cost-lost").is_some());
        assert!(facts[1].is_empty());
        // A version-2 predictor says nothing about invasions.
        assert!(
            decision_facts(&seen, &choice, &a, &tiny(2))
                .iter()
                .all(Vec::is_empty)
        );
        let _ = b;
    }

    #[test]
    fn a_retreat_announcement_carries_the_odds_of_staying_in() {
        let content = ti4_content::ContentStore::embedded();
        let (mut state, system) = board("hacan", "letnev");
        let (a, b) = (PlayerId::new("a"), PlayerId::new("b"));
        ti4_engine::fixtures::put(&mut state, &system, "dreadnought", &a, 1);
        ti4_engine::fixtures::put(&mut state, &system, "cruiser", &b, 2);
        state.active = Some(a.clone());
        let seen = Observed::new(&state, content, ti4_model::POK, None);
        let here = system.to_string();
        let choice = Choice::new(
            b.clone(),
            "announce a retreat",
            vec![
                ChoiceOption::labelled("stay", "retreat", "stay and fight")
                    .with("system", here.clone()),
                ChoiceOption::labelled("retreat", "retreat", "announce a retreat")
                    .with("system", here),
            ],
        )
        .contextualized(ti4_engine::decision_context::DecisionContext::new(
            b.clone(),
            ti4_engine::decision_context::DecisionSource::Rule("78.9".to_owned()),
            "announce_retreat",
            ti4_model::state::Phase::Action,
            1,
        ));
        let (attacker, defender, acting_attacks) =
            retreat_query(&seen, &choice, &b).expect("a covered fight");
        assert_eq!(attacker.units, vec![("dreadnought".to_owned(), 1)]);
        assert_eq!(defender.units, vec![("cruiser".to_owned(), 2)]);
        assert!(!acting_attacks, "the defender is the one asked");
        let input = encode_at(4, &attacker, &defender, true).expect("encodes");
        assert_eq!(input[input.len() - 1], 1.0, "the under-way flag is set");

        let facts = decision_facts(&seen, &choice, &b, &dreadnought_counter(4));
        assert_eq!(facts.len(), 2);
        assert_eq!(facts[0], facts[1], "both options carry the same odds");
        let win = facts[0]
            .iter()
            .find(|(name, _)| *name == "action-plan:battle-stay-win")
            .map(|(_, v)| *v)
            .expect("stay odds");
        let loss = facts[0]
            .iter()
            .find(|(name, _)| *name == "action-plan:battle-stay-loss")
            .map(|(_, v)| *v)
            .expect("stay odds");
        // The predictor favours the attacker's dreadnought, so the defender's stay-win is low.
        assert!(win < loss, "seen from the defender: {win} vs {loss}");
        // Version 3 has no such facts.
        assert!(
            decision_facts(&seen, &choice, &b, &tiny(3))
                .iter()
                .all(Vec::is_empty)
        );
    }
}
