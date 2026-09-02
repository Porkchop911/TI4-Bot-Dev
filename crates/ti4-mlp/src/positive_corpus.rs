//! A permanent corpus of opening trajectories that cleared, one file per faction.
//!
//! # What is stored, and why it is not features
//!
//! A trajectory is stored as its **specification** — seed, rotation, faction, and the option id
//! chosen at every non-forced decision — never as a dump of feature vectors.
//!
//! # Decisions are not actions
//!
//! An *action* in TI4 is a defined thing: on a turn a player takes exactly one, and it is a
//! tactical, strategic or component action. A *decision* is a question the engine asks. One
//! tactical action produces many decisions — which system to activate, which ships to move, what to
//! load, where to commit ground forces. This module counts and stores **decisions**, and reports the
//! **action** count separately, because conflating them makes a length figure unreadable to anyone
//! who knows the game.
//!
//! A **transaction is not an action either**. 94.1a makes it free and the turn continues, so it is
//! offered among the turn options without being one. [`actions_taken`] excludes them.
//!
//! The engine is deterministic given a seed, so replaying those ids reproduces the exact game and
//! the exact decisions, and the features can be recomputed on demand under whatever model is being
//! trained. A feature dump would be roughly 300 million numbers for 300 seeds, would be pinned to
//! today's vocabulary generation, and would be stale the moment the projection changed. The
//! specification is a few megabytes and stays true as long as the engine and the map pool do.
//!
//! Option **ids** rather than positions, so a replay can assert the recorded action is actually on
//! offer and fail closed if anything has drifted. A position would silently select a different
//! action instead.
//!
//! # The temperature is part of the specification
//!
//! A trajectory is a line played *against five particular opponents*. Those opponents are the same
//! policy sampling at the generating temperature, and their stream is offset by it, so replaying at
//! any other temperature gives different opponents, a different game, and recorded ids that are no
//! longer on offer. The first version of this format omitted the temperature and 59 of 60 replays
//! failed for exactly that reason.
//!
//! It is stored in thousandths as an integer, because the value also seeds the stream offset and a
//! decimal that did not round-trip exactly would reproduce different opponents while looking right.
//!
//! # The quality filter
//!
//! Clearing the bar is necessary and not sufficient. A trajectory is rejected when it contains a
//! **wasted activation**: a tactical action that activated a system and then neither moved a ship
//! nor produced anything. That is a turn spent placing a command token, and cloning it teaches the
//! policy to spend turns.
//!
//! The segmentation is per seat and comes from the seat's own decision stream rather than the event
//! log, which carries names without attribution and cannot say whose activation it was.
//!
//! # Admission is binary
//!
//! A trajectory that cleared the bar without a wasted activation is a good example. There is no
//! further quality ranking: overshooting the bar does not make a demonstration better, and an
//! earlier version of this module reported a "cleared with slack" share as though it did.
//!
//! The final planets, systems and composition are still stored, because they are free facts about
//! an admitted trajectory and a consumer may want them. They are not admission criteria and nothing
//! here weights by them. Difficulty weighting, per-faction balancing and rescued successes are
//! likewise consumer concerns — a corpus that pre-weighted its own contents would have to be
//! regenerated whenever the weighting changed.

use std::collections::BTreeMap;

/// One decision as the filter sees it.
///
/// Deliberately not the feature vector: this is about what the seat *did*, and the filter needs the
/// option's identity and whether it was a refusal, neither of which a sparse vector carries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Note {
    /// The schema head this decision routed to.
    pub head: String,
    /// The id of the option taken.
    pub chosen: String,
    /// Whether that option was the decline.
    pub declined: bool,
}

/// The option id that begins a tactical action.
///
/// Mirrors `ti4_engine::game::TACTICAL_ACTION_ID`. Stated here as a constant with this comment
/// rather than imported, because `ti4-mlp` does not otherwise depend on the engine's action ids and
/// a silent divergence would make every segment boundary wrong.
pub const TACTICAL_ACTION_ID: &str = "tactical";

