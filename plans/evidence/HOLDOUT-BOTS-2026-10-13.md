# Holdout-only bot benchmark - 2026-10-13

Operator: benchmark ONLY on holdout pool going forward. Supersede generated-server-map strength comparisons with six-player held-out results. Existing holdout files all have six slots; operator explicitly chose leave four-player pending; no implicit conversion/generation.

Use existing rule_vs_mlp rollout/training evaluation path, Validation role/checksum verified immutable poolbytes, DEFAULT content/card setup, no server objective filter, checkpoint16448, T.001, cutoff4, core InScope roster. Seeds900000000..900000099, one original seat rotation (not six historical rotations), bothswaps mixed. Six arms all-rule/all-MLP/mixed x diplomacyOFF/ON =>800attempts on100distinct heldout boards (first100 of1000pool). Paired seedcluster stats on common successful seeds; no failed games scored0 or dropped silently. Do not label cutoff leaders as gamewins.

Scope P0/P1/P2: add homogeneous modes to existing runner, focused mode test, logs/perseat outputs/new manifests in existingout/rule-bot and plan records. Existing engine setup API/policy unchanged. No dirs/network/GPU/dependencies/training/promotions/remoteactions/deletion/overwrite. Cargo-j4/Rayon16; actual free physical RAM73.2GiB at preflight permits16seed workers despite stale queue headroom note, one coordinator. Bound40ksteps/game and existing500no-progress guard; cancel only taskowned process. Full producer logs/exitstatus retained. Independent Luna source/statistical review required. No automatic fixing repeated engine bugs in this measurement package.

Status: code reviewed by Luna, no correctness blocker. Tests109lib+2examplepass; buildpass. Actual two-game wide mixed seed900000000 ON regression reproduces all12 retained forensic rows exactly. Source/binary/pool/checkpoint manifest hashes/settings captured. Six-arm100map campaign completed on2026-10-14; operator chose leavefourplayerpending. Rule-only still copies actors perworker (reviewresourcecaveat), acceptable with measured73GiB freeRAM.

Historical interim snapshot, superseded by Final results below (run crossed midnight into2026-10-14): ON selfMLP97/100 successful mean3.579038VP/seat, three guards seeds900000066/095/096; ON selfrule100/100 mean2.763333. MixedONrunning; these allavailable-armmeans are preliminary and not finalcommonseedcomparisons. No inference or engine fix claimed.

## Final results

Benchmark uses only the checksum/Validation-role verified holdout pool. Four-player runs remain pending by explicit operator choice: no four-home holdout pool was found and no six-home board was converted. Generated-server-map scores are superseded for operator-facing strength comparisons.

800 attempted cutoff games; 764 usable; 36 retained guard failures. Main comparison uses the SAME 71 complete seeds across all six arms; both mixed swaps required. Excluded seeds: [900000008, 900000009, 900000012, 900000020, 900000022, 900000025, 900000030, 900000033, 900000035, 900000039, 900000045, 900000046, 900000047, 900000048, 900000051, 900000055, 900000058, 900000061, 900000062, 900000065, 900000066, 900000071, 900000077, 900000086, 900000087, 900000093, 900000095, 900000096, 900000099]. These are round4 standings, not official completed-game wins.

| Diplomacy | Rule self-play | MLP self-play | Rule in mixed | MLP in mixed |
|---|---|---|---|---|
| OFF | 3.347 [3.232, 3.463] | 2.254 [2.128, 2.379] | 3.465 [3.308, 3.621] | 1.840 [1.707, 1.974] |
| ON | 2.786 [2.669, 2.904] | 3.601 [3.470, 3.732] | 3.207 [3.070, 3.343] | 3.749 [3.616, 3.882] |

Values are meanVP/seat [nominal95% tCI]. Independent unit is seed/map. Average six homogeneous seats, or three seats/type/game then the two mixed swaps (each faction once pertype/seed). Same faction and seat assignments checked across all available arms. Pairing uses identical seeds, pool selection and starting setup through the same unchanged rollout API. No saved per-game initial hashes in this legacy output; source-path parity and faction/seat parity are verified, not a hash assertion. Source has one original seating orientation per seed, not the historical six rotations; sample covers100 unique poolmaps, not all1000.

