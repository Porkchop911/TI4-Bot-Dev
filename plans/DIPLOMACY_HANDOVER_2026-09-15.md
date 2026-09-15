# Structured diplomacy: handover to Codex, 2026-09-15

Your implementation stopped at 15:18 (usage limit) in the middle of Stage 8. Claude continued in this worktree
and has now handed it back to you. This document covers what changed since you stopped, the evidence, and
recommended next steps. The user has **not** approved any of the recommended steps yet: confirm with the user
before committing, merging or re-baselining.

## Ground rules (from the user)

- **No commits and no merges without the user's go-ahead.** Everything below is uncommitted on
  `codex/diplomacy-v1` (base 5f2b2c6) in this worktree.
- **No new folders** outside an assigned repo: no new worktrees, sibling directories, temp-directory folders or
  new `CARGO_TARGET_DIR`s. Ask the user before creating a folder inside a repo.
- **The D: checkout `D:\Projects\ti4-engine-rs` is shared with parallel sessions.** Don't switch its branch, and
  append to shared files rather than overwriting them.
- **Don't share a cargo target directory between worktrees.**
- **Re-baselining ti4-sim needs the user's approval.**
- Build here with this worktree's own `target/` and `LIBTORCH=D:/Projects/ti4-engine-rs/out/libtorch-2.9.1-cpu`,
  plus `LIBTORCH_BYPASS_VERSION_CHECK=1` and that `lib/` directory on `PATH`. This worktree has no `out/`.
  Examples that read `out/pools/...` must be run with `D:\Projects\ti4-engine-rs` as the working directory; the
  already-built binary in `target\release\examples` works.
- The D: checkout carries **uncommitted** engine fixes that are not in this branch's base: neutral-unit combat,
  promissory-note usage windows, Hacan hero, Support for the Throne return paths, Rin tech filter, and others.

## Stage status

| Stage | State |
|---|---|
| 0 Contract freeze | Skipped. No plans/evidence package beyond this document. |
| 1 Domain types + match state | Done (yours). Opt-in (`enabled: false` by default); legacy promises migrate to inert history. |
| 2 Relationships, deltas, decay | Done (yours), plus Claude's guard for unseated owners. |
| 3 Promise predicates + lifecycle | Done (yours), plus Claude's settlement-side fix. |
| 4 Negotiation window | Done (yours), plus Claude's fixes to counters, empty contacts and accept payloads. |
| 5 Candidate templates | Done (yours). `PayForVote` is declared but never generated. |
| 6 Signals + third-party effects | Done (yours). Subject is always the counterparty; no conditions. |
| 7 Observation | State facts (yours) plus option features (Claude). |
| 8 MLP head | Finished by Claude. |
| 9 Corpus/logging | Not started. |
| 10 Self-play harness | Not started, beyond a soak test. |
| 11 LLM adapter boundary | Not started. |

## What Claude changed

### Bugs fixed in the engine slice

1. **Promises from the recipient of a one-sided deal crashed the game at round end.**
   - Where: `diplomacy/promises.rs`.
   - Cause: `inspect_terms` and `deadline_terms` inferred the side from `offset == 0`. That is also true for
     recipient terms when the proposer promises nothing, which is every `FuturePayment` bundle.
   - Effect: `settle_term` returned `UnknownTerm`, and the `expect` in `begin_next_round` panicked.
   - Fix: the side is now passed explicitly.
   - Test: `a_promise_from_the_recipient_of_a_one_sided_deal_settles_at_its_deadline`.
2. **An offered contact could refuse its own step.**
   - Where: `diplomacy/window.rs`.
   - Cause: `DiplomacyWindow::open` refused when no bundle survived filtering, but `available_contacts` offers the
     contact without seeing that filtering.
   - Fix: the window now opens with zero bundles. A signal and "make no offer" are still legal.
3. **Counters could ask a side for more than it holds.**
   - Where: `diplomacy/candidates.rs`.
   - Effect: accepting such a counter failed `transactions::resolve` and blocked the game.
   - Fix: `generate_counter_candidates(state, proposer, recipient, current, author, round)` drops revisions whose
     immediate trade goods or commodities exceed holdings. The window calls it through `DiplomacyWindow::counters`.
   - Test: `counters_never_ask_a_side_for_more_trade_goods_than_it_holds`.
4. **Events naming an unseated owner grew the relationship matrix.**
   - Where: `diplomacy/relations.rs`.
   - Cause: combat against neutral units (owner `neutral`, never seated) would add a row that
     `DiplomacyState::validate` refuses. This becomes live once the D: neutral-combat fix is merged.
   - Fix: `apply_relationship_event` ignores events naming a player without a matrix row.
   - Test: `an_engagement_with_an_unseated_owner_changes_no_relationship`.
