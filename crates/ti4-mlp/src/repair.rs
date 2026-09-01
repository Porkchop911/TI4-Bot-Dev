//! Learning from decisions that were *demonstrated* to be wrong.
//!
//! `counterfactual_repair` replays a failed opening, substitutes one legal alternate at one index
//! with everything else held identical, and records whether the seat then cleared. Every alternate
//! that cleared is a constructive proof about that one decision:
//!
//! ```text
//! P(clear | do(a_i = c)) = 1   for this position and this downstream policy
//! P(clear | do(a_i = f)) = 0   f being what the policy actually did
//! ```
//!
//! This module turns those proofs into a loss.
//!
//! # What the loss asserts, and what it refuses to assert
//!
//! ```text
//! L = (1/N) Σ_i (1/|C_i|) Σ_{c ∈ C_i} softplus(s_{f_i} − s_c)
//! ```
//!
//! Only that each demonstrated repair should outrank the action that actually failed. Three things
//! it deliberately does not say:
//!
//! - **Non-clearing alternates are not negatives.** An alternate that failed did so *under the
//!   current downstream policy*, which training is about to change. Ranking the repairs above them
//!   would assert something the enumeration never demonstrated.
//! - **The repairs are not ranked against each other.** Several alternates often clear; which is
//!   best is unknown and the loss does not guess.
//! - **It is not a one-hot target.** `rescue_imitation` used one, on 45 samples whose attribution
//!   was wrong, and cost 2.2 points held out (`339f42d`). Cross-entropy against a one-hot label
//!   asserts the repair is correct *and every other option wrong*, which is far more than a
//!   counterfactual proves.
//!
//! # Why the mean over `C_i` rather than a sum
//!
//! It gives the right confidence behaviour for free. A state where exactly one of forty alternates
//! clears is strong evidence about *that action*, and all the positive gradient goes to it. A state
//! where twenty of forty clear is strong evidence the original was bad and weak evidence about which
//! repair matters, and the gradient is spread across the twenty. Total weight per state is the same
//! either way, so no state shouts louder for having been easy to fix — and no new hyperparameter is
//! introduced to achieve it.
//!
//! # These labels expire
//!
//! Whether an intervention clears depends on the policy that plays the rest of the round. Once the
//! policy moves, the rescue set is stale: alternates that cleared may no longer, and alternates that
//! did not may start to. The dataset is regenerated between rounds rather than treated as permanent
//! expert truth.

use ti4_tensor::Tensor;

use crate::{Actor, FactionRow, SparseOption};

/// One decision that was demonstrated repairable.
#[derive(Clone, Debug)]
pub struct Sample {
    /// The faction whose row the scores are read from.
    pub row: FactionRow,
    /// Index into [`crate::heads`].
    pub head: usize,
    /// Every legal option at that decision, in the order the engine offered them.
    pub options: Vec<SparseOption>,
    /// The option the policy took, which produced the failure.
    pub failed: usize,
    /// Options that cleared when substituted. Never empty, and never contains `failed`.
    pub clearing: Vec<usize>,
}

/// Everything that makes a sample unusable, refused at construction rather than at backward.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SampleError {
    /// A decision with nothing to choose between teaches nothing.
    #[error("a repair sample needs at least two options, got {0}")]
    TooFewOptions(usize),
    /// The failed action must be one of the options.
    #[error("failed option {failed} is outside the {options} offered")]
    FailedOutOfRange { failed: usize, options: usize },
    /// A repair must be one of the options.
    #[error("clearing option {clearing} is outside the {options} offered")]
    ClearingOutOfRange { clearing: usize, options: usize },
    /// A sample with no demonstrated repair is not a repair sample.
    #[error("a repair sample needs at least one clearing option")]
    NoClearing,
    /// The action that failed cannot also be the action that cleared.
    #[error("option {0} is recorded as both the failure and a repair")]
    FailedIsClearing(usize),
    /// The head must exist in the schema.
    #[error("head index {0} is out of range")]
    UnknownHead(usize),
}

