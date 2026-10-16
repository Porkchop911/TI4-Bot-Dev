@{
    # Projection-shaping pilot, PROJECTION arm (operator: "Go", 2026-10-02). 50 updates from
    # checkpoint-49232. Identical to the control arm except: fleet-weight 0.015 -> 0 and
    # projection-weight 0.02 (potential-based shaping over power_map::opportunity, zeroed at the end
    # of the game, so it reshapes credit without changing the optimal policy).
    # Why: within-faction, opportunity predicts winning (+0.15; top fifth 23.3% vs bottom 9.8%)
    # while fleet strength does not (-0.04; top fifth 14.5% vs bottom 19.1%).
    # Same seeds as the control (seed-base 1262700000). Judge by paired greedy eval.
    Bundle   = 'D:\Projects\ti4-engine-rs\out\ppo-armB-waste1-tech01-fleet015-resume33436-20260930\checkpoints\checkpoint-49232'
    Pool     = 'D:\Projects\ti4-engine-rs\out\pools\full_np8_12_train.json'
    Run      = 'D:\Projects\ti4-engine-rs\out\ppo-armB-waste1-tech01-proj002-pilot-49232-20261002'
    LibTorch = 'D:\Projects\ti4-engine-rs\out\libtorch-2.9.1-cu128'

    Diplomacy       = $true
    Background      = $true
    Build           = $true
    AllowConcurrent = $false

    Flags = @{
        'stage'            = 2
        'rounds'           = 4
        'temperature'      = 2.5
        'learning-rate'    = '1e-4'
        'movement-entropy' = 0.05
        'entropy-final'    = 0.25
        'updates'          = 50
        'report-every'     = '1:1,25:250,100'
        'seed-base'        = 1262700000
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