5. **The accept option carried no terms, so a policy accepted blind.**
   - Accept now carries the bundle.
   - Every bundle-bearing option (offer, counter, accept) carries `actor_is_proposer`. Terms stay stored against
     the original proposer and recipient, so this marker is how features know which side the deciding seat is on.

### Stage 7: option features

`ti4-policy/src/features.rs`: `diplomacy_decision_features`, `diplomacy_counterparty` and `relationship_features`,
called from `explicit_option_features_with`. They apply to kinds `open_diplomacy` and `diplomacy_*`, under the
already-registered `diplomacy` family:

- **Counterparty:** `diplomacy:counterparty:{out|in}:{trust|cooperation|threat|hostility}` (÷100, non-zero only),
  `diplomacy:counterparty:recent-attack` and `recent-breach`, and `diplomacy:counterparty:slot-{i}` (the OBS-005
  opponent slot).
  - For contact options the counterparty comes from the faction in the id; for payment options from the deal id;
    otherwise from `DecisionTarget::Player`.
- **Bundle:**
  - `diplomacy:template:{snake_case}` and `diplomacy:revision`;
  - `diplomacy:{gives|gets}-{now|later}`;
  - `diplomacy:{i-promise|they-promise}:{payment|no-activation|non-aggression|vote|attack|due-this-round}`;
  - `diplomacy:attack-target:{...}` relationship values toward a third-party target, or
    `diplomacy:attack-target:is-me`.
- **Option ids:** the per-bundle hash is no longer emitted as an `option:` token for offer and counter kinds.
- **Identities:** no raw seat, system or agenda identity is emitted.
- **Test:** `diplomacy_bundle_options_read_from_the_deciding_seats_side`.
- Not done: third-party-to-third-party relationship facts, and objective or military relevance (still zero in
  `CandidateFeatures`).

### Stage 8: MLP head

- **`ti4-mlp/src/lib.rs`:**
  - `all_heads()`: the 15-name superset. Layouts only append, so index `i` names the same head in every layout.
  - `Actor::with_diplomacy_head()`: the explicit 7→9 / 8→10 migration. It appends a zero row to `w_shared`,
    `b_shared`, `delta` and `b_delta`, preserves `requires_grad`, and is a no-op on a diplomacy actor.
  - Test: `migrating_to_the_diplomacy_layout_keeps_every_legacy_head_and_adds_a_neutral_one`. Legacy head logits
    are bit-identical after migration, and the diplomacy head scores zero.
- **Index-to-name lookups** now use `all_heads()` in `ppo.rs` (`freeze`, `score`, `head_of`), `distill.rs`,
  `repair.rs`, `critic_warmup.rs` and `positive_corpus.rs`. Distill's random readout initialisation is sized from
  `actor.head_names()`.
- **`ppo::update_inner`** refuses any step whose head index is outside the actor's layout, before mutating
  anything. A diplomacy step handed to a schema-7/8 actor fails loudly instead of indexing past the readout.
- **`bundle.rs`:** the schema-6 test now expects your new error text (`unsupported MLP bundle schema 6`), and the
  unused `heads` import is removed.
- **Unchanged on purpose:** `Actor::resolve_head` and `Actor::head_index` stay static, documented as the frozen
  schema-4 layout.

### Soak test

`crates/ti4-training/tests/diplomacy_soak.rs` plays seeded-random six-seat tables on `OpeningMap::RustVaried` with
diplomacy enabled. It asserts:

- no refused step;
- valid state at the end;
- that every negotiation outcome occurs.

The default is 6 seeds × 3 rounds (about 3.5 s in debug). Set `DIPLOMACY_SOAK_SEEDS` and `DIPLOMACY_SOAK_ROUNDS`
for longer runs. It found bugs 1 and 2 above.

## Evidence (this worktree, after all changes)

- **Tests:** `ti4-model` 81, `ti4-engine` lib 1301, `decision_delivery_inventory` 4, `ti4-policy` lib 245 and
  `ti4-mlp` lib 86 all pass.
  - `ti4-mlp` was run with `--skip bundle::` so nothing is created in the temp directory. Bundle tests passed in
    the baseline, and the schema-6 test passed after its fix.
- **Build:** `cargo check --workspace --all-targets` has no errors.
- **Soak:** `DIPLOMACY_SOAK_SEEDS=60 DIPLOMACY_SOAK_ROUNDS=6` passed, with no refused step and valid state.
  - Offered 4042, countered 2375, accepted 2017, declined 2025, settled 1798, signals 7176.
