# Focus + GAE continuation — 2026-10-11

Status: LAUNCHED 2026-10-11 11:01 CEST, controller PID31004; CUDA smoke running. Not qualified. Live status is in the run status.json.

Authority: operator releases GPU ("gpu use is permitted lets go") and confirms no other sessions working on the checkout/target. GPU experiments and fresh generated run/checkpoint output are within the authorized continuation. Preserve every pre-existing dirty source file and retained artifact. No Cargo/build, commit, push, cleanup, or source edits.

Checkout wp/base-factions at a8ed02533f3b2c1cc3bb38f49b280b5ba1b7e12a. Existing uncommitted focus/GAE changes and other engine/UI work remain untouched. Earlier catch-up read committed evidence and missed the later dirty compute queue/card-tag pilot; live state now reconciled.

## Experiment

The card-tag pilot is complete: newer-faction T-minus-control VP +0.005 [-0.021,+0.030], table -0.020 [-0.041,+0.002]. Operator retained tags. Latest line is a single focus=1 / GAE lambda=.95 wide-roster run, stopped at update229 for GPU handback. Latest COMPLETE checkpoint-16448 contains200updates. Updates201–229 were not saved.

Warm start checkpoint-16448 for800updates (200saved+800new), seeds1264300100 onward, 30games/update,4rounds,T2.5, original reward/entropy/lr settings. Optimizer moments, faction-focus smoothed VP, and local update index reset. This is NOT exact resume. No promotion/causal claim about tags, focus or GAE from this single combined trajectory.

Reuse exact binaries, built Oct9 from recorded 8432ba71 + dirty source, not compiled from current a8ed0253. Current source upgrade fix is therefore not claimed present. GIT_COMMIT preserves recorded compiled-source revision; launch.json also records actual checkout HEAD, hashes and dirty build provenance. Original outputs retained.

Trainer SHA256 B3DF6353A4760296CD2478211A8845A28BFC67144FA4815E615B0318C4B987E3.
Evaluator SHA256 69BCDE77A0CA3BCA9488FB21329F9776BD2FA93FEB271F667BCB720FC7DDF839.

Scripts: scripts/resume_focusgae_20261011.psd1; scripts/continue_focusgae_20261011.ps1.
Output: out/ppo-focusgae-FG-resume800-20261011 (fresh; refusal if exists).
Controller:1update/2game CUDA smoke, no checkpoint, distinct seeds1264300000; then800updates with8hour limit. Full stdout/stderr, child PID/phase in status.json. Failure/timeout stops progression; timeout stops ONLY the controller's owned child. No concurrent trainer/evaluator accepted at preflight.

After successful final checkpoint publish (source says800updates), run evaluator sequentially for baseline T checkpoint3584 and final: wide roster,600Validation seeds900000000..900000599,6rotations,4rounds,T.001,diplomacy. Retain matched per-seat TSVs. Report VP paired differences grouped original-six/new/table with seed-cluster uncertainty. Clearance's opening bar is six-calibrated, secondary for new factions.

Parallelism:16rayon workers, tensor intraop/interop1. About68GiB physical RAM free at preflight. No other training/eval/cargo/rustc processes found. GPU RTX3090, no compute process listed. Disk131GiB free. Training previously~27s/update (~6h for800); controller bound8h; each eval bound2h.

## Verification

- Existing historical focused test logs:20PPO tests,52seating tests passed Oct9 (not rerun now).
- Controller -CheckOnly:PASS for paths, every checkpoint manifest checksum, both binary fingerprints, settings, exclusive process check.
- PowerShell parser:0syntax errors.
- Independent script review: training_review (gpt-5.6-sol, separate agent), no blocking findings. Accepted final-manifest checksum verification and wait/clear PID after owned-child timeout. Warm-start and one-trajectory limits retained. Prior update219 logged infinite absolute log ratio once; monitor recurrence, no claim this historical event is fixed.
- GPU smoke/training/evaluation:not yet run.

## Remaining gates

Independent focus/GAE training-math review is not replaced by the script review. BF22wide migration/arena reviews, BF24manual decision coverage,BF25exit qualification remain open. Watch original-six regressions. Final paired eval uncertainty captures evaluation variation, not training variance.

## Cancellation

Read out/ppo-focusgae-FG-resume800-20261011/status.json. Stop the recorded controller_pid FIRST, then its recorded child_pid only after verifying executable/path ownership. This prevents the controller advancing into another phase. Preserve run data. No stop of unrelated processes.


Background launch diagnostic: first Windows PowerShell controller failed on module discovery BEFORE output creation/GPU use. Explicit builtin Utility/Management module imports added; hidden pwsh -CheckOnly then passed exit0. Original failure logs retained. Independent follow-up review accepts controller safeguards and paired VP maths; strict expected eval shape added before launch, analysis path/hash pinned.


Luna implemented scripts/paired_vp_bootstrap_20261011.py (stdlib), meaningful self-tests pass: identical arms,+1VP,mismatched keys,duplicate rows,valid/truncated/missing-rotation shape. On prior cardtagCvsT21600rows: table delta-.0196 CI[-.0398,+.0001], original-six-.1177,new+.0047, matching recorded pilot conclusion. Root reran self-test:PASS. Independent training_review follow-up approves bootstrap maths. Script SHA pinned in launch.json and rechecked before report. Grouped VP report after completed eval in paired-summary.stdout.log; evaluation-only intervals,1000resamples,seed20261011.


11:02CEST:CUDA smoke completed exit0,2games/12seat-games,parameters moved,Adam advanced8steps. Controller phase training,childPID49308,30games/update. No stderr reported. Live-check provenance does not qualify unrelated dirty source.


First training checkpoint92 published and reloaded identically; source confirms1update from checkpoint16448. Startup verified. Controller31004/child49308 active; automatic training/eval/report stages pending. No fresh full-suite qualification,commit or push.


Operator requested immediate stop2026-10-11. Owned controller31004 stopped first,then trainer49308; both exits verified. All outputs retained. Latest saved:D:\Projects\ti4-engine-rs\out\ppo-focusgae-FG-resume800-20261011\checkpoints\checkpoint-92;M10-034 PPO, 1 update(s) from D:\Projects\ti4-engine-rs\out\ppo-focusgae-FG-wide-long1000-20261010\checkpoints\checkpoint-16448, roster Wide. No eval/automatic restart. Next:baseline and correctness reviews before further training.

