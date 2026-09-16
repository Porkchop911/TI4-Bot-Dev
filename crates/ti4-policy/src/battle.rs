//! Battle inputs and the frozen space-combat predictor (ARENA-002).
//!
//! One encoding serves both the arena trainer and live play, so the predictor never sees a
//! different vector in a game than it was trained on. The predictor itself is a plain-Rust
//! forward pass: the live policy crate must not depend on the tensor library.
//!
//! Scope of `FEATURE_VERSION` 1: the six factions the bots play, with and without upgrades,
//! space combat only, both sides' space cannon before combat, the declared sustain-first
//! casualty order. A side containing any other unit is unsupported rather than approximated.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use serde::{Deserialize, Serialize};
use ti4_engine::choice::{Choice, ChoiceOption, Observed};
use ti4_model::id::PlayerId;

/// Bumped whenever the encoding below changes meaning.
pub const FEATURE_VERSION: u32 = 1;

/// Units the version-1 encoding knows, in slot order. Never reorder; append under a new version.
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

/// Factions version 1 covers, with their shift to every combat roll.
pub const FACTIONS: [(&str, i64); 6] = [
    ("sol", 0),
    ("letnev", 0),
    ("xxcha", 0),
    ("hacan", 0),
    ("jolnar", -1),
    ("l1z1x", 0),
];

/// Width of one side's block: counts, damaged counts, dice shift.
pub const SIDE_WIDTH: usize = 2 * UNIT_IDS.len() + 1;

/// Width of a whole input: attacker block then defender block.
pub const INPUT_WIDTH: usize = 2 * SIDE_WIDTH;

/// One side of a space battle: ships by unit id, how many of each start damaged, and the
/// faction's shift to every combat roll.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BattleSide {
    pub units: Vec<(String, usize)>,
    pub damaged: Vec<(String, usize)>,
    pub modifier: i64,
}

/// Why a side cannot be encoded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unsupported {
    /// A unit outside `UNIT_IDS`.
    Unit(String),
    /// More damaged ships of an id than there are ships of it.
    Damage(String),
    /// A faction outside `FACTIONS`, whose abilities the predictor never saw.
    Faction(String),
    /// Ships of more than one other player at the destination.
    Opponents,
    /// The destination is an anomaly, whose combat effects the predictor never saw.
    Anomaly,
    /// A unit on a planet there has SPACE CANNON; the predictor models ships only.
    Guns,
    /// A galvanized ship, whose modifiers the predictor never saw.
    Galvanized,
    /// The acting seat would fight with no ships.
    EmptyFleet,
}

/// What a movement option means for the fight at its destination.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BattleQuery {
    /// Not a movement decision, or no other player's ships at the destination.
    NotApplicable,
    /// A fight follows, but outside what the predictor covers. The reason is public.
    Unsupported(Unsupported),
    /// The fight as it would stand if movement ended with this option taken.
    Supported {
        attacker: BattleSide,
        defender: BattleSide,
    },
}

