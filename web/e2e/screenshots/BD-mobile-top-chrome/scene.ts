import type { Page } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { opponent, playerWithHand } from "../_shared/players";

/** Which side of the change a run is: `before` is taken on the commit without the minimal bar. */
export const prefix = process.env.TI4_SHOT_PREFIX ?? "after";

export const SIZES = {
  "390x844": { width: 390, height: 844 },
  "360x740": { width: 360, height: 740 },
  "844x390": { width: 844, height: 390 },
  "980x740": { width: 980, height: 740 },
  "1440x900": { width: 1440, height: 900 },
} as const;
export type SizeName = keyof typeof SIZES;

const tokens = { tactic: 3, fleet: 3, strategy: 2 };

export type Situation = "waiting" | "your-turn" | "decision";

/** The mocked game: waiting on the opponent, the viewer's turn menu, or another pending decision. */
export async function openScene(page: Page, size: SizeName, situation: Situation) {
  await page.setViewportSize(SIZES[size]);
  const players = [playerWithHand({ technologies: ["amd", "st"] }), opponent];
  if (situation === "waiting") {
    await openMockedGame(page, {
      players,
      choice: {
        prompt: "action phase",
        player: opponent.id,
        options: [{ id: "pass", label: "pass", kind: "action" }],
      },
    });
  } else if (situation === "your-turn") {
    await openMockedGame(page, {
      players,
      choice: {
        prompt: "action phase",
        options: [
          { id: "tactical", label: "take a tactical action", kind: "action" },
          { id: "strategic|pok3politics", label: "take the strategic action of 3. Politics", kind: "action" },
          { id: "pass", label: "pass", kind: "action" },
        ],
        details: {
          kind: "turn_menu",
          closing: false,
          tokens,
          strategy_cards: [{ card: "pok3politics", used: false, option: "strategic|pok3politics" }],
          partners: [],
        },
      },
    });
    await page.getByTestId("turn-action-bar").waitFor();
  } else {
    await openMockedGame(page, {
      players,
      choice: {
        prompt: "Choose a command token to return",
        options: [
          { id: "a", label: "Tactic pool", kind: "choice" },
          { id: "b", label: "Fleet pool", kind: "choice" },
        ],
      },
    });
    // The decision dialog covers the screen; minimized it leaves the top bar and the map in view.
    await page.getByRole("button", { name: "Minimize decision" }).click();
    await page.getByRole("dialog", { name: /command token/i }).waitFor({ state: "hidden" }).catch(() => undefined);
  }
  await page.getByTestId("ti4-board-svg").waitFor();
  await page.waitForTimeout(400);
}

/** Opens the phone menu when there is one (the base commit has none: everything is in the header). */
export async function openMenu(page: Page) {
  const button = page.getByTestId("top-menu-button");
  if (!(await button.count())) return;
  await button.click();
  await page.getByTestId("top-menu-sheet").waitFor();
}

export async function closeMenu(page: Page) {
  const sheet = page.getByTestId("top-menu-sheet");
  if (!(await sheet.count())) return;
  await page.keyboard.press("Escape");
  await sheet.waitFor({ state: "detached" });
}

/** Picks a map view (overlay); on a phone it lives in the menu. */
export async function pickOverlay(page: Page, mode: string) {
  await openMenu(page);
  await page.getByTestId(`overlay-btn-${mode}`).click();
  await closeMenu(page);
  await page.waitForTimeout(300);
}

/** Heights of the top chrome and the map: the first thing under the status header. */
export async function measureTop(page: Page) {
  return page.evaluate(() => {
    const rect = (sel: string) => {
      const el = document.querySelector(sel);
      if (!el) return null;
      const r = el.getBoundingClientRect();
      return { y: Math.round(r.top), h: Math.round(r.height), b: Math.round(r.bottom) };
    };
    const stage = rect('[data-testid="board-stage"]');
    const svg = rect('[data-testid="ti4-board-svg"]');
    const pane = rect('[data-testid="turn-action-bar"]');
    const svgBottom = svg ? svg.b : null;
    return {
      // The action pane covers the lower part of a stacked map: how much of the map lies under it.
      paneTop: pane?.y ?? null,
      mapUnderPane: pane && svgBottom ? Math.max(0, svgBottom - pane.y) : 0,
      mapVisibleH: svg ? Math.round((pane ? Math.min(svg.b, pane.y) : svg.b) - svg.y) : null,
      topChrome: Math.round(stage?.y ?? 0),
      header: rect(".app-shell__header"),
      chrome: rect('[data-testid="board-chrome"]'),
      stageTop: stage?.y ?? null,
      stageHeight: stage?.h ?? null,
      svgHeight: svg?.h ?? null,
      docScrollW: document.documentElement.scrollWidth,
    };
  });
}
