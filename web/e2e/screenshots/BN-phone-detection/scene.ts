import type { Browser, BrowserContext, Page } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { opponent, playerWithHand } from "../_shared/players";

/** Which side of the change a run is: `before` is the old behaviour (layout forced to desktop). */
export const prefix = process.env.TI4_SHOT_PREFIX ?? "after";

const MOBILE_UA =
  "Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36";
const DESKTOP_UA =
  "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36";
const IPAD_UA =
  "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15";

/**
 * Device emulations. "desktop-site" is what Chrome Android / Safari iOS do for "Request desktop
 * site": a desktop user agent and a ~980 px wide layout viewport (here 980 x 2121, the 390 x 844
 * screen scaled by 980/390), while the physical screen is still 390 x 844 CSS px with a coarse,
 * hover-less pointer.
 */
export const DEVICES = {
  mobile: {
    viewport: { width: 390, height: 844 },
    screen: { width: 390, height: 844 },
    isMobile: true,
    hasTouch: true,
    deviceScaleFactor: 2,
    userAgent: MOBILE_UA,
  },
  "mobile-landscape": {
    viewport: { width: 844, height: 390 },
    screen: { width: 844, height: 390 },
    isMobile: true,
    hasTouch: true,
    deviceScaleFactor: 2,
    userAgent: MOBILE_UA,
  },
  "desktop-site": {
    viewport: { width: 980, height: 2121 },
    screen: { width: 390, height: 844 },
    isMobile: false,
    hasTouch: true,
    deviceScaleFactor: 1,
    userAgent: DESKTOP_UA,
  },
  ipad: {
    viewport: { width: 820, height: 1180 },
    screen: { width: 820, height: 1180 },
    isMobile: true,
    hasTouch: true,
    deviceScaleFactor: 1,
    userAgent: IPAD_UA,
  },
  desktop: {
    viewport: { width: 1440, height: 900 },
    screen: { width: 1440, height: 900 },
    isMobile: false,
    hasTouch: false,
    deviceScaleFactor: 1,
    userAgent: DESKTOP_UA,
  },
} as const;
export type DeviceName = keyof typeof DEVICES;

export async function newDevicePage(browser: Browser, device: DeviceName, baseURL?: string): Promise<{ context: BrowserContext; page: Page }> {
  const context = await browser.newContext({ ...DEVICES[device], colorScheme: "dark", baseURL });
  const page = await context.newPage();
  return { context, page };
}

const tokens = { tactic: 3, fleet: 3, strategy: 2 };
export type Situation = "waiting" | "your-turn" | "decision";

export const players = () => [playerWithHand({ technologies: ["amd", "st"] }), opponent];

export async function openScene(page: Page, situation: Situation, layout?: "desktop" | "compact") {
  // Before the change the page had no layout switch: forcing "desktop" reproduces that behaviour.
  if (layout) await page.addInitScript((value) => window.localStorage.setItem("ti4.layout.v1", value), layout);
  if (situation === "waiting") {
    await openMockedGame(page, {
      players: players(),
      choice: { prompt: "action phase", player: opponent.id, options: [{ id: "pass", label: "pass", kind: "action" }] },
    });
  } else if (situation === "your-turn") {
    await openMockedGame(page, {
      players: players(),
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
      players: players(),
      choice: {
        prompt: "Choose a command token to return",
        options: [
          { id: "a", label: "Tactic pool", kind: "choice" },
          { id: "b", label: "Fleet pool", kind: "choice" },
        ],
      },
    });
  }
  await page.getByTestId("ti4-board-svg").waitFor();
  await page.waitForTimeout(400);
}

/** What the page decided: layout attribute, top bar, widths and the media features the decision reads. */
export async function probe(page: Page) {
  return page.evaluate(() => ({
    layout: document.documentElement.getAttribute("data-layout"),
    innerWidth: window.innerWidth,
    innerHeight: window.innerHeight,
    screen: `${window.screen.width}x${window.screen.height}`,
    dpr: window.devicePixelRatio,
    coarse: window.matchMedia("(pointer: coarse)").matches,
    noHover: window.matchMedia("(hover: none)").matches,
    narrow: window.matchMedia("(max-width: 720px), (max-height: 500px)").matches,
    topMenuButton: document.querySelector('[data-testid="top-menu-button"]') !== null,
    viewportMeta: document.querySelector('meta[name="viewport"]')?.getAttribute("content") ?? null,
    scrollWidth: document.documentElement.scrollWidth,
    headerH: Math.round(document.querySelector(".app-shell__header")?.getBoundingClientRect().height ?? 0),
  }));
}
