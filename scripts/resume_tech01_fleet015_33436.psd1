@{
    # Resume the tech-0.1 / fleet-0.015 line from checkpoint-33436 (operator request, 2026-09-30:
    # "resume training from the latest checkpoint under the same conditions").
    #
    # Every flag is copied from scripts/resume_tech01_fleet015_114592.psd1 (launch.json, git 3f92016d).
    # Reward function UNCHANGED. The 114592 run stopped after update 400 of 1000 (process gone, no
    # error in stderr); its last report: T=2.5 table 86.28% clearance, 3.498 mean VP; checkpoint-33436
    # reloaded identical.
    #
    # What differs, and why:
    #   Bundle -- checkpoint-33436, the update-400 checkpoint of that run.
    #   updates 600 -- the remainder of the requested 1000.
    #   seed-base 1262572000 -- that run consumed 1262560000..1262571999 (30 seeds x 400 updates).
    #   Code -- HEAD 74866832; since 3f92016d only a stage-1 test, ti4-sim behaviour suite, replayer
    #     and bridge fixtures changed. No engine, mlp, reward or rollout code changed.
    Bundle   = 'D:\Projects\ti4-engine-rs\out\ppo-armB-waste1-tech01-fleet015-resume114592-20260929\checkpoints\checkpoint-33436'
    Pool     = 'D:\Projects\ti4-engine-rs\out\pools\full_np8_12_train.json'
    Run      = 'D:\Projects\ti4-engine-rs\out\ppo-armB-waste1-tech01-fleet015-resume33436-20260930'
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
        'updates'          = 600
        'report-every'     = '1:1,25:250,100'
        'seed-base'        = 1262572000
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

