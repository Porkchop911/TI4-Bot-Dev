@{
    # 2026-10-21 r8 (operator rule 2026-10-20: launch only if r7's greedy eval shows > 0.1 wasted activations
    # per seat-game): continues r7's final per-faction models with --waste-penalty 11 (~0.9 VP per wasted
    # activation at vp-weight 12), another 10000 updates. Also (operator, 2026-10-21): unused-resources weight
    # doubled to 0.2 and counted after the status phase's scoring. Every other setting as r7.

    # r7 greedy eval (eval320_r7_waste.txt, 320 holdout seeds, T 0.001): 3.79 VP, 3.54 wasted activations per seat-game.
    Bundle   = 'D:\Projects\ti4-engine-rs\out\ppo-async-faction-terms-blank-20261020-r7\checkpoints\sol\checkpoint-10800'
    Pool     = 'D:\Projects\ti4-engine-rs\out\pools\full_np8_12_train.json'
    Run      = 'D:\Projects\ti4-engine-rs\out\ppo-async-faction-terms-waste11-20261021-r8'
    LibTorch = 'D:\Projects\ti4-engine-rs\out\libtorch-2.9.1-cu128-20260908'

    SeparateModels  = $true
    Diplomacy       = $true
    Background      = $true
    Build           = $false
    AllowConcurrent = $true

    Flags = @{
        'roster'           = 'wide'
        'stage'            = 2
        'rounds'           = 5           # was 4: operator, 2026-10-20
        'temperature'      = 2.5
        'learning-rate'    = '1e-4'
        'movement-entropy' = 0.05
        'entropy-final'    = 0.25
        'entropy-start'    = 0.25
        'updates'          = 10000
        'report-every'     = '500'
        'async-batch-games'   = 60
        'async-publish-games' = 120
        'rollout-workers'     = 30
        'seed-base'        = 1279058500  # fresh seeds for the continuation
        'seeds-per-update' = 30
        'rotations'        = 1
        'device'           = 'cuda'
        'rollout-backend'  = 'infer'
        'phase-lock'       = 'D:\Projects\ti4-engine-rs\out'
        'faction-focus'    = 1
        'gae-lambda'       = 0.95

        # Points
        'vp-weight'        = 12
        'sftt-weight'      = 0.3
        'objective-weight'          = 1.8    # was 0.6: tempo shaping 0.05 -> 0.15 VP per scoreable public (462/477)
        'secret-weight'             = 1.2    # was 0.4: 0.03 -> 0.10 VP per scoreable secret
        'zero-terminal-objectives'  = 1

        # Expansion (kept, not shaping) - trimmed so VP stays dominant now that faction terms carry the plan
        'r1-expansion-weight'    = 1.0       # was 2.0: ~1.5 -> ~0.75 VP for a typical round-one slice
        'round-expansion-weight' = 0.15      # was 0.25: five rounds instead of four; ~1.1 -> ~0.9 VP per game

        # Production and spending (unchanged; measured small and guide-consistent)
        'build-weight'              = 0.035
        'scuttle-multiplier'        = 2
        'unused-resources-weight'   = 0.2    # was 0.1 (operator, 2026-10-21)
        'unused-after-scoring'      = 1      # counted after status-phase scoring, before planets ready (operator)
        'overpay-penalty'           = 0.05
        'waste-penalty'             = 11   # was 1 (operator, 2026-10-20)

        # Home and tech
        'home-loss-weight'          = 1.2    # was 0.5: 0.04 -> 0.10 VP per lost home planet (kept)
        'tech-weight'               = 0      # was 0.1 (0.008 VP/tech): faction tech goals now carry this

        # New terms
        'faction-terms-weight'      = 1      # per-faction milestones, <= 1 VP per faction, shaping
        'fracture-entry-bonus'      = 1.2    # 0.10 VP once
        'fracture-planet-shaping'   = 1.8    # 0.15 VP per Fracture planet, shaping
        'styx-shaping'              = 3.6    # 0.30 VP while holding Styx, shaping
        'relic-shaping'             = 1.2    # 0.10 VP per relic (max 2), shaping

        # Off, as in r6
        'negotiation-step-penalty'  = 0.0005
        'clearance-weight' = 0
        'r1-bonus'         = 0
        'r1-shaping'       = 0
        'fleet-weight'              = 0
        'projection-weight'         = 0
        'keep-terminal-projection'  = 1
        'projection-rank-weight'    = 0
        'fleet-hoard-penalty'       = 0
        'zero-fleet-penalty'        = 0
        'trade-goods-hoard-weight'  = 0
        'strategy-diversity-weight' = 0
        'styx-bonus'                = 0
        'fracture-planet-bonus'     = 0
    }
}