- **Legacy compatibility:** `crossplay_eval` loaded
  `out/blank-shaped-4layers/fracture-1x-styx16-20260915/checkpoint-19280` (schema 8, two residual blocks, OOV
  v10) and played 72 seat-games with diplomacy off.
- **Baseline before Claude's changes:** everything passed except that one `bundle.rs` schema-6 test.

## Open problems and recommendations

Recommended order. The user has not approved these yet.

### 1. Commit the D: checkout's engine fixes first (needs user approval)

Those fixes are audited and are what live training runs on. Diplomacy is newer, off by default and mostly additive,
so it should be the side that adapts. They overlap your changes in `combat.rs`, `game.rs`, `invasion.rs`,
`choice.rs` and `transactions.rs`.

### 2. Write a v10→v11 vocabulary migration (blocks merging)

- **The problem:** the OOV registry bump to v11 means `bundle::write` validates `slots.json` with
  `Vocabulary::from_json`, which only accepts the current version (`bundle.rs`, `slot_count_of`). `bundle::read`
  uses `from_json_for_inference`, which also accepts v10.
- **The consequence:** every existing checkpoint is v10. A legacy PPO run resumed on this branch loads, trains,
  and then fails at its first checkpoint write.
- **Recommendation: write the migration rather than relax `write`.** The policy crate deliberately pins "training
  load must stay current" (`vocabulary.rs` test `version_nine_loads_for_inference_without_renumbering_its_columns`),
  and relaxing it would create a permanent exception.
- **The layout change:** v10 has 47 family OOV rows after the global OOV at column 0. v11 inserts `oov:diplomacy`
  at column 48, so every assigned slot moves up by one and `oov_count` and `allocated_for` each grow by one.
- **The migration must:**
  - insert that slot and bump `oov_registry_version`;
  - shift `W1` (`[capacity, width]`) rows the same way;
  - apply the same shift to any separate-critic input table;
  - grow `capacity`, which is a multiple of 4096, if the vocabulary is full.
- **The acceptance test:** an original and a migrated bundle produce identical logits and values for option
  vectors built from the old feature names. New diplomacy names should land on the new reserved column.
- **The resulting path:** schema 8 v10 → schema 8 v11 → (optional) `with_diplomacy_head` → schema 10. Existing runs
  keep training, and any run can add the diplomacy head later.
- **Until this exists,** do not merge this branch into anything a live training run builds from.

### 3. Merge diplomacy onto the fixes, then rerun everything

- Rerun:
  - the engine tests;
  - the D: checkout's `support_audit` example (`crates/ti4-training/examples/support_audit.rs`, untracked there);
  - the diplomacy soak;
  - ti4-sim.
- **Expect ti4-sim to flag differences even with diplomacy off.** `GameState` serialization changed: `diplomacy`
  is added and `promises` is no longer serialized, so state digests can move. Re-baselining needs user approval.
  - ti4-sim's `faction_differentiation` bound already fails on the D: checkout, not yet attributed to a cause.
    Don't confuse that with the digest change.
- The unseated-owner guard should make neutral combat safe; confirm with the soak on merged code.

### 4. Fix the clippy warnings the diplomacy code introduces

- About 40 pedantic warnings, mostly mechanical:
  - `as u8` casts of trade goods and commodities, which wrap above 255 (use `try_from`/`min`);
  - missing `# Errors` docs;
  - `is_multiple_of`;
  - `Terms::default()`;
  - long `generate_initial_candidates`, `DiplomacyWindow::resolve` and `apply_acceptance`.
- The repo is not clippy-clean elsewhere either (`reward.rs`, `stage1.rs`, `ti4-mlp/src/ppo.rs`); leave
  pre-existing warnings alone.

### 5. Measure the cost before building Stages 9–10

- **What to measure:** use the soak harness to compare diplomacy on and off: decisions per game, steps per second,
  peak memory and options per decision.
- **Why it matters:** the user chose one initiation per target per turn, so a player can open up to five contacts
  every turn, plus payment options. Random play already produces heavy signal traffic.
- **If inflation is large,** propose narrowing when contacts are offered to the user; don't change the user's
  per-target rule unilaterally.
  - Examples: only players with at least one surviving bundle, or neighbours only for transfer templates.
- **Bound `DiplomacyState::journal`:** it grows without limit and must be bounded before long self-play.

### 6. Stage 9: corpus and logging

