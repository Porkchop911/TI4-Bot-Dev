# Reviewer demo assets

Enough of the gitignored working tree to let a fresh clone open the reviewer and watch a trained
policy play, without training anything first.

Everything under `/out/` is ignored by design — it is scratch, and it is large. These four things
are the exception, copied here because without them `ti4-review simulate` has no policy to run and
no board to run it on, and a clone can only look at the source.

| Path | What it is | Size |
|---|---|---:|
| `policy/stage2-r3-m2.494/` | A schema-6 MLP bundle: the Stage-2 r3 champion | 17 MB |
| `maps/save52_e400_train.json.gz` | The Save-52 Stage-2 training map pool | 63 KB |
| `maps/save52_e400_holdout.json.gz` | Its disjoint holdout pool | 19 KB |
| `reviews/four-actions-seed42.ti4review.json` | A pre-recorded session, so `validate` and `render` work with no model at all | 3 MB |

## The one prerequisite that is not in the repository

`ti4-review` links libtorch, which is 9,313 files and does not belong in git. `.cargo/config.toml`
points `LIBTORCH` at `out/libtorch-2.9.1-cpu`, relative to the repository root, so the pin travels
with the checkout rather than depending on a machine-specific path. Reproduce that directory from
`plans/artifacts/libtorch-2.9.1-cpu.manifest.json`, which fixes every file by SHA-256. The version
is exact, not a floor: `tch` is pinned `=0.22.0` because that is the release built against
libtorch 2.9.x.

On Windows the DLLs must also be on `PATH` to run the built binary directly.

```powershell
cargo build --release -p ti4-review
$env:PATH = "$PWD\out\libtorch-2.9.1-cpu\lib;$env:PATH"
```

## Three ways to see it work

**Open the recorded session.** No policy is loaded, so this works even if the bundle is missing.

```powershell
.\target\release\ti4-review.exe validate assets\demo\reviews\four-actions-seed42.ti4review.json
.\target\release\ti4-review.exe render  assets\demo\reviews\four-actions-seed42.ti4review.json out.html
```

**Play a fresh game with the champion.** `--unit` selects what `--count` counts; `action` is the
coarsest and the one worth starting from.

```powershell
.\target\release\ti4-review.exe simulate `
  --checkpoint assets\demo\policy\stage2-r3-m2.494 `
  --map-pool   assets\demo\maps\save52_e400_holdout.json.gz `
  --out        game.ti4review.json `
  --seed 7 --unit action --count 4
```

**The GUI.** Run `ti4-review` with no arguments. It starts with empty checkpoint and map-pool
fields and remembers what you last used in `out/reviews/reviewer-settings.json`; paste the two
paths above into them. It also lists previous sessions from `out/reviews`, which is empty in a
fresh clone until you save one.

## What the policy is

`stage2-r3-m2.494` is the Stage-2 result that survived replication: **+2.494 margin, 87.4% win,
3.079 mean VP** against a table of five frozen Stage-1 champions, holding 93.32% opening clearance.
Three later legs claimed further gains, and an r4 replicate with only the rollout seed base changed
put the noise floor for "best checkpoint of one leg" at 0.223 — wider than the gains. So this is
the honest champion to demo, not the highest number in the campaign log.

Its `manifest.json` records the rest: width 256, 14 heads, 33 faction rows, `slot_count` 11,147 in
a capacity of 16,384, `critic_mode: batch_mean`, and the commit that produced it.

## What is deliberately not here

The other ten champion bundles, every training checkpoint and log, the vocabulary corpus, the
Save-54 Stage-1 pools, and the full-length recorded sessions — some of which run to 275 MB each.
They stay in `/out/`, which stays ignored. This directory is a demo, not a backup.
