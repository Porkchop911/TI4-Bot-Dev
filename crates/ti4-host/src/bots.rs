//! The seats that a host plays itself: the rule bot, the random seat, and an MLP model.
//!
//! A table names the kind of a seat by a string ([`BotKind::id`]): `smart`, `random`, or
//! `mlp:<name>` for the model of that name. A host resolves the string to a [`BotKind`] when it
//! seats the table ([`BotKind::parse`], against the models it has loaded), and plays each seat
//! with a [`Bot`]. Both hosts (the server and the engine of the page) answer a seat with
//! [`choose`], so a table of a seed is the same game on both.
//!
//! # Determinism
//!
//! Every answer comes from the seed of the game, the place of the seat in the seat order and the
//! number of the decision in the game ([`ti4_rule_bot::random::decision_rng`]), as the rule bot's
//! always did. The rule bot and the random seat keep nothing between two decisions. An MLP seat
//! does: its decision pipeline ([`ti4_policy::mlp::Pipeline`]) carries the fleet plan of the
//! activation being moved, the power maps of the seat and the table of its latest activation
//! (its [`Carry`]). So that a game played again goes on as the game that was not interrupted:
//!
//! - a host that plays a game again from its recorded decisions (a recovered game on the server,
//!   a saved game in the page) gives each recorded decision of the seat to [`Bot::follow`], which
//!   changes the pipeline as the decision changed it when it was made;
//! - a host that goes back to a copy of the game (the checkpoints of the page) keeps the
//!   [`Bot::carry`] of each seat with the copy and restores it with the copy;
//! - a host never starts a game from a copy that skipped the decisions of an MLP seat (the server
//!   replays the whole log of a table with an MLP seat).
//!
//! The models are loaded once per process into a registry by name ([`register`], [`loaded`]):
//! the server loads its model directory at start, the page loads a model when a seat needs it.
//! A [`BotKind`] is written as its id; reading `mlp:<name>` needs the model in the registry.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::{Arc, Mutex};

use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};
use ti4_content::galaxy::Galaxy;
use ti4_engine::choice::{Choice, ChoiceOption, Observed, SeatObservation};
use ti4_model::id::PlayerId;
use ti4_model::state::GameState;
use ti4_policy::battle::BattlePredictor;
use ti4_policy::mlp::{Columns, HeadLayout, MlpError, ModelInfo, Pipeline, Scorer};
use ti4_policy::vocabulary::Vocabulary;

pub use ti4_policy::mlp::Carry;

use crate::setup::bot_kind;

/// The prefix of the kind of an MLP model: `mlp:<name>`.
pub const MLP_PREFIX: &str = "mlp:";

/// A loaded `.ti4m` model, ready to score the decisions of a seat: the [`Scorer`] of the
/// decision pipeline over [`ti4_infer::Model`]. Cheap to clone; one model serves every seat of
/// every game.
#[derive(Clone)]
pub struct MlpModel(Arc<Loaded>);

struct Loaded {
    model: ti4_infer::Model,
    layout: HeadLayout,
    battle: Option<BattlePredictor>,
    vocabulary: Arc<Vocabulary>,
}

thread_local! {
    /// The working memory of a forward pass, one per thread.
    static SCRATCH: std::cell::RefCell<ti4_infer::Scratch> =
        std::cell::RefCell::new(ti4_infer::Scratch::new());
}

impl MlpModel {
    /// The model of a loaded file, with its head layout, its vocabulary and its battle predictor
    /// checked.
    ///
    /// # Errors
    /// A message naming the model when its heads, its vocabulary or its predictor are not of a
    /// model that this build can play.
    pub fn new(model: ti4_infer::Model) -> Result<Self, String> {
        let name = model.name().to_owned();
        if name.is_empty() {
            return Err("a model needs a name".to_owned());
        }
        let layout = HeadLayout::of(model.heads())
            .ok_or_else(|| format!("model {name}: its heads are of no known layout"))?;
        let vocabulary = Vocabulary::from_json_for_inference(model.slots_json())
            .map_err(|error| format!("model {name}: its vocabulary: {error}"))?;
        if vocabulary.capacity() > model.capacity() {
            return Err(format!(
                "model {name}: its vocabulary has {} rows, the model {}",
                vocabulary.capacity(),
                model.capacity()
            ));
        }
        let battle = model
            .battle_json()
            .map(BattlePredictor::from_json)
            .transpose()
            .map_err(|error| format!("model {name}: its battle predictor: {error}"))?;
        Ok(Self(Arc::new(Loaded {
            model,
            layout,
            battle,
            vocabulary: Arc::new(vocabulary),
        })))
    }

