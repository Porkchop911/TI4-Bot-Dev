import type { Page } from "@playwright/test";
import type { MockOptions } from "../../../src/offline/mockLauncher";
import type { OfflineGameSummary } from "../../../src/offline/types";

export const SIZES = { desktop: { width: 1440, height: 900 }, "390x844": { width: 390, height: 844 } } as const;

const now = Math.floor(Date.now() / 1000);
export const GAMES: OfflineGameSummary[] = [
  { gameId: "off_a", playerCount: 6, humanFaction: "hacan", humanSeat: 3, round: 4, phase: "Action", answers: 612, createdAt: now - 86400, updatedAt: now - 3600 * 5, status: "in_progress", resumeEstimateMs: 12000 },
  { gameId: "off_b", playerCount: 4, humanFaction: "sol", humanSeat: 1, round: 2, phase: "Strategy", answers: 140, createdAt: now - 7200, updatedAt: now - 600, status: "in_progress", resumeEstimateMs: 3000 },
  { gameId: "off_c", playerCount: 3, humanFaction: "xxcha", humanSeat: 2, round: 5, phase: null, answers: 480, createdAt: now - 400000, updatedAt: now - 300000, status: "finished", resumeEstimateMs: null },
];

/** Opens the real app with the in-memory offline launcher registered (dev builds only). */
export async function openOffline(page: Page, path: string, mock: MockOptions = {}) {
  await page.route("**/health", (route) =>
    route.fulfill({ json: { status: "ok", protocol_version: 4, commit: "51390594", public: true } }),
  );
  await page.route("**/api/feedback", (route) => route.fulfill({ status: 500, json: {} }));
  await page.route("**/api/**", (route) => {
    const url = route.request().url();
    if (url.includes("/api/access")) return route.fulfill({ json: { required: false } });
    if (url.includes("/api/build")) return route.fulfill({ json: { commit: "dev", protocol_version: 1 } });
    return route.fulfill({ status: 503, json: { error: "offline" } });
  });
  await page.addInitScript((m) => {
    (window as unknown as { __TI4_OFFLINE_MOCK__: unknown }).__TI4_OFFLINE_MOCK__ = m;
  }, mock);
  await page.goto(path);
}
