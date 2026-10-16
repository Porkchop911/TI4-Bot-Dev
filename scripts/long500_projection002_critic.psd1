@{
    # Long run, PROJECTION + CRITIC (operator 2026-10-02: 'Do 1 and 2'). 500 updates from pilot 2's
    # checkpoint-4092. Entropy carried over: --entropy-start 0.25 = where every earlier run ended, held
    # at 0.25 (no restart at full strength). Reward unchanged from the pilots. New seeds 1262704000.

    Bundle   = 'D:\Projects\ti4-engine-rs\out\ppo-armB-waste1-tech01-proj002-critic-pilot2-20261002\checkpoints\checkpoint-4092'
    Pool     = 'D:\Projects\ti4-engine-rs\out\pools\full_np8_12_train.json'
    Run      = 'D:\Projects\ti4-engine-rs\out\ppo-armB-waste1-tech01-proj002-critic-long500-20261002-r2'
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
        'updates'          = 500
        'report-every'     = '1:1,25:250,100'
        'seed-base'        = 1262704000
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








