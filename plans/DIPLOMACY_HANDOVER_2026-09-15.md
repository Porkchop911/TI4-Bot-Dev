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

## Addendum, 2026-09-15 night: commits, merge, migrated checkpoint, ti4-sim v38

The user approved committing both branches, the merge and the ti4-sim run, and chose bisect-then-re-baseline.

### Commits

**On `codex/fix-six-faction-leaders`, in the shared D: checkout:**

- `896fb28` holds Claude's session engine and training fixes only.
- The other session's uncommitted example and script edits, and `target-cuda-repack/`, were left untouched.

**On `codex/diplomacy-v1`, in this worktree:**

| Commit | What it is |
|---|---|
| `71c398c` | Structured diplomacy |
| `4817d71` | Merge of `896fb28`; no textual conflicts |
| `62c733d` | Warfare's free tactical action now judges `DoNotActivate` promises (it activated without the diplomacy hook), plus the decision-delivery inventory registrations for `combat.rs::roll_round` (War Funding), `game.rs::ask_to_use_note` and `leaders.rs::offer_production_hero` |
| `fa72051` | `examples/migrate_bundle_to_diplomacy.rs` |
| `b15df02` | ti4-sim behaviour bounds v38, with the bisection and old/new table in `plans/evidence/M08-021.md` |

### Migrated checkpoint

- `out/blank-shaped-4layers/fracture-1x-styx16-20260915/checkpoint-19280-diplomacy-v11`.
- Schema 10, OOV v11, 15 heads, update 19280; it reads back through `bundle::read`.
- Diplomacy-off play identity for the in-memory migration: see the earlier addendum.

### Test results after the merge

- **Unit tests:** engine 1320, decision-delivery inventory 4, model 81, bridge 62, MLP 88 (`--skip bundle::`), training lib 144.
- **Policy:** the diplomacy and vocabulary tests pass.
- **Soak:** passes.
- **Support for the Throne audit:** 1000 games × 8 rounds, PASS.
- **ti4-sim:** 51 of 52 pass with v38.
  - `profile::tests::fixture_capture_is_deterministic` fails at every commit, including the base, because this worktree has no `out/pools`.

### Contacts

- The user reviewed the contact behaviour and keeps it as it is: contacts go to anyone, once per pair per turn.
- The journal also stays unbounded for now.

### Still open

- The D: branch `codex/fix-six-faction-leaders` (`896fb28`) lacks the inventory registrations and the v38 re-baseline. They exist only on `codex/diplomacy-v1`.
- No paired greedy evaluation of the migrated checkpoint has been run yet.
- No diplomacy training has been run.

## Addendum, 2026-09-15 late night: diplomacy v2 (packages R, T, S, L)

After watching games in the reviewer, the user gave feedback on four points and chose a direction for each. The
feedback, the decisions and the package designs are in `plans/DIPLOMACY_V2_PLAN_2026-09-15.md`; read that first.
This addendum records the state of the work.

### State

- **Nothing is committed.** All v2 work sits uncommitted on top of `85d0151` in this worktree. Committing needs
  the user's approval; Claude asked and has no answer yet.
- `git diff --stat` shows 49 files. About 30 of them hold only rustfmt reflows from `cargo fmt --all`, in code that
  came in with the merge (examples, `galaxy.rs`, `action_cards.rs`, `deck.rs` and similar). They can go in a
  separate formatting commit.

### What was built

**R, reviewer readability**

- The left panel is light with black text. Seat colour appears only as a swatch.
- Relationship cells show a stance word (friendly, neutral, wary, hostile) with a written legend, in both the
  native window and the HTML export. The thresholds are in `ti4-review/src/diplomacy.rs::stance`.
- `ti4-review/src/diplomacy.rs` (new) holds all the plain-language text: terms, deals, signals, the journal and
  relationship changes.
- Structured diplomacy is switched on with the GUI checkbox or `simulate --diplomacy`. See
  `plans/REVIEWER_HOWTO.md`.

**T, transactions inside contacts**

