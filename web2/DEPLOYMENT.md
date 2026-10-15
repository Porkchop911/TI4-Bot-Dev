# web2: deployment of the dev host

How to bring the dev host of web2 up to date with a branch, and how to check it. web2 is served by
the Vite dev server, not by a build: a change of the TypeScript needs no build, but the engine of
the page, the server and the MLP models do.

## The setup

- Two tmux sessions on the host:
  - `server`, in the repository root: `TI4_MODEL_DIR=out/models target/debug/server`, on
    `127.0.0.1:8080`. The games are in `./data/games`.
  - `ui`, in `web2/`: `npm run dev`, Vite on `127.0.0.1:3100` (`vite.config.ts`). It proxies
    `/api` and `/ws` to the server. Caddy and Cloudflare are in front of it
    (`web2.beckermann-zibert.com`).
- The banner of the server names port 3000 and `web/`: that is the old app. web2 is on 3100.
- The page loads three things that are not in the repository, and each is built here:
  - `web2/src/session/ti4.wasm`: the engine of the page (`scripts/build-wasm.sh`).
  - `target/debug/server`: the server (`cargo b-web`).
  - `out/models/`: the MLP models that the server offers (`scripts/pack-models.sh`).

## Steps

Look at the machine first (`free -g`, `nproc`). With less than 4 GB, run every build alone, one
after the other. Each step below is one command at a time; none runs in the background.

1. Fetch and update the branch. The working tree must be clean (`git status`).
   ```
   git fetch origin <branch>
   git checkout <branch>        # or: git pull --ff-only origin <branch>
   ```
2. Read what changed for the deployment (`git diff --stat <old>..HEAD`). Look at:
   `.cargo/config.toml` (the aliases and their packages), `web2/package.json`,
   `web2/vite.config.ts`, `web2/tsconfig.json`, `web2/scripts/`, `scripts/`, `web-shared/`,
   `crates/ti4-server/src/bin/server.rs` (the environment variables), `models/index.json` and
   `deploy/`. If a change needs something that this file does not describe, stop and ask.
3. `cd web2 && npm install` (needed when `package.json` or `package-lock.json` changed; harmless
   otherwise).
4. The models, when `models/index.json`, `crates/ti4-infer/` or `scripts/pack-models.sh` changed,
   or `out/models/` is missing: `scripts/pack-models.sh`. It needs the bundles that the index names
   (`examples/reviewer/checkpoint-473312`) and checks their SHA-256; a mismatch stops it. Without the
   `brotli` CLI it writes no `.br`, and the server sends the plain file.
5. The engine of the page: `web2/scripts/build-wasm.sh` (the release build, not `--dev`). Minutes
   after a change of the engine or a `cargo clean`.
6. The server: `cargo b-web`. Use only the alias, never `cargo run -p ti4-server` or
   `cargo build -p ...`: another package set builds the engine again with other features.
7. Restart the server:
   ```
   tmux send-keys -t server C-c
   tmux send-keys -t server 'clear; TI4_MODEL_DIR=out/models target/debug/server' Enter
   ```
   Without `TI4_MODEL_DIR` the server starts with no models, and single player has no MLP bots.
8. Restart Vite when `vite.config.ts`, `package.json` or `tsconfig.json` changed (Vite does not
   always pick that up by itself). Otherwise its HMR is enough, also for a new `ti4.wasm`.
   ```
   tmux send-keys -t ui C-c
   tmux send-keys -t ui 'clear; npm run dev' Enter
   ```

`cargo clean` frees the disk (`target/` grows to more than 10 GB), and the running server and page
keep working. The next deployment then builds steps 4 to 6 from the start.

## Checks

1. The server: `tmux capture-pane -pt server | tail -30`.
   - `MLP model: mlp:<name> (<label>) sha256 … offered on avx2` for each model of the index.
   - `Durable storage: recovered N game(s); M failed`. Games with "Persistence checksum mismatch"
     are old saves that this engine refuses; their files are kept. A rise of M after a deployment is
     worth a note to the owner.
   - `curl -s http://127.0.0.1:8080/health`: `"status":"ok"` and the `protocol_version`.
2. Vite: `tmux capture-pane -pt ui | tail -5` says `ready` and `http://127.0.0.1:3100/`.
3. Through the proxy:
   ```
   curl -s http://127.0.0.1:3100/api/models
   curl -s -o /dev/null -w '%{http_code} %{size_download}\n' \
     http://127.0.0.1:3100/api/models/<sha256>.ti4m
   ```
   The list names each offered model; the file is 200 with the size that the list gives.
   `curl -s http://127.0.0.1:3100/src/session/ti4.wasm | wc -c` is the size of the new
   `web2/src/session/ti4.wasm`.
4. In a browser (headless Chromium with web2's `playwright` is enough), with the console errors
   collected:
   - `/` shows the start screen ("Single player", "Multiplayer").
   - `/?single&seed=5&players=4`: remove a rule bot, seat a model ("Reviewer 473312"), "Start
     game". The game reaches "Round 1 · Strategy phase", the seat of the model has "✦", and after
     the strategy card of the player the game reaches "Round 1 · Action phase". This checks the
     engine in the Worker, the model files and the MLP bot.
   - No console errors.
   - `/?online` opens a lobby, but it also creates a table on the server. Leave it out, or remove
     the table afterwards.

The test suites (`npm test`, `npm run e2e`, `cargo t-web`) are the gates of a change
(`docs/agents/testing.md`), not of a deployment. Say in the report which of them were not run.
