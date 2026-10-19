#!/usr/bin/env python3
"""HTML report for an async per-faction PPO run.

Usage: python scripts/training_report.py <run dir> <output.html>

Reads <run dir>/stdout.log (required); stderr.log and router_status.json are
read when present. Writes one self-contained page with inline SVG and CSS and
no JavaScript. Stdlib only. Never writes to the run directory and never
touches the training process.
"""
import datetime as dt
import html
import json
import math
import os
import re
import sys

ROLL = 50            # rolling window, in updates
FRESH_S = 300        # data older than this shows "Stale"
W, H = 640, 300      # chart viewBox
L, R, T, B = 66, 18, 16, 42

UPDATE_RE = re.compile(r"^\s*update\s+(\d+)\s+\[([^\]]+)\]")
SEAT_RE = re.compile(r"(\d+)\s+seat-games")
VP_RE = re.compile(r"mean VP\s+([\d.]+)")
GE6_RE = re.compile(r">=6 VP\s+([\d.]+)%")
LAG_RE = re.compile(r"lag max\s+\d+\s+mean\s+([\d.]+)")
OPT_RE = re.compile(r"optimise\s+([\d.]+)(µs|us|ms|s)\b")
QUEUED_RE = re.compile(r"queued\s+(\d+)")
BUILT_RE = re.compile(r"built/seat\s+([\d.]+)")
FLEET_RE = re.compile(r"fleet/seat\s+([\d.]+)")
PROBLEM_RE = re.compile(r"panicked|REFUSED|out of memory|non-finite", re.I)
UNIT = {"µs": 1e-6, "us": 1e-6, "ms": 1e-3, "s": 1.0}

LIGHT = {"bg": "#f5f8f9", "fg": "#17262c", "muted": "#5b6f77", "line": "#d6e0e4",
         "accent": "#13766f", "good": "#1d7a45", "warn": "#8f5e00", "bad": "#b42318"}
DARK = {"bg": "#0f1619", "fg": "#d7e3e7", "muted": "#8ca0a8", "line": "#243740",
        "accent": "#56c2b6", "good": "#6fcf97", "warn": "#e8b452", "bad": "#f0877a"}

CSS = """
:root{@@LIGHT@@color-scheme:light}
@media (prefers-color-scheme: dark){:root:not([data-theme="light"]){@@DARK@@color-scheme:dark}}
:root[data-theme="dark"]{@@DARK@@color-scheme:dark}
*{box-sizing:border-box}
body{margin:0;background:var(--bg);color:var(--fg);font:15px/1.5 "IBM Plex Sans",system-ui,sans-serif;
padding-left:max(16px,env(safe-area-inset-left,0px));padding-right:max(16px,env(safe-area-inset-right,0px));
padding-top:24px;padding-bottom:24px}
main{max-width:1100px;margin:0 auto}
h1{font-size:26px;margin:0 0 2px;font-weight:600}
h2{font-size:17px;font-weight:600;margin:30px 0 8px}
.sub{margin:0;color:var(--muted)}
.status{margin:10px 0 0;font:13px/1.5 "IBM Plex Mono",ui-monospace,monospace;color:var(--muted);display:flex;flex-wrap:wrap;gap:6px 12px;align-items:center}
.pill{border:1px solid currentColor;border-radius:999px;padding:0 10px;font:600 13px/1.6 "IBM Plex Sans",system-ui,sans-serif}
.good{color:var(--good)} .warn{color:var(--warn)} .bad{color:var(--bad)} .muted{color:var(--muted)}
.tiles{display:grid;grid-template-columns:repeat(auto-fit,minmax(140px,1fr));gap:12px;margin-top:20px}
.tile{border:1px solid var(--line);border-radius:6px;padding:10px 12px;min-width:0}
.tile .k{display:block;font-size:12px;color:var(--muted);text-transform:uppercase;letter-spacing:.04em}
.tile .v{display:block;font:500 20px/1.3 "IBM Plex Mono",ui-monospace,monospace;font-variant-numeric:tabular-nums;overflow-wrap:anywhere}
figure{margin:0}
svg{display:block;width:100%;max-width:720px;height:auto}
figcaption{font-size:13px;color:var(--muted);margin-top:4px}
.grid{stroke:var(--line);stroke-width:1}
.axis{stroke:var(--muted);stroke-width:1}
.tick{fill:var(--muted);font:18px "IBM Plex Mono",ui-monospace,monospace}
.dot{fill:var(--muted);fill-opacity:.5}
.roll{fill:none;stroke:var(--accent);stroke-width:4;stroke-linejoin:round;stroke-linecap:round}
.roll2{fill:none;stroke:var(--fg);stroke-width:3;stroke-dasharray:10 7;stroke-linejoin:round}
.sw{display:inline-block;width:20px;height:3px;vertical-align:middle;margin-right:4px;background:var(--accent)}
.sw2{background:var(--fg)}
.scroll{overflow-x:auto;max-width:100%}
table{border-collapse:collapse;width:100%;font-size:14px}
th,td{text-align:right;padding:5px 8px;border-bottom:1px solid var(--line);white-space:nowrap}
th:first-child,td:first-child{text-align:left}
th{font-weight:600;color:var(--muted);font-size:12px;text-transform:uppercase}
td{font-family:"IBM Plex Mono",ui-monospace,monospace;font-variant-numeric:tabular-nums}
pre{margin:0;font:13px/1.45 "IBM Plex Mono",ui-monospace,monospace;overflow-x:auto;border:1px solid var(--line);border-radius:6px;padding:12px}
"""


