# Plan: public playtest server with a feedback button

Goal: a handful of friends can open a URL, create or join games (with or without bots), play on
phone or desktop, and send feedback from inside the game. Not a goal: accounts, matchmaking,
scaling beyond a few concurrent games.

## 1. Where we are today (checked in the code)

- The server (`crates/ti4-server`, axum) listens on `HOST`/`PORT` (default `0.0.0.0:8080`), keeps
  games on disk under `TI4_DATA_DIR`, and speaks HTTP (`/api/...`) plus a WebSocket per game
  (`/ws/games/{id}`).
- It does **not** serve the web client. The root page tells you to run `npm run dev`. The phone
  instance is Vite dev on 13000 proxying to the backend on 18080, reached by an SSH tunnel.
- No TLS, no accounts. A seat is controlled by a per-seat token sent as the
  `x-ti4-player-session` header (stored in `player_sessions.json`; tokens are redacted in logs).
  Per-viewer redaction of hidden information is implemented and tested (`projection_redaction.rs`).
- CORS is `allow_origin(Any)`. No rate limiting on HTTP (only a preview refusal on the WebSocket).
  JSON bodies are capped at 8 KiB. `/api/dev/scenarios` and `/api/dev/scenarios/launch` are
  routed in production builds. Anyone who can reach the server can create games and bots.
- The phone instance runs a **debug** build; that is slow. The box also runs the nightly sweep and
  agent builds (8 CPUs, 15 GB RAM), so a public instance must not share its target dirs or
  starve under load.

## 2. Recommended architecture (the server is on Hetzner)

The machine already has a public IPv4/IPv6 address, so no tunnel is needed:

```
friend's browser --HTTPS/WSS--> Caddy on the Hetzner box (ports 80/443, automatic Let's Encrypt)
                                   --> 127.0.0.1:PORT  ti4-server (release build)
                                          serves web/dist + /api + /ws
```

- **Domain:** any domain or subdomain with an A/AAAA record to the server IP. With no domain yet,
  `<ip-with-dashes>.sslip.io` also gets a real Let's Encrypt certificate from Caddy.
- **Firewall:** a Hetzner Cloud Firewall that allows only 22 (ideally restricted to your IP),
  80 and 443. The ti4-server itself stays bound to 127.0.0.1; only Caddy is public. The phone
  instance ports (18080/13000) stay closed to the internet.
- **Optional Cloudflare in front** (proxied DNS) for free DDoS filtering and rate-limit rules;
  then trust `CF-Connecting-IP` only when the request comes from Cloudflare's ranges.
- **Isolation:** this box also runs the nightly sweep and agent builds (8 CPUs, 15 GB RAM). Either
  run the public instance as its own user with a systemd slice (higher CPU weight, `MemoryMax`),
  or, cleaner, order a small separate Hetzner Cloud server (about 4-6 EUR/month) just for the
  playtest so a runaway build or the nightly sweep can never take the game down, and so the box
  with the repo, credentials and agent tooling is not exposed at all. I recommend the separate
  server.
- Harden SSH (key-only, no root password, fail2ban or Hetzner firewall IP allow-list).

## 3. Work items

### A. Serve the client from the server (small, ~0.5 day)
- Add `TI4_WEB_DIR` (default unset). When set, serve that directory with an SPA fallback
  (`index.html` for non-API paths) via `tower_http::services::ServeDir`. One origin, one port,
  no Vite proxy, no CORS needed.
- Build with `cd web && npm run build` (runs content verification + tsc + vite build).
- Release build of the server (`cargo build --release -p ti4-server --bin server`); never expose
  the debug build.

### B. Lock the server down (medium, ~1-2 days)
1. Gate `/api/dev/*` behind an env flag (`TI4_ENABLE_DEV=1`, off in the public instance).
2. CORS: allow only the public origin (or none, since same-origin).
3. Rate limits per client IP (use the `CF-Connecting-IP` header only when the request really
   comes through the proxy): game create, lobby join, WebSocket connect, batch submissions,
   feedback. Use `tower_governor` or a small token bucket in the registry.
4. Caps: max concurrent games, max lobbies per IP, max players per game (existing 8), max active
   bots (`TI4_MAX_ACTIVE_BOTS` exists), WebSocket max message size and idle timeout, disk quota
   for the data dir.
5. Access control for a closed playtest: a shared invite code. Simplest: Cloudflare Access
   (email allow-list or one-time PIN) if Cloudflare fronts the site, or Caddy `basicauth` with one shared password, zero code. Alternative: an
   `TI4_INVITE_CODE` env checked on game create/join, entered once and kept in localStorage.
6. Review hostile-input paths that were written for a trusted client: splice/turn-redo
   endpoints (`/history`, `/turn-redo`) and the replay endpoint (CPU heavy; restrict to the seat's
   own game and rate-limit), the 8 KiB body cap vs. the feedback endpoint (separate limit).
7. Confirm no hidden information leaks to spectators or other seats (the existing redaction tests
   are the baseline; add a test that an unauthenticated viewer gets spectator projection only).
8. Logging: no seat tokens, no IPs in game logs; rotate logs.

### C. Deployment and operations (small-medium, ~1 day)
- Separate Linux user or at least a separate directory tree, systemd unit with
  `Restart=on-failure`, `MemoryMax`, `TasksMax`, `Nice`, `LimitNOFILE`.
