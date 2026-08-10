# Pi RPC control — low-token supervision

The migration Pi session is managed by `tools/pi_rpc_bridge.py`. It binds only to loopback and
requires the random token stored in the ignored `.pi-control/token` file.

Controller state:

- `.pi-control/controller.json` — controller/Pi process IDs, workspace, session, and port
- `.pi-control/token` — local bearer token; never commit or disclose it
- `.pi-control/events.jsonl` — compact lifecycle events; reasoning and streaming deltas are discarded
- `.pi-control/pi.stderr.log` — Pi diagnostics

Supported bridge calls:

- `GET /health` — controller/Pi health and current managed-task status
- `GET /summary` — one bounded task/checkpoint summary; the default monitoring endpoint
- `GET /events?since=<sequence>` — compact lifecycle events; use only for diagnosis
- `POST /task` — start one bounded work unit with watchdog limits
- `POST /rpc` — forward one native Pi RPC command from Pi's installed `docs/rpc.md` protocol

Every request needs `X-Pi-Token`. Native commands such as `steer`, `abort`, `get_state`,
`get_session_stats`, and `compact` remain available through `/rpc`, but normal work starts through
`/task`.

## Low-token operating policy

1. Send exactly one small, deterministic work package through `/task`.
2. Do not poll `/events` while Pi is generating. Pi's full transcript remains in its session file.
3. Check `/summary` only at a meaningful checkpoint: expected completion time, a watchdog abort,
   or when the user explicitly requests status.
4. On completion, review only `git status`, the final diff, package evidence, and test output.
5. Do not ingest token-by-token thinking, repeated command output, or the full transcript into a
   frontier-model context.
6. If the watchdog aborts a task, split it into a smaller package before retrying. Allow one retry.

Default task limits:

- absolute runtime: 600 seconds;
- first repository edit: 120 seconds;
- tool errors: one allowed; the second error aborts the task.

Callers can lower these limits per task. Increasing them should be exceptional and recorded in the
package evidence.

## PowerShell example

```powershell
$token = (Get-Content .pi-control\token -Raw).Trim()
$headers = @{ 'X-Pi-Token' = $token }

Invoke-RestMethod -Headers $headers http://127.0.0.1:41873/health

$task = @{
  id = 'M00-002-ledger-fix'
  prompt = 'Perform only the specified ledger correction and acceptance checks, then stop.'
  timeoutSeconds = 300
  noEditSeconds = 90
  maxToolErrors = 1
} | ConvertTo-Json
Invoke-RestMethod -Method Post -Headers $headers -ContentType application/json `
  -Body $task http://127.0.0.1:41873/task

Invoke-RestMethod -Headers $headers http://127.0.0.1:41873/summary
```

The summary contains elapsed time, task status, tool-call/error counts, bounded last-tool metadata,
abort reason, bounded final agent text, and baseline/current/final Git snapshots. It never contains
streaming reasoning deltas.

Only one Pi writer may own the session. Do not launch the interactive TUI or another RPC process
against the same session while this controller is running.
