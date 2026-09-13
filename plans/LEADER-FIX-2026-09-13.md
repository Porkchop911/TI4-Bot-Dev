# Six-faction leader runtime correction

## Scope

Operator-requested correction for Hacan, Jol-Nar, L1Z1X, Letnev, Sol, and Xxcha leaders after the
2026-09-13 corpus audit found that isolated handlers existed without driven-game delivery.

Permission class required: P1.

- Writable paths: `crates/ti4-engine/src/{game,leaders,combat,invasion,vote,production,reactions}.rs`,
  narrowly required model state if an effect cannot be represented by existing sequence markers,
  focused tests in those modules, this plan, execution state, and package evidence.
- Read-only inputs: embedded `crates/ti4-content/content/leaders.json` and accepted Rust engine
  specifications/tests.
- Network access: none.
- Processes: bounded Cargo build/test/clippy processes, at most four build jobs and four test
  threads while unrelated generation/training is absent.
- Generated artifacts: ordinary ignored Cargo output only; no corpus or checkpoint writes.
- Destructive actions and external-state changes: none.

## Normative behavior

The embedded leader records' `abilityWindow`, `abilityText`, and `unlockCondition` fields are the
versioned source for this correction. Stable legal-choice delivery, deterministic ordering, typed
decision context, player-visible observations, atomic failure, and current source scope remain
engine invariants.

## Packages

1. **LEADER-FIX-001 — deployment, unlock, and component actions.** Resolve the PoK/Thunder's Edge
   Xxcha hero replacement, refresh commander unlocks at decision boundaries, expose legal action
   leaders, implement all required player selections, and preserve Letnev's hero through the end of
   its game round.
2. **LEADER-FIX-002 — reactive combat, activation, and production windows.** Deliver L1Z1X,
   Letnev, Sol, and Hacan timing-window leaders to the correct owner/beneficiary and make selected
   extra dice affect exactly one eligible unit.
3. **LEADER-FIX-003 — voting and remaining commander semantics.** Replace fixed vote bonuses with
   Xxcha per-planet votes and Hacan trade-good spending, honour Xxcha's voting protection, and make
   Letnev's optional sustain reward an actual legal choice.

Each package requires red-first focused tests, affected-crate tests, formatting, Clippy, evidence,
an independent review, and a focused commit before the next package begins.

## Non-goals

- Regenerating or relabelling existing corpora.
- Training or evaluating another checkpoint.
- Adding factions outside the six named by the operator.
- Reinterpreting unrelated faction, action-card, or Thunder's Edge rules.

## Definition of done

Every in-scope leader has an end-to-end test proving deployment/source selection, unlock timing,
legal offer at its printed window (or passive application), chosen target/payment semantics, state
transition, exhaustion/purge lifecycle, and absence outside that window. The affected engine suite
and strict Clippy pass, review findings are resolved, evidence is committed, and the working tree is
clean.