- With diplomacy on, `transactions::available_actions` returns nothing.
- A contact with a transaction partner adds up to 12 `DealTemplate::Trade` bundles. They are built from
  `offer_options` and `offer_from` through `transfers::assets_of`.
- `game.rs::open_contact` emits `TRANSACTION_OPENED` and records the transaction.
- `step_diplomacy` emits `TRANSACTION_RESOLVED` and `TRANSACTION` when immediate transfers apply.
- An accepted immediate-only bundle gives `FairTransaction` when it is fair.

**S, concrete signals**

- `SignalStatement` replaces the old subject and condition. Its variants are `WillNotAttack`, `StayOutOf{system}`,
  `DoNotAttackMe`, `RetaliateIfAttacked` and `AttackIfYouActivate{system}`. The kind is derived from the statement.
- `SignalStatus` values: Open, Honoured, Broken, Heeded, Ignored, Triggered, CarriedOut, Bluffed.
- `signals::judge_event` runs from `promises::evaluate_event`, and `signals::settle_deadlines` runs from
  `settle_deadlines`. Each judgement journals `DiplomacyEvent::SignalJudged` and applies one relationship event
  from the table in `relations.rs`.
- `candidates::generate_signal_statements` offers at most 6 statements per contact, and names only contested
  systems.

**L, new tradeables and favours**

- **New terms:** `DealTerm::UseLeaderFor`, `SkipSecondary` and `ReturnNote`. `FollowSecondary` was dropped
  because following almost never helps the card's holder.
- **Judging:**
  - `DiplomacyEventContext::LeaderUsedFor` is raised in `leaders.rs`: by `hacanagent` when it replenishes another
    seat, and by `l1z1xagent` when it swaps a mech for the active seat.
  - `SecondaryResolved` is raised in `game.rs::step_secondary`.
  - At the deadline, `SkipSecondary` counts as kept. `ReturnNote` counts as kept if the note sits with its owner
    (`promises::note_is_home`).
- **Templates:** `PayForAgentFavour`, `SellAgentFavour`, `PayToSkipSecondary`, `NoteForNonAggression`,
  `NoteLoan`, and `PayForVote` (only when `CandidateContext::agenda` is set).
- **Cap:** `cap_across_templates` takes templates in turns up to 36. Plain truncation of the id-sorted list had
  been cutting whole templates alphabetically.
- **Agenda talks** (`game.rs`):
  - With diplomacy on and a map, `open_next_vote` stores `AgendaTalks` and resets transactions and pair
    allowances.
  - `agenda_talks_choice` asks each seat in voting order (speaker last) the `diplomacy_agenda_talks` decision:
    open contacts, or `candidates::END_TALKS_ID`.
  - `start_vote` opens the vote once no seat is still negotiating.
  - The site is registered in `tests/decision_delivery_inventory.rs`.
- **Rule change:** `transactions::why_illegal` skips the neighbour check in `Phase::Agenda`, because anyone may
  transact during the agenda phase (94).
- **Duplicate factions:** `available_contacts` skips a target whose faction another seat shares. Contact ids name
  the target by faction, so such a contact could resolve to the wrong seat and be refused.

### Contract changes to be aware of

- **Serialized diplomacy data:**
  - `Signal` now requires `statement`, and `SignalSubject`/`SignalCondition` are gone.
  - `DealTerm`, `DealTemplate` and `DiplomacyEvent` gained variants.
  - Diplomacy-on journals, reviews or corpus files written before v2 will not deserialize. Diplomacy-off data is
    unaffected.
- **Relationship events:** `RelationshipEvent::PublicThreat` and `PublicWarning` were replaced by `SignalMade`,
  `AssuranceKept`, `AssuranceBroken`, `RequestHeeded`, `RequestIgnored`, `ThreatCarriedOut` and `ThreatBluffed`.
- **Bridge:** `ti4-bridge` `SignalDraft` now carries `statement: SignalStatement`.
- **Policy features:** there are new feature names under `diplomacy:`, for example
  `diplomacy:template:pay_for_agent_favour` and the term kinds `agent-favour`, `skip-secondary` and
  `return-note`. They fall into OOV buckets for the migrated checkpoint.
