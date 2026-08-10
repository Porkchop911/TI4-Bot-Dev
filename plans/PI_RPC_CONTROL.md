# Pi RPC control

The migration Pi session is managed by `tools/pi_rpc_bridge.py`. It binds only to loopback and
requires the random token stored in the ignored `.pi-control/token` file.

Controller state:

- `.pi-control/controller.json` — controller/Pi process IDs, workspace, session, and port
- `.pi-control/token` — local bearer token; never commit or disclose it
- `.pi-control/events.jsonl` — ordered RPC responses and events
- `.pi-control/pi.stderr.log` — Pi diagnostics

Supported bridge calls:

- `GET /health` — controller and Pi status
- `GET /events?since=<sequence>` — ordered events retained in memory
- `POST /rpc` — one native Pi RPC command from Pi's installed `docs/rpc.md` protocol

Every request needs `X-Pi-Token`. Commands such as `prompt`, `steer`, `follow_up`, `abort`,
`get_state`, `get_session_stats`, and `compact` are forwarded unchanged.

PowerShell example:

```powershell
$token = (Get-Content .pi-control\token -Raw).Trim()
$headers = @{ 'X-Pi-Token' = $token }

Invoke-RestMethod -Headers $headers http://127.0.0.1:41873/health

$command = @{ id = 'state-1'; type = 'get_state' } | ConvertTo-Json -Compress
Invoke-RestMethod -Method Post -Headers $headers -ContentType application/json `
  -Body $command http://127.0.0.1:41873/rpc

Invoke-RestMethod -Headers $headers 'http://127.0.0.1:41873/events?since=0'
```

Only one Pi writer may own the session. Do not launch the interactive TUI or another RPC process
against the same session while this controller is running.
