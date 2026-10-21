@{
    # 2026-10-19 r6: continues r5 (per-faction models from r5 checkpoint-1500, +0.29 VP vs checkpoint-201104 on 320 paired games); fewest-updates-first target, STOP-file graceful stop, vp-weight 12, checkpoints every 500 updates; self-imitation off.

    Bundle   = 'D:\Projects\ti4-engine-rs\out\ppo-async-separate-wide-20261019-r5\checkpoints\sol\checkpoint-1500'
    Pool     = 'D:\Projects\ti4-engine-rs\out\pools\full_np8_12_train.json'
    Run      = 'D:\Projects\ti4-engine-rs\out\ppo-async-separate-wide-20261019-r6'
    LibTorch = 'D:\Projects\ti4-engine-rs\out\libtorch-2.9.1-cu128-20260908'

    SeparateModels  = $true
    Diplomacy       = $true
    Background      = $true
    Build           = $false
    AllowConcurrent = $true

    Flags = @{
        'overpay-penalty'           = 0.05
        'unused-resources-weight'   = 0.1
        'build-weight'              = 0.035
        'scuttle-multiplier'        = 2
        'roster'           = 'wide'
        'stage'            = 2
        'rounds'           = 4
        'temperature'      = 2.5
        'learning-rate'    = '1e-4'
        'movement-entropy' = 0.05
        'entropy-final'    = 0.25
        'entropy-start'    = 0.25
        'updates'          = 10800
        'report-every'     = '500'
        'async-batch-games'   = 60
        'async-publish-games' = 120
        'rollout-workers'     = 30
        'seed-base'        = 1275058500
        'seeds-per-update' = 30
        'rotations'        = 1
        'device'           = 'cuda'
        'rollout-backend'  = 'infer'
        'phase-lock'       = 'D:\Projects\ti4-engine-rs\out'
        'sftt-weight'      = 0.3

        'vp-weight'        = 12
        'waste-penalty'    = 1
        'clearance-weight' = 0
        'r1-bonus'         = 0
        'r1-shaping'       = 0
        'r1-expansion-weight' = 2.0
        'round-expansion-weight' = 0.25

        'objective-weight'          = 0.6
        'secret-weight'             = 0.4
        'zero-terminal-objectives'  = 1
        'home-loss-weight'          = 0.5
        'tech-weight'               = 0.1
        'fleet-weight'              = 0
        'projection-weight'         = 0
        'keep-terminal-projection'  = 1
        'projection-rank-weight'    = 0
        'negotiation-step-penalty'  = 0.0005
        'fleet-hoard-penalty'       = 0
        'zero-fleet-penalty'        = 0
        'faction-focus'    = 1
        'gae-lambda'       = 0.95
        'trade-goods-hoard-weight'  = 0
        'strategy-diversity-weight' = 0
        'styx-bonus'                = 0
        'fracture-entry-bonus'      = 0
        'fracture-planet-bonus'     = 0
    }
}
