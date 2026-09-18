@{
    # Copy this file, edit the copy, then run:
    #   .\scripts\ppo_train.ps1 -Config .\scripts\my_ppo_run.psd1 -DryRun
    #   .\scripts\ppo_train.ps1 -Config .\scripts\my_ppo_run.psd1

    Bundle = 'D:\Projects\ti4-engine-rs\out\ppo-diplomacy-overnight-waste-20260915\checkpoints\checkpoint-240'
    Pool = 'D:\Projects\ti4-engine-rs\out\pools\full_np8_12_train.json'
    Run = 'D:\Projects\ti4-engine-rs\out\ppo-diplomacy-my-run'
    LibTorch = 'D:\Projects\ti4-engine-rs\out\libtorch-2.9.1-cu128'

    Diplomacy = $true
    Background = $true
    Build = $true
    AllowConcurrent = $false

    # Keys are the PPO flag names without the leading "--". Values may be numbers or strings.
    Flags = @{
        'stage' = 2
        'rounds' = 4
        'temperature' = 2.5
        'learning-rate' = '1e-4'
        'movement-entropy' = 0.05
        'entropy-final' = 0.25
        'waste-penalty' = 10
        'updates' = 1000
        'report-every' = '1:1,25:250,100'
        'seed-base' = 1261000000
        'device' = 'cuda'

        'vp-weight' = 1.1
        'objective-weight' = 0.35
        'secret-weight' = 0.35
        'r1-bonus' = 3
        'r1-shaping' = 0.1
        'clearance-weight' = 0.5
        'fleet-weight' = 0.03
        'tech-weight' = 0.1
        'strategy-diversity-weight' = 0.5
        'fleet-hoard-penalty' = 1
        'zero-fleet-penalty' = 10
        'trade-goods-hoard-weight' = 0.05
        'styx-bonus' = 16
        'fracture-entry-bonus' = 0.3
    }
}
