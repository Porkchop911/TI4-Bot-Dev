@{
    # Continue the overnight tech-0.1 / fleet-0.015 line (operator request, 2026-09-29: "continue
    # training with the last used config").
    #
    # Every flag is copied from
    # out/ppo-armB-waste1-overnight-tech01-fleet015-20260923/launch.json
    # (config scripts/overnight_tech01_fleet015.psd1, git fe88d2a8). THE REWARD FUNCTION IS UNCHANGED:
    #   vp-weight 1, waste-penalty 1, clearance-weight 0.5, r1-bonus 3, r1-shaping 0.1,
    #   tech-weight 0.1, fleet-weight 0.015, every other term explicitly 0.
    #
    # What differs from that launch, and why:
    #   Bundle -- checkpoint-40132-engine-3f92016d: checkpoint-40132, where the source run stopped
    #     (1800 updates asked for, interrupted after 500 at table clearance 84.21%, mean VP 3.321),
    #     with this engine's feature names appended into its preallocated zero rows so the policy can
    #     tell the new content apart instead of collapsing it into each family's out-of-vocabulary
    #     column (operator request, 2026-09-29: "make sure it gets current engine").
    #
    #     Two passes of vocab_census + migrate_bundle_append_names at git 3f92016d:
    #       pass 1  24 games, 1317 unseen names, 1061 seen >=3 appended  -> 19400 slots
    #       pass 2  36 games,  929 unseen names,  651 seen >=3 appended  -> 20051 slots (429 free)
    #     Appended rows are zero and verified zero, so they contribute nothing until PPO trains them.
    #     Play is NOT identical to checkpoint-40132's: a name that used to route to a trained OOV
    #     column now routes to a zero row, which is the whole point and does move scores slightly.
    #
    #     The residual tail cannot be closed by migration and was not chased. Its largest members are
    #     objective-keyed (objective-count/need/progress:cost_spend_resources, colours_2, monument,
    #     on_the_rim, weaker_neighbours, non_home), so which names a game produces depends on the
    #     objective deal, not on the engine version. Spending the last 429 rows on whichever
    #     objectives 36 games happened to draw would leave nothing for the next engine change.
    #
    #     Schema 12, projection ABI 4, diplomacy head: loadable by this build.
    #   Run -- a new directory; ppo_train.ps1 refuses to launch into one that exists.
    #   updates 1300 -- the remainder of the 1800 the source run was given.
    #   seed-base 1262520000 -- the source run consumed 1262500000..1262514999 (30 seeds x 500
    #     updates). Resuming on the old base would replay the same 500 updates' games as the first
    #     500 of this one, which is not continuing.
    #
    # The engine is not the engine of 2026-09-23: since then the L1Z1X agent, Plasma Scoring, the
    # Political Stability draft fix, the agenda-phase ready order and Elder Qanoj have landed. Games
    # are therefore not comparable to the source run's; the reward flags are.

    Bundle   = 'D:\Projects\ti4-engine-rs\out\ppo-armB-waste1-overnight-tech01-fleet015-20260923\checkpoints\checkpoint-40132-engine-3f92016d'
    Pool     = 'D:\Projects\ti4-engine-rs\out\pools\full_np8_12_train.json'
    Run      = 'D:\Projects\ti4-engine-rs\out\ppo-armB-waste1-tech01-fleet015-resume40132-20260929'
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
        'updates'          = 1300
        'report-every'     = '1:1,25:250,100'
        'seed-base'        = 1262520000
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
