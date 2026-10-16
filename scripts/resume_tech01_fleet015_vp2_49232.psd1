@{
    # 3000 updates from checkpoint-49232 with DOUBLE the VP reward (operator request, 2026-10-01:
    # "run it with double the reward for scoring vp").
    #
    # Copied from scripts/resume_tech01_fleet015_49232.psd1; the ONLY reward change is
    # vp-weight 1 -> 2. All other terms unchanged (waste 1, clearance 0.5, r1-bonus 3, r1-shaping 0.1,
    # tech 0.1, fleet 0.015, rest 0).
    #   Run -- new directory; the vp-weight-1 launch (resume49232-20261001) was stopped by the
    #     operator after update 1 and is left in place.
    #   seed-base 1262600000 -- past everything the stopped launch could have used (1262590000..).
    #   Code -- HEAD 74866832.
    #
    # Note: this changes the reward scale, so the critic's value targets shift; expect a critic-loss
    # spike early. Mean-VP tables are not comparable 1:1 to the vp-weight-1 line's returns.
    Bundle   = 'D:\Projects\ti4-engine-rs\out\ppo-armB-waste1-tech01-fleet015-resume33436-20260930\checkpoints\checkpoint-49232'
    Pool     = 'D:\Projects\ti4-engine-rs\out\pools\full_np8_12_train.json'
    Run      = 'D:\Projects\ti4-engine-rs\out\ppo-armB-waste1-tech01-fleet015-resume49232-vp2-20261001'
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
        'updates'          = 3000
        'report-every'     = '1:1,25:250,100'
        'seed-base'        = 1262600000
        'seeds-per-update' = 30
        'rotations'        = 1
        'device'           = 'cuda'

        'vp-weight'        = 2
        'waste-penalty'    = 1
        'clearance-weight' = 0.5
        'r1-bonus'         = 3
        'r1-shaping'       = 0.1

        'objective-weight'          = 0
        'secret-weight'             = 0
        'tech-weight'               = 0.1
        'fleet-weight'              = 0.015
        'fleet-hoard-penalty'       = 0
        'zero-fleet-penalty'        = 0
        'trade-goods-hoard-weight'  = 0
        'strategy-diversity-weight' = 0
        'styx-bonus'                = 0
        'fracture-entry-bonus'      = 0
        'fracture-planet-bonus'     = 0
    }
}



