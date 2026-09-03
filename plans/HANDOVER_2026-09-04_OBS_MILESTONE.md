# Handover — Stage 2 decision contract, packages 002a through 007b

Written 2026-09-04 for whoever picks this up next. Everything below is committed and green:
engine 1,135 lib tests + 4 integration + 5 docs, training 133, `RUSTFLAGS=-D warnings cargo clippy
-p ti4-engine --all-targets` clean.

## What is done

| package | state | commit |
|---|---|---|
| OBS-002a decision producer / delivery audit | **accepted**, reviewed and committed | `6871726`, `d03a765` |
| OBS-002b rule-dependency and aliasing matrix | **complete** | `330e1bf`, `cd11f29`, `211a46c` |
| OBS-003a typed choice-context schema | **complete**, unwired by design | `372e08e` |
| OBS-003b replay / hash migration | **complete** | `0d3c226` |
| OBS-003c seat-bound delivery migration | **14 of 15**; one left, scoped below | `bf6e788`, `905a899` |
| OBS-004 actor-owned inventory | **complete** | `cebc897` |
| OBS-007a preview contract | **complete**, computes nothing by design | `020fe00` |
| OBS-007b deterministic preview foundation | **complete** | `229519b` |

Next in the plan's own recommended order is **OBS-008c**, production and payment options. It is the
first package that wires context and previews into producers, so it is the first that can change
what a policy sees.

## Decisions you should not quietly undo

**`DecisionContext`, `Preview` and `deterministic::spend` are additive and unused on landing.** That
is deliberate. A schema argued over while eighty producers are being edited is a schema nobody can
review. Wire them in OBS-008, not before.

**Previews are analytic, never simulated.** `deterministic::spend` computes the pool afterwards from
the plan's `worth`. Do not "simplify" it by cloning the state and applying the change: it would then
agree with application by construction and the agreement test would check nothing.

**The pool falls by worth, not by cost.** Exhausting a four-influence planet against a
three-influence bill removes four from what remains spendable. This is the same arithmetic that
billed seven influence for two command tokens.

**`Unknown` is not `Unavailable`, and neither is a zero.** Folding "not computed" into "no change"
teaches a shaping term that the action is free. Folding it into "illegal" teaches a policy to stop
choosing exactly the options nobody has modelled yet. `Preview::is_informative()` is the guard;
`expected()` returns `None` rather than zero.

**Do not route asks through a convenience wrapper.** An `ask_seat` helper on `Game` was written and
reverted. The audit scanner keys on calls to the delivery API, so a wrapper removed nine pre-existing
sites from the scan and broke the registry's `ObservedVia` mapping. `imperial_arbiter` carries an
`expect(clippy::too_many_lines)` with that reason rather than hiding its delivery.

**The viewless-ask assertion checks the source, not the registry.** It sums the scanned
`AskViewless` sites as well as the `VIEWLESS_ASKS` constant. Keep both: the constant alone cannot
fail, and it was the scanned half that caught a second `neuraloop` ask written in a different
syntactic form.

## The one remaining viewless ask, and why it is not a slice

`VIEWLESS_ASKS` holds exactly `("timing.rs", "pick", 1)`.

`timing::pick` is reached from `Resolver::run_window`, and `Resolver` holds a `Table` and **no game
state at all** — no `state`, `content` or `sources`. Its sibling `pick_with_context` already delivers
seat-bound because `run_window_with_context` is handed a `TimingContext`.

So migrating it means giving every contextless emit a context, and **55 call sites use
`Resolver::emit` against 13 using `emit_with_context`**. That is its own package.

It is **not** a genuine setup/offline exception. 55 live sites is not an offline path, so it stays
classified as migration work rather than being written into an exceptions registry.

## What the measurements say, and what they do not

`plans/evidence/OBS-002B_ALIASING_CENSUS.md` and `OBS-002B_RULE_DEPENDENCY_MATRIX.md`.

- **164 proven aliases**: same state context, same option set, and the seat's own facts differed
  between decisions. 108 are the `tokens` head, 48 `landing`.
