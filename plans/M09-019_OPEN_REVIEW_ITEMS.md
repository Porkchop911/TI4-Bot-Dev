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

M09-019a: implementation complete, **pending independent Tier-D frontier review** (first of two).
No findings yet; observations above are recorded for the reviewer's disposition.

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
