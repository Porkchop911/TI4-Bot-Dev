# Brief: action cards and agendas

For a second implementer working alongside the main engine work. Self-contained — you are not
expected to have read the rest of this session.

## What this repo is

`ti4-engine-rs` is a Rust implementation of Twilight Imperium 4 (base + Prophecy of Kings +
Codices + Thunder's Edge), plus a self-play learner that trains on it. The engine is the part that
matters here.

Governing documents, in order of authority:

1. `AGENTS.md` — the execution protocol. Read it first.
2. `plans/ENGINE_COMPLETION_PLAN.md` — the plan this work is phase 7 of.
3. `engine-rules-audit.md` — every rule topic against the engine, with what is missing.

## Your scope

**Action-card effects and agenda effects. Nothing else.**

| | now | target |
|---|---|---|
| action cards | 34 of 142 | as many as possible |
| agendas | 45 of 63 | 63 |

**Start with the 51 action cards that are writable today.** A reaction card can only fire if its
printed window maps to an event the engine emits. 22 printed windows are still unmapped, and a card
bound to one of those is inert however well you write it. The list is queryable:

```rust
ti4_engine::reactions::reachable(content, sources)   // cards whose window is mapped
ti4_engine::action_cards::unimplemented(content)     // cards with no effect
// the intersection is your work queue: 51 cards
ti4_engine::reactions::unmapped_windows(content, sources) // what is blocked, and on what
```

Do not try to close the unmapped windows. That needs combat decomposed into announced steps, it is
phase 8, and it is being done by the other implementer. Writing a card for an unmapped window is
wasted work that will look finished.

## Files you own

- `crates/ti4-engine/src/action_cards.rs`
- `crates/ti4-engine/src/agenda_effects.rs`

## Files you must not touch

Being changed concurrently. A conflict here is expensive:

- `crates/ti4-engine/src/combat.rs`, `invasion.rs`, `reactions.rs`, `game.rs`
- `crates/ti4-engine/src/coexistence.rs`, `breakthroughs.rs`, `synergy.rs`, `fracture.rs`,
  `neutral_units.rs`, `entropic_scars.rs`, `space_stations.rs`
- `crates/ti4-sim/src/behavior.rs` — the behavioural baseline. See "if a bound fails" below.
- `crates/ti4-content/content/*` — the corpus

If a card genuinely needs a change outside your files, stop and say so rather than reaching in.

## How to add an effect

Both modules use the same shape: a registry naming what is implemented, and a dispatch that must
agree with it exactly.

**Action cards** (`action_cards.rs`): write `fn my_card(context: &mut TimingContext<'_>, player:
&PlayerId)`, add the alias to `effect_for`, add it to `registered_aliases`. Note the four-copies
idiom — `"mb1" | "mb2" | "mb3" | "mb4" => Some(morale_boost)` — a card printed four times has four
aliases and leaving one off makes that copy permanently unplayable with no symptom.

**Agendas** (`agenda_effects.rs`): add a match arm in `resolve_with` and the alias to
`registered_aliases`. An agenda with **no arm is unavailable**, so a card whose whole effect is a
standing rule still needs an empty arm with a comment saying where the rule lives.

## Verification standard

Not optional. These come from defects this project actually shipped.

1. **Test through the function the engine calls, not the one you wrote.** A helper that returns the
   right number is worthless if nothing consults it. Four laws were listed as enforced with a
   predicate written and zero callers; two coverage figures reported finished work as missing.
   Before claiming a card, grep for a caller.
2. **Probe every gate.** Break the implementation deliberately, confirm the test fails, revert. A
   test that has never been seen to fail has not been shown to work. One guard here asserted
   `all.len() > 50` and held whether the function worked or not.
3. **Quote the rule** in a doc comment on the effect, so a reader can check code against card
   without leaving the file.
4. **Assert the invariant, not a size.** "Reported implemented iff it has an effect", not "the list
   is long".

## Commands

```bash
cargo test --release --workspace          # 1,633 passing as of this brief; keep it there
cargo clippy --release --workspace --all-targets   # zero warnings is the standard
cargo run --release -p ti4-engine --example coverage_report   # your progress meter
```

`LIBTORCH` must point at `out/libtorch-2.9.1-cpu` for anything touching the learner crates; the
engine alone does not need it.

## If a behavioural bound fails

`ti4-sim`'s `the_suite_reproduces_and_stays_within_the_recorded_bounds` guards version-to-version
drift. If it fails, **diagnose before touching the bounds** — they may only move through the
versioned process in `crates/ti4-sim/src/behavior.rs`, which needs old and new recorded side by side
in `plans/evidence/M08-021.md`, the cause stated, and review approval. `cargo run --release -p
ti4-sim --example rebaseline_behavior` prints old against new and changes nothing.

An action card that nobody plays should not move it. If it does, that is worth understanding before
proceeding.

## Definition of done, per card

- effect written, alias in both the registry and the dispatch
- a test that drives it through the engine's own path
- the test probed
- rule text quoted in the doc comment
- `coverage_report` moved by exactly the number of cards you added
- workspace green, clippy clean

## Commit convention

One commit per coherent batch, not per card. Say what the cards do and what was decided, not that
you added them. End with:

```
Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
```

## The trap in this codebase

Registries drift out of sync with the code that dispatches from them, and the guards tend to assert
something weaker than the invariant. Every real defect found in this area recently was that shape —
a list that was not the list the engine reads — rather than a misread rule. When you finish a batch,
check your registry against your dispatch programmatically rather than by eye.