/// How many of a seat's tactical actions activated a system and then did nothing with it.
///
/// A tactical action opens when the seat takes the tactical option at its turn decision and runs
/// until the next turn decision. Within one, the seat is credited with having done something if it
/// moved, built, or **landed ground forces** — a `movement`, `production` or `landing` decision
/// that was not declined. An activation with none of the three is wasted.
///
/// Landing is not an afterthought in that list. A seat may activate a system its ships already
/// occupy and invade without moving anything, which takes planets and is the whole point of the
/// opening. Counting only movement and production called two thirds of all clearing trajectories
/// wasteful, which was the filter being wrong rather than the policy.
///
/// `cargo` is deliberately excluded: loading troops is preparation, and an activation that only
/// loaded moved nothing and took nothing.
///
/// Decisions before the first tactical action belong to no segment and are ignored, which is what
/// makes a strategy pick or a setup choice unable to open one by accident.
/// The prefix of a turn option that opens a transaction rather than taking an action.
///
/// Mirrors `ti4_engine::transactions::OPEN_PREFIX`.
pub const TRANSACTION_PREFIX: &str = "component|trade|";

/// How many TI4 actions a seat took.
///
/// An action is tactical, strategic or component, and a turn is one of them. A **transaction is not
/// an action**: LRR 94.1a makes it free, the engine models it as free — "closing it does not end the
/// turn" — and the seat is asked again afterwards. So a transaction appears among the turn options
/// without consuming the turn, and counting every turn decision overcounts by exactly the number of
/// transactions.
///
/// That error is not small. Hacan's Guild Ships makes it neighbours with every player, so it may
/// transact with all five each turn; counting those made it read as 21.7 actions per round against
/// 5.4 for L1Z1X, and look like a policy stuck in a loop. Its real figure is about 4.3, the same as
/// everyone else, and the seventeen transactions are legal and free.
#[must_use]
pub fn actions_taken(notes: &[Note]) -> usize {
    notes
        .iter()
        .filter(|note| note.head == "turn" && !note.chosen.starts_with(TRANSACTION_PREFIX))
        .count()
}

#[must_use]
pub fn wasted_activations(notes: &[Note]) -> usize {
    let mut wasted = 0usize;
    let mut open = false;
    let mut activated = false;
    let mut acted = false;

    // A segment is closed by the next turn decision or by the end of the trajectory, so the same
    // three lines run in both places.
    let close = |activated: bool, acted: bool, wasted: &mut usize| {
        if activated && !acted {
            *wasted += 1;
        }
    };

    for note in notes {
        if note.head == "turn" {
            if open {
                close(activated, acted, &mut wasted);
            }
            open = note.chosen == TACTICAL_ACTION_ID;
            activated = false;
            acted = false;
            continue;
        }
        if !open {
            continue;
        }
        match note.head.as_str() {
            "activation" => activated = true,
            "movement" | "production" | "landing" => {
                if !note.declined {
                    acted = true;
                }
            }
            _ => {}
        }
    }
    if open {
        close(activated, acted, &mut wasted);
    }
    wasted
}

/// One stored trajectory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Trajectory {
    pub seed: u64,
    pub rotation: usize,
    pub faction: String,
    /// Planets gained, systems held and whether fleet composition was met, at the end of the round.
    ///
    /// Free facts about a trajectory that has already been admitted. Not admission criteria: every
    /// trajectory here cleared the bar, and clearing it by more does not make the demonstration
    /// better.
    pub planets: usize,
    pub systems: usize,
    pub units_ok: bool,
    /// The sampling temperature this line was played at, in thousandths.
    ///
    /// Needed to reproduce the other five seats: they sample at this temperature and their stream is
    /// offset by it. Without it a replay faces different opponents and the line does not exist.
    pub temperature_milli: u64,
    /// How many TI4 **actions** the seat took — tactical, strategic and component together.
    ///
    /// The game's own unit of a turn, counted from the seat's turn decisions. Stored because it is
    /// the figure a reader of this corpus will expect the word "action" to mean.
    pub actions: usize,
    /// The option id chosen at every non-forced **decision**, in order. Many per action.
    pub decisions: Vec<String>,
}

/// Everything that makes a corpus line unusable.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CorpusError {
    #[error("corpus line {line} has {fields} fields, expected at least 9")]
    ShortLine { line: usize, fields: usize },
    #[error("corpus line {line}: {field} is not a number")]
    NotANumber { line: usize, field: &'static str },
    #[error("corpus line {line} declares {declared} decisions and carries {carried}")]
    DecisionCount {
        line: usize,
        declared: usize,
        carried: usize,
    },
    /// An id with whitespace would make the line unparseable on the way back in.
    #[error("option id {0:?} contains whitespace and cannot be stored")]
    UnstorableId(String),
}

