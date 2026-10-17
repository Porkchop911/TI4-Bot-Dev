import { expect, type Locator, type Page, type TestInfo } from "@playwright/test";
import { shot } from "../_shared/shot";
import { hookEngine, type Web2Server } from "./web2";

/** The two layouts of web2 (AGENTS.md, "Screen size"): desktop, and a Pixel 9 in portrait. */
export const DESKTOP = { width: 1920, height: 1080 };
export const PHONE = { width: 412, height: 923 };

/**
 * The late game of the engine (8 seats, round 4, action phase, Sol to act) with every seat played
 * in this page (`humans=255`): the page shows whichever seat the game asks, so a reaction window
 * of any seat comes up as that seat's own decision. The hands are dealt by the scenario: Yin holds
 * Sabotage, Sardakk N'orr Coup d'Etat and Reverse Engineer, Hacan Flank Speed.
 */
export async function openLateGame(page: Page, server: Web2Server) {
  await hookEngine(page);
  await page.setViewportSize(DESKTOP);
  await page.goto(`${server.url}/?scenario=late-game&humans=255`);
  await expect(sheet(page)).toBeVisible({ timeout: 30_000 });
}

/** The step sheet over the board: the viewer's open decision. */
export const sheet = (page: Page) => page.getByRole("region", { name: "Open step" });

/** The open step has `text`: the game asks the next question (the engine may take seconds). */
export async function asked(page: Page, text: string | RegExp) {
  await expect(sheet(page)).toContainText(text, { timeout: 20_000 });
  await page.waitForTimeout(400);
}

export async function clickText(scope: Page | Locator, text: string) {
  await scope.getByText(text, { exact: false }).first().click();
}

export async function press(scope: Page | Locator, name: string | RegExp) {
  await scope.getByRole("button", { name }).first().click();
}

/** The toolbar's chips of the effects in force. */
export const chips = (page: Page) => page.getByRole("list", { name: "Effects in force" }).getByRole("listitem");

/** The page's header row (the toolbar). */
export const toolbar = (page: Page) => page.locator("header").first();

/** Shoots the viewport on desktop, then the same state at the width of a phone, then goes back. */
export async function both(page: Page, testInfo: TestInfo, name: string, phone = true) {
  await page.setViewportSize(DESKTOP);
  await page.waitForTimeout(300);
  await shot(page, testInfo, `${name}-desktop`);
  if (!phone) return;
  await page.setViewportSize(PHONE);
  await page.waitForTimeout(600);
  // Nothing should be cut at the side on a phone. Said, not asserted: the shots show it.
  const wide = await page.evaluate(() => {
    const out: string[] = [];
    for (const element of document.querySelectorAll("body *")) {
      const box = element.getBoundingClientRect();
      if (box.width > 0 && box.right > window.innerWidth + 1 && !element.closest("svg")) {
        out.push(`${element.tagName.toLowerCase()}.${String(element.className).slice(0, 60)} right=${Math.round(box.right)}`);
      }
    }
    return { scroll: document.documentElement.scrollWidth, overflow: out.slice(0, 5) };
  });
  if (wide.scroll > PHONE.width + 1 || wide.overflow.length) console.log(`${name}: wider than the phone`, JSON.stringify(wide));
  await shot(page, testInfo, `${name}-phone`);
  await page.setViewportSize(DESKTOP);
  await page.waitForTimeout(300);
}
