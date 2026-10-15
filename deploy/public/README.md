# Public playtest: operator guide

What this sets up: the TI4 server (release build, serving the web client itself) behind Caddy with
automatic HTTPS, on this Hetzner box, as its own unix user in its own systemd slice. Access is one
shared password. Feedback lands as JSON files on disk. The dev routes are off.

Nothing in the repo installs anything. Every command below is run by you.

```
browser --HTTPS/WSS--> Caddy :443 --> 127.0.0.1:18200  ti4-server (TI4_PUBLIC=1)
                                        serves /  (web/dist)   /api   /ws   /health
```

## 0. Decide the address

* With a domain: create an A (and AAAA) record `play.example.org` -> the server's public IP.
* Without a domain: use `<ip-with-dashes>.sslip.io`, for 203.0.113.7 that is `203-0-113-7.sslip.io`.
  It resolves to that IP and gets a real Let's Encrypt certificate.

Put the name into `deploy/public/Caddyfile` (site address) and `TI4_PUBLIC_ORIGIN` in the env file.

## 1. Hetzner Cloud Firewall (in the Hetzner console, applied to the server)

Inbound rules only:

| Protocol | Port | Source |
|---|---|---|
| TCP | 22 | your own IP (`x.x.x.x/32`; add a second entry if it changes) |
| TCP | 80 | any (needed for the Let's Encrypt challenge and the http->https redirect) |
| TCP | 443 | any |
| UDP | 443 | any (optional, HTTP/3) |

Everything else is dropped, in particular 18080/13000 (phone instance), 8080 and 18200 (this
server). The server also listens on `127.0.0.1` only, so even without the firewall it is not
reachable directly. Outbound: leave open.

## 2. SSH hardening and fail2ban

* `/etc/ssh/sshd_config.d/10-hardening.conf`:
  `PasswordAuthentication no`, `PermitRootLogin prohibit-password`, `KbdInteractiveAuthentication no`;
  then `sudo systemctl reload ssh`. Test a second session before closing the first.
* `sudo apt install fail2ban` (default sshd jail is enough). Optional: a Caddy jail on 401/429.
* The firewall rule for port 22 restricted to your IP is the strongest of these.

## 3. Install Caddy (once)

Follow https://caddyserver.com/docs/install#debian-ubuntu-raspbian, then:

```
sudo mkdir -p /var/log/caddy && sudo chown caddy:caddy /var/log/caddy
sudo cp deploy/public/Caddyfile /etc/caddy/Caddyfile     # after editing the domain
sudo systemctl reload caddy
```

## 4. User, directories, config

```
sudo useradd --system --home /var/lib/ti4-public --shell /usr/sbin/nologin ti4
sudo install -d -o ti4 -g ti4 -m 750 /var/lib/ti4-public /var/backups/ti4-public
sudo install -d -m 755 /opt/ti4-public /opt/ti4-public/releases
sudo install -d -m 755 /etc/ti4-public
sudo install -m 640 -o root -g ti4 deploy/public/env.example /etc/ti4-public/env
sudo editor /etc/ti4-public/env          # set TI4_ACCESS_PASSWORD, TI4_PUBLIC_ORIGIN, TI4_COOKIE_SECRET
sudo cp deploy/public/ti4-public.slice deploy/public/ti4-public.service /etc/systemd/system/
sudo systemctl daemon-reload
```

`TI4_COOKIE_SECRET` (`openssl rand -hex 32`) keeps everybody logged in across restarts; without
it the secret is random per start. The password is the only thing guarding the site: pick one
you can send over chat and change it (then `systemctl restart ti4-public`) to lock everybody out.

## 5. First release

```
deploy/public/deploy.sh
sudo systemctl enable --now ti4-public     # first time only; deploy.sh restarts it afterwards
```

`deploy.sh` builds `origin/main` (or `--ref <ref>`) in a temporary git worktree (it never switches
the checkout you run it from), runs `npm ci && npm run build` (including the wasm engine for
offline play), `cargo build --release -p ti4-server --bin server` (own target dir under `~/.cache`,
never the agents' target dirs), runs the persistence pre-flight (section "How to deploy safely"),
installs `server` + `dist/` to `/opt/ti4-public/releases/<timestamp>-<commit>`, swaps the `current`
symlink atomically, restarts the service, polls `/health` and re-checks the saved games the live
service recovered. If the health check or that check fails it points `current` back at the
previous release by itself. Manual rollback: `deploy/public/deploy.sh rollback`.
The last 4 releases are kept. The commit is baked into the binary and shown in `/health` and in
the page footer.

Check: `curl https://play.example.org/health` -> `{"status":"ok","protocol_version":...,"commit":"...","public":true}`.
Open the site on your phone, enter the password, create a game, add a Rule bot.

Updates can break running games (alpha): a protocol bump needs a page reload, an engine change can
stop an old game from replaying. Tell the testers when you deploy.

### How to deploy safely

```
deploy/public/deploy.sh --ref my-branch --preflight-only   # build + load every saved game on a COPY, stop
deploy/public/deploy.sh --ref my-branch                    # the real thing
deploy/public/deploy.sh --ref my-branch --allow-failed game_a36f40db...   # accept losing this game
deploy/public/check-headers.sh --bin <server>              # CSP / wasm / cache / compression, on loopback
```

* The pre-flight (`recovery-check.sh preflight`) copies the live data dir (without `feedback/`),
  starts the NEW server binary on a free loopback port against the copy and reads
  `Durable storage: recovered N game(s); F failed`. N plus F must equal the number of `game_*`
  directories in the live dir and every failed id must be named with `--allow-failed`; otherwise the
  deploy stops before anything is installed or restarted. Lesson: a checksum change once made all 22
  saved games unrecoverable and it was noticed only after the restart. (Failed games keep their
  files on disk; the server never deletes them.)
* After the restart the script reads the live service's own recovery line from the journal. Any
  failure the pre-flight did not allow rolls back to the previous release automatically.
* A deliberate engine or format change that makes a game unloadable (for example the pickup
  dedupe change and `game_a36f40db...`) needs `--allow-failed <id>` for exactly that game.
* If the ref contains offline play, `dist/` must contain `ti4_host-*.wasm`; the script writes `.br`
  and `.gz` next to it and the server serves them (Caddy has no built-in brotli). A build with
  `web/src/offline` and no wasm prints a warning.
* After editing the Caddyfile run `check-headers.sh` (uses `caddy` if installed) before
  `sudo cp ... /etc/caddy/Caddyfile`.
* Never run it with a dirty idea of "just pull in the shared checkout": the shared checkout is used
  by the nightly sweep; the script no longer touches it.
* See `docs/OFFLINE.md` for what offline play needs from the server.

## 6. Backups

```
sudo crontab -e
17 3 * * *  TI4_DATA_DIR=/var/lib/ti4-public /path/to/repo/deploy/public/backup.sh
```

Tar of the data dir (games, feedback) into `/var/backups/ti4-public`, 14 days kept
(`TI4_BACKUP_KEEP_DAYS`). Put `TI4_BACKUP_DIR` on another disk or a Hetzner volume if you can.
Restore: stop the service, untar into `/var/lib`, `chown -R ti4:ti4`, start.

## 7. Reading feedback

Each report is one file: `/var/lib/ti4-public/feedback/YYYY-MM-DD/<id>.json` (category, text,
build commit, game id, seat, faction, round/phase, the open decision and its option ids, the last
20 public events, viewport, browser). No IP addresses and no seat tokens are stored.

```
tools/public/list_feedback.sh            # 20 newest, readable
tools/public/list_feedback.sh -n 5 -v    # with the full context
/opt/ti4-public/current/server feedback-list --data-dir /var/lib/ti4-public --limit 50
```

A report with a game id can be looked at on the server: the game is in `/var/lib/ti4-public/<game_id>/`.
Nobody is notified; check the list when you play with friends or once a day.

## 8. What protects the box

* Firewall: only 22 (your IP), 80, 443. Server on 127.0.0.1.
* One shared password for every `/api` and `/ws` call (HttpOnly, SameSite=Lax, Secure cookie, 30 days,
  signed; 8 wrong attempts then one per 30 s per IP).
* Per-IP rate limits on game create (6, then 1 per 10 min), lobby join/bots, websocket connects,
  batch submits, replay/history/redo, feedback (10/h), password attempts. `TI4_TRUST_PROXY=1` makes
  the server use Caddy's `X-Forwarded-For` (Caddy overwrites whatever the client sent).
* Caps: `TI4_MAX_GAMES` (200), `TI4_MAX_LOBBIES_PER_IP` (4 unfinished games per address),
  `TI4_MAX_DATA_BYTES` (2 GiB), websocket messages 4 KiB, idle sockets dropped after 5 min,
  API bodies 8 KiB (feedback 256 KB).
* systemd: own user, own slice (`CPUWeight=400`, `MemoryMax=3G`, `TasksMax=512`), read-only
  filesystem except the data dir, no capabilities.
* `/api/dev/*` routes and `start_preset` are refused in public mode.

## 9. Not covered

* Rate limits are per process and in memory; they reset on restart.
* The shared password is one secret for everyone; no accounts.
* Cloudflare in front is optional. If you use it, remove `header_up -CF-Connecting-IP` and
  restrict ports 80/443 to Cloudflare's ranges, otherwise anyone can fake the header.
* Check that the card text and map art in the build are fine to publish before sharing the link
  widely.
