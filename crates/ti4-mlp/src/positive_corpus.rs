//! A permanent corpus of opening trajectories that cleared, one file per faction.
//!
//! # What is stored, and why it is not features
//!
//! A trajectory is stored as its **specification** — seed, rotation, faction, and the option id
//! chosen at every non-forced decision — never as a dump of feature vectors.
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
//! # What this corpus deliberately does not decide
//!
//! Which trajectories are *worth more*. Difficulty weighting, per-faction balancing and rescued
//! successes from positions the policy currently fails are all consumer concerns; the outcome slack
//! each trajectory records is there so a consumer can compute them. A corpus that pre-weighted its
//! own contents would have to be regenerated every time the weighting changed.

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
    /// Kept so a consumer can prefer trajectories that cleared with slack over ones that met every
    /// condition at the last possible moment. The corpus records the facts and weights nothing.
    pub planets: usize,
    pub systems: usize,
    pub units_ok: bool,
    /// The option id chosen at every non-forced decision, in order.
    pub actions: Vec<String>,
}

/// Everything that makes a corpus line unusable.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CorpusError {
    #[error("corpus line {line} has {fields} fields, expected at least 7")]
    ShortLine { line: usize, fields: usize },
    #[error("corpus line {line}: {field} is not a number")]
    NotANumber { line: usize, field: &'static str },
    #[error("corpus line {line} declares {declared} actions and carries {carried}")]
    ActionCount {
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
    for action in &trajectory.actions {
        if action.chars().any(char::is_whitespace) {
            return Err(CorpusError::UnstorableId(action.clone()));
        }
    }
    if trajectory.faction.chars().any(char::is_whitespace) {
        return Err(CorpusError::UnstorableId(trajectory.faction.clone()));
    }
    Ok(format!(
        "{} {} {} {} {} {} {} {}",
        trajectory.seed,
        trajectory.rotation,
        trajectory.faction,
        trajectory.planets,
        trajectory.systems,
        u8::from(trajectory.units_ok),
        trajectory.actions.len(),
        trajectory.actions.join(" ")
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
    if fields.len() < 7 {
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
    let rotation = usize::try_from(number(1, "rotation")?)
        .map_err(|_| CorpusError::NotANumber { line, field: "rotation" })?;
    let faction = fields[2].to_owned();
    let planets = usize::try_from(number(3, "planets")?)
        .map_err(|_| CorpusError::NotANumber { line, field: "planets" })?;
    let systems = usize::try_from(number(4, "systems")?)
        .map_err(|_| CorpusError::NotANumber { line, field: "systems" })?;
    let units_ok = number(5, "units_ok")? != 0;
    let declared = usize::try_from(number(6, "actions")?)
        .map_err(|_| CorpusError::NotANumber { line, field: "actions" })?;
    let actions: Vec<String> = fields[7..].iter().map(|piece| (*piece).to_owned()).collect();
    if actions.len() != declared {
        return Err(CorpusError::ActionCount {
            line,
            declared,
            carried: actions.len(),
        });
    }
    Ok(Trajectory {
        seed,
        rotation,
        faction,
        planets,
        systems,
        units_ok,
        actions,
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

#[cfg(test)]
mod tests {
    use super::{CorpusError, Note, TACTICAL_ACTION_ID, Trajectory, read_all, read_line, wasted_activations, write_line};

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
    fn a_trajectory_survives_the_round_trip() {
        let trajectory = Trajectory {
            seed: 800_000_123,
            rotation: 3,
            faction: "jolnar".to_owned(),
            planets: 4,
            systems: 3,
            units_ok: true,
            actions: vec!["tactical".to_owned(), "system-42".to_owned()],
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
        let error = read_line(7, "800000123 3 jolnar 4 3 1 5 tactical system-42").expect_err("short");
        assert_eq!(
            error,
            CorpusError::ActionCount {
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
            planets: 3,
            systems: 3,
            units_ok: true,
            actions: vec!["two words".to_owned()],
        };
        assert_eq!(
            write_line(&trajectory).expect_err("refused"),
            CorpusError::UnstorableId("two words".to_owned())
        );
    }
}
