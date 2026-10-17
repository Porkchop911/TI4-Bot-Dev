# Fork coordination plan (2026-10-14)

Status: proposal for the owner (Porkchop911) and the two fork maintainers, Felix Stamm
(`Feelx234/TI4-Bot-Dev`) and ProbablyPlayable (`ProbablyPlayable/TI4-Bot-Dev`). Nothing here is
decided until the owner marks it. Every number below was measured on 2026-10-14 from the three
repositories' Git history; the commands are in the appendix so the numbers can be refreshed.

## Status update (2026-10-15)

- **Phase 1 is done, as option B1.** PR #6 (`integrate/forks-2026-10-14`) merged
  `ProbablyPlayable/dev` (which held `Feelx234/main`), `wp/rule-bot` and `wp/base-factions` into
  `main` (`9bbded7a`, history kept). `cargo test -p ti4-engine --release` on that `main` gives
  3,015 passed, 0 failed, 2 ignored.
- **Still open from §3 and §7:**
  - D6/D7 were resolved by keeping both sides. `technology::gain` and `unit_form_violations` and
    their two corpus-wide tests are both on `main`, and so are `FactionRoster` and
    `lobby_faction_draw`.
  - `main`'s `AGENTS.md` is now the forks' 4-line file. The integration merge `38200ca0` took it
    over the owner's 155-line file on `wp/rule-bot` (which still has it), and
    `crates/ti4-engine/AGENTS.md` (§6.2) does not exist yet.
  - `cargo fmt --check` still fails on 34 files of `main`, so CI never reaches the tests.
- **New since the merge:** ProbablyPlayable's `dev` is 59 commits ahead of `main`. It folds
  `ti4-view` into `ti4-host`, adds `web-shared/` (one transport, save store and wasm build for
  `web/` and `web2/`) and generates wire types with ts-rs; `web/` changes by +3.9k/−17k lines.
  `Feelx234/main` has not moved since 2026-10-14 12:37 UTC.

## 1. Where we are

### 1.1 Three people, two lines of history

| Line | Tip (2026-10-14) | Relation |
|---|---|---|
| `Porkchop911/main` | `1b4fcec` (2026-10-08) | Has not moved for six days. |
| `Porkchop911/wp/base-factions` | `a8ed025` (2026-10-10) | 28 commits that neither fork has. |
| `Feelx234/main` | 2026-10-14 12:37 UTC | Contains all of `Porkchop911/main`. 2,006 commits ahead, 0 behind. |
| `ProbablyPlayable/dev` | 2026-10-14 15:53 UTC | Contains all of `Feelx234/main` plus 25 commits. 2,031 ahead, 0 behind. |

The two forks are **one line**: they merge each other every day or two, and all of their side
branches are already merged. So there are two lines in all: the owner's and the forks'.

Of the ~2,000 "ahead" commits, **694 are the forks' own work** (437 Felix, 257 ProbablyPlayable).
**104 of them touch the engine crates** (`ti4-engine`, `ti4-content`, `ti4-model`, `ti4-sim`,
`ti4-replayer`). The other ~1,300 are the owner's own history from before the history reset on
2026-10-03, when `main` got new roots (`002c86d9`, `759e78e8`). They came back in on 2026-10-07
through ProbablyPlayable's merge `3b0f0cd0` ("merge main"): the `web-ui` branch started before the
reset, so merging it brings back the old root `80d15ebd` and ~550 MB (uncompressed) of blobs that
`main` no longer has.

PR #3 (Felix, 2026-10-07) did land: its head `baa3f471` is in `main` through merge `03a0a9c6`, and
the owner merged it into the base-factions line in `151b9c7a`. PR #2 (ProbablyPlayable, `web-ui`)
did not land. Since 2026-10-07 nothing has flowed from the forks to the owner.

### 1.2 Pace

Forks' own commits per day: 230 on 2026-10-07/08, then 45–80 a day (2026-10-09 to 10-14).
Since 2026-09-20 the owner's commits touched `ti4-engine` 647 times, `plans/` 1,140 times and the
training crates (`ti4-mlp`, `ti4-training`, `ti4-policy`, `ti4-replayer`, `ti4-review`) ~350 times.

### 1.3 Who works where (files touched, own commits)

