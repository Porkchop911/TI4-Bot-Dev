import { test } from "@playwright/test";
import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { artifactDir } from "../_shared/shot";
import { runAudit, type AuditRow } from "./audit";
import { prefix, SIZES, type SizeName } from "./scene";

// The audit table: every overlay opened on three phone layouts, the share of the map viewport it
// covers (the map's own toolbar and legend excluded), whether a minimize / close control exists and
// what is left covering after it is used. Measured in the page with elementFromPoint on a 12x12 grid
// of the map viewport; the table is drawn into a PNG so it can be compared before / after the fix.
test.setTimeout(20 * 60_000);

const sizes = Object.keys(SIZES) as SizeName[];
const pct = (n: number) => `${Math.round(n * 100)}%`;

function verdict(all: AuditRow[]): { text: string; tone: "ok" | "warn" | "bad" } {
  const rows = all.filter((r) => r.open.samples > 0);
  if (!rows.length) return { text: "n/a: the page leaves no free map", tone: "warn" };
  const worst = rows.reduce((a, r) => (r.open.covered > a.open.covered ? r : a));
  if (worst.open.covered <= 0.15) return { text: "does not cover the map", tone: "ok" };
  const control = rows.find((r) => r.open.minimizeControl);
  if (!control) {
    return worst.open.covered <= 0.3
      ? { text: `slim bar (${pct(worst.open.covered)}), carries the confirm control`, tone: "ok" }
      : { text: "covers the map, no way to minimize", tone: "bad" };
  }
  const still = rows.filter((r) => r.minimized && r.minimized.covered > 0.2);
  if (still.length) return { text: `still covers ${pct(Math.max(...still.map((r) => r.minimized!.covered)))} after minimizing`, tone: "warn" };
  return { text: "minimizes / closes to the map", tone: "ok" };
}

test("audit table", async ({ browser, page }, testInfo) => {
  const all: AuditRow[] = [];
  for (const size of sizes) all.push(...(await runAudit(browser, size)));
  const dir = join(artifactDir(testInfo), "out");
  mkdirSync(dir, { recursive: true });
  writeFileSync(join(dir, `audit-${prefix}.json`), JSON.stringify(all, null, 1));

  const overlays = [...new Set(all.map((r) => r.overlay))];
  const body = overlays
    .map((name) => {
      const rows = sizes.map((s) => all.find((r) => r.overlay === name && r.size === s)).filter((r): r is AuditRow => !!r);
      if (!rows.length) return "";
      const v = verdict(rows);
      const cells = sizes
        .map((s) => {
          const r = rows.find((x) => x.size === s);
          if (!r) return "<td>-</td>";
          if (r.open.samples === 0) return "<td>map hidden by the page<small>no free map to measure</small></td>";
          const min = r.minimized ? ` &rarr; ${pct(r.minimized.covered)}` : "";
          return `<td>${pct(r.open.covered)}${min}<small>${r.open.small.length} &lt;44px</small></td>`;
        })
        .join("");
      const control = rows.find((r) => r.open.minimizeControl)?.open.minimizeControl ?? "none";
      return `<tr><th>${name}<small>${rows[0]!.trigger}</small></th>${cells}<td><code>${control}</code></td><td class="${v.tone}">${v.text}</td></tr>`;
    })
    .join("");
  await page.setViewportSize({ width: 1100, height: 400 });
  await page.setContent(`<!doctype html><meta charset="utf-8"><style>
    body{margin:0;padding:16px;background:#0b1020;color:#e6ebf7;font:13px/1.35 system-ui,sans-serif}
    table{border-collapse:collapse;width:100%}
    th,td{padding:5px 8px;border-bottom:1px solid #2a3454;text-align:left;vertical-align:top}
    thead th{color:#9aa6c4;font-size:11px;text-transform:uppercase;letter-spacing:.05em}
    th small,td small{display:block;color:#9aa6c4;font-weight:400;font-size:11px}
    td.ok{color:#7ee2a8}td.warn{color:#fcd34d}td.bad{color:#ff8a8a;font-weight:600}
    code{font-size:11px;color:#9aa6c4}
  </style><table><thead><tr><th>Overlay / trigger</th>${sizes.map((s) => `<th>${s}<small>map covered (open &rarr; minimized)</small></th>`).join("")}<th>Control</th><th>Verdict</th></tr></thead><tbody>${body}</tbody></table>`);
  const height = await page.evaluate(() => Math.ceil(document.body.scrollHeight));
  await page.setViewportSize({ width: 1100, height });
  await page.screenshot({ path: join(dir, `${prefix}-audit-table.png`), animations: "disabled" });
});