## Paired effects and robustness

| Contrast | VP/seat change [95% paired CI] | Holm-adjusted p (8 tests) |
|---|---|---:|
| diplomacy/self/rule | -0.561 [-0.694, -0.428] | 2.1172e-11 |
| diplomacy/self/mlp | 1.347 [1.189, 1.506] | 1.1398e-25 |
| diplomacy/mixed/rule | -0.258 [-0.404, -0.113] | 0.0021178 |
| diplomacy/mixed/mlp | 1.908 [1.772, 2.045] | 1.0386e-38 |
| mixed-minus-self/off/rule | 0.117 [-0.063, 0.297] | 0.19761 |
| mixed-minus-self/off/mlp | -0.413 [-0.556, -0.270] | 1.0493e-06 |
| mixed-minus-self/on/rule | 0.420 [0.273, 0.567] | 1.0945e-06 |
| mixed-minus-self/on/mlp | 0.148 [0.012, 0.284] | 0.066414 |

Eight exploratory paired tests: four diplomacy gains and four mixed-minus-self changes. Holm family controls multiplicity within these8; no preregistration or checkpoint promotion claim. Percentile bootstrap resamples whole matchedseedclusters20000times (RNG20261013) and is saved for sensitivity. Individual cell/faction confidence intervals are descriptive, not simultaneous. Differences between separate homogeneous tables measure scoring pace in different opponent environments, not direct head-to-head strength.

| Setting | MLP minus rule VP/seat [95% paired CI] |
|---|---|
| self/off | -1.094 [-1.240, -0.948] |
| self/on | 0.815 [0.686, 0.944] |
| mixed/off | -1.624 [-1.798, -1.451] |
| mixed/on | 0.542 [0.359, 0.726] |

## Faction means (descriptive)

| Faction | OFF rule self | OFF MLP self | OFF rule mixed | OFF MLP mixed | ON rule self | ON MLP self | ON rule mixed | ON MLP mixed |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| hacan | 3.352 | 2.169 | 3.310 | 1.493 | 2.845 | 6.704 | 3.338 | 5.437 |
| sol | 3.831 | 2.310 | 3.831 | 1.718 | 2.901 | 3.042 | 3.394 | 3.563 |
| jolnar | 3.465 | 2.296 | 3.634 | 1.986 | 2.845 | 2.915 | 3.282 | 3.408 |
| l1z1x | 3.380 | 2.239 | 3.761 | 2.282 | 2.958 | 3.099 | 3.155 | 3.493 |
| letnev | 2.944 | 2.127 | 3.141 | 1.606 | 2.789 | 3.099 | 3.197 | 3.099 |
| xxcha | 3.113 | 2.380 | 3.113 | 1.958 | 2.380 | 2.746 | 2.873 | 3.493 |

## Failure/exclusion accounting