def esc(v):
    return html.escape(str(v), quote=True)


def tokens(d):
    return "".join(f"--{k}:{v};" for k, v in d.items())


def mean(xs):
    xs = list(xs)
    return sum(xs) / len(xs) if xs else None


def fmt(v, nd=2, suffix=""):
    if v is None:
        return "&mdash;"
    if nd is None:
        return f"{v:,}{suffix}"
    return f"{v:.{nd}f}{suffix}"


def parse_stdout(text):
    """Return (header lines before the first update, list of update dicts)."""
    header, updates, cur = [], [], None
    for line in text.splitlines():
        m = UPDATE_RE.match(line)
        if m:
            cur = {"update": int(m.group(1)), "faction": m.group(2).strip()}
            seat, vp, ge6 = SEAT_RE.search(line), VP_RE.search(line), GE6_RE.search(line)
            lag, opt, q = LAG_RE.search(line), OPT_RE.search(line), QUEUED_RE.search(line)
            cur.update(
                seat_games=int(seat.group(1)) if seat else None,
                vp=float(vp.group(1)) if vp else None,
                ge6=float(ge6.group(1)) if ge6 else None,
                lag_mean=float(lag.group(1)) if lag else None,
                optimise_s=float(opt.group(1)) * UNIT[opt.group(2)] if opt else None,
                queued=int(q.group(1)) if q else None,
                built=None,
                fleet=None,
            )
            updates.append(cur)
        elif cur is None:
            header.append(line.rstrip())
        else:
            # detail lines (overpay/built/fleet) follow their update line
            b, f = BUILT_RE.search(line), FLEET_RE.search(line)
            if b:
                cur["built"] = float(b.group(1))
            if f:
                cur["fleet"] = float(f.group(1))
    while header and not header[0].strip():
        header.pop(0)
    while header and not header[-1].strip():
        header.pop()
    return header, updates


def rolling(pts, win=ROLL):
    out = []
    for i, (x, _) in enumerate(pts):
        window = [y for _, y in pts[max(0, i - win + 1): i + 1]]
        out.append((x, sum(window) / len(window)))
    return out


