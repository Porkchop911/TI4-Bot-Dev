import type { Page } from "@playwright/test";
import { openScene } from "../BD-mobile-top-chrome/scene";
import { openMockedLobby } from "../_shared/mockLobby";

export const SIZES = {
  desktop: { width: 1440, height: 900 },
  phone: { width: 390, height: 844 },
} as const;
export type SizeName = keyof typeof SIZES;

/** What `GET /health` answers on the public playtest server. */
export async function publicHealth(page: Page) {
  await page.route("**/health", (route) =>
    route.fulfill({ json: { status: "ok", protocol_version: 4, commit: "51390594", public: true } }),
  );
}

/** A server whose password gate is open for this visitor (the probe answers "granted"). */
async function accessGranted(page: Page) {
  await page.route("**/api/access", (route) => route.fulfill({ json: { required: true, granted: true } }));
}

export async function openPasswordScreen(page: Page, size: SizeName) {
  await page.setViewportSize(SIZES[size]);
  await publicHealth(page);
  await page.route("**/api/access", (route) =>
    route.request().method() === "GET"
      ? route.fulfill({ json: { required: true, granted: false } })
      : route.fulfill({ status: 401, json: { error: "invalid_password", message: "wrong password" } }),
  );
  await page.goto("/");
}

export async function openPublicLobby(page: Page, size: SizeName) {
  await page.setViewportSize(SIZES[size]);
  await publicHealth(page);
  await accessGranted(page);
  await openMockedLobby(page);
}

export async function openPublicGame(page: Page, size: SizeName) {
  await publicHealth(page);
  await accessGranted(page);
  const bd = size === "phone" ? "390x844" : "1440x900";
  await openScene(page, bd, "your-turn");
}

/** The mocked answer of POST /api/feedback; resolves with the body the page sent. */
export async function mockFeedbackEndpoint(page: Page) {
  const received: Array<Record<string, unknown>> = [];
  await page.route("**/api/feedback", async (route) => {
    received.push(route.request().postDataJSON() as Record<string, unknown>);
    await route.fulfill({ json: { id: "fb_1760268000_9f3a1c2e" } });
  });
  return received;
}