/// Serialise one trajectory as a single line.
///
/// Space-separated, because option ids are engine aliases and carry no whitespace — which is
/// checked rather than assumed, since a corpus that cannot be read back is worse than one that
/// refuses to be written.
///
/// # Errors
/// [`CorpusError::UnstorableId`] if any id contains whitespace.
pub fn write_line(trajectory: &Trajectory) -> Result<String, CorpusError> {
    for decision in &trajectory.decisions {
        if decision.chars().any(char::is_whitespace) {
            return Err(CorpusError::UnstorableId(decision.clone()));
        }
    }
    if trajectory.faction.chars().any(char::is_whitespace) {
        return Err(CorpusError::UnstorableId(trajectory.faction.clone()));
    }
    Ok(format!(
        "{} {} {} {} {} {} {} {} {} {}",
        trajectory.seed,
        trajectory.rotation,
        trajectory.faction,
        trajectory.temperature_milli,
        trajectory.planets,
        trajectory.systems,
        u8::from(trajectory.units_ok),
        trajectory.actions,
        trajectory.decisions.len(),
        trajectory.decisions.join(" ")
    ))
}

/// Parse one line back.
///
/// # Errors
/// [`CorpusError`] for a line that is short, malformed, or whose action count does not match what
/// it declared — the last of which is the check that makes a truncated file an error rather than a
/// quietly shorter corpus.
pub fn read_line(line: usize, text: &str) -> Result<Trajectory, CorpusError> {
    let fields: Vec<&str> = text.split_whitespace().collect();
    if fields.len() < 9 {
        return Err(CorpusError::ShortLine {
            line,
            fields: fields.len(),
        });
    }
    let number = |index: usize, name: &'static str| -> Result<u64, CorpusError> {
        fields[index]
            .parse()
            .map_err(|_| CorpusError::NotANumber { line, field: name })
    };
    let seed = number(0, "seed")?;
    let rotation =
        usize::try_from(number(1, "rotation")?).map_err(|_| CorpusError::NotANumber {
            line,
            field: "rotation",
        })?;
    let faction = fields[2].to_owned();
    let temperature_milli = number(3, "temperature")?;
    let planets = usize::try_from(number(4, "planets")?).map_err(|_| CorpusError::NotANumber {
        line,
        field: "planets",
    })?;
    let systems = usize::try_from(number(5, "systems")?).map_err(|_| CorpusError::NotANumber {
        line,
        field: "systems",
    })?;
    let units_ok = number(6, "units_ok")? != 0;
    let actions = usize::try_from(number(7, "actions")?).map_err(|_| CorpusError::NotANumber {
        line,
        field: "actions",
    })?;
    let declared =
        usize::try_from(number(8, "decisions")?).map_err(|_| CorpusError::NotANumber {
            line,
            field: "decisions",
        })?;
    let decisions: Vec<String> = fields[9..]
        .iter()
        .map(|piece| (*piece).to_owned())
        .collect();
    if decisions.len() != declared {
        return Err(CorpusError::DecisionCount {
            line,
            declared,
            carried: decisions.len(),
        });
    }
    Ok(Trajectory {
        seed,
        rotation,
        faction,
        temperature_milli,
        planets,
        systems,
        units_ok,
        actions,
        decisions,
    })
}

/// Read a whole corpus file, keyed by faction.
///
/// # Errors
/// The first malformed line, with its number.
pub fn read_all(text: &str) -> Result<BTreeMap<String, Vec<Trajectory>>, CorpusError> {
    let mut by_faction: BTreeMap<String, Vec<Trajectory>> = BTreeMap::new();
    for (index, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let trajectory = read_line(index + 1, line)?;
        by_faction
            .entry(trajectory.faction.clone())
            .or_default()
            .push(trajectory);
    }
    Ok(by_faction)
}

/// One demonstrated decision, as behaviour cloning sees it.
///
/// Built by replaying a stored trajectory: the options are the features of the position the
/// demonstration actually reached, and `chosen` is what the successful line did there.
#[derive(Clone, Debug)]
pub struct Demo {
    pub row: crate::FactionRow,
    pub head: usize,
    pub options: Vec<crate::SparseOption>,
    pub chosen: usize,
    /// Relative weight of this decision in the batch.
    ///
    /// Averaging the loss over *decisions* is not the same as balancing over trajectories, and the
    /// first version of this conflated them. Hacan runs 79.5 decisions a trajectory against L1Z1X's
    /// 43.8 -- almost entirely free transactions -- so an equal number of trajectories delivered
    /// 1.8x the gradient for Hacan while the trainer reported itself balanced.
    ///
    /// The weight is set by the caller, which knows the hierarchy it wants: `1 / decisions` makes a
    /// trajectory count once however long it is, a further `1 / trajectories` per starting position
    /// stops a position with forty discovered solutions outweighing one with a single solution, and
    /// a further `1 / trajectories` per faction restores balance across factions.
    pub weight: f64,
}