- **Decision subtype:** `diplomacy_agenda_talks` is new. Its options are `open_diplomacy`, so it routes to the
  diplomacy head.
- **`CandidateContext`** has a new field, `agenda: Option<&str>`.

### Evidence (this worktree, final state)

- **Tests:** engine lib 1330, decision-delivery inventory 4, model 81, bridge lib 62, policy `diplomacy` 1, MLP lib
  88 (`--skip bundle::`), reviewer 33.
- **Build:** `cargo check --workspace --all-targets` is clean.
- **New tests:**
  - `promises::tests::favours_settle_on_the_moment_they_name`
  - `promises::tests::a_lent_note_is_judged_by_where_it_sits_at_the_deadline`
  - `candidates::tests::favours_and_votes_are_offered_only_when_their_components_exist`
  - `candidates::tests::a_contact_is_offered_only_to_a_seat_its_faction_names_unambiguously`
  - `candidates::tests::signals_are_concrete_and_name_only_contested_systems`
  - `candidates::tests::with_diplomacy_on_trading_happens_inside_the_contact`
  - `game::tests::with_diplomacy_on_voters_negotiate_before_each_agenda_vote`
  - reviewer `favours_read_as_what_the_promiser_will_do`
  - reviewer `a_signal_reads_as_the_sentence_sent_and_what_came_of_it`
- **Soak** (release, 6 seeds × 3 rounds): no refused step, and replay is identical.

  | Count | Value |
  |---|---|
  | offered | 261 |
  | countered | 131 |
  | accepted | 130 |
  | declined | 131 |
  | settled | 99 |
  | signals | 222 |
  | signal outcomes | 152 |
  | transactions | 3 |
  | favours | 2 (1 note loan, 1 skipped secondary, 0 agent favours) |

- **Environmental failure:** `ti4-bridge --test hexsummary_golden` fails because
  `tests/golden/hexsummary_captures.json` exists neither here nor in D:. It is not caused by this work.

### Open problems and recommendations

1. **Commit (needs approval).** Suggested split:
   - the formatting-only reflows;
   - R and T;
   - S;
   - L.
2. **Run ti4-sim before any merge.**
   - Engine paths changed: the hooks in `leaders.rs` and `step_secondary`, which return early when diplomacy is
     off; the agenda-phase exemption in `why_illegal`; and the `open_contact` refactor.
   - Diplomacy-off play should not move from v38. If it does, bisect before re-baselining, and re-baseline only
     with approval.
3. **Favours barely occur.** Most favour templates need trade goods, which seats rarely hold early.
   - Consider goods-free swaps: an agent favour for non-aggression, or a skipped secondary for a skipped
     secondary.
   - Run a longer soak that reaches the agenda phase (`DIPLOMACY_SOAK_SEEDS=60 DIPLOMACY_SOAK_ROUNDS=6`). The soak
     only asserts `favours > 0` in total.
4. **Cost.**
   - Agenda talks let every seat contact every other seat once per agenda, on top of the ×1.29 decision increase
     measured earlier.
   - The ignored `diplomacy_cost_probe` still asserts `max_options <= 29`, but a contact can now offer 36 bundles,
     6 signals and a no-op. Update the bound and re-measure.
5. **Contact ids by seat.** Naming the target by faction forced the duplicate-faction skip. An id that carries the
   seat would remove it; policy features and the reviewer parse the faction from the id, so update them too.
6. **Not built from the plan:**
   - the agenda-phase assurance signal ("I will vote OUTCOME");
   - `UseLeaderFor` hooks for leaders other than the two agents.
7. **Not yet checked:**
   - clippy on the v2 code;
   - the HTML export rendered in a browser for the S and L text. R was checked in a browser; S and L only in code.
8. **Unchanged from the previous addendum:**
   - the D: branch lacks the inventory and v38 commits;
   - no paired greedy evaluation of the migrated checkpoint has been run;
   - no diplomacy training has been run.