- **Head routing:** capture examples (`capture_offline_pilot`, `corpus_train`, `build_positive_corpus`, and the
  `inference_cost`/`trunk_depth_cost` tooling) still route heads with the static 14-head `Actor::resolve_head`.
  Diplomacy decisions would silently train `other`, so make them layout-aware.
- **Diplomacy log:** add the `DiplomacyLogRecord` export: denormalized per deal, append-only, and not replay input.

### 7. Experimental diplomacy training, only after 2–6

- **No teacher:** no authored bot plays diplomacy, so behaviour cloning has nothing to supervise the diplomacy
  head. PPO learns it from scratch, and early random diplomacy will likely cost results.
- **Run it separately:** start from a migrated checkpoint, never inside a main run.
- **Judge it** with paired greedy evaluations against a diplomacy-off control. The user's standing guidance is that
  in-training tables mask greedy regressions.

### 8. Stage 11

Documentation only, and it can wait.

## Addendum, 2026-09-15 evening: Claude's check of Codex's second session

Codex worked 16:46-17:31 until its usage limit again, adding the v10→v11 migration, Stage 9, Stage 11, the
relationship matrix and a cost probe; see `DIPLOMACY_IMPLEMENTATION_EVIDENCE_2026-09-15.md`. Claude re-verified
that state and resumed the worktree while Codex is at its limit.

### Test results on Codex's final state

- Workspace check clean.
- Unit tests: engine 1302, delivery inventory 4, model 81, policy 247 (full), bridge 62, MLP 87 (`--skip bundle::`).
- The 60x6 soak passes with exactly the same counts as before Codex's second session.

### Real-checkpoint migration check

New example, which writes nothing: `crates/ti4-mlp/examples/diplomacy_migration_check.rs`.

- **What it does:** it loads a bundle twice, migrates one copy in memory (`bundle::migrate_v10_to_v11` plus
  `with_diplomacy_head`), and plays seeded greedy six-seat tables on RustVaried.
- **Checkpoint:** `out/blank-shaped-4layers/fracture-1x-styx16-20260915/checkpoint-19280` (schema 8, OOV v10).
- **Setup:** 3 seeds × 3 rounds.

**Identity, diplomacy off, original vs migrated: IDENTICAL.** Every offered option set and every choice matched,
about 5,400 decisions.

**Cost, migrated actor, diplomacy on vs off** (release build):

| Seed | Mode | Decisions | Steps | Seconds | Contacts | Max negotiation options | State KB | Journal events | Journal KB |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| 910000000 | off | 1857 | 1544 | 1.38 | 0 | - | 35.1 | 0 | 0.0 |
| 910000000 | on | 2535 | 2179 | 2.01 | 286 | 19 | 200.8 | 793 | 139.9 |
| 910000001 | off | 1897 | 1568 | 1.40 | 0 | - | 35.0 | 0 | 0.0 |
| 910000001 | on | 2218 | 1881 | 1.76 | 386 | 15 | 204.1 | 886 | 141.7 |
| 910000002 | off | 1636 | 1372 | 1.34 | 0 | - | 34.7 | 0 | 0.0 |
| 910000002 | on | 2179 | 1813 | 1.69 | 314 | 19 | 248.2 | 1023 | 187.2 |

Totals, on vs off:

- decisions ×1.29 and steps ×1.31;
- wall time ×1.32, but **seconds per decision only ×1.03**, so the new observation features are cheap and the cost
  is the number of decisions;
- **final state ×6.2**, almost all of it the journal, which grows by roughly 50-60 KB per round.

Note: the first run's filter also counted Diplomacy strategy-card decisions, 1-2 per "off" game. It is fixed in the
example, and the table above omits those "off" counts.

**Reading:**

1. A policy never trained with contact options, greedy, opens about 100 contacts per round across six seats.
   Contacts are free actions, so it takes them until the per-pair allowance runs out. Expect this until the turn
   and diplomacy heads learn otherwise.
2. The journal, not feature extraction, is the scaling problem for long games and captured states.

### Next steps: need a decision or approval

- **Journal placement or bounding.** Options: move the journal out of `GameState` into `Game`, so it stays
  exportable but is not part of snapshots or digests; compact `Offered`/`Countered` events to ids plus a bounded
  revision table; or drain it into the diplomacy log at capture checkpoints. Any of these changes the Stage 9
  contract and the replay test.
- **Contact inflation.** If it persists after training, narrow when contacts are offered: only players with at
  least one surviving bundle, or cap contacts per turn. That changes the user's per-target rule and needs the user.
- **Persisting a migrated checkpoint** needs a user-chosen output folder.
- Unchanged: committing, merging onto the D: fixes, and running or re-baselining ti4-sim need approval.
