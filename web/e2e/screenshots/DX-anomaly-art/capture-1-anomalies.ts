import { test, type Page } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { shot } from "../_shared/shot";
import { galleryPlayers, actor } from "../_shared/fixtures";
import type { BoardView, BoardTileView, PlacedUnitView } from "../../../src/protocol/types";

const planet = (id: string, label: string, r = 2, i = 1) => ({
  id, label, resources: r, influence: i, traits: [], tech_specialties: [],
});
const unit = (unit_type: string, extra: Partial<PlacedUnitView> = {}): PlacedUnitView => ({
  owner: actor, unit_type, damaged: false, ...extra,
});

type T = Partial<BoardTileView> & { system_id: string; q: number; r: number };
const tiles: T[] = [
  { system_id: "18", label: "Mecatol Rex", q: 0, r: 0, planets: [planet("mecatolrex", "Mecatol Rex", 1, 6)] },
  { system_id: "41", label: "Gravity Rift", q: 1, r: 0, anomalies: ["gravity rift"], planets: [] },
  { system_id: "42", label: "Supernova", q: 0, r: 1, anomalies: ["supernova"], planets: [] },
  { system_id: "43", label: "Nebula", q: -1, r: 1, anomalies: ["nebula"], planets: [] },
  { system_id: "44", label: "Asteroid Field", q: -1, r: 0, anomalies: ["asteroid field"], planets: [] },
  { system_id: "45", label: "Entropic Scar", q: 0, r: -1, anomalies: ["entropic scar"], planets: [] },
  { system_id: "64", label: "Rift and Nebula", q: 1, r: -1, anomalies: ["gravity rift", "nebula"], planets: [] },
  { system_id: "67", label: "Cormund", q: 2, r: -1, anomalies: ["gravity rift"], planets: [planet("cormund", "Cormund", 2, 0)] },
  { system_id: "65", label: "Asteroid and Supernova", q: 2, r: 0, anomalies: ["asteroid field", "supernova"], planets: [] },
  { system_id: "26", label: "Lodor", q: -2, r: 1, anomalies: ["nebula"], planets: [planet("lodor", "Lodor", 3, 1)] },
  { system_id: "27", label: "Mellon", q: -2, r: 2, planets: [planet("mellon", "Mellon", 0, 2), planet("zohbat", "Zohbat", 3, 1)] },
  { system_id: "28", label: "Quann", q: -1, r: 2, planets: [planet("quann", "Quann", 2, 1)] },
  { system_id: "29", label: "Empty", q: 1, r: 1, planets: [] },
];
const board: BoardView = {
  active_system: null,
  map_tiles: tiles as BoardTileView[],
  systems: {
    "18": { system_id: "18", command_tokens: [], planets: {}, units: [unit("infantry", { planet: "mecatolrex" }), unit("infantry", { planet: "mecatolrex" })] },
    "43": { system_id: "43", command_tokens: [], planets: {}, units: [unit("carrier"), unit("fighter"), unit("fighter")] },
    "41": { system_id: "41", command_tokens: [actor], planets: {}, units: [unit("destroyer")] },
    "27": { system_id: "27", command_tokens: [], planets: {}, units: [unit("infantry", { planet: "zohbat" }), unit("spacedock", { planet: "mellon" })] },
  },
} as BoardView;

const SIZES = { desktop: { width: 1440, height: 900 }, "390x844": { width: 390, height: 844 } } as const;

async function open(page: Page, size: { width: number; height: number }) {
  await page.setViewportSize(size);
  await openMockedGame(page, { board, players: galleryPlayers, choice: null });
  await page.getByTestId("system-hex-18").waitFor();
}

/** Puts the previous look back on the live DOM: no art, text badge, tight planet letters. */
async function asBefore(page: Page) {
  await page.evaluate(() => {
    document.querySelectorAll('[data-testid="anomaly-art"]').forEach((e) => e.remove());
    document.querySelectorAll('[data-testid="anomaly-badge"]').forEach((e) => {
      const t = e.querySelector("text");
      const name = t?.textContent?.replace(/ \+ .*/, "") ?? "";
      e.replaceChildren();
      const x = e.closest("g")?.parentElement?.querySelector("text");
      void x; void name;
    });
    document.querySelectorAll("[data-testid^='system-hex-'] text").forEach((t) => {
      if (t.getAttribute("letter-spacing") === "1") {
        t.setAttribute("letter-spacing", "0");
        t.setAttribute("stroke-width", "2.5");
      }
    });
  });
}

const zoomOut = async (page: Page, n: number) => {
  // The phone has no zoom buttons (pinch only); its default zoom already fits the map.
  const btn = page.getByTitle("Zoom Out").first();
  if (!(await btn.isVisible().catch(() => false))) return;
  for (let i = 0; i < n; i++) await btn.click();
};

for (const [name, size] of Object.entries(SIZES)) {
  test(`full map ${name}`, async ({ page }, testInfo) => {
    await open(page, size);
    await shot(page, testInfo, `1-map-${name}`);
  });
  test(`small hexes ${name}`, async ({ page }, testInfo) => {
    await open(page, size);
    await zoomOut(page, 3);
    await shot(page, testInfo, `2-small-${name}`);
  });
}

test("before and after, small and phone", async ({ page }, testInfo) => {
  for (const [name, size] of Object.entries(SIZES)) {
    await open(page, size);
    await zoomOut(page, name === "desktop" ? 2 : 1);
    await shot(page, testInfo, `3-after-${name}`);
    await asBefore(page);
    await shot(page, testInfo, `3-before-${name}`);
  }
});

test("zoomed in dual anomalies", async ({ page }, testInfo) => {
  await open(page, SIZES.desktop);
  for (let i = 0; i < 3; i++) await page.getByTitle("Zoom In").first().click();
  await shot(page, testInfo, "4-large-desktop");
});

test("inspector", async ({ page }, testInfo) => {
  await open(page, SIZES.desktop);
  await page.getByTestId("system-hex-64").click();
  await page.waitForTimeout(300);
  await shot(page, testInfo, "5-inspector-desktop");
});