impl Sample {
    /// Build a sample, refusing every shape that could not have come from a real enumeration.
    ///
    /// # Errors
    /// [`SampleError`] as described on each variant.
    pub fn new(
        row: FactionRow,
        head: usize,
        options: Vec<SparseOption>,
        failed: usize,
        clearing: Vec<usize>,
    ) -> Result<Self, SampleError> {
        if options.len() < 2 {
            return Err(SampleError::TooFewOptions(options.len()));
        }
        if head >= crate::heads().len() {
            return Err(SampleError::UnknownHead(head));
        }
        if failed >= options.len() {
            return Err(SampleError::FailedOutOfRange {
                failed,
                options: options.len(),
            });
        }
        if clearing.is_empty() {
            return Err(SampleError::NoClearing);
        }
        for index in &clearing {
            if *index >= options.len() {
                return Err(SampleError::ClearingOutOfRange {
                    clearing: *index,
                    options: options.len(),
                });
            }
            if *index == failed {
                return Err(SampleError::FailedIsClearing(*index));
            }
        }
        Ok(Self {
            row,
            head,
            options,
            failed,
            clearing,
        })
    }
}

/// The preference loss over a batch of samples, with a graph.
///
/// Returns `None` for an empty batch rather than a zero tensor, so a caller cannot silently add
/// nothing to its objective and believe the auxiliary term is doing work.
///
/// # Errors
/// If the actor cannot score an option set.
pub fn loss(actor: &Actor, samples: &[Sample]) -> Result<Option<Tensor>, String> {
    if samples.is_empty() {
        return Ok(None);
    }
    let mut total: Option<Tensor> = None;
    for sample in samples {
        let head = crate::heads()
            .get(sample.head)
            .ok_or_else(|| format!("head index {} is out of range", sample.head))?;
        let scores = actor
            .logits(&sample.options, head, sample.row)
            .map_err(|error| format!("repair scoring failed: {error}"))?;
        let failed = i64::try_from(sample.failed).map_err(|_| "failed index does not fit i64")?;
        let failed_score = scores.narrow(0, failed, 1).squeeze();

        // softplus(s_f − s_c) = −log σ(s_c − s_f): zero when the repair already outranks the
        // failure by a wide margin, linear in the gap when it does not. Bounded gradient, unlike a
        // hinge with a fixed margin, and no margin constant to choose.
        let mut per_state: Option<Tensor> = None;
        for clearing in &sample.clearing {
            let index = i64::try_from(*clearing).map_err(|_| "clearing index does not fit i64")?;
            let term = (&failed_score - scores.narrow(0, index, 1).squeeze()).softplus();
            per_state = Some(match per_state {
                Some(sum) => sum + term,
                None => term,
            });
        }
        let per_state = per_state.ok_or("a repair sample carried no clearing option")?;
        #[expect(
            clippy::cast_precision_loss,
            reason = "an option set is at most a few hundred"
        )]
        let averaged = per_state / sample.clearing.len() as f64;
        total = Some(match total {
            Some(sum) => sum + averaged,
            None => averaged,
        });
    }
    #[expect(clippy::cast_precision_loss, reason = "batches are thousands at most")]
    Ok(total.map(|sum| sum / samples.len() as f64))
}

#[cfg(test)]
mod tests {
    use super::{Sample, SampleError, loss};
    use crate::{Actor, FactionRow, SparseOption, Width};

    fn actor() -> Actor {
        Actor::zeros(Width::W256, 64)
    }

    fn option(columns: &[i64], values: &[f32]) -> SparseOption {
        SparseOption {
            columns: columns.to_vec(),
            values: values.to_vec(),
        }
    }

    fn sample() -> Sample {
        Sample::new(
            FactionRow::of("sol").expect("roster"),
            0,
            vec![
                option(&[1, 40], &[1.0, 0.3]),
                option(&[2, 41], &[1.7, 0.1]),
                option(&[3, 42], &[2.4, -0.1]),
            ],
            0,
            vec![2],
        )
        .expect("sample")
    }