| Arm | Seed | Swap | Error |
|---|---|---|---|
| on/mlp | 900000066 | false | seat0 was asked "after AGENDA_REVEALED" 501 times in one engine step without progress (limit 500); the decision cycle never ends |
| on/mlp | 900000095 | false | seat2 was asked "after SYSTEM_ACTIVATED" 501 times in one engine step without progress (limit 500); the decision cycle never ends |
| on/mlp | 900000096 | false | seat0 was asked "after AGENDA_REVEALED" 501 times in one engine step without progress (limit 500); the decision cycle never ends |
| on/mixed | 900000008 | false | seat1 was asked "after AGENDA_REVEALED" 501 times in one engine step without progress (limit 500); the decision cycle never ends |
| on/mixed | 900000020 | false | seat1 was asked "after SYSTEM_ACTIVATED" 501 times in one engine step without progress (limit 500); the decision cycle never ends |
| on/mixed | 900000030 | true | seat2 was asked "after AGENDA_REVEALED" 501 times in one engine step without progress (limit 500); the decision cycle never ends |
| on/mixed | 900000045 | false | seat3 was asked "after SYSTEM_ACTIVATED" 501 times in one engine step without progress (limit 500); the decision cycle never ends |
| on/mixed | 900000058 | false | seat3 was asked "after AGENDA_REVEALED" 501 times in one engine step without progress (limit 500); the decision cycle never ends |
| on/mixed | 900000062 | true | seat2 was asked "after SYSTEM_ACTIVATED" 501 times in one engine step without progress (limit 500); the decision cycle never ends |
| on/mixed | 900000086 | false | seat5 was asked "after AGENDA_REVEALED" 501 times in one engine step without progress (limit 500); the decision cycle never ends |
| off/mlp | 900000008 | false | seat4 was asked "after AGENDA_REVEALED" 501 times in one engine step without progress (limit 500); the decision cycle never ends |
| off/mlp | 900000012 | false | seat4 was asked "after SYSTEM_ACTIVATED" 501 times in one engine step without progress (limit 500); the decision cycle never ends |
| off/mlp | 900000020 | false | seat1 was asked "after SYSTEM_ACTIVATED" 501 times in one engine step without progress (limit 500); the decision cycle never ends |
| off/mlp | 900000039 | false | seat5 was asked "after AGENDA_REVEALED" 501 times in one engine step without progress (limit 500); the decision cycle never ends |
| off/mlp | 900000045 | false | seat5 was asked "after SYSTEM_ACTIVATED" 501 times in one engine step without progress (limit 500); the decision cycle never ends |
| off/mlp | 900000047 | false | seat1 was asked "after AGENDA_REVEALED" 501 times in one engine step without progress (limit 500); the decision cycle never ends |
| off/mlp | 900000048 | false | seat3 was asked "after SYSTEM_ACTIVATED" 501 times in one engine step without progress (limit 500); the decision cycle never ends |
| off/mlp | 900000051 | false | seat5 was asked "after SYSTEM_ACTIVATED" 501 times in one engine step without progress (limit 500); the decision cycle never ends |
| off/mlp | 900000065 | false | seat2 was asked "after SYSTEM_ACTIVATED" 501 times in one engine step without progress (limit 500); the decision cycle never ends |
| off/mlp | 900000071 | false | seat0 was asked "after AGENDA_REVEALED" 501 times in one engine step without progress (limit 500); the decision cycle never ends |
| off/mlp | 900000086 | false | seat0 was asked "after AGENDA_REVEALED" 501 times in one engine step without progress (limit 500); the decision cycle never ends |
| off/mlp | 900000087 | false | seat4 was asked "after SYSTEM_ACTIVATED" 501 times in one engine step without progress (limit 500); the decision cycle never ends |
| off/mlp | 900000095 | false | seat2 was asked "after SYSTEM_ACTIVATED" 501 times in one engine step without progress (limit 500); the decision cycle never ends |
| off/mixed | 900000009 | false | seat1 was asked "after AGENDA_REVEALED" 501 times in one engine step without progress (limit 500); the decision cycle never ends |
| off/mixed | 900000022 | false | seat5 was asked "when AGENDA_REVEALED" 501 times in one engine step without progress (limit 500); the decision cycle never ends |
| off/mixed | 900000025 | false | seat1 was asked "after SYSTEM_ACTIVATED" 501 times in one engine step without progress (limit 500); the decision cycle never ends |
| off/mixed | 900000033 | true | seat0 was asked "after AGENDA_REVEALED" 501 times in one engine step without progress (limit 500); the decision cycle never ends |
| off/mixed | 900000035 | true | seat2 was asked "after AGENDA_REVEALED" 501 times in one engine step without progress (limit 500); the decision cycle never ends |
| off/mixed | 900000046 | false | seat5 was asked "after AGENDA_REVEALED" 501 times in one engine step without progress (limit 500); the decision cycle never ends |
| off/mixed | 900000055 | true | seat2 was asked "after SYSTEM_ACTIVATED" 501 times in one engine step without progress (limit 500); the decision cycle never ends |
| off/mixed | 900000061 | true | seat4 was asked "after STRATEGY_PHASE_BEGAN" 501 times in one engine step without progress (limit 500); the decision cycle never ends |
| off/mixed | 900000066 | false | seat1 was asked "when AGENDA_REVEALED" 501 times in one engine step without progress (limit 500); the decision cycle never ends |
| off/mixed | 900000077 | false | seat5 was asked "after SYSTEM_ACTIVATED" 501 times in one engine step without progress (limit 500); the decision cycle never ends |
| off/mixed | 900000087 | true | seat4 was asked "after SYSTEM_ACTIVATED" 501 times in one engine step without progress (limit 500); the decision cycle never ends |
| off/mixed | 900000093 | true | seat2 was asked "after AGENDA_REVEALED" 501 times in one engine step without progress (limit 500); the decision cycle never ends |
| off/mixed | 900000099 | true | seat4 was asked "after AGENDA_REVEALED" 501 times in one engine step without progress (limit 500); the decision cycle never ends |

