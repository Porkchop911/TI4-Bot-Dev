@{
    # 2026-10-19 continues the scuttle2/vp3 run from its update 200 (checkpoint-14008): round-one expansion 4x the original 0.5 (= 2.0), later-round expansion 0.5 per round, fresh seeds; everything else unchanged.

    Bundle   = 'D:\Projects\ti4-engine-rs\out\ppo-async-build01-scuttle2-vp3-exp2-20261019\checkpoints\checkpoint-14008'
    Pool     = 'D:\Projects\ti4-engine-rs\out\pools\full_np8_12_train.json'
    Run      = 'D:\Projects\ti4-engine-rs\out\ppo-async-r1exp4-roundexp-from200-20261019'
    LibTorch = 'D:\Projects\ti4-engine-rs\out\libtorch-2.9.1-cu128-20260908'

    Diplomacy       = $true
    Background      = $true
    Build           = $false
    AllowConcurrent = $true

    Flags = @{
        'overpay-penalty'           = 0.05
        'unused-resources-weight'   = 0.01
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
        'updates'          = 3600
        'report-every'     = '1:1,50'
        'async-batch-games'   = 30
        'async-publish-games' = 60
        'rollout-workers'     = 30
        'seed-base'        = 1273058500
        'seeds-per-update' = 30
        'rotations'        = 1
        'device'           = 'cuda'
        'rollout-backend'  = 'infer'
        'phase-lock'       = 'D:\Projects\ti4-engine-rs\out'
        'sftt-weight'      = 0.3

        'vp-weight'        = 3
        'waste-penalty'    = 1
        'clearance-weight' = 0
        'r1-bonus'         = 0
        'r1-shaping'       = 0
        'r1-expansion-weight' = 2.0
        'round-expansion-weight' = 0.5

        'objective-weight'          = 0.3
        'secret-weight'             = 0.2
        'zero-terminal-objectives'  = 1
        'home-loss-weight'          = 0.5
        'tech-weight'               = 0.1
        'fleet-weight'              = 0
        'projection-weight'         = 0.02
        'keep-terminal-projection'  = 1
        'projection-rank-weight'    = 0.3
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