/// Weighted mean negative log-likelihood of the demonstrated actions.
///
/// Plain behaviour cloning: `-(1/N) Σ log π(a_i | s_i)`. A one-hot target is the right shape here
/// and was the wrong shape for the repair objective, and the difference is the state distribution
/// rather than the loss. Repair labelled 0.4% of the decisions, all of them positions where the
/// policy was known to be wrong, so nothing constrained the other 99.6% and the trunk was free to
/// rewrite it. These demonstrations cover whole trajectories of ordinary successful play, so the
/// states where the policy is already right supply their own targets — which say "keep doing this"
/// and hold the rest of the distribution in place as a learning signal rather than a brake.
///
/// Returns `None` for an empty batch rather than a zero tensor.
///
/// # Errors
/// If the actor cannot score an option set, or a target is outside it.
pub fn clone_loss(
    actor: &crate::Actor,
    demos: &[Demo],
) -> Result<Option<ti4_tensor::Tensor>, String> {
    if demos.is_empty() {
        return Ok(None);
    }
    let mut total: Option<ti4_tensor::Tensor> = None;
    let mut mass = 0.0f64;
    for demo in demos {
        if !demo.weight.is_finite() || demo.weight < 0.0 {
            return Err(format!("a demonstration carries weight {}", demo.weight));
        }
        let head = crate::heads()
            .get(demo.head)
            .ok_or_else(|| format!("head index {} is out of range", demo.head))?;
        let logits = actor
            .logits(&demo.options, head, demo.row)
            .map_err(|error| format!("cloning scoring failed: {error}"))?;
        let chosen = i64::try_from(demo.chosen).map_err(|_| "chosen index does not fit i64")?;
        if demo.chosen >= demo.options.len() {
            return Err(format!(
                "demonstrated option {} is outside the {} offered",
                demo.chosen,
                demo.options.len()
            ));
        }
        let log_probs = logits.log_softmax(0, ti4_tensor::Kind::Float);
        let term = -log_probs.narrow(0, chosen, 1).squeeze() * demo.weight;
        mass += demo.weight;
        total = Some(match total {
            Some(sum) => sum + term,
            None => term,
        });
    }
    if mass <= 0.0 {
        return Err("the demonstration batch carries no weight".to_owned());
    }
    Ok(total.map(|sum| sum / mass))
}

#[cfg(test)]
mod tests {
    use super::{
        CorpusError, Note, TACTICAL_ACTION_ID, Trajectory, actions_taken, read_all, read_line,
        wasted_activations, write_line,
    };

    fn note(head: &str, chosen: &str, declined: bool) -> Note {
        Note {
            head: head.to_owned(),
            chosen: chosen.to_owned(),
            declined,
        }
    }

    fn tactical() -> Note {
        note("turn", TACTICAL_ACTION_ID, false)
    }

    #[test]
    fn an_activation_that_moved_nothing_and_built_nothing_is_wasted() {
        let wasted_line = vec![
            tactical(),
            note("activation", "system-42", false),
            note("movement", "decline", true),
            note("production", "decline", true),
        ];
        assert_eq!(wasted_activations(&wasted_line), 1);
    }

    #[test]
    fn moving_or_building_redeems_the_activation() {
        let moved = vec![
            tactical(),
            note("activation", "system-42", false),
            note("movement", "carrier-a", false),
            note("production", "decline", true),
        ];
        assert_eq!(wasted_activations(&moved), 0);

        let built = vec![
            tactical(),
            note("activation", "system-42", false),
            note("movement", "decline", true),
            note("production", "infantry", false),
        ];
        assert_eq!(wasted_activations(&built), 0);
    }

    #[test]
    fn landing_without_moving_is_a_productive_activation() {
        // A seat can activate a system its ships already hold and invade without moving anything.
        // That takes planets, which is what the opening bar is made of. An earlier version of this
        // filter counted only movement and production and rejected two thirds of all clearing
        // trajectories for it.
        let notes = vec![
            tactical(),
            note("activation", "system-7", false),
            note("movement", "decline", true),
            note("landing", "infantry-a", false),
            note("production", "decline", true),
        ];
        assert_eq!(wasted_activations(&notes), 0);
    }

