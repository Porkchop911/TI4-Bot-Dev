# Integrating ProbablyPlayable's `main` (2026-10-15)

Investigation on branch `pp-main-trial` (a trial merge of `probablyplayable/main` into c3605f1b;
throwaway, nothing pushed or deployed). Numbers below come from that trial.

## 1. Relation

- `probablyplayable/main` = `probablyplayable/dev` = **ded0b4d5** (main was fast-forwarded to dev; there
  is no separate "main" content). It is a merge of their `dev` with `niko/main`.
- Merge base with our line (c3605f1b) = **6ab90e1a**, the `dev` tip we merged on 2026-10-15
  (`docs/PP_DEV_INTEGRATION.md`). Ours is 126 commits ahead, theirs 92.
- So "main" is **not** a new UI branch. It is what PP did on `dev` after our merge (2026-10-14
  evening to 2026-10-15 17:09 Berlin): 92 commits, 925 files (+79.7k / -31.8k lines, of which about
  22 % is generated TypeScript wire types).
- No change to `deploy/`, `CLAUDE.md`, `AGENTS.md`, `Cargo.toml`, `rust-toolchain.toml`, CI or
  `.cargo/config.toml` between 6ab90e1a and ded0b4d5 (checked), except small CI/script items inside
  the commits below. The deploy script and the Caddy CSP of c3605f1b still apply.

## 2. What is in their main

Three bundles:

**A. The "unification" = `web-shared/` (steps 1 to 22 of `web2/plans/2026-10-14-SHARED_TRANSPORT_PLAN.md`,
status done) and the host dedup (steps 6.1 to 6.10 of `2026-10-14-DEDUPLICATE_HOSTS_PLAN.md`).**
It does **not** merge `web/` and `web2/`. Both UIs stay, with their own state models and screens.
What is shared is the layer below, a new directory `web-shared/` (imported as `@ti4/shared/*`
through a vite/tsconfig alias in both apps; no package of its own):

