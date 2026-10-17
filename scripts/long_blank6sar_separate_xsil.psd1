@{
    # Cross-imitation arm 2026-10-17 (operator): separate per-faction models resumed at update 450 of
    # ppo-blank6-separate-long1000-20261017, Sardakk replacing Hacan (its model starts from Hacan's), each
    # model also imitating the self-imitation picks of all six factions as its own (0.2 cap, +1 VP, keep 4).
    # 550 more updates to 1000; seeds continue where update 450 left off.

    Bundle   = 'D:\Projects\ti4-engine-rs\out\ppo-blank6-separate-long1000-20261017\checkpoints\hacan\checkpoint-5876'
    Pool     = 'D:\Projects\ti4-engine-rs\out\pools\full_np8_12_train.json'
    Run      = 'D:\Projects\ti4-engine-rs\out\ppo-blank6sar-separate-xsil-resume450-20261017'
    LibTorch = 'D:\Projects\ti4-engine-rs\out\libtorch-2.9.1-cu128-20260908'

    Diplomacy       = $true
    Background      = $true
    Build           = $false
    AllowConcurrent = $true
    SeparateModels  = $true

    Flags = @{
        'roster'           = 'six'
        'factions'         = 'sol,letnev,xxcha,sardakk,jolnar,l1z1x'
        'stage'            = 2
        'rounds'           = 4
        'temperature'      = 2.5
        'learning-rate'    = '1e-4'
        'movement-entropy' = 0.05
        'entropy-final'    = 0.25
        'entropy-start'    = 0.25
        'updates'          = 550
        'report-every'     = '1:1,50'
        'seed-base'        = 1268013500
        'seeds-per-update' = 30
        'rotations'        = 1
        'device'           = 'cuda'
        'rollout-backend'  = 'infer'
        'phase-lock'       = 'D:\Projects\ti4-engine-rs\out'

        'vp-weight'        = 1
        'waste-penalty'    = 1
        'clearance-weight' = 0
        'r1-bonus'         = 0
        'r1-shaping'       = 0
        'r1-expansion-weight' = 0.5

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
        'faction-focus'    = 0
        'self-imitation'   = 0.2
        'sil-margin'       = 1
        'sil-keep'         = 4
        'gae-lambda'       = 0.95
        'trade-goods-hoard-weight'  = 0
        'strategy-diversity-weight' = 0
        'styx-bonus'                = 0
        'fracture-entry-bonus'      = 0
        'fracture-planet-bonus'     = 0
    }
}
