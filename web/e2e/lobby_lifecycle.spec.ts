import { expect, Page, test } from "@playwright/test";

function failOnBrowserErrors(page: Page): void {
  page.on("pageerror", (error) => {
    throw error;
  });
  page.on("console", (message) => {
    if (message.type() === "error" && !message.text().includes("favicon"))
      throw new Error(message.text());
  });
}

test("creates, joins, leaves, rejoins, starts, and restores using the real server", async ({
  browser,
}) => {
  const hostContext = await browser.newContext();
  await hostContext.grantPermissions(["clipboard-read", "clipboard-write"]);
  const host = await hostContext.newPage();
  failOnBrowserErrors(host);
  await host.goto("/");
  await host.getByLabel("Players").selectOption("2");
  await host.getByLabel("Nickname").fill("Player 1");
  await host.getByTestId("create-game-button").click();
  await expect(host.getByText("Game lobby")).toBeVisible();
  const hostUrl = host.url();
  expect(hostUrl).not.toContain("#");
  await expect(host.getByTestId("start-game-button")).toBeDisabled();
  await host.getByTestId("ready-button").click();
  await host.getByRole("button", { name: "Copy game URL" }).click();
  const gameUrl = await host.evaluate(() => navigator.clipboard.readText());
  const gameId = new URL(hostUrl).pathname.split("/").pop()!;
  expect(gameUrl).toBe(hostUrl);
  await expect(host.getByText(/Position 1: Player 1 \(Host\)/)).toBeVisible();
  await expect(host.locator(".lobby-panel")).not.toContainText(gameId);
  await expect(host.locator(".lobby-panel")).not.toContainText("player_");
  const guestContext = await browser.newContext();
  const guest = await guestContext.newPage();
  failOnBrowserErrors(guest);
  await guest.goto(gameUrl);
  expect(guest.url()).toBe(gameUrl);
  await expect(guest.getByText("Join or watch")).toBeVisible();
  await guest.getByRole("button", { name: "Watch" }).click();
  await expect(guest.getByText("Watching as spectator")).toBeVisible();
  await guest.getByLabel("Nickname").fill("Player 2");
  await guest.getByRole("button", { name: "Join game" }).click();
  await expect(guest.getByTestId("ready-button")).toBeVisible();
  await expect(guest.getByText(/Position 2: Player 2/)).toBeVisible();
  await guest.getByRole("button", { name: "Leave lobby" }).click();
  await expect(guest).toHaveURL(new URL("/", gameUrl).href);
  await expect(host.getByText(/Position 2: Open/)).toBeVisible();
  await guest.goto(gameUrl);
  await guest.getByRole("button", { name: "Join game" }).click();
  await expect(guest.getByTestId("ready-button")).toBeVisible();
  await guest.getByTestId("ready-button").click();
  await expect(host.getByTestId("start-game-button")).toBeEnabled();
  await host.getByTestId("start-game-button").click();
  await expect(host.getByTestId("turn-status-bar")).toBeVisible();
  await host.reload();
  await expect(host.getByTestId("turn-status-bar")).toBeVisible();
  await guest.reload();
  await expect(guest.getByTestId("turn-status-bar")).toBeVisible();
  await hostContext.close();
  await guestContext.close();
});

test("reorders open positions, watches a running game, and revokes a disconnected player on takeover", async ({
  browser,
  request,
}) => {
  const hostContext = await browser.newContext();
  const host = await hostContext.newPage();
  failOnBrowserErrors(host);
  await host.goto("/");
  await host.getByLabel("Players").selectOption("2");
  await host.getByLabel("Nickname").fill("Player 1");
  await host.getByTestId("create-game-button").click();
  await expect(host.getByText("Game lobby")).toBeVisible();
  const gameUrl = host.url();
  const gameId = new URL(gameUrl).pathname.split("/").pop()!;
  const hostSession = await host.evaluate(
    (id) => sessionStorage.getItem(`ti4.player-session:${id}`),
    gameId,
  );
  expect(hostSession).toBeTruthy();
  await host.getByRole("button", { name: "Move position 1 down" }).click();
  await expect(host.getByText(/Position 2: Player 1 \(Host\)/)).toBeVisible();
  const guestContext = await browser.newContext();
  const guest = await guestContext.newPage();
  failOnBrowserErrors(guest);
  await guest.goto(gameUrl);
  await guest.getByLabel("Nickname").fill("Player 2");
  await guest.getByRole("button", { name: "Join game" }).click();
  await expect(guest.getByText(/Position 1: Player 2/)).toBeVisible();
  await expect(guest.getByTestId("ready-button")).toBeVisible();
  const guestSession = await guest.evaluate(
    (id) => sessionStorage.getItem(`ti4.player-session:${id}`),
    gameId,
  );
  expect(guestSession).toBeTruthy();
  await host.getByTestId("ready-button").click();
  await guest.getByTestId("ready-button").click();
  await host.getByTestId("start-game-button").click();
  await expect(host.getByTestId("turn-status-bar")).toBeVisible();
  const spectatorContext = await browser.newContext();
  const spectator = await spectatorContext.newPage();
  failOnBrowserErrors(spectator);
  await spectator.goto(gameUrl);
  await spectator.getByRole("button", { name: "Watch" }).click();
  await expect(spectator.getByTestId("ti4-board-svg")).toBeVisible();
  await expect(spectator.locator('[data-private-card="true"]')).toHaveCount(0);
  expect(await spectator.evaluate(() => Object.keys(sessionStorage))).toEqual([]);

  await guestContext.close();
  // The host stays present through browser heartbeats; the E2E server uses a short grace period.
  await expect
    .poll(
      async () => {
        const response = await request.get(
          `http://127.0.0.1:${process.env.TI4_E2E_BACKEND_PORT ?? "8080"}/api/games/${gameId}/lobby`,
        );
        const lobby = await response.json();
        expect(lobby.slots[1].connected).toBe(true);
        return lobby.slots[0].can_take_over;
      },
      { timeout: 10_000, intervals: [250] },
    )
    .toBe(true);
  const takeoverContext = await browser.newContext();
  const takeover = await takeoverContext.newPage();
  failOnBrowserErrors(takeover);
  await takeover.goto(gameUrl);
  await takeover.getByLabel("Nickname").fill("Player 2");
  await expect(takeover.getByRole("button", { name: /Rejoin as Player 2/ })).toBeVisible();
  await takeover.getByRole("button", { name: /Rejoin as Player 2/ }).click();
  await expect(takeover.getByTestId("turn-status-bar")).toBeVisible();
  const replaced = await takeover.evaluate(
    (id) => sessionStorage.getItem(`ti4.player-session:${id}`),
    gameId,
  );
  expect(replaced).toBeTruthy();
  expect(replaced).not.toBe(guestSession);
  const base = `http://127.0.0.1:${process.env.TI4_E2E_BACKEND_PORT ?? "8080"}/api/games/${gameId}`;
  expect(
    (
      await request.get(`${base}/snapshot`, { headers: { "x-ti4-player-session": guestSession! } })
    ).status(),
  ).toBe(403);
  const resumed = await request.post(`${base}/lobby/join`, {
    headers: { "x-ti4-player-session": replaced! },
    data: { kind: "new" },
  });
  expect(resumed.ok()).toBeTruthy();
  expect((await resumed.json()).player.id).toBe(
    (await request.get(`${base}/lobby`).then((r) => r.json())).slots[0].occupant,
  );
  await takeoverContext.close();
  await spectatorContext.close();
  await hostContext.close();
});