| shared module | what moved there |
| --- | --- |
| `engine/` | the wasm `Host` driver, `HostRunner`, Worker, `hasJspi`, odds queries (was `web/src/offline/local/{hostApi,hostRunner,hostWorker,jspi,hostProtocol}.ts`) |
| `link/` | `GameLink` with a socket link and an engine link; web's `LocalTransport` and web2's single player both run on it |
| `net/` | socket/reconnect, `api.ts` (all HTTP paths, session header, lobby posts) |
| `storage/` | ONE IndexedDB save store (`ti4-offline`, DB version 2) with slots, factions, list and limits, used by both apps. **HostSave format 2**: the game and every seat's decisions. Old saves are not read (web format 1, web2 formats 1 to 3): the page offers to delete them |
| `wire/` | TS types **generated from Rust** by ts-rs (`cargo wire-web`; `cargo t-web` fails when stale) plus `decode.ts` (server message validation) |
| `content/` | `loadCatalog` from the engine (`ti4_catalog` / `GET /api/catalog`), `humanizeId`, faction ids. web's generated `generatedContentManifest.ts` (9858 lines) and its `verify:content` step are gone |
| `identity/` | nickname rules and key (`ti4.nickname`) |
| `scripts/build-wasm.mjs` | the one wasm build for both apps (web's `TI4_*` flags) |
| `test/server.ts` | `spawnTestServer()` for the Playwright tests of both apps; `web/e2e/offlineServer.ts` is deleted |

Decisions in their plan that hit `web/`: old offline saves are dropped; the combat-odds advisor
client (`web/src/services/advisorService.ts`, `advisorTypes.ts`) is deleted and odds come from the
engine (`spaceOdds`/`groundOdds`); the content manifest is deleted; the wire types are generated.

**B. Host dedup in Rust.** The planning/undo/script/staged/preview logic of the server moved into
`ti4-host` (`planning/{script,undo}.rs`, `staged/`, `preview.rs`, scenarios, `view/records.rs`);
`ti4-server` lost about 5k lines (runner, scenarios, session/planning). wasm `HostSession` has a typed
`HostSave` (`SAVE_FORMAT = 2`), `staged` per seat, `table`/`local`/`setup` as `GameRequest`.
Deterministic seating ("the lobby rows are the seats", "Shuffle seats", bots seeded by seat
**place**, not seat id: `ti4-engine/src/seating.rs`, `ti4-rule-bot/src/random.rs`).
Engine fixes (Naalu Z'eu agent, Antimass Deflectors, TE Warfare free action, Muaat war sun,
hyperlane tiles are not systems, Chaos Mapping duplicate, unit upgrades, Argent, Luminous).

**C. niko/main training work** (rule bot vs MLP, wide faction roster, card tags, no-progress guard,
`RuleBotDecider` in `ti4-rule-bot`, MLP inference without libtorch with SIMD in `ti4-host`,
model download in web2). Mostly `crates/ti4-mlp`, `ti4-review`, `ti4-policy`, scripts, plans; it
touches `ti4-rule-bot` (overlaps bot v2) and enlarges the wasm.

### web2 and web after this

- web2 gets the same single-player engine path as web (Worker, shared save list, "open imported and
  web games"), the shared nickname/HTTP API, generated wire types. web2's bespoke C ABI
  (`ti4_play*`, `wasmTransport` main thread) is replaced by `HostSession`.
- web gets: generated wire types, shared decoder, catalog from the engine, shared save store
  (format 2), shared lobby/API paths, `othersDeciding` turn-status prop in `GameShell` (PP touches
  web here), engine-side odds. It does **not** get web2's features (staged seat sync, lobby, reaction
  settings UI, turn counter): those stay web2-only. Nothing moves from web into web2.
- Both apps open each other's single-player saves in the browser (same IndexedDB), which needs both
  on one origin (phase 4 of `2026-10-14-SAME_SEAT_TABS_AND_ONE_DOMAIN_PLAN.md`, not started).

### What happens to our features

| our feature | outcome in their main |
| --- | --- |
| offline mode (`web/src/offline/**`) | the engine driver, jspi, worker and the whole `offline/storage/` are replaced by `web-shared`; `LocalTransport`/`launcher`/`session`/`register` are adapted. Auto-merged except `register.ts` |
| offline map select | **collides**: `useMapCatalog.ts`, `register.ts`, `mapSource.ts` (imports the deleted `hostProtocol.ts`), `OfflineMapPicker`, `MapChoiceDto` (our client type vs generated `MapChoiceView`) |
| notification log, swipe toasts | untouched by them; only its e2e test sits next to theirs in `offline_local.spec.ts` (adjacent edits) |
| undo / browser Back button | untouched; our e2e kept; `web-shared` has the same undo track now in the host |
| hyperlane art | untouched in src; `hyperlaneTrace.test.tsx` needs the new `MapPreview` type |
| anomaly art | untouched |
| action card UI, reaction previews, active effects strip, agenda peek | **collides** with the generated wire types: `EffectPreview`, `ActiveEffectView`, `AgendaPeekCard` have no ts-rs derive on our line, so they are missing from `web-shared/src/wire/generated`; our `decodeDecisionTrigger/decodeEffectPreviews/decodeActiveEffects` lived in the deleted `protocol/decode.ts`; `client.ts`, `reactionModel.ts`, `SpaceCombatOverlay.tsx` read them |
| bot v2 | **collides** in `ti4-rule-bot` (`answers` + `decider` modules both needed; RNG seeded by seat id vs seat place), `ti4-wasm` `Start.bot_version`/save |
| screenshots tooling | works; 22 small type-rename diffs in `web/e2e/screenshots/_shared/*`, `web/src/dev/*` galleries (auto-merged). Not re-run in the trial |
| `CLAUDE.md` / `AGENTS.md` | untouched upstream since dev; no conflict. New `web-shared/AGENTS.md` and `web2/AGENTS.md` bring their own rules (ownership: both maintainers must agree on a stored format, `GameLink` shape, generated wire types, removing exports, new deps) |

## 3. Trial merge (`pp-main-trial`, from c3605f1b)

`git merge probablyplayable/main` reports **18 conflicted paths, 26 hunks in total** (17 content, 1
modify/delete). Everything else auto-merged (about 900 files).

| cluster | files | assessment |
| --- | --- | --- |
| bot v2 vs host save | `ti4-rule-bot/src/lib.rs`, `ti4-wasm/src/{host_api,host_session,host_setup}.rs` | Easy to resolve, but **semantic**: (a) `mod answers` + `mod decider` both kept; (b) our `choose_explained` took a seat id, their `decision_rng` takes the seat place: I changed our signature to `usize` place and fixed `ti4-selfplay` (our calibration runs on a different RNG stream afterwards); (c) their `HostSave` is typed, so `bot_version` became a field of `HostSave` and `bot_version()` reads `identity.game.setup`; **a `table`/`local` game has no `setup`, so its bot version is not recorded yet** (open item); (d) `Start.bot_version` and `staged` both kept; (e) `examples/replay_save.rs` and `ti4-selfplay` needed one-line fixes |
| generated wire vs our protocol | `web/src/protocol/{types,decode,client}.ts`, `presentation/reactionModel.ts` | `types.ts` becomes a re-export of the generated types (I took theirs and added `*Dto` aliases). Our extra types need a ts-rs derive in Rust (`EffectPreview`, `EffectExpiry`, `ActiveEffectView`, `AgendaPeekCard`: 4 attributes, then `cargo wire-web`). Our tolerant decoders for previews/active effects were re-added in `web/src/protocol/decode.ts` (they could move to `web-shared/wire/decode.ts`; additive, no agreement needed). `ChoiceOption.description` does not exist in the engine's type: our UI code showing it is dead and was dropped (the hunk in `PendingChoiceModal.tsx`) |
| offline map select | `useMapCatalog.ts`, `offline/local/register.ts`, `mockLauncher.ts`, `mapSource.ts` | Restored our `useMapCatalog` with their API paths and imports; `mapSource.ts` now imports the shared `engine/protocol.ts` (`FromWorker`, `HostPort`). Needs a real look: the shared Worker protocol must offer our `query` message, and the shared save format has no `map` in `setup` |
| unit pick / turn bar | `UnitPickParts.tsx`, `unitPick.ts`, `GameShell.tsx`, `App.tsx` | Ours kept for the unit pick (board argument); `GameShell` keeps both props (`over` ours, `othersDeciding` theirs) |
| tests | `web/e2e/offline_local.spec.ts` (+ deleted `offlineServer.ts`) | Both sides kept (imports merged; they use `spawnTestServer`). Our tests rely on `storedSetup`, which reads the old save shape |

### Does it build (all in a private target dir, no libtorch crates)

- `cargo l-web` (clippy, all targets, game crates): **clean** after the fixes above.
- `cargo t-web`: **4048 passed, 0 failed, 15 ignored** (wire-currency test included; the engine did not compile with the ts feature before
  I added the four ts-rs derives).
- `cargo build -p ti4-wasm --profile wasm-release` + `scripts/wasm-size-gate.mjs`: raw **6.96 MB**
  (budget 7.00), gzip **1.74 MB** (1.80), brotli **1.21 MB** (1.30). Headroom is 0.04 MB raw, 0.06 MB
  gzip. Before the merge (dev integration) it was 6.84 / 1.69.
- `web`: `tsc` clean for non-test sources. 74 type errors remain **in test files only**: the generated
  types are strict (`ChoiceOption.payload` is required, `DecisionContext` has required fields,
  `PlacedUnitView` needs `planet`/`galvanized`), and our fixtures in about 17 test files are looser.
  Mechanical fixture work (about 0.5 day), not a design problem.
- `web` vitest: **2226 passed, 3 skipped**; one suite (`launcher.test.ts`) cannot run here only because
  `fake-indexeddb` is missing from the node_modules I borrowed (it is in `package.json`). The count
  is lower than 2273 because the old `offline/storage` tests were deleted upstream in favour of
  `web-shared/src/storage` tests.
- `vite build` of web: ok (main chunk 1.11 MB, 321 KB gzip).
- Offline Playwright (`test:e2e:offline`, production build with the merged wasm): **10 of 11 pass**,
  including the notification log, Back button, undo, export/import, "older save is not opened" and
  feedback tests. The one failure is the map-picker test: `storedSetup` reads the save's old
  `setup` field, which format 2 stores under `game`.
- Not run: web2 typecheck/vitest/Playwright (no node_modules available; their code is unchanged by the
  trial but the wasm API it calls changed in the two small ways above), web server e2e, the
  screenshots, `cargo` for libtorch crates (`ti4-mlp`, `ti4-review`, `ti4-policy`: niko's work,
  unreviewed here).

The trial is committed on `pp-main-trial` as a merge commit with these partial resolutions, to be
used as a reference, **not** to be merged as is (map select and bot version are half done).

## 4. Risks

1. **Saved games of the alpha (server side).** The server persistence is unchanged in format, but
   the engine now offers different decisions in places (Z'eu, Antimass, TE warfare, Muaat, hyperlane,
   Chaos Mapping), and seating/bot seeding changed (place instead of seat id). A recorded game whose
   log passes through such a decision can fail to replay on recovery. Mitigation exists:
   `deploy/public/recovery-check.sh preflight` runs the new binary against a copy of the live data
   before the switch (`deploy.sh` does this). Run it by hand on the alpha data first and decide per
   failed game (`--allow-failed`). Not run here (data is off limits).
2. **Browser saves (offline games).** Format 2 replaces format 1: every existing offline save of a
   tester becomes "older version, delete?". Accepted by PP's plan; for us it must be announced to
   the testers, and the exported file format changes too. Our `replay_save` tool and our
   `docs/OFFLINE.md` describe format 1.
3. **CSP / wasm serving.** The `web-shared` Worker is created from a path in `web-shared/src/engine/worker.ts`
   and the precompressed wasm is still produced by `deploy.sh` from `web/dist`. The live CSP
   (`script-src 'self' 'wasm-unsafe-eval'; worker-src 'self'`) should still hold, because the Worker is a
   same-origin bundle, but `check-headers.sh` and the offline CSP test must be re-run on the merged
   build. web2 on `/web2/` or a subdomain needs its own wasm path and the same headers (their
   phase 4, not started; `web2/` is still not in `deploy/`).
4. **Bundle budgets.** wasm gzip headroom is 0.06 MB after this merge, before bot v2's own growth and
   any MLP inference code that rides along from niko (SIMD inference is in `ti4-host`, which the wasm
   links). Expect to raise the budget in `scripts/wasm-size-gate.mjs` or feature-gate the MLP. JS main
   chunk: no budget script exists; the web chunk was already over 500 kB.
5. **Deploy script.** `deploy.sh` builds web's wasm with `web/scripts/build-wasm.mjs`; upstream
   moved the real logic to `web-shared/scripts/build-wasm.mjs` and kept web's script as a caller.
   `TI4_REQUIRE_WASM` and the `TI4_*` flags are kept (read the diff before the first deploy).
   `cargo wire-web` has to be run (and committed) whenever a wire type changes: CI fails otherwise.
6. **`CLAUDE.md` vs `AGENTS.md`.** No change upstream since the dev merge, so nothing to merge now. But
   `web-shared/AGENTS.md` makes changes to stored formats, `GameLink`, wire types and exports a
   two-maintainer matter. Our rules (screenshots for UI work, Berlin time, alpha feedback) stay in
   `CLAUDE.md`; consider telling PP about the screenshot requirement, since web2 UI work has none.
7. **Bot v2 calibration.** Moving to seat-place seeding changes which games the bot v2 benchmarks play;
   results are statistically comparable but not game-by-game reproducible with earlier runs.
8. **Type strictness.** Generated wire types make web's loose fixtures and some defensive reads
   fail typecheck; every later protocol change in Rust now needs `cargo wire-web`.

## 5. Recommended order of work

| # | step | effort |
| --- | --- | --- |
| 0 | Decide the open questions below (especially 1 to 3) | 15 min |
| 1 | Run `recovery-check.sh preflight` with a binary of their main against a copy of the alpha data (read only), list failed games | 1 h |
| 2 | Real merge of `probablyplayable/main` into a branch off our line: Rust side (rule-bot, wasm host/save/setup, ts-rs derives, `cargo wire-web`), record `bot_version` for `table`/`local` games, fix clippy/tests | 0.5 day |
| 3 | web side: types/decoders for our protocol additions (move them into `web-shared/wire/decode.ts` as additive), offline map select on the shared Worker protocol, `storedSetup` and the other save-shape helpers in e2e | 1 day |
| 4 | Test fixtures to the strict generated types (about 17 test files), vitest, `fake-indexeddb` install | 0.5 day |
| 5 | Offline Playwright, web server e2e, screenshots re-run (`npm run screenshots -- <letter>` for the folders that use `openMockedGame`), web2 gates (`typecheck`, `lint`, `test`, Playwright) against the merged wasm | 0.5 day |
| 6 | CI/size budget decision, `deploy.sh` dry run + `check-headers.sh` on a throwaway worktree build, `docs/OFFLINE.md`, `PP_DEV_INTEGRATION.md` follow-up, tell testers that offline saves reset | 0.5 day |
| 7 | Optional: bring web2 features into web (staged seat sync, combat odds on the movement screen, reaction settings) or serve web2 on the same domain (their phase 4) | separate decisions, days each |

Total for the integration itself (steps 1 to 6): about 3 days of agent work, plus the alpha data
check. Step 3 is the one that can grow (offline map select against the shared save format).

## 6. Decisions needed from Felix

1. **Take it now or not at all?** There is no "UI merge" in their main; it is the shared-layer
   refactor with a **breaking save format** (offline saves reset) and a **bigger wasm**. Merge it
   (recommended, otherwise every PP commit diverges further: 92 commits in one day) or pin the
   alpha at c3605f1b for a while?
2. **Timing relative to the alpha/nightly sweep.** The nightly sweep tests whatever is in the live
   line before 20:30 Berlin. A merge of this size should land right after a sweep, not just before
   one. When?
3. **Offline saves of testers**: OK to drop all existing offline saves (PP's decision) or must old
   saves be migrated (extra 1 to 2 days)?
4. **wasm size budget**: raise the gate (7.0 MB / 1.8 MB gzip) or gate the MLP/SIMD code out of the
   browser build?
5. **Bot seeding**: accept PP's seat-place seeding for bot v2 (needed for server/page parity), and
   re-baseline our bot benchmarks?
6. **Who owns `web-shared/`** on our side: accept the two-maintainer rule in `web-shared/AGENTS.md`
   (PP must agree to stored-format, `GameLink`, wire and export changes), or ask for a looser rule?
7. **web2**: still only `web2/` in the repo, not deployed. Serve it (phase 4: one domain, `/web2/`)
   in this integration, or later?
8. **Port web2 features into web** (staged seat sync, odds on the movement screen, reaction
   settings) after this: yes/no/when? (Same question as in the dev integration notes.)
