# Temperature evaluation — 2026-09-09

The earlier arm-evaluation numbers use candidate temperature **0.001**, with the five frozen opponents also at 0.001. In-training reports use **2.5 for all six seats**. Those are different evaluation conditions.

## Findings

- Near-greedy 0.001 has the highest mean VP in every row. No warmer setting showed a positive mean change, so there is no temperature improvement here to promote into a larger confirmation run.
- Temperature 2.5 reduces mean VP by **0.585–0.703** versus the same checkpoint at 0.001; every paired interval lies below zero.
- Temperature 0.5 differs by only −0.022 to −0.061 VP; all paired intervals include zero. This screen does not establish that 0.001 is better than 0.5 by a small amount.
- `vponly` has the highest mean VP at each tested temperature. This is a checkpoint comparison in one map cohort, not replicated proof of a recipe effect.
- Keep the established near-greedy acceptance setting. These results do **not** imply that training should use near-greedy sampling; that would change exploration and requires a separate training experiment.

## Matched temperature screen

Mean candidate VP at four rounds; all conditions use the same 20 Validation map seeds, 910001000–910001019. Each checkpoint/temperature cell contains 720 candidate games (six faction rotations × six candidate positions per seed). The independent resampling unit is the map seed.

| Checkpoint | T=0.001 | T=0.5 | T=1.0 | T=2.5 |
|---|---:|---:|---:|---:|
| start | 3.771 | 3.710 | 3.542 | 3.068 |
| control | 3.603 | 3.581 | 3.462 | 2.947 |
| vponly | 3.897 | 3.872 | 3.831 | 3.235 |
| waste1 | 3.774 | 3.725 | 3.508 | 3.189 |

## Paired change from each checkpoint’s own near-greedy result

50,000 paired map-bootstrap resamples, RNG seed 20260909. These are exploratory percentile 95% intervals, unadjusted for the twelve temperature comparisons; they do not estimate training-replicate variance.

| Checkpoint | Temperature | Delta VP | Paired 95% interval |
|---|---:|---:|---:|
| start | 0.5 | -0.061 | [-0.167, +0.044] |
| start | 1 | -0.229 | [-0.340, -0.121] |
| start | 2.5 | -0.703 | [-0.835, -0.564] |
| control | 0.5 | -0.022 | [-0.183, +0.147] |
| control | 1 | -0.140 | [-0.303, +0.015] |
| control | 2.5 | -0.656 | [-0.831, -0.485] |
| vponly | 0.5 | -0.025 | [-0.126, +0.082] |
| vponly | 1 | -0.067 | [-0.168, +0.031] |
| vponly | 2.5 | -0.662 | [-0.776, -0.536] |
| waste1 | 0.5 | -0.049 | [-0.182, +0.085] |
| waste1 | 1 | -0.265 | [-0.385, -0.153] |
| waste1 | 2.5 | -0.585 | [-0.701, -0.476] |

## What the comparison does and does not answer

Only the candidate temperature changes. The five opponents are always checkpoint-473312 at 0.001. This isolates candidate sampling against the established benchmark; it is not an all-seats-hot self-play experiment. Do not compare these numbers directly with the trainer’s temperature-2.5 self-play averages.

Temperature changes how an existing checkpoint acts, not its weights. Checkpoints may have different logit scales, so the same temperature is not necessarily the same effective exploration level across policies. A temperature chosen by maximizing this screen needs confirmation on fresh seeds before declaring a deployment optimum. Twenty maps are suitable for detecting the large hot-versus-greedy differences seen here, not for establishing small recipe gains. No 100-map temperature confirmation is claimed.

The crossplay tool reports tactical waste incidence but not tactical actions per seat. This report therefore makes no claim that a temperature improves waste or activity. It does not propose lowering training temperature simply because near-greedy evaluation scores better. Exploration during learning and sampling at deployment have different purposes.

## Method and reproducibility

All 16 conditions completed without reported inference errors or truncations; 11,520 candidate games including reused near-greedy results. Existing near-greedy logs were reused only when the model/seed condition was complete. Each cell uses the same 20-map subset; the 100-map greedy means from the separate arm evaluation must not be substituted into this table.

Frozen evaluator SHA-256: `8736f60fc3bfc28c2b40fceda4357bb856e3dddc7d6f1ad5fdeee2a1f7a166fa`. Inputs and checkpoint hashes: `out/temperature-eval-20260909/inputs.json`. Model paths and flags: `design.json`; runner: `run.py`; analysis: `analyse.py`; per-map values and intervals: `summary.json`; raw logs: `<arm>-t<temperature>-<seed>.log`, all under the same directory. All vocabularies match generation `fa3d6f94…`.

Two CPU evaluation processes used 12 Rayon workers each, alongside the already-running independent greedy evaluation. No performance timing claims are made. No CUDA training, changes to weights, inference kernels, legal options or observations, and no shared-source edits. No all-seats temperature experiment was run.

The exact per-map VP totals are retained below so this small evidence survives loss of ignored runtime artifacts. Each total is over 36 candidate games, recovered unambiguously by rounding the printed mean times 36.

## Per-map VP totals

