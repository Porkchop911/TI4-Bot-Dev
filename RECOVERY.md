# Recovery

If work on this repository goes wrong, this is how to get back to a known-good state.

## The known-good point

**Tag:** `known-good/2026-08-13-learning-loop`
**Branch:** `backup/2026-08-13-before-pi`
**Commit:** `80eb78b` — *Hand over at a working loop, with what real training still needs*

Verified at the moment it was tagged:

- `cargo fmt --all --check` — clean
- `RUSTFLAGS="-D warnings" cargo clippy --workspace --all-targets` — no errors
- `cargo test --workspace` — 18 test binaries, **0 failures**
- The Python oracle at `D:\Projects\ti4-engine` — untouched, `git status` clean

Both refs point at the same commit. Two names for it rather than one, because a branch is easy to
delete by accident and a tag is not usually the thing somebody reaches for.

## Getting back

**See what changed since the known-good point:**

```
git log --oneline known-good/2026-08-13-learning-loop..HEAD
git diff known-good/2026-08-13-learning-loop..HEAD --stat
```

**Look at the known-good tree without disturbing anything:**

```
git switch --detach known-good/2026-08-13-learning-loop
```

**Start again from it, keeping the bad work for inspection:**

```
git switch -c recovery/<what-you-are-doing> known-good/2026-08-13-learning-loop
```

This is the one to reach for. It leaves whatever went wrong exactly where it is, on its own
branch, so it can be read rather than only regretted.

**Recover one file:**

```
git checkout known-good/2026-08-13-learning-loop -- path/to/file.rs
```

**Recover a commit that seems to have vanished** — a bad reset, a branch deleted, a rebase gone
sideways. Git keeps every commit HEAD has pointed at for 90 days:

```
git reflog
git switch -c rescue <sha-from-the-reflog>
```

## If the whole `.git` directory is lost

There is a bundle of every ref at `.backup/known-good-2026-08-13.bundle`:

```
git clone .backup/known-good-2026-08-13.bundle recovered
```

**The bundle is untracked and gitignored, so `git clean -xdf` will delete it.** It guards against
losing `.git`, not against losing the directory. Copy it somewhere else before doing anything
drastic.

## Two gaps worth knowing about

**There is no remote.** `git remote -v` is empty, so every copy of this work is on one machine.
Nothing in this file survives the disk. Adding a remote and pushing — including the tag — is the
single largest improvement available to this repository's safety, and it is a decision for the
owner rather than something to do quietly.

**`main` is 181 commits behind.** It sits at `56c0435`, from before most of the engine existed, so
it is not a useful fallback. The working branch is
`wp/m08-007f-public-trade-good-reserves`. Falling back to `main` would discard nearly everything;
fall back to the tag instead.

## Rules that keep this recoverable

- **Never force-push, and never rewrite history that somebody else may have.**
- **Never `git reset --hard`** without first branching from where you are. `git switch -c` costs
  nothing and makes a mistake reversible.
- **Never delete the tag or the backup branch.**
- `git add -A` sweeps the `.worktrees/` gitlinks into a commit. Prefer `git add crates plans`.
- Commit at every green point, not at the end of a session. A commit that passes
  `cargo test --workspace` is a place somebody can return to; a working tree is not.
