@{
    # Focus/GAE pilot (operator 2026-10-09: "do 1 and 3"). CONTROL: plain PPO.
    # All three arms: start card-tag T checkpoint-3584, wide roster, main-line rewards, 1000 updates x 30 seeds (operator: 1000 updates before the greedy eval), seed base 1264200000.
    # Judge by greedy 600-seed clearance_eval --roster wide (VP; the opening bar is calibrated for the six).

    Bundle   = 'D:\Projects\ti4-engine-rs\out\ppo-cardtag-T-wide-pilot50-20261009\checkpoints\checkpoint-3584'
    Pool     = 'D:\Projects\ti4-engine-rs\out\pools\full_np8_12_train.json'
    Run      = 'D:\Projects\ti4-engine-rs\out\ppo-focusgae-C-wide-long1000-20261009'
    LibTorch = 'D:\Projects\ti4-engine-rs\out\libtorch-2.9.1-cu128'

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
        'updates'          = 1000
        'report-every'     = '1:1,50'
        'seed-base'        = 1264200000
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








