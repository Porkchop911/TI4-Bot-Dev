@{
    # BF-22 pilot (operator 2026-10-08): 50 updates so the factions outside the six get their own
    # rows. Tables seat only those factions (--roster new); only their per-faction readout rows and
    # embeddings train (--faction-rows-only), so the trunk and every trained faction stay as they
    # are. From the wide checkpoint with battle predictor v8 and the wide-roster diplomacy teacher.
    # Rewards and entropy as the main line (long500 projection + critic); learning rate 3e-4
    # because only freshly zeroed rows move.

    Bundle   = 'D:\Projects\ti4-engine-rs\out\trade-teacher-wide-v8-20261008\checkpoint-20'
    Pool     = 'D:\Projects\ti4-engine-rs\out\pools\full_np8_12_train.json'
    Run      = 'D:\Projects\ti4-engine-rs\out\ppo-newfactions-rows-pilot50-20261008'
    LibTorch = 'D:\Projects\ti4-engine-rs\out\libtorch-2.9.1-cu128'

    Diplomacy       = $true
    FactionRowsOnly = $true
    Background      = $true
    Build           = $false
    AllowConcurrent = $false

    Flags = @{
        'roster'           = 'new'
        'stage'            = 2
        'rounds'           = 4
        'temperature'      = 2.5
        'learning-rate'    = '3e-4'
        'movement-entropy' = 0.05
        'entropy-final'    = 0.25
        'entropy-start'    = 0.25
        'updates'          = 50
        'report-every'     = '1:1,10'
        'seed-base'        = 1263900000
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
