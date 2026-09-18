@{
    # Arm B overnight, 2026-09-18: the fleet-decision architecture (fact version 7) on victory
    # points with the opening priced, continuing
    # checkpoint-2548 (100 updates at waste penalty 10) with the penalty down to 1.
    #
    # Victory points are still the objective. What is added is the opening: `clearance-weight` is a
    # per-game penalty when the opening does not clear (charged at the final slot, so every
    # decision's return carries it), and `r1-bonus` / `r1-shaping` price the path through round one
    # rather than demanding it. Every VP-only arm so far let clearance slide from 94% to ~88%.
    #
    # The three values are the settled ones, not fresh guesses: `r1-bonus 3` and `r1-shaping 0.1`
    # are the reward's own defaults (reward.rs refuses shaping above 1.0 -- at 1.0 Stage 2 becomes
    # Stage 1), and `clearance-weight 0.5` is the reference config's. Earlier stage-2 runs used
    # 1.0 and 2.0 and relaxed back once clearance stopped gating.
    #
    # Original header:
    # VP-only pilot against frozen benchmark seats, 2026-09-16.
    #
    # Astra's transparent control: victory points at weight 1 and every other term explicitly zero,
    # so the objective is the thing promotion asks about and nothing else. Clearance and waste are
    # still reported per update as diagnostics; they simply earn no reward.
    #
    #   .\scripts\ppo_train.ps1 -Config .\scripts\activation_armB_waste1.psd1 -DryRun
    #   .\scripts\ppo_train.ps1 -Config .\scripts\activation_armB_waste1.psd1

    Bundle = 'D:\Projects\ti4-engine-rs\out\ppo-activation-armB-waste-20260918\checkpoints\checkpoint-2548'
    Pool = 'D:\Projects\ti4-engine-rs\out\pools\full_np8_12_train.json'
    Run = 'D:\Projects\ti4-engine-rs\out\ppo-activation-armB-waste1-20260918'
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
        'updates' = 1200
        'report-every' = '1:1,25:250,100'
        'seed-base' = 1262000000
        'device' = 'cuda'

        # The objective, and nothing else.
        'vp-weight' = 1

        # Explicitly off. The reward reads any value at or below zero as off, and stating them here
        # means launch.json records the control rather than leaving defaults to be inferred later.
        'objective-weight' = 0
        'secret-weight' = 0
        'r1-bonus' = 3
        'r1-shaping' = 0.1
        'clearance-weight' = 0.5
        'fleet-weight' = 0
        'tech-weight' = 0
        'strategy-diversity-weight' = 0
        'fleet-hoard-penalty' = 0
        'zero-fleet-penalty' = 0
        'trade-goods-hoard-weight' = 0
        'styx-bonus' = 0
        'fracture-entry-bonus' = 0
        'fracture-planet-bonus' = 0
        # Mild on purpose. 100 updates at 10 took waste from 0.104 to 0.030 per tactical action --
        # and tactical actions per seat from 1.33 to 0.82, which is the penalty suppressing the
        # action rather than the waste. At 1 the term stays present without paying for inaction.
        # Watch tactical/seat: if it does not climb back toward 1.3, restart from the tip before
        # any waste penalty (ppo-activation-armB-clear2-20260918/checkpoints/checkpoint-35376).
        'waste-penalty' = 1
    }
}