    /// The model of the bytes of a `.ti4m` file, its trailer checked, on `backend` (`None`: the
    /// fastest deterministic one, [`ti4_infer::Backend::best`]).
    ///
    /// # Errors
    /// A message when the file is not read or the model is not one this build can play.
    pub fn from_bytes(bytes: &[u8], backend: Option<ti4_infer::Backend>) -> Result<Self, String> {
        let model = ti4_infer::Model::from_bytes(bytes).map_err(|error| error.to_string())?;
        let model = match backend {
            Some(backend) => model
                .with_backend(backend)
                .map_err(|error| error.to_string())?,
            None => model,
        };
        Self::new(model)
    }

    /// The name: the `<name>` of the kind `mlp:<name>`.
    #[must_use]
    pub fn name(&self) -> &str {
        self.0.model.name()
    }

    /// The label shown in the lobby.
    #[must_use]
    pub fn label(&self) -> &str {
        self.0.model.label()
    }

    /// The SHA-256 of the file.
    #[must_use]
    pub fn digest(&self) -> &str {
        self.0.model.digest()
    }

    /// The model itself.
    #[must_use]
    pub fn model(&self) -> &ti4_infer::Model {
        &self.0.model
    }

    /// The kind of a seat played by this model: `mlp:<name>`.
    #[must_use]
    pub fn kind_id(&self) -> String {
        format!("{MLP_PREFIX}{}", self.name())
    }

    /// The model as the setup options list it.
    #[must_use]
    pub fn entry(&self) -> ModelEntry {
        ModelEntry {
            name: self.name().to_owned(),
            label: self.label().to_owned(),
        }
    }
}

impl PartialEq for MlpModel {
    fn eq(&self, other: &Self) -> bool {
        self.name() == other.name() && self.digest() == other.digest()
    }
}

impl Eq for MlpModel {}

impl fmt::Debug for MlpModel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MlpModel")
            .field("name", &self.name())
            .field("digest", &self.digest())
            .field("backend", &self.0.model.backend())
            .finish()
    }
}

impl ModelInfo for MlpModel {
    fn layout(&self) -> HeadLayout {
        self.0.layout
    }

    fn battle_predictor(&self) -> Option<&BattlePredictor> {
        self.0.battle.as_ref()
    }

    fn temperature(&self) -> f64 {
        f64::from(self.0.model.temperature())
    }
}

impl Scorer for MlpModel {
    fn logits(
        &self,
        faction: usize,
        head: usize,
        options: &[Columns],
    ) -> Result<Vec<f32>, MlpError> {
        let mut out = Vec::with_capacity(options.len());
        SCRATCH
            .with_borrow_mut(|scratch| {
                self.0
                    .model
                    .forward_with(scratch, faction, head, options, &mut out)
            })
            .map_err(|error| MlpError::Model(error.to_string()))?;
        Ok(out)
    }
}

/// A model as a host lists it for a table: its name and its label. The page lists the models of
/// the server before it has loaded any.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ModelEntry {
    /// The `<name>` of the kind `mlp:<name>`.
    pub name: String,
    /// What the lobby shows.
    pub label: String,
}

// --- the registry ------------------------------------------------------------------------------

static MODELS: Mutex<BTreeMap<String, MlpModel>> = Mutex::new(BTreeMap::new());

/// Load `model` into the registry of this process, in place of a model of the same name. A seat
/// that plays the old one keeps it.
pub fn register(model: MlpModel) {
    MODELS
        .lock()
        .expect("models lock")
        .insert(model.name().to_owned(), model);
}

/// The model of `name` in the registry.
#[must_use]
pub fn loaded(name: &str) -> Option<MlpModel> {
    MODELS.lock().expect("models lock").get(name).cloned()
}

/// Every model in the registry, by name.
#[must_use]
pub fn models() -> Vec<MlpModel> {
    MODELS
        .lock()
        .expect("models lock")
        .values()
        .cloned()
        .collect()
}

