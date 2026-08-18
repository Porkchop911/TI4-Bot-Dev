#!/usr/bin/env bash
# Stage 2 with PPO, resuming from the Stage-1 table.
#
# PPO rather than REINFORCE because the null that argued against it was measured in a different
# regime. Every arm of that pilot resumed from a CONVERGED Stage-2 champion, where PPO was -0.020
# table VP. This starts from Stage-1 weights that have never seen the four-round VP objective --
# close to a fresh start for Stage 2's purposes, which is the regime where PPO won by 3.3-4.0x at
# Stage 1. Applying the polishing-regime result here was the mistake.
#
# --rollout-depth is deliberately absent: PPO refuses it, because a wave hands a batch to weights
# that have since moved and the importance ratio would then measure the scheduler's staleness
# rather than the epochs. So this gives up ~8% scheduling throughput on top of PPO's 1.5x per
# update. Equal games is preserved regardless -- one update is 96 games for every configuration.
#
# The Stage-1 strategy-head defect does NOT apply here. It is degenerate at Stage 1 because the
# draft is the first decision of a one-round game, so the seat state is the setup state and the
# cross is a constant multiple of card identity. At four rounds the draft recurs each round with
# spent tokens and gained planets, so the same features carry real information.
#
# 8 seeds x 4 threads = 32 logical cores. Eight seeds rather than three because seed variance has
# been larger than the effects being measured all session -- jolnar spanned 0.34 to 0.98 at Stage 1
# from identical zero weights.
set -u
cd "$(dirname "$0")/../.." || exit 1

POOL="D:/Projects/ti4-engine/data/map_pools/save52_e400_n8192.json.gz"
START="out/prod2/stage1_ppo_s0.json"
UPDATES=${UPDATES:-1000}
OUT="out/stage2_ppo"

# Nothing an arm overrides may appear here: the parser takes the FIRST occurrence of a flag.
BASE="--updates $UPDATES --every 250 --no-boundaries --train-seeds 16 --rounds 4
      --entropy 0.05 --high-vp-bonus 1.0 --clearance-weight 0 --round-baseline
      --ppo-epochs 4 --ppo-clip 0.2 --scramble-seats
      --map-pool $POOL --checkpoint $START"

echo "stage-2 PPO starting $(date)  updates=$UPDATES  8 seeds x 4 threads"
for idx in 0 1 2 3 4 5 6 7; do
  RAYON_NUM_THREADS=4 "$OUT/trainer.exe" $BASE \
    --train-seed-base $((93000000 + idx * 1000000)) \
    --out "$OUT/s${idx}.json" > "$OUT/s${idx}.log" 2>&1 &
done
wait
echo "complete $(date)"