| Person | Main areas | Engine |
|---|---|---|
| Porkchop911 (porkchop411) | plans and evidence, engine rules and factions, training (`ti4-mlp`, `ti4-training`, `ti4-policy`, `ti4-tensor`), `ti4-replayer`, `ti4-review`, fixtures | 647 file-touches since 09-20 |
| Felix Stamm | `web/` and its e2e screenshots (~2,450), `ti4-server` (468), `tools/`, `deploy/`, nightly smoke | 183: rules fixes from playtests, fork/snapshot, RNG positions, deck reserve, display-only decision details |
| ProbablyPlayable | `web2/` (1,053), `web/` (894), `ti4-server` (700), `ti4-view` (107), `ti4-wasm` (79) | 120: fork primitive, force a turn, randomness markers, the rule-bot crate, wasm build changes |

The interests overlap less than the commit counts suggest. The collisions are in three places: the
engine core (`game.rs`, `choice.rs`, `technology.rs`, `exploration.rs`, `seating.rs`),
`ti4-server` (Felix and ProbablyPlayable), and the two web clients (`web/` and `web2/`, both with an
offline wasm mode built on the same `ti4-wasm`).

### 1.4 What is going wrong

1. **The forks no longer have the project's engine rules.** `AGENTS.md` went from 140 lines to 4
   on 2026-10-08 (`52423f47`, "remove all weird instructions"); `CLAUDE.md` links to it. Most fork
   commits are written by Claude sessions, so those agents work without the accuracy rules
   (determinism, generated legality, stable choice ids, hidden information through typed views).
   The forks' new code points at a rule that no file contains (`game.rs`: "FORK RULE (see
   AGENTS.md)").
2. **The same features are already being built twice.** Unit upgrades: the owner's
   `technology::gain` (`a8ed0253`) against Felix's one-form-per-seat rule plus
   `unit_form_violations` (`1b09dd48`), each with its own test over every unit upgrade. Faction
   seating: the owner's `FactionRoster::{InScope, Wide, New}` (the roster for training the 26 new
   factions) against the forks' `lobby_faction_draw`, with a fixed pool of 24 base and PoK
   factions. The decision table (`choice.rs`): the owner's no-progress guard against the forks'
   observers and `auto_resolve`.
3. **Merging back is getting more expensive.** A trial merge of `wp/base-factions` into
   `ProbablyPlayable/dev` conflicts in 5 engine files with 10 conflicting sections (`game.rs` 4,
   `choice.rs` 2, `technology.rs` 2, `exploration.rs` 1, `seating.rs` 1). Both sides changed the
   same 31 crate files.