// --- the kinds ---------------------------------------------------------------------------------

/// The kind of a seat that a host plays itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BotKind {
    /// The rule bot ([`ti4_rule_bot`]): its heuristics where it has an opinion, else random.
    Smart,
    /// A seat that decides at random among what the game allows.
    Random,
    /// An MLP model.
    Mlp(MlpModel),
}

impl BotKind {
    /// The id of the kind as a table names it: `smart`, `random` or `mlp:<name>`.
    #[must_use]
    pub fn id(&self) -> String {
        match self {
            Self::Smart => bot_kind::RULE.to_owned(),
            Self::Random => bot_kind::RANDOM.to_owned(),
            Self::Mlp(model) => model.kind_id(),
        }
    }

    /// The kind of `id`, its model found by `model`.
    ///
    /// # Errors
    /// A message for an id that is no kind, or that names a model `model` does not have (the
    /// message names the model).
    pub fn parse_with(
        id: &str,
        model: impl FnOnce(&str) -> Option<MlpModel>,
    ) -> Result<Self, String> {
        match id {
            bot_kind::RULE => Ok(Self::Smart),
            bot_kind::RANDOM => Ok(Self::Random),
            _ => {
                let name = model_name(id).ok_or_else(|| format!("no bot kind '{id}'"))?;
                model(name)
                    .map(Self::Mlp)
                    .ok_or_else(|| format!("the model '{name}' is not loaded"))
            }
        }
    }

    /// The kind of `id`, its model from the registry ([`loaded`]).
    ///
    /// # Errors
    /// As [`Self::parse_with`].
    pub fn parse(id: &str) -> Result<Self, String> {
        Self::parse_with(id, loaded)
    }
}

/// The name of the model of the kind `mlp:<name>`; `None` for any other kind.
#[must_use]
pub fn model_name(id: &str) -> Option<&str> {
    id.strip_prefix(MLP_PREFIX).filter(|name| !name.is_empty())
}

impl Serialize for BotKind {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.id())
    }
}

impl<'de> Deserialize<'de> for BotKind {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let id = String::deserialize(deserializer)?;
        Self::parse(&id).map_err(serde::de::Error::custom)
    }
}

// --- a seat ------------------------------------------------------------------------------------

/// One seat that a host plays itself, with what it keeps between its decisions.
#[derive(Debug)]
pub enum Bot {
    /// [`BotKind::Smart`].
    Smart,
    /// [`BotKind::Random`].
    Random,
    /// [`BotKind::Mlp`]: the model and the pipeline of the seat.
    Mlp(Box<MlpSeat>),
}

/// The model of an MLP seat and its decision pipeline.
#[derive(Debug)]
pub struct MlpSeat {
    model: MlpModel,
    pipeline: Pipeline,
}

impl Bot {
    /// The seat of `player` played by `kind`. `initial` is the game as it starts (before its
    /// first decision), on `galaxy`: an MLP seat measures its progress against what the seat
    /// holds there. A faction that the models have no row for is played by the rule bot.
    #[must_use]
    pub fn new(
        kind: &BotKind,
        initial: &GameState,
        galaxy: Option<&Galaxy>,
        player: &PlayerId,
    ) -> Self {
        let initial = Observed::new(
            initial,
            ti4_content::ContentStore::embedded(),
            ti4_model::content_types::POK,
            galaxy,
        );
        let initial = &initial;
        match kind {
            BotKind::Smart => Self::Smart,
            BotKind::Random => Self::Random,
            BotKind::Mlp(model) => {
                let row = initial
                    .seat(player)
                    .and_then(|seat| ti4_policy::mlp::faction_row(seat.faction.as_str()));
                let Some(row) = row else {
                    return Self::Smart;
                };
                let mut pipeline = Pipeline::new(Arc::clone(&model.0.vocabulary), row);
                pipeline.set_baseline(ti4_policy::progress::Baseline::taken(initial, player));
                Self::Mlp(Box::new(MlpSeat {
                    model: model.clone(),
                    pipeline,
                }))
            }
        }
    }

