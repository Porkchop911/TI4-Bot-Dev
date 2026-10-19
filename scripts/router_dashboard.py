#!/usr/bin/env python3
"""Terminal dashboard for the async trainer's router_status.json (stdlib only).

Usage: python scripts/router_dashboard.py <run dir> [--interval 2] [--once]
Every field of the status file is optional; missing values show as "—".
"""
import argparse, json, os, re, shutil, sys, time
from datetime import datetime, timezone
from pathlib import Path

DASH = "—"
R, B, D = "\x1b[0m", "\x1b[1m", "\x1b[2m"
RED, GRN, YEL, CYA, MAG = "\x1b[31m", "\x1b[32m", "\x1b[33m", "\x1b[36m", "\x1b[35m"
REV = "\x1b[7m"
ANSI = re.compile(r"\x1b\[[0-9;?]*[A-Za-z]")


def c(s, *codes):
    return "".join(codes) + str(s) + R


def vlen(s):
    return len(ANSI.sub("", s))


def pad(s, w):
    return s + " " * max(0, w - vlen(s))


def clip(s, w):
    """Cut a coloured line to w visible columns."""
    out, n, i = [], 0, 0
    while i < len(s) and n < w:
        m = ANSI.match(s, i)
        if m:
            out.append(m.group()); i = m.end()
        else:
            out.append(s[i]); n += 1; i += 1
    return "".join(out) + R


def num(v, fmt="{:.1f}"):
    return DASH if v is None else fmt.format(v)


def bar(v, total, w, color=None, full_red=False):
    if v is None or not total:
        return c("·" * w, D)
    f = max(0.0, min(1.0, v / total))
    n = int(round(f * w))
    col = RED if (full_red and f >= 1) else (color or GRN)
    return c("█" * n, col) + c("░" * (w - n), D)


def parse_ts(s):
    if not s:
        return None
    try:  # trim ns to us: fromisoformat accepts at most 6 fractional digits
        return datetime.fromisoformat(re.sub(r"(\.\d{6})\d+", r"\1", s).replace("Z", "+00:00"))
    except ValueError:
        return None


def dur(sec):
    if sec is None:
        return DASH
    sec = int(sec)
    d, sec = divmod(sec, 86400); h, sec = divmod(sec, 3600); m, s = divmod(sec, 60)
    return (f"{d}d" if d else "") + (f"{h}h" if d or h else "") + (f"{m:02d}m" if d or h else f"{m}m") + f"{s:02d}s"


