@{
    # Arena pilot, 2026-09-17: the VP-only pilot with battle facts on movement options.
    #
    # Identical to vp_pilot.psd1 except the learner bundle, which is checkpoint-212544 migrated to
    # arena schema 12 (frozen battle predictor, zeroed battle-fact rows). The frozen opponents stay
    # the plain 212544, so the VP pilot's checkpoint-10456 is the no-arena control.
    #
    # Original header:
    # VP-only pilot against frozen benchmark seats, 2026-09-16.
    #
    # Astra's transparent control: victory points at weight 1 and every other term explicitly zero,
    # so the objective is the thing promotion asks about and nothing else. Clearance and waste are
    # still reported per update as diagnostics; they simply earn no reward.
    #
    #   .\scripts\ppo_train.ps1 -Config .\scripts\arena_pilot.psd1 -DryRun
    #   .\scripts\ppo_train.ps1 -Config .\scripts\arena_pilot.psd1

    Bundle = 'D:\Projects\ti4-engine-rs\out\ppo-diplomacy-my-run\checkpoints\checkpoint-212544-arena-v1'
    Pool = 'D:\Projects\ti4-engine-rs\out\pools\full_np8_12_train.json'
    Run = 'D:\Projects\ti4-engine-rs\out\ppo-arena-pilot-20260917'
    LibTorch = 'D:\Projects\ti4-engine-rs\out\libtorch-2.9.1-cu128'

    Diplomacy = $true
    Background = $true
    Build = $true
    AllowConcurrent = $false

    Flags = @{
        # Five frozen copies of the starting policy; one rotating faction learns per game.
        'opponent' = 'D:\Projects\ti4-engine-rs\out\ppo-diplomacy-my-run\checkpoints\checkpoint-212544'

        'stage' = 2
        'rounds' = 4
        'temperature' = 2.5
        'learning-rate' = '1e-4'
        'movement-entropy' = 0.05
        'entropy-final' = 0.25
        'updates' = 300
        'report-every' = '1:1,25:250,100'
        'seed-base' = 1262000000
        'device' = 'cuda'

        # The objective, and nothing else.
        'vp-weight' = 1

        # Explicitly off. The reward reads any value at or below zero as off, and stating them here
        # means launch.json records the control rather than leaving defaults to be inferred later.
        'objective-weight' = 0
        'secret-weight' = 0
        'r1-bonus' = 0
        'r1-shaping' = 0
        'clearance-weight' = 0
        'fleet-weight' = 0
        'tech-weight' = 0
        'strategy-diversity-weight' = 0
        'fleet-hoard-penalty' = 0
        'zero-fleet-penalty' = 0
        'trade-goods-hoard-weight' = 0
        'styx-bonus' = 0
        'fracture-entry-bonus' = 0
        'fracture-planet-bonus' = 0
        'waste-penalty' = 0
    }
}
