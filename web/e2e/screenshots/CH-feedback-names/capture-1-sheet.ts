import { expect, test, type Page } from "@playwright/test";
import { shot } from "../_shared/shot";
import { galleryPlayers } from "../_shared/fixtures";
import { openMockedGame } from "../_shared/mockGame";

const SIZES = { desktop: { width: 1440, height: 900 }, phone: { width: 390, height: 844 } } as const;
type Size = keyof typeof SIZES;

async function publicServer(page: Page) {
  await page.route("**/health", (route) =>
    route.fulfill({ json: { status: "ok", protocol_version: 3, commit: "51390594", public: true } }),
  );
  await page.route("**/api/access", (route) => route.fulfill({ json: { required: true, granted: true } }));
}

async function mockFeedback(page: Page) {
  const received: Array<Record<string, unknown>> = [];
  await page.route("**/api/feedback", async (route) => {
    received.push(route.request().postDataJSON() as Record<string, unknown>);
    await route.fulfill({ json: { id: "fb_1760268000_9f3a1c2e" } });
  });
  return received;
}

const names = ["Alex", "np", "Casey", "Blair", "Drew", "Eli"];
const nicknames = Object.fromEntries(galleryPlayers.map((p, i) => [p.id, names[i] ?? `P${i + 1}`]));

async function openSheet(page: Page, size: Size, spectator: boolean) {
  await page.setViewportSize(SIZES[size]);
  await publicServer(page);
  await openMockedGame(page, { nicknames, spectator });
  await page.getByTestId("feedback-button").click();
  await expect(page.getByTestId("feedback-sheet")).toBeVisible();
}

for (const size of ["desktop", "phone"] as Size[]) {
  test(`named player ${size}`, async ({ page }, testInfo) => {
    const received = await mockFeedback(page);
    await openSheet(page, size, false);
    await expect(page.getByTestId("feedback-who")).toContainText(nicknames[galleryPlayers[0].id]);
    await expect(page.getByTestId("feedback-name")).toHaveCount(0);
    await page.getByTestId("feedback-text").fill("np gave me a note but I never saw the card in my hand.");
    await shot(page, testInfo, `named-${size}`);
    await page.getByTestId("feedback-submit").click();
    await expect(page.getByTestId("feedback-sent")).toBeVisible();
    const context = received[0].context as { sender: Record<string, unknown>; players: Array<Record<string, unknown>> };
    expect(context.sender.nickname).toBe(nicknames[galleryPlayers[0].id]);
    expect(context.players.map((p) => p.nickname)).toContain("np");
    expect(context.players.filter((p) => p.is_sender)).toHaveLength(1);
    expect(JSON.stringify(received[0])).not.toContain("screenshot-session");
  });

  test(`no nickname: optional name field ${size}`, async ({ page }, testInfo) => {
    const received = await mockFeedback(page);
    await openSheet(page, size, true);
    await expect(page.getByTestId("feedback-who")).toHaveCount(0);
    const field = page.getByTestId("feedback-name");
    await expect(field).toBeVisible();
    expect((await field.boundingBox())!.height).toBeGreaterThanOrEqual(44);
    expect(await field.evaluate((el) => parseFloat(getComputedStyle(el).fontSize))).toBeGreaterThanOrEqual(16);
    const { width, height } = page.viewportSize()!;
    const box = (await page.getByTestId("feedback-sheet").boundingBox())!;
    expect(box.x + box.width).toBeLessThanOrEqual(width + 1);
    expect(box.y + box.height).toBeLessThanOrEqual(height + 1);
    await field.fill("Sam");
    await page.getByTestId("feedback-text").fill("The log is hard to read while spectating.");
    await shot(page, testInfo, `typed-name-${size}`);
    await page.getByTestId("feedback-submit").click();
    await expect(page.getByTestId("feedback-sent")).toBeVisible();
    expect((received[0].context as { sender: unknown }).sender).toEqual({ typed_name: "Sam" });
  });
}