def nice_ticks(lo, hi, target=5):
    if hi <= lo:
        hi = lo + 1.0
    raw = (hi - lo) / (target - 1)
    mag = 10 ** math.floor(math.log10(raw))
    step = next(m * mag for m in (1, 2, 5, 10) if m * mag >= raw)
    start = math.floor(lo / step + 1e-9) * step
    end = math.ceil(hi / step - 1e-9) * step
    n = round((end - start) / step)
    return step, [round(start + i * step, 9) for i in range(n + 1)]


def svg_chart(layers, label):
    """layers: list of (points, css class, 'dots' or 'line')."""
    pts_all = [p for pts, _, _ in layers for p in pts]
    if not pts_all:
        return '<p class="muted">No data parsed.</p>'
    _, xt = nice_ticks(min(x for x, _ in pts_all), max(x for x, _ in pts_all), 6)
    ystep, yt = nice_ticks(min(y for _, y in pts_all), max(y for _, y in pts_all), 5)
    x0, x1, y0, y1 = xt[0], xt[-1], yt[0], yt[-1]
    iw, ih = W - L - R, H - T - B

    def sx(x):
        return L + (x - x0) / (x1 - x0) * iw

    def sy(y):
        return T + (1 - (y - y0) / (y1 - y0)) * ih

    ydec = max(0, math.ceil(-math.log10(ystep) - 1e-9))
    out = [f'<svg viewBox="0 0 {W} {H}" role="img" aria-label="{esc(label)}">']
    for v in yt:
        y = sy(v)
        out.append(f'<line class="grid" x1="{L}" x2="{W - R}" y1="{y:.1f}" y2="{y:.1f}"/>')
        out.append(f'<text class="tick" x="{L - 10}" y="{y + 6:.1f}" text-anchor="end">{v:.{ydec}f}</text>')
    for v in xt:
        out.append(f'<text class="tick" x="{sx(v):.1f}" y="{H - B + 26}" text-anchor="middle">{v:.0f}</text>')
    out.append(f'<line class="axis" x1="{L}" x2="{W - R}" y1="{H - B}" y2="{H - B}"/>')
    for pts, cls, kind in layers:
        if kind == "dots":
            out += [f'<circle class="{cls}" cx="{sx(x):.1f}" cy="{sy(y):.1f}" r="2.6"/>' for x, y in pts]
        elif pts:
            coords = " ".join(f"{sx(x):.1f},{sy(y):.1f}" for x, y in pts)
            out.append(f'<polyline class="{cls}" points="{coords}"/>')
    out.append("</svg>")
    return "".join(out)


def series(updates, key):
    return [(u["update"], u[key]) for u in updates if u[key] is not None]


def faction_rows(updates, router_factions):
    by = {}
    for u in updates:
        if u["vp"] is not None:
            by.setdefault(u["faction"], []).append(u)
    rows = []
    for name in sorted(set(by) | set(router_factions)):
        us = by.get(name, [])
        vps = [u["vp"] for u in us]
        ge6 = [u["ge6"] for u in us if u["ge6"] is not None]
        rows.append({
            "name": name,
            "n": len(us),
            "last5": mean(vps[-5:]),
            # change needs 6+ batches so the first and last five do not overlap
            "change": mean(vps[-5:]) - mean(vps[:5]) if len(us) >= 6 else None,
            "ge6": mean(ge6),
            "last": us[-1]["update"] if us else None,
        })
    rows.sort(key=lambda r: (r["n"], r["name"]))
    return rows


def last_sunday(year, month):
    nxt = dt.date(year + (month == 12), month % 12 + 1, 1)
    d = nxt - dt.timedelta(days=1)
    return d - dt.timedelta(days=(d.weekday() + 1) % 7)


