@{
    # Arm B from update 100 (operator 2026-10-17): Support for the Throne points count 0.3 of a
    # point from here on. Resumes ppo-blankwide-shared-rec-B-20261017 at its update-100 checkpoint;
    # 450 more updates to 1000, seeds continue. Arms A and B differ only in seeds. Seats ending at
    # >= 6 VP (Hacan >= 7) have their full training records written to <run>\best\<faction>\.

    Bundle   = 'D:\Projects\ti4-engine-rs\out\ppo-blankwide-shared-rec-B-20261017\checkpoints\checkpoint-8588'
    Pool     = 'D:\Projects\ti4-engine-rs\out\pools\full_np8_12_train.json'
    Run      = 'D:\Projects\ti4-engine-rs\out\ppo-blankwide-shared-rec-B-sftt03-20261017'
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
        'updates'          = 450
        'report-every'     = '1:1,50'
        'seed-base'        = 1271003000
        'seeds-per-update' = 30
        'rotations'        = 1
        'device'           = 'cuda'
        'rollout-backend'  = 'infer'
        'phase-lock'       = 'D:\Projects\ti4-engine-rs\out'
        'record-best'      = 'D:\Projects\ti4-engine-rs\out\ppo-blankwide-shared-rec-B-sftt03-20261017\best'
        'record-vp'        = '6,hacan=7'
        'sftt-weight'      = 0.3

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
        'faction-focus'    = 1
        'gae-lambda'       = 0.95
        'trade-goods-hoard-weight'  = 0
        'strategy-diversity-weight' = 0
        'styx-bonus'                = 0
        'fracture-entry-bonus'      = 0
        'fracture-planet-bonus'     = 0
    }
}