No incomplete game is scored0, silently retried, or counted as win. Allsuccessfulgames pass inference status checks; numerical fallback/unusedmodel errors cause refusal. Guard errors retain seed/swap and full error in raw logs, not inner decision traces; these symptoms alone do not establish the exact reaction root. Known agenda nested-decline bug remains open and SYSTEM_ACTIVATED roots remain separate/unconfirmed. Results conditional on complete-case seeds; exclusions may be informative.

| Cell | Available complete seeds | Available-seed mean | Main matched mean |
|---|---:|---:|---:|
| off/self/rule | 100 | 3.330 | 3.347 |
| off/self/mlp | 87 | 2.193 | 2.254 |
| off/mixed/rule | 87 | 3.456 | 3.465 |
| off/mixed/mlp | 87 | 1.793 | 1.840 |
| on/self/rule | 100 | 2.763 | 2.786 |
| on/self/mlp | 97 | 3.579 | 3.601 |
| on/mixed/rule | 93 | 3.151 | 3.207 |
| on/mixed/mlp | 93 | 3.758 | 3.749 |

## Checks and artifacts

Tests109MLPlib+2example passed, focused homogeneous/mixedseatmapping and unused-inference refusal tested; buildpassed. Actual two-game mixed wideON seed900000000 reproduces all12 prior retained forensic rows exactly. Runner independently source reviewed by Luna before campaign. New no-overwrite behavior refuses existing per-seat outputs before starting and creates exclusively at final write. Validation artifact role/checksum checked on everyarm; source/pool remained equal to settingsmanifest through analysis.

Analysisassertions verify gamecounts/factions/modes/seats/swapcoverage/producerexitstatus and samplematching. Rawsixarmlogs+perseatTXT+exitfiles/settings are underexisting out/rule-bot/holdout-only-20261013*. Reproducible analyze/report scripts, fullsummary, matchedseedCSV and SHA256resultsmanifest (selfexcluded) retained. No raw overwrite or deletion. Roster InScope DEFAULTcontent/cardsetup via existing trainingrollout, tileoffset0, fixed checkpoint16448 andtemperature.001. No serverobjectivefilter/no generatedmaps.

Final Luna statistics/accounting review passed: separate raw recalculation verified common71,means/t95/bootstrap20k/Holm8/rowparity/36guards/exitcodes and all manifests. No training/enginefix/dependencyinstall/GPU/network/promotion/fullworkspace/GUI qualification. No claimed historical improvement: although holdout setup is restored, thischeckpoint/engine/samples/rotationcount differ from old dated evaluations. Four-player holdoutawaits compatiblepool.

Completion: all owned processes exited and compute released. Actual CLI invalid-mode and existing-output refusal both exit2; regression output SHA256 unchanged. Plotted common71 estimates [chart](../../out/rule-bot/holdout-only-20261013-chart.png) / [PDF](../../out/rule-bot/holdout-only-20261013-chart.pdf), rasterized and visually inspected. Luna source+statistics review passed, no correctness blockers. All task changes remain uncommitted; unrelated dirty Cargo.lock preserved.
