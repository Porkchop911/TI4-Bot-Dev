import { test } from "@playwright/test";
import { NEUTRAL_COLOR, UNASSIGNED_COLOR } from "../../../src/presentation/boardPresentation";
import { contrastRatio, SEAT_COLORS, SEAT_SYMBOLS, seatInk } from "../../../src/presentation/playerDisplay";
import { shot } from "../_shared/shot";

const OLD_SEAT_3 = "#F1F5F9";
const NAMES = ["blue", "green", "cyan", "red", "pink", "orange", "yellow", "purple"];

function row(label: string, colors: string[], seat3: string) {
  const chips = colors
    .map(
      (c, i) =>
        `<div style="display:flex;flex-direction:column;align-items:center;gap:4px;width:84px"><span style="width:56px;height:56px;border-radius:50%;background:${c};color:${seatInk(c)};display:grid;place-items:center;font-size:24px;border:3px solid ${c}">${SEAT_SYMBOLS[i]}</span><small>${i + 1} ${i === 2 ? seat3 : NAMES[i]}<br>${c}</small></div>`,
    )
    .join("");
  return `<h3>${label}</h3><div style="display:flex;flex-wrap:wrap;gap:8px">${chips}</div>`;
}

test("palette overview", async ({ page }, testInfo) => {
  await page.setViewportSize({ width: 900, height: 620 });
  const oldColors = SEAT_COLORS.map((c, i) => (i === 2 ? OLD_SEAT_3 : c));
  const planet = (c: string, t: string) =>
    `<div style="display:flex;align-items:center;gap:8px"><span style="width:64px;height:64px;border-radius:50%;background:#1e293b;border:6px solid ${c}"></span><small>${t}</small></div>`;
  await page.setContent(`<body style="margin:0;padding:20px;background:#0f172a;color:#e2e8f0;font:14px system-ui">
    <h2 style="margin:0 0 8px">Seat colours</h2>
    ${row("Before", oldColors, "white")}${row("After", [...SEAT_COLORS], "cyan")}
    <h3>Planet control ring next to the grey styles</h3>
    <div style="display:flex;gap:28px;flex-wrap:wrap">
      ${planet(OLD_SEAT_3, "before: seat 3 (white)")}${planet(SEAT_COLORS[2], "after: seat 3 (cyan)")}
      ${planet(UNASSIGNED_COLOR, "unassigned " + UNASSIGNED_COLOR)}${planet(NEUTRAL_COLOR, "neutral " + NEUTRAL_COLOR)}
    </div>
    <p>Contrast on the board background #0f172a: old white ${contrastRatio(OLD_SEAT_3, "#0f172a").toFixed(1)}, cyan ${contrastRatio(SEAT_COLORS[2], "#0f172a").toFixed(1)}.</p></body>`);
  await shot(page, testInfo, "palette");
});
