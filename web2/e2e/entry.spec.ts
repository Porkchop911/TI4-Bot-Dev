import { existsSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { expect, test, type Page } from "@playwright/test";

// The address decides the app: `/` is the start screen, `?single` is single player (the setup of
// a table in the page, or its game), `?local=` a game in the page on a fixed setup, `?example=`
// is the demo.
const SINGLE_PLAYER = "/?single";
/** The local game of the header of the demo: seed 42, eight seats. */
const LOCAL_GAME = "/?local=42&players=8&humans=1";
const WASM = fileURLToPath(new URL("../src/session/ti4.wasm", import.meta.url));
test.use({ viewport: { width: 1920, height: 1080 } });

/** The engine is not there: the page must fall back to the demo, and say why. */
const withoutEngine = (page: Page) =>
  page.route("**/ti4.wasm", (route) => route.fulfill({ status: 404 }));

test("the bare address is the choice between single player and multiplayer", async ({ page }) => {
  await page.goto("/");
  const rows = page.getByRole("navigation", { name: "Game" }).getByRole("link");
  await expect(rows).toHaveText([/Single player/, /Multiplayer/]);
  await expect(rows.nth(0)).toHaveAttribute("href", SINGLE_PLAYER);
  await expect(rows.nth(1)).toHaveAttribute("href", "/?online");
  await expect(page.getByText("Round 1")).toHaveCount(0);
  await page.screenshot({ path: "shots/start.png", animations: "disabled" });
  // The name of the player is kept for the tables of this browser.
  await page.getByLabel("Name").fill("Ada");
  expect(await page.evaluate(() => localStorage.getItem("ti4.nickname"))).toBe("Ada");
  await page.reload();
  await expect(page.getByLabel("Name")).toHaveValue("Ada");
  // A row is the control, and it has a key.
  await page.getByLabel("Name").blur();
  await page.keyboard.press("1");
  await page.waitForURL((url) => url.searchParams.has("single"));
});

test("the local game of an address: seed 42, eight seats", async ({ page }) => {
  test.skip(!existsSync(WASM), "the engine is not built: run web2/scripts/build-wasm.sh");
  await page.goto(LOCAL_GAME);
  await page.getByRole("button", { name: "Dev", exact: true }).click();
  await expect(page.getByRole("complementary", { name: "Dev" })).toContainText("seed 42 · 8 seats");
  await page.keyboard.press("Escape");
  await expect(page.getByRole("button", { name: "Demo", exact: true })).toBeVisible();
});

test("without the engine a game in the page shows the demo, with the reason", async ({ page }) => {
  await withoutEngine(page);
  await page.goto(LOCAL_GAME);
  await expect(page.getByText("Local game unavailable.")).toBeVisible();
  await expect(page.locator("#example")).toBeVisible();
  await page.goto(SINGLE_PLAYER);
  await expect(page.getByText("Single player unavailable.")).toBeVisible();
  await expect(page.locator("#example")).toBeVisible();
});

test("the header switches between the demo and the local game", async ({ page }) => {
  await withoutEngine(page);
  await page.goto("/?example=live-strategy-2");
  await page.getByRole("button", { name: "Local game", exact: true }).click();
  await page.waitForURL((url) => url.searchParams.get("local") === "42");
  await expect(page.getByText("Local game unavailable.")).toBeVisible();
});