    /// The answer of the seat to its decision number `index` of the game with `game_seed`
    /// ([`choose`]).
    pub fn choose(
        &mut self,
        choice: &Choice,
        seen: Option<&SeatObservation<'_>>,
        game_seed: u64,
        place: usize,
        index: usize,
    ) -> (usize, ChaCha8Rng) {
        choose(self, choice, seen, game_seed, place, index)
    }

    /// A decision of the seat that was made before, given again with its answer `chosen` (a
    /// game played again from its log): the seat keeps what the decision made it keep. Nothing
    /// for a seat that keeps nothing.
    pub fn follow(
        &mut self,
        choice: &Choice,
        seen: Option<&SeatObservation<'_>>,
        chosen: &ChoiceOption,
        game_seed: u64,
        place: usize,
        index: usize,
    ) {
        let (Self::Mlp(seat), Some(seen)) = (self, seen) else {
            return;
        };
        let mut rng = ti4_rule_bot::random::decision_rng(game_seed, place, index);
        let temperature = seat.model.temperature();
        // A decision that failed when it was made fell back to the rule bot, and keeps nothing
        // more here.
        let _ = seat
            .pipeline
            .follow(&seat.model, choice, seen, chosen, temperature, &mut rng);
        seat.pipeline.take_trace();
    }

    /// What the seat keeps that its later decisions depend on; `None` for a seat that keeps
    /// nothing.
    #[must_use]
    pub fn carry(&self) -> Option<Carry> {
        match self {
            Self::Mlp(seat) => Some(seat.pipeline.carry()),
            Self::Smart | Self::Random => None,
        }
    }

    /// Put back a [`Self::carry`] of this seat.
    pub fn restore(&mut self, carry: Option<Carry>) {
        if let (Self::Mlp(seat), Some(carry)) = (self, carry) {
            seat.pipeline.restore(carry);
        }
    }

    /// Another seat of the same kind with `carry` (of this seat): a copy of the game is played on
    /// by it without changing this one.
    #[must_use]
    pub fn detached(&self, carry: Option<Carry>) -> Self {
        match self {
            Self::Smart => Self::Smart,
            Self::Random => Self::Random,
            Self::Mlp(seat) => {
                let mut pipeline = Pipeline::new(
                    Arc::clone(&seat.model.0.vocabulary),
                    seat.pipeline.faction(),
                );
                pipeline.set_baseline(seat.pipeline.baseline());
                if let Some(carry) = carry {
                    pipeline.restore(carry);
                }
                Self::Mlp(Box::new(MlpSeat {
                    model: seat.model.clone(),
                    pipeline,
                }))
            }
        }
    }
}

/// The answer of a seat to its decision number `index` of the game with `game_seed` (decisions
/// of every seat counted, from 0): the index of one of the options, and the generator after its
/// draws (for the think time of a host). `place` is the place of the seat in the seat order of
/// the game, from 0. The choice must have at least one option.
///
/// - [`Bot::Smart`] is [`ti4_rule_bot::choose`] with `seen`; [`Bot::Random`] without it.
/// - [`Bot::Mlp`] runs the decision pipeline on the model at its temperature, drawing from
///   [`ti4_rule_bot::random::decision_rng`]. Without `seen`, or when the pipeline or the model
///   fails, the rule bot answers.
pub fn choose(
    bot: &mut Bot,
    choice: &Choice,
    seen: Option<&SeatObservation<'_>>,
    game_seed: u64,
    place: usize,
    index: usize,
) -> (usize, ChaCha8Rng) {
    match bot {
        Bot::Smart => ti4_rule_bot::choose(choice, seen, game_seed, place, index),
        Bot::Random => ti4_rule_bot::choose(choice, None, game_seed, place, index),
        Bot::Mlp(seat) => {
            let Some(seen) = seen else {
                return ti4_rule_bot::choose(choice, None, game_seed, place, index);
            };
            let mut rng = ti4_rule_bot::random::decision_rng(game_seed, place, index);
            let temperature = seat.model.temperature();
            let decided = seat
                .pipeline
                .decide(&seat.model, choice, seen, temperature, &mut rng);
            seat.pipeline.take_trace();
            match decided.ok().and_then(|option| {
                choice
                    .options
                    .iter()
                    .position(|offered| offered.id == option.id)
            }) {
                Some(picked) => (picked, rng),
                None => ti4_rule_bot::choose(choice, Some(seen), game_seed, place, index),
            }
        }
    }
}

