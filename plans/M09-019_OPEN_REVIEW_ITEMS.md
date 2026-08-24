# M09-019 open review items

Ledger for the post-rules baseline/profile row (children a/b). Findings are recorded here as they
arise; dispositions must be resolved before M09-019 closes and before M09-030.

## M09-019a observations (implementer, 2026-08-23) — pending Tier-D review

### O-M09-019a-1 — per-game records omit wall-clock by design — INFO

`panel.json` game records carry no `seconds` field. This is deliberate: the determinism proof
compares runs byte-for-byte, and a wall-clock field would make every comparison diverge (the
M08-019 `GameResult.seconds` trap). Timing evidence belongs to M09-019b under the M00 protocol,
where raw samples are preserved separately from semantic results.

### O-M09-019a-2 — zero completions at the 4-round horizon is a measurement, not a failure — INFO

All 30 baseline games ended `horizon_reached` with mean VP ≈ 2.2–2.7 per seat: the r6 champion
does not finish full games on the corrected engine within its training horizon. This is exactly
the kind of post-rules behavior §8 says must be measured before any MLP comparison. Consequence:
outcome-level (winner/completion) metrics have no spread at this horizon; if a later package needs
discriminative outcome evidence, it must choose and record a longer horizon — that is a scope
decision for M09-019b or its reviewer, not something to change silently here.

### O-M09-019a-3 — no committed test plays the real out/ artifacts — LOW (known gap)

The hermetic determinism test uses a synthetic six-slot pool; the real-artifact path is exercised
by the example binary and recorded in evidence, but no *committed test* loads `out/stage2_r6` or
the pools. This matches F-M09-018-4 (no committed real-artifact import test anywhere yet) and is
the natural home of M09-020's durable fixtures: once the baselines are archived into Git, a
committed test can verify them without depending on gitignored local data.

## Status

M09-019a: **accepted by fresh independent Tier-D pass-1 recheck of `1a06ca9` on 2026-08-24.**
M09-019b is dependency-ready and retains the row's second Tier-D review pass.

---

## M09-019a independent Tier-D review 1 on `7ccae2e` (Codex frontier, 2026-08-23)

**Verdict: changes required; M09-019a is not accepted.** The recorded baseline independently
reproduces byte-for-byte on the named artifacts, but the panel command does not implement the
fail-closed contract it claims.

### F-M09-019a-1 — HIGH: checkpoint manifest checksum is never enforced

`run_panel` verifies only `pool_sha_prefix`; `Champions::load_checkpoint_accepted` computes and
reports the checkpoint digest but compares it to nothing. The example likewise hashes the
checkpoint only for before/after equality. Any parseable envelope with faction-valid profiles can
replace `final10000.json`, run successfully, and be reported as the r6 baseline even though MLP
plan §10 requires every panel command to validate artifact role/checksum before starting.

**Required:** add the manifest checkpoint prefix (`be792a2a207ced25`) to the fail-closed input
contract and verify it against the exact bytes deserialized before any game runs. Add a focused
test proving a valid-but-wrong checkpoint is rejected. Avoid a verify-then-reread gap: checksum and
deserialization should be derived from the same byte buffer.

### F-M09-019a-2 — HIGH: failed games still produce a successful baseline command

`play_learned` represents missing champions, deployment faults, and engine failures as
`GameResult.error`; `run_panel` unconditionally wraps the collected games in `Ok(PanelReport)`.
The example then writes `panel.json`, prints the failed count, and returns `Ok(())` regardless of
that count. A panel with 30 failed games therefore exits successfully and can look like evidence,
contrary to the repository rule that a worker/game failure must never become apparent success.

**Required:** make the acceptance path fail closed when any game has `error` (with failing seeds
and reasons preserved), before writing/reporting an accepted baseline. Add a focused test using a
missing required champion or another deterministic failure and assert the panel returns an error;
retain the per-game failure detail in the error/evidence path. An empty seed panel should likewise
be refused rather than accepted as a zero-game baseline.

### What passed independently

- Named artifacts match the manifest exactly: checkpoint
  `be792a2a207ced25d589162d875bae4fb1f320c8e5637045486db6a24ce5b55b`; validation pool
  `aba33c81aa04cefb15857b8ed1d40173f6f3de5e9b6e9633a6855c1d5a4c27e5`.