- Own data dir (`/var/lib/ti4-public`), nightly backup (`tar` of the data dir to another disk),
  retention.
- Deploy script: pull `main`, build web + release server, restart; keep the previous binary for
  rollback. Show the git commit and protocol version on the client footer so feedback names the
  build. (Games persist across restarts; clients reconnect. A protocol bump needs a page reload.)
- Health check (`/health` exists) polled by an external uptime monitor.
- Capacity: measure memory per game and replay time on the release build before inviting people;
  a restart replays games (about 60 s of start-up with many games on the debug build).
- Keep the nightly sweep and agent builds off the public instance's CPU (cgroup weights or run
  the public instance with higher priority).
- Bots: the rule bot is in-process Rust and needs nothing extra. The ML advisor/bot-agent path
  (`TI4_BOT_PASSWORD`, `TI4_ADVISOR_URL`, `TI4_BOT_AGENT_BIN`) needs libtorch; leave it disabled.

### D. Feedback button (medium, ~1.5-2 days)
UI (web):
- A floating "Feedback" button, always reachable on phone (44 px target, clear of the bottom
  action bar and the safe area), also in the phone top menu and the lobby.
- Opens a sheet: category (Bug / Confusing / Idea / Other), free text, optional
  "include my game state" checkbox (default on), optional screenshot upload or paste (cap 1 MB).
  A "Report this decision" shortcut on the decision panel that pre-fills the pending decision.
- Auto-attached context: build commit + protocol version, game id, the viewer's seat and faction,
  round/phase, pending decision subtype, prompt text and offered option ids, the last ~20 public
  events, viewport size, user agent, timestamp. Only data the viewer is allowed to see
  (use the viewer-projected snapshot, never the full state).
- Confirmation toast, offline queue (retry on reconnect), disabled while submitting.
Server:
- `POST /api/feedback` with its own body limit (e.g. 256 KB; screenshots as base64 or a second
  upload route), rate-limited per IP (e.g. 10/hour), validation, then write one JSON file per
  report under `TI4_DATA_DIR/feedback/YYYY-MM-DD/` with an id; reply with the id.
- Notification so you actually see it: a Discord or Slack webhook (best for friends), or email,
  or auto-creating a GitHub issue with a labelled template (needs a token with issue scope).
- A small admin view (`GET /api/feedback`, password-protected, or just a CLI command that lists
  and prints reports) and a "replay link" so a report with a game id can be opened in the
  history/replay viewer.
Tests and rule from CLAUDE.md: vitest for the form and context builder, server tests for
validation, size limits, rate limiting and the no-hidden-info guarantee, plus a screenshot folder
and private Artifact for the new UI.

### E. Onboarding for friends (small, ~0.5 day)
- One-page "How to play / known limits" (phone vs desktop, what bots do, expect bugs, how to
  send feedback, how long games take).
- Short privacy note on the feedback form (what is sent, stored where, deleted on request).
- Disclaimer: unofficial fan project; Twilight Imperium is owned by its publisher. Check that the
  content shipped in the public build (card text, art) is acceptable to publish, because the
  repo and the site are public.

## 4. Order of work and go/no-go

| Step | Content | Effort |
|---|---|---|
| 1 | A: serve web from the server, release build, run locally | 0.5 d |
| 2 | B1-B5: dev routes off, CORS, rate limits, caps, invite gate | 1-2 d |
| 3 | C: systemd, data dir, backup, deploy script, build id on the client | 1 d |
| 4 | First friend test on the public URL (Caddy + sslip.io or your domain; feedback via chat) | 0.5 d |
| 5 | D: feedback button + endpoint + notification | 1.5-2 d |
| 6 | B6-B8 hardening review, load check with 3-4 concurrent games | 1 d |
| 7 | E: friends' guide, then invite | 0.5 d |

Go/no-go before inviting anyone: release build running as a service, dev routes off, rate limits
on, invite gate on, backup working, feedback reachable, one full 4-player game played through the
public URL on a phone. Total about 6-8 working days of agent time; step 4 gives a first playable
link after about 2-3 days.

## 5. Risks

- A server written for a trusted client may have costly endpoints (replay, turn redo, splice
  preview) that one user can spam; rate limits and per-game locking need checking under load.
- Stored games from older builds may not replay after updates (known); for a public playtest
  decide the policy: a version bump may reset games, tell testers.
- Shared machine: the nightly sweep and agent builds can starve the server; separate cgroup or a
  separate small VPS removes this risk entirely.
- Caddy and Cloudflare both pass WebSockets, but long idle connections need heartbeats
  (the lobby heartbeat exists; verify the game socket pings).
- Legal/IP: public hosting of a Twilight Imperium implementation, card text and map art.

## 6. Decisions needed from you

1. Hosting on Hetzner: this box (isolated by systemd slice) vs. a separate small Hetzner server (recommended), and which domain/subdomain to use.
2. Access: closed invite (recommended) vs. open link.
3. Feedback delivery: Discord webhook, email, or GitHub issues.
4. Whether to keep the ML advisor/bots disabled (recommended) and the rule bot as the only AI.
5. Policy for resetting games across incompatible updates.
