# Work instruction — five tasks

For the agent working as **Pi**. Written 2026-08-12 by the agent working the content registries.

Five tasks, independent of each other. Do them **in the order given**, one commit each. Every
one has an existing pattern in this repository to copy — none requires you to design anything
new. If a task turns out to need a design decision, stop and say so rather than inventing one.

## Ground rules

1. **Work only inside `D:\Projects\ti4-engine-rs`.** The Python repository at
   `D:\Projects\ti4-engine` is a read-only oracle. Never edit, format, stage, commit, clean,
   reset, or write files into it. Reading it is fine and often useful.
2. **Do not touch `crates/ti4-engine/src/timing.rs` or `crates/ti4-engine/src/event.rs`.**
   Another agent is actively changing both. Touching them will cause a collision.
3. **Never weaken a test or a lint to make something pass.** If a test fails, either the code is
   wrong or the test is wrong; work out which and say which. Deleting an assertion is not a fix.
4. **Never claim a result you did not observe.** Paste the command output you actually got.
5. Comments explain **why**, not what. The code already says what it does. Match the surrounding
   style: test names read as sentences, and a comment earns its place by recording a trap.

## Verification, after every task

Run all four. All four must be clean before you commit.

```bash
cargo fmt --all
RUSTFLAGS="-D warnings" cargo clippy --workspace --all-targets
cargo test --workspace
cargo test --workspace 2>&1 | grep -c FAILED     # must print 0
```

Count failures with `grep -c FAILED`. Do **not** check by looking for passing lines — that
cannot tell "no failures" apart from "the crate did not report", and a red test was shipped
once in this project exactly that way.

Baseline before you start: **539 engine + 121 content + 68 model + 1 doc-test, 0 failed.**
Your changes should only ever raise those numbers.

## Commit format

One commit per task. Subject in the imperative, under ~60 characters. Body explains why the
change was needed, not what the diff shows. End every commit message with:

```
Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
```

---

## Task 1 — Replace the duplicated seed search with a deterministic die

**Files:** `crates/ti4-engine/src/dice.rs`, `relics.rs`, `agenda_effects.rs`

Two test helpers named `seed_rolling` exist, one in `relics.rs` and one in `agenda_effects.rs`.
They are identical: each brute-forces up to 10,000 RNG seeds to find one whose next die shows a
wanted face, so that a test can force a branch. It works, but it is duplicated, slow, and
obscure.

Replace it with a test-support constructor on `Dice` that yields a fixed sequence of faces —
for example `Dice::from_faces([10])` — and use it in both tests.

**Done when:**

- `Dice` has one new constructor, documented, that returns the given faces in order.
- Decide and document what it does when the sequence runs out. Continuing to roll normally is
  fine; panicking is fine; silently repeating the last face is not. Say which you chose and why.
- Both `seed_rolling` helpers are gone, and both tests still force **both** of their branches.
- `grep -rn "fn seed_rolling" crates/` returns nothing.
- The two tests still assert exactly what they asserted before. This task changes how the dice
  are produced, never what the tests claim.

**Watch for:** the constructor must not be usable to fake results in real play by accident.
Gate it with `#[cfg(test)]`, or document plainly that it is for tests and content that calls for
a fixed die.

---

## Task 2 — Report unimplemented cards for secrets and agenda effects

**Files:** `crates/ti4-engine/src/secrets.rs`, `crates/ti4-engine/src/agenda_effects.rs`

Four modules already have a public `unimplemented()` that lists the cards in the corpus which
this engine has no handler for: `exploration.rs`, `relics.rs`, `laws.rs`, `action_cards.rs`.
`secrets.rs` and `agenda_effects.rs` do not.

Copy the established pattern into both. Read `relics.rs:289` first — it is the clearest example.

**Done when:**

- `secrets::unimplemented(content, sources)` returns every secret objective in the corpus with
  no registered requirement.
- `agenda_effects::unimplemented(content, sources)` returns every agenda with no registered
  effect.
- Each has a test asserting that nothing in `registered_aliases()` appears in the returned list,
  and that the list is not empty (both registries genuinely have gaps today).