def load(path):
    try:
        return json.loads(Path(path).read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return None


def faction_row(name, f, batch, target, w_bar=10):
    p = f.get("pending")
    upd = f.get("updates")
    left = None if p is None else max(0, batch - p)
    cells = [
        pad(name[:11], 11),
        bar(p, batch, w_bar, YEL if name == target else GRN),
        pad(f"{p if p is not None else DASH}/{batch}", 5),
        pad(DASH if left is None else str(left), 3),
        pad(DASH if upd is None else str(upd), 4),
        pad(num(f.get("mean_vp"), "{:.2f}"), 5),
        pad(num(f.get("last_update_mean_vp"), "{:.2f}"), 5),
        pad(num(None if f.get("ge6_share") is None else f["ge6_share"] * 100, "{:.0f}%"), 5),
    ]
    row = " ".join(cells)
    if name == target:
        return REV + row.replace(R, R + REV) + R
    if not upd:
        return c(ANSI.sub("", row), D)
    return row


FHEAD = " ".join([pad("faction", 11), pad("bucket", 10), pad("pend", 5), pad("nxt", 3),
                  pad("upd", 4), pad("vp", 5), pad("last", 5), pad("≥6", 5)])
FW = vlen(FHEAD)


def render(path, W, H):
    d = load(path)
    run = Path(path).parent.name
    if d is None:
        return [c(f" {run} ", REV, B), "", f"waiting for {path} ...", "",
                c(f"Ctrl+C to quit", D)]
    now = datetime.now(timezone.utc)
    upd = parse_ts(d.get("updated"))
    age = (now - upd).total_seconds() if upd else None
    st = parse_ts(d.get("started"))
    up = (now - st).total_seconds() if st else None
    age_s = DASH if age is None else (c(f"{age:.0f}s STALE", RED, B) if age > 10 else c(f"{age:.0f}s", GRN))
    batch = d.get("batch") or 60
    facs = d.get("factions") or {}
    target = d.get("target")
    gps, sgps = d.get("games_per_s"), d.get("seat_games_per_s")

    L = []
    L.append(clip(c(f" {run} ", REV, B) + f"  up {dur(up)}  data age {age_s}  updates {c(num(d.get('updates_done'), '{}'), B)}"
                  f"  games {c(num(d.get('games_finished'), '{}'), B)}", W))
    L.append("")
    blk = d.get("workers_blocked_pct")
    bcol = None if blk is None else GRN if blk < 5 else YEL if blk < 25 else RED
    blk_s = DASH if blk is None else c(f"{blk:.1f}%", bcol)
    L.append(f"{c('games/s', D)} {num(gps)}  {c('seat-games/s', D)} {num(sgps)}  "
             f"{c('workers', D)} {num(d.get('workers'), '{}')} (blocked {blk_s})")
    q, ql = d.get("queued_batches"), d.get("queue_limit")
    held, mh = d.get("held"), d.get("max_held")
    ram = d.get("ram_mb")
    L.append(f"{c('queue', D)} {num(q, '{}')}/{num(ql, '{}')} batches {bar(q, ql, 8, full_red=True)}  "
             f"{c('held', D)} {num(held, '{}')}/{num(mh, '{}')} {bar(held, mh, 12, full_red=True)}  "
             f"{c('RAM', D)} {num(None if ram is None else ram / 1024, '{:.1f}')} GB")
    g = d.get("gpu") or {}
    idle = g.get("optimiser_idle_pct")
    if blk is None or idle is None:
        verdict = c(DASH, D)
    elif blk >= 25 and idle < 25:
        verdict = c("GPU-bound", RED, B)
    elif idle >= 25 and blk < 25:
        verdict = c("CPU-bound", YEL, B)
    else:
        verdict = c("balanced", GRN, B)
    L.append(f"{c('GPU', D)} optimise {num(g.get('last_optimise_s'))}s  freeze {num(g.get('last_freeze_s'))}s  "
             f"publish {num(g.get('last_publish_s'))}s  optimiser idle {num(idle, '{:.0f}%')}  -> {verdict}")
    L.append("")

    # target panel
    if target:
        tf = facs.get(target, {})
        p = tf.get("pending")
        left = None if p is None else max(0, batch - p)
        eta = DASH if (left is None or not gps) else dur(left / gps)
        L.append(c(f" TARGET  {target.upper()} ", REV, B, YEL) + f"  bucket {bar(p, batch, 24, YEL)} "
                 f"{num(p, '{}')}/{batch}  games as target {num(d.get('target_since_games'), '{}')}  "
                 f"ETA to batch {c(eta, B)} (≈ games/s)")
    else:
        L.append(c(" TARGET ", REV, B) + f" {DASH}")
    L.append("")

    # footer + recent sizing
    recent = (d.get("recent") or [])[-10:]
    foot = [c(f"{path}  refresh {ARGS.interval:g}s  Ctrl+C to quit", D)]
    rec_n = len(recent)
    two = W >= 2 * FW + 3
    names = sorted(facs, key=lambda n: (-(facs[n].get("pending") or 0), n))
    need = (len(names) + 1) // 2 if two else len(names)
    fixed = len(L) + 1 + 1 + len(foot) + 2  # table title+head, blank, spare
    avail = max(4, H - fixed - (3 + rec_n if rec_n else 0))
    show = min(need, avail)
    rec_room = H - fixed - show
    if rec_n and rec_room < 3 + rec_n:
        rec_n = max(0, rec_room - 3)
        recent = recent[-rec_n:] if rec_n else []
    cap = show * (2 if two else 1)
    shown = names[:cap]
    more = len(names) - len(shown)
    L.append(c(f"Factions ({len(names)}, fullest first)", B) + (c(f"  +{more} more", D) if more else ""))
    if two:
        L.append(FHEAD + "   " + FHEAD)
        half = (len(shown) + 1) // 2
        for i in range(half):
            a = faction_row(shown[i], facs[shown[i]], batch, target)
            b = faction_row(shown[i + half], facs[shown[i + half]], batch, target) if i + half < len(shown) else ""
            L.append(pad(a, FW) + "   " + b)
    else:
        L.append(FHEAD)
        for n in shown:
            L.append(faction_row(n, facs[n], batch, target))
    if rec_n:
        L.append("")
        L.append(c("Recent updates", B))
        L.append(c(f"{'#':>5} {'faction':<11}{'seat':>5} {'vp':>6} {'≥6':>5} {'opt s':>6} {'lag':>5}", D))
        for r in reversed(recent):
            e = c(" EARLY", RED, B) if r.get("early_cut") else ""
            ge = r.get("ge6")
            L.append(f"{num(r.get('update'), '{}'):>5} {str(r.get('faction', DASH)):<11}"
                     f"{num(r.get('seat_games'), '{}'):>5} {num(r.get('mean_vp'), '{:.2f}'):>6} "
                     f"{num(None if ge is None else ge * 100, '{:.0f}%'):>5} {num(r.get('optimise_s')):>6} "
                     f"{num(r.get('lag_mean')):>5}{e}")
    L.append("")
    L += foot
    return [clip(x, W) for x in L]


def main():
    global ARGS
    ap = argparse.ArgumentParser()
    ap.add_argument("run_dir")
    ap.add_argument("--interval", type=float, default=2.0)
    ap.add_argument("--once", action="store_true")
    ARGS = ap.parse_args()
    path = str(Path(ARGS.run_dir) / "router_status.json")
    if hasattr(sys.stdout, "reconfigure"):
        sys.stdout.reconfigure(encoding="utf-8")
    if os.name == "nt":
        os.system("")  # enables VT escape processing in the Windows console
    def frame():
        sz = shutil.get_terminal_size((120, 40))
        return render(path, sz.columns, sz.lines - 1)
    if ARGS.once:
        print("\n".join(frame()))
        return
    try:
        sys.stdout.write("\x1b[?25l\x1b[2J")
        while True:
            sys.stdout.write("\x1b[H" + "".join(l + "\x1b[K\n" for l in frame()) + "\x1b[J")
            sys.stdout.flush()
            time.sleep(ARGS.interval)
    except KeyboardInterrupt:
        pass
    finally:
        sys.stdout.write("\x1b[?25h\n")


if __name__ == "__main__":
    main()