    #[test]
    fn loading_alone_does_not_redeem_an_activation() {
        // Cargo is preparation. An activation that only loaded moved nothing and took nothing.
        let notes = vec![
            tactical(),
            note("activation", "system-7", false),
            note("cargo", "infantry-a", false),
            note("movement", "decline", true),
        ];
        assert_eq!(wasted_activations(&notes), 1);
    }

    #[test]
    fn the_segment_ends_at_the_next_turn_decision() {
        // Two tactical actions, the first productive and the second wasted. A filter that did not
        // close the segment would credit the second with the first's movement and pass a trajectory
        // it should reject.
        let notes = vec![
            tactical(),
            note("activation", "system-1", false),
            note("movement", "carrier-a", false),
            tactical(),
            note("activation", "system-2", false),
            note("movement", "decline", true),
        ];
        assert_eq!(wasted_activations(&notes), 1);
    }

    #[test]
    fn a_non_tactical_turn_does_not_open_a_segment() {
        // A strategic action's production must not be credited to a later activation, and a
        // strategic turn must not itself be counted as a wasted activation.
        let notes = vec![
            note("turn", "warfare", false),
            note("production", "infantry", false),
            note("activation", "system-1", false),
        ];
        assert_eq!(wasted_activations(&notes), 0);
    }

    #[test]
    fn a_trajectory_that_never_activates_is_not_wasteful() {
        let notes = vec![tactical(), note("movement", "decline", true)];
        assert_eq!(wasted_activations(&notes), 0);
        assert_eq!(wasted_activations(&[]), 0);
    }

    #[test]
    fn the_last_segment_is_closed_at_the_end_of_the_trajectory() {
        // The round ends without another turn decision, so a filter that only closed on the next
        // turn would never examine the final tactical action -- and the final one is exactly where
        // a seat that has run out of useful moves spends a token.
        let notes = vec![tactical(), note("activation", "system-9", false)];
        assert_eq!(wasted_activations(&notes), 1);
    }

    #[test]
    fn cloning_costs_less_when_the_demonstrated_action_scores_higher() {
        // The contract in one check: the loss is the negative log-likelihood of the demonstrated
        // action, so a uniform distribution over n options must cost exactly ln n, and demonstrating
        // a different option under the same weights must cost the same when the scores are equal.
        let actor = crate::Actor::zeros(crate::Width::W256, 64);
        let options = vec![
            crate::SparseOption {
                columns: vec![1],
                values: vec![1.0],
            },
            crate::SparseOption {
                columns: vec![2],
                values: vec![1.0],
            },
            crate::SparseOption {
                columns: vec![3],
                values: vec![1.0],
            },
        ];
        let row = crate::FactionRow::of("sol").expect("roster");
        let demo = |chosen| super::Demo {
            row,
            head: 0,
            options: options.clone(),
            chosen,
            weight: 1.0,
        };
        let cost = |chosen| {
            f64::try_from(
                super::clone_loss(&actor, &[demo(chosen)])
                    .expect("loss")
                    .expect("some"),
            )
            .expect("scalar")
        };
        let a = cost(0);
        assert!(
            (a - 3.0f64.ln()).abs() < 1e-6,
            "equal scores over three options must cost ln 3, got {a}"
        );
        assert!(
            (a - cost(2)).abs() < 1e-9,
            "equal scores must not prefer a position"
        );
        assert!(super::clone_loss(&actor, &[]).expect("loss").is_none());
    }