- The most repeated single input is "gain a command token into which pool" — seven decisions, seven
  distinct seat states, two different rounds, one input.
- **`seat_facts` already computes round and the pool counts.** They exist and are lost in the
  option-crossing. This is a delivery defect, not a missing-feature one, and it is worth testing
  before committing to the full OBS-004→008 feature build-out.
- **Laws reach no feature at all**: fourteen producers read law, agenda and custodians state and
  `features.rs` contains zero occurrences of `laws` or `custodians`. OBS-004 added
  `Observed::laws()` and `law_outcome()`; nothing reads them yet.
- **No hidden-information leak was found.** Nine producers touch hidden collections; every one is
  actor-scoped or reads the public discard. Cleared by reading, because the automated scope
  heuristic over-flagged seven of them.

The first version of the census measured the wrong thing. "Same observation, different legal
actions ⇒ incomplete" is **false for a per-option architecture**: the option set is part of what the
model sees. 453 findings were reclassified as ordinary. If you extend this diagnostic, keep that
distinction.

## Engine defects found and fixed this session

All committed with regression tests.

- **Ghost Squad looped for ever.** Options regenerated from board state with no memory of what had
  moved, so A→B left B→A as the only move and a greedy policy oscillated. One game repeated the same
  decision 324,507 times. The repeats sit inside one `Game::step`, so `--max-steps` can never fire —
  which is why lowering it changed nothing.
- **Fleet supply was never enforced.** `enforce_everywhere` existed and was called by nothing. Now
  enforced table-wide at every turn end; overhead optimised 1.7× → 1.12× (empty-pair guard,
  catalogue hoisted, `system_state` clone replaced by a borrow).
- **`baseUpgrade` was read by no engine code**, so a faction could research the generic upgrade its
  own unit replaces (Sol → Carrier II).
- **Technology primary could decline its mandatory first research.**
- **Research options now carry their price in the payload**, not the label — `payload-number:cost`
  is a real feature; a label is read by nobody.

## Still open, not mine to close silently

- **A committed ground force vanishes.** Seed 501, rotation 1, seat 3, system 70: four committed,
  three landed, both planets taken. Not ground combat (planets were empty and unowned), not the
  commit path (a minimal repro passes), not fleet enforcement (space-only). The reporter's
  exploration-card hypothesis fits every observation. `every_committed_force_lands` pins the
  invariant.
- **`return_support` and `status_tokens` are implemented, tested, and called by nothing** — Support
  for the Throne never returns, Sol's Versatile never grants its token.
- **The review narrative is a board-state diff and mispairs units.** It invented a planet-to-planet
  move and misplaced a casualty in seed 501 while the decision log beneath it was correct. Anyone
  judging bot behaviour from these logs is reading some fiction.
- **`wiring.rs` is the systemic fix.** Five defects this session were one shape: correct code that
  nothing calls. That file exists for exactly this and says so in its header, but its checklist has
  seven entries and covered none of them.

## Two things worth deciding before more training compute

Recorded as opinion, clearly marked, because the instruction was to implement the plan as written
and that is what the commits do.

1. **There is no critic.** `CriticMode::BatchMean` means no value tensors at all; every update logs
   `critic 0.00000`. Advantages have no state dependence, so all ~300 decisions in a game share one
   sign. The plan puts critic alignment at OBS-010, after twenty packages.
2. **The 4-round horizon is unvalidated.** Running the existing champions at 4/5/6 rounds with no
   training is cheap and could invalidate the target. If the ranking inverts with horizon, part of
   this programme is aimed at the wrong objective.

## Working notes

- Shells: PowerShell for anything needing libtorch — on Windows the PATH separator is `;`, and
  exporting it with `:` in Bash silently yields `exit 127, c10.dll not found` under `timeout`/`grep`.
- `git add` by explicit path. A broad `git add -A crates` once swept up another session's
  in-progress seating fix; it was split back out, but do not repeat it.
- `sample.html` and `sample.ti4review.json` are pre-existing untracked review samples. Leave them.