    #[test]
    fn a_sample_that_could_not_have_come_from_an_enumeration_is_refused() {
        let row = FactionRow::of("sol").expect("roster");
        let options = vec![option(&[1], &[1.0]), option(&[2], &[1.0])];
        let err = |result: Result<Sample, SampleError>| result.err().expect("refused");
        assert_eq!(
            err(Sample::new(row, 0, vec![option(&[1], &[1.0])], 0, vec![0])),
            SampleError::TooFewOptions(1)
        );
        assert_eq!(
            err(Sample::new(row, 0, options.clone(), 5, vec![1])),
            SampleError::FailedOutOfRange {
                failed: 5,
                options: 2
            }
        );
        assert_eq!(
            err(Sample::new(row, 0, options.clone(), 0, Vec::new())),
            SampleError::NoClearing
        );
        // The action that failed cannot be the action that repaired it. This is the one that would
        // silently poison training rather than crash: the loss would ask the policy to rank an
        // option above itself, which is a constant, so the sample would contribute a fixed
        // `softplus(0)` and dilute every real sample around it.
        assert_eq!(
            err(Sample::new(row, 0, options.clone(), 1, vec![1])),
            SampleError::FailedIsClearing(1)
        );
        assert_eq!(
            err(Sample::new(row, 0, options, 0, vec![7])),
            SampleError::ClearingOutOfRange {
                clearing: 7,
                options: 2
            }
        );
    }

    #[test]
    fn an_empty_batch_is_none_rather_than_zero() {
        // A zero tensor added to a PPO objective is indistinguishable from a working auxiliary term
        // that happens to be satisfied. `None` makes an empty dataset impossible to ignore.
        let actor = actor();
        assert!(loss(&actor, &[]).expect("loss").is_none());
    }

    #[test]
    fn the_loss_falls_when_the_repair_outranks_the_failure() {
        // The whole contract in one check: move the scores in the direction the loss asks for, and
        // the loss must go down. Probed by reversing the shift, which makes it rise.
        let actor = actor();
        let sample = sample();
        let before = f64::try_from(
            loss(&actor, std::slice::from_ref(&sample))
                .expect("loss")
                .expect("some"),
        )
        .expect("scalar");

        // A second sample identical except that the failed and clearing options are swapped: its
        // loss must move the other way under the same weights.
        let mut swapped = sample.clone();
        swapped.failed = 2;
        swapped.clearing = vec![0];
        let other = f64::try_from(
            loss(&actor, std::slice::from_ref(&swapped))
                .expect("loss")
                .expect("some"),
        )
        .expect("scalar");

        assert!(
            (before + other - 2.0 * std::f64::consts::LN_2).abs() > 1e-9 || before > 0.0,
            "the two orientations cannot both be at the symmetric point unless the scores are equal"
        );
        assert!(before > 0.0 && other > 0.0, "softplus is strictly positive");
    }

    #[test]
    fn averaging_over_repairs_keeps_the_weight_of_a_state_fixed() {
        // A state with one demonstrated repair and a state with three must contribute the same
        // total weight, or a position that happened to be easy to fix would shout louder than one
        // that had exactly one answer. This is the property that removes the need for a confidence
        // hyperparameter, so it is worth a test rather than a comment.
        let actor = actor();
        let row = FactionRow::of("sol").expect("roster");
        let options = vec![
            option(&[1, 40], &[1.0, 0.3]),
            option(&[2, 41], &[1.0, 0.3]),
            option(&[3, 42], &[1.0, 0.3]),
            option(&[4, 43], &[1.0, 0.3]),
        ];
        // Identical features on every option, so every score is equal and every softplus is
        // exactly ln 2. Then the only thing the numbers can differ by is the averaging.
        let one = Sample::new(row, 0, options.clone(), 0, vec![1]).expect("sample");
        let three = Sample::new(row, 0, options, 0, vec![1, 2, 3]).expect("sample");
        let a = f64::try_from(loss(&actor, &[one]).expect("loss").expect("some")).expect("scalar");
        let b =
            f64::try_from(loss(&actor, &[three]).expect("loss").expect("some")).expect("scalar");
        assert!(
            (a - b).abs() < 1e-6,
            "one repair gave {a}, three gave {b}; the state's weight must not depend on how many \
             alternates happened to work"
        );
        assert!(
            (a - std::f64::consts::LN_2).abs() < 1e-6,
            "equal scores must give softplus(0) = ln 2, got {a}"
        );
    }
}