def berlin(utc):
    """Format a UTC datetime as Berlin local time (EU DST rule, stdlib only)."""
    start = dt.datetime.combine(last_sunday(utc.year, 3), dt.time(1), tzinfo=dt.timezone.utc)
    end = dt.datetime.combine(last_sunday(utc.year, 10), dt.time(1), tzinfo=dt.timezone.utc)
    summer = start <= utc < end
    local = utc + dt.timedelta(hours=2 if summer else 1)
    return local.strftime("%Y-%m-%d %H:%M ") + ("CEST" if summer else "CET")


def parse_iso(s):
    s = re.sub(r"(\.\d{6})\d+", r"\1", s)  # nanoseconds -> microseconds
    t = dt.datetime.fromisoformat(s)
    return t if t.tzinfo else t.replace(tzinfo=dt.timezone.utc)


def read_text(path):
    if not os.path.isfile(path):
        return None
    with open(path, encoding="utf-8", errors="replace") as f:
        return f.read()


def main(argv):
    if len(argv) != 3:
        print(__doc__.strip(), file=sys.stderr)
        return 2
    run_dir, out_path = argv[1], argv[2]
    stdout_path = os.path.join(run_dir, "stdout.log")
    stdout_text = read_text(stdout_path)
    if stdout_text is None:
        print(f"no stdout.log in {run_dir}", file=sys.stderr)
        return 1

    header, updates = parse_stdout(stdout_text)
    router_text = read_text(os.path.join(run_dir, "router_status.json"))
    router = json.loads(router_text) if router_text else {}
    gpu = router.get("gpu", {})
    stderr_text = read_text(os.path.join(run_dir, "stderr.log")) or ""

    now = dt.datetime.now(dt.timezone.utc)
    if router.get("updated"):
        stamp = parse_iso(router["updated"])
    else:
        stamp = dt.datetime.fromtimestamp(os.path.getmtime(stdout_path), tz=dt.timezone.utc)
    age_s = (now - stamp).total_seconds()
    if PROBLEM_RE.search(stderr_text):
        state, state_cls = "Problem", "bad"
    elif age_s >= FRESH_S:
        state, state_cls = "Stale", "warn"
    else:
        state, state_cls = "Healthy", "good"

    name = os.path.basename(os.path.normpath(run_dir))
    # ...-20261019-r6b -> r6b; ...-wide-blank-20261019 -> blank
    m = re.search(r"\d{8}-(.+)$", name) or re.search(r"-([a-z0-9]+)-\d{8}$", name)
    short = m.group(1) if m else name
    title = f"TI4 Run {short}"
    last_no = updates[-1]["update"] if updates else "none"
    target = router.get("target") or (updates[-1]["faction"] if updates else "&mdash;")

    tiles = [
        ("Updates", fmt(router.get("updates_done", len(updates)), None)),
        ("Games", fmt(router.get("games_finished"), None)),
        ("Games/s", fmt(router.get("games_per_s"), 2)),
        ("Workers blocked", fmt(router.get("workers_blocked_pct"), 1, "%")),
        ("GPU idle", fmt(gpu.get("optimiser_idle_pct"), 1, "%")),
        ("Target", target if target.startswith("&") else esc(target)),
    ]
    tiles_html = "".join(
        f'<div class="tile"><span class="k">{k}</span><span class="v">{v}</span></div>' for k, v in tiles)

    vp = series(updates, "vp")
    opt = series(updates, "optimise_s")
    built = series(updates, "built")
    fleet = series(updates, "fleet")
    charts = [
        ("Mean VP per update", svg_chart(
            [(vp, "dot", "dots"), (rolling(vp), "roll", "line")], "Mean VP per update"),
         "Dots: each update's batch mean VP (60 seat-games). Line: rolling 50-update mean. x: update number."),
        ("Optimise time per update", svg_chart(
            [(opt, "dot", "dots"), (rolling(opt), "roll", "line")], "Optimise seconds per update"),
         "Seconds per optimiser step. Dots: each update. Line: rolling 50-update mean."),
    ]
    if built or fleet:
        charts.append(("Built and fleet per seat", svg_chart(
            [(rolling(built), "roll", "line"), (rolling(fleet), "roll2", "line")],
            "Built and fleet per seat, rolling mean"),
            '<span class="sw"></span>built/seat (rolling 50) &nbsp; <span class="sw sw2"></span>fleet/seat (rolling 50)'))
    charts_html = "".join(
        f"<h2>{h}</h2><figure>{svg}<figcaption>{cap}</figcaption></figure>" for h, svg, cap in charts)

    rows = faction_rows(updates, router.get("factions", {}))
    row_html = []
    for r in rows:
        c = r["change"]
        cls = "" if c is None else ("good" if c > 0.005 else "bad" if c < -0.005 else "muted")
        chg = "&mdash;" if c is None else f"{'+' if c >= 0 else '&minus;'}{abs(c):.2f}"
        row_html.append(
            f"<tr><td>{esc(r['name'])}</td><td>{r['n']}</td><td>{fmt(r['last5'])}</td>"
            f'<td class="{cls}">{chg}</td><td>{fmt(r["ge6"], 1, "%")}</td><td>{fmt(r["last"], None)}</td></tr>')
    faction_html = (
        '<div class="scroll" style="overflow-x:auto"><table><thead><tr><th>Faction</th><th>Updates</th>'
        "<th>Mean VP, last 5</th><th>Change vs first 5</th><th>&ge;6 VP share</th><th>Last update</th>"
        "</tr></thead><tbody>" + "".join(row_html) + "</tbody></table></div>")

    recent_html = []
    for u in updates[-15:][::-1]:
        recent_html.append(
            f"<tr><td>{u['update']}</td><td>{esc(u['faction'])}</td><td>{fmt(u['seat_games'], None)}</td>"
            f"<td>{fmt(u['vp'])}</td><td>{fmt(u['ge6'], 1, '%')}</td><td>{fmt(u['lag_mean'], 1)}</td>"
            f"<td>{fmt(u['optimise_s'], 1)}</td><td>{fmt(u['queued'], None)}</td></tr>")
    recent_table = (
        '<div class="scroll" style="overflow-x:auto"><table><thead><tr><th>Update</th><th>Faction</th>'
        "<th>Seat-games</th><th>Mean VP</th><th>&ge;6 VP</th><th>Lag mean</th><th>Optimise s</th>"
        "<th>Queued</th></tr></thead><tbody>" + "".join(recent_html) + "</tbody></table></div>")

    age_txt = f"data {age_s / 60:.1f} min old"
    settings = esc("\n".join(header))
    body = f"""<main>
<header>
<h1>{esc(title)}</h1>
<p class="sub">Per-faction PPO, wide roster</p>
<p class="status"><span>Last updated {esc(berlin(stamp))}, update {last_no}</span>
<span class="pill {state_cls}">{state}</span><span>{esc(age_txt)}</span></p>
</header>
<section class="tiles">{tiles_html}</section>
{charts_html}
<h2>Per-faction (fewest updates first)</h2>
{faction_html}
<h2>Recent updates</h2>
{recent_table}
<h2>Settings</h2>
<pre style="overflow-x:auto">{settings}</pre>
</main>
"""
    css = CSS.replace("@@LIGHT@@", tokens(LIGHT)).replace("@@DARK@@", tokens(DARK))
    page = "".join([
        f"<title>{esc(title)}</title>\n",
        '<meta name="claude:viewport-fit-sides" content="cover">\n',
        '<link rel="stylesheet" href="https://fonts.googleapis.com/css2?family=IBM+Plex+Mono:wght@400;500'
        '&family=IBM+Plex+Sans:wght@400;600&display=swap">\n',
        f"<style>{css}</style>\n",
        body,
    ])
    with open(out_path, "w", encoding="ascii", errors="xmlcharrefreplace", newline="\n") as f:
        f.write(page)
    print(f"parsed {len(updates)} updates, {len(rows)} factions -> {out_path}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