## Codex continuation addendum — 2026-09-15

Superseded as the immediate operational resume point by
`plans/CLAUDE_HANDOVER_2026-09-16.md`; retain this file for the fuller implementation history.

The user authorized continuation. This work remains uncommitted pending the required independent Tier-C review.
Exact evidence is in `plans/evidence/DIPLOMACY_V2_CONTINUATION_2026-09-15.md`.

### Completed after the Claude handover

- Updated the cost probe to derive the 43-option cap from engine constants; the probe passes and observed only
  8 diplomacy options in its fixed sample.
- Replaced faction-addressed contact IDs with seating-order indices. Duplicate factions are no longer skipped;
  engine resolution, policy relationship features, and reviewer naming agree on the target seat.
- Added the agenda assurance `WillVote { agenda, outcome }`. It is offered only during concrete agenda talks,
  settles from the typed recorded ballot, renders as a sentence, and carries bounded signal kind/statement
  policy features.
- Versioned the breaking v2 output: `ti4-diplomacy-log-v2`, `ti4-offline-selfplay-v3`, and
  `seat-authorized-canonical-mlp-v3`. Older declared schema pairs remain accepted by readers.
- Ran the requested 60-seed x 6-round release soak: 6,036 offers, 3,001 accepts, 3,370 counters, 3,834 signals,
  195 transactions, and 69 favours (14 agent / 43 note loan / 12 skipped secondary), with no refused step.
- Full affected tests pass: model 81; engine 1,331 plus integration/docs; policy 248; bridge 62; reviewer 33;
  MLP 88 with bundle tests skipped; capture example 20; offline-BC example 6. Focused strict Clippy passes with
  only the repository's documented unrelated baseline lints allowed.
- Diplomacy-off simulation reproduced inside v38. The same single environmental fixture failure remains because
  this checkout lacks the map pool; no rebaseline was performed.

### Remaining blockers

1. `DiplomacyState::journal` is still unbounded. Do not truncate it: that would break the complete authenticated
   export. Move lossless journal ownership out of checkpointed game state (or design an equivalent sidecar) in a
   separately reviewed package.
2. `UseLeaderFor` still supports only the Hacan and L1Z1X agent hooks.
3. S/L reviewer HTML still needs a manual browser rendering check.
4. Paired greedy evaluation, a fresh v3 corpus, BC, and later PPO have not run.
5. Independent Tier-C review is required before committing/integrating the legality and schema changes.

### Exploratory overnight PPO

- Added an explicit opt-in `--diplomacy` path to the PPO driver and a capability-aware rollout
  wrapper. Legacy PPO remains diplomacy-disabled by default, and `--diplomacy` refuses bundles
  without the appended diplomacy head.
- A one-update CUDA smoke test passed: 96 two-round games, 114,689 decisions, 13.3 seconds total,
  parameters moved, and Adam advanced.
- The first run (PID 58692) stopped at its update-25 publish boundary: `GIT_COMMIT` included a
  descriptive dirty-tree suffix, but bundle provenance admits only 7–64 hexadecimal characters.
  `checkpoint-5988.tmp` contains staged tensors but no manifest and is not a usable checkpoint.
- Corrected active run: PID 77484, 1,000 four-round updates, schema-10 migrated checkpoint,
  learning rate 1e-4, temperature 1.0, movement entropy 0.05, entropy schedule ending at 0.25,
  and faction waste penalties `15,12,5,5,8,8`. Output and logs:
  `D:/Projects/ti4-engine-rs/out/ppo-diplomacy-overnight-waste-20260915`. Its update-1
  `checkpoint-240` was published with a manifest and reloaded identically.
- First full update passed: 242,532 decisions, 24.1-second rollout, 9.8-second CUDA optimization,
  33.9 seconds total, 2.25% clipped. Projected duration is about 9.5 hours. This exploratory run
  does not replace the paired evaluation, fresh-corpus BC, replay, or qualification gates.
