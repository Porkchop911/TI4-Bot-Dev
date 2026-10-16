@{
    # Power-facts pilot, ARM B (operator 2026-10-03). checkpoint-49232-power2: actor + critic power facts,
    # projection reward 0.02, fleet-weight 0. 50 updates, entropy 0.25x, same seeds as arm A.

    Bundle   = 'D:\Projects\ti4-engine-rs\out\ppo-armB-waste1-tech01-fleet015-resume33436-20260930\checkpoints\checkpoint-49232-power2'
    Pool     = 'D:\Projects\ti4-engine-rs\out\pools\full_np8_12_train.json'
    Run      = 'D:\Projects\ti4-engine-rs\out\ppo-armB-waste1-tech01-powerfacts-B-proj002-20261003'
    LibTorch = 'D:\Projects\ti4-engine-rs\out\libtorch-2.9.1-cu128'

    Diplomacy       = $true
    Background      = $true
    Build           = $false
    AllowConcurrent = $false

    Flags = @{
        'stage'            = 2
        'rounds'           = 4
        'temperature'      = 2.5
        'learning-rate'    = '1e-4'
        'movement-entropy' = 0.05
        'entropy-final'    = 0.25
        'entropy-start'    = 0.25
        'updates'          = 50
        'report-every'     = '1:1,25:250,100'
        'seed-base'        = 1262730000
        'seeds-per-update' = 30
        'rotations'        = 1
        'device'           = 'cuda'

        'vp-weight'        = 1
        'waste-penalty'    = 1
        'clearance-weight' = 0.5
        'r1-bonus'         = 3
        'r1-shaping'       = 0.1

        'objective-weight'          = 0
        'secret-weight'             = 0
        'tech-weight'               = 0.1
        'fleet-weight'              = 0
        'projection-weight'         = 0.02
        'fleet-hoard-penalty'       = 0
        'zero-fleet-penalty'        = 0
        'trade-goods-hoard-weight'  = 0
        'strategy-diversity-weight' = 0
        'styx-bonus'                = 0
        'fracture-entry-bonus'      = 0
        'fracture-planet-bonus'     = 0
    }
}