- `cargo test -p ti4-sim --lib baseline` — **3/0**.
- `cargo test -p ti4-sim` — **35/0**, doc tests 0/0.
- `cargo clippy -p ti4-sim --all-targets` — no ti4-sim warning; only the two recorded pre-existing
  engine warnings. `cargo fmt -p ti4-sim --check` and commit diff-check clean.
- Independent `cargo run --release -p ti4-sim --example m09_019a_panel` reproduced 30 games,
  0 failed, 0 completed, 33,825 decisions, the six quoted VP means, and output sha256
  `c94788677d73de9ee5359f0954a258ba2dea4875938a0161bc5f0b3f9f06cd4e`; both input hashes were
  unchanged afterward.

The measured numbers need not be discarded: the independently inspected inputs were the intended
ones and this run had zero failures. Fix both gates, rerun the focused/full sim checks and real
panel, update evidence, then request a fresh Tier-D pass-1 recheck. M09-019b should not begin on
this branch until M09-019a is accepted.

---

## F-M09-019a-1/2 correction round (implementer, 2026-08-23)

- **F-M09-019a-1 resolved:** `R6_CHECKPOINT_SHA_PREFIX` pins the §10 manifest identity.
  `Champions::load_checkpoint_accepted` now hashes one byte buffer, checks the prefix, and
  deserializes that same buffer, avoiding a verify-then-reread gap. A valid profile envelope with
  the wrong expected prefix is refused by a focused test.
- **F-M09-019a-2 resolved:** `run_panel` refuses an empty seed list and converts any collected
  `GameResult.error` into `BaselineError::GameFailures`, retaining every failing seed and reason.
  The example cannot reach its output write on either error. A focused test supplies only five of
  six required champions and verifies the returned error names seed 919001 and the missing-profile
  reason.
- **Reruns:** focused baseline **4/0**; ti4-sim **36/0**; ti4-sim Clippy/rustfmt clean (only the two
  recorded pre-existing engine warnings); diff-check clean. The release panel remains 30 games,
  0 failed, 0 completed, 33,825 decisions with identical VP means and output sha256 `c9478867…`;
  checkpoint and pool hashes remain manifest-identical.

**Status: both Tier-D pass-1 findings resolved; requesting a fresh independent Tier-D recheck.**
M09-019b remains pending until that acceptance.

---

## Fresh Tier-D pass-1 recheck of `1a06ca9` (Codex frontier, 2026-08-24)

**Verdict: ACCEPTED.** F-M09-019a-1 and F-M09-019a-2 are resolved. The checkpoint digest is
compared with the manifest prefix before deserializing that same immutable byte buffer. Empty
panels return `EmptyPanel` before artifact access. Every collected `GameResult.error` is converted
to `GameFailures` before an accepted report reaches the example's write path, retaining the
failing seed and reason. No new actionable finding was identified in the correction diff.

Independent gates: baseline tests **4/0**; ti4-sim **36/0**, doc tests **0/0**; ti4-sim Clippy has
no package warning (only the two recorded pre-existing engine warnings); scoped rustfmt and commit
diff-check clean. The real release panel reproduced 30 games, 0 failed, 0 completed, 33,825
decisions and the six recorded VP means. `panel.json` remains byte-identical at sha256
`c94788677d73de9ee5359f0954a258ba2dea4875938a0161bc5f0b3f9f06cd4e`; input hashes remain
validation pool `aba33c81…` and checkpoint `be792a2a…`.

O-M09-019a-1/2 are accepted as measurements/design choices. O-M09-019a-3 remains a recorded LOW
gap owned by M09-020's durable-fixture work, not a blocker for M09-019a. M09-019b is now
dependency-ready; its completion still requires the row's Tier-D pass 2.

---

## Independent Tier-D frontier review — pass 2 over `624d91c` (2026-08-24)

**Verdict: changes required; M09-019b and parent row 019 are not accepted.** Focused profile tests
6/0, the inventory pin 1/0, workspace 1,347/0, and scoped Clippy pass. The independently rerun
release campaign passes every semantic gate but exposes protocol/evidence defects below.

### F-M09-019b-1 — mandatory variance repeat and disposition are not implemented — HIGH