/// The battle a movement option leads to, from public information only.
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
    let Some(enemy) = opponents.first().copied() else {
        return BattleQuery::NotApplicable;
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
    if here
        .planet_units
        .values()
        .flatten()
        .any(|unit| kind(unit.type_id.as_str()).is_some_and(|k| k.has_space_cannon()))
    {
        return BattleQuery::Unsupported(Unsupported::Guns);
    }

    let side = |owner: &PlayerId, extra: Option<(&str, bool)>| -> Result<BattleSide, Unsupported> {
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
        Ok(BattleSide {
            units: units.into_iter().collect(),
            damaged: damaged.into_iter().collect(),
            modifier,
        })
    };
    let built = side(player, moving).and_then(|attacker| {
        if attacker.units.is_empty() {
            return Err(Unsupported::EmptyFleet);
        }
        let defender = side(enemy, None)?;
        encode(&attacker, &defender)?;
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
    fn encode(&self, out: &mut [f32]) -> Result<(), Unsupported> {
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
        Ok(())
    }
}

/// The version-1 input for a fight between `attacker` (the active player) and `defender`.
///
/// # Errors
///
/// A side carries a unit or damage the encoding does not cover.
pub fn encode(
    attacker: &BattleSide,
    defender: &BattleSide,
) -> Result<[f32; INPUT_WIDTH], Unsupported> {
    let mut out = [0.0; INPUT_WIDTH];
    attacker.encode(&mut out[..SIDE_WIDTH])?;
    defender.encode(&mut out[SIDE_WIDTH..])?;
    Ok(out)
}

/// Probabilities of the three ways a space battle ends.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BattlePrediction {
    pub attacker_wins: f32,
    pub defender_wins: f32,
    pub mutual_destruction: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Layer {
    inputs: usize,
    outputs: usize,
    /// Row-major, `outputs` rows of `inputs`.
    weight: Vec<f32>,
    bias: Vec<f32>,
}

/// A trained predictor: dense layers with ReLU between them, softmax over three outcomes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BattlePredictor {
    pub feature_version: u32,
    /// Identity of the labels it was trained on, e.g. the casualty policy and rule choices.
    pub continuation: String,
    unit_ids: Vec<String>,
    layers: Vec<Layer>,
}

/// Why a predictor file was refused.
#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    #[error("predictor file: {0}")]
    Json(#[from] serde_json::Error),
    #[error("predictor built for feature version {found}, this build encodes version {expected}")]
    Version { found: u32, expected: u32 },
    #[error("predictor unit list does not match this build's")]
    Units,
    #[error("predictor layers do not chain from {INPUT_WIDTH} inputs to 3 outputs")]
    Shape,
}

impl BattlePredictor {
    /// Assemble from `(inputs, outputs, weight, bias)` layers, checking shapes.
    ///
    /// # Errors
    ///
    /// The layers do not chain from `INPUT_WIDTH` to three outputs, or a buffer has the wrong
    /// length.
    pub fn new(
        continuation: impl Into<String>,
        layers: Vec<(usize, usize, Vec<f32>, Vec<f32>)>,
    ) -> Result<Self, LoadError> {
        let predictor = Self {
            feature_version: FEATURE_VERSION,
            continuation: continuation.into(),
            unit_ids: UNIT_IDS.iter().map(|id| (*id).to_owned()).collect(),
            layers: layers
                .into_iter()
                .map(|(inputs, outputs, weight, bias)| Layer {
                    inputs,
                    outputs,
                    weight,
                    bias,
                })
                .collect(),
        };
        predictor.validate()?;
        Ok(predictor)
    }

    fn validate(&self) -> Result<(), LoadError> {
        if self.feature_version != FEATURE_VERSION {
            return Err(LoadError::Version {
                found: self.feature_version,
                expected: FEATURE_VERSION,
            });
        }
        if self.unit_ids.iter().map(String::as_str).ne(UNIT_IDS) {
            return Err(LoadError::Units);
        }
        let mut width = INPUT_WIDTH;
        for layer in &self.layers {
            if layer.inputs != width
                || layer.weight.len() != layer.inputs * layer.outputs
                || layer.bias.len() != layer.outputs
            {
                return Err(LoadError::Shape);
            }
            width = layer.outputs;
        }
        if width != 3 || self.layers.is_empty() {
            return Err(LoadError::Shape);
        }
        Ok(())
    }

    /// # Errors
    ///
    /// Malformed JSON, or a predictor for a different encoding.
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

    /// Outcome probabilities for an encoded input.
    #[must_use]
    pub fn predict(&self, input: &[f32; INPUT_WIDTH]) -> BattlePrediction {
        let mut current: Vec<f32> = input.to_vec();
        for (index, layer) in self.layers.iter().enumerate() {
            let mut next = layer.bias.clone();
            for (row, value) in next.iter_mut().enumerate() {
                let weights = &layer.weight[row * layer.inputs..(row + 1) * layer.inputs];
                *value += weights
                    .iter()
                    .zip(&current)
                    .map(|(w, x)| w * x)
                    .sum::<f32>();
            }
            if index + 1 < self.layers.len() {
                for value in &mut next {
                    *value = value.max(0.0);
                }
            }
            current = next;
        }
        let top = current.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        let exp: Vec<f32> = current.iter().map(|v| (v - top).exp()).collect();
        let total: f32 = exp.iter().sum();
        BattlePrediction {
            attacker_wins: exp[0] / total,
            defender_wins: exp[1] / total,
            mutual_destruction: exp[2] / total,
        }
    }
}

/// The closed battle facts, under the `action-plan` family: what an option does to the fight
/// at its destination. Appended into preallocated vocabulary rows by the arena migration.
pub const FACT_NAMES: [&str; 6] = [
    "action-plan:battle-fight",
    "action-plan:battle-unsupported",
    "action-plan:battle-win",
    "action-plan:battle-loss",
    "action-plan:battle-mutual",
    "action-plan:battle-win-change",
];

/// Battle facts for every option of a decision, in option order.
///
/// An option that leads to no fight carries nothing: a missing estimate is not zero odds. A fight
/// outside the predictor's cover carries only the two flags. A covered fight carries the acting
/// seat's win, loss and mutual-destruction odds, and -- when finishing movement is itself a
/// covered fight -- how much this option changes the win odds against finishing now. Identical
/// inputs within the decision are predicted once.
#[must_use]
pub fn decision_facts(
    seen: &Observed<'_>,
    choice: &Choice,
    player: &PlayerId,
    predictor: &BattlePredictor,
) -> Vec<Vec<(&'static str, f64)>> {
    let queries: Vec<BattleQuery> = choice
        .options
        .iter()
        .map(|option| movement_query(seen, choice, option, player))
        .collect();
    let mut cache: HashMap<[u32; INPUT_WIDTH], BattlePrediction> = HashMap::new();
    let predictions: Vec<Option<BattlePrediction>> = queries
        .iter()
        .map(|query| {
            let BattleQuery::Supported { attacker, defender } = query else {
                return None;
            };
            let input = encode(attacker, defender).ok()?;
            Some(
                *cache
                    .entry(input.map(f32::to_bits))
                    .or_insert_with(|| predictor.predict(&input)),
            )
        })
        .collect();
    let finish = choice
        .options
        .iter()
        .position(|option| option.id == "done_moving")
        .and_then(|index| predictions[index]);
    queries
        .iter()
        .zip(&predictions)
        .map(|(query, prediction)| {
            let mut facts = Vec::new();
            match query {
                BattleQuery::NotApplicable => {}
                BattleQuery::Unsupported(_) => {
                    facts.push((FACT_NAMES[0], 1.0));
                    facts.push((FACT_NAMES[1], 1.0));
                }
                BattleQuery::Supported { .. } => {
                    facts.push((FACT_NAMES[0], 1.0));
                    if let Some(p) = prediction {
                        facts.push((FACT_NAMES[2], f64::from(p.attacker_wins)));
                        facts.push((FACT_NAMES[3], f64::from(p.defender_wins)));
                        facts.push((FACT_NAMES[4], f64::from(p.mutual_destruction)));
                        if let Some(base) = finish {
                            facts.push((
                                FACT_NAMES[5],
                                f64::from(p.attacker_wins - base.attacker_wins),
                            ));
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
        BattleSide {
            units: units.iter().map(|(id, n)| ((*id).to_owned(), *n)).collect(),
            damaged: damaged
                .iter()
                .map(|(id, n)| ((*id).to_owned(), *n))
                .collect(),
            modifier,
        }
    }

    #[test]
    fn encoding_places_counts_damage_and_shift_by_role() {
        let attacker = side(
            &[("dreadnought", 2), ("fighter", 4)],
            &[("dreadnought", 1)],
            -1,
        );
        let defender = side(&[("carrier", 1)], &[], 0);
        let x = encode(&attacker, &defender).expect("supported");
        let d = UNIT_IDS
            .iter()
            .position(|id| *id == "dreadnought")
            .expect("slot");
        let f = UNIT_IDS
            .iter()
            .position(|id| *id == "fighter")
            .expect("slot");
        let c = UNIT_IDS
            .iter()
            .position(|id| *id == "carrier")
            .expect("slot");
        assert!((x[d] - 0.25).abs() < 1e-6);
        assert!((x[f] - 0.25).abs() < 1e-6);
        assert!((x[UNIT_IDS.len() + d] - 0.125).abs() < 1e-6);
        assert!((x[2 * UNIT_IDS.len()] + 1.0).abs() < 1e-6);
        assert!((x[SIDE_WIDTH + c] - 0.125).abs() < 1e-6);
    }

    #[test]
    fn unknown_units_and_impossible_damage_are_refused() {
        let plain = side(&[("carrier", 1)], &[], 0);
        let naalu = side(&[("naalu_fighter", 2)], &[], 0);
        assert_eq!(
            encode(&naalu, &plain),
            Err(Unsupported::Unit("naalu_fighter".to_owned()))
        );
        let hurt = side(&[("dreadnought", 1)], &[("dreadnought", 2)], 0);
        assert_eq!(
            encode(&plain, &hurt),
            Err(Unsupported::Damage("dreadnought".to_owned()))
        );
    }

    #[test]
    fn a_predictor_round_trips_and_its_softmax_sums_to_one() {
        let layers = vec![
            (INPUT_WIDTH, 2, vec![0.1; INPUT_WIDTH * 2], vec![0.0, 0.5]),
            (
                2,
                3,
                vec![1.0, 0.0, 0.0, 1.0, -1.0, -1.0],
                vec![0.0, 0.0, 0.0],
            ),
        ];
        let predictor = BattlePredictor::new("test", layers).expect("valid");
        let again = BattlePredictor::from_json(&predictor.to_json().expect("json")).expect("loads");
        let x = encode(
            &side(&[("carrier", 1)], &[], 0),
            &side(&[("cruiser", 1)], &[], 0),
        )
        .expect("supported");
        let p = again.predict(&x);
        let total = p.attacker_wins + p.defender_wins + p.mutual_destruction;
        assert!((total - 1.0).abs() < 1e-5);
        assert_eq!(p, predictor.predict(&x));
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

        let BattleQuery::Supported { attacker, defender } =
            movement_query(&seen, &choice, &carrier, &a)
        else {
            panic!("carrier move is supported");
        };
        assert_eq!(
            attacker.units,
            vec![("carrier".to_owned(), 1), ("dreadnought".to_owned(), 1)]
        );
        assert_eq!(attacker.modifier, -1);
        assert_eq!(defender.units, vec![("cruiser".to_owned(), 2)]);

        let BattleQuery::Supported { attacker, .. } =
            movement_query(&seen, &choice, &hurt_dread, &a)
        else {
            panic!("dreadnought move is supported");
        };
        assert_eq!(attacker.units, vec![("dreadnought".to_owned(), 2)]);
        assert_eq!(attacker.damaged, vec![("dreadnought".to_owned(), 1)]);

        let BattleQuery::Supported { attacker, .. } = movement_query(&seen, &choice, &done, &a)
        else {
            panic!("finishing is supported");
        };
        assert_eq!(attacker.units, vec![("dreadnought".to_owned(), 1)]);
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
            movement_query(&seen, &choice, &carrier, &a),
            BattleQuery::NotApplicable
        );

        let (mut state, system) = board("sol", "sardakk");
        ti4_engine::fixtures::put(&mut state, &system, "cruiser", &PlayerId::new("b"), 1);
        let seen = Observed::new(&state, content, ti4_model::POK, None);
        assert_eq!(
            movement_query(&seen, &choice, &carrier, &a),
            BattleQuery::Unsupported(Unsupported::Faction("sardakk".to_owned()))
        );

        let other = Choice::new(a.clone(), "activation", vec![carrier.clone()]);
        assert_eq!(
            movement_query(&seen, &other, &carrier, &a),
            BattleQuery::NotApplicable
        );
    }

    #[test]
    fn decision_facts_compare_each_move_with_finishing() {
        let content = ti4_content::ContentStore::embedded();
        let (mut state, system) = board("hacan", "letnev");
        let (a, b) = (PlayerId::new("a"), PlayerId::new("b"));
        ti4_engine::fixtures::put(&mut state, &system, "cruiser", &a, 1);
        ti4_engine::fixtures::put(&mut state, &system, "cruiser", &b, 2);
        let seen = Observed::new(&state, content, ti4_model::POK, None);
        // A predictor that scores only the attacker's dreadnought slot, so more dreadnoughts win.
        let dread = UNIT_IDS
            .iter()
            .position(|id| *id == "dreadnought")
            .expect("slot");
        let mut first = vec![0.0; INPUT_WIDTH];
        first[dread] = 8.0;
        let predictor = BattlePredictor::new(
            "test",
            vec![
                (INPUT_WIDTH, 1, first, vec![0.0]),
                (1, 3, vec![1.0, 0.0, 0.0], vec![0.0, 0.0, 0.0]),
            ],
        )
        .expect("valid");
        let dreadnought = ChoiceOption::new("move|18|0", "move").with("unit", "dreadnought");
        let done = ChoiceOption::new("done_moving", "decline");
        let other = ChoiceOption::new("pass", "decline");
        let choice = movement(vec![dreadnought, done, other]);
        let facts = decision_facts(&seen, &choice, &a, &predictor);
        assert_eq!(facts.len(), 3);
        let value =
            |row: &[(&str, f64)], name: &str| row.iter().find(|(n, _)| *n == name).map(|(_, v)| *v);
        let change = value(&facts[0], "action-plan:battle-win-change").expect("compared");
        assert!(
            change > 0.0,
            "adding a dreadnought raises the win odds: {change}"
        );
        assert_eq!(value(&facts[1], "action-plan:battle-win-change"), Some(0.0));
        assert!(facts[2].is_empty(), "a non-movement option carries nothing");
    }

    #[test]
    fn mismatched_shapes_are_refused() {
        let layers = vec![(INPUT_WIDTH, 3, vec![0.0; 3], vec![0.0; 3])];
        assert!(matches!(
            BattlePredictor::new("test", layers),
            Err(LoadError::Shape)
        ));
    }
}
