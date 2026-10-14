# Saar saved replay diagnosis — 2026-10-12

Scope: operator reports free 2 infantry + 2 fighters during Warfare, production after a carrier moves, and influence payment. Original `out/saarreplay.r02.json.zst` preserved. No training, commits, or golden refresh. Artifact names retain the investigation's 20261011 start date.

## Recorded facts

Validated project using `ti4_replayer::persistence::load_project` (schema/checksum validation). Cached current debug libraries reconstruct branch0 for402frames and replay455 recorded choices with no Gate identity divergence, policy failure, or frame error. No original frame snapshots exist in this project, so this is a current-engine reconstruction matching the recorded decision prefix, not a frame-fingerprint comparison against the original running binary. Source project names historical engine37305da5; current checkout HEADa8ed0253 plus dirty work.

- Frame178: Saar uses TE Warfare, activating110 without a command token.
- Frames185–186: Saar's Floating Factory moves independently from11 into110. The `load saar_spacedock` question asks what the moving factory carries; it is not a factory being loaded onto a carrier. Factory production5 explains the production offer.
- Frame192: Winnu seat4 uses Berekar Berekon, then Yssaril seat0 copies it with Ssruu. Both agents exhaust; each reduces the combined bill by2, totaling4. Corpus and faction hook support cross-player production discounts.
- Frames193–194: infantry pair and fighter pair each have printed cost1, effective cost0. Saar TG3 and exhausted planet set remain unchanged. Four new units are real, but the zero cost follows the two legitimate agent discounts. Production-context `paid`0/2/4 measures capacity consumed, not resources paid. Earlier informal free-bug/automatic-payment hypotheses are superseded by this reconstructed state.
- Frames399–401: later carrier moves to106. Saved project ends after loading; it does not record that move's invasion/production/payment.

Artifacts: `out/logs/saarreplay-current-reconstruction-20261011.json`; source `saarreplay-reconstruct-20261011.rs`; raw log `check-saarreplay-reconstruct-20261011.log` (exit0). Assert script `check-saarreplay-state-20261011.py` passes.

## Explicit synthetic continuations, separate from the saved choices

Finish the carrier move, land on Cresius: no producer, no ordinary build offer; the negotiation option misleadingly said `continue to production` before immediately ending the action.

Finish the same move, land on Lazul Rex, select CULTURAL exploration: draws frln3 Freelancers. This grants one produced unit without a factory and explicitly permits influence as resources. Reconstructed payment offers `exhaust|ragh|influence`; diagnostic policy chooses a dreadnought and pays with TG/resource faces. Influence permission is empty after resolution; no ordinary PRODUCTION_USED event fires. The INDUSTRIAL alternative draws biotic and has no build offer. These continuations reproduce a legal explanation for the remaining reports, but do not establish the human's unrecorded choices.

Artifacts: `saarreplay-synthetic-carrier-continuation-20261011.json`, `saarreplay-synthetic-lazul-continuation-20261011.json`, `saarreplay-synthetic-cultural-continuation-20261011.json` in out/logs, corresponding scratch Rust source/executable/logs. Cultural state assert `check-saarreplay-cultural-state-20261011.py` passes.

## Minimal source change and verification

Luna authored, root frontier independently source-reviewed:
- game.rs: preserve continue_action ID/kind/context/contacts; label the post-invasion pause `finish the tactical action` when current production capacity0, otherwise keep `continue to production`. Invasion label unchanged. No timing/payment/legality change.
- game.rs regression covers carrier-only, factory-present, and invasion labels.
- production.rs two regressions: actual Saar carrier/infantry/fighter without factory offers no production and query leaves state unchanged; ordinary resource production does not count a controlled zero-resource influence planet or expose influence payment.

Checks:
- Initial new payment test compile failed on iterator reference depth; Luna repaired the two predicate sites. Original log retained: check-saar-production-resources-20261011.log.
- Focused repaired payment test1passed, exit0: check-saar-production-resources-fixed-20261011.log.
- Before wording edit engine lib2815passed/0failed/1ignored, exit0: check-saar-engine-lib-20261011.log.
- Final engine lib2816passed/0failed/1ignored, exit0: check-saar-engine-label-lib-20261011.log, Cargo-j8/testthreads8.
- Final replayer live integration15passed/0failed, exit0: check-saar-label-live-20261011.log.
- git diff --check passes (existing CRLF notices only).

Independent gpt-5.6-luna verified recorded agent discounts/factory move and synthetic Freelancers content/temporary-flag cleanup. Root rejected its initial mistaken factory-as-cargo interpretation and payment reading of ProductionCapacity counters; corrected analysis is above. Root reviewed the separate Luna-authored label/test diff against enter_production's zero-capacity early return and shared production/payment callers.

Limits: original replay stops before the later exploration; no claim the human selected CULTURAL. No full workspace rerun; prior sole semantic-golden failure remains open. Desktop executable not rebuilt/relaunched; source/tests updated, existing running UI may retain old wording.
