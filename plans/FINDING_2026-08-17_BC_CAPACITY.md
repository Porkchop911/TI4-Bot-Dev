# Behaviour-cloning capacity test: what it found, and what it could not

Date 2026-08-17. Tool: `cargo run -p ti4-training --example bc_capacity --release`.

---

## The headline is not the one that was expected

**The authored bot is weaker than the plateaued learned policy.** On the same validation seeds
(96,000,000 +120 games, `RustVaried`, four rounds):

| policy | table total VP |
|---|---|
| `ScoredBot` (hand-written constants) | **10.43** |
| Learned champion `run_pure_u5000` | **13.71** |
| Evolution champion (archive, g122 anchor) | 17.39 |

The learned policy already beats the authored baseline by **31%**. `ti4-policy`'s own module doc
calls `ScoredBot` "the baseline a learned policy has to beat" — that bar has been cleared, and
nothing in the training reports says so because nothing measures the two against each other.

## What that does to the test

The test asks whether a linear function of the current features can express good play, by fitting
it to a teacher's decisions and reading the **training** agreement. That inference only works if
the teacher plays *better* than the student. It does not, so:

> **The headline question — policy class or optimiser — is not answered by this run.**

Fitting a weaker policy at 58% cannot show whether the class can express a stronger one. Recorded
plainly rather than dressed up, because the natural temptation is to read 0.578 against 0.322
chance as "the class is fine".

## What it did establish

The fit converged (train curves flatten) and is the same functional form as inference — one weight
per feature, scores dotted, softmax over the option set — so the cross-entropy gradient is exactly
`φ_chosen − Σₒ pₒφₒ`, the expression the trainer already uses.

120 games, 92,809 decisions, 20 epochs, every fifth decision held out:

| head | train n | opts/dec | chance | **TRAIN** | test |
|---|---|---|---|---|---|
| `ability` | 1,709 | 2.0 | 0.498 | **0.873** | 0.857 |
| `cargo` | 8,726 | 3.1 | 0.358 | **0.809** | 0.778 |
| `scoring` | 1,150 | 2.6 | 0.436 | **0.817** | 0.643 |
| `turn` | 12,808 | 4.6 | 0.261 | 0.664 | 0.675 |
| `strategy` | 2,314 | 5.5 | 0.203 | 0.647 | 0.633 |
| `landing` | 4,720 | 3.0 | 0.359 | 0.650 | 0.646 |
| `activation` | 5,149 | 32.4 | 0.045 | 0.340 | 0.335 |
| `production` | 3,356 | 7.8 | 0.176 | 0.265 | 0.230 |
| `development` | 1,095 | 16.2 | 0.072 | 0.161 | 0.151 |
| **`secondary`** | **8,787** | **2.0** | **0.500** | **0.505** | 0.482 |
| pooled | | | 0.322 | 0.578 | 0.559 |

**One finding here is valid regardless of teacher strength.** A head whose training agreement sits
*at chance* has no discriminative power in this representation at all — that is a statement about
the features, not about who is being imitated:

- **`secondary`: 0.505 against 0.500 chance over 8,787 decisions.** The representation cannot tell
  the options of a strategy-card secondary apart. `secondary` is ~9.5% of all decisions.
- `development` (0.161 / 0.072) and `production` (0.265 / 0.176) are weak in absolute terms.
- `activation` lifts 7.6× over chance but still reproduces only a third of the teacher's choices —
  and it is 44.5% of all options generated.

## Caveat on the numbers

`movement` had not converged at 20 epochs (train curve 0.08 → 0.56 and still climbing), so its row
understates the achievable fit and the pooled figure with it. The heads that matter for the
conclusion — `secondary` especially — were flat from the first checkpoint.

An earlier pilot at a higher learning rate produced *below-chance* training accuracy on some heads.
That is the signature of an optimiser diverging, not a model that cannot fit, and it is why the
step is now averaged and decayed and why the per-epoch train curve is printed. A capacity test that
cannot distinguish "did not fit" from "blew up" measures nothing.

## What is needed to answer the original question

A teacher stronger than the student. The only one known to exist is the **evolution champion**
(table total 17.39, xxcha 4.57), which lives in `E:\ti4-engine\archive\stage2_blank_002` as evolved
parameters over the Python heuristic evaluator. Using it means either porting that evaluator to
Rust or exporting its decisions from the Python side.

Until then, the two live hypotheses are unresolved — with one exception: whatever else is true,
`secondary` cannot be learned in this representation, and that is worth fixing on its own.

## The other thing this changes

`ScoredBot` is used as the reference opponent in `ti4-sim` (`Seats::Scored`) and is described
throughout as the bar to beat. It is now 24% *below* the learned policy. Any report that compares
against it is measuring against something the project has outgrown, and the arena's plan to use it
as a skill floor should be revised: it is a floor, not a target.
