@{
    # Arm 1 resumed at update 100 under --phase-lock (fresh Adam), sharing the GPU with arm 2.
    # Long run 2026-10-17 (operator): from FG checkpoint-16448, R rewards with rank 0.3 (rank now the
    # mean of projection rank and non-home-planet rank) and a 0.0005 per-step negotiation penalty.


    Bundle   = 'D:\Projects\ti4-engine-rs\out\ppo-negpen-rank03-long1000-20261017\checkpoints\checkpoint-8856'
    Pool     = 'D:\Projects\ti4-engine-rs\out\pools\full_np8_12_train.json'
    Run      = 'D:\Projects\ti4-engine-rs\out\ppo-negpen-rank03-resume100-20261017'
    LibTorch = 'D:\Projects\ti4-engine-rs\out\libtorch-2.9.1-cu128-20260908'

    Diplomacy       = $true
    Background      = $true
    Build           = $false
    AllowConcurrent = $true

    Flags = @{
        'roster'           = 'wide'
        'stage'            = 2
        'rounds'           = 4
        'temperature'      = 2.5
        'learning-rate'    = '1e-4'
        'movement-entropy' = 0.05
        'entropy-final'    = 0.25
        'entropy-start'    = 0.25
        'updates'          = 900
        'report-every'     = '1:1,50'
        'seed-base'        = 1267003000
        'seeds-per-update' = 30
        'rotations'        = 1
        'device'           = 'cuda'
        'rollout-backend'  = 'infer'
        'phase-lock'       = 'D:\Projects\ti4-engine-rs\out'

        'vp-weight'        = 1
        'waste-penalty'    = 1
        'clearance-weight' = 0
        'r1-bonus'         = 3
        'r1-shaping'       = 0.1

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
