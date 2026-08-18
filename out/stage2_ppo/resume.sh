#!/usr/bin/env bash
# Resume the stage-2 PPO run from its last checkpoints.
#
# The trainer reads `final_update` from the checkpoint and continues the training seed stream from
# there, so resuming replays no games. It refuses --checkpoint and --out pointing at one file, so
# each seed's checkpoint is copied aside and read from the copy.
#
#   UPDATES=500 bash out/stage2_ppo/resume.sh     # 500 more updates per seed
set -u
cd "$(dirname "$0")/../.." || exit 1
POOL="D:/Projects/ti4-engine/data/map_pools/save52_e400_n8192.json.gz"
UPDATES=${UPDATES:-500}
OUT="out/stage2_ppo"

BASE="--updates $UPDATES --every 250 --no-boundaries --train-seeds 16 --rounds 4
      --entropy 0.05 --high-vp-bonus 1.0 --clearance-weight 0 --round-baseline
      --ppo-epochs 4 --ppo-clip 0.2 --scramble-seats --map-pool $POOL"

echo "resuming $(date)  +$UPDATES updates per seed"
for idx in 0 1 2 3 4 5 6 7; do
  cp "$OUT/s${idx}.json" "$OUT/s${idx}.from.json"
  RAYON_NUM_THREADS=4 "$OUT/trainer.exe" $BASE \
    --checkpoint "$OUT/s${idx}.from.json" \
    --train-seed-base $((93000000 + idx * 1000000)) \
    --out "$OUT/s${idx}.json" >> "$OUT/s${idx}.log" 2>&1 &
done
wait
echo "complete $(date)"
