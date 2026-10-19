@{
    # 2026-10-19 separate per-faction models on the wide roster (async, per-faction batches of 60 seat-games, publish every 120), all 32 starting from checkpoint-201104 of the r1exp4 run; self-imitation on seat-games with >= 6 VP; unused resources 0.1, VP-based rewards doubled, later-round expansion 0.25. One update = one faction's batch: 10800 updates = the 108k games of the old 3600 x 30.

    Bundle   = 'D:\Projects\ti4-engine-rs\out\ppo-async-r1exp4-roundexp-from200-20261019\checkpoints\checkpoint-201104'
    Pool     = 'D:\Projects\ti4-engine-rs\out\pools\full_np8_12_train.json'
    Run      = 'D:\Projects\ti4-engine-rs\out\ppo-async-separate-wide-20261019'
    LibTorch = 'D:\Projects\ti4-engine-rs\out\libtorch-2.9.1-cu128-20260908'

    SeparateModels  = $true
    Diplomacy       = $true
    Background      = $true
    Build           = $false
    AllowConcurrent = $true

    Flags = @{
        'overpay-penalty'           = 0.05
        'unused-resources-weight'   = 0.1
        'build-weight'              = 0.1
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
        'report-every'     = '1500'
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

        'vp-weight'        = 6
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
        'self-imitation'   = 0.2
        'sil-min-vp'       = 6
        'trade-goods-hoard-weight'  = 0
        'strategy-diversity-weight' = 0
        'styx-bonus'                = 0
        'fracture-entry-bonus'      = 0
        'fracture-planet-bonus'     = 0
    }
}