M00-012e and the package spec require one fresh same-build 30-sample repeat whenever either
threshold fails, retention of both reports, then `unstable` only if either run passes or
`rejected_variance` if both fail. `run_campaign` executes each workload once, has no repeat/result
state, and overwrites one fixed filename. The evidence labels the first failed release run
`unstable`; the debug build is not a repeat of the release workload. The independent release rerun
also failed both thresholds for W1/W2/W3 (9.37/14.55%, 18.30/39.78%, 16.28/36.36%), which would
make all three `rejected_variance` if both valid runs were retained. Implement the fixed policy,
retain run 1 and run 2 separately, never combine them, and report the correct disposition.

### F-M09-019b-2 — input identity and report publication are not one fail-closed boundary — HIGH

The pool is hashed from one `fs::read` and then reopened by `MapPool::load`, so the bytes consumed
need not be the bytes approved. Only the pool is hashed after the campaign; the checkpoint has no
after-hash despite the package's explicit before/after claim. Each workload report is written
immediately, before later workload gates and the final input check, so a failed campaign or changed
input can leave valid-looking partial reports. Parse the verified pool bytes, verify both inputs
afterward, assemble all reports in memory, and publish atomically only after every semantic,
variance, and integrity gate completes. Add failure-path tests proving no accepted report survives.

### F-M09-019b-3 — recorded timings are not tied to the source tree measured — HIGH

All six evidence reports name `rust_commit = 22a7fa7`, the parent commit before profile.rs and the
pinning test existed. They measured an uncommitted working tree, so that commit cannot reproduce
the measured program. The independent rerun at `624d91c` correctly records that commit but produces
different samples/hashes and cannot silently replace the claimed evidence. After corrections are
committed, run the final campaign from a clean reviewed commit and record its exact commit and raw
report hashes; reject a dirty tree or record a complete diff identity rather than attributing it to
HEAD alone.

### F-M09-019b-4 — population stdev violates the fixed sample-stdev rule — MEDIUM

M00-012b requires sample standard deviation. `ProfileReport::assemble` delegates to
`benchmark::Statistics::over`, which divides squared deviations by `n`; the package spec explicitly
acknowledges this population convention and substitutes it for the normative calculation. Use
`n - 1` for the M00 report/threshold and add a fixture that distinguishes the two formulas. A claim
that the difference is unlikely to flip a threshold does not satisfy an exact protocol.

### F-M09-019b-5 — required M00 audit metadata/equality behavior is absent — MEDIUM

M00-012c requires the Windows processor group, actual current process affinity, and an operator
assertion that no competing benchmark/simulation process is known to be running. The report records
only the literal `inherited; unchanged by the runner`, and evidence repeats it without the group,
mask, or operator assertion. M00-012a also says warmup output is retained locally, but the runner
discards every warmup result. Finally, M00-012d excludes `captured_at_utc` from hash/equality, while
`ProfileReport` derives equality over it and evidence hashes the timestamped file directly. Record
the required audit data/warmups and define a canonical equality/hash projection that omits only the
timestamp.

### F-M09-019b-6 — feature inventory and pin do not cover the claimed vocabulary — MEDIUM

The evidence aggregates all 13 legacy prefixes into one row rather than cataloguing each family's
name shape, extractor, factual-vs-hashed status, and head mapping as required. It claims the pinning
test asserts all four rows verbatim, but the test asserts only `STAGE1_DECISION_HEADS`, not the
19-entry `DECISION_HEADS` row, and it does not define a closed explicit-family vocabulary: an
unexercised explicit family can be added without breaking the pin. Supply the per-family inventory
and tests that pin both head lists and exact legacy/explicit family sets.

### F-M09-019b-7 — the stated rustfmt gate is not clean — LOW

`cargo fmt -p ti4-sim -p ti4-policy --check` exits 1. Two diffs at features.rs:690/752 are the
recorded pre-existing drift, but the added assertion around line 1888 is a new package-owned diff.
Format the new test and report the scoped/pre-existing distinction accurately.

### Independent execution evidence

- Profile unit tests **6/0**; feature inventory pin **1/0**; workspace **1,347/0**.
- Clippy: no new ti4-sim/ti4-policy warning; two recorded pre-existing engine warnings.
- Rustfmt: failed as described in F-M09-019b-7; commit diff-check clean.
- The original three release raw reports were preserved under ignored
  `out/profiles/review-624d91c-original/` before the reviewer rerun. The rerun at exact commit
  `624d91c` passed all semantic gates and reproduced input hashes, but all variance thresholds
  failed and the runner overwrote the primary report paths, independently confirming F1/F3.