| Arm | Map seed | T=0.001 | T=0.5 | T=1.0 | T=2.5 |
|---|---:|---:|---:|---:|---:|
| start | 910001000 | 140 | 127 | 134 | 113 |
| start | 910001001 | 143 | 133 | 132 | 117 |
| start | 910001002 | 125 | 127 | 125 | 103 |
| start | 910001003 | 145 | 128 | 121 | 100 |
| start | 910001004 | 150 | 153 | 138 | 124 |
| start | 910001005 | 104 | 99 | 106 | 100 |
| start | 910001006 | 117 | 120 | 115 | 101 |
| start | 910001007 | 166 | 154 | 142 | 126 |
| start | 910001008 | 150 | 158 | 148 | 119 |
| start | 910001009 | 117 | 118 | 123 | 107 |
| start | 910001010 | 105 | 111 | 97 | 102 |
| start | 910001011 | 125 | 121 | 118 | 99 |
| start | 910001012 | 160 | 143 | 143 | 126 |
| start | 910001013 | 126 | 129 | 119 | 99 |
| start | 910001014 | 137 | 148 | 143 | 124 |
| start | 910001015 | 139 | 139 | 139 | 107 |
| start | 910001016 | 150 | 140 | 136 | 116 |
| start | 910001017 | 141 | 155 | 123 | 113 |
| start | 910001018 | 140 | 138 | 134 | 116 |
| start | 910001019 | 135 | 130 | 114 | 97 |
| control | 910001000 | 147 | 138 | 139 | 111 |
| control | 910001001 | 137 | 126 | 121 | 97 |
| control | 910001002 | 115 | 124 | 112 | 91 |
| control | 910001003 | 133 | 120 | 136 | 109 |
| control | 910001004 | 128 | 154 | 132 | 118 |
| control | 910001005 | 89 | 105 | 110 | 90 |
| control | 910001006 | 102 | 109 | 104 | 104 |
| control | 910001007 | 151 | 143 | 145 | 134 |
| control | 910001008 | 169 | 158 | 150 | 144 |
| control | 910001009 | 112 | 105 | 112 | 83 |
| control | 910001010 | 99 | 123 | 104 | 90 |
| control | 910001011 | 135 | 129 | 102 | 78 |
| control | 910001012 | 144 | 151 | 151 | 117 |
| control | 910001013 | 121 | 100 | 111 | 100 |
| control | 910001014 | 148 | 148 | 118 | 112 |
| control | 910001015 | 124 | 143 | 128 | 109 |
| control | 910001016 | 151 | 129 | 130 | 107 |
| control | 910001017 | 130 | 122 | 123 | 119 |
| control | 910001018 | 130 | 128 | 140 | 107 |
| control | 910001019 | 129 | 123 | 125 | 102 |
| vponly | 910001000 | 153 | 136 | 156 | 126 |
| vponly | 910001001 | 156 | 149 | 143 | 122 |
| vponly | 910001002 | 130 | 121 | 114 | 92 |
| vponly | 910001003 | 141 | 143 | 148 | 119 |
| vponly | 910001004 | 151 | 148 | 132 | 129 |
| vponly | 910001005 | 120 | 127 | 114 | 92 |
| vponly | 910001006 | 112 | 101 | 108 | 95 |
| vponly | 910001007 | 153 | 169 | 151 | 138 |
| vponly | 910001008 | 169 | 163 | 171 | 138 |
| vponly | 910001009 | 132 | 129 | 138 | 101 |
| vponly | 910001010 | 120 | 120 | 108 | 92 |
| vponly | 910001011 | 128 | 119 | 128 | 102 |
| vponly | 910001012 | 148 | 150 | 160 | 153 |
| vponly | 910001013 | 139 | 140 | 136 | 116 |
| vponly | 910001014 | 147 | 149 | 147 | 127 |
| vponly | 910001015 | 155 | 150 | 143 | 118 |
| vponly | 910001016 | 139 | 154 | 140 | 119 |
| vponly | 910001017 | 145 | 143 | 152 | 125 |
| vponly | 910001018 | 130 | 144 | 135 | 121 |
| vponly | 910001019 | 138 | 133 | 134 | 104 |
| waste1 | 910001000 | 151 | 143 | 138 | 115 |
| waste1 | 910001001 | 145 | 127 | 125 | 118 |
| waste1 | 910001002 | 126 | 124 | 128 | 104 |
| waste1 | 910001003 | 137 | 144 | 128 | 113 |
| waste1 | 910001004 | 154 | 135 | 131 | 129 |
| waste1 | 910001005 | 111 | 130 | 105 | 101 |
| waste1 | 910001006 | 117 | 112 | 109 | 99 |
| waste1 | 910001007 | 155 | 148 | 147 | 134 |
| waste1 | 910001008 | 159 | 156 | 158 | 151 |
| waste1 | 910001009 | 108 | 119 | 106 | 95 |
| waste1 | 910001010 | 104 | 118 | 107 | 97 |
| waste1 | 910001011 | 121 | 120 | 111 | 106 |
| waste1 | 910001012 | 150 | 146 | 138 | 123 |
| waste1 | 910001013 | 131 | 123 | 112 | 105 |
| waste1 | 910001014 | 138 | 150 | 132 | 110 |
| waste1 | 910001015 | 133 | 138 | 138 | 119 |
| waste1 | 910001016 | 166 | 144 | 134 | 121 |
| waste1 | 910001017 | 141 | 134 | 132 | 124 |
| waste1 | 910001018 | 132 | 141 | 132 | 106 |
| waste1 | 910001019 | 138 | 130 | 115 | 126 |

## Separate completed 100-map near-greedy evaluation

These results use seeds 910001000–910001099, candidate and opponents both at 0.001. They are not paired baselines for the 20-map sweep above.

| Arm | Maps | Mean VP |
|---|---:|---:|
| start | 100 | 3.7275 |
| control | 100 | 3.5939 |
| vponly | 100 | 3.8725 |
| waste1 | 100 | 3.7661 |