- Each has a test asserting that **every** alias in `registered_aliases()` is a card the corpus
  actually knows. This one matters: aliases have been wrong three times in this project — an
  alias `usc` that does not exist, a `requirements` field that is the literal string `"null"`,
  and an `electType` that is null on every agenda. A handler registered under a misspelt alias is
  unreachable for ever and nothing else will notice.

---

## Task 3 — Guard the driver subsystems that have no wiring test

**File:** `crates/ti4-engine/src/wiring.rs`

Read the module docstring at the top of `wiring.rs` before starting. The single most common
failure in this project — seven times now — is a module that arrives correct, fully tested, and
called by nothing. A unit test proves a module *works*; it never proves anything *uses* it.
`wiring.rs` exists to catch that.

`game.rs` currently reaches these subsystems:

```
agenda  agenda_effects  combat  draft  fleet  invasion  movement  objectives
phase  production  relics  setup  status  strategy  tactical  tokens  transactions
transit  vote
```

Some already have a guard. Several do not.

**Done when:**

- You have listed which of those have a guard today and which do not. Put the list in the commit
  message.
- Every one that lacks a guard has one, following the existing style in the file.
- **Each new guard is proven by breaking it.** Temporarily remove or rename the call it guards,
  run the test, watch it fail, then restore the code. A guard nobody has watched fail is
  decoration. Report in the commit message that you did this.

**Watch for:** prefer a guard that drives a real game and asserts an event was reached, over one
that greps the source text, when both are possible. Both kinds already exist in the file; the
first is stronger, the second is better than nothing.

---

## Task 4 — Add runnable examples to the main public APIs

**Files:** `crates/ti4-engine/src/choice.rs`, `transactions.rs`, `objectives.rs`,
`crates/ti4-content/src/lib.rs`

The whole workspace has exactly **one** doc-test. The public API is documented in prose but
nothing demonstrates use, and nothing checks that the prose still compiles.

Add short ```` ```rust ```` examples to the most important public items. Start with:

- `choice::Table` and `choice::Decider` — how a decision is asked and answered.
- `choice::Window` — the shape every resumable decision sequence follows.
- `transactions::TradeWindow` — opening negotiations and answering an offer.
- `objectives::scoreable` — asking what a player could score.
- `ti4_content::ContentStore::embedded` — loading the corpus.

**Done when:**

- `cargo test --doc --workspace` runs the new examples and they pass.
- The doc-test count has risen from 1 to at least 6.
- Every example is real, compiling code. An example that needs `no_run` or `ignore` to pass is
  not finished — if it cannot be made to run, leave that item out and say why.

**Watch for:** keep each example to a handful of lines showing one thing. This is not a place
for a tutorial.

---

## Task 5 — Separate the evidence stubs from the real evidence

**Directory:** `plans/evidence/`

There are **430** files in `plans/evidence/`. Most are placeholders generated during planning,
one per package, and they are easy to mistake for completed work — the session handover names
this as an active hazard. A handful are genuine evidence written when a package landed.

Produce an index that tells them apart. This is a documentation task; change no code.

**Done when:**

- `plans/evidence/INDEX.md` exists, listing every file under one of two headings: **written**
  (real evidence for a package that landed) and **placeholder** (generated, not yet work).
- You state the rule you used to classify them, and it is a rule anybody can re-apply — file
  length, a template phrase that only stubs contain, or similar. State it at the top of the
  index.
- Spot-check at least five in each group by opening them, and say in the commit message that
  you did.
- The handover also notes that some package IDs in filenames drifted from `MASTER_PLAN.md`:
  `M05-004_TACTICAL_DRIVER` and `M06-001_SPACE_COMBAT` sit in slots the plan assigns to other
  packages. **Do not rename anything.** Just record the drift in the index so the next reader
  is not misled by a filename.

---

## If you get stuck

Stop and report. Say what you tried, what you expected, and what happened, and paste the real
output. An accurate account of a blocked task is worth more than a task reported as done.

Two specific things that should make you stop rather than push on:

- A test fails and you cannot tell whether the test or the code is wrong.
- A task looks like it needs a change to `timing.rs`, `event.rs`, or to how abilities reach game
  state. That work belongs to another agent and is being changed right now.
