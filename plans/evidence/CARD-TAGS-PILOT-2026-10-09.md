# Card-tag pilot (2026-10-09)

Operator decisions: roster wide; no noise-floor replicates first; GPU released after 20 min idle; strata
stopped with operator permission (not restarted).

Arms: `scripts/pilot_cardtag_T.psd1` (start `out/card-tags-migrated-20261009/checkpoint-20`, card-tag names in
zero rows) and `scripts/pilot_cardtag_C.psd1` (start `out/trade-teacher-wide-v8-20261008/checkpoint-20`). Both
use roster wide, main-line rewards, T=2.5, 50 updates x 30 seeds, seed base 1264100000, `--device cuda`, run
concurrently. Binary: `cargo build --release -j6 -p ti4-mlp --example ppo_update --example clearance_eval` at HEAD 8432ba71
over a dirty shared tree (other sessions' uncommitted engine/training edits compiled in, the same binary for both arms).
Log `out/logs/build-cardtag-pilot-20261009.log`. Update 1 of both arms was identical (zero rows), as expected.
Final: T `out/ppo-cardtag-T-wide-pilot50-20261009/checkpoints/checkpoint-3584`, C `.../ppo-cardtag-C-wide-pilot50-20261009/checkpoints/checkpoint-3560`.

Eval: `clearance_eval --roster wide --seeds 600 --rounds 4 --temperature 0.001 --diplomacy --per-seat ...`,
holdout pool, 21,600 seats each, run sequentially, all exit 0. Logs `out/logs/eval-greedy600-cardtag-{start,T,C}-20261009.log`,
per-seat `out/eval-greedy600-cardtag-{start,T,C}-20261009.per_seat.tsv`.

| | table VP | table clearance |
|---|---|---|
| start | 3.163 | 44.66% |
| T (card tags) | 3.499 | 64.76% |
| C (control) | 3.519 | 64.78% |

Paired by seat, map-seed bootstrap (1,000 resamples, 95% CI):

| comparison | VP, original six | VP, other 27 | VP, table |
|---|---|---|---|
| T vs start | -0.024 [-0.092, +0.037] | +0.425 [+0.398, +0.455] | +0.336 [+0.313, +0.358] |
| C vs start | +0.093 [+0.028, +0.153] | +0.420 [+0.392, +0.449] | +0.355 [+0.333, +0.377] |
| T vs C | -0.118 [-0.176, -0.065] | +0.005 [-0.021, +0.030] | -0.020 [-0.041, +0.002] |

Reading:
- Wide-roster PPO from this start helps the 27 newer factions a lot in 50 updates (+0.42 VP, both arms).
- Card tags show no benefit at 50 updates: the newer factions are identical (+0.005), and the six are lower
  in T (-0.118). The intervals cover evaluation noise only. Training variance is unmeasured (no replicates by
  operator choice), so the -0.118 on the six may be run-to-run spread rather than an effect of the tags.
- Clearance on the wide roster uses the six-faction opening bar; read VP.

Verdict: no measurable benefit at 50 updates, and no table-level cost (-0.02 VP, CI includes zero).
Operator decision (2026-10-09): keep card tags. The arm T line (checkpoint-3584) continues with the features on.
Watch the original six: their -0.118 VP vs control is unresolved without replicates; check it in later paired evals.
