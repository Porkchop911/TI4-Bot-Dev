#!/usr/bin/env bash
# Paired 360-game greedy holdout evals of the R long run against the C pilot's final checkpoint,
# at every 200 updates. 12 Rayon threads so training keeps most of the CPU.
set -u
main=/d/Projects/ti4-engine-rs
run=$main/out/ppo-rewardterms-R-long1000-20261017
baseline=$main/out/ppo-rewardterms-C-pilot50e-20261016/eval60.per-seat.tsv
ev=$main/.claude/worktrees/reward-terms-training-e8c88a/target/release/examples/clearance_eval.exe
export LIBTORCH="D:/Projects/ti4-engine-rs/out/libtorch-2.9.1-cu128" LIBTORCH_BYPASS_VERSION_CHECK=1
export RAYON_NUM_THREADS=12 OMP_NUM_THREADS=1 MKL_NUM_THREADS=1
export PATH="$main/out/libtorch-2.9.1-cu128/lib:$PATH"
for n in 200 400 600 800 1000; do
  ck=""
  until [ -n "$ck" ]; do
    for dir in "$run"/checkpoints/checkpoint-*; do
      grep -q "\"M10-034 PPO, $n update(s)" "$dir/manifest.json" 2>/dev/null && ck=$dir
    done
    [ -n "$ck" ] && break
    # Trainer gone without this checkpoint: stop.
    tasklist | grep -qi ppo_update || { echo "update $n: trainer exited without a checkpoint"; exit 1; }
    sleep 60
  done
  out=$run/eval-$n
  "$ev" --bundle "$ck" --map-pool "$main/out/pools/full_np8_12_holdout.json" --roster wide --seeds 60 \
    --seed-base 900000000 --rounds 4 --temperature 0.001 --diplomacy --per-seat "$out.per-seat.tsv" > "$out.log" 2>&1
  echo "update $n: eval exit $? skipped $(grep -c SKIPPED "$out.log") $(grep -E '^  table' "$out.log")"
  C:/Python314/python.exe -B "$main/scripts/paired_vp_bootstrap_20261011.py" "$baseline" "$out.per-seat.tsv" > "$out.paired.json" 2>&1
  C:/Python314/python.exe -c "import json,sys; g=json.load(open(sys.argv[1]))['groups']['table']; print(f'update $n: R-C {g[\"paired_delta_vp\"]:+.3f} VP, 95% {g[\"paired_delta_ci95_vp\"]}')" "$out.paired.json"
done