/// A small model for the tests of both hosts, the same on every target: a synthetic model
/// ([`ti4_infer::synthetic::parts`], width 16, one residual block) with a vocabulary of the
/// reserved columns that also places the power facts and the deal value (so the table of an
/// activation is kept), and a battle predictor of fact version 8 (so an activation is followed
/// by a fleet decision and its plan), played at temperature 1 (so that its seats also choose the
/// fleets of the menu). Named `test`, labelled `Test model`.
///
/// # Panics
/// Never for this build: the parts are fixed.
#[must_use]
pub fn test_model_bytes() -> Vec<u8> {
    use ti4_policy::battle::{GROUND_INPUT_WIDTH, GROUND_OUTPUT_WIDTH, input_width, output_width};
    let names = ti4_policy::power_facts::NAMES
        .iter()
        .copied()
        .chain([ti4_policy::deal_value::FACT_SCORE]);
    let vocabulary = Vocabulary::build(names).expect("a vocabulary of fixed names");
    let mut parts = ti4_infer::synthetic::parts(0x7e57, 16, vocabulary.capacity(), 1);
    parts.slots = vocabulary.to_json().expect("a vocabulary writes");
    let version = 8;
    let (input, output) = (input_width(version), output_width(version));
    let space = vec![
        (input, 2, vec![0.01; input * 2], vec![0.0, 0.5]),
        (2, output, vec![0.2; 2 * output], vec![0.0; output]),
    ];
    let ground = vec![
        (
            GROUND_INPUT_WIDTH,
            1,
            vec![0.3; GROUND_INPUT_WIDTH],
            vec![0.0],
        ),
        (
            1,
            GROUND_OUTPUT_WIDTH,
            vec![1.0; GROUND_OUTPUT_WIDTH],
            vec![0.0; GROUND_OUTPUT_WIDTH],
        ),
    ];
    let predictor = BattlePredictor::assemble(version, "test", space, ground)
        .expect("a predictor of fixed layers");
    parts.battle = Some(predictor.to_json().expect("a predictor writes"));
    // A model with a battle predictor is of schema 11 or 12.
    parts.source.schema = 12;
    let meta = ti4_infer::Meta {
        name: "test".to_owned(),
        label: "Test model".to_owned(),
        temperature: 1.0,
    };
    ti4_infer::format::write(&meta, &parts).expect("the test model packs")
}

/// [`test_model_bytes`] loaded on the scalar backend and put in the registry, once per process.
#[must_use]
pub fn test_model() -> MlpModel {
    static TEST: std::sync::OnceLock<MlpModel> = std::sync::OnceLock::new();
    TEST.get_or_init(|| {
        let model = MlpModel::from_bytes(&test_model_bytes(), Some(ti4_infer::Backend::Scalar))
            .expect("the test model loads");
        register(model.clone());
        model
    })
    .clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_kind_is_written_and_read_as_its_id() {
        let model = test_model();
        for kind in [BotKind::Smart, BotKind::Random, BotKind::Mlp(model.clone())] {
            let text = serde_json::to_string(&kind).unwrap();
            assert_eq!(text, format!("\"{}\"", kind.id()));
            assert_eq!(serde_json::from_str::<BotKind>(&text).unwrap(), kind);
        }
        assert_eq!(BotKind::Mlp(model).id(), "mlp:test");
        let error = BotKind::parse("mlp:nobody").unwrap_err();
        assert!(error.contains("'nobody'"), "{error}");
        assert!(BotKind::parse("mlp").is_err());
        assert!(BotKind::parse("mlp:").is_err());
        assert!(serde_json::from_str::<BotKind>("\"mlp:nobody\"").is_err());
    }

    #[test]
    fn the_test_model_plays_the_pipeline_with_its_predictor_and_power_facts() {
        let model = test_model();
        assert!(model.battle_predictor().is_some());
        assert_eq!(model.layout(), HeadLayout::Legacy);
        assert!(
            model
                .0
                .vocabulary
                .is_assigned(ti4_policy::power_facts::NAMES[0])
        );
        assert_eq!(model.model().backend(), ti4_infer::Backend::Scalar);
    }
}
