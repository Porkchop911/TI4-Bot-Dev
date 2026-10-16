@{
    # Continue the tech-0.1 / fleet-0.015 line for another 1000 updates (operator request,
    # 2026-09-29: "Continue for another 1000").
    #
    # Every flag is copied from
    # out/ppo-armB-waste1-tech01-fleet015-resume40132-20260929/launch.json
    # (config scripts/resume_tech01_fleet015_40132.psd1, git 3f92016d). THE REWARD FUNCTION IS
    # UNCHANGED, for the third run in this line:
    #   vp-weight 1, waste-penalty 1, clearance-weight 0.5, r1-bonus 3, r1-shaping 0.1,
    #   tech-weight 0.1, fleet-weight 0.015, every other term explicitly 0.
    #
    # What differs from that launch, and why:
    #   Bundle -- checkpoint-114592, the end of the 1300-update run that preceded this one (in-training
    #     T=2.5 table 88.19% clearance, 3.605 mean VP over 18000 seat-games; the run's own report
    #     verified the checkpoint reloads identical, and it exited cleanly at its full update count).
    #
    #     No vocabulary migration this time, and that is deliberate rather than an omission: this
    #     bundle was written BY this engine at this commit (manifest git_commit 3f92016d == HEAD), so
    #     there is no version gap to close. It already carries the 1712 names appended in the two
    #     census passes before the previous run: 20051 of 20480 slots, 429 free.
    #
    #     The objective-keyed residual tail is still open and still not worth chasing - the names are
    #     keyed by which public objectives a game deals (cost_spend_resources, colours_2, monument,
    #     on_the_rim, weaker_neighbours, non_home), not by engine version, so no finite census closes
    #     it and the last 429 rows are better kept for the next real engine change.
    #
    #     Schema 12, projection ABI 4, diplomacy head: loadable by this build.
    #   Run -- a new directory; ppo_train.ps1 refuses to launch into one that exists.
    #   updates 1000 -- what was asked for.
    #   seed-base 1262560000 -- the previous run consumed 1262520000..1262558999 (30 seeds x 1300
    #     updates). Reusing that base would replay games this policy has already trained on.
    #
    # STILL OUTSTANDING: no paired greedy eval has been run on this line. The table above is T=2.5
    # in-training sampling, which has read flat on this project while greedy clearance fell 5pp, so
    # "88.19% beats the 85.39% plateau" is not yet a measured improvement. checkpoint-114592 vs
    # checkpoint-40132-engine-3f92016d on identical seeds is the comparison that would settle it.

    Bundle   = 'D:\Projects\ti4-engine-rs\out\ppo-armB-waste1-tech01-fleet015-resume40132-20260929\checkpoints\checkpoint-114592'
    Pool     = 'D:\Projects\ti4-engine-rs\out\pools\full_np8_12_train.json'
    Run      = 'D:\Projects\ti4-engine-rs\out\ppo-armB-waste1-tech01-fleet015-resume114592-20260929'
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
        'updates'          = 1000
        'report-every'     = '1:1,25:250,100'
        'seed-base'        = 1262560000
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
