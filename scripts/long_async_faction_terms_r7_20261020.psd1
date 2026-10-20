@{
    # 2026-10-20 r7 (PREPARED, NOT LAUNCHED - needs the operator's go): continues r6b's per-faction models with
    # the per-faction milestone terms (--faction-terms-weight), the shared Fracture/Styx/relic shaping, a
    # five-round horizon and the rebalanced base terms. Rationale and per-term sizes:
    # plans/FACTION_REWARD_CONDITIONS_2026-10-20.md section 7. At vp-weight 12, 1 VP = 12 reward units.

    Bundle   = 'D:\Projects\ti4-engine-rs\out\ppo-async-separate-wide-20261019-r6b\checkpoints\sol\checkpoint-2098'
    Pool     = 'D:\Projects\ti4-engine-rs\out\pools\full_np8_12_train.json'
    Run      = 'D:\Projects\ti4-engine-rs\out\ppo-async-faction-terms-20261020-r7'
    LibTorch = 'D:\Projects\ti4-engine-rs\out\libtorch-2.9.1-cu128-20260908'

    SeparateModels  = $true
    Diplomacy       = $true
    Background      = $true
    Build           = $false
    AllowConcurrent = $true

    Flags = @{
        'roster'           = 'wide'
        'stage'            = 2
        'rounds'           = 5           # was 4: operator, 2026-10-20
        'temperature'      = 2.5
        'learning-rate'    = '1e-4'
        'movement-entropy' = 0.05
        'entropy-final'    = 0.25
        'entropy-start'    = 0.25
        'updates'          = 10800
        'report-every'     = '500'
        'async-batch-games'   = 60
        'async-publish-games' = 120
        'rollout-workers'     = 30
        'seed-base'        = 1275058500
        'seeds-per-update' = 30
        'rotations'        = 1
        'device'           = 'cuda'
        'rollout-backend'  = 'infer'
        'phase-lock'       = 'D:\Projects\ti4-engine-rs\out'
        'faction-focus'    = 1
        'gae-lambda'       = 0.95

        # Points
        'vp-weight'        = 12
        'sftt-weight'      = 0.3
        'objective-weight'          = 1.8    # was 0.6: tempo shaping 0.05 -> 0.15 VP per scoreable public (462/477)
        'secret-weight'             = 1.2    # was 0.4: 0.03 -> 0.10 VP per scoreable secret
        'zero-terminal-objectives'  = 1

        # Expansion (kept, not shaping) - trimmed so VP stays dominant now that faction terms carry the plan
        'r1-expansion-weight'    = 1.0       # was 2.0: ~1.5 -> ~0.75 VP for a typical round-one slice
        'round-expansion-weight' = 0.15      # was 0.25: five rounds instead of four; ~1.1 -> ~0.9 VP per game

        # Production and spending (unchanged; measured small and guide-consistent)
        'build-weight'              = 0.035
        'scuttle-multiplier'        = 2
        'unused-resources-weight'   = 0.1
        'overpay-penalty'           = 0.05
        'waste-penalty'             = 1

        # Home and tech
        'home-loss-weight'          = 1.2    # was 0.5: 0.04 -> 0.10 VP per lost home planet (kept)
        'tech-weight'               = 0      # was 0.1 (0.008 VP/tech): faction tech goals now carry this

        # New terms
        'faction-terms-weight'      = 1      # per-faction milestones, <= 1 VP per faction, shaping
        'fracture-entry-bonus'      = 1.2    # 0.10 VP once
        'fracture-planet-shaping'   = 1.8    # 0.15 VP per Fracture planet, shaping
        'styx-shaping'              = 3.6    # 0.30 VP while holding Styx, shaping
        'relic-shaping'             = 1.2    # 0.10 VP per relic (max 2), shaping

        # Off, as in r6
        'negotiation-step-penalty'  = 0.0005
        'clearance-weight' = 0
        'r1-bonus'         = 0
        'r1-shaping'       = 0
        'fleet-weight'              = 0
        'projection-weight'         = 0
        'keep-terminal-projection'  = 1
        'projection-rank-weight'    = 0
        'fleet-hoard-penalty'       = 0
        'zero-fleet-penalty'        = 0
        'trade-goods-hoard-weight'  = 0
        'strategy-diversity-weight' = 0
        'styx-bonus'                = 0
        'fracture-planet-bonus'     = 0
    }
}