    #[test]
    fn weight_decides_how_much_a_demonstration_counts() {
        // The property the faction balance rests on. Two demonstrations of different cost, one
        // weighted to nothing: the batch must read as the other alone. An unweighted mean over
        // decisions cannot express that, which is how a trajectory 1.8x longer came to carry 1.8x
        // the gradient while the trainer called itself balanced across factions.
        let actor = crate::Actor::zeros(crate::Width::W256, 64);
        let column = |c: i64| crate::SparseOption {
            columns: vec![c],
            values: vec![1.0],
        };
        let row = crate::FactionRow::of("sol").expect("roster");
        let cost = |demos: Vec<super::Demo>| {
            f64::try_from(
                super::clone_loss(&actor, &demos)
                    .expect("loss")
                    .expect("some"),
            )
            .expect("scalar")
        };
        let over_two = super::Demo {
            row,
            head: 0,
            options: vec![column(1), column(2)],
            chosen: 0,
            weight: 1.0,
        };
        let over_four = super::Demo {
            row,
            head: 0,
            options: (1..=4).map(column).collect(),
            chosen: 0,
            weight: 1.0,
        };

        // Equal scores throughout, so the costs are exactly ln 2 and ln 4.
        assert!((cost(vec![over_two.clone()]) - 2.0f64.ln()).abs() < 1e-6);
        assert!((cost(vec![over_four.clone()]) - 4.0f64.ln()).abs() < 1e-6);

        let both = cost(vec![over_two.clone(), over_four.clone()]);
        assert!(
            (both - (2.0f64.ln() + 4.0f64.ln()) / 2.0).abs() < 1e-6,
            "equal weights must average the two, got {both}"
        );
        let mut muted = over_four;
        muted.weight = 0.0;
        let only_two = cost(vec![over_two, muted]);
        assert!(
            (only_two - 2.0f64.ln()).abs() < 1e-6,
            "a zero-weighted demonstration must not count, got {only_two}"
        );
    }

    #[test]
    fn a_target_outside_the_option_set_is_refused() {
        let actor = crate::Actor::zeros(crate::Width::W256, 64);
        let error = super::clone_loss(
            &actor,
            &[super::Demo {
                row: crate::FactionRow::of("sol").expect("roster"),
                head: 0,
                options: vec![crate::SparseOption {
                    columns: vec![1],
                    values: vec![1.0],
                }],
                chosen: 4,
                weight: 1.0,
            }],
        )
        .expect_err("refused");
        assert!(error.contains("outside"), "{error}");
    }

    #[test]
    fn an_action_is_a_turn_not_a_decision() {
        // The distinction the corpus exists to keep straight. Four decisions here, one action.
        let notes = vec![
            tactical(),
            note("activation", "system-1", false),
            note("movement", "carrier-a", false),
            note("landing", "infantry-a", false),
        ];
        assert_eq!(actions_taken(&notes), 1);
        assert_eq!(notes.len(), 4);

        // A strategic turn and a faction component turn are actions. A transaction is not: 94.1a
        // makes it free and the turn continues, so it appears among the turn options without being
        // an action. Counting it made Hacan read as 21.7 actions a round instead of about 4.
        let mixed = vec![
            note("turn", "warfare", false),
            note("production", "infantry", false),
            tactical(),
            note("activation", "system-1", false),
            note("turn", "component|trade|letnev", false),
            note("trade", "pnms:sol:0", false),
            note("turn", "faction|orbital_drop", false),
        ];
        assert_eq!(actions_taken(&mixed), 3);
    }

    #[test]
    fn a_trajectory_survives_the_round_trip() {
        let trajectory = Trajectory {
            seed: 800_000_123,
            rotation: 3,
            faction: "jolnar".to_owned(),
            temperature_milli: 500,
            planets: 4,
            systems: 3,
            units_ok: true,
            actions: 3,
            decisions: vec!["tactical".to_owned(), "system-42".to_owned()],
        };
        let line = write_line(&trajectory).expect("writes");
        assert_eq!(read_line(1, &line).expect("reads"), trajectory);

        let corpus = read_all(&format!("# a comment\n\n{line}\n")).expect("reads");
        assert_eq!(corpus["jolnar"], vec![trajectory]);
    }

    #[test]
    fn a_truncated_line_is_an_error_rather_than_a_shorter_trajectory() {
        // The declared count is what makes truncation visible. Without it a half-written line would
        // parse as a complete, shorter demonstration and teach the policy to stop early.
        let error =
            read_line(7, "800000123 3 jolnar 500 4 3 1 3 5 tactical system-42").expect_err("short");
        assert_eq!(
            error,
            CorpusError::DecisionCount {
                line: 7,
                declared: 5,
                carried: 2
            }
        );
        assert!(matches!(
            read_line(1, "800000123 3 jolnar").expect_err("short"),
            CorpusError::ShortLine { .. }
        ));
    }

    #[test]
    fn an_id_that_could_not_be_read_back_is_refused_at_write_time() {
        let trajectory = Trajectory {
            seed: 1,
            rotation: 0,
            faction: "sol".to_owned(),
            temperature_milli: 250,
            planets: 3,
            systems: 3,
            units_ok: true,
            actions: 1,
            decisions: vec!["two words".to_owned()],
        };
        assert_eq!(
            write_line(&trajectory).expect_err("refused"),
            CorpusError::UnstorableId("two words".to_owned())
        );
    }
}
