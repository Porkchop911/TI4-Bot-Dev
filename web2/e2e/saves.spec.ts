import { existsSync } from "node:fs";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { expect, test, type Page } from "@playwright/test";

// The saved games of single player on the start screen. Needs the engine: `scripts/build-wasm.sh`.
const WASM = fileURLToPath(new URL("../src/session/ti4.wasm", import.meta.url));
test.skip(!existsSync(WASM), "the engine is not built: run web2/scripts/build-wasm.sh");
test.use({ viewport: { width: 1920, height: 1080 } });

/** Starts a table of single player and waits for its first question. Says the address of the game. */
async function startSingle(page: Page, seed: number) {
  await page.goto(`/?single&seed=${seed}&players=3`);
  await page
    .getByRole("button", { name: /^Start/ })
    .last()
    .click();
  await expect(page.getByRole("region", { name: "Open step" })).toBeVisible({ timeout: 15_000 });
  await expect(page).toHaveURL(/\?single=[^&]+$/);
  return new URL(page.url()).search;
}

test("desktop: a browser keeps several games of single player; Export writes one with its report, and Import opens it as single player", async ({
  page,
}) => {
  test.setTimeout(45_000);
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  const first = await startSingle(page, 3);
  // A reload of the address is the same game.
  await page.reload();
  await expect(page.getByRole("region", { name: "Open step" })).toBeVisible({ timeout: 15_000 });
  await expect(page).toHaveURL(new RegExp(`\\${first}$`));
  const second = await startSingle(page, 4);
  expect(second).not.toBe(first);

  await page.goto("/");
  const saved = page.getByRole("region", { name: "Saved games" });
  const rows = saved.getByRole("link", { name: /^Single player/ });
  await expect(rows).toHaveCount(2);
  await expect(saved.locator(`a[href="/${first}"]`)).toHaveCount(1);
  await expect(saved.locator(`a[href="/${second}"]`)).toHaveCount(1);

  const download = page.waitForEvent("download");
  await saved.getByRole("button", { name: "Export Single player" }).first().click();
  const path = await (await download).path();
  const text = await readFile(path, "utf8");
  const file = JSON.parse(text);
  expect(file.kind).toBe("ti4-offline-game");
  expect(typeof file.report.device).toBe("string");

  await saved.getByRole("button", { name: "Import" }).click();
  await page.getByLabel("Saved game file").setInputFiles({
    name: "game.json",
    mimeType: "application/json",
    buffer: Buffer.from(text),
  });
  await expect(page).toHaveURL(/\?single=[^&]+$/);
  const imported = new URL(page.url()).search;
  expect([first, second]).not.toContain(imported);
  await expect(page.getByRole("region", { name: "Open step" })).toBeVisible({ timeout: 15_000 });

  await page.goto("/");
  await expect(rows).toHaveCount(3);
  expect(errors).toEqual([]);
});
