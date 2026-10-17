# Integration of ProbablyPlayable's `dev` (2026-10-15)

Branch `pp-dev-integration`, based on `consolidated-2026-10-15b` (2987c500), merged with
`probablyplayable/dev` (64 commits, 2026-10-13 to 2026-10-14; merge base cb0480fa). It is a real
merge commit, so later pushes of `dev` merge again without redoing this.

## Taken (whole)

- **`web2/`**, the second UI (React 19, own `AGENTS.md`, plans, e2e). It is a separate app next to
  `web/`: no file of `web/` is shared with it, so nothing of our UI is replaced. It brings the lobby
  (faction select, single player on the lobby screen, the table in the game), staged work of an
  online seat on the server (the same in every tab), reaction settings (offered, only on my turns,
  never), the turn counter, the board click that shows a system's contents, the phone panes and
  the odds of a space combat on the movement screen.
- **Rust refactor**: `ti4-view` is folded into `ti4-host` (`ti4_host::view`, `ti4_host::projection`,
  `ti4_host::protocol::view`), the `ti4-server` re-export layer is removed, one setup path for
  every host (`ti4_host::setup`), server tests live in `tests/it/`, feature sets are pinned
  (`.cargo/config.toml` aliases `l-web`, `t-web`, `b-web`).
- **Engine/calc**: `ti4-calc` space-combat odds, `reach_query`, never-offer detail
  (`ACTIVE_DETAIL`), small engine fixes.
- **Protocol**: staged-work messages (`protocol/staged.rs`), `waiting`/`step`/`switch_seat` of the
  wasm host. Additive; `web/` ignores what it does not know.
- **`ti4-wasm`**: PP's one-host rewrite (`table` and `local` requests, several seats in one page).
  The JSPI/bench scripts (`js/jspi.mjs`, `js/bench.mjs`) are gone upstream; the CI still uses
  `js/host.mjs`.

## Ported onto our line (conflicts resolved toward our architecture)

- `ti4-view/src/effects.rs` (active action-card effects) and the `agenda_peek` field (Insider
  Information) moved to `ti4-host/src/view/effects.rs` and `ti4-host/src/protocol/view.rs`; the
  projection paths follow. The wire format is unchanged.
- `ti4-engine/src/timing.rs`: both our `reaction_preview` previews and their `ACTIVE_DETAIL`
  (whose turn it is, for the "own turns only" setting) are attached to a reaction choice.
- Offline files in `web/src/offline/**`, `web/e2e/offline*`, `docs/OFFLINE.md`,
  `crates/ti4-wasm/**`: three-way merged with the offline-final commit (328cd3e3, which PP had
  already merged) as base, so only our later work (map picker, replay_save, sweep, CSP test) and
  their later work (`table`/`local` hosts, `hostRunner` tweak) were combined. No hunk conflicted.
- `CLAUDE.md` stays ours. Upstream replaced it by a symlink to `AGENTS.md`; the root `AGENTS.md`
  they added is kept, but it only points to per-directory notes.
- `Cargo.lock`: upstream's, then cargo re-resolved offline (no new crates from our side).

## Skipped / not ported

- Nothing was dropped. But **none of their features is ported into `web/`**: staged seats, combat
  odds, the reaction settings UI, the lobby and the turn counter exist only in `web2/`. `web/` is
  still our UI and the one deployed. Porting is a separate decision (below).
- `web2/` is not wired into `deploy/` (Caddy, `deploy.sh`) and has no screenshot folder under
  `web/e2e/screenshots/`; it has its own `npm run shots`. No new UI was written in this merge, so
  there is no new screenshot artifact.

## Verification

- `cargo l-web` (clippy, all targets, 8 game crates): clean. `cargo t-web`: all pass.
  `cargo fmt --all` applied (7 files were already unformatted on our line).
- `cargo check -p ti4-host/-p ti4-wasm --target wasm32`; wasm build 6.84 MB raw, 1.69 MB gzip
  (budget 7.0 MB / 1.8 MB: the gzip headroom is thin); `js/host.mjs` smoke ok.
- `web`: tsc clean, vitest 2273 passed. Offline Playwright (`test:e2e:offline`): 9/9.
- `web` server e2e against the merged server: same 3 failures as against a server built from
  2987c500 (`multiplayer_invariants` timestamp of "Game initialized", `empty_movement`,
  `technology_research` tarmann): pre-existing, not from this merge. The run hits the 300 s global
  timeout; only part of the suite ran.
- `web2`: tsc clean, vitest 339 passed; Playwright desktop + phone (with the engine copied to
  `src/session/ti4.wasm`): 62/65, the 3 timing-sensitive ones pass alone with a longer timeout.

## Open decisions

1. Which `web2` features to bring to `web/` (combat odds on the movement screen and the staged
   seat sync are the most valuable), or whether `web2` is meant to replace `web/`.
2. Whether and how `web2` is served (domain, Caddy, deploy.sh, CSP `wasm-unsafe-eval`); their plan:
   `web2/plans/2026-10-14-SAME_SEAT_TABS_AND_ONE_DOMAIN_PLAN.md`.
3. Root `AGENTS.md` vs our `CLAUDE.md` (kept both). `web2/CLAUDE.md` files are symlinks to
   `AGENTS.md`; fine on Linux.
4. Five `turn_redo` tests are `#[ignore]` upstream (pinned decision positions moved); re-pin them.
5. Bot v2 (`rulebot-opt1`) merges into the same line: `ti4-wasm`, `ti4-host/setup.rs` (`bots?`
   `smart` default) and the server `random_bot` files are the likely overlap.

## Follow-up: ProbablyPlayable `main` (ded0b4d5), branch `pp-main-integration`

Plan and analysis: `docs/PP_MAIN_INTEGRATION_PLAN.md`. Decisions taken (Felix, 2026-10-15): old offline
browser saves are dropped without migration (save format 2, the page offers deletion; tester note in
`docs/OFFLINE_SAVE_RESET_NOTE.md`); PP's seat-place bot seeding is accepted and the rule-bot golden set
was re-baselined; the two-maintainer rule of `web-shared/AGENTS.md` is accepted; web2 is not served and no
web2 features were ported into web.

What was finished on top of the trial merge:
- `bot_version` is carried by the session identity, so `table`/`local` games record it in `HostSave`
  (`crates/ti4-wasm/examples/bot_version_check.rs` covers setup, local and version-1 saves).
- Offline map select runs on the shared Worker `query` protocol; the map lives in `game.setup.map` of the
  format-2 save (the engine always fills it in). The e2e helper `storedSetup` reads `save.game.setup`.
- Test fixtures conform to the generated wire types (`web/src/test/wire.ts` helpers); `fake-indexeddb` runs.
- `docs/OFFLINE.md` describes save format 2.

Bot re-baseline (`tools/rulebot/baseline.json`, golden set of 200 games): every game hash differs (the RNG
stream follows the seat place now, not the seat id), as expected. Aggregates: 200/200 victories before and
after, no cutoffs, mean rounds 6.805 -> 6.885, mean decisions 2255.8 -> 2262.9, tied at top 1 -> 6 games,
seat-a win rate 0.200 -> 0.235 (within noise at n=200).

Alpha data: `deploy.sh --preflight-only` finds 8 of 22 saved games recoverable and 14 failing with
"Persistence checksum mismatch" (init.json) for BOTH this branch and the current line c3605f1b, so the
failures are not caused by this merge; they need their own decision before any deploy (`--allow-failed`).