4. **The forks depend on engine contracts the owner doesn't know about.** `Game::snapshot` lists
   every field of `Game` (a new field doesn't compile until it is copied). Every deck draw must go
   through `ti4_model::deck_reserve`. Turn redo sets the positions of the dice and deck random
   streams.
5. **Behaviour drift between where policies train and where people play.** Fork lobby games
   default to the TE strategy card set, draw the speaker at random, and deal out objectives the
   map can't satisfy. These live in `ti4-host`/`ti4-view`/`ti4-server`, so engine-level training
   is unaffected, but people play under different conditions than the policies trained under.
   Enigmatic Device got new decision ids; old games still replay.
6. **CI doesn't protect anything.** Every recent run on `Porkchop911/TI4-Bot-Dev` fails at
   `cargo fmt --check` (48 files on `main` are not formatted), so Clippy and the tests never run.
   The forks' engine suite is healthy: `cargo test -p ti4-engine --release` on
   `ProbablyPlayable/dev` (`3f62f650`) gives 2,995 passed, 0 failed, 2 ignored.

## 2. Goals

- One canonical branch that all three build on, at most a few days behind any person's work.
- One set of engine rules that every person's agents read.
- No feature built twice without a decision first.
- A change that alters seeded outcomes or replays is made on purpose, with sign-off from whoever
  owns training.
- CI that runs the tests, and nobody merges on red.

## 3. Decisions the owner needs to make first

| # | Decision | Options | Recommendation |
|---|---|---|---|
| D1 | Which repository is canonical | `Porkchop911` / a fork / a new shared org | `Porkchop911/TI4-Bot-Dev`, with Felix and ProbablyPlayable as collaborators with push rights to branches |
| D2 | Can the pre-reset history (old root `80d15ebd`, ~550 MB of blobs) come back into `main`? | yes / no (rewrite the fork line onto the new roots) | Depends on why `main` was reset on 10-03. If it was to drop data or size, say no and integrate by squash or rebase (approach B2). Otherwise yes. |
| D3 | Who has the final say on engine semantics | the owner / a named engine maintainer / consensus | the owner, for anything that changes outcomes, ids or defaults |
| D4 | `web/` and `web2/` | keep both / web2 replaces web on a date / merge them | Decide within two weeks; see §6.3 |
| D5 | Lobby defaults (TE card set, random speaker, objective deal-out) | keep / make them opt-in | Keep in the lobby, but make them explicit game options that training can name too |
| D6 | One unit-upgrade implementation | `technology::gain` / one-form-per-seat / combine | Combine: keep `gain` as the single way in and `unit_form_violations` as the invariant test |
| D7 | One faction-seating implementation | `FactionRoster` / `lobby_faction_draw` / combine | The lobby pool becomes a `FactionRoster` variant; one seeded draw |

## 4. Integration models

### A. Hub and spoke: the owner's `main` is the trunk, the forks send PRs

Each person keeps a fork or works on branches in the main repository. Work reaches `main` only
through PRs that are reviewed and green.

- For: one source of truth; the owner sees every engine change; GitHub keeps the record.
- Against: the owner becomes the bottleneck at 50–100 commits a day; the PRs of 10-07 show that
  big batches go stale fast.
- Needs: small PRs (one coherent change, at most a few days old), working CI, a review rota that
  doesn't depend on one person.

### B. Adopt the fork line as the new `main` (once), then hub and spoke

The fork line already contains all of `main`, and its suite is green. Merge `wp/base-factions`
into it once, resolve the 5 conflicts, and make the result `main`.

- **B1, keep the history:** fast-forward `main` to the merged result. Simple, but brings back the
  pre-reset history (D2).
- **B2, keep `main`'s roots:** take the merged tree as one commit (or a few squashed commits) on
  top of `1b4fcec`. Clean history; the forks then reset their branches to the new `main` and
  rebase or replay whatever they haven't pushed.
- For: the cheapest way to converge (the forks never have to rebase 700 commits; the owner merges
  28); after one integration day, everyone is on the same code.
- Against: one large unreviewed delta lands at once (~12.7k engine lines). Mitigation: review by
  area (§7, phase 1) and the tests.

### C. Federated: each keeps their own line, with a fixed sync schedule

The lines stay separate and somebody merges both ways twice a week.

- For: nobody is blocked by anybody.
- Against: this is what is happening now, plus a schedule. Every sync pays for every collision,
  and duplicate features still only show up at merge time. Not recommended for the engine.

### D. Split the engine into its own versioned component

`ti4-engine`, `ti4-content` and `ti4-model` (and probably `ti4-view`/`ti4-host`) become a library
with releases; the server, the web clients and training pin a version.

- For: a hard boundary; engine changes are deliberate events with changelogs.
- Against: a lot of ceremony for three people; nearly every UI feature needs a small engine change
  (display-only details, read-only queries), so most work would cross the boundary.
- Worth it later, if outside contributors appear or training needs a frozen engine for months.

### Recommendation

**B now, then A.** B is the only option that converges in days rather than weeks. After that,
hub and spoke keeps the line together, as long as PRs stay small and CI works. Pick B1 or B2 by D2.
Keep D in reserve.

## 5. Ways to divide the work

### 5.1 By layer, with owners

Each crate or directory has an owner who reviews every change to it.

- For: clear responsibility; matches how the work already splits (§1.3).
- Against: almost every user-visible feature crosses layers (engine → host/view → server →
  client), so features wait for reviews.

### 5.2 By feature, end to end

A person takes a feature (trade, combat odds, a faction) across every layer.

- For: no hand-offs; one person knows the whole feature.
- Against: everyone edits `game.rs`; this is exactly how the duplicate unit-upgrade and seating
  work happened.

### 5.3 Hybrid (recommended): layer owners plus feature leads

Layers have owners (CODEOWNERS). Features have a lead who may change any layer, but each layer's
owner reviews the change to their layer, and the engine follows the change classes of §6.1.

Proposed owners, based on who already does the work:

| Area | Owner | Second reviewer |
|---|---|---|
| Engine rules, factions, content (`ti4-engine` rules modules, `ti4-content`, `ti4-model`) | Porkchop911 | Felix |
| Engine copy and replay contracts (`Game::fork`/`snapshot`, `deck_reserve`, RNG stream positions, `hash_survey`) | Felix | Porkchop911 |
| Engine read-only queries and display-only data (`secondary_preview`, `reach_query`, `DecisionContext`/`details`) | whoever needs it, by change class 1 | Porkchop911 |
| Training (`ti4-training`, `ti4-policy`, `ti4-mlp`, `ti4-tensor`), `ti4-replayer`, `ti4-review`, `plans/` | Porkchop911 | — |
| Server (`ti4-server`: sessions, turn redo, batches, recovery, lobby HTTP) | Felix | ProbablyPlayable |
| Shared host layer (`ti4-host`, `ti4-view`, wire protocol) | ProbablyPlayable | Felix |
| `ti4-wasm`, `ti4-rule-bot`, `ti4-calc` | ProbablyPlayable | Porkchop911 (rule bot and calc touch training) |
| `web/` and playtest operations (`deploy/`, nightly smoke, feedback) | Felix | — |
| `web2/` | ProbablyPlayable | — |

A one-line CODEOWNERS file for each row puts this into GitHub.

### 5.4 Bug intake from playtests

Most engine fixes on the forks come from playtests (Chaos Mapping, Muaat war sun, Antimass
Deflectors, Naalu Z'eu, hyperlanes). Keep that flow, but route it:

1. Whoever finds the bug writes a failing engine test (a repro, not a UI test).
2. If the fix is local to a rule, the finder fixes it (change class 2) and the engine owner
   reviews.
3. If it touches shared machinery (timing, reactions, choice, payment), the engine owner fixes it
   or approves the approach first.

## 6. Engine rules everybody follows

### 6.1 Change classes

Every engine PR says which class it is in.

| Class | Example | Required evidence | Approval |
|---|---|---|---|
| 1. Additive, display-only or read-only | `details.agenda`, `reach_query`, `DecisionContext.hit` | engine tests pass; `hash_survey` gives identical decision and final-state hashes on saved games | any owner |
| 2. Rules bug fix | Muaat war sun, Antimass Deflectors | a failing test first; note which seeds or recorded games change | engine owner |
| 3. Default or setup change | TE card set by default, random speaker, objective deal-out | an option that is off where training runs; listed in the changelog | engine owner and training owner |
| 4. Choice-id or protocol change | Enigmatic Device ids, free-form trade ids | old ids still replay (as Enigmatic Device already does); registry updated | engine owner and training owner |

### 6.2 Put the rules where the forks' agents will read them

The forks' 4-line `AGENTS.md` tells agents to read the AGENTS/CLAUDE.md of the directory they work
in. So a `crates/ti4-engine/AGENTS.md` reaches every agent that touches the engine, in all three
lines, without reverting anyone's root file. It should hold:

- the accuracy rules from the owner's root `AGENTS.md` (generated legality, determinism, exact
  arithmetic, typed hidden information, stable choice ids);
- the copy rule (`Game::snapshot`/`GameSnapshot` destructure without `..`; a new field is copied
  or listed as deliberately not copied; `tests/fork_equivalence.rs` must pass);
- the deck rule (every draw goes through `ti4_model::deck_reserve`);
- the change classes of §6.1.

The owner's root `AGENTS.md` keeps the process rules (plans, evidence, Pi packages), which the
forks are free not to use.

### 6.3 Two clients

`web/` (Felix) and `web2/` (ProbablyPlayable) both render every decision type, and both have an
offline wasm mode. Each new decision type or display-only field therefore costs two UI
implementations, two screenshot sets and two e2e suites. Options:

- **Keep both, share more:** both read only `ti4-view`'s projections and `ti4-host`'s protocol,
  so the engine and server work is done once. Reasonable while web2 is still catching up.
- **web2 replaces web on a date:** freeze new features in `web/` (bug fixes only) once web2
  covers every decision type; Felix moves to the server, playtest operations and web2 screens.
- **Merge:** take web2's architecture (session transports, the wasm host) and port web's decision
  screens into it.

This is the owners' call (D4). Whichever it is, the decision type → screen coverage table should
exist for both clients so a gap is visible.

## 7. Plan

### Phase 0: agree (1–2 days)

- [ ] Owner decides D1–D7 (§3); write the answers into this file.
- [ ] Give Felix and ProbablyPlayable push access to branches in `Porkchop911/TI4-Bot-Dev`, if D1
      stays as recommended.
- [ ] Short call or thread: the owners of §5.3 agree to their rows.

### Phase 1: converge (2–3 days, one person drives, the others review by area)

- [ ] Merge `wp/base-factions` into `ProbablyPlayable/dev` on an integration branch. Resolve the
      conflicts by decision: `technology.rs` and `exploration.rs` by D6; `seating.rs` by D7;
      `choice.rs` keeps both (the no-progress guard and the observers); `game.rs` keeps both
      (`STATUS_INCOME_TRADE_GOODS` and the observers; check that `GameSnapshot` copies any new
      field).
- [ ] Run `cargo fmt --all`, `cargo clippy --workspace --all-targets`, `cargo test --workspace`,
      `hash_survey` on the saved games, and a short training smoke from the owner's runbook.
- [ ] Review by area: the owner reviews the 104 engine commits as one diff by file (biggest:
      `game.rs`, `exploration.rs`, `action_cards.rs`, `reactions.rs`, `timing.rs`); Felix and
      ProbablyPlayable review the conflict resolutions in their areas.
- [ ] Land it as the new `main` (B1 or B2 by D2). The forks reset `main`/`dev` to it.
- [ ] Add `crates/ti4-engine/AGENTS.md` (§6.2) in the same PR or straight after it.

### Phase 2: make CI a real gate (1 day, any one person)

- [ ] Format `main` once (48 files today) so the Format step passes.
- [ ] Protect `main`: PRs only, CI green, one review from the CODEOWNER of each touched area.
- [ ] Add `hash_survey` (or a short form of it) to CI, so class 1 changes prove that hashes don't
      change.
- [ ] Add CODEOWNERS from §5.3.
- [ ] CI runs on `windows-latest`. If Linux is enough, it is faster and cheaper; check that the
      content checksum step doesn't depend on line endings.

### Phase 3: steady state (ongoing)

- **Branches live at most 3 days.** Anything longer goes behind an option or is split.
- **PR size:** one coherent change. A web screenshot set may be large; the code part should not
  be.
- **Claim before building:** a one-line entry in a shared "in progress" list (an issue, or a
  `plans/IN_PROGRESS.md` table) before starting anything in the engine core or a shared seam. This
  is the cheapest way to stop the next duplicate of unit upgrades or seating.
- **Weekly sync, 30 minutes:** what landed, what is claimed, the open decisions, the CI state.
- **Changelog for classes 3 and 4**, so training knows which runs are comparable.

### Phase 4: later, if needed

- Engine as a versioned component (approach D), if training needs a frozen engine or outside
  contributors appear.
- A protocol version on the wire messages of `ti4-host`, if web and web2 deploy separately.

## 8. Risks

| Risk | Mitigation |
|---|---|
| Phase 1 lands a large delta nobody fully reviewed | review by area; the engine suite (about 3,000 tests) and `hash_survey`; a revert plan (keep the old `main` as a tag) |
| The forks keep committing during phase 1 | announce a 2–3 day window; afterwards rebase what landed in the window (a few dozen commits, not hundreds) |
| The owner becomes the bottleneck on review | second reviewers in §5.3; class 1 changes need any owner, not the engine owner |
| History size if B1 (≈550 MB uncompressed) | choose B2 if size or content was the reason for the reset |
| Training runs before and after convergence can't be compared | tag the convergence commit; the changelog of classes 3 and 4 says which defaults changed |
| Known bugs parked as ignored tests drift | list them in the in-progress table: `payment::plans` caps a payment at 6 planets (`MAX_PLANETS_PER_PLAN`, also on `main`; test `eight_one_resource_planets_can_pay_eight` in the forks); `session_planning.rs` has one failing and one flaky, hanging test |

## Appendix: commands behind the numbers

```sh
git remote add feelx https://github.com/Feelx234/ti4-bot-dev
git remote add pp https://github.com/ProbablyPlayable/ti4-bot-dev
git fetch origin && git fetch feelx && git fetch pp

# ahead/behind of each line
git rev-list --count origin/main..pp/dev; git rev-list --count pp/dev..origin/main
# the forks' own commits, and the engine-touching ones
git rev-list --count --no-merges pp/dev --not --remotes=origin
git rev-list --count --no-merges pp/dev --not --remotes=origin -- \
  crates/ti4-engine crates/ti4-content crates/ti4-model crates/ti4-sim crates/ti4-replayer
# owner commits the forks lack
git rev-list --count --no-merges origin/wp/base-factions --not pp/dev
# trial merge (in a throwaway worktree)
git worktree add --detach ../trial pp/dev && cd ../trial
git merge --no-commit --no-ff origin/wp/base-factions; git diff --name-only --diff-filter=U
git merge --abort
# formatting state of main
cargo fmt --all -- --check
```
