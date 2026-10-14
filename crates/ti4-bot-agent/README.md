# ti4-bot-agent

`ti4-bot-agent` is a headless WebSocket client that receives an actor's
pending choice from `ti4-server`, asks an external `ti4-advisor` service to
evaluate it, and submits one of the legal options.

## Run unit tests

From the repository root:

```bash
cargo test -p ti4-bot-agent
```

## Run the real end-to-end test

The opt-in E2E test starts a local `ti4-server`, a real `ti4-advisor` loaded
from `examples/reviewer/checkpoint-473312`, two bot agents, and one scripted
seat. It requires the pinned CPU libtorch runtime.

On Linux or WSL, install the runtime once if it is not already present:

```bash
scripts/install_libtorch_cpu_linux.sh
```

Then, from the repository root, configure the runtime and run the ignored E2E
test explicitly:

```bash
export LIBTORCH="$PWD/out/libtorch-2.9.1-cpu-linux"
export LIBTORCH_BYPASS_VERSION_CHECK=1
export LD_LIBRARY_PATH="$LIBTORCH/lib:${LD_LIBRARY_PATH:-}"

cargo test -p ti4-bot-agent --features real-e2e --test real_e2e -- --ignored
```

The test is ignored by default because it requires both libtorch and the
committed checkpoint. Supplying `--features real-e2e` compiles it; `-- --ignored`
executes it.

## Run a bot agent

Start `ti4-server` and `ti4-advisor` first, then run a bot with a game ID and
an explicit nickname (1–64 UTF-8 bytes, trimmed, without control or format
characters). The nickname is public display text, not an advisor identity.
The bot joins the first open position, marks ready, and waits for the host to
start. If the lobby is full (or already running), it lists eligible
disconnected players and requires an interactive selection before takeover.
Its private session stays in process memory; restart after a crash requires
explicit takeover after the presence grace period.

```bash
cargo run -p ti4-bot-agent -- \
  --server ws://127.0.0.1:8080 \
  --game game_123 \
  --nickname "Table bot" \
  --advisor http://127.0.0.1:8081
```

Use `--sample-seed <u64>` to enable reproducible probability sampling.
Without it, the agent chooses the highest-probability option, preserving the
server choice's stable option order for ties.

## Server-managed self-service bot mode

`ti4-server` can also spawn and manage `ti4-bot-agent` processes directly on
behalf of lobby hosts via the web UI.

To enable this feature on `ti4-server`:

1. Build the `ti4-bot-agent` binary:
   ```bash
   cargo build -p ti4-bot-agent
   ```
2. Set the `TI4_BOT_PASSWORD` environment variable when starting `ti4-server`:
   ```bash
   TI4_BOT_PASSWORD="secret" cargo run -p ti4-server --bin server
   ```

Environment variables supported by `ti4-server`:
- `TI4_BOT_PASSWORD`: Required to enable self-service bot recruitment. If unset,
  bot recruitment endpoints and UI controls remain disabled.
- `TI4_ADVISOR_URL`: Address of the `ti4-advisor` service (defaults to `http://127.0.0.1:8081`).
- `TI4_BOT_AGENT_BIN`: Explicit path to the `ti4-bot-agent` executable (defaults to
  `./target/release/ti4-bot-agent` or `./target/debug/ti4-bot-agent`).
- `TI4_MAX_ACTIVE_BOTS`: Maximum concurrent child bot processes permitted (defaults to `6`).
