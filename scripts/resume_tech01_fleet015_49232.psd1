@{
    # Continue the tech-0.1 / fleet-0.015 line for another 3000 updates (operator request,
    # 2026-10-01: "start another 3000").
    #
    # Every flag is copied from scripts/resume_tech01_fleet015_33436.psd1. Reward function UNCHANGED.
    #   Bundle -- checkpoint-49232, the end of the 600-update run (exited cleanly at 600/600;
    #     T=2.5 table 89.66% clearance, 3.717 mean VP; reloaded identical).
    #   updates 3000 -- what was asked for.
    #   seed-base 1262590000 -- that run consumed 1262572000..1262589999 (30 seeds x 600 updates).
    #   Code -- HEAD 74866832, same as the previous run.
    #
    # STILL OUTSTANDING: no paired greedy eval on this line.
    Bundle   = 'D:\Projects\ti4-engine-rs\out\ppo-armB-waste1-tech01-fleet015-resume33436-20260930\checkpoints\checkpoint-49232'
    Pool     = 'D:\Projects\ti4-engine-rs\out\pools\full_np8_12_train.json'
    Run      = 'D:\Projects\ti4-engine-rs\out\ppo-armB-waste1-tech01-fleet015-resume49232-20261001'
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
        'seed-base'        = 1262590000
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


